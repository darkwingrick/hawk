use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};

use gpui::{App, AppContext as _, Context, Entity, EventEmitter, Task, WeakEntity};

use crate::LocalExpertService;

const DEBOUNCE_DELAY: Duration = Duration::from_secs(2);

/// Events emitted by [`IndexWatcher`].
#[derive(Clone, Debug)]
pub enum IndexWatcherEvent {
    ReindexStarted { count: usize },
    ReindexCompleted,
    ReindexError(Arc<str>),
}

impl EventEmitter<IndexWatcherEvent> for IndexWatcher {}

/// GPUI entity that debounces file-change notifications and drives incremental
/// re-embedding via [`LocalExpertService`].
///
/// Callers subscribe to `WorktreeStoreEvent::WorktreeUpdatedEntries` and
/// forward changed absolute paths via [`IndexWatcher::notify_changed`].
pub struct IndexWatcher {
    service: Arc<LocalExpertService>,
    pending_paths: HashSet<PathBuf>,
    /// Dropping this cancels the pending debounce timer.
    _debounce_task: Option<Task<()>>,
    paused: bool,
}

impl IndexWatcher {
    pub fn new(service: Arc<LocalExpertService>, _cx: &mut App) -> Self {
        Self {
            service,
            pending_paths: HashSet::new(),
            _debounce_task: None,
            paused: false,
        }
    }

    /// Notify that `paths` changed. Resets the debounce timer.
    pub fn notify_changed(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.pending_paths.extend(paths);
        let weak = cx.weak_entity();
        self._debounce_task = Some(cx.spawn(async move |_, async_cx| {
            async_cx.background_executor().timer(DEBOUNCE_DELAY).await;
            weak.update(async_cx, |this, cx| this.flush_pending(cx))
                .ok();
        }));
    }

    /// Pause indexing. Pending changes will not be processed until resumed.
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resume indexing. Pending changes will be processed after the debounce delay.
    pub fn resume(&mut self, cx: &mut Context<Self>) {
        self.paused = false;
        if !self.pending_paths.is_empty() {
            let weak = cx.weak_entity();
            self._debounce_task = Some(cx.spawn(async move |_, async_cx| {
                async_cx.background_executor().timer(DEBOUNCE_DELAY).await;
                weak.update(async_cx, |this, cx| this.flush_pending(cx))
                    .ok();
            }));
        }
    }

    fn flush_pending(&mut self, cx: &mut Context<Self>) {
        if self.paused {
            return;
        }
        let paths: Vec<PathBuf> = std::mem::take(&mut self.pending_paths)
            .into_iter()
            .collect();
        if paths.is_empty() {
            return;
        }
        let count = paths.len();
        cx.emit(IndexWatcherEvent::ReindexStarted { count });

        let service = self.service.clone();
        cx.spawn(async move |this: WeakEntity<IndexWatcher>, async_cx| {
            let mut first_error: Option<String> = None;
            for path in paths {
                if let Err(err) = service.reindex_file(&path).await {
                    if first_error.is_none() {
                        first_error = Some(err.to_string());
                    }
                }
            }
            this.update(async_cx, |_, cx| {
                if let Some(msg) = first_error {
                    cx.emit(IndexWatcherEvent::ReindexError(Arc::from(msg.as_str())));
                } else {
                    cx.emit(IndexWatcherEvent::ReindexCompleted);
                }
            })
            .ok();
        })
        .detach();
    }
}

/// Convenience constructor: creates an `IndexWatcher` entity.
///
/// The caller is responsible for subscribing to `WorktreeStoreEvent` and
/// forwarding changed absolute paths to `watcher.update(cx, |w, cx| w.notify_changed(paths, cx))`.
pub fn create_index_watcher(
    service: Arc<LocalExpertService>,
    cx: &mut App,
) -> Entity<IndexWatcher> {
    cx.new(|cx| IndexWatcher::new(service, cx))
}
