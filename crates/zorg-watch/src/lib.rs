//! Live workspace watching contracts for Zorg.
//!
//! The crate owns long-running watch service types so CLI and LSP integrations
//! can consume the same event model.

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use notify::event::{AccessKind, AccessMode, CreateKind, ModifyKind, RemoveKind, RenameMode};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use zorg_core::{ZorgError, ZorgResult};
use zorg_store::{ReindexSummary, Store, StoreOptions};

/// Default debounce used by the live watcher when no caller override is set.
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(250);

/// Options for a `zorg-watch` service instance.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WatchOptions {
    root: PathBuf,
    database_path: PathBuf,
    debounce: Duration,
    run_control: RunControl,
}

impl WatchOptions {
    /// Builds watcher options from an explicit root and database path.
    pub fn new(root: impl Into<PathBuf>, database_path: impl Into<PathBuf>) -> ZorgResult<Self> {
        let store_options = StoreOptions::new(root, database_path)?;
        Ok(Self::from_store_options(store_options))
    }

    /// Builds watcher options from store options.
    #[must_use]
    pub fn from_store_options(store_options: StoreOptions) -> Self {
        Self {
            root: store_options.corpus_root().to_path_buf(),
            database_path: store_options.database_path().to_path_buf(),
            debounce: DEFAULT_DEBOUNCE,
            run_control: RunControl::Unbounded,
        }
    }

    /// Overrides the debounce interval.
    #[must_use]
    pub fn with_debounce(mut self, debounce: Duration) -> Self {
        self.debounce = debounce;
        self
    }

    /// Overrides the bounded-run control.
    #[must_use]
    pub fn with_run_control(mut self, run_control: RunControl) -> Self {
        self.run_control = run_control;
        self
    }

    /// Returns the watched corpus root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the SQLite database path that must be ignored by watch events.
    #[must_use]
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    /// Returns the debounce interval.
    #[must_use]
    pub const fn debounce(&self) -> Duration {
        self.debounce
    }

    /// Returns the bounded-run control.
    #[must_use]
    pub const fn run_control(&self) -> RunControl {
        self.run_control
    }

    fn store_options(&self) -> ZorgResult<StoreOptions> {
        StoreOptions::new(&self.root, &self.database_path)
    }
}

/// Optional controls used by tests and bounded CLI invocations.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RunControl {
    /// Run until external shutdown.
    Unbounded,
    /// Emit readiness and stop without opening a filesystem watcher.
    StopAfterReady,
    /// Run exactly one store reindex pass and stop.
    InitialReindexOnly,
    /// Stop after observing this many accepted filesystem events.
    StopAfterEvents(u64),
}

/// Filesystem event shape consumed by the debounced reindex engine.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WatchEvent {
    /// A source or directory was created.
    Created { path: PathBuf },
    /// A source was written or metadata changed.
    Written { path: PathBuf },
    /// A source or directory was removed.
    Removed { path: PathBuf },
    /// A source or directory moved within the watched tree.
    Renamed { from: PathBuf, to: PathBuf },
    /// The watcher backend reported overflow or otherwise requested a rescan.
    RescanNeeded { reason: String },
    /// A path event was intentionally ignored before indexing.
    Ignored { path: PathBuf, reason: IgnoreReason },
}

impl WatchEvent {
    /// Returns whether this event should schedule a debounced full reindex.
    #[must_use]
    pub const fn requests_reindex(&self) -> bool {
        !matches!(self, Self::Ignored { .. })
    }
}

/// Runtime state emitted by a watcher service.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WatchState {
    /// Watched corpus root.
    pub root: PathBuf,
    /// SQLite database path owned by this watcher.
    pub database_path: PathBuf,
    /// State payload.
    pub kind: WatchStateKind,
}

impl WatchState {
    fn new(options: &WatchOptions, kind: WatchStateKind) -> Self {
        Self {
            root: options.root.clone(),
            database_path: options.database_path.clone(),
            kind,
        }
    }
}

/// Lifecycle states suitable for CLI text/JSON output and future editor jobs.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum WatchStateKind {
    /// The service has accepted configuration and is initializing.
    Starting,
    /// Filesystem watching is active.
    Ready,
    /// A debounced indexing pass is running.
    Indexing,
    /// An indexing pass completed.
    Indexed { summary: WatchIndexSummary },
    /// The service is still alive but graph freshness is degraded.
    Degraded { message: String },
    /// A recoverable or terminal error occurred.
    Error { message: String },
    /// Shutdown has been requested.
    Stopping,
    /// The service has stopped.
    Stopped,
}

