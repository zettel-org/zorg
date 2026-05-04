use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use zorg_store::StoreOptions;

use crate::actions::{self, CaptureOutcome, FixApplyOutcome, ReindexOutcome, TodoApplyOutcome};
use crate::model::SearchHistory;
use crate::model::{
    AutoRefreshConfig, AutoRefreshEvent, AutoRefreshSkipReason, CaptureDraft,
    CaptureTemplatePicker, DashboardFrame, DashboardOverlay, DashboardRenderState,
    DashboardSnapshot, DiagnosticFilterDraft, DiagnosticPreviewContext, GraphLoadState,
    MarkedDiagnosticsSummary, Panel, PanelId, PanelRow, PanelRowId, PendingActivity,
    PendingOperationKind, SearchPanel, SeverityKind, SingleLineInput, SnapshotFreshness,
    SourceLocation, StatusEvent, TodoActionOverlay, TodoPromptAction, TodoPromptDraft,
    TodoPromptField, YankCommand, YankOverlay, format_duration,
};
use zorg_refactor::TodoDateField;

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
const FRESHNESS_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const DEFAULT_VISIBLE_ROW_COUNT: usize = 10;
const STATUS_EVENT_LIMIT: usize = 50;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum AppCommand {
    Continue,
    Quit,
    Open(SourceLocation),
    Yank(YankCommand),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PanelViewport {
    selected_index: usize,
    scroll_offset: usize,
    visible_row_count: usize,
    total_row_count: usize,
    selected_row_id: Option<PanelRowId>,
}

impl PanelViewport {
    fn new(visible_row_count: usize) -> Self {
        Self {
            selected_index: 0,
            scroll_offset: 0,
            visible_row_count: visible_row_count.max(1),
            total_row_count: 0,
            selected_row_id: None,
        }
    }

    pub(crate) const fn selected_index(&self) -> usize {
        self.selected_index
    }

    #[cfg(test)]
    pub(crate) const fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    fn set_visible_row_count(&mut self, visible_row_count: usize, rows: &[PanelRow]) {
        self.visible_row_count = visible_row_count.max(1);
        self.sync_rows(rows);
    }

    fn sync_rows(&mut self, rows: &[PanelRow]) {
        self.total_row_count = rows.len();
        if rows.is_empty() {
            self.selected_index = 0;
            self.scroll_offset = 0;
            self.selected_row_id = None;
            return;
        }

        if let Some(row_id) = &self.selected_row_id
            && let Some(index) = rows.iter().position(|row| row.row_id() == *row_id)
        {
            self.selected_index = index;
        }

        self.selected_index = self.selected_index.min(rows.len() - 1);
        self.selected_row_id = rows.get(self.selected_index).map(PanelRow::row_id);
        self.clamp_scroll_offset();
    }

    fn set_selected_index(&mut self, index: usize, rows: &[PanelRow]) {
        self.total_row_count = rows.len();
        if rows.is_empty() {
            self.selected_index = 0;
            self.scroll_offset = 0;
            self.selected_row_id = None;
            return;
        }

        self.selected_index = index.min(rows.len() - 1);
        self.selected_row_id = rows.get(self.selected_index).map(PanelRow::row_id);
        self.clamp_scroll_offset();
    }

    fn move_by(&mut self, delta: isize, rows: &[PanelRow]) {
        let index = moved_index(self.selected_index, delta, rows.len());
        self.set_selected_index(index, rows);
    }

    fn move_page(&mut self, direction: isize, rows: &[PanelRow]) {
        let step = self.visible_row_count.max(1) as isize;
        self.move_by(step.saturating_mul(direction), rows);
    }

    fn move_half_page(&mut self, direction: isize, rows: &[PanelRow]) {
        let step = (self.visible_row_count / 2).max(1) as isize;
        self.move_by(step.saturating_mul(direction), rows);
    }

    fn clamp_scroll_offset(&mut self) {
        if self.total_row_count == 0 {
            self.scroll_offset = 0;
            return;
        }

        let visible = self.visible_row_count.max(1);
        let max_offset = self.total_row_count.saturating_sub(visible);
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset.saturating_add(visible) {
            self.scroll_offset = self
                .selected_index
                .saturating_add(1)
                .saturating_sub(visible);
        }
        self.scroll_offset = self.scroll_offset.min(max_offset);
    }
}

fn moved_index(current: usize, delta: isize, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current
            .saturating_add(delta as usize)
            .min(count.saturating_sub(1))
    }
}

fn loading_blocks_key(key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => false,
        KeyCode::Char('/')
        | KeyCode::Char('r')
        | KeyCode::Char('R')
        | KeyCode::Char('c')
        | KeyCode::Char('e')
        | KeyCode::Char('a')
        | KeyCode::Char('t')
        | KeyCode::Char('o')
        | KeyCode::Char(':')
        | KeyCode::Char(' ')
        | KeyCode::Char('f')
        | KeyCode::Char('d')
        | KeyCode::Char('p')
        | KeyCode::Char('s')
        | KeyCode::Char('y')
        | KeyCode::Enter => true,
        _ => false,
    }
}

#[derive(Debug)]
pub(crate) struct AppState {
    frame: DashboardFrame,
    store_options: StoreOptions,
    viewports: BTreeMap<PanelId, PanelViewport>,
    overlay: DashboardOverlay,
    status_events: Vec<StatusEvent>,
    next_status_order: usize,
    generation: usize,
    pending_initial_load: Option<PendingOperation>,
    pending_refresh: Option<PendingOperation>,
    pending_reindex: Option<PendingOperation>,
    pending_capture: Option<PendingOperation>,
    pending_fix_preview: Option<PendingOperation>,
    pending_fix_apply: Option<PendingOperation>,
    pending_todo_apply: Option<PendingOperation>,
    pending_search: Option<PendingOperation>,
    pending_freshness_check: Option<PendingOperation>,
    next_freshness_check_at: Option<Instant>,
    graph_cache: BTreeMap<GraphCacheKey, GraphLoadState>,
    pending_graph: BTreeSet<GraphCacheKey>,
    snapshot_generation: usize,
    auto_refresh_config: Option<AutoRefreshConfig>,
    next_auto_refresh_at: Option<Instant>,
    pending_auto_refresh_generation: Option<usize>,
    last_auto_refresh_skip: Option<AutoRefreshSkipReason>,
    search_due_at: Option<Instant>,
    search_editing: bool,
    search_edit_original: Option<SearchPanel>,
    search_editor: SingleLineInput,
    search_history: SearchHistory,
    activity_tick: usize,
    sender: Sender<AsyncResult>,
    receiver: Receiver<AsyncResult>,
}

#[derive(Debug, Clone, Copy)]
struct PendingOperation {
    generation: usize,
    started_at: Instant,
    kind: PendingOperationKind,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct GraphCacheKey {
    snapshot_generation: usize,
    row_id: PanelRowId,
}

impl PendingOperation {
    fn new(generation: usize, kind: PendingOperationKind) -> Self {
        Self {
            generation,
            started_at: Instant::now(),
            kind,
        }
    }

    fn elapsed(self) -> Duration {
        self.started_at.elapsed()
    }

    fn activity(self, tick: usize) -> PendingActivity {
        PendingActivity::new(self.kind, self.elapsed(), tick)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum RefreshOrigin {
    Manual,
    Auto,
}

impl AppState {
    pub(crate) fn new(frame: DashboardFrame, store_options: StoreOptions) -> Self {
        let (sender, receiver) = mpsc::channel();
        let auto_refresh_config = frame.auto_refresh.config;
        let viewports = frame
            .panels
            .iter()
            .map(|panel| {
                (
                    panel.id.clone(),
                    PanelViewport::new(DEFAULT_VISIBLE_ROW_COUNT),
                )
            })
            .collect();
        let mut state = Self {
            frame,
            store_options,
            viewports,
            overlay: DashboardOverlay::None,
            status_events: Vec::new(),
            next_status_order: 1,
            generation: 0,
            pending_initial_load: None,
            pending_refresh: None,
            pending_reindex: None,
            pending_capture: None,
            pending_fix_preview: None,
            pending_fix_apply: None,
            pending_todo_apply: None,
            pending_search: None,
            pending_freshness_check: None,
            next_freshness_check_at: Some(Instant::now() + FRESHNESS_CHECK_INTERVAL),
            graph_cache: BTreeMap::new(),
            pending_graph: BTreeSet::new(),
            snapshot_generation: 0,
            auto_refresh_config,
            next_auto_refresh_at: auto_refresh_config
                .map(|config| Instant::now() + config.interval),
            pending_auto_refresh_generation: None,
            last_auto_refresh_skip: None,
            search_due_at: None,
            search_editing: false,
            search_edit_original: None,
            search_editor: SingleLineInput::new(""),
            search_history: SearchHistory::default(),
            activity_tick: 0,
            sender,
            receiver,
        };
        state.sync_all_viewports();
        state
    }

    pub(crate) fn frame(&self) -> &DashboardFrame {
        &self.frame
    }

    pub(crate) fn selected_index(&self) -> usize {
        self.active_viewport().selected_index()
    }

    pub(crate) fn active_viewport(&self) -> &PanelViewport {
        self.viewports
            .get(&self.frame.active_panel_id())
            .expect("active panel viewport should exist")
    }

    pub(crate) fn active_render_state(&self) -> DashboardRenderState {
        let viewport = self.active_viewport();
        DashboardRenderState::new(
            viewport.selected_index,
            viewport.scroll_offset,
            viewport.total_row_count,
        )
    }

    pub(crate) fn set_active_visible_row_count(&mut self, visible_row_count: usize) {
        let rows = self.frame.active_rows();
        self.active_viewport_mut()
            .set_visible_row_count(visible_row_count, &rows);
    }

    pub(crate) fn overlay(&self) -> &DashboardOverlay {
        &self.overlay
    }

    #[cfg(test)]
    pub(crate) fn status(&self) -> &str {
        self.latest_status_event()
            .map(|event| event.message.as_str())
            .unwrap_or("")
    }

    pub(crate) fn latest_status_event(&self) -> Option<&StatusEvent> {
        self.status_events.last()
    }

    pub(crate) fn status_events(&self) -> &[StatusEvent] {
        &self.status_events
    }

    pub(crate) fn restore_search_history(&mut self, entries: Vec<String>) {
        self.search_history = SearchHistory::from_entries(entries);
    }

    pub(crate) fn record_state_notice(&mut self, message: String) {
        self.record_status(SeverityKind::Warning, message);
    }

    pub(crate) fn persisted_state(&self, mouse: bool) -> crate::state::PersistedDashboardState {
        crate::state::PersistedDashboardState::from_frame(&self.frame, &self.search_history, mouse)
    }

    pub(crate) fn pending_activity(&self) -> Option<PendingActivity> {
        [
            self.pending_initial_load,
            self.pending_refresh,
            self.pending_reindex,
            self.pending_search,
            self.pending_capture,
            self.pending_fix_preview,
            self.pending_fix_apply,
            self.pending_todo_apply,
            self.pending_freshness_check,
        ]
        .into_iter()
        .flatten()
        .min_by_key(|pending| pending.started_at)
        .map(|pending| pending.activity(self.activity_tick))
    }

    pub(crate) fn advance_activity_tick(&mut self) {
        if self.pending_activity().is_some() {
            self.activity_tick = self.activity_tick.wrapping_add(1);
        }
    }

    #[cfg(test)]
    pub(crate) fn is_search_editing(&self) -> bool {
        self.search_editing
    }

    pub(crate) fn drain_worker_results(&mut self) {
        while let Ok(result) = self.receiver.try_recv() {
            self.apply_async_result(result);
        }
    }

    pub(crate) fn ensure_selected_graph_context(&mut self) {
        let Some(row) = self.frame.selected_zettel_row(self.selected_index()) else {
            self.frame.clear_graph_context();
            return;
        };
        let row_id = row.row_id();
        let key = GraphCacheKey {
            snapshot_generation: self.snapshot_generation,
            row_id: row_id.clone(),
        };
        if let Some(state) = self.graph_cache.get(&key).cloned() {
            self.frame.set_graph_context(row_id, state);
            return;
        }
        if self.pending_graph.contains(&key) {
            self.frame
                .set_graph_context(row_id, GraphLoadState::Loading);
            return;
        }

        self.pending_graph.insert(key.clone());
        self.frame
            .set_graph_context(row_id.clone(), GraphLoadState::Loading);
        self.record_status(SeverityKind::Info, "graph context loading");
        let options = self.store_options.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = crate::data::load_graph_neighborhood(options, &row);
            let _ = sender.send(AsyncResult::GraphContext { key, result });
        });
    }

