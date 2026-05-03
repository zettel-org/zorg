//! Live workspace watching contracts for Zorg.
//!
//! The crate owns long-running watch service types so CLI and LSP integrations
//! can consume the same event model. The real filesystem loop is implemented in
//! a later phase; this phase defines the boundary and testable filtering logic.

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

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

/// Runs a bounded watcher service skeleton.
///
/// The unbounded filesystem loop is intentionally left for the reindex-engine
/// phase. Bounded controls allow CLI and service tests to validate state output
/// without creating a real `notify` watcher.
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
        RunControl::Unbounded | RunControl::StopAfterEvents(_) => Err(ZorgError::Unsupported(
            "unbounded zorg-watch service is implemented in the watcher reindex engine phase",
        )),
    }
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
}
