use std::{pin::Pin, sync::Arc};

use anyhow::Result;
use futures::Future;
use http_client::HttpClient;

pub trait EmbeddingEngine: Send + Sync {
    fn embed<'a>(
        &'a self,
        inputs: &'a [String],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Vec<f32>>>> + Send + 'a>>;
}

pub struct OllamaEmbeddingEngine {
    client: Arc<dyn HttpClient>,
    api_url: String,
    model: String,
}

impl OllamaEmbeddingEngine {
    pub fn new(client: Arc<dyn HttpClient>, api_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client,
            api_url: api_url.into(),
            model: model.into(),
        }
    }
}

impl EmbeddingEngine for OllamaEmbeddingEngine {
    fn embed<'a>(
        &'a self,
        inputs: &'a [String],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Vec<f32>>>> + Send + 'a>> {
        Box::pin(async move {
            let response = ollama::embed(
                self.client.as_ref(),
                &self.api_url,
                None,
                ollama::EmbedRequest {
                    model: self.model.clone(),
                    input: inputs.to_vec(),
                },
            )
            .await?;
            Ok(response.embeddings)
        })
    }
}
