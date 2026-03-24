use std::{pin::Pin, sync::Arc};

use anyhow::Result;
use futures::{Future, Stream, StreamExt};
use http_client::HttpClient;
use parking_lot::Mutex;

use crate::{ModelFamily, ModelPreset};

// ---------------------------------------------------------------------------
// Download-state tracking
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum ModelDownloadState {
    NotInstalled,
    Downloading { completed: u64, total: Option<u64> },
    Installed,
    Error(Arc<str>),
}

// ---------------------------------------------------------------------------
// Pull-progress events (re-exported for callers)
// ---------------------------------------------------------------------------

/// A single progress event from an in-flight model pull.
#[derive(Clone, Debug)]
pub struct PullProgress {
    pub status: Arc<str>,
    pub completed: Option<u64>,
    pub total: Option<u64>,
}

// ---------------------------------------------------------------------------
// ModelManager
// ---------------------------------------------------------------------------

pub struct ModelManager {
    download_states: Arc<Mutex<std::collections::HashMap<Arc<str>, ModelDownloadState>>>,
}

impl Default for ModelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelManager {
    pub fn new() -> Self {
        Self {
            download_states: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    // ------------------------------------------------------------------
    // Preset registry
    // ------------------------------------------------------------------

    pub fn default_presets() -> Vec<ModelPreset> {
        vec![
            ModelPreset {
                id: "qwen2.5-coder-14b-instruct".into(),
                family: ModelFamily::QwenCoder,
                quantization: "q4_k_m".into(),
                context_window: 131_072,
                recommended_for_default: true,
            },
            ModelPreset {
                id: "qwen2.5-coder-7b-instruct".into(),
                family: ModelFamily::QwenCoder,
                quantization: "q4_k_m".into(),
                context_window: 131_072,
                recommended_for_default: false,
            },
        ]
    }

    // ------------------------------------------------------------------
    // Installed-model discovery (via Ollama API)
    // ------------------------------------------------------------------

    /// Return which of the default presets are currently installed in Ollama.
    pub fn list_installed_presets<'a>(
        &'a self,
        client: Arc<dyn HttpClient>,
        api_url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ModelPreset>>> + Send + 'a>> {
        Box::pin(async move {
            let listings = ollama::get_models(client.as_ref(), api_url, None).await?;
            let installed_names: std::collections::HashSet<String> =
                listings.into_iter().map(|l| l.name).collect();

            Ok(Self::default_presets()
                .into_iter()
                .filter(|p| installed_names.contains(p.id.as_ref()))
                .collect())
        })
    }

    // ------------------------------------------------------------------
    // Pull / download
    // ------------------------------------------------------------------

    /// Pull `preset_id` from Ollama's registry.
    /// Returns a stream of [`PullProgress`] events so the UI can show progress.
    pub fn pull_model<'a>(
        &'a self,
        client: Arc<dyn HttpClient>,
        api_url: &'a str,
        preset_id: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Pin<Box<dyn Stream<Item = Result<PullProgress>> + Send>>>> + Send + 'a>>
    {
        let id: Arc<str> = Arc::from(preset_id);
        Box::pin(async move {
            {
                let mut states = self.download_states.lock();
                states.insert(
                    id.clone(),
                    ModelDownloadState::Downloading {
                        completed: 0,
                        total: None,
                    },
                );
            }

            let event_stream =
                ollama::pull_model(client.as_ref(), api_url, None, preset_id).await?;

            let states = self.download_states.clone();
            let preset_id_arc = id.clone();

            let progress_stream: Pin<Box<dyn Stream<Item = Result<PullProgress>> + Send>> =
                Box::pin(event_stream.map(move |event| {
                    let event = event?;
                    let progress = PullProgress {
                        status: Arc::from(event.status.as_str()),
                        completed: event.completed,
                        total: event.total,
                    };
                    let mut lock = states.lock();
                    if event.completed.is_some() || event.total.is_some() {
                        lock.insert(
                            preset_id_arc.clone(),
                            ModelDownloadState::Downloading {
                                completed: event.completed.unwrap_or(0),
                                total: event.total,
                            },
                        );
                    } else if event.status.contains("success") {
                        lock.insert(preset_id_arc.clone(), ModelDownloadState::Installed);
                    }
                    Ok(progress)
                }));

            Ok(progress_stream)
        })
    }

    // ------------------------------------------------------------------
    // State queries
    // ------------------------------------------------------------------

    pub fn download_state(&self, preset_id: &str) -> ModelDownloadState {
        self.download_states
            .lock()
            .get(preset_id)
            .cloned()
            .unwrap_or(ModelDownloadState::NotInstalled)
    }

    pub fn mark_installed(&self, preset_id: &str) {
        self.download_states
            .lock()
            .insert(Arc::from(preset_id), ModelDownloadState::Installed);
    }

    pub fn mark_error(&self, preset_id: &str, message: &str) {
        self.download_states.lock().insert(
            Arc::from(preset_id),
            ModelDownloadState::Error(Arc::from(message as &str)),
        );
    }
}
