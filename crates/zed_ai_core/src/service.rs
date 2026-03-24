use std::{path::Path, pin::Pin, sync::Arc};

use anyhow::{Result, anyhow};
use futures::Stream;
use http_client::HttpClient;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};

use crate::{
    CodeChunk, EmbeddingEngine, HeedIndexStore, HeedNotesStore, IndexDiagnostics, IndexStore,
    InferenceEngine, LocalExpertConfig, ModelManager, ModelPreset, NoteRecord, NotesStore,
    OllamaEmbeddingEngine, OllamaInferenceEngine, PullProgress, QueryHit, SemanticChunker,
    TokenChunk,
};

pub struct LocalExpertService {
    pub config: LocalExpertConfig,
    active_model_preset: Mutex<Option<ModelPreset>>,
    embedding_engine: Option<Arc<dyn EmbeddingEngine>>,
    inference_engine: Option<Arc<dyn InferenceEngine>>,
    index_store: Option<Arc<dyn IndexStore>>,
    notes_store: Option<Arc<dyn NotesStore>>,
    model_manager: ModelManager,
    chunker: SemanticChunker,
}

impl Default for LocalExpertService {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalExpertService {
    pub fn new() -> Self {
        Self {
            config: LocalExpertConfig::default(),
            active_model_preset: Mutex::new(None),
            embedding_engine: None,
            inference_engine: None,
            index_store: None,
            notes_store: None,
            model_manager: ModelManager::new(),
            chunker: SemanticChunker,
        }
    }

    pub fn with_engines(
        embedding_engine: Arc<dyn EmbeddingEngine>,
        inference_engine: Arc<dyn InferenceEngine>,
        index_store: Arc<dyn IndexStore>,
        notes_store: Arc<dyn NotesStore>,
        config: LocalExpertConfig,
    ) -> Self {
        Self {
            config,
            active_model_preset: Mutex::new(None),
            embedding_engine: Some(embedding_engine),
            inference_engine: Some(inference_engine),
            index_store: Some(index_store),
            notes_store: Some(notes_store),
            model_manager: ModelManager::new(),
            chunker: SemanticChunker,
        }
    }

    /// Convenience constructor: creates a fully Ollama-backed service for a project.
    ///
    /// Stores are opened (or created) under `project_root/.zed-ai/`.
    pub fn default_with_ollama(
        project_root: &Path,
        http_client: Arc<dyn HttpClient>,
        api_url: impl Into<String>,
        embedding_model: impl Into<String>,
        mut config: LocalExpertConfig,
    ) -> Result<Arc<Self>> {
        let api_url = api_url.into();
        let embedding_model = embedding_model.into();
        let zed_ai_dir = project_root.join(".zed-ai");
        let index_store = HeedIndexStore::open(&zed_ai_dir.join("index"))?;
        let notes_store = HeedNotesStore::open(&zed_ai_dir.join("notes"))?;
        let embedding_engine: Arc<dyn EmbeddingEngine> = Arc::new(OllamaEmbeddingEngine::new(
            http_client.clone(),
            &api_url,
            embedding_model.clone(),
        ));
        let inference_engine: Arc<dyn InferenceEngine> =
            Arc::new(OllamaInferenceEngine::new(http_client, &api_url));
        config.embedding_model_preset = Some(embedding_model);
        Ok(Arc::new(Self::with_engines(
            embedding_engine,
            inference_engine,
            index_store,
            notes_store,
            config,
        )))
    }

    pub fn default_model_presets() -> Vec<ModelPreset> {
        ModelManager::default_presets()
    }

    pub fn list_model_presets(&self) -> Vec<ModelPreset> {
        Self::default_model_presets()
    }

    pub fn download_state(&self, preset_id: &str) -> crate::ModelDownloadState {
        self.model_manager.download_state(preset_id)
    }