/// Stable subset of `zorg-store` reindex fields emitted by the watcher.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct WatchIndexSummary {
    /// Number of canonical `.z` files discovered under the corpus root.
    pub discovered_files: usize,
    /// Number of files parsed and written to the index.
    pub indexed_files: usize,
    /// Number of files already matching the indexed hash.
    pub unchanged_files: usize,
    /// Number of files newly added to the index.
    pub new_files: usize,
    /// Number of files whose content changed.
    pub changed_files: usize,
    /// Number of indexed files no longer present on disk.
    pub deleted_files: usize,
    /// Number of zettel rows written.
    pub zettel_count: usize,
    /// Number of diagnostic rows written.
    pub diagnostic_count: usize,
    /// Number of materialized effective tag rows currently indexed.
    pub effective_tag_count: usize,
    /// Latest successful index timestamp in Unix milliseconds when available.
    pub last_indexed_at_unix_ms: Option<i64>,
}

impl From<ReindexSummary> for WatchIndexSummary {
    fn from(summary: ReindexSummary) -> Self {
        Self {
            discovered_files: summary.discovered_files,
            indexed_files: summary.indexed_files,
            unchanged_files: summary.unchanged_files,
            new_files: summary.new_files,
            changed_files: summary.changed_files,
            deleted_files: summary.deleted_files,
            zettel_count: summary.zettel_count,
            diagnostic_count: summary.diagnostic_count,
            effective_tag_count: summary.effective_tag_count,
            last_indexed_at_unix_ms: summary.last_indexed_at_unix_ms,
        }
    }
}

/// Callback boundary used by CLI, LSP, and tests.
pub trait WatchEventSink: Send + Sync + 'static {
    /// Emits one watcher lifecycle state.
    fn emit(&self, state: WatchState);
}

impl<F> WatchEventSink for F
where
    F: Fn(WatchState) + Send + Sync + 'static,
{
    fn emit(&self, state: WatchState) {
        self(state);
    }
}

/// Result returned when a watcher run exits.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct WatchRunResult {
    /// Number of indexing passes completed by this run.
    pub indexed_passes: usize,
}

/// Runs a watcher service.
///
/// The service opens one `Store` and keeps all reindex writes serialized through
/// that handle. Bounded run controls stop accepting new events after their
/// condition is met, finish the current debounced reindex pass, then emit
/// `stopping` and `stopped`.
pub async fn run_watch_service<S>(options: WatchOptions, sink: S) -> ZorgResult<WatchRunResult>
where
    S: WatchEventSink,
{
    sink.emit(WatchState::new(&options, WatchStateKind::Starting));

    match options.run_control {
        RunControl::StopAfterReady => {
            sink.emit(WatchState::new(&options, WatchStateKind::Ready));
            sink.emit(WatchState::new(&options, WatchStateKind::Stopping));
            sink.emit(WatchState::new(&options, WatchStateKind::Stopped));
            Ok(WatchRunResult { indexed_passes: 0 })
        }
        RunControl::InitialReindexOnly => {
            let mut store = Store::open_with_options(options.store_options()?)?;
            sink.emit(WatchState::new(&options, WatchStateKind::Indexing));
            let summary = store.reindex()?;
            sink.emit(WatchState::new(
                &options,
                WatchStateKind::Indexed {
                    summary: summary.into(),
                },
            ));
            sink.emit(WatchState::new(&options, WatchStateKind::Stopping));
            sink.emit(WatchState::new(&options, WatchStateKind::Stopped));
            Ok(WatchRunResult { indexed_passes: 1 })
        }
        RunControl::Unbounded | RunControl::StopAfterEvents(_) => {
            run_notify_loop(options, sink).await
        }
    }
}