    pub(crate) fn start_initial_load(&mut self) {
        if self.pending_initial_load.is_some() {
            self.record_status(SeverityKind::Warning, "initial load already running");
            return;
        }
        if !self.frame.is_loading() {
            self.record_status(SeverityKind::Info, "initial load already complete");
            return;
        }

        let generation = self.next_generation();
        self.pending_initial_load = Some(PendingOperation::new(
            generation,
            PendingOperationKind::InitialLoad,
        ));
        self.record_status(SeverityKind::Info, "initial load running");
        let options = self.store_options.clone();
        let query = self.frame.query.clone();
        let dashboard_id = self.frame.requested_dashboard_id.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let snapshot =
                crate::data::load_snapshot(options, query.as_deref(), dashboard_id.as_deref());
            let _ = sender.send(AsyncResult::InitialLoad {
                generation,
                snapshot,
            });
        });
    }

    pub(crate) fn drive_search_debounce(&mut self) {
        if self
            .search_due_at
            .is_some_and(|due_at| Instant::now() >= due_at)
        {
            self.start_search_now(false);
        }
    }

    pub(crate) fn drive_freshness_check(&mut self) {
        let now = Instant::now();
        if !self
            .next_freshness_check_at
            .is_some_and(|due_at| now >= due_at)
        {
            return;
        }
        self.next_freshness_check_at = Some(now + FRESHNESS_CHECK_INTERVAL);
        if self.pending_freshness_check.is_some() || self.has_pending_data_operation() {
            return;
        }
        let Some(captured) = self.frame.snapshot_generation() else {
            return;
        };

        let generation = self.next_generation();
        self.pending_freshness_check = Some(PendingOperation::new(
            generation,
            PendingOperationKind::FreshnessCheck,
        ));
        let options = self.store_options.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let freshness = crate::data::check_snapshot_freshness(options, captured);
            let _ = sender.send(AsyncResult::FreshnessCheck {
                generation,
                freshness,
            });
        });
    }

    pub(crate) fn drive_auto_refresh(&mut self) {
        let Some(config) = self.auto_refresh_config else {
            return;
        };
        let now = Instant::now();
        let due = self
            .next_auto_refresh_at
            .is_some_and(|due_at| now >= due_at);
        let newer_index = matches!(
            self.frame.snapshot_freshness,
            SnapshotFreshness::NewerIndexAvailable { .. }
        );
        if !due && !newer_index {
            return;
        }

        if let Some(reason) = self.auto_refresh_blocker() {
            if due {
                self.next_auto_refresh_at = Some(now + config.interval);
            }
            self.record_auto_refresh_skip(reason);
            return;
        }

        self.next_auto_refresh_at = Some(now + config.interval);
        self.last_auto_refresh_skip = None;
        self.start_refresh_with_origin(RefreshOrigin::Auto);
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> AppCommand {
        if self.overlay.is_confirming_reindex() {
            return self.handle_reindex_confirmation_key(key);
        }

        if matches!(self.overlay, DashboardOverlay::ConfirmFixApply(_)) {
            return self.handle_fix_apply_confirmation_key(key);
        }

        if matches!(self.overlay, DashboardOverlay::ConfirmTodoApply(_)) {
            return self.handle_todo_apply_confirmation_key(key);
        }

        if matches!(self.overlay, DashboardOverlay::TodoPrompt(_)) {
            return self.handle_todo_prompt_key(key);
        }

        if matches!(self.overlay, DashboardOverlay::Yank(_)) {
            return self.handle_yank_key(key);
        }

        if matches!(self.overlay, DashboardOverlay::CapturePicker(_)) {
            return self.handle_capture_picker_key(key);
        }

        if matches!(self.overlay, DashboardOverlay::Capture(_)) {
            return self.handle_capture_key(key);
        }

        if matches!(self.overlay, DashboardOverlay::DiagnosticFilter(_)) {
            return self.handle_diagnostic_filter_key(key);
        }

        if self.search_editing && self.is_swog_help_key(key) {
            self.open_swog_help();
            return AppCommand::Continue;
        }

        if self.search_editing {
            return self.handle_search_key(key);
        }

        if !matches!(self.overlay, DashboardOverlay::None) {
            if matches!(self.overlay, DashboardOverlay::FixPreview(_))
                && matches!(key.code, KeyCode::Char('F'))
            {
                self.confirm_fix_apply();
                return AppCommand::Continue;
            }
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')
            ) || (matches!(self.overlay, DashboardOverlay::FixPreview(_))
                && matches!(key.code, KeyCode::Char('f')))
            {
                self.overlay = DashboardOverlay::None;
            }
            return AppCommand::Continue;
        }

        if self.frame.is_loading() && loading_blocks_key(key) {
            self.record_status(
                SeverityKind::Warning,
                "dashboard still loading: data action unavailable",
            );
            return AppCommand::Continue;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => AppCommand::Quit,
            KeyCode::Char('?') => {
                self.overlay = DashboardOverlay::Help;
                AppCommand::Continue
            }
            key_code
                if self.frame.custom_panel.is_none()
                    && self.frame.panel == Panel::Search
                    && self.is_swog_help_code(key_code) =>
            {
                self.open_swog_help();
                AppCommand::Continue
            }
            KeyCode::Char('L') => {
                self.overlay = DashboardOverlay::EventLog;
                AppCommand::Continue
            }
            KeyCode::Tab => {
                self.switch_panel(self.frame.next_panel_id());
                AppCommand::Continue
            }
            KeyCode::BackTab => {
                self.switch_panel(self.frame.previous_panel_id());
                AppCommand::Continue
            }
            KeyCode::Right => {
                self.switch_panel(self.frame.next_panel_id());
                AppCommand::Continue
            }
            KeyCode::Left => {
                self.switch_panel(self.frame.previous_panel_id());
                AppCommand::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection(1);
                AppCommand::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection(-1);
                AppCommand::Continue
            }
            KeyCode::Char('g') => {
                self.set_selection(0);
                AppCommand::Continue
            }
            KeyCode::Char('G') => {
                let last = self.active_row_count().saturating_sub(1);
                self.set_selection(last);
                AppCommand::Continue
            }
            KeyCode::PageDown => {
                self.move_page(1);
                AppCommand::Continue
            }
            KeyCode::PageUp => {
                self.move_page(-1);
                AppCommand::Continue
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_half_page(1);
                AppCommand::Continue
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_half_page(-1);
                AppCommand::Continue
            }
            KeyCode::Char('/') => {
                self.begin_search_edit();
                self.record_status(
                    SeverityKind::Info,
                    "search edit: type SWOG or @query/id, enter runs, esc cancels",
                );
                AppCommand::Continue
            }
            KeyCode::Char('r') => {
                self.start_refresh();
                AppCommand::Continue
            }
            KeyCode::Char('R') => {
                self.overlay = DashboardOverlay::ConfirmReindex;
                AppCommand::Continue
            }
            KeyCode::Char('c') => {
                self.open_capture_flow();
                AppCommand::Continue
            }
            KeyCode::Char('e') => {
                self.cycle_diagnostic_severity_filter();
                AppCommand::Continue
            }
            KeyCode::Char('a') => {
                self.clear_diagnostic_filters();
                AppCommand::Continue
            }
            KeyCode::Char('t') => {
                self.cycle_today_mode();
                AppCommand::Continue
            }
            KeyCode::Char(':') => {
                self.open_diagnostic_filter_edit();
                AppCommand::Continue
            }
            KeyCode::Char(' ') => {
                self.toggle_selected_diagnostic_mark();
                AppCommand::Continue
            }
            KeyCode::Char('f') => {
                self.start_fix_preview();
                AppCommand::Continue
            }
            KeyCode::Char('d') => {
                self.confirm_mark_done();
                AppCommand::Continue
            }
            KeyCode::Char('p') => {
                self.open_postpone_prompt();
                AppCommand::Continue
            }
            KeyCode::Char('s') => {
                self.open_schedule_prompt();
                AppCommand::Continue
            }
            KeyCode::Char('y') => {
                self.open_yank_overlay();
                AppCommand::Continue
            }
            KeyCode::Char('o') => self.open_selected_source(),
            KeyCode::Enter
                if self.frame.custom_panel.is_none() && self.frame.panel == Panel::Queries =>
            {
                self.run_selected_query_row()
            }
            KeyCode::Enter => self.open_selected_source(),
            _ => AppCommand::Continue,
        }
    }

    pub(crate) fn record_open_result(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.overlay = DashboardOverlay::None;
                self.record_status(SeverityKind::Info, "editor returned");
            }
            Err(message) => {
                self.show_log("Open failed", message);
            }
        }
    }

    pub(crate) fn record_yank_result(
        &mut self,
        request: YankCommand,
        result: Result<actions::ClipboardTransport, String>,
    ) {
        match result {
            Ok(transport) => {
                self.overlay = DashboardOverlay::None;
                self.record_status(
                    SeverityKind::Info,
                    format!(
                        "yanked {} for {} via {}",
                        request.kind.label(),
                        request.target_summary,
                        transport.label()
                    ),
                );
            }
            Err(message) => {
                self.show_log_with_severity(
                    SeverityKind::Warning,
                    "Yank clipboard unavailable",
                    format!(
                        "{message}\n\n{} for {}:\n{}",
                        request.kind.label(),
                        request.target_summary,
                        request.value
                    ),
                );
            }
        }
    }

    fn handle_reindex_confirmation_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.overlay = DashboardOverlay::None;
                self.start_reindex();
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.overlay = DashboardOverlay::None;
                self.record_status(SeverityKind::Warning, "reindex canceled");
            }
            _ => {}
        }
        AppCommand::Continue
    }

    fn handle_todo_apply_confirmation_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let overlay = match self.overlay.clone() {
                    DashboardOverlay::ConfirmTodoApply(overlay) => overlay,
                    _ => return AppCommand::Continue,
                };
                self.overlay = DashboardOverlay::None;
                self.start_todo_apply(overlay);
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.overlay = DashboardOverlay::None;
                self.record_status(SeverityKind::Warning, "todo mark done canceled");
            }
            _ => {}
        }
        AppCommand::Continue
    }

    fn handle_todo_prompt_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Esc => {
                let action = match &self.overlay {
                    DashboardOverlay::TodoPrompt(draft) => draft.action.label(),
                    _ => "todo",
                };
                self.overlay = DashboardOverlay::None;
                self.record_status(SeverityKind::Warning, format!("todo {action} canceled"));
            }
            KeyCode::Enter => {
                if let DashboardOverlay::TodoPrompt(draft) = self.overlay.clone() {
                    self.submit_todo_prompt(draft);
                }
            }
            KeyCode::Tab | KeyCode::Down => {
                self.with_todo_prompt_draft(TodoPromptDraft::next_field);
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.with_todo_prompt_draft(TodoPromptDraft::previous_field);
            }
            KeyCode::Left => {
                self.with_todo_prompt_draft(|draft| {
                    if draft.active == TodoPromptField::Target {
                        draft.cycle_target(-1);
                        draft.clear_error();
                    }
                });
            }
            KeyCode::Right => {
                self.with_todo_prompt_draft(|draft| {
                    if draft.active == TodoPromptField::Target {
                        draft.cycle_target(1);
                        draft.clear_error();
                    }
                });
            }
            KeyCode::Backspace => {
                self.with_todo_prompt_draft(|draft| {
                    if draft.active == TodoPromptField::Date {
                        draft.active_value_mut().pop();
                        draft.clear_error();
                    }
                });
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.with_todo_prompt_draft(|draft| {
                    if draft.active == TodoPromptField::Date {
                        draft.active_value_mut().clear();
                        draft.clear_error();
                    }
                });
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.with_todo_prompt_draft(|draft| {
                    match (draft.active, character) {
                        (TodoPromptField::Target, 'd') => draft.select_target(TodoDateField::Due),
                        (TodoPromptField::Target, 'o') => draft.select_target(TodoDateField::Do),
                        (TodoPromptField::Target, _) => {}
                        (TodoPromptField::Date, character) => {
                            draft.active_value_mut().push(character);
                        }
                    }
                    draft.clear_error();
                });
            }
            _ => {}
        }
        AppCommand::Continue
    }

    fn handle_yank_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.overlay = DashboardOverlay::None;
                self.record_status(SeverityKind::Info, "yank canceled");
                AppCommand::Continue
            }
            KeyCode::Tab | KeyCode::Down => {
                self.with_yank_overlay(|overlay| overlay.move_selection(1));
                AppCommand::Continue
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.with_yank_overlay(|overlay| overlay.move_selection(-1));
                AppCommand::Continue
            }
            KeyCode::Enter | KeyCode::Char('y') => self.yank_selected_overlay_value(),
            KeyCode::Char(character @ '1'..='3') => {
                self.with_yank_overlay(|overlay| {
                    overlay.select_index((character as u8 - b'1') as usize);
                });
                self.yank_selected_overlay_value()
            }
            _ => AppCommand::Continue,
        }
    }

    fn handle_fix_apply_confirmation_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let overlay = match self.overlay.clone() {
                    DashboardOverlay::ConfirmFixApply(overlay) => overlay,
                    _ => return AppCommand::Continue,
                };
                self.overlay = DashboardOverlay::None;
                self.start_fix_apply(overlay);
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.overlay = DashboardOverlay::None;
                self.record_status(SeverityKind::Warning, "fix apply canceled");
            }
            _ => {}
        }
        AppCommand::Continue
    }

    fn handle_capture_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Esc => {
                self.overlay = DashboardOverlay::None;
                self.record_status(SeverityKind::Warning, "capture canceled");
            }
            KeyCode::Tab | KeyCode::Down => self.with_capture_draft(CaptureDraft::next_field),
            KeyCode::BackTab | KeyCode::Up => {
                self.with_capture_draft(CaptureDraft::previous_field);
            }
            KeyCode::Enter => {
                if let DashboardOverlay::Capture(draft) = self.overlay.clone() {
                    self.overlay = DashboardOverlay::None;
                    self.start_capture(draft);
                }
            }
            KeyCode::Backspace => {
                self.with_capture_draft(|draft| {
                    draft.active_value_mut().pop();
                });
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.with_capture_draft(|draft| draft.active_value_mut().clear());
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.with_capture_draft(|draft| draft.active_value_mut().push(character));
            }
            _ => {}
        }
        AppCommand::Continue
    }

    fn handle_capture_picker_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Esc => {
                self.overlay = DashboardOverlay::None;
                self.record_status(SeverityKind::Warning, "capture template selection canceled");
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.with_capture_template_picker(CaptureTemplatePicker::next);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.with_capture_template_picker(CaptureTemplatePicker::previous);
            }
            KeyCode::Enter => {
                let result = match &self.overlay {
                    DashboardOverlay::CapturePicker(picker) => picker.selected_draft(),
                    _ => return AppCommand::Continue,
                };
                match result {
                    Ok(draft) => {
                        let template = draft.template_label();
                        self.overlay = DashboardOverlay::Capture(draft);
                        self.record_status(
                            SeverityKind::Info,
                            format!("capture template selected: {template}"),
                        );
                    }
                    Err(message) => {
                        self.record_status(
                            SeverityKind::Warning,
                            format!("capture template unavailable: {message}"),
                        );
                    }
                }
            }
            _ => {}
        }
        AppCommand::Continue
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> AppCommand {
        if key.kind != KeyEventKind::Press {
            return AppCommand::Continue;
        }
        match key.code {
            KeyCode::Esc => {
                self.cancel_search_edit();
            }
            KeyCode::Enter => {
                self.finish_search_edit();
                self.start_search_now(true);
            }
            KeyCode::Backspace => {
                if self.search_editor.backspace() {
                    self.apply_search_editor_change();
                }
            }
            KeyCode::Delete => {
                if self.search_editor.delete() {
                    self.apply_search_editor_change();
                }
            }
            KeyCode::Left => {
                self.search_editor.move_left();
            }
            KeyCode::Right => {
                self.search_editor.move_right();
            }
            KeyCode::Home => {
                self.search_editor.move_home();
            }
            KeyCode::End => {
                self.search_editor.move_end();
            }
            KeyCode::Up => {
                if let Some(query) = self
                    .search_history
                    .recall_previous(self.search_editor.text())
                {
                    self.search_editor.set_text(query);
                    self.apply_search_editor_change();
                }
            }
            KeyCode::Down => {
                if let Some(query) = self.search_history.recall_next() {
                    self.search_editor.set_text(query);
                    self.apply_search_editor_change();
                }
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.search_editor.delete_previous_word() {
                    self.apply_search_editor_change();
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.search_editor.clear_before_cursor() {
                    self.apply_search_editor_change();
                }
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.search_editor.insert(character);
                self.search_history.cancel_recall();
                self.apply_search_editor_change();
            }
            _ => {}
        }
        AppCommand::Continue
    }

    fn is_swog_help_key(&self, key: KeyEvent) -> bool {
        self.is_swog_help_code(key.code)
            && !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    }

    fn is_swog_help_code(&self, code: KeyCode) -> bool {
        matches!(code, KeyCode::F(1))
            || (self.frame.custom_panel.is_none()
                && self.frame.panel == Panel::Search
                && matches!(code, KeyCode::Char('H')))
    }

    fn open_swog_help(&mut self) {
        self.search_editing = false;
        self.overlay = DashboardOverlay::SwogHelp;
        self.record_status(SeverityKind::Info, "SWOG help opened");
    }

    fn run_selected_query_row(&mut self) -> AppCommand {
        let selected = self.frame.active_rows().get(self.selected_index()).cloned();
        let Some(PanelRow::Query(row)) = selected else {
            self.record_status(SeverityKind::Warning, "selected row is not a saved query");
            return AppCommand::Continue;
        };
        if !row.valid {
            self.record_status_with_detail(
                SeverityKind::Warning,
                format!("saved query @{} is invalid", row.id),
                row.error.clone(),
            );
            return AppCommand::Continue;
        }

        let query = format!("@{}", row.id);
        self.switch_panel(PanelId::BuiltIn(Panel::Search));
        self.frame.set_query(Some(query.clone()));
        self.frame.set_search(SearchPanel::empty(query));
        self.advance_snapshot_generation();
        self.sync_active_viewport();
        self.search_editing = false;
        self.search_history.cancel_recall();
        self.start_search_now(true);
        AppCommand::Continue
    }

    fn open_selected_source(&mut self) -> AppCommand {
        self.frame
            .selected_source_location(self.selected_index())
            .map(AppCommand::Open)
            .unwrap_or_else(|| {
                self.show_log(
                    "Open",
                    "selected row has no source location to open".to_owned(),
                );
                AppCommand::Continue
            })
    }

    fn handle_diagnostic_filter_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Esc => {
                self.overlay = DashboardOverlay::None;
                self.record_status(SeverityKind::Info, "diagnostic filter edit canceled");
            }
            KeyCode::Tab | KeyCode::Down => {
                self.with_diagnostic_filter_draft(DiagnosticFilterDraft::next_field);
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.with_diagnostic_filter_draft(DiagnosticFilterDraft::previous_field);
            }
            KeyCode::Enter => {
                if let DashboardOverlay::DiagnosticFilter(draft) = self.overlay.clone() {
                    self.overlay = DashboardOverlay::None;
                    self.frame.diagnostic_filters.code = draft.code.trim().to_owned();
                    self.frame.diagnostic_filters.path = draft.path.trim().to_owned();
                    self.sync_all_viewports();
                    self.record_filter_status();
                }
            }
            KeyCode::Backspace => {
                self.with_diagnostic_filter_draft(|draft| {
                    draft.active_value_mut().pop();
                });
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.with_diagnostic_filter_draft(|draft| draft.active_value_mut().clear());
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.with_diagnostic_filter_draft(|draft| {
                    draft.active_value_mut().push(character);
                });
            }
            _ => {}
        }
        AppCommand::Continue
    }

    fn open_capture_flow(&mut self) {
        match actions::capture_templates(self.store_options.corpus_root()) {
            Ok(templates) if templates.is_empty() => {
                self.show_log(
                    "Capture unavailable",
                    "capture failed: no #z/tmpl templates were found".to_owned(),
                );
            }
            Ok(templates) if templates.len() == 1 => match templates[0].draft() {
                Ok(draft) => {
                    self.overlay = DashboardOverlay::Capture(draft);
                    self.record_status(
                        SeverityKind::Info,
                        "capture edit: tab fields, enter creates, esc cancels",
                    );
                }
                Err(message) => {
                    self.show_log("Capture unavailable", message);
                }
            },
            Ok(templates) => {
                self.overlay =
                    DashboardOverlay::CapturePicker(CaptureTemplatePicker::new(templates));
                self.record_status(
                    SeverityKind::Info,
                    "capture template picker: enter selects, esc cancels",
                );
            }
            Err(message) => {
                self.show_log("Capture unavailable", message);
            }
        }
    }

    fn open_diagnostic_filter_edit(&mut self) {
        self.overlay = DashboardOverlay::DiagnosticFilter(DiagnosticFilterDraft::from_filters(
            &self.frame.diagnostic_filters,
        ));
        self.record_status(
            SeverityKind::Info,
            "diagnostic filter edit: tab fields, enter applies, esc cancels",
        );
    }

    fn open_postpone_prompt(&mut self) {
        let Some(row) = self.selected_zettel_todo_row("todo postpone") else {
            return;
        };
        let fields = postpone_field_options(&row);
        if fields.is_empty() {
            self.record_status(
                SeverityKind::Warning,
                "todo postpone unavailable: selected todo has no due or do date",
            );
            return;
        }
        self.overlay = DashboardOverlay::TodoPrompt(TodoPromptDraft::postpone(row, fields));
        self.record_status(
            SeverityKind::Info,
            "todo postpone: enter date or +1d/+1w, tab fields, enter applies",
        );
    }

    fn open_schedule_prompt(&mut self) {
        let Some(row) = self.selected_zettel_todo_row("todo schedule") else {
            return;
        };
        match row.todo_marker.as_deref() {
            Some("[ ]" | "[N]") => {}
            Some(_) => {
                self.record_status(
                    SeverityKind::Warning,
                    "todo schedule unavailable: selected todo is not open or next",
                );
                return;
            }
            None => {
                self.record_status(
                    SeverityKind::Warning,
                    "todo schedule unavailable: selected zettel has no todo marker",
                );
                return;
            }
        }
        self.overlay = DashboardOverlay::TodoPrompt(TodoPromptDraft::schedule(row));
        self.record_status(
            SeverityKind::Info,
            "todo schedule: enter date or +1d/+1w, enter applies, esc cancels",
        );
    }

    fn open_yank_overlay(&mut self) {
        let selected = self.frame.active_rows().get(self.selected_index()).cloned();
        let Some(row) = selected else {
            self.record_status(SeverityKind::Warning, "yank unavailable: no selected row");
            return;
        };

        self.overlay = DashboardOverlay::Yank(row.yank_overlay(&self.frame.root));
        self.record_status(
            SeverityKind::Info,
            "yank: choose row id, source link, or diagnostic message",
        );
    }

    fn selected_zettel_todo_row(&mut self, operation: &str) -> Option<crate::model::ZettelRow> {
        let selected = self.frame.active_rows().get(self.selected_index()).cloned();
        let Some(PanelRow::Zettel(row)) = selected else {
            self.record_status(
                SeverityKind::Warning,
                format!("{operation} unavailable: selected row is not a todo"),
            );
            return None;
        };
        if row.todo_marker.is_none() {
            self.record_status(
                SeverityKind::Warning,
                format!("{operation} unavailable: selected zettel has no todo marker"),
            );
            return None;
        }
        Some(row)
    }

    fn submit_todo_prompt(&mut self, mut draft: TodoPromptDraft) {
        let date = match actions::parse_todo_prompt_date(&draft.date_input) {
            Ok(date) => date,
            Err(message) => {
                draft.error = Some(message);
                self.overlay = DashboardOverlay::TodoPrompt(draft);
                return;
            }
        };

        let preview = match draft.action {
            TodoPromptAction::Postpone => {
                let Some(field) = draft.target_field else {
                    draft.error = Some("choose due or do before applying".to_owned());
                    self.overlay = DashboardOverlay::TodoPrompt(draft);
                    return;
                };
                actions::todo_postpone_preview(self.store_options.clone(), &draft.row, field, date)
            }
            TodoPromptAction::Schedule => {
                actions::todo_schedule_preview(self.store_options.clone(), &draft.row, date)
            }
        };

        match preview {
            Ok(plan) => {
                let title = draft.action.title();
                let row = draft.row;
                self.overlay = DashboardOverlay::None;
                self.start_todo_apply(TodoActionOverlay::new(title, row, plan));
            }
            Err(message) => {
                draft.error = Some(message);
                self.overlay = DashboardOverlay::TodoPrompt(draft);
            }
        }
    }

    fn cycle_diagnostic_severity_filter(&mut self) {
        self.frame.diagnostic_filters.severity = self.frame.diagnostic_filters.severity.next();
        self.sync_all_viewports();
        self.record_filter_status();
    }

    fn clear_diagnostic_filters(&mut self) {
        if !self.frame.diagnostic_filters.is_active() {
            self.record_status(SeverityKind::Info, "diagnostic filters already clear");
            return;
        }
        self.frame.diagnostic_filters.clear();
        self.sync_all_viewports();
        self.record_status(SeverityKind::Info, "diagnostic filters cleared");
    }

    fn cycle_today_mode(&mut self) {
        if self.frame.custom_panel.is_some() || self.frame.panel != Panel::Today {
            self.record_status(
                SeverityKind::Warning,
                "today mode unavailable: switch to Today",
            );
            return;
        }
        self.frame.today_mode = self.frame.today_mode.next();
        self.sync_active_viewport();
        let label = self.frame.today_mode.label();
        let visible = self
            .frame
            .today_counts()
            .map(|counts| counts.visible)
            .unwrap_or(0);
        self.record_status(
            SeverityKind::Info,
            format!("today mode: {label} ({visible} rows)"),
        );
    }

    fn record_filter_status(&mut self) {
        if self.frame.diagnostic_filters.is_active() {
            self.record_status(
                SeverityKind::Info,
                format!(
                    "diagnostic filters: {}",
                    self.frame.diagnostic_filters.active_labels().join(" ")
                ),
            );
        } else {
            self.record_status(SeverityKind::Info, "diagnostic filters cleared");
        }
    }

    fn toggle_selected_diagnostic_mark(&mut self) {
        let selected = self.frame.active_rows().get(self.selected_index()).cloned();
        let Some(PanelRow::Diagnostic(diagnostic)) = selected else {
            self.record_status(
                SeverityKind::Warning,
                "mark unavailable: selected row is not diagnostic",
            );
            return;
        };

        let marked = self.frame.toggle_diagnostic_mark(&diagnostic);
        let action = if marked { "marked" } else { "unmarked" };
        self.record_status(
            SeverityKind::Info,
            format!(
                "diagnostic {action}: {} marked",
                self.frame.marked_diagnostic_count()
            ),
        );
    }

    fn with_capture_draft(&mut self, update: impl FnOnce(&mut CaptureDraft)) {
        if let DashboardOverlay::Capture(draft) = &mut self.overlay {
            update(draft);
        }
    }

    fn with_capture_template_picker(&mut self, update: impl FnOnce(&mut CaptureTemplatePicker)) {
        if let DashboardOverlay::CapturePicker(picker) = &mut self.overlay {
            update(picker);
        }
    }

    fn with_diagnostic_filter_draft(&mut self, update: impl FnOnce(&mut DiagnosticFilterDraft)) {
        if let DashboardOverlay::DiagnosticFilter(draft) = &mut self.overlay {
            update(draft);
        }
    }

    fn with_todo_prompt_draft(&mut self, update: impl FnOnce(&mut TodoPromptDraft)) {
        if let DashboardOverlay::TodoPrompt(draft) = &mut self.overlay {
            update(draft);
        }
    }

    fn with_yank_overlay(&mut self, update: impl FnOnce(&mut YankOverlay)) {
        if let DashboardOverlay::Yank(overlay) = &mut self.overlay {
            update(overlay);
        }
    }

    fn yank_selected_overlay_value(&mut self) -> AppCommand {
        let command = match &self.overlay {
            DashboardOverlay::Yank(overlay) => overlay.selected_command(),
            _ => return AppCommand::Continue,
        };
        match command {
            Ok(command) => AppCommand::Yank(command),
            Err(message) => {
                self.record_status(SeverityKind::Warning, message);
                AppCommand::Continue
            }
        }
    }

    fn switch_panel(&mut self, panel: impl Into<PanelId>) {
        self.frame.set_active_panel_id(panel.into());
        self.ensure_viewports_for_registry();
        self.sync_active_viewport();
    }

    fn move_selection(&mut self, delta: isize) {
        let rows = self.frame.active_rows();
        self.active_viewport_mut().move_by(delta, &rows);
    }

    fn move_page(&mut self, direction: isize) {
        let rows = self.frame.active_rows();
        self.active_viewport_mut().move_page(direction, &rows);
    }

    fn move_half_page(&mut self, direction: isize) {
        let rows = self.frame.active_rows();
        self.active_viewport_mut().move_half_page(direction, &rows);
    }

    fn set_selection(&mut self, index: usize) {
        let rows = self.frame.active_rows();
        self.active_viewport_mut().set_selected_index(index, &rows);
    }

    fn sync_active_viewport(&mut self) {
        let rows = self.frame.active_rows();
        self.active_viewport_mut().sync_rows(&rows);
        self.frame.refresh_telemetry_row_counts();
    }

    fn sync_all_viewports(&mut self) {
        self.ensure_viewports_for_registry();
        for panel in self.frame.panels.clone() {
            let rows = self.frame.rows_for_panel_id(&panel.id);
            self.viewports
                .entry(panel.id)
                .or_insert_with(|| PanelViewport::new(DEFAULT_VISIBLE_ROW_COUNT))
                .sync_rows(&rows);
        }
        self.frame.refresh_telemetry_row_counts();
    }

    fn advance_snapshot_generation(&mut self) {
        self.snapshot_generation = self.snapshot_generation.saturating_add(1);
        self.frame.clear_graph_context();
        let current_generation = self.snapshot_generation;
        self.graph_cache
            .retain(|key, _| key.snapshot_generation == current_generation);
        self.pending_graph
            .retain(|key| key.snapshot_generation == current_generation);
    }

    fn active_viewport_mut(&mut self) -> &mut PanelViewport {
        let active = self.frame.active_panel_id();
        self.viewports
            .entry(active)
            .or_insert_with(|| PanelViewport::new(DEFAULT_VISIBLE_ROW_COUNT))
    }

    fn ensure_viewports_for_registry(&mut self) {
        for panel in &self.frame.panels {
            self.viewports
                .entry(panel.id.clone())
                .or_insert_with(|| PanelViewport::new(DEFAULT_VISIBLE_ROW_COUNT));
        }
    }

    fn active_row_count(&self) -> usize {
        self.frame.active_rows().len()
    }

    fn has_pending_data_operation(&self) -> bool {
        self.pending_initial_load.is_some()
            || self.pending_refresh.is_some()
            || self.pending_reindex.is_some()
            || self.pending_capture.is_some()
            || self.pending_fix_preview.is_some()
            || self.pending_fix_apply.is_some()
            || self.pending_todo_apply.is_some()
            || self.pending_search.is_some()
    }

    fn start_refresh(&mut self) {
        self.start_refresh_with_origin(RefreshOrigin::Manual);
    }

    fn start_refresh_with_origin(&mut self, origin: RefreshOrigin) {
        if self.pending_initial_load.is_some() {
            self.record_status(
                SeverityKind::Warning,
                "refresh unavailable: initial load running",
            );
            return;
        }
        if self.pending_refresh.is_some() {
            self.record_status(SeverityKind::Warning, "refresh already running");
            return;
        }
        let generation = self.next_generation();
        self.pending_refresh = Some(PendingOperation::new(
            generation,
            PendingOperationKind::Refresh,
        ));
        if origin == RefreshOrigin::Auto {
            self.pending_auto_refresh_generation = Some(generation);
            self.frame
                .record_auto_refresh_event(AutoRefreshEvent::Refreshing);
            self.record_status(SeverityKind::Info, "auto-refresh running");
        } else {
            self.record_status(SeverityKind::Info, "refresh running");
        }
        let options = self.store_options.clone();
        let query = self.frame.query.clone();
        let dashboard_id = self.frame.requested_dashboard_id.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let snapshot = actions::refresh_snapshot(options, query, dashboard_id);
            let _ = sender.send(AsyncResult::Refresh {
                generation,
                snapshot,
            });
        });
    }

    fn has_any_pending_operation(&self) -> bool {
        self.has_pending_data_operation()
            || self.pending_freshness_check.is_some()
            || !self.pending_graph.is_empty()
    }

    fn auto_refresh_blocker(&self) -> Option<AutoRefreshSkipReason> {
        if self.has_any_pending_operation() {
            return Some(AutoRefreshSkipReason::PendingOperation);
        }
        if self.search_editing
            || matches!(
                self.overlay,
                DashboardOverlay::Capture(_)
                    | DashboardOverlay::TodoPrompt(_)
                    | DashboardOverlay::DiagnosticFilter(_)
            )
        {
            return Some(AutoRefreshSkipReason::Editing);
        }
        if !matches!(self.overlay, DashboardOverlay::None) {
            return Some(AutoRefreshSkipReason::Overlay);
        }
        if self.frame.snapshot_generation().is_none() {
            return Some(AutoRefreshSkipReason::SnapshotUnavailable);
        }
        None
    }

    fn record_auto_refresh_skip(&mut self, reason: AutoRefreshSkipReason) {
        self.frame
            .record_auto_refresh_event(AutoRefreshEvent::Skipped(reason));
        if self.last_auto_refresh_skip != Some(reason) {
            self.last_auto_refresh_skip = Some(reason);
            self.record_status(
                SeverityKind::Info,
                format!("auto-refresh skipped: {}", reason.label()),
            );
        }
    }

    fn update_search_input(&mut self, query: String) {
        self.frame
            .set_query((!query.trim().is_empty()).then(|| query.clone()));
        self.frame.set_search(SearchPanel::empty(query.clone()));
        self.advance_snapshot_generation();
        self.sync_active_viewport();

        if query.trim().is_empty() {
            self.pending_search = None;
            self.search_due_at = None;
            self.record_status(SeverityKind::Info, "search cleared");
        } else {
            self.search_due_at = Some(Instant::now() + SEARCH_DEBOUNCE);
            self.record_status(SeverityKind::Info, "search pending");
        }
    }

    fn start_search_now(&mut self, commit_history: bool) {
        self.search_due_at = None;
        let query = self.current_query_input();
        self.frame
            .set_query((!query.trim().is_empty()).then(|| query.clone()));
        if query.trim().is_empty() {
            self.pending_search = None;
            self.frame.set_search(SearchPanel::empty(query));
            self.advance_snapshot_generation();
            self.sync_active_viewport();
            self.record_status(SeverityKind::Info, "search cleared");
            return;
        }
        if commit_history {
            self.search_history.commit(&query);
        }

        let generation = self.next_generation();
        self.pending_search = Some(PendingOperation::new(
            generation,
            PendingOperationKind::Search,
        ));
        self.record_status(SeverityKind::Info, "search running");
        let options = self.store_options.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = crate::data::load_search_panel(options, &query);
            let _ = sender.send(AsyncResult::Search { generation, result });
        });
    }

    fn current_query_input(&self) -> String {
        self.frame
            .search_panel()
            .map(|search| search.input.clone())
            .or_else(|| self.frame.query.clone())
            .unwrap_or_default()
    }

    fn begin_search_edit(&mut self) {
        self.switch_panel(PanelId::BuiltIn(Panel::Search));
        let original = self
            .frame
            .search_panel()
            .cloned()
            .unwrap_or_else(|| SearchPanel::empty(self.current_query_input()));
        self.search_editor.set_text(original.input.clone());
        self.search_edit_original = Some(original);
        self.search_history.cancel_recall();
        self.search_editing = true;
    }

    fn finish_search_edit(&mut self) {
        self.search_editing = false;
        self.search_edit_original = None;
        self.search_history.cancel_recall();
        self.frame.set_query(
            (!self.search_editor.text().trim().is_empty())
                .then(|| self.search_editor.text().to_owned()),
        );
        self.frame
            .set_search(SearchPanel::empty(self.search_editor.text().to_owned()));
        self.advance_snapshot_generation();
        self.sync_active_viewport();
    }

    fn cancel_search_edit(&mut self) {
        self.search_editing = false;
        self.search_history.cancel_recall();
        self.search_due_at = None;
        self.pending_search = None;
        if let Some(original) = self.search_edit_original.take() {
            self.frame
                .set_query((!original.input.trim().is_empty()).then(|| original.input.clone()));
            self.search_editor.set_text(original.input.clone());
            self.frame.set_search(original);
            self.advance_snapshot_generation();
            self.sync_active_viewport();
        }
        self.record_status(SeverityKind::Info, "search edit canceled");
    }

    fn apply_search_editor_change(&mut self) {
        self.update_search_input(self.search_editor.text().to_owned());
    }

    fn start_reindex(&mut self) {
        if self.pending_reindex.is_some() {
            self.record_status(SeverityKind::Warning, "reindex already running");
            return;
        }
        let generation = self.next_generation();
        self.pending_reindex = Some(PendingOperation::new(
            generation,
            PendingOperationKind::Reindex,
        ));
        self.record_status(SeverityKind::Info, "reindex running");
        let options = self.store_options.clone();
        let query = self.frame.query.clone();
        let dashboard_id = self.frame.requested_dashboard_id.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = actions::reindex(options, query, dashboard_id);
            let _ = sender.send(AsyncResult::Reindex { generation, result });
        });
    }

    fn start_capture(&mut self, draft: CaptureDraft) {
        if self.pending_capture.is_some() {
            self.record_status(SeverityKind::Warning, "capture already running");
            return;
        }
        let generation = self.next_generation();
        self.pending_capture = Some(PendingOperation::new(
            generation,
            PendingOperationKind::Capture,
        ));
        self.record_status(SeverityKind::Info, "capture running");
        let options = self.store_options.clone();
        let query = self.frame.query.clone();
        let dashboard_id = self.frame.requested_dashboard_id.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = actions::capture(options, query, dashboard_id, draft);
            let _ = sender.send(AsyncResult::Capture { generation, result });
        });
    }

    fn start_fix_preview(&mut self) {
        if self.pending_fix_preview.is_some() {
            self.record_status(SeverityKind::Warning, "fix preview already running");
            return;
        }

        let selected = self.frame.active_rows().get(self.selected_index()).cloned();
        let Some(PanelRow::Diagnostic(diagnostic)) = selected else {
            if self.frame.marked_diagnostic_count() > 0 {
                let summary = self.frame.marked_diagnostic_summaries(None);
                self.overlay = DashboardOverlay::FixPreview(marked_fix_preview_overlay(summary));
                self.record_status(
                    SeverityKind::Warning,
                    "fix preview shows marked diagnostics; bulk apply is unavailable",
                );
                return;
            }
            self.record_status(
                SeverityKind::Warning,
                "fix preview unavailable: selected row is not diagnostic",
            );
            return;
        };
        let marked_summary = self.frame.marked_diagnostic_summaries(Some(&diagnostic));

        let generation = self.next_generation();
        self.pending_fix_preview = Some(PendingOperation::new(
            generation,
            PendingOperationKind::FixPreview,
        ));
        self.record_status(SeverityKind::Info, "fix preview running");
        let options = self.store_options.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = actions::fix_preview(options, diagnostic);
            let _ = sender.send(AsyncResult::FixPreview {
                generation,
                result,
                marked_summary,
            });
        });
    }

    fn confirm_fix_apply(&mut self) {
        let DashboardOverlay::FixPreview(preview) = self.overlay.clone() else {
            return;
        };

        if !preview.can_apply_selected_fix() {
            self.record_status(
                SeverityKind::Warning,
                "fix apply unavailable: no safe preview",
            );
            return;
        }

        self.overlay = DashboardOverlay::ConfirmFixApply(preview);
    }

    fn confirm_mark_done(&mut self) {
        if self.frame.custom_panel.is_some() || self.frame.panel != Panel::Today {
            self.record_status(
                SeverityKind::Warning,
                "todo mark done unavailable: switch to Today",
            );
            return;
        }

        let selected = self.frame.active_rows().get(self.selected_index()).cloned();
        let Some(PanelRow::Zettel(row)) = selected else {
            self.record_status(
                SeverityKind::Warning,
                "todo mark done unavailable: selected row is not a todo",
            );
            return;
        };

        if row.todo_marker.is_none() {
            self.record_status(
                SeverityKind::Warning,
                "todo mark done unavailable: selected zettel has no todo marker",
            );
            return;
        }

        match actions::todo_mark_done_preview(self.store_options.clone(), &row) {
            Ok(plan) => {
                self.overlay = DashboardOverlay::ConfirmTodoApply(TodoActionOverlay::new(
                    "Confirm Mark Done",
                    row,
                    plan,
                ));
                self.record_status(SeverityKind::Info, "todo mark done preview ready");
            }
            Err(message) => {
                self.show_log("Todo mark done unavailable", message);
            }
        }
    }

    fn start_fix_apply(&mut self, preview: crate::model::FixPreviewOverlay) {
        if self.pending_fix_apply.is_some() {
            self.record_status(SeverityKind::Warning, "fix apply already running");
            return;
        }

        if !preview.can_apply_selected_fix() {
            self.record_status(
                SeverityKind::Warning,
                "fix apply unavailable: no safe preview",
            );
            return;
        }

        let generation = self.next_generation();
        self.pending_fix_apply = Some(PendingOperation::new(
            generation,
            PendingOperationKind::FixApply,
        ));
        self.record_status(SeverityKind::Info, "fix apply running");
        let options = self.store_options.clone();
        let query = self.frame.query.clone();
        let dashboard_id = self.frame.requested_dashboard_id.clone();
        let selector = preview.selector;
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = actions::fix_apply(options, query, dashboard_id, selector);
            let _ = sender.send(AsyncResult::FixApply { generation, result });
        });
    }

    fn start_todo_apply(&mut self, overlay: TodoActionOverlay) {
        if self.pending_todo_apply.is_some() {
            self.record_status(SeverityKind::Warning, "todo apply already running");
            return;
        }

        let generation = self.next_generation();
        self.pending_todo_apply = Some(PendingOperation::new(
            generation,
            PendingOperationKind::TodoApply,
        ));
        self.record_status(SeverityKind::Info, "todo apply running");
        let options = self.store_options.clone();
        let query = self.frame.query.clone();
        let dashboard_id = self.frame.requested_dashboard_id.clone();
        let plan = overlay.plan;
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = actions::todo_apply(options, query, dashboard_id, plan);
            let _ = sender.send(AsyncResult::TodoApply { generation, result });
        });
    }

    fn finish_pending(
        pending: &mut Option<PendingOperation>,
        generation: usize,
    ) -> Option<Duration> {
        if pending.as_ref().map(|pending| pending.generation) != Some(generation) {
            return None;
        }
        pending.take().map(PendingOperation::elapsed)
    }

    fn next_generation(&mut self) -> usize {
        self.generation = self.generation.saturating_add(1);
        self.generation
    }

    fn apply_async_result(&mut self, result: AsyncResult) {
        match result {
            AsyncResult::InitialLoad {
                generation,
                snapshot,
            } => {
                let Some(elapsed) =
                    Self::finish_pending(&mut self.pending_initial_load, generation)
                else {
                    return;
                };
                let before = self.frame.snapshot.metrics();
                let after = snapshot.metrics();
                let message = match &snapshot {
                    DashboardSnapshot::Ready { .. } => "initial load complete",
                    DashboardSnapshot::Degraded { .. } => "initial load degraded",
                    DashboardSnapshot::Loading => "initial load incomplete",
                };
                let severity = match &snapshot {
                    DashboardSnapshot::Ready { .. } => SeverityKind::Info,
                    DashboardSnapshot::Degraded { .. } | DashboardSnapshot::Loading => {
                        SeverityKind::Warning
                    }
                };
                let degraded_message = match &snapshot {
                    DashboardSnapshot::Degraded { message } => Some(message.clone()),
                    DashboardSnapshot::Loading | DashboardSnapshot::Ready { .. } => None,
                };
                self.frame.set_snapshot(snapshot);
                self.advance_snapshot_generation();
                self.sync_all_viewports();
                self.frame.record_initial_load_duration(elapsed);
                let detail = match degraded_message {
                    Some(message) => {
                        format!(
                            "{}\n{message}",
                            snapshot_change_detail(elapsed, before, after)
                        )
                    }
                    None => snapshot_change_detail(elapsed, before, after),
                };
                self.record_status_with_detail(severity, message, Some(detail));
            }
            AsyncResult::Refresh {
                generation,
                snapshot,
            } => {
                let Some(elapsed) = Self::finish_pending(&mut self.pending_refresh, generation)
                else {
                    return;
                };
                let was_auto_refresh = self.pending_auto_refresh_generation == Some(generation);
                if was_auto_refresh {
                    self.pending_auto_refresh_generation = None;
                    self.frame
                        .record_auto_refresh_event(AutoRefreshEvent::Refreshed);
                }
                let before = self.frame.snapshot.metrics();
                let after = snapshot.metrics();
                self.frame.set_snapshot(snapshot);
                self.advance_snapshot_generation();
                self.sync_all_viewports();
                self.frame.record_refresh_duration(elapsed);
                let message = if was_auto_refresh {
                    "auto-refresh complete"
                } else {
                    "refresh complete"
                };
                self.record_status_with_detail(
                    SeverityKind::Info,
                    message,
                    Some(snapshot_change_detail(elapsed, before, after)),
                );
            }
            AsyncResult::Reindex { generation, result } => {
                let Some(elapsed) = Self::finish_pending(&mut self.pending_reindex, generation)
                else {
                    return;
                };
                match result {
                    Ok(outcome) => {
                        let before = self.frame.snapshot.metrics();
                        let after = outcome.snapshot.metrics();
                        self.frame.set_snapshot(outcome.snapshot);
                        self.advance_snapshot_generation();
                        self.sync_all_viewports();
                        self.frame
                            .record_action_duration(PendingOperationKind::Reindex, elapsed);
                        self.record_status_with_detail(
                            SeverityKind::Info,
                            actions::reindex_summary_line(outcome.summary),
                            Some(snapshot_change_detail(elapsed, before, after)),
                        );
                    }
                    Err(message) => {
                        self.frame
                            .record_action_duration(PendingOperationKind::Reindex, elapsed);
                        self.show_log_with_elapsed("Reindex failed", elapsed, message);
                    }
                }
            }
            AsyncResult::Capture { generation, result } => {
                let Some(elapsed) = Self::finish_pending(&mut self.pending_capture, generation)
                else {
                    return;
                };
                match result {
                    Ok(outcome) => {
                        let template_label = outcome.template_label.clone();
                        let before = self.frame.snapshot.metrics();
                        let after = outcome.snapshot.metrics();
                        self.frame.set_snapshot(outcome.snapshot);
                        self.advance_snapshot_generation();
                        self.sync_all_viewports();
                        self.frame
                            .record_action_duration(PendingOperationKind::Capture, elapsed);
                        let id = outcome.result.zettel_id.declaration();
                        let destination = outcome.result.destination.display().to_string();
                        self.show_log_with_severity(
                            SeverityKind::Info,
                            "Capture complete",
                            format!(
                                "template: {template_label}\n{}\ndestination: {destination}\nzettel_id: {id}",
                                snapshot_change_detail(elapsed, before, after)
                            ),
                        );
                    }
                    Err(message) => {
                        self.frame
                            .record_action_duration(PendingOperationKind::Capture, elapsed);
                        self.show_log_with_elapsed("Capture failed", elapsed, message);
                    }
                }
            }
            AsyncResult::FixPreview {
                generation,
                result,
                marked_summary,
            } => {
                let Some(elapsed) = Self::finish_pending(&mut self.pending_fix_preview, generation)
                else {
                    return;
                };
                match result {
                    Ok(mut preview) => {
                        self.frame
                            .record_action_duration(PendingOperationKind::FixPreview, elapsed);
                        if !marked_summary.is_empty() {
                            preview.marked_summary = Some(marked_summary);
                        }
                        let preview_count = preview.previews.len();
                        let has_preview = preview_count > 0;
                        self.overlay = DashboardOverlay::FixPreview(preview);
                        if has_preview {
                            self.record_status_with_detail(
                                SeverityKind::Info,
                                format!("fix preview ready: {preview_count} safe preview(s)"),
                                Some(elapsed_detail(elapsed)),
                            );
                        } else {
                            self.record_status_with_detail(
                                SeverityKind::Warning,
                                "fix preview unavailable",
                                Some(elapsed_detail(elapsed)),
                            );
                        }
                    }
                    Err(message) => {
                        self.frame
                            .record_action_duration(PendingOperationKind::FixPreview, elapsed);
                        self.show_log_with_elapsed("Fix preview failed", elapsed, message);
                    }
                }
            }
            AsyncResult::FixApply { generation, result } => {
                let Some(elapsed) = Self::finish_pending(&mut self.pending_fix_apply, generation)
                else {
                    return;
                };
                match result {
                    Ok(outcome) => {
                        let before = self.frame.snapshot.metrics();
                        let after = outcome.snapshot.metrics();
                        let detail = format_fix_apply_detail(&outcome, elapsed, before, after);
                        self.frame.set_snapshot(outcome.snapshot);
                        self.advance_snapshot_generation();
                        self.sync_all_viewports();
                        self.frame
                            .record_action_duration(PendingOperationKind::FixApply, elapsed);
                        self.record_status_with_detail(
                            SeverityKind::Info,
                            "fix apply complete",
                            Some(detail),
                        );
                    }
                    Err(message) => {
                        self.frame
                            .record_action_duration(PendingOperationKind::FixApply, elapsed);
                        self.show_log_with_elapsed("Fix apply failed", elapsed, message);
                    }
                }
            }
            AsyncResult::TodoApply { generation, result } => {
                let Some(elapsed) = Self::finish_pending(&mut self.pending_todo_apply, generation)
                else {
                    return;
                };
                match result {
                    Ok(outcome) => {
                        let before = self.frame.snapshot.metrics();
                        let after = outcome.snapshot.metrics();
                        let detail = format_todo_apply_detail(&outcome, elapsed, before, after);
                        self.frame.set_snapshot(outcome.snapshot);
                        self.advance_snapshot_generation();
                        self.sync_all_viewports();
                        self.frame
                            .record_action_duration(PendingOperationKind::TodoApply, elapsed);
                        self.record_status_with_detail(
                            SeverityKind::Info,
                            "todo apply complete",
                            Some(detail),
                        );
                    }
                    Err(message) => {
                        self.frame
                            .record_action_duration(PendingOperationKind::TodoApply, elapsed);
                        self.show_log_with_elapsed("Todo apply failed", elapsed, message);
                    }
                }
            }
            AsyncResult::GraphContext { key, result } => {
                self.pending_graph.remove(&key);
                self.graph_cache.insert(key.clone(), result.clone());
                let current_row_id = self
                    .frame
                    .selected_zettel_row(self.selected_index())
                    .map(|row| row.row_id());
                if key.snapshot_generation == self.snapshot_generation
                    && current_row_id.as_ref() == Some(&key.row_id)
                {
                    self.frame.set_graph_context(key.row_id, result.clone());
                    match result {
                        GraphLoadState::Ready(_) => {
                            self.record_status(SeverityKind::Info, "graph context ready");
                        }
                        GraphLoadState::Failed { message } => {
                            self.record_status_with_detail(
                                SeverityKind::Warning,
                                "graph context unavailable",
                                Some(message),
                            );
                        }
                        GraphLoadState::Unavailable => {
                            self.record_status(SeverityKind::Warning, "graph context unavailable");
                        }
                        GraphLoadState::Loading => {}
                    }
                }
            }
            AsyncResult::Search { generation, result } => {
                let Some(elapsed) = Self::finish_pending(&mut self.pending_search, generation)
                else {
                    return;
                };
                match result {
                    Ok(search) => {
                        let row_count = search.rows.len();
                        let has_error = search.error.is_some();
                        self.frame.set_query(
                            (!search.input.trim().is_empty()).then(|| search.input.clone()),
                        );
                        self.frame.set_search(search);
                        self.advance_snapshot_generation();
                        self.sync_active_viewport();
                        self.frame.record_search_duration(elapsed);
                        if has_error {
                            self.record_status_with_detail(
                                SeverityKind::Error,
                                "search error",
                                Some(elapsed_detail(elapsed)),
                            );
                        } else {
                            self.record_status_with_detail(
                                SeverityKind::Info,
                                format!("search complete: {row_count} rows"),
                                Some(elapsed_detail(elapsed)),
                            );
                        }
                    }
                    Err(message) => {
                        self.frame.record_search_duration(elapsed);
                        self.show_log_with_elapsed("Search failed", elapsed, message);
                    }
                }
            }
            AsyncResult::FreshnessCheck {
                generation,
                freshness,
            } => {
                let Some(_elapsed) =
                    Self::finish_pending(&mut self.pending_freshness_check, generation)
                else {
                    return;
                };
                let previous = self.frame.snapshot_freshness.clone();
                self.frame.set_snapshot_freshness(freshness.clone());
                self.record_freshness_transition(&previous, &freshness);
            }
        }
    }

    fn record_freshness_transition(
        &mut self,
        previous: &SnapshotFreshness,
        freshness: &SnapshotFreshness,
    ) {
        if previous == freshness {
            return;
        }

        match freshness {
            SnapshotFreshness::Unknown => {}
            SnapshotFreshness::Current { .. } => {
                if !matches!(
                    previous,
                    SnapshotFreshness::Unknown | SnapshotFreshness::Current { .. }
                ) {
                    self.record_status_with_detail(
                        SeverityKind::Info,
                        "snapshot freshness current",
                        freshness.status_detail(),
                    );
                }
            }
            SnapshotFreshness::NewerIndexAvailable { .. }
            | SnapshotFreshness::StaleSources { .. }
            | SnapshotFreshness::CheckFailed { .. } => {
                let severity = match freshness {
                    SnapshotFreshness::CheckFailed { .. } => SeverityKind::Warning,
                    _ => SeverityKind::Info,
                };
                if let Some(message) = freshness.status_message() {
                    self.record_status_with_detail(severity, message, freshness.status_detail());
                }
            }
        }
    }

    fn show_log(&mut self, title: &str, message: String) {
        self.show_log_with_severity(SeverityKind::Error, title, message);
    }

    fn show_log_with_elapsed(&mut self, title: &str, elapsed: Duration, message: String) {
        self.show_log_with_severity(
            SeverityKind::Error,
            title,
            format!("{}\n{message}", elapsed_detail(elapsed)),
        );
    }

    fn show_log_with_severity(&mut self, severity: SeverityKind, title: &str, message: String) {
        self.record_status_with_detail(severity, title, Some(message.clone()));
        self.overlay = DashboardOverlay::Log {
            title: title.to_owned(),
            message,
        };
    }

    fn record_status(&mut self, severity: SeverityKind, message: impl Into<String>) {
        self.record_status_with_detail(severity, message, None);
    }

    fn record_status_with_detail(
        &mut self,
        severity: SeverityKind,
        message: impl Into<String>,
        detail: Option<String>,
    ) {
        let event = StatusEvent::new(self.next_status_order, severity, message, detail);
        self.next_status_order = self.next_status_order.saturating_add(1);
        self.status_events.push(event);
        if self.status_events.len() > STATUS_EVENT_LIMIT {
            self.status_events.remove(0);
        }
    }
}