    pub async fn pull_model(
        &self,
        client: Arc<dyn HttpClient>,
        api_url: &str,
        preset_id: &str,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<PullProgress>> + Send>>> {
        self.model_manager
            .pull_model(client, api_url, preset_id)
            .await
    }

    pub fn set_active_model_preset(&self, preset: ModelPreset) -> Result<()> {
        if preset.id.is_empty() {
            return Err(anyhow!("model preset id cannot be empty"));
        }
        *self.active_model_preset.lock() = Some(preset);
        Ok(())
    }

    pub fn active_model_preset(&self) -> Option<ModelPreset> {
        self.active_model_preset.lock().clone()
    }

    // ------------------------------------------------------------------
    // Indexing
    // ------------------------------------------------------------------

    /// Walk `project_root`, chunk all eligible files, embed them, and persist
    /// to the index store. Skips files larger than `config.max_file_bytes`.
    pub async fn index_project(&self, project_root: &Path) -> Result<()> {
        let embedding_engine = self
            .embedding_engine
            .as_ref()
            .ok_or_else(|| anyhow!("no embedding engine configured"))?
            .clone();
        let index_store = self
            .index_store
            .as_ref()
            .ok_or_else(|| anyhow!("no index store configured"))?
            .clone();

        let files = collect_eligible_files(project_root, &self.config)?;
        let batch_size = 8;

        for batch in files.chunks(batch_size) {
            let mut pairs: Vec<(CodeChunk, Vec<f32>)> = Vec::new();
            for file_path in batch {
                let source = match std::fs::read_to_string(file_path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let chunks = self.chunker.chunk_file(file_path, &source);
                let contents: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
                if contents.is_empty() {
                    continue;
                }
                let embeddings = embedding_engine.embed(&contents).await?;
                for (chunk, embedding) in chunks.into_iter().zip(embeddings.into_iter()) {
                    pairs.push((chunk, embedding));
                }
            }
            if !pairs.is_empty() {
                index_store.upsert_chunks(&pairs)?;
            }
        }
        Ok(())
    }

    /// Re-index a single file that has changed on disk.
    /// Removes the old chunks for that file and re-embeds the new content.
    pub async fn reindex_file(&self, path: &Path) -> Result<()> {
        let embedding_engine = self
            .embedding_engine
            .as_ref()
            .ok_or_else(|| anyhow!("no embedding engine configured"))?
            .clone();
        let index_store = self
            .index_store
            .as_ref()
            .ok_or_else(|| anyhow!("no index store configured"))?
            .clone();

        // Skip files that exceed the size limit.
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > self.config.max_file_bytes {
                return Ok(());
            }
        }

        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return Ok(()), // Binary or unreadable file — skip silently.
        };
        let chunks = self.chunker.chunk_file(path, &source);
        if chunks.is_empty() {
            return Ok(());
        }

        // Get existing hashes for the file to skip unchanged chunks.
        let existing_hashes = index_store.get_file_hashes(path)?;
        let mut chunks_to_embed = Vec::new();
        let mut embeddings_needed = Vec::new();

        for chunk in chunks {
            let chunk_hash = Sha256::digest(chunk.content.as_bytes()).to_vec();
            if !existing_hashes.contains(&chunk_hash) {
                embeddings_needed.push(chunk.content.clone());
                chunks_to_embed.push((chunk, chunk_hash));
            }
        }

        if embeddings_needed.is_empty() {
            return Ok(());
        }

        // Remove the file to replace with updated chunks.
        index_store.remove_file(path)?;

        let embeddings = embedding_engine.embed(&embeddings_needed).await?;
        let pairs: Vec<(CodeChunk, Vec<f32>)> = chunks_to_embed
            .into_iter()
            .map(|(chunk, _)| chunk)
            .zip(embeddings)
            .collect();
        index_store.upsert_chunks(&pairs)?;
        Ok(())
    }

    /// Drop all index data and re-index from scratch.
    pub async fn rebuild_index(&self, project_root: &Path) -> Result<()> {
        let index_store = self
            .index_store
            .as_ref()
            .ok_or_else(|| anyhow!("no index store configured"))?
            .clone();
        index_store.clear()?;
        self.index_project(project_root).await
    }