async fn run_notify_loop<S>(options: WatchOptions, sink: S) -> ZorgResult<WatchRunResult>
where
    S: WatchEventSink,
{
    let mut store = match Store::open_with_options(options.store_options()?) {
        Ok(store) => store,
        Err(error) => {
            sink.emit(WatchState::new(
                &options,
                WatchStateKind::Error {
                    message: error.to_string(),
                },
            ));
            return Err(error);
        }
    };
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut watcher = match notify::recommended_watcher(move |event| {
        let _ = sender.send(event);
    }) {
        Ok(watcher) => watcher,
        Err(error) => return emit_setup_error(&options, &sink, error),
    };
    if let Err(error) = watcher.watch(options.root(), RecursiveMode::Recursive) {
        return emit_setup_error(&options, &sink, error);
    }

    let mut scheduler = DebounceScheduler::new(options.debounce());
    let mut accepted_events = 0_u64;
    let mut indexed_passes = 0_usize;
    let mut stopping = false;

    sink.emit(WatchState::new(&options, WatchStateKind::Ready));

    loop {
        if scheduler
            .next_deadline()
            .is_some_and(|deadline| deadline <= Instant::now())
        {
            run_debounced_reindex(
                &options,
                &sink,
                &mut store,
                &mut scheduler,
                &mut indexed_passes,
            );
            if stopping && scheduler.next_deadline().is_none() {
                break;
            }
            continue;
        }

        if stopping {
            let Some(deadline) = scheduler.next_deadline() else {
                break;
            };
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
            continue;
        }

        match scheduler.next_deadline() {
            Some(deadline) => {
                let wait = deadline.saturating_duration_since(Instant::now());
                if let Ok(event) = tokio::time::timeout(wait, receiver.recv()).await {
                    let Some(event) = event else {
                        break;
                    };
                    process_notify_event(
                        &options,
                        &sink,
                        &mut scheduler,
                        event,
                        &mut accepted_events,
                    );
                    stopping = should_stop_after_events(options.run_control(), accepted_events);
                    if stopping {
                        sink.emit(WatchState::new(&options, WatchStateKind::Stopping));
                    }
                }
            }
            None => {
                let Some(event) = receiver.recv().await else {
                    break;
                };
                process_notify_event(&options, &sink, &mut scheduler, event, &mut accepted_events);
                stopping = should_stop_after_events(options.run_control(), accepted_events);
                if stopping {
                    sink.emit(WatchState::new(&options, WatchStateKind::Stopping));
                }
            }
        }
    }

    drop(watcher);
    if !stopping {
        sink.emit(WatchState::new(&options, WatchStateKind::Stopping));
    }
    sink.emit(WatchState::new(&options, WatchStateKind::Stopped));
    Ok(WatchRunResult { indexed_passes })
}

fn process_notify_event<S>(
    options: &WatchOptions,
    sink: &S,
    scheduler: &mut DebounceScheduler,
    event: notify::Result<Event>,
    accepted_events: &mut u64,
) where
    S: WatchEventSink,
{
    match event {
        Ok(event) => {
            for watch_event in watch_events_from_notify_event(options, event) {
                if watch_event.requests_reindex() {
                    *accepted_events += 1;
                }
                scheduler.record_event(&watch_event, Instant::now());
            }
        }
        Err(error) => {
            sink.emit(WatchState::new(
                options,
                WatchStateKind::Error {
                    message: error.to_string(),
                },
            ));
            let event = WatchEvent::RescanNeeded {
                reason: format!("watch backend error: {error}"),
            };
            *accepted_events += 1;
            scheduler.record_event(&event, Instant::now());
        }
    }
}

fn run_debounced_reindex<S>(
    options: &WatchOptions,
    sink: &S,
    store: &mut Store,
    scheduler: &mut DebounceScheduler,
    indexed_passes: &mut usize,
) where
    S: WatchEventSink,
{
    if scheduler.take_due(Instant::now()).is_none() {
        return;
    }

    sink.emit(WatchState::new(options, WatchStateKind::Indexing));
    match store.reindex() {
        Ok(summary) => {
            *indexed_passes += 1;
            sink.emit(WatchState::new(
                options,
                WatchStateKind::Indexed {
                    summary: summary.into(),
                },
            ));
        }
        Err(error) => sink.emit(WatchState::new(
            options,
            WatchStateKind::Error {
                message: error.to_string(),
            },
        )),
    }
}

fn should_stop_after_events(run_control: RunControl, accepted_events: u64) -> bool {
    matches!(run_control, RunControl::StopAfterEvents(limit) if accepted_events >= limit)
}

