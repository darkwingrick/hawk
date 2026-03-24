use std::{path::PathBuf, sync::Arc, time::SystemTime};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelFamily {
    QwenCoder,
    Mistral,
    Llama,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPreset {
    pub id: Arc<str>,
    pub family: ModelFamily,
    pub quantization: Arc<str>,
    pub context_window: usize,
    pub recommended_for_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenChunk {
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub question: String,
    pub project_root: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CodeChunk {
    pub path: PathBuf,
    pub language: Option<String>,
    pub symbol_path: Option<String>,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteRecord {
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryHit {
    pub source: Arc<str>,
    pub excerpt: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodebaseIndexStatus {
    NotIndexed,
    Indexing,
    Indexed,
    Error(String),
}

#[derive(Clone, Debug)]
pub struct IndexStats {
    pub total_chunks: usize,
    pub last_updated: Option<SystemTime>,
}

#[derive(Clone, Debug)]
pub struct IndexDiagnostics {
    pub embedding_model: String,
    pub index_size_bytes: u64,
    pub last_indexed: Option<SystemTime>,
}
