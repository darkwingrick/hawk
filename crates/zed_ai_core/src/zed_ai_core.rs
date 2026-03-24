mod chunking;
mod config;
mod embedding;
mod index_store;
mod inference;
mod model_manager;
mod notes;
mod retrieval;
mod service;
mod types;
mod watcher;

pub use chunking::*;
pub use config::*;
pub use embedding::*;
pub use index_store::*;
pub use inference::*;
pub use model_manager::{ModelDownloadState, ModelManager, PullProgress};
pub use notes::*;
pub use service::*;
pub use types::*;
pub use watcher::*;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{LocalExpertService, ModelFamily, ModelPreset};

    #[test]
    fn default_model_presets_include_qwen_14b() {
        let presets = LocalExpertService::default_model_presets();
        assert!(presets.iter().any(|preset| {
            preset.id.as_ref() == "qwen2.5-coder-14b-instruct"
                && preset.family == ModelFamily::QwenCoder
                && preset.recommended_for_default
        }));
    }

    #[test]
    fn active_model_preset_can_be_changed() {
        let service = LocalExpertService::new();
        let target = ModelPreset {
            id: Arc::<str>::from("custom-qwen"),
            family: ModelFamily::QwenCoder,
            quantization: Arc::<str>::from("q4_k_m"),
            context_window: 65536,
            recommended_for_default: false,
        };

        service
            .set_active_model_preset(target.clone())
            .expect("setting active preset should succeed");

        assert_eq!(
            service.active_model_preset().expect("active preset should be set"),
            target
        );
    }
}