fn watch_events_from_notify_event(options: &WatchOptions, event: Event) -> Vec<WatchEvent> {
    if event.need_rescan() {
        return vec![WatchEvent::RescanNeeded {
            reason: event.info().map_or_else(
                || "watch backend requested rescan".to_owned(),
                str::to_owned,
            ),
        }];
    }

    match event.kind {
        EventKind::Create(_) => single_path_events(options, &event, AcceptedEventKind::Created),
        EventKind::Remove(_) => single_path_events(options, &event, AcceptedEventKind::Removed),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() >= 2 => {
            rename_event(options, &event.paths[0], &event.paths[1])
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
            single_path_events(options, &event, AcceptedEventKind::Removed)
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
            single_path_events(options, &event, AcceptedEventKind::Created)
        }
        EventKind::Modify(_) => single_path_events(options, &event, AcceptedEventKind::Written),
        EventKind::Any | EventKind::Other => vec![WatchEvent::RescanNeeded {
            reason: format!("imprecise watch event: {:?}", event.kind),
        }],
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
            single_path_events(options, &event, AcceptedEventKind::Written)
        }
        EventKind::Access(_) => Vec::new(),
    }
}

fn single_path_events(
    options: &WatchOptions,
    event: &Event,
    event_kind: AcceptedEventKind,
) -> Vec<WatchEvent> {
    event
        .paths
        .iter()
        .map(|path| {
            classify_watch_path(options, path, path_kind_for_event(path, &event.kind))
                .into_event(event_kind)
        })
        .collect()
}

fn rename_event(options: &WatchOptions, from: &Path, to: &Path) -> Vec<WatchEvent> {
    let from = normalize_path(from);
    let to = normalize_path(to);
    let from_decision = classify_watch_path(
        options,
        &from,
        path_kind_for_event(
            &from,
            &EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
        ),
    );
    let to_decision = classify_watch_path(
        options,
        &to,
        path_kind_for_event(&to, &EventKind::Modify(ModifyKind::Name(RenameMode::Both))),
    );

    if from_decision.requests_reindex() || to_decision.requests_reindex() {
        vec![WatchEvent::Renamed { from, to }]
    } else {
        vec![
            from_decision.into_event(AcceptedEventKind::Removed),
            to_decision.into_event(AcceptedEventKind::Created),
        ]
    }
}

fn path_kind_for_event(path: &Path, event_kind: &EventKind) -> PathKind {
    match event_kind {
        EventKind::Create(CreateKind::File) | EventKind::Remove(RemoveKind::File) => PathKind::File,
        EventKind::Create(CreateKind::Folder) | EventKind::Remove(RemoveKind::Folder) => {
            PathKind::Directory
        }
        _ => std::fs::metadata(path).map_or(PathKind::Unknown, |metadata| {
            if metadata.is_dir() {
                PathKind::Directory
            } else {
                PathKind::File
            }
        }),
    }
}

fn notify_error(error: notify::Error) -> ZorgError {
    ZorgError::OperationFailed {
        message: format!("watcher setup failed: {error}"),
    }
}

fn emit_setup_error<S, T>(options: &WatchOptions, sink: &S, error: notify::Error) -> ZorgResult<T>
where
    S: WatchEventSink,
{
    let error = notify_error(error);
    sink.emit(WatchState::new(
        options,
        WatchStateKind::Error {
            message: error.to_string(),
        },
    ));
    sink.emit(WatchState::new(options, WatchStateKind::Stopping));
    sink.emit(WatchState::new(options, WatchStateKind::Stopped));
    Err(error)
}

/// Path kind supplied by filesystem metadata or a fake test event.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PathKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// Metadata is unavailable, commonly for delete events.
    Unknown,
}

/// Result of applying watcher path filters.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PathDecision {
    /// Canonical Zorg source event accepted for debounce.
    Source(PathBuf),
    /// Directory event accepted as a traversal/container hint.
    Directory(PathBuf),
    /// Event ignored before any database mutation.
    Ignored {
        /// Ignored path.
        path: PathBuf,
        /// Ignore reason.
        reason: IgnoreReason,
    },
}