fn marked_fix_preview_overlay(
    marked_summary: MarkedDiagnosticsSummary,
) -> crate::model::FixPreviewOverlay {
    crate::model::FixPreviewOverlay {
        diagnostic: DiagnosticPreviewContext {
            severity: "info".to_owned(),
            code: "marked.diagnostics".to_owned(),
            message: "Marked diagnostics are queued for review.".to_owned(),
            path: "-".to_owned(),
            position: "-".to_owned(),
        },
        previews: Vec::new(),
        unavailable_reason: Some(
            "Bulk apply from marked diagnostics is not available; preview and apply one selected safe fix at a time."
                .to_owned(),
        ),
        selector: Default::default(),
        marked_summary: Some(marked_summary),
    }
}

fn format_fix_apply_detail(
    outcome: &FixApplyOutcome,
    elapsed: Duration,
    before: crate::model::DashboardSnapshotMetrics,
    after: crate::model::DashboardSnapshotMetrics,
) -> String {
    let rule_codes = if outcome.applied_rule_codes.is_empty() {
        "-".to_owned()
    } else {
        outcome.applied_rule_codes.join(", ")
    };
    format!(
        "{}\npath: {}\nrule_codes: {rule_codes}\napplied_edits: {}\n{}",
        snapshot_change_detail(elapsed, before, after),
        outcome.changed_path.display(),
        outcome.applied_edits,
        actions::reindex_summary_line(outcome.reindex_summary.clone())
    )
}

