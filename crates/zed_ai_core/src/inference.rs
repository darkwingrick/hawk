use std::{pin::Pin, sync::Arc};

use anyhow::Result;
use futures::{Future, Stream, StreamExt};
use http_client::HttpClient;
use ollama::{ChatMessage, ChatOptions, ChatRequest as OllamaChatRequest, KeepAlive};

use crate::{ChatRequest, ModelPreset, TokenChunk};

pub trait InferenceEngine: Send + Sync {
    fn warm_up<'a>(
        &'a self,
        preset: &'a ModelPreset,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    fn stream_chat<'a>(
        &'a self,
        preset: &'a ModelPreset,
        request: ChatRequest,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<Pin<Box<dyn Stream<Item = Result<TokenChunk>> + Send>>>,
                > + Send
                + 'a,
        >,
    >;
}

pub struct OllamaInferenceEngine {
    client: Arc<dyn HttpClient>,
    api_url: String,
}

impl OllamaInferenceEngine {
    pub fn new(client: Arc<dyn HttpClient>, api_url: impl Into<String>) -> Self {
        Self {
            client,
            api_url: api_url.into(),
        }
    }
}

impl InferenceEngine for OllamaInferenceEngine {
    fn warm_up<'a>(
        &'a self,
        _preset: &'a ModelPreset,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        // Ollama warms up on first use; nothing to do here.
        Box::pin(async move { Ok(()) })
    }

    fn stream_chat<'a>(
        &'a self,
        preset: &'a ModelPreset,
        request: ChatRequest,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<Pin<Box<dyn Stream<Item = Result<TokenChunk>> + Send>>>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let ollama_request = OllamaChatRequest {
                model: preset.id.as_ref().to_string(),
                messages: vec![ChatMessage::User {
                    content: request.question,
                    images: None,
                }],
                stream: true,
                keep_alive: KeepAlive::indefinite(),
                options: Some(ChatOptions {
                    num_ctx: Some(preset.context_window as u64),
                    ..Default::default()
                }),
                tools: Vec::new(),
                think: None,
            };
            let delta_stream = ollama::stream_chat_completion(
                self.client.as_ref(),
                &self.api_url,
                None,
                ollama_request,
            )
            .await?;

            let token_stream: Pin<Box<dyn Stream<Item = Result<TokenChunk>> + Send>> =
                Box::pin(delta_stream.map(|delta| {
                    let delta = delta?;
                    let text = match delta.message {
                        ChatMessage::Assistant { content, .. } => content,
                        _ => String::new(),
                    };
                    Ok(TokenChunk { text })
                }));
            Ok(token_stream)
        })
    }
}