    // ------------------------------------------------------------------
    // Querying
    // ------------------------------------------------------------------

    /// Embed the question, retrieve top-k chunks, assemble a prompt, and
    /// stream the model's response.
    pub async fn query(
        &self,
        question: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<TokenChunk>> + Send>>> {
        let embedding_engine = self
            .embedding_engine
            .as_ref()
            .ok_or_else(|| anyhow!("no embedding engine configured"))?
            .clone();
        let index_store = self
            .index_store
            .as_ref()
            .ok_or_else(|| anyhow!("no index store configured"))?
            .clone();
        let inference_engine = self
            .inference_engine
            .as_ref()
            .ok_or_else(|| anyhow!("no inference engine configured"))?
            .clone();
        let preset = self
            .active_model_preset()
            .or_else(|| self.list_model_presets().into_iter().next())
            .ok_or_else(|| anyhow!("no model preset available"))?;

        let query_embeddings = embedding_engine.embed(&[question.to_string()]).await?;
        let query_vector = query_embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("embedding engine returned empty result"))?;

        let hits = index_store.search_by_vector(&query_vector, 8)?;
        let context = assemble_context(&hits);

        let augmented_question = format!(
            "You are an expert on this codebase. Use the following code excerpts to answer the question.\n\
             If the answer is not in the excerpts, say so.\n\n\
             --- CODE CONTEXT ---\n{context}\n--- END CONTEXT ---\n\n\
             Question: {question}"
        );

        let chat_request = crate::ChatRequest {
            question: augmented_question,
            project_root: None,
        };
        inference_engine.stream_chat(&preset, chat_request).await
    }

    // ------------------------------------------------------------------
    // Notes
    // ------------------------------------------------------------------

    pub async fn save_note(&self, note: NoteRecord) -> Result<()> {
        let embedding_engine = self
            .embedding_engine
            .as_ref()
            .ok_or_else(|| anyhow!("no embedding engine configured"))?
            .clone();
        let notes_store = self
            .notes_store
            .as_ref()
            .ok_or_else(|| anyhow!("no notes store configured"))?
            .clone();

        let body = note.body.clone();
        let embeddings = embedding_engine.embed(&[body]).await?;
        let embedding = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("embedding engine returned empty result"))?;
        notes_store.save_note(note, embedding)?;
        Ok(())
    }

    /// Get diagnostics information about the index.
    pub fn diagnostics(&self) -> Result<IndexDiagnostics> {
        let index_store = self
            .index_store
            .as_ref()
            .ok_or_else(|| anyhow!("no index store configured"))?
            .clone();

        let stats = index_store.stats()?;
        Ok(IndexDiagnostics {
            embedding_model: self
                .config
                .embedding_model_preset
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            index_size_bytes: (stats.total_chunks as u64) * 1000, // Rough estimate
            last_indexed: stats.last_updated,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn collect_eligible_files(
    root: &Path,
    config: &LocalExpertConfig,
) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    collect_recursive(root, root, config, &mut files)?;
    Ok(files)
}

fn collect_recursive(
    _root: &Path,
    dir: &Path,
    config: &LocalExpertConfig,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        if is_always_excluded(name) {
            continue;
        }

        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if meta.is_dir() {
            collect_recursive(_root, &path, config, out)?;
        } else if meta.is_file() && meta.len() <= config.max_file_bytes {
            out.push(path);
        }
    }
    Ok(())
}

fn is_always_excluded(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | ".zed-ai" | "dist" | "build" | ".DS_Store"
    ) || name.starts_with('.') && name != ".zed"
}

fn assemble_context(hits: &[QueryHit]) -> String {
    hits.iter()
        .enumerate()
        .map(|(i, hit)| {
            format!(
                "[{}] {} ({})\n{}",
                i + 1,
                hit.source,
                hit.excerpt.lines().count(),
                hit.excerpt
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