fn format_todo_apply_detail(
    outcome: &TodoApplyOutcome,
    elapsed: Duration,
    before: crate::model::DashboardSnapshotMetrics,
    after: crate::model::DashboardSnapshotMetrics,
) -> String {
    let fields = if outcome.planner.changed_fields.is_empty() {
        "-".to_owned()
    } else {
        outcome.planner.changed_fields.join(", ")
    };
    let target = outcome
        .planner
        .canonical_id
        .as_deref()
        .map(|id| format!("@{id}"))
        .unwrap_or_else(|| {
            outcome
                .planner
                .zettel_store_id
                .map(|id| format!("store:{id}"))
                .unwrap_or_else(|| "-".to_owned())
        });
    format!(
        "{}\npath: {}\ntarget: {target}\nchanged_fields: {fields}\n{}",
        snapshot_change_detail(elapsed, before, after),
        outcome.planner.changed_path.display(),
        actions::reindex_summary_line(outcome.reindex_summary.clone())
    )
}

fn snapshot_change_detail(
    elapsed: Duration,
    before: crate::model::DashboardSnapshotMetrics,
    after: crate::model::DashboardSnapshotMetrics,
) -> String {
    format!(
        "{}\nrow_deltas: {}",
        elapsed_detail(elapsed),
        row_deltas(before, after)
    )
}

