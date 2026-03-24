# Local Codebase Expert — Implementation Progress

## Completed

### Milestone 1 — Core crate scaffold
- [x] Created `crates/zed_ai_core/` with all modules: `config`, `types`, `chunking`, `embedding`, `index_store`, `inference`, `model_manager`, `notes`, `retrieval`, `service`, `watcher`
- [x] `chunking.rs` — `SemanticChunker` using tree-sitter-rust for `.rs` files; sliding-window fallback for all others
- [x] `embedding.rs` — `EmbeddingEngine` trait + `OllamaEmbeddingEngine` hitting `/api/embed`
- [x] `inference.rs` — `InferenceEngine` trait + `OllamaInferenceEngine` streaming `/api/chat`
- [x] `index_store.rs` — `IndexStore` trait + `HeedIndexStore` (LMDB via heed) with cosine-similarity search
- [x] `notes.rs` — `NotesStore` trait + `HeedNotesStore`
- [x] `types.rs`, `config.rs`, `retrieval.rs` — shared types and `LocalExpertConfig`
- [x] Unit tests passing for chunking, index store math, and model preset selection

### Milestone 2 — Incremental indexing
- [x] `service.rs` — `LocalExpertService::index_project()` (batch embed + upsert)
- [x] `service.rs` — `LocalExpertService::reindex_file()` (remove old chunks, re-embed single file)
- [x] `service.rs` — `LocalExpertService::rebuild_index()` (clear + full re-index)
- [x] `watcher.rs` — `IndexWatcher` GPUI entity: debounces file-change paths (2 s), emits `ReindexStarted / ReindexCompleted / ReindexError` events
- [x] `service.rs` — `LocalExpertService::query()` — embed question → cosine search → RAG prompt → stream answer

### Milestone 3 — Model management
- [x] `ollama.rs` — added `embed()` and `pull_model()` (streaming) to the ollama crate
- [x] `model_manager.rs` — `ModelManager` with `default_presets()` (Qwen2.5-Coder 14B + 7B), `list_installed_presets()`, `pull_model()` streaming with `ModelDownloadState` tracking
- [x] `service.rs` — `default_with_ollama()` factory: opens heed stores under `.zed-ai/`, wires Ollama embedding + inference engines

### Milestone 4 — Assistant-shell integration
- [x] `agent_ui.rs` — `AskCodebase` action declared
- [x] `agent_panel.rs` — `AskCodebase` handler registered, `ask_codebase()` method seeds a text thread with the RAG prompt
- [x] `agent_panel.rs` — `ASK_CODEBASE_PROMPT` constant ("Answer using only the current codebase…")
- [x] `project_settings.rs` — `LocalCodebaseExpertSettings` struct with `enabled`, `auto_index_on_open`, `chat_model_preset`, `embedding_model_preset`, `max_file_bytes`, `notes_enabled`
- [x] `agent_ui/Cargo.toml` — `zed_ai_core` dependency wired
- [x] Initialize `LocalExpertService` in `AgentPanel::new()` — Added fields, Ollama integration, settings respect, background indexing
- [x] Subscribe `IndexWatcher` to `WorktreeStoreEvent::WorktreeUpdatedEntries` — Automatic reindexing on file changes
- [x] Project status pill — Index state display in agent panel header
- [x] `SaveNote` action — Save current thread as note in codebase
- [x] Source citations — File paths included in RAG context for AI responses

---

## Remaining Work

### Milestone 5 — Rules & Skills UI

- [x] `prompt_store.rs` — add `PromptKind::Skill` variant (already present)
- [x] `rules_library.rs` — add import/export actions for prompt assets (`export_prompt_asset`, `import_prompt_asset`) (export_rule implemented, import_rule added with file picker)
- [x] Wire `LocalExpertService::export_prompt_asset()` and `import_prompt_asset()` stubs to real `prompt_store` logic (moved export/import functionality to rules_library for direct PromptStore access)
- [x] UI: create/edit/delete local Skills; file picker for import; copy-to-clipboard for export (handled by existing actions: NewSkill, DeleteRule, etc.)

### Milestone 6 — Performance hardening & UX polish

- [ ] Background indexing throttle: pause/resume on user activity
- [x] Hash-based incremental skip: skip re-embedding chunks whose `content_hash` hasn't changed (implemented in reindex_file with get_file_hashes)
- [ ] Corrupted-index rebuild: detect heed open errors and offer a "Rebuild index" prompt
- [ ] `ModelManager` UI: model download progress bar, active-preset switcher in panel header
- [ ] Diagnostics panel: show embedding model, index size, last-indexed timestamp
- [ ] Graceful degradation for remote/collab workspaces (disable indexing, show explanation)

---

## Architecture Notes

| Layer | Implementation | Location |
|---|---|---|
| Chunking | tree-sitter (Rust) + text windows | `zed_ai_core/chunking.rs` |
| Embedding | Ollama `/api/embed` | `zed_ai_core/embedding.rs` |
| Vector store | LMDB via `heed` (cosine similarity) | `zed_ai_core/index_store.rs` |
| Notes store | LMDB via `heed` | `zed_ai_core/notes.rs` |
| Inference | Ollama `/api/chat` streaming | `zed_ai_core/inference.rs` |
| Model mgmt | Ollama `/api/tags` + `/api/pull` | `zed_ai_core/model_manager.rs` |
| File watcher | GPUI entity + debounce | `zed_ai_core/watcher.rs` |
| Orchestrator | `LocalExpertService` | `zed_ai_core/service.rs` |
| UI entry point | `AskCodebase` action | `agent_ui/agent_panel.rs` |
| Settings | `LocalCodebaseExpertSettings` | `project/project_settings.rs` |

Default preset: `qwen2.5-coder-14b-instruct` (q4_k_m, 131K context)
Stores location: `<project_root>/.zed-ai/index/` and `.zed-ai/notes/`
Ollama default URL: `http://localhost:11434`