impl PathDecision {
    /// Converts an accepted decision into a watch event.
    #[must_use]
    pub fn into_event(self, event_kind: AcceptedEventKind) -> WatchEvent {
        match (self, event_kind) {
            (Self::Source(path) | Self::Directory(path), AcceptedEventKind::Created) => {
                WatchEvent::Created { path }
            }
            (Self::Source(path) | Self::Directory(path), AcceptedEventKind::Written) => {
                WatchEvent::Written { path }
            }
            (Self::Source(path) | Self::Directory(path), AcceptedEventKind::Removed) => {
                WatchEvent::Removed { path }
            }
            (Self::Ignored { path, reason }, _) => WatchEvent::Ignored { path, reason },
        }
    }

    /// Returns true when this path should schedule a debounced reindex.
    #[must_use]
    pub const fn requests_reindex(&self) -> bool {
        matches!(self, Self::Source(_) | Self::Directory(_))
    }
}

/// Accepted single-path event classes.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AcceptedEventKind {
    /// Path was created.
    Created,
    /// Path was written.
    Written,
    /// Path was removed.
    Removed,
}

/// Reason a path event is ignored before indexing.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IgnoreReason {
    /// The path is outside the configured root.
    OutsideRoot,
    /// The path is the configured database or lives under `.zorg`.
    InternalState,
    /// The path has a legacy Zorg extension.
    LegacyExtension,
    /// The path looks like an editor temporary, swap, backup, or scratch file.
    EditorScratch,
    /// The path is not a canonical Zorg source or directory.
    NonSource,
}

/// Applies Phase 11A watcher path filtering rules to one event path.
#[must_use]
pub fn classify_watch_path(
    options: &WatchOptions,
    path: impl AsRef<Path>,
    kind: PathKind,
) -> PathDecision {
    let path = path.as_ref();
    let path = normalize_path(path);

    if !is_under_root(&path, options.root()) {
        return ignored(path, IgnoreReason::OutsideRoot);
    }

    if is_database_related_path(&path, options.database_path())
        || has_internal_component(&path, options)
    {
        return ignored(path, IgnoreReason::InternalState);
    }

    let relative_path = path.strip_prefix(options.root()).unwrap_or(path.as_path());
    if has_editor_scratch_component(relative_path) {
        return ignored(path, IgnoreReason::EditorScratch);
    }

    if kind == PathKind::Directory {
        return PathDecision::Directory(path);
    }

    if has_legacy_extension(&path) {
        return ignored(path, IgnoreReason::LegacyExtension);
    }

    if has_editor_scratch_name(&path) {
        return ignored(path, IgnoreReason::EditorScratch);
    }

    if has_canonical_source_extension(&path) {
        PathDecision::Source(path)
    } else {
        ignored(path, IgnoreReason::NonSource)
    }
}

/// Debounce scheduler used by the watcher reindex loop.
#[derive(Debug, Clone)]
pub struct DebounceScheduler {
    debounce: Duration,
    pending: Option<PendingBatch>,
}

impl DebounceScheduler {
    /// Creates a scheduler with the supplied debounce interval.
    #[must_use]
    pub const fn new(debounce: Duration) -> Self {
        Self {
            debounce,
            pending: None,
        }
    }

    /// Records a watch event and updates the pending deadline if needed.
    pub fn record_event(&mut self, event: &WatchEvent, now: Instant) -> DebounceRecord {
        if !event.requests_reindex() {
            return DebounceRecord::Ignored;
        }

        let due_at = now + self.debounce;
        match &mut self.pending {
            Some(pending) => {
                pending.event_count += 1;
                pending.due_at = due_at;
                if matches!(event, WatchEvent::RescanNeeded { .. }) {
                    pending.rescan_required = true;
                }
                DebounceRecord::Extended { due_at }
            }
            None => {
                self.pending = Some(PendingBatch {
                    started_at: now,
                    due_at,
                    event_count: 1,
                    rescan_required: matches!(event, WatchEvent::RescanNeeded { .. }),
                });
                DebounceRecord::Started { due_at }
            }
        }
    }

    /// Returns the current deadline for a pending batch.
    #[must_use]
    pub fn next_deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|pending| pending.due_at)
    }

    /// Returns true when a pending batch is ready.
    #[must_use]
    pub fn is_due(&self, now: Instant) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| now >= pending.due_at)
    }

    /// Takes and returns the pending batch when it is due.
    pub fn take_due(&mut self, now: Instant) -> Option<DebouncedBatch> {
        if !self.is_due(now) {
            return None;
        }

        self.pending.take().map(|pending| DebouncedBatch {
            started_at: pending.started_at,
            due_at: pending.due_at,
            event_count: pending.event_count,
            rescan_required: pending.rescan_required,
        })
    }
}

