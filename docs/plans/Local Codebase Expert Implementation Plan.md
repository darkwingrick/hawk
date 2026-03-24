
**Summary**
- Build a new non-UI crate, `crates/zed_ai_core`, and surface it through the existing assistant shell as a first-class “Ask Codebase” mode/profile instead of building a separate chat runtime.
- Keep everything local and offline after model download. Scope v1 to local worktrees only. Notes are project-local only. Skills are reusable local prompt/workflow assets, not executable autonomous agents.
- Make the chat model explicitly swappable. Optimize v1 around a **Qwen-family 14B-class coder model**, with `Qwen2.5-Coder-14B-Instruct` as the initial preset and later model changes handled via settings/model presets rather than re-architecture.

**Key Changes**
- Add `crates/zed_ai_core` with modules: `config`, `types`, `chunking`, `embedding`, `index_store`, `watcher`, `retrieval`, `notes`, `model_manager`, `inference`, `service`.
- `chunking` uses existing workspace `tree-sitter` grammars to produce semantic chunks for files, modules, types, impls, functions, methods, and fallback text blocks.
- `embedding` uses the repo’s existing patched Candle line behind an `EmbeddingEngine` trait. Embeddings remain independent from the chat model so generator upgrades do not require reindexing.
- `index_store` persists code and note embeddings in `.zed-ai/index/lancedb/`, with sidecar metadata for schema version, file hashes, chunk ids, and indexing state.
- `watcher` subscribes to `WorktreeStoreEvent::WorktreeUpdatedEntries` for incremental reindexing rather than adding an unrelated watcher stack.
- `inference` defines a backend-neutral `InferenceEngine` and a model registry/preset layer. The service accepts any supported local model preset, but v1 defaults and benchmarks target a 14B-class Qwen-family coder model.
- Reuse the current assistant shell in [agent_panel.rs](/Users/rray/Dev/hawk/crates/agent_ui/src/agent_panel.rs) and [agent_ui.rs](/Users/rray/Dev/hawk/crates/agent_ui/src/agent_ui.rs) by adding an “Ask Codebase” mode/profile, command-palette action, index status affordances, and “Save as Note”.
- Extend [project_settings.rs](/Users/rray/Dev/hawk/crates/project/src/project_settings.rs) with `local_codebase_expert` settings: `enabled`, `auto_index_on_open`, `chat_model_preset`, `embedding_model_preset`, `max_file_bytes`, `exclude_globs`, `notes_enabled`.
- Extend the existing rules/prompt system in [prompt_store.rs](/Users/rray/Dev/hawk/crates/prompt_store/src/prompt_store.rs) and [rules_library.rs](/Users/rray/Dev/hawk/crates/rules_library/src/rules_library.rs) with `PromptKind::{Rule, Skill}` and import/export metadata, keeping skills as reusable local prompt/workflow templates.

**Public Interfaces**
- Add internal traits and types:
```rust
pub trait EmbeddingEngine {
    async fn embed(&self, inputs: &[String]) -> anyhow::Result<Vec<Vec<f32>>>;
}

pub trait InferenceEngine {
    async fn warm_up(&self, preset: &ModelPreset) -> anyhow::Result<()>;
    async fn stream_chat(
        &self,
        preset: &ModelPreset,
        request: ChatRequest,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<TokenChunk>> + Send>>>;
}

pub struct ModelPreset {
    pub id: Arc<str>,
    pub family: ModelFamily,
    pub quantization: Arc<str>,
    pub context_window: usize,
    pub recommended_for_default: bool,
}

pub enum PromptKind {
    Rule,
    Skill,
}
```
- Keep retrieval/storage schema model-agnostic:
  - `code_chunks`: project/worktree/path/language/kind/content/hash/vector
  - `notes`: project/title/body/tags/timestamps/vector
- Add a single orchestrator surface for UI and commands:
  - `index_project`
  - `query`
  - `save_note`
  - `rebuild_index`
  - `list_model_presets`
  - `set_active_model_preset`
  - `export_prompt_asset`
  - `import_prompt_asset`

**Implementation Changes**
- Milestone 1: CLI spike using the exact architecture above, semantic chunking, LanceDB persistence, Candle embeddings, and streamed answers with the initial 14B Qwen preset.
- Milestone 2: project indexing service with hash-based incremental updates, cancellation, debouncing, corrupted-index rebuild, and explicit unsupported-state handling for remote/collab workspaces.
- Milestone 3: local model management with preset registry, one-time download, checksum validation, warm-up, active-preset switching, and strict repo-only prompt assembly.
- Milestone 4: assistant-shell integration with “Ask Codebase” mode, command-palette entry, source citations, project status pill, and save-note flow.
- Milestone 5: Rules & Skills UI updates, including local create/edit/delete plus import/export for prompt assets.
- Milestone 6: performance hardening and UX polish for large repos, including background throttling, pause/resume indexing, and clearer model/index diagnostics.

**Test Plan**
- Unit tests for semantic chunk boundaries, ignore/exclude behavior, hash-based change detection, prompt assembly, note scoping, model preset selection, and prompt asset import/export.
- Integration tests for project-open indexing, `WorktreeStoreEvent`-driven incremental updates, settings wiring, assistant mode activation, and disabled behavior on unsupported workspace types.
- End-to-end tests on a real Rust fixture repo:
  - initial index completes and persists
  - editing one file re-embeds only affected chunks
  - querying returns cited repo-local answers
  - saving a note makes it retrievable in later sessions
  - changing the active model preset does not require index rebuild
  - importing/exporting a skill preserves metadata and content
- Performance acceptance for the v1 default 14B preset: warm model streams usable first tokens without freezing the UI; indexing is cancelable; incremental updates stay bounded to changed files.

**Assumptions And Defaults**
- Initial shipped default is a **Qwen-family 14B-class coder preset**, with `Qwen2.5-Coder-14B-Instruct` as the initial implementation target unless a better same-class Qwen-family coder preset is pinned before coding starts.
- Later model changes are expected and supported through preset/config updates, not schema or UI rewrites.
- Embeddings are decoupled from the chat model; changing the generator later should not require reindexing unless embedding settings also change.
- `.zed-ai/` remains project-local as requested, even though the repo’s settings convention uses `.zed/`.
- V1 does not include remote workspaces, cross-project notes, multi-file edits, inline code transforms, LoRA training, or general-knowledge fallback.
- As of March 24, 2026, I could verify official current Qwen coder lines and Qwen-family model listings, but not a clearly published official `Qwen3.5-Coder-14B` target suitable to pin here; therefore the revised plan intentionally optimizes for a configurable 14B-class Qwen-family preset instead of hard-coding a speculative newer model.
