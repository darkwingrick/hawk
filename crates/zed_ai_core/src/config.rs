use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalExpertConfig {
    pub max_file_bytes: u64,
    pub exclude_globs: Vec<String>,
    pub notes_enabled: bool,
    pub embedding_model_preset: Option<String>,
}

impl Default for LocalExpertConfig {
    fn default() -> Self {
        Self {
            max_file_bytes: 256 * 1024,
            exclude_globs: Vec::new(),
            notes_enabled: true,
            embedding_model_preset: None,
        }
    }
}