/// Result of recording a fake or real watch event into the debounce scheduler.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DebounceRecord {
    /// Ignored events never schedule indexing.
    Ignored,
    /// A new batch was started.
    Started { due_at: Instant },
    /// An existing batch deadline was extended.
    Extended { due_at: Instant },
}

#[derive(Debug, Clone)]
struct PendingBatch {
    started_at: Instant,
    due_at: Instant,
    event_count: usize,
    rescan_required: bool,
}

/// Debounced batch ready for one full incremental `Store::reindex()` pass.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct DebouncedBatch {
    /// Time the first event in the batch was recorded.
    pub started_at: Instant,
    /// Deadline that made this batch ready.
    pub due_at: Instant,
    /// Number of accepted events coalesced into the batch.
    pub event_count: usize,
    /// True when backend overflow or rescan was part of the batch.
    pub rescan_required: bool,
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn is_under_root(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

fn is_database_related_path(path: &Path, database_path: &Path) -> bool {
    if path == database_path || path.starts_with(database_path) {
        return true;
    }

    let Some(database_parent) = database_path.parent() else {
        return false;
    };
    let Some(database_name) = database_path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    path.parent() == Some(database_parent)
        && matches!(
            file_name.strip_prefix(database_name),
            Some("-wal" | "-shm" | "-journal")
        )
}

fn has_internal_component(path: &Path, options: &WatchOptions) -> bool {
    path.strip_prefix(options.root())
        .unwrap_or(path)
        .components()
        .any(|component| component.as_os_str() == ".zorg")
}

fn has_editor_scratch_component(path: &Path) -> bool {
    path.components().any(|component| {
        let Some(name) = component.as_os_str().to_str() else {
            return false;
        };
        name.starts_with('.') || name.starts_with('#') || name.ends_with('#')
    })
}

fn has_editor_scratch_name(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    file_name.ends_with('~')
        || file_name.ends_with(".tmp")
        || file_name.ends_with(".temp")
        || file_name.ends_with(".bak")
        || file_name.ends_with(".orig")
        || file_name.ends_with(".part")
        || file_name.ends_with(".swp")
        || file_name.ends_with(".swo")
        || file_name.ends_with(".crswap")
}

fn has_legacy_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("zo" | "zoq" | "zot" | "zoc")
    )
}

fn has_canonical_source_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension == "z")
}