fn elapsed_detail(elapsed: Duration) -> String {
    format!("elapsed: {}", format_duration(elapsed))
}

fn row_deltas(
    before: crate::model::DashboardSnapshotMetrics,
    after: crate::model::DashboardSnapshotMetrics,
) -> String {
    [
        delta_label("today", before.today_rows, after.today_rows),
        delta_label("inbox", before.inbox_rows, after.inbox_rows),
        delta_label("search", before.search_rows, after.search_rows),
        delta_label("diagnostics", before.diagnostic_rows, after.diagnostic_rows),
        delta_label(
            "index_diagnostics",
            before.index_diagnostics,
            after.index_diagnostics,
        ),
    ]
    .join(" ")
}

fn delta_label(label: &str, before: usize, after: usize) -> String {
    let delta = after as isize - before as isize;
    if delta > 0 {
        format!("{label} +{delta}")
    } else {
        format!("{label} {delta}")
    }
}

fn postpone_field_options(row: &crate::model::ZettelRow) -> Vec<TodoDateField> {
    let has_due = row.properties.iter().any(|(key, _)| key == "due")
        || row.badges.iter().any(|badge| badge.label == "due");
    let has_do = row.properties.iter().any(|(key, _)| key == "do")
        || row.badges.iter().any(|badge| badge.label == "do");
    let mut fields = Vec::new();
    if has_due {
        fields.push(TodoDateField::Due);
    }
    if has_do {
        fields.push(TodoDateField::Do);
    }
    fields
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum AsyncResult {
    InitialLoad {
        generation: usize,
        snapshot: DashboardSnapshot,
    },
    Refresh {
        generation: usize,
        snapshot: DashboardSnapshot,
    },
    Reindex {
        generation: usize,
        result: Result<ReindexOutcome, String>,
    },
    Capture {
        generation: usize,
        result: Result<CaptureOutcome, String>,
    },
    FixPreview {
        generation: usize,
        result: Result<crate::model::FixPreviewOverlay, String>,
        marked_summary: MarkedDiagnosticsSummary,
    },
    FixApply {
        generation: usize,
        result: Result<FixApplyOutcome, String>,
    },
    TodoApply {
        generation: usize,
        result: Result<TodoApplyOutcome, String>,
    },
    GraphContext {
        key: GraphCacheKey,
        result: GraphLoadState,
    },
    Search {
        generation: usize,
        result: Result<SearchPanel, String>,
    },
    FreshnessCheck {
        generation: usize,
        freshness: SnapshotFreshness,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;
    use zorg_core::SourceSpan;
    use zorg_store::{Store, StoreOptions};

    use crate::model::{
        AutoRefreshEvent, CaptureField, CaptureTemplateRow, DashboardSnapshot, DiagnosticRow,
        IndexPanel, IndexStatusRow, PanelRow, QueryBadge, QueryPanel, QueryRow, SearchPanel,
        SnapshotFreshness, ZettelRow,
    };

    #[test]
    fn navigation_clamps_selection_and_switches_panels() {
        let mut app = test_app(Panel::Today);

        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected_index(), 1);

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.frame().panel, Panel::Inbox);
        assert_eq!(app.selected_index(), 0);

        app.handle_key(key(KeyCode::BackTab));
        assert_eq!(app.frame().panel, Panel::Today);
        assert_eq!(app.selected_index(), 1);

        app.handle_key(key(KeyCode::Char('g')));
        assert_eq!(app.selected_index(), 0);
        app.handle_key(key(KeyCode::Char('G')));
        assert_eq!(app.selected_index(), 1);
    }

    #[test]
    fn reindex_requires_confirmation() {
        let mut app = test_app(Panel::Index);
        assert!(!app.overlay().is_confirming_reindex());

        app.handle_key(key(KeyCode::Char('R')));
        assert!(app.overlay().is_confirming_reindex());

        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(app.overlay(), &DashboardOverlay::None);
        assert_eq!(app.status(), "reindex canceled");
    }

    #[test]
    fn refresh_completion_updates_snapshot_and_status() {
        let mut app = test_app(Panel::Index);
        app.pending_refresh = Some(pending(3, PendingOperationKind::Refresh));
        app.apply_async_result(AsyncResult::Refresh {
            generation: 3,
            snapshot: DashboardSnapshot::Degraded {
                message: "missing db".to_owned(),
            },
        });

        assert!(matches!(
            app.frame().snapshot,
            DashboardSnapshot::Degraded { .. }
        ));
        assert_eq!(app.status(), "refresh complete");
        assert!(
            app.latest_status_event()
                .and_then(|event| event.detail.as_ref())
                .is_some_and(|detail| {
                    detail.contains("elapsed:") && detail.contains("row_deltas:")
                })
        );
        assert_eq!(app.frame().telemetry.refresh_count, 1);
        assert!(app.frame().telemetry.last_refresh.is_some());
        assert_eq!(app.frame().telemetry.row_counts.today, 0);
    }

    #[test]
    fn initial_load_completion_replaces_loading_snapshot_and_reports_elapsed() {
        let mut app = test_app_with_snapshot(Panel::Today, DashboardSnapshot::Loading);
        app.pending_initial_load = Some(pending(2, PendingOperationKind::InitialLoad));

        app.apply_async_result(AsyncResult::InitialLoad {
            generation: 2,
            snapshot: ready_snapshot(
                vec![PanelRow::Zettel(zettel(7, "loaded"))],
                Vec::new(),
                vec![IndexStatusRow::new("Discovered files", 1)],
            ),
        });

        assert!(app.frame().is_ready());
        assert_eq!(app.status(), "initial load complete");
        assert_eq!(app.frame().active_rows().len(), 1);
        assert!(
            app.latest_status_event()
                .and_then(|event| event.detail.as_ref())
                .is_some_and(|detail| detail.contains("elapsed:"))
        );
        assert!(app.frame().telemetry.last_initial_load.is_some());
        assert_eq!(app.frame().telemetry.row_counts.today, 1);
    }

    #[test]
    fn freshness_check_is_scheduled_only_for_ready_snapshots() {
        let mut app = test_app(Panel::Index);
        app.next_freshness_check_at = Some(Instant::now() - Duration::from_millis(1));

        app.drive_freshness_check();

        assert!(app.pending_freshness_check.is_some());
        assert_eq!(
            app.pending_freshness_check.map(|pending| pending.kind),
            Some(PendingOperationKind::FreshnessCheck)
        );
    }

    #[test]
    fn freshness_check_updates_frame_and_records_newer_index_hint() {
        let mut app = test_app(Panel::Index);
        app.pending_freshness_check = Some(pending(4, PendingOperationKind::FreshnessCheck));
        let captured = app.frame().snapshot_generation().expect("ready generation");
        let current = crate::model::IndexGeneration::new(
            captured.schema_version,
            captured
                .last_indexed_at_unix_ms
                .map(|timestamp| timestamp + 1),
            captured.indexed_files,
            captured.diagnostic_count,
        );

        app.apply_async_result(AsyncResult::FreshnessCheck {
            generation: 4,
            freshness: SnapshotFreshness::NewerIndexAvailable { captured, current },
        });

        assert!(matches!(
            app.frame().snapshot_freshness,
            SnapshotFreshness::NewerIndexAvailable { .. }
        ));
        assert_eq!(app.status(), "newer index available");
    }

    #[test]
    fn auto_refresh_waits_until_due_when_freshness_is_current() {
        let mut app = test_app(Panel::Index);
        enable_auto_refresh(&mut app, Duration::from_millis(1_000));
        app.next_auto_refresh_at = Some(Instant::now() + Duration::from_secs(30));

        app.drive_auto_refresh();

        assert!(app.pending_refresh.is_none());
        assert_eq!(app.status(), "");
    }

    #[test]
    fn auto_refresh_skips_while_editing() {
        let mut app = test_app(Panel::Index);
        enable_auto_refresh(&mut app, Duration::from_millis(1_000));
        app.next_auto_refresh_at = Some(Instant::now() - Duration::from_millis(1));
        app.search_editing = true;

        app.drive_auto_refresh();

        assert!(app.pending_refresh.is_none());
        assert_eq!(app.status(), "auto-refresh skipped: editing");
        assert_eq!(
            app.frame().auto_refresh.last_event,
            AutoRefreshEvent::Skipped(AutoRefreshSkipReason::Editing)
        );
    }

    #[test]
    fn auto_refresh_skips_while_operation_is_pending() {
        let mut app = test_app(Panel::Index);
        enable_auto_refresh(&mut app, Duration::from_millis(1_000));
        app.next_auto_refresh_at = Some(Instant::now() - Duration::from_millis(1));
        app.pending_search = Some(pending(12, PendingOperationKind::Search));

        app.drive_auto_refresh();

        assert!(app.pending_refresh.is_none());
        assert_eq!(app.status(), "auto-refresh skipped: pending operation");
        assert_eq!(
            app.frame().auto_refresh.last_event,
            AutoRefreshEvent::Skipped(AutoRefreshSkipReason::PendingOperation)
        );
    }

    #[test]
    fn auto_refresh_skips_while_graph_load_is_pending() {
        let mut app = test_app(Panel::Index);
        enable_auto_refresh(&mut app, Duration::from_millis(1_000));
        app.next_auto_refresh_at = Some(Instant::now() - Duration::from_millis(1));
        app.pending_graph.insert(GraphCacheKey {
            snapshot_generation: 1,
            row_id: PanelRowId::ZettelCanonical("graph".to_owned()),
        });

        app.drive_auto_refresh();

        assert!(app.pending_refresh.is_none());
        assert_eq!(app.status(), "auto-refresh skipped: pending operation");
    }

    #[test]
    fn auto_refresh_schedules_refresh_when_due() {
        let mut app = test_app(Panel::Index);
        enable_auto_refresh(&mut app, Duration::from_millis(1_000));
        app.next_auto_refresh_at = Some(Instant::now() - Duration::from_millis(1));

        app.drive_auto_refresh();

        assert_eq!(
            app.pending_refresh.map(|pending| pending.kind),
            Some(PendingOperationKind::Refresh)
        );
        assert!(app.pending_auto_refresh_generation.is_some());
        assert_eq!(app.status(), "auto-refresh running");
        assert_eq!(
            app.frame().auto_refresh.last_event,
            AutoRefreshEvent::Refreshing
        );
    }

    #[test]
    fn auto_refresh_schedules_refresh_for_newer_index_before_due_interval() {
        let mut app = test_app(Panel::Index);
        enable_auto_refresh(&mut app, Duration::from_millis(1_000));
        app.next_auto_refresh_at = Some(Instant::now() + Duration::from_secs(30));
        let captured = app.frame().snapshot_generation().expect("ready generation");
        let current = crate::model::IndexGeneration::new(
            captured.schema_version,
            captured
                .last_indexed_at_unix_ms
                .map(|timestamp| timestamp + 1),
            captured.indexed_files,
            captured.diagnostic_count,
        );
        app.frame
            .set_snapshot_freshness(SnapshotFreshness::NewerIndexAvailable { captured, current });

        app.drive_auto_refresh();

        assert!(app.pending_refresh.is_some());
        assert_eq!(app.status(), "auto-refresh running");
    }

    #[test]
    fn failed_actions_record_duration_without_changing_row_counts() {
        let mut app = test_app(Panel::Diagnostics);
        let before = app.frame().telemetry.row_counts;
        app.pending_fix_apply = Some(pending(23, PendingOperationKind::FixApply));

        app.apply_async_result(AsyncResult::FixApply {
            generation: 23,
            result: Err("fix apply refused: index is stale relative to source".to_owned()),
        });

        assert_eq!(app.frame().telemetry.row_counts, before);
        assert_eq!(
            app.frame().telemetry.last_action.map(|action| action.kind),
            Some(PendingOperationKind::FixApply)
        );
        assert!(matches!(app.overlay(), DashboardOverlay::Log { .. }));
    }

    #[test]
    fn loading_state_allows_help_and_quit_but_blocks_data_actions() {
        let mut app = test_app_with_snapshot(Panel::Today, DashboardSnapshot::Loading);

        app.handle_key(key(KeyCode::Char('?')));
        assert!(matches!(app.overlay(), DashboardOverlay::Help));
        app.handle_key(key(KeyCode::Esc));

        app.handle_key(key(KeyCode::Char('y')));
        assert_eq!(
            app.status(),
            "dashboard still loading: data action unavailable"
        );
        assert_eq!(app.handle_key(key(KeyCode::Char('q'))), AppCommand::Quit);
    }

    #[test]
    fn pending_activity_is_render_facing_and_duplicate_refresh_is_polite() {
        let mut app = test_app(Panel::Today);
        app.pending_refresh = Some(pending(3, PendingOperationKind::Refresh));

        let activity = app.pending_activity().expect("pending activity");
        assert_eq!(activity.operation, PendingOperationKind::Refresh);
        assert_eq!(activity.spinner(), "|");

        app.advance_activity_tick();
        assert_eq!(
            app.pending_activity().expect("pending activity").spinner(),
            "/"
        );

        app.handle_key(key(KeyCode::Char('r')));
        assert_eq!(app.status(), "refresh already running");
    }

    #[test]
    fn refresh_key_is_guarded_while_initial_load_is_pending() {
        let mut app = test_app(Panel::Today);
        app.pending_initial_load = Some(pending(3, PendingOperationKind::InitialLoad));

        app.handle_key(key(KeyCode::Char('r')));

        assert!(app.pending_refresh.is_none());
        assert_eq!(app.status(), "refresh unavailable: initial load running");
    }

    #[test]
    fn initial_load_is_reported_as_pending_activity() {
        let mut app = test_app_with_snapshot(Panel::Today, DashboardSnapshot::Loading);
        app.pending_initial_load = Some(pending(3, PendingOperationKind::InitialLoad));

        let activity = app.pending_activity().expect("pending activity");
        assert_eq!(activity.operation, PendingOperationKind::InitialLoad);
        assert_eq!(activity.spinner(), "|");
    }

    #[test]
    fn failed_open_action_shows_log_overlay() {
        let mut app = test_app(Panel::Today);
        app.record_open_result(Err("open failed: $EDITOR is not set".to_owned()));

        assert_eq!(app.status(), "Open failed");
        assert!(matches!(app.overlay(), DashboardOverlay::Log { .. }));
    }

    #[test]
    fn capture_overlay_edits_fields_and_can_cancel() {
        let mut app = test_app(Panel::Today);
        app.overlay = DashboardOverlay::Capture(CaptureDraft::new("@tmpl/todo", None));

        app.handle_key(key(KeyCode::Tab));
        assert!(matches!(
            app.overlay(),
            DashboardOverlay::Capture(CaptureDraft {
                active: CaptureField::Title,
                ..
            })
        ));

        for character in "New task".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }

        let DashboardOverlay::Capture(draft) = app.overlay() else {
            panic!("capture overlay should remain open");
        };
        assert_eq!(draft.title, "New task");

        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.overlay(), &DashboardOverlay::None);
        assert_eq!(app.status(), "capture canceled");
    }

    #[test]
    fn capture_open_shows_log_when_no_templates_exist() {
        let temp = temp_path("dash-capture-empty");
        let root = temp.join("corpus");
        fs::create_dir_all(&root).expect("create root");
        let mut app = test_app_with_root(Panel::Today, root, temp.join("zorg.sqlite3"));

        app.open_capture_flow();

        let DashboardOverlay::Log { title, message } = app.overlay() else {
            panic!("missing capture unavailable log");
        };
        assert_eq!(title, "Capture unavailable");
        assert!(message.contains("no #z/tmpl templates were found"));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn capture_open_skips_picker_when_one_template_exists() {
        let temp = temp_path("dash-capture-one");
        let root = temp.join("corpus");
        fs::create_dir_all(&root).expect("create root");
        write_capture_templates(
            &root,
            "\
%%% @system #z/ref
System
%%%

- @system/templates/todo #z/tmpl title::Todo capture dest::inbox.z
  ```zorg-template
  - @{{id}} #z/todo [ ] {{title}}
  ```
",
        );
        let mut app = test_app_with_root(Panel::Today, root, temp.join("zorg.sqlite3"));

        app.open_capture_flow();

        let DashboardOverlay::Capture(draft) = app.overlay() else {
            panic!("one selectable template should open capture form");
        };
        assert_eq!(draft.template, "@system/templates/todo");
        assert_eq!(draft.template_title.as_deref(), Some("Todo"));
        assert_eq!(draft.template_variables, vec!["id", "title"]);
        assert_eq!(
            draft.editable_fields(),
            vec![
                CaptureField::Template,
                CaptureField::Title,
                CaptureField::Destination,
            ]
        );
        assert_eq!(draft.destination, "inbox.z");
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn capture_open_uses_picker_for_multiple_templates() {
        let temp = temp_path("dash-capture-multiple");
        let root = temp.join("corpus");
        fs::create_dir_all(&root).expect("create root");
        write_capture_templates(
            &root,
            "\
%%% @system #z/ref
System
%%%

- @system/templates/project #z/tmpl title::Project note dest::projects
  ```zorg-template
  - @{{id}} #z/ref {{title}}
  ```

- @system/templates/todo #z/tmpl title::Todo capture dest::inbox.z
  ```zorg-template
  - @{{id}} #z/todo [ ] {{title}}
  ```
",
        );
        let mut app = test_app_with_root(Panel::Today, root, temp.join("zorg.sqlite3"));

        app.open_capture_flow();

        let DashboardOverlay::CapturePicker(picker) = app.overlay() else {
            panic!("multiple templates should open picker");
        };
        assert_eq!(picker.rows.len(), 2);
        assert_eq!(picker.selected, 0);
        assert_eq!(
            app.status(),
            "capture template picker: enter selects, esc cancels"
        );
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn capture_picker_navigates_cancels_and_selects() {
        let mut app = test_app(Panel::Today);
        app.overlay = DashboardOverlay::CapturePicker(CaptureTemplatePicker::new(vec![
            CaptureTemplateRow {
                selector: Some("@tmpl/project".to_owned()),
                id: Some("@tmpl/project".to_owned()),
                title: Some("Project".to_owned()),
                destination: Some("projects".to_owned()),
                path: Some(PathBuf::from("templates.z")),
                variables: vec!["title".to_owned()],
            },
            CaptureTemplateRow {
                selector: Some("@tmpl/todo".to_owned()),
                id: Some("@tmpl/todo".to_owned()),
                title: Some("Todo".to_owned()),
                destination: Some("inbox.z".to_owned()),
                path: Some(PathBuf::from("templates.z")),
                variables: vec!["title".to_owned(), "body".to_owned()],
            },
        ]));

        app.handle_key(key(KeyCode::Down));
        let DashboardOverlay::CapturePicker(picker) = app.overlay() else {
            panic!("picker should remain open");
        };
        assert_eq!(picker.selected, 1);

        app.handle_key(key(KeyCode::Enter));
        let DashboardOverlay::Capture(draft) = app.overlay() else {
            panic!("selection should open capture form");
        };
        assert_eq!(draft.template, "@tmpl/todo");
        assert_eq!(draft.destination, "inbox.z");
        assert_eq!(draft.template_label(), "Todo (@tmpl/todo)");
        assert_eq!(
            draft.editable_fields(),
            vec![
                CaptureField::Template,
                CaptureField::Title,
                CaptureField::Body,
                CaptureField::Destination,
            ]
        );

        app.overlay =
            DashboardOverlay::CapturePicker(CaptureTemplatePicker::new(vec![CaptureTemplateRow {
                selector: Some("@tmpl/project".to_owned()),
                id: Some("@tmpl/project".to_owned()),
                title: None,
                destination: None,
                path: None,
                variables: Vec::new(),
            }]));
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.overlay(), &DashboardOverlay::None);
        assert_eq!(app.status(), "capture template selection canceled");
    }

    #[test]
    fn capture_picker_keeps_unselectable_template_open() {
        let mut app = test_app(Panel::Today);
        app.overlay =
            DashboardOverlay::CapturePicker(CaptureTemplatePicker::new(vec![CaptureTemplateRow {
                selector: None,
                id: None,
                title: None,
                destination: Some("misc.z".to_owned()),
                path: Some(PathBuf::from("templates.z")),
                variables: vec!["title".to_owned()],
            }]));

        app.handle_key(key(KeyCode::Enter));

        assert!(matches!(app.overlay(), DashboardOverlay::CapturePicker(_)));
        assert!(app.status().contains("capture template unavailable"));
    }

    #[test]
    fn failed_capture_action_shows_log_overlay() {
        let mut app = test_app(Panel::Today);
        app.pending_capture = Some(pending(9, PendingOperationKind::Capture));
        app.apply_async_result(AsyncResult::Capture {
            generation: 9,
            result: Err("capture failed: template is required".to_owned()),
        });

        assert_eq!(app.status(), "Capture failed");
        assert!(matches!(app.overlay(), DashboardOverlay::Log { .. }));
    }

    #[test]
    fn status_event_ring_is_bounded_and_ordered() {
        let mut app = test_app(Panel::Today);

        for index in 0..55 {
            app.record_status(SeverityKind::Info, format!("event-{index}"));
        }

        assert_eq!(app.status_events().len(), STATUS_EVENT_LIMIT);
        assert_eq!(
            app.status_events().first().map(|event| event.order),
            Some(6)
        );
        assert_eq!(
            app.status_events().last().map(|event| event.order),
            Some(55)
        );
        assert_eq!(app.status(), "event-54");
    }

    #[test]
    fn log_key_opens_recent_status_event_overlay() {
        let mut app = test_app(Panel::Today);
        app.record_status_with_detail(
            SeverityKind::Error,
            "Open failed",
            Some("open failed: $EDITOR is not set".to_owned()),
        );

        app.handle_key(key(KeyCode::Char('L')));

        assert_eq!(app.overlay(), &DashboardOverlay::EventLog);
        assert_eq!(app.status_events().len(), 1);
    }

    #[test]
    fn slash_enters_search_editing_and_typing_schedules_query() {
        let mut app = test_app(Panel::Today);

        app.handle_key(key(KeyCode::Char('/')));
        assert_eq!(app.frame().panel, Panel::Search);
        assert!(app.is_search_editing());

        for character in "#z/inbox".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }

        assert_eq!(app.frame().query.as_deref(), Some("#z/inbox"));
        assert_eq!(
            app.frame()
                .search_panel()
                .map(|search| search.input.as_str()),
            Some("#z/inbox")
        );
        assert_eq!(app.status(), "search pending");
    }

    #[test]
    fn search_editing_supports_cursor_keys_and_control_deletes() {
        let mut app = test_app(Panel::Search);

        app.handle_key(key(KeyCode::Char('/')));
        for character in "#z/todo due:<=today".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Left));
        app.handle_key(key(KeyCode::Left));
        app.handle_key(key(KeyCode::Char('X')));
        assert_eq!(
            app.frame()
                .search_panel()
                .map(|search| search.input.as_str()),
            Some("#z/todo due:<=todXay")
        );

        app.handle_key(key(KeyCode::Backspace));
        app.handle_key(key(KeyCode::Delete));
        assert_eq!(
            app.frame()
                .search_panel()
                .map(|search| search.input.as_str()),
            Some("#z/todo due:<=tody")
        );

        app.handle_key(key(KeyCode::End));
        app.handle_key(ctrl_key('w'));
        assert_eq!(
            app.frame()
                .search_panel()
                .map(|search| search.input.as_str()),
            Some("#z/todo ")
        );

        app.handle_key(key(KeyCode::End));
        app.handle_key(key(KeyCode::Char('x')));
        app.handle_key(key(KeyCode::Left));
        app.handle_key(ctrl_key('u'));
        assert_eq!(
            app.frame()
                .search_panel()
                .map(|search| search.input.as_str()),
            Some("x")
        );
    }

    #[test]
    fn enter_commits_search_history_and_up_down_recalls_draft() {
        let mut app = test_app(Panel::Search);

        app.handle_key(key(KeyCode::Char('/')));
        for character in "#z/inbox".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.search_history.entries(), &["#z/inbox".to_owned()]);
        assert!(!app.is_search_editing());

        app.search_history.commit("@queries/today");
        app.handle_key(key(KeyCode::Char('/')));
        for character in "draft".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Up));
        assert_eq!(
            app.frame()
                .search_panel()
                .map(|search| search.input.as_str()),
            Some("@queries/today")
        );
        app.handle_key(key(KeyCode::Up));
        assert_eq!(
            app.frame()
                .search_panel()
                .map(|search| search.input.as_str()),
            Some("#z/inbox")
        );
        app.handle_key(key(KeyCode::Down));
        assert_eq!(
            app.frame()
                .search_panel()
                .map(|search| search.input.as_str()),
            Some("@queries/today")
        );
        app.handle_key(key(KeyCode::Down));
        assert_eq!(
            app.frame()
                .search_panel()
                .map(|search| search.input.as_str()),
            Some("#z/inboxdraft")
        );
    }

    #[test]
    fn esc_cancels_search_edit_and_restores_original_panel_state() {
        let mut app = test_app(Panel::Search);
        app.frame.set_query(Some("#z/inbox".to_owned()));
        app.frame.set_search(SearchPanel::with_rows(
            "#z/inbox",
            vec![zettel(9, "search")],
        ));

        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key(KeyCode::Char('x')));
        assert_eq!(
            app.frame()
                .search_panel()
                .map(|search| search.input.as_str()),
            Some("#z/inboxx")
        );

        app.handle_key(key(KeyCode::Esc));
        let search = app.frame().search_panel().expect("search panel");
        assert_eq!(search.input, "#z/inbox");
        assert_eq!(search.rows.len(), 1);
        assert_eq!(app.frame().query.as_deref(), Some("#z/inbox"));
        assert!(!app.is_search_editing());
        assert_eq!(app.status(), "search edit canceled");
    }

    #[test]
    fn search_editing_ignores_key_release_and_repeat_events() {
        let mut app = test_app(Panel::Search);

        app.handle_key(key(KeyCode::Char('/')));
        app.handle_key(key_with_kind(KeyCode::Char('x'), KeyEventKind::Repeat));
        app.handle_key(key_with_kind(KeyCode::Char('x'), KeyEventKind::Release));
        app.handle_key(key(KeyCode::Char('x')));

        assert_eq!(
            app.frame()
                .search_panel()
                .map(|search| search.input.as_str()),
            Some("x")
        );
    }

    #[test]
    fn f1_opens_swog_help_from_search_editing() {
        let mut app = test_app(Panel::Today);

        app.handle_key(key(KeyCode::Char('/')));
        assert!(app.is_search_editing());

        assert_eq!(app.handle_key(key(KeyCode::F(1))), AppCommand::Continue);

        assert_eq!(app.frame().panel, Panel::Search);
        assert!(!app.is_search_editing());
        assert_eq!(app.overlay(), &DashboardOverlay::SwogHelp);
        assert_eq!(app.status(), "SWOG help opened");
    }

    #[test]
    fn h_opens_swog_help_when_search_panel_is_focused() {
        let mut app = test_app(Panel::Search);

        assert_eq!(
            app.handle_key(key(KeyCode::Char('H'))),
            AppCommand::Continue
        );

        assert_eq!(app.overlay(), &DashboardOverlay::SwogHelp);
        assert_eq!(app.status(), "SWOG help opened");
    }

    #[test]
    fn enter_on_valid_queries_row_runs_stored_query_in_search_panel() {
        let mut app = queries_app(vec![query_row("queries/inbox", true)]);

        assert_eq!(app.handle_key(key(KeyCode::Enter)), AppCommand::Continue);

        assert_eq!(app.frame().panel, Panel::Search);
        assert_eq!(app.frame().query.as_deref(), Some("@queries/inbox"));
        assert_eq!(
            app.frame()
                .search_panel()
                .map(|search| search.input.as_str()),
            Some("@queries/inbox")
        );
        assert!(app.pending_search.is_some());
        assert_eq!(app.search_history.entries(), &["@queries/inbox".to_owned()]);
        assert_eq!(app.status(), "search running");
    }

    #[test]
    fn enter_on_invalid_queries_row_warns_without_running_search() {
        let mut app = queries_app(vec![query_row("queries/bad", false)]);

        assert_eq!(app.handle_key(key(KeyCode::Enter)), AppCommand::Continue);

        assert_eq!(app.frame().panel, Panel::Queries);
        assert_eq!(app.frame().query, None);
        assert!(app.pending_search.is_none());
        assert_eq!(app.selected_index(), 0);
        assert_eq!(app.status(), "saved query @queries/bad is invalid");
    }

    #[test]
    fn o_on_queries_row_opens_query_source() {
        let mut app = queries_app(vec![query_row("queries/inbox", true)]);

        let command = app.handle_key(key(KeyCode::Char('o')));

        assert_eq!(
            command,
            AppCommand::Open(SourceLocation {
                path: PathBuf::from("/tmp/corpus/queries.z"),
                line: Some(4),
                column: Some(2),
            })
        );
        assert_eq!(app.frame().panel, Panel::Queries);
    }

    #[test]
    fn stale_search_results_are_ignored() {
        let mut app = test_app(Panel::Search);
        app.pending_search = Some(pending(4, PendingOperationKind::Search));

        app.apply_async_result(AsyncResult::Search {
            generation: 3,
            result: Ok(SearchPanel::with_rows("#z/inbox", vec![zettel(9, "stale")])),
        });

        assert_eq!(app.frame().active_rows(), Vec::new());
    }

    #[test]
    fn fix_preview_key_reports_non_diagnostic_selection() {
        let mut app = test_app(Panel::Today);

        app.handle_key(key(KeyCode::Char('f')));

        assert_eq!(
            app.status(),
            "fix preview unavailable: selected row is not diagnostic"
        );
        assert_eq!(app.overlay(), &DashboardOverlay::None);
    }

    #[test]
    fn fix_preview_async_result_opens_and_f_closes_overlay() {
        let mut app = test_app(Panel::Diagnostics);
        app.pending_fix_preview = Some(pending(12, PendingOperationKind::FixPreview));

        app.apply_async_result(AsyncResult::FixPreview {
            generation: 12,
            result: Ok(empty_fix_preview_overlay()),
            marked_summary: empty_marked_summary(),
        });

        assert!(matches!(app.overlay(), DashboardOverlay::FixPreview(_)));
        assert_eq!(app.status(), "fix preview unavailable");

        app.handle_key(key(KeyCode::Char('f')));
        assert_eq!(app.overlay(), &DashboardOverlay::None);
    }

    #[test]
    fn fix_preview_upper_f_confirms_and_cancel_leaves_source_unapplied() {
        let mut app = test_app(Panel::Diagnostics);
        app.overlay = DashboardOverlay::FixPreview(safe_fix_preview_overlay());

        app.handle_key(key(KeyCode::Char('F')));
        assert!(matches!(
            app.overlay(),
            DashboardOverlay::ConfirmFixApply(_)
        ));

        app.handle_key(key(KeyCode::Char('n')));
        assert_eq!(app.overlay(), &DashboardOverlay::None);
        assert_eq!(app.status(), "fix apply canceled");
    }

    #[test]
    fn fix_preview_upper_f_refuses_unavailable_preview() {
        let mut app = test_app(Panel::Diagnostics);
        app.overlay = DashboardOverlay::FixPreview(empty_fix_preview_overlay());

        app.handle_key(key(KeyCode::Char('F')));

        assert!(matches!(app.overlay(), DashboardOverlay::FixPreview(_)));
        assert_eq!(app.status(), "fix apply unavailable: no safe preview");
    }

    #[test]
    fn fix_apply_async_success_refreshes_snapshot_and_logs_detail() {
        let mut app = test_app(Panel::Diagnostics);
        app.pending_fix_apply = Some(pending(22, PendingOperationKind::FixApply));

        app.apply_async_result(AsyncResult::FixApply {
            generation: 22,
            result: Ok(FixApplyOutcome {
                changed_path: PathBuf::from("/tmp/corpus/links.z"),
                applied_rule_codes: vec!["fix.unresolved_absolute_link_typo".to_owned()],
                applied_edits: 1,
                reindex_summary: reindex_summary(0),
                snapshot: ready_snapshot(
                    Vec::new(),
                    Vec::new(),
                    vec![IndexStatusRow::new("Diagnostics", 0)],
                ),
            }),
        });

        assert_eq!(app.status(), "fix apply complete");
        assert_eq!(app.frame().diagnostics_label(), "0");
        assert!(
            app.latest_status_event()
                .and_then(|event| event.detail.as_ref())
                .is_some_and(|detail| detail.contains("fix.unresolved_absolute_link_typo"))
        );
    }

    #[test]
    fn fix_apply_async_failure_shows_log_overlay() {
        let mut app = test_app(Panel::Diagnostics);
        app.pending_fix_apply = Some(pending(23, PendingOperationKind::FixApply));

        app.apply_async_result(AsyncResult::FixApply {
            generation: 23,
            result: Err("fix apply refused: index is stale relative to source".to_owned()),
        });

        assert_eq!(app.status(), "Fix apply failed");
        assert!(matches!(app.overlay(), DashboardOverlay::Log { .. }));
        assert!(
            app.latest_status_event()
                .and_then(|event| event.detail.as_ref())
                .is_some_and(|detail| detail.contains("elapsed:"))
        );
    }

    #[test]
    fn refresh_preserves_zettel_selection_by_canonical_identity_after_reorder() {
        let mut app = test_app(Panel::Today);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected_index(), 1);

        app.pending_refresh = Some(pending(7, PendingOperationKind::Refresh));
        app.apply_async_result(AsyncResult::Refresh {
            generation: 7,
            snapshot: ready_snapshot(
                vec![
                    PanelRow::Zettel(zettel(4, "c")),
                    PanelRow::Zettel(zettel(1, "a")),
                    PanelRow::Zettel(zettel(2, "b")),
                ],
                Vec::new(),
                vec![IndexStatusRow::new("Discovered files", 3)],
            ),
        });

        assert_eq!(app.selected_index(), 2);
    }

    #[test]
    fn refresh_preserves_zettel_selection_by_store_fallback_identity() {
        let mut app = test_app_with_snapshot(
            Panel::Today,
            ready_snapshot(
                vec![
                    PanelRow::Zettel(zettel_without_canonical_id(1, "a")),
                    PanelRow::Zettel(zettel_without_canonical_id(2, "b")),
                ],
                Vec::new(),
                vec![IndexStatusRow::new("Discovered files", 2)],
            ),
        );
        app.handle_key(key(KeyCode::Down));

        app.pending_refresh = Some(pending(8, PendingOperationKind::Refresh));
        app.apply_async_result(AsyncResult::Refresh {
            generation: 8,
            snapshot: ready_snapshot(
                vec![
                    PanelRow::Zettel(zettel_without_canonical_id(3, "c")),
                    PanelRow::Zettel(zettel_without_canonical_id(2, "b")),
                    PanelRow::Zettel(zettel_without_canonical_id(1, "a")),
                ],
                Vec::new(),
                vec![IndexStatusRow::new("Discovered files", 3)],
            ),
        });

        assert_eq!(app.selected_index(), 1);
    }

    #[test]
    fn refresh_preserves_diagnostic_and_index_selection_by_identity() {
        let mut diagnostics_app = test_app_with_snapshot(
            Panel::Diagnostics,
            ready_snapshot(
                Vec::new(),
                vec![
                    diagnostic(10, "first", "a.z"),
                    diagnostic(11, "second", "b.z"),
                ],
                vec![IndexStatusRow::new("Discovered files", 2)],
            ),
        );
        diagnostics_app.handle_key(key(KeyCode::Down));

        diagnostics_app.pending_refresh = Some(pending(9, PendingOperationKind::Refresh));
        diagnostics_app.apply_async_result(AsyncResult::Refresh {
            generation: 9,
            snapshot: ready_snapshot(
                Vec::new(),
                vec![
                    diagnostic(12, "third", "c.z"),
                    diagnostic(10, "first", "a.z"),
                    diagnostic(11, "second", "b.z"),
                ],
                vec![IndexStatusRow::new("Discovered files", 3)],
            ),
        });
        assert_eq!(diagnostics_app.selected_index(), 2);

        let mut index_app = test_app_with_snapshot(
            Panel::Index,
            ready_snapshot(
                Vec::new(),
                Vec::new(),
                vec![
                    IndexStatusRow::new("Discovered files", 2),
                    IndexStatusRow::new("Diagnostics", 1),
                ],
            ),
        );
        index_app.handle_key(key(KeyCode::Down));

        index_app.pending_refresh = Some(pending(10, PendingOperationKind::Refresh));
        index_app.apply_async_result(AsyncResult::Refresh {
            generation: 10,
            snapshot: ready_snapshot(
                Vec::new(),
                Vec::new(),
                vec![
                    IndexStatusRow::new("Indexed files", 2),
                    IndexStatusRow::new("Discovered files", 3),
                    IndexStatusRow::new("Diagnostics", 4),
                ],
            ),
        });
        assert_eq!(index_app.selected_index(), 2);
    }

    #[test]
    fn page_and_half_page_navigation_update_selection_and_scroll_offset() {
        let mut app = test_app_with_snapshot(
            Panel::Today,
            ready_snapshot(
                (0..12)
                    .map(|index| PanelRow::Zettel(zettel(index, &format!("row-{index}"))))
                    .collect(),
                Vec::new(),
                vec![IndexStatusRow::new("Discovered files", 12)],
            ),
        );
        let rows = app.frame.active_rows();
        app.active_viewport_mut().set_visible_row_count(4, &rows);

        app.handle_key(key(KeyCode::PageDown));
        assert_eq!(app.selected_index(), 4);
        assert_eq!(app.active_viewport().scroll_offset(), 1);

        app.handle_key(key(KeyCode::PageDown));
        assert_eq!(app.selected_index(), 8);
        assert_eq!(app.active_viewport().scroll_offset(), 5);

        app.handle_key(key(KeyCode::PageUp));
        assert_eq!(app.selected_index(), 4);
        assert_eq!(app.active_viewport().scroll_offset(), 4);

        app.handle_key(ctrl_key('d'));
        assert_eq!(app.selected_index(), 6);
        assert_eq!(app.active_viewport().scroll_offset(), 4);

        app.handle_key(ctrl_key('u'));
        assert_eq!(app.selected_index(), 4);
        assert_eq!(app.active_viewport().scroll_offset(), 4);
    }

    #[test]
    fn switching_panels_preserves_each_panel_viewport() {
        let mut app = test_app_with_snapshot(
            Panel::Today,
            ready_snapshot(
                (0..12)
                    .map(|index| PanelRow::Zettel(zettel(index, &format!("row-{index}"))))
                    .collect(),
                Vec::new(),
                vec![IndexStatusRow::new("Discovered files", 12)],
            ),
        );
        let rows = app.frame.active_rows();
        app.active_viewport_mut().set_visible_row_count(4, &rows);
        app.handle_key(key(KeyCode::PageDown));

        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.frame().panel, Panel::Inbox);
        assert_eq!(app.selected_index(), 0);

        app.handle_key(key(KeyCode::BackTab));
        assert_eq!(app.frame().panel, Panel::Today);
        assert_eq!(app.selected_index(), 4);
        assert_eq!(app.active_viewport().scroll_offset(), 1);
    }

    #[test]
    fn today_mode_key_filters_rows_and_preserves_visible_selection_by_identity() {
        let mut app = test_app_with_snapshot(
            Panel::Today,
            ready_snapshot(
                vec![
                    PanelRow::Zettel(zettel(1, "a")),
                    PanelRow::Diagnostic(diagnostic_with_severity(10, "warning", "diag", "diag.z")),
                    PanelRow::Zettel(zettel(2, "b")),
                ],
                Vec::new(),
                vec![IndexStatusRow::new("Diagnostics", 1)],
            ),
        );
        app.handle_key(key(KeyCode::Char('G')));
        assert_eq!(app.selected_index(), 2);

        app.handle_key(key(KeyCode::Char('t')));

        assert_eq!(app.frame.today_mode.label(), "todos");
        assert_eq!(app.frame.active_rows().len(), 2);
        assert_eq!(app.selected_index(), 1);
        assert_eq!(app.status(), "today mode: todos (2 rows)");

        app.handle_key(key(KeyCode::Char('t')));

        assert_eq!(app.frame.today_mode.label(), "diagnostics");
        assert_eq!(app.frame.active_rows().len(), 1);
        assert_eq!(app.selected_index(), 0);
    }

    #[test]
    fn diagnostic_filter_keys_preserve_selection_by_row_identity() {
        let mut app = test_app_with_snapshot(
            Panel::Diagnostics,
            ready_snapshot(
                Vec::new(),
                vec![
                    diagnostic_with_severity(10, "warning", "first", "a.z"),
                    diagnostic_with_severity(11, "error", "second", "b.z"),
                    diagnostic_with_severity(12, "error", "third", "c.z"),
                ],
                vec![IndexStatusRow::new("Diagnostics", 3)],
            ),
        );
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected_index(), 1);

        app.handle_key(key(KeyCode::Char('e')));

        assert_eq!(app.frame.diagnostic_filters.severity.label(), "error");
        assert_eq!(app.selected_index(), 0);
        assert_eq!(app.frame.active_rows().len(), 2);

        app.handle_key(key(KeyCode::Char('a')));

        assert!(!app.frame.diagnostic_filters.is_active());
        assert_eq!(app.selected_index(), 1);
    }

    #[test]
    fn diagnostic_filter_edit_applies_code_and_path_filters() {
        let mut app = test_app_with_snapshot(
            Panel::Diagnostics,
            ready_snapshot(
                Vec::new(),
                vec![
                    diagnostic_with_severity(10, "warning", "missing link", "notes/a.z"),
                    diagnostic_with_severity(11, "warning", "sort issue", "other/b.z"),
                ],
                vec![IndexStatusRow::new("Diagnostics", 2)],
            ),
        );

        app.handle_key(key(KeyCode::Char(':')));
        for character in "reference".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Tab));
        for character in "notes".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.frame.diagnostic_filters.code, "reference");
        assert_eq!(app.frame.diagnostic_filters.path, "notes");
        assert_eq!(app.frame.active_rows().len(), 1);
        assert_eq!(
            app.status(),
            "diagnostic filters: code=reference path=notes"
        );
    }

    #[test]
    fn space_marks_and_unmarks_diagnostic_rows() {
        let mut app = test_app_with_snapshot(
            Panel::Diagnostics,
            ready_snapshot(
                Vec::new(),
                vec![diagnostic_with_severity(10, "warning", "first", "a.z")],
                vec![IndexStatusRow::new("Diagnostics", 1)],
            ),
        );

        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.frame().marked_diagnostic_count(), 1);
        assert_eq!(app.status(), "diagnostic marked: 1 marked");

        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.frame().marked_diagnostic_count(), 0);
        assert_eq!(app.status(), "diagnostic unmarked: 0 marked");
    }

    #[test]
    fn yank_key_opens_overlay_and_returns_selected_value() {
        let mut app = test_app(Panel::Today);

        app.handle_key(key(KeyCode::Char('y')));
        let DashboardOverlay::Yank(overlay) = app.overlay() else {
            panic!("yank overlay should open");
        };
        assert_eq!(overlay.target_summary, "@a a");
        assert_eq!(overlay.options[0].value.as_deref(), Some("@a"));
        assert_eq!(
            overlay.options[1].value.as_deref(),
            Some("/tmp/corpus/a.z:1:1")
        );
        assert!(overlay.options[2].value.is_none());

        let command = app.handle_key(key(KeyCode::Char('1')));
        assert_eq!(
            command,
            AppCommand::Yank(YankCommand {
                kind: crate::model::YankValueKind::RowId,
                target_summary: "@a a".to_owned(),
                value: "@a".to_owned(),
            })
        );
    }

    #[test]
    fn yank_unsupported_value_reports_clear_status() {
        let mut app = test_app(Panel::Today);
        app.handle_key(key(KeyCode::Char('y')));

        let command = app.handle_key(key(KeyCode::Char('3')));

        assert_eq!(command, AppCommand::Continue);
        assert_eq!(
            app.status(),
            "yank diagnostic message unavailable: selected row is not diagnostic"
        );
    }

    #[test]
    fn yank_clipboard_failure_shows_value_in_log_overlay() {
        let mut app = test_app(Panel::Today);
        let request = YankCommand {
            kind: crate::model::YankValueKind::RowId,
            target_summary: "@a a".to_owned(),
            value: "@a".to_owned(),
        };

        app.record_yank_result(
            request,
            Err("clipboard transport unavailable: stdout is not a terminal".to_owned()),
        );

        assert_eq!(app.status(), "Yank clipboard unavailable");
        let DashboardOverlay::Log { title, message } = app.overlay() else {
            panic!("clipboard fallback should show log overlay");
        };
        assert_eq!(title, "Yank clipboard unavailable");
        assert!(message.contains("@a"));
        assert!(message.contains("stdout is not a terminal"));
    }

    #[test]
    fn marked_diagnostics_survive_refresh_by_identity_and_prune_disappeared_rows() {
        let mut app = test_app_with_snapshot(
            Panel::Diagnostics,
            ready_snapshot(
                Vec::new(),
                vec![
                    diagnostic_with_severity(10, "warning", "first", "a.z"),
                    diagnostic_with_severity(11, "warning", "second", "b.z"),
                ],
                vec![IndexStatusRow::new("Diagnostics", 2)],
            ),
        );
        app.handle_key(key(KeyCode::Char(' ')));
        app.handle_key(key(KeyCode::Down));
        app.handle_key(key(KeyCode::Char(' ')));
        assert_eq!(app.frame().marked_diagnostic_count(), 2);

        app.pending_refresh = Some(pending(31, PendingOperationKind::Refresh));
        app.apply_async_result(AsyncResult::Refresh {
            generation: 31,
            snapshot: ready_snapshot(
                Vec::new(),
                vec![diagnostic_with_severity(11, "warning", "second", "b.z")],
                vec![IndexStatusRow::new("Diagnostics", 1)],
            ),
        });

        assert_eq!(app.frame().marked_diagnostic_count(), 1);
        let rows = app.frame().active_rows();
        assert!(rows.iter().all(|row| app.frame().is_row_marked(row)));
    }

    #[test]
    fn fix_preview_from_non_diagnostic_selection_summarizes_marked_queue() {
        let mut app = test_app_with_snapshot(
            Panel::Today,
            ready_snapshot(
                vec![PanelRow::Zettel(zettel(1, "a"))],
                vec![diagnostic_with_severity(10, "warning", "first", "a.z")],
                vec![IndexStatusRow::new("Diagnostics", 1)],
            ),
        );
        app.switch_panel(Panel::Diagnostics);
        app.handle_key(key(KeyCode::Char(' ')));
        app.switch_panel(Panel::Today);

        app.handle_key(key(KeyCode::Char('f')));

        let DashboardOverlay::FixPreview(preview) = app.overlay() else {
            panic!("marked queue should open fix preview overlay");
        };
        assert_eq!(preview.diagnostic.code, "marked.diagnostics");
        assert_eq!(
            preview
                .marked_summary
                .as_ref()
                .map(|summary| summary.marked_count),
            Some(1)
        );
        assert_eq!(
            app.status(),
            "fix preview shows marked diagnostics; bulk apply is unavailable"
        );
    }

    #[test]
    fn mark_done_key_requires_today_todo_row() {
        let mut app = test_app(Panel::Inbox);

        app.handle_key(key(KeyCode::Char('d')));

        assert_eq!(app.status(), "todo mark done unavailable: switch to Today");

        let mut app = test_app_with_snapshot(
            Panel::Today,
            ready_snapshot(
                vec![PanelRow::Diagnostic(diagnostic_with_severity(
                    10, "warning", "first", "a.z",
                ))],
                Vec::new(),
                vec![IndexStatusRow::new("Diagnostics", 1)],
            ),
        );

        app.handle_key(key(KeyCode::Char('d')));

        assert_eq!(
            app.status(),
            "todo mark done unavailable: selected row is not a todo"
        );
    }

    #[test]
    fn mark_done_confirmation_can_cancel_without_writing() {
        let (_temp, mut app, path, original) = todo_app();

        app.handle_key(key(KeyCode::Char('d')));

        assert!(matches!(
            app.overlay(),
            DashboardOverlay::ConfirmTodoApply(_)
        ));
        assert_eq!(app.status(), "todo mark done preview ready");

        app.handle_key(key(KeyCode::Esc));

        assert_eq!(app.overlay(), &DashboardOverlay::None);
        assert_eq!(app.status(), "todo mark done canceled");
        assert_eq!(fs::read_to_string(path).expect("read source"), original);
    }

    #[test]
    fn mark_done_apply_reindexes_and_removes_completed_today_row() {
        let (_temp, mut app, path, _original) = todo_app();

        app.handle_key(key(KeyCode::Char('d')));
        app.handle_key(key(KeyCode::Char('y')));
        drain_until_idle(&mut app);

        let source = fs::read_to_string(path).expect("read source");
        assert!(source.contains("[X] did::"));
        assert_eq!(app.status(), "todo apply complete");
        assert!(app.frame().active_rows().is_empty());
        assert_eq!(app.selected_index(), 0);
        assert!(
            app.latest_status_event()
                .and_then(|event| event.detail.as_ref())
                .is_some_and(|detail| detail.contains("changed_fields: todo, did"))
        );
    }

    #[test]
    fn postpone_prompt_requires_valid_date_and_explicit_ambiguous_field() {
        let today = today_iso();
        let source = format!(
            "\
%%% @root #z/ref
Root
%%%

- @root/task #z/todo [ ] due::{today} do::{today} Task.
"
        );
        let (_temp, mut app, path, original) = todo_app_with_source(&source);

        app.handle_key(key(KeyCode::Char('p')));

        let DashboardOverlay::TodoPrompt(draft) = app.overlay() else {
            panic!("postpone prompt should open");
        };
        assert_eq!(
            draft.field_options,
            vec![TodoDateField::Due, TodoDateField::Do]
        );
        assert_eq!(draft.active, TodoPromptField::Target);
        assert_eq!(draft.target_field, None);

        app.handle_key(key(KeyCode::Tab));

        for character in "2026-02-30".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Enter));

        let DashboardOverlay::TodoPrompt(draft) = app.overlay() else {
            panic!("invalid date should stay in prompt");
        };
        assert!(
            draft
                .error
                .as_deref()
                .is_some_and(|error| error.contains("day"))
        );
        assert_eq!(fs::read_to_string(&path).expect("read source"), original);

        for _ in 0.."2026-02-30".len() {
            app.handle_key(key(KeyCode::Backspace));
        }
        for character in "2026-05-09".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Enter));

        let DashboardOverlay::TodoPrompt(draft) = app.overlay() else {
            panic!("ambiguous field should stay in prompt");
        };
        assert_eq!(
            draft.error.as_deref(),
            Some("choose due or do before applying")
        );
        assert_eq!(fs::read_to_string(&path).expect("read source"), original);
    }

    #[test]
    fn postpone_prompt_applies_relative_date_to_selected_field() {
        let today = today_iso();
        let expected = actions::parse_todo_prompt_date("+1w")
            .expect("relative date")
            .iso;
        let source = format!(
            "\
%%% @root #z/ref
Root
%%%

- @root/task #z/todo [ ] due::{today} do::{today} Task.
"
        );
        let (_temp, mut app, path, _original) = todo_app_with_source(&source);

        app.handle_key(key(KeyCode::Char('p')));
        app.handle_key(key(KeyCode::Char('o')));
        app.handle_key(key(KeyCode::Tab));
        for character in "+1w".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Enter));
        drain_until_idle(&mut app);

        let source = fs::read_to_string(path).expect("read source");
        assert!(source.contains(&format!("due::{today}")));
        assert!(source.contains(&format!("do::{expected}")));
        assert_eq!(app.status(), "todo apply complete");
        assert!(
            app.latest_status_event()
                .and_then(|event| event.detail.as_ref())
                .is_some_and(|detail| detail.contains("changed_fields: do"))
        );
    }

    #[test]
    fn schedule_prompt_can_cancel_or_apply_without_touching_unrelated_source() {
        let source = "\
%%% @root #z/ref
Root
%%%

- @root/task #z/todo [ ] Task.
  Body stays.
";
        let (_temp, mut app, path, original) = todo_app_with_source(source);

        app.handle_key(key(KeyCode::Char('s')));
        app.handle_key(key(KeyCode::Esc));

        assert_eq!(app.overlay(), &DashboardOverlay::None);
        assert_eq!(app.status(), "todo schedule canceled");
        assert_eq!(fs::read_to_string(&path).expect("read source"), original);

        app.handle_key(key(KeyCode::Char('s')));
        for character in "2026-05-09".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Enter));
        drain_until_idle(&mut app);

        let source = fs::read_to_string(path).expect("read source");
        assert!(source.contains("[ ] do::2026-05-09 Task."));
        assert!(source.contains("Body stays."));
        assert_eq!(app.status(), "todo apply complete");
    }

    #[test]
    fn todo_prompt_planner_rejection_stays_visible_without_writing() {
        let today = today_iso();
        let source = format!(
            "\
%%% @root #z/ref
Root
%%%

- @root/task #z/todo [ ] due::{today} Task.
"
        );
        let (_temp, mut app, path, _original) = todo_app_with_source(&source);

        app.handle_key(key(KeyCode::Char('p')));
        fs::write(&path, source.replace("Task.", "Changed task.")).expect("stale write");
        for character in "2026-05-09".chars() {
            app.handle_key(key(KeyCode::Char(character)));
        }
        app.handle_key(key(KeyCode::Enter));

        let DashboardOverlay::TodoPrompt(draft) = app.overlay() else {
            panic!("planner rejection should stay in prompt");
        };
        assert!(
            draft
                .error
                .as_deref()
                .is_some_and(|error| error.contains("index is stale relative to source"))
        );
        assert!(
            !fs::read_to_string(path)
                .expect("read source")
                .contains("2026-05-09")
        );
    }

    fn test_app(panel: Panel) -> AppState {
        test_app_with_snapshot(
            panel,
            ready_snapshot(
                vec![
                    PanelRow::Zettel(zettel(1, "a")),
                    PanelRow::Zettel(zettel(2, "b")),
                ],
                Vec::new(),
                vec![IndexStatusRow::new("Discovered files", 1)],
            ),
        )
    }

    fn test_app_with_snapshot(panel: Panel, snapshot: DashboardSnapshot) -> AppState {
        let root = PathBuf::from("/tmp/corpus");
        let db = PathBuf::from("/tmp/zorg.sqlite3");
        let frame = DashboardFrame::new(root.clone(), db.clone(), panel, None, snapshot);
        let options = StoreOptions::new(root, db).expect("store options");
        AppState::new(frame, options)
    }

    fn test_app_with_root(panel: Panel, root: PathBuf, db: PathBuf) -> AppState {
        let frame = DashboardFrame::new(
            root.clone(),
            db.clone(),
            panel,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let options = StoreOptions::new(root, db).expect("store options");
        AppState::new(frame, options)
    }

    fn temp_path(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("zorg-dash-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }

    fn write_capture_templates(root: &std::path::Path, source: &str) {
        fs::write(root.join("templates.z"), source).expect("write templates");
    }

    fn enable_auto_refresh(app: &mut AppState, interval: Duration) {
        let config = AutoRefreshConfig::new(interval);
        app.auto_refresh_config = Some(config);
        app.next_auto_refresh_at = Some(Instant::now() + interval);
        app.frame.set_auto_refresh_config(Some(config));
    }

    fn todo_app() -> (tempfile::TempDir, AppState, PathBuf, String) {
        let source = "\
%%% @root #z/ref
Root
%%%

- @root/task #z/todo [ ] Task.
";
        todo_app_with_source(source)
    }

    fn todo_app_with_source(source: &str) -> (tempfile::TempDir, AppState, PathBuf, String) {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().to_path_buf();
        let path = root.join("tasks.z");
        fs::write(&path, source).expect("write source");
        let db = root.join("zorg.sqlite3");
        let options = StoreOptions::new(root.clone(), db.clone()).expect("store options");
        let mut store = Store::open_with_options(options.clone()).expect("open store");
        store.reindex().expect("reindex");
        drop(store);
        let snapshot = crate::data::load_snapshot(options.clone(), None, None);
        let frame = DashboardFrame::new(root, db, Panel::Today, None, snapshot);
        (temp, AppState::new(frame, options), path, source.to_owned())
    }

    fn today_iso() -> String {
        let today = crate::data::current_query_date();
        format!("{:04}-{:02}-{:02}", today.year, today.month, today.day)
    }

    fn drain_until_idle(app: &mut AppState) {
        for _ in 0..100 {
            app.drain_worker_results();
            if app.pending_todo_apply.is_none() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("todo apply worker did not finish");
    }

    fn ready_snapshot(
        today: Vec<PanelRow>,
        diagnostics: Vec<DiagnosticRow>,
        index_rows: Vec<IndexStatusRow>,
    ) -> DashboardSnapshot {
        DashboardSnapshot::Ready {
            index: Box::new(IndexPanel {
                schema_version: 2,
                rows: index_rows,
                discovered_files: 1,
                indexed_files: 1,
                changed_files: 0,
                new_files: 0,
                deleted_files: 0,
                diagnostic_count: diagnostics.len(),
                last_indexed_at_unix_ms: Some(1),
            }),
            diagnostics,
            today,
            inbox: vec![zettel(3, "inbox")],
            queries: QueryPanel::empty(),
            search: SearchPanel::empty(""),
            selected_dashboard: None,
            custom_panels: BTreeMap::new(),
        }
    }

    fn queries_app(rows: Vec<QueryRow>) -> AppState {
        let mut snapshot = ready_snapshot(
            Vec::new(),
            Vec::new(),
            vec![IndexStatusRow::new("Discovered files", 1)],
        );
        if let DashboardSnapshot::Ready { queries, .. } = &mut snapshot {
            *queries = QueryPanel { rows };
        }
        test_app_with_snapshot(Panel::Queries, snapshot)
    }

    fn query_row(id: &str, valid: bool) -> QueryRow {
        QueryRow {
            id: id.to_owned(),
            title: format!("{id} title"),
            source_path: Some(PathBuf::from("queries.z")),
            source_kind: valid.then_some(zorg_query::QueryDefinitionSourceKind::Property),
            output_kind: valid.then_some(zorg_query::QueryResultKind::List),
            definition_preview: valid.then(|| "#z/inbox".to_owned()),
            valid,
            error: (!valid).then(|| "invalid query definition".to_owned()),
            row_count_preview: None,
            row_count_error: None,
            start_line: Some(4),
            start_column: Some(2),
        }
    }

    fn zettel(store_id: i64, title: &str) -> ZettelRow {
        ZettelRow {
            store_id,
            canonical_id: Some(title.to_owned()),
            file_path: PathBuf::from(format!("/tmp/corpus/{title}.z")),
            title: title.to_owned(),
            todo_marker: None,
            todo_span: None,
            source_span: SourceSpan::bytes(0, 0),
            source_order: store_id,
            start_line: Some(1),
            start_column: Some(1),
            lifecycle_date: None,
            tags: Vec::new(),
            properties: Vec::new(),
            preview: None,
            badges: vec![QueryBadge::new("due", "#z/todo")],
        }
    }

    fn zettel_without_canonical_id(store_id: i64, title: &str) -> ZettelRow {
        let mut row = zettel(store_id, title);
        row.canonical_id = None;
        row
    }

    fn diagnostic(id: i64, message: &str, path: &str) -> DiagnosticRow {
        diagnostic_with_severity(id, "warning", message, path)
    }

    fn diagnostic_with_severity(
        id: i64,
        severity: &str,
        message: &str,
        path: &str,
    ) -> DiagnosticRow {
        DiagnosticRow {
            id,
            severity: severity.to_owned(),
            category: "semantic".to_owned(),
            code: Some("reference.missing".to_owned()),
            message: message.to_owned(),
            absolute_path: None,
            relative_path: Some(PathBuf::from(path)),
            start_byte: Some(0),
            end_byte: Some(1),
            start_line: Some(1),
            start_column: Some(1),
            end_line: Some(1),
            end_column: Some(2),
            zettel_id: None,
        }
    }

    fn empty_fix_preview_overlay() -> crate::model::FixPreviewOverlay {
        crate::model::FixPreviewOverlay {
            diagnostic: crate::model::DiagnosticPreviewContext {
                severity: "warning".to_owned(),
                code: "reference.unresolved_absolute".to_owned(),
                message: "unresolved absolute reference".to_owned(),
                path: "links.z".to_owned(),
                position: "4:5-4:17".to_owned(),
            },
            previews: Vec::new(),
            unavailable_reason: Some("No safe matching fix was found.".to_owned()),
            selector: Default::default(),
            marked_summary: None,
        }
    }

    fn safe_fix_preview_overlay() -> crate::model::FixPreviewOverlay {
        crate::model::FixPreviewOverlay {
            diagnostic: crate::model::DiagnosticPreviewContext {
                severity: "error".to_owned(),
                code: "reference.unresolved_absolute".to_owned(),
                message: "unresolved absolute reference".to_owned(),
                path: "links.z".to_owned(),
                position: "4:5-4:17".to_owned(),
            },
            previews: vec![crate::model::FixPreviewRow {
                rule_code: "fix.unresolved_absolute_link_typo".to_owned(),
                severity: "error".to_owned(),
                path: PathBuf::from("links.z"),
                primary_line: Some(4),
                primary_column: Some(5),
                replacement_preview: "#project/plan".to_owned(),
                replacement_truncated: false,
                is_preferred: true,
                is_safe: true,
                explanation: "Rewrite unresolved link to #project/plan".to_owned(),
            }],
            unavailable_reason: None,
            selector: Default::default(),
            marked_summary: None,
        }
    }

    fn empty_marked_summary() -> MarkedDiagnosticsSummary {
        MarkedDiagnosticsSummary {
            marked_count: 0,
            selected_is_marked: false,
            selected_file_marked_count: None,
            rows: Vec::new(),
        }
    }

    fn reindex_summary(diagnostic_count: usize) -> zorg_store::ReindexSummary {
        zorg_store::ReindexSummary {
            discovered_files: 1,
            indexed_files: 1,
            unchanged_files: 0,
            new_files: 0,
            changed_files: 1,
            deleted_files: 0,
            zettel_count: 1,
            diagnostic_count,
            effective_tag_count: 0,
            last_indexed_at_unix_ms: Some(1),
        }
    }

    fn pending(generation: usize, kind: PendingOperationKind) -> PendingOperation {
        let started_at = Instant::now()
            .checked_sub(Duration::from_millis(125))
            .unwrap_or_else(Instant::now);
        PendingOperation {
            generation,
            started_at,
            kind,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    fn key_with_kind(code: KeyCode, kind: KeyEventKind) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::empty(), kind)
    }

    fn ctrl_key(character: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
    }
}