fn ignored(path: PathBuf, reason: IgnoreReason) -> PathDecision {
    PathDecision::Ignored { path, reason }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use tempfile::TempDir;
    use tokio::task::LocalSet;

    fn options() -> WatchOptions {
        WatchOptions::new(
            "/tmp/zorg-watch-root",
            "/tmp/zorg-watch-root/.zorg/zorg.sqlite3",
        )
        .expect("watch options")
    }

    fn reason(decision: PathDecision) -> IgnoreReason {
        match decision {
            PathDecision::Ignored { reason, .. } => reason,
            other => panic!("expected ignored decision, got {other:?}"),
        }
    }

    fn write_source(root: &Path, name: &str, source: &str) {
        std::fs::write(root.join(name), source).expect("write source");
    }

    fn source(id: &str, title: &str, body: &str) -> String {
        format!(
            "\
%%% @{id} #z/ref
{title}
%%%

{body}
"
        )
    }

    fn temp_options(temp: &TempDir) -> WatchOptions {
        let root = temp.path().join("corpus");
        std::fs::create_dir_all(&root).expect("create corpus");
        WatchOptions::new(&root, temp.path().join("zorg.sqlite3"))
            .expect("watch options")
            .with_debounce(Duration::from_millis(50))
            .with_run_control(RunControl::StopAfterEvents(1))
    }

    fn run_bounded_watch(
        options: WatchOptions,
        mutate: impl FnOnce() + 'static,
    ) -> (WatchRunResult, Vec<WatchState>) {
        let states = Arc::new(Mutex::new(Vec::new()));
        let sink_states = Arc::clone(&states);
        let sink = move |state| sink_states.lock().expect("states lock").push(state);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("runtime");
        let local = LocalSet::new();
        let states_for_ready = Arc::clone(&states);

        let result = local.block_on(&runtime, async move {
            let handle = tokio::task::spawn_local(run_watch_service(options, sink));
            wait_for_ready(states_for_ready).await;
            mutate();
            tokio::time::timeout(Duration::from_secs(5), handle)
                .await
                .expect("watch service timed out")
                .expect("watch service join")
                .expect("watch service result")
        });
        let states = Arc::try_unwrap(states)
            .expect("states still referenced")
            .into_inner()
            .expect("states mutex");

        (result, states)
    }

    async fn wait_for_ready(states: Arc<Mutex<Vec<WatchState>>>) {
        for _ in 0..100 {
            if states
                .lock()
                .expect("states lock")
                .iter()
                .any(|state| matches!(state.kind, WatchStateKind::Ready))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        panic!("watch service did not become ready");
    }

    #[test]
    fn path_filter_accepts_canonical_sources_and_directories() {
        let options = options();

        assert_eq!(
            classify_watch_path(&options, "/tmp/zorg-watch-root/notes/day.z", PathKind::File),
            PathDecision::Source(PathBuf::from("/tmp/zorg-watch-root/notes/day.z"))
        );
        assert_eq!(
            classify_watch_path(&options, "/tmp/zorg-watch-root/notes", PathKind::Directory),
            PathDecision::Directory(PathBuf::from("/tmp/zorg-watch-root/notes"))
        );
        assert_eq!(
            classify_watch_path(
                &options,
                "/tmp/zorg-watch-root/removed.z",
                PathKind::Unknown
            ),
            PathDecision::Source(PathBuf::from("/tmp/zorg-watch-root/removed.z"))
        );
    }

    #[test]
    fn path_filter_rejects_internal_legacy_scratch_and_non_source_paths() {
        let options = options();

        assert_eq!(
            reason(classify_watch_path(
                &options,
                "/tmp/other/day.z",
                PathKind::File
            )),
            IgnoreReason::OutsideRoot
        );
        assert_eq!(
            reason(classify_watch_path(
                &options,
                "/tmp/zorg-watch-root/.zorg/zorg.sqlite3",
                PathKind::File
            )),
            IgnoreReason::InternalState
        );
        let custom_db = WatchOptions::new(
            "/tmp/zorg-watch-root",
            "/tmp/zorg-watch-root/index/custom.sqlite3",
        )
        .expect("custom db options");
        assert_eq!(
            reason(classify_watch_path(
                &custom_db,
                "/tmp/zorg-watch-root/index/custom.sqlite3-wal",
                PathKind::File
            )),
            IgnoreReason::InternalState
        );
        assert_eq!(
            reason(classify_watch_path(
                &options,
                "/tmp/zorg-watch-root/legacy.zoq",
                PathKind::File
            )),
            IgnoreReason::LegacyExtension
        );
        assert_eq!(
            reason(classify_watch_path(
                &options,
                "/tmp/zorg-watch-root/.note.z.swp",
                PathKind::File
            )),
            IgnoreReason::EditorScratch
        );
        assert_eq!(
            reason(classify_watch_path(
                &options,
                "/tmp/zorg-watch-root/readme.md",
                PathKind::File
            )),
            IgnoreReason::NonSource
        );
    }

    #[test]
    fn debounce_coalesces_fake_events_and_extends_deadline() {
        let mut scheduler = DebounceScheduler::new(Duration::from_millis(50));
        let start = Instant::now();
        let source = WatchEvent::Written {
            path: PathBuf::from("/tmp/zorg-watch-root/day.z"),
        };
        let second = start + Duration::from_millis(20);

        assert_eq!(
            scheduler.record_event(&source, start),
            DebounceRecord::Started {
                due_at: start + Duration::from_millis(50)
            }
        );
        assert_eq!(
            scheduler.record_event(&source, second),
            DebounceRecord::Extended {
                due_at: second + Duration::from_millis(50)
            }
        );
        assert!(
            scheduler
                .take_due(start + Duration::from_millis(69))
                .is_none()
        );

        let batch = scheduler
            .take_due(start + Duration::from_millis(70))
            .expect("due batch");
        assert_eq!(batch.event_count, 2);
        assert_eq!(batch.started_at, start);
        assert_eq!(batch.due_at, second + Duration::from_millis(50));
        assert!(!batch.rescan_required);
        assert!(scheduler.next_deadline().is_none());
    }

    #[test]
    fn debounce_ignores_ignored_events_and_tracks_rescan_batches() {
        let mut scheduler = DebounceScheduler::new(Duration::from_millis(10));
        let start = Instant::now();
        let ignored = WatchEvent::Ignored {
            path: PathBuf::from("/tmp/zorg-watch-root/readme.md"),
            reason: IgnoreReason::NonSource,
        };
        let rescan = WatchEvent::RescanNeeded {
            reason: "backend overflow".to_owned(),
        };

        assert_eq!(
            scheduler.record_event(&ignored, start),
            DebounceRecord::Ignored
        );
        assert!(scheduler.next_deadline().is_none());

        scheduler.record_event(&rescan, start);
        let batch = scheduler
            .take_due(start + Duration::from_millis(10))
            .expect("rescan batch");
        assert_eq!(batch.event_count, 1);
        assert!(batch.rescan_required);
    }

    #[test]
    fn watcher_reindexes_new_source_after_save_event() {
        let temp = tempfile::tempdir().expect("tempdir");
        let options = temp_options(&temp);
        let root = options.root().to_path_buf();
        let db = options.database_path().to_path_buf();

        let (result, states) = run_bounded_watch(options, move || {
            write_source(
                &root,
                "saved.z",
                &source("saved", "Saved", "Created by watcher."),
            );
        });

        assert_eq!(result.indexed_passes, 1);
        assert!(states.iter().any(|state| matches!(
            state.kind,
            WatchStateKind::Indexed { summary }
                if summary.new_files == 1 && summary.discovered_files == 1
        )));
        let store = Store::open_with_options(
            StoreOptions::new(temp.path().join("corpus"), db).expect("options"),
        )
        .expect("open store");
        let saved = store
            .lookup_zettel_by_canonical_id("saved")
            .expect("lookup")
            .expect("saved zettel");
        assert_eq!(saved.body_text, "Created by watcher.");
    }

    #[test]
    fn watcher_reindexes_deleted_source() {
        let temp = tempfile::tempdir().expect("tempdir");
        let options = temp_options(&temp);
        let root = options.root().to_path_buf();
        let db = options.database_path().to_path_buf();
        write_source(&root, "delete.z", &source("delete", "Delete", "Remove me."));
        {
            let mut store =
                Store::open_with_options(StoreOptions::new(&root, &db).expect("options"))
                    .expect("open store");
            store.reindex().expect("initial reindex");
        }

        let (result, states) = run_bounded_watch(options, move || {
            std::fs::remove_file(root.join("delete.z")).expect("remove source");
        });

        assert_eq!(result.indexed_passes, 1);
        assert!(states.iter().any(|state| matches!(
            state.kind,
            WatchStateKind::Indexed { summary } if summary.deleted_files == 1
        )));
        let store = Store::open_with_options(
            StoreOptions::new(temp.path().join("corpus"), db).expect("options"),
        )
        .expect("open store");
        assert!(
            store
                .lookup_zettel_by_canonical_id("delete")
                .expect("lookup")
                .is_none()
        );
    }

    #[test]
    fn watcher_reindexes_source_rename_as_delete_plus_add() {
        let temp = tempfile::tempdir().expect("tempdir");
        let options = temp_options(&temp);
        let root = options.root().to_path_buf();
        let db = options.database_path().to_path_buf();
        write_source(&root, "old.z", &source("old", "Old", "Old body."));
        {
            let mut store =
                Store::open_with_options(StoreOptions::new(&root, &db).expect("options"))
                    .expect("open store");
            store.reindex().expect("initial reindex");
        }

        let (result, states) = run_bounded_watch(options, move || {
            std::fs::rename(root.join("old.z"), root.join("new.z")).expect("rename source");
            write_source(&root, "new.z", &source("new", "New", "New body."));
        });

        assert_eq!(result.indexed_passes, 1);
        assert!(states.iter().any(|state| matches!(
            state.kind,
            WatchStateKind::Indexed { summary }
                if summary.new_files == 1 && summary.deleted_files == 1
        )));
        let store = Store::open_with_options(
            StoreOptions::new(temp.path().join("corpus"), db).expect("options"),
        )
        .expect("open store");
        assert!(
            store
                .lookup_zettel_by_canonical_id("old")
                .expect("lookup")
                .is_none()
        );
        assert!(
            store
                .lookup_zettel_by_canonical_id("new")
                .expect("lookup")
                .is_some()
        );
    }
}
