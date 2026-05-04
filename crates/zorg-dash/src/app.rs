use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use zorg_store::StoreOptions;

use crate::actions::{self, CaptureOutcome, FixApplyOutcome, ReindexOutcome};
use crate::model::{
    CaptureDraft, DashboardFrame, DashboardOverlay, DashboardRenderState, DashboardSnapshot,
    DiagnosticFilterDraft, Panel, PanelRow, PanelRowId, SearchPanel, SeverityKind, SourceLocation,
    StatusEvent,
};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
const DEFAULT_VISIBLE_ROW_COUNT: usize = 10;
const STATUS_EVENT_LIMIT: usize = 50;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum AppCommand {
    Continue,
    Quit,
    Open(SourceLocation),
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

#[derive(Debug)]
pub(crate) struct AppState {
    frame: DashboardFrame,
    store_options: StoreOptions,
    viewports: [PanelViewport; Panel::ALL.len()],
    overlay: DashboardOverlay,
    status_events: Vec<StatusEvent>,
    next_status_order: usize,
    generation: usize,
    pending_refresh: Option<usize>,
    pending_reindex: Option<usize>,
    pending_capture: Option<usize>,
    pending_fix_preview: Option<usize>,
    pending_fix_apply: Option<usize>,
    pending_search: Option<usize>,
    search_due_at: Option<Instant>,
    search_editing: bool,
    sender: Sender<AsyncResult>,
    receiver: Receiver<AsyncResult>,
}

impl AppState {
    pub(crate) fn new(frame: DashboardFrame, store_options: StoreOptions) -> Self {
        let (sender, receiver) = mpsc::channel();
        let mut state = Self {
            frame,
            store_options,
            viewports: std::array::from_fn(|_| PanelViewport::new(DEFAULT_VISIBLE_ROW_COUNT)),
            overlay: DashboardOverlay::None,
            status_events: Vec::new(),
            next_status_order: 1,
            generation: 0,
            pending_refresh: None,
            pending_reindex: None,
            pending_capture: None,
            pending_fix_preview: None,
            pending_fix_apply: None,
            pending_search: None,
            search_due_at: None,
            search_editing: false,
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
        &self.viewports[self.frame.panel.index()]
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

    #[cfg(test)]
    pub(crate) fn is_search_editing(&self) -> bool {
        self.search_editing
    }

    pub(crate) fn drain_worker_results(&mut self) {
        while let Ok(result) = self.receiver.try_recv() {
            self.apply_async_result(result);
        }
    }

    pub(crate) fn drive_search_debounce(&mut self) {
        if self
            .search_due_at
            .is_some_and(|due_at| Instant::now() >= due_at)
        {
            self.start_search_now();
        }
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> AppCommand {
        if self.overlay.is_confirming_reindex() {
            return self.handle_reindex_confirmation_key(key);
        }

        if matches!(self.overlay, DashboardOverlay::ConfirmFixApply(_)) {
            return self.handle_fix_apply_confirmation_key(key);
        }

        if matches!(self.overlay, DashboardOverlay::Capture(_)) {
            return self.handle_capture_key(key);
        }

        if matches!(self.overlay, DashboardOverlay::DiagnosticFilter(_)) {
            return self.handle_diagnostic_filter_key(key);
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

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => AppCommand::Quit,
            KeyCode::Char('?') => {
                self.overlay = DashboardOverlay::Help;
                AppCommand::Continue
            }
            KeyCode::Char('L') => {
                self.overlay = DashboardOverlay::EventLog;
                AppCommand::Continue
            }
            KeyCode::Tab => {
                self.switch_panel(self.frame.panel.next());
                AppCommand::Continue
            }
            KeyCode::BackTab => {
                self.switch_panel(self.frame.panel.previous());
                AppCommand::Continue
            }
            KeyCode::Right => {
                self.switch_panel(self.frame.panel.next());
                AppCommand::Continue
            }
            KeyCode::Left => {
                self.switch_panel(self.frame.panel.previous());
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
                self.switch_panel(Panel::Search);
                self.search_editing = true;
                self.record_status(
                    SeverityKind::Info,
                    "search edit: type SWOG or @query/id, enter runs, esc stops",
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
            KeyCode::Char(':') => {
                self.open_diagnostic_filter_edit();
                AppCommand::Continue
            }
            KeyCode::Char('f') => {
                self.start_fix_preview();
                AppCommand::Continue
            }
            KeyCode::Enter => self
                .frame
                .selected_source_location(self.selected_index())
                .map(AppCommand::Open)
                .unwrap_or_else(|| {
                    self.show_log(
                        "Open",
                        "selected row has no source location to open".to_owned(),
                    );
                    AppCommand::Continue
                }),
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

    fn handle_search_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Esc => {
                self.search_editing = false;
                self.record_status(SeverityKind::Info, "search edit stopped");
            }
            KeyCode::Enter => {
                self.search_editing = false;
                self.start_search_now();
            }
            KeyCode::Backspace => {
                let mut query = self.current_query_input();
                query.pop();
                self.update_search_input(query);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.update_search_input(String::new());
            }
            KeyCode::Char(character)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let mut query = self.current_query_input();
                query.push(character);
                self.update_search_input(query);
            }
            _ => {}
        }
        AppCommand::Continue
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
        match actions::capture_defaults(self.store_options.corpus_root()) {
            Ok(defaults) => {
                self.overlay = DashboardOverlay::Capture(CaptureDraft::new(
                    defaults.template,
                    defaults.destination,
                ));
                self.record_status(
                    SeverityKind::Info,
                    "capture edit: tab fields, enter creates, esc cancels",
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

    fn with_capture_draft(&mut self, update: impl FnOnce(&mut CaptureDraft)) {
        if let DashboardOverlay::Capture(draft) = &mut self.overlay {
            update(draft);
        }
    }

    fn with_diagnostic_filter_draft(&mut self, update: impl FnOnce(&mut DiagnosticFilterDraft)) {
        if let DashboardOverlay::DiagnosticFilter(draft) = &mut self.overlay {
            update(draft);
        }
    }

    fn switch_panel(&mut self, panel: Panel) {
        self.frame.panel = panel;
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
    }

    fn sync_all_viewports(&mut self) {
        for panel in Panel::ALL {
            let rows = self.frame.rows_for_panel(panel);
            self.viewports[panel.index()].sync_rows(&rows);
        }
    }

    fn active_viewport_mut(&mut self) -> &mut PanelViewport {
        &mut self.viewports[self.frame.panel.index()]
    }

    fn active_row_count(&self) -> usize {
        self.frame.active_rows().len()
    }

    fn start_refresh(&mut self) {
        if self.pending_refresh.is_some() {
            self.record_status(SeverityKind::Warning, "refresh already running");
            return;
        }
        let generation = self.next_generation();
        self.pending_refresh = Some(generation);
        self.record_status(SeverityKind::Info, "refresh running");
        let options = self.store_options.clone();
        let query = self.frame.query.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let snapshot = actions::refresh_snapshot(options, query);
            let _ = sender.send(AsyncResult::Refresh {
                generation,
                snapshot,
            });
        });
    }

    fn update_search_input(&mut self, query: String) {
        self.frame
            .set_query((!query.trim().is_empty()).then(|| query.clone()));
        self.frame.set_search(SearchPanel::empty(query.clone()));
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

    fn start_search_now(&mut self) {
        self.search_due_at = None;
        let query = self.current_query_input();
        self.frame
            .set_query((!query.trim().is_empty()).then(|| query.clone()));
        if query.trim().is_empty() {
            self.pending_search = None;
            self.frame.set_search(SearchPanel::empty(query));
            self.sync_active_viewport();
            self.record_status(SeverityKind::Info, "search cleared");
            return;
        }

        let generation = self.next_generation();
        self.pending_search = Some(generation);
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

    fn start_reindex(&mut self) {
        if self.pending_reindex.is_some() {
            self.record_status(SeverityKind::Warning, "reindex already running");
            return;
        }
        let generation = self.next_generation();
        self.pending_reindex = Some(generation);
        self.record_status(SeverityKind::Info, "reindex running");
        let options = self.store_options.clone();
        let query = self.frame.query.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = actions::reindex(options, query);
            let _ = sender.send(AsyncResult::Reindex { generation, result });
        });
    }

    fn start_capture(&mut self, draft: CaptureDraft) {
        if self.pending_capture.is_some() {
            self.record_status(SeverityKind::Warning, "capture already running");
            return;
        }
        let generation = self.next_generation();
        self.pending_capture = Some(generation);
        self.record_status(SeverityKind::Info, "capture running");
        let options = self.store_options.clone();
        let query = self.frame.query.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = actions::capture(options, query, draft);
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
            self.record_status(
                SeverityKind::Warning,
                "fix preview unavailable: selected row is not diagnostic",
            );
            return;
        };

        let generation = self.next_generation();
        self.pending_fix_preview = Some(generation);
        self.record_status(SeverityKind::Info, "fix preview running");
        let options = self.store_options.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = actions::fix_preview(options, diagnostic);
            let _ = sender.send(AsyncResult::FixPreview { generation, result });
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
        self.pending_fix_apply = Some(generation);
        self.record_status(SeverityKind::Info, "fix apply running");
        let options = self.store_options.clone();
        let query = self.frame.query.clone();
        let selector = preview.selector;
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = actions::fix_apply(options, query, selector);
            let _ = sender.send(AsyncResult::FixApply { generation, result });
        });
    }

    fn next_generation(&mut self) -> usize {
        self.generation = self.generation.saturating_add(1);
        self.generation
    }

    fn apply_async_result(&mut self, result: AsyncResult) {
        match result {
            AsyncResult::Refresh {
                generation,
                snapshot,
            } => {
                if self.pending_refresh != Some(generation) {
                    return;
                }
                self.pending_refresh = None;
                self.frame.set_snapshot(snapshot);
                self.sync_all_viewports();
                self.record_status(SeverityKind::Info, "refresh complete");
            }
            AsyncResult::Reindex { generation, result } => {
                if self.pending_reindex != Some(generation) {
                    return;
                }
                self.pending_reindex = None;
                match result {
                    Ok(outcome) => {
                        self.frame.set_snapshot(outcome.snapshot);
                        self.sync_all_viewports();
                        self.record_status(
                            SeverityKind::Info,
                            actions::reindex_summary_line(outcome.summary),
                        );
                    }
                    Err(message) => {
                        self.show_log("Reindex failed", message);
                    }
                }
            }
            AsyncResult::Capture { generation, result } => {
                if self.pending_capture != Some(generation) {
                    return;
                }
                self.pending_capture = None;
                match result {
                    Ok(outcome) => {
                        self.frame.set_snapshot(outcome.snapshot);
                        self.sync_all_viewports();
                        let id = outcome.result.zettel_id.declaration();
                        let destination = outcome.result.destination.display().to_string();
                        self.show_log_with_severity(
                            SeverityKind::Info,
                            "Capture complete",
                            format!("destination: {destination}\nzettel_id: {id}"),
                        );
                    }
                    Err(message) => {
                        self.show_log("Capture failed", message);
                    }
                }
            }
            AsyncResult::FixPreview { generation, result } => {
                if self.pending_fix_preview != Some(generation) {
                    return;
                }
                self.pending_fix_preview = None;
                match result {
                    Ok(preview) => {
                        let preview_count = preview.previews.len();
                        let has_preview = preview_count > 0;
                        self.overlay = DashboardOverlay::FixPreview(preview);
                        if has_preview {
                            self.record_status(
                                SeverityKind::Info,
                                format!("fix preview ready: {preview_count} safe preview(s)"),
                            );
                        } else {
                            self.record_status(SeverityKind::Warning, "fix preview unavailable");
                        }
                    }
                    Err(message) => {
                        self.show_log("Fix preview failed", message);
                    }
                }
            }
            AsyncResult::FixApply { generation, result } => {
                if self.pending_fix_apply != Some(generation) {
                    return;
                }
                self.pending_fix_apply = None;
                match result {
                    Ok(outcome) => {
                        let detail = format_fix_apply_detail(&outcome);
                        self.frame.set_snapshot(outcome.snapshot);
                        self.sync_all_viewports();
                        self.record_status_with_detail(
                            SeverityKind::Info,
                            "fix apply complete",
                            Some(detail),
                        );
                    }
                    Err(message) => {
                        self.show_log("Fix apply failed", message);
                    }
                }
            }
            AsyncResult::Search { generation, result } => {
                if self.pending_search != Some(generation) {
                    return;
                }
                self.pending_search = None;
                match result {
                    Ok(search) => {
                        let row_count = search.rows.len();
                        let has_error = search.error.is_some();
                        self.frame.set_query(
                            (!search.input.trim().is_empty()).then(|| search.input.clone()),
                        );
                        self.frame.set_search(search);
                        self.sync_active_viewport();
                        if has_error {
                            self.record_status(SeverityKind::Error, "search error");
                        } else {
                            self.record_status(
                                SeverityKind::Info,
                                format!("search complete: {row_count} rows"),
                            );
                        }
                    }
                    Err(message) => {
                        self.show_log("Search failed", message);
                    }
                }
            }
        }
    }

    fn show_log(&mut self, title: &str, message: String) {
        self.show_log_with_severity(SeverityKind::Error, title, message);
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

fn format_fix_apply_detail(outcome: &FixApplyOutcome) -> String {
    let rule_codes = if outcome.applied_rule_codes.is_empty() {
        "-".to_owned()
    } else {
        outcome.applied_rule_codes.join(", ")
    };
    format!(
        "path: {}\nrule_codes: {rule_codes}\napplied_edits: {}\n{}",
        outcome.changed_path.display(),
        outcome.applied_edits,
        actions::reindex_summary_line(outcome.reindex_summary.clone())
    )
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum AsyncResult {
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
    },
    FixApply {
        generation: usize,
        result: Result<FixApplyOutcome, String>,
    },
    Search {
        generation: usize,
        result: Result<SearchPanel, String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use zorg_store::StoreOptions;

    use crate::model::{
        CaptureField, DashboardSnapshot, DiagnosticRow, IndexPanel, IndexStatusRow, PanelRow,
        QueryBadge, SearchPanel, ZettelRow,
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
        app.pending_refresh = Some(3);
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
    fn failed_capture_action_shows_log_overlay() {
        let mut app = test_app(Panel::Today);
        app.pending_capture = Some(9);
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
    fn stale_search_results_are_ignored() {
        let mut app = test_app(Panel::Search);
        app.pending_search = Some(4);

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
        app.pending_fix_preview = Some(12);

        app.apply_async_result(AsyncResult::FixPreview {
            generation: 12,
            result: Ok(empty_fix_preview_overlay()),
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
        app.pending_fix_apply = Some(22);

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
        app.pending_fix_apply = Some(23);

        app.apply_async_result(AsyncResult::FixApply {
            generation: 23,
            result: Err("fix apply refused: index is stale relative to source".to_owned()),
        });

        assert_eq!(app.status(), "Fix apply failed");
        assert!(matches!(app.overlay(), DashboardOverlay::Log { .. }));
    }

    #[test]
    fn refresh_preserves_zettel_selection_by_canonical_identity_after_reorder() {
        let mut app = test_app(Panel::Today);
        app.handle_key(key(KeyCode::Down));
        assert_eq!(app.selected_index(), 1);

        app.pending_refresh = Some(7);
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

        app.pending_refresh = Some(8);
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

        diagnostics_app.pending_refresh = Some(9);
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

        index_app.pending_refresh = Some(10);
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
            search: SearchPanel::empty(""),
        }
    }

    fn zettel(store_id: i64, title: &str) -> ZettelRow {
        ZettelRow {
            store_id,
            canonical_id: Some(title.to_owned()),
            file_path: PathBuf::from(format!("/tmp/corpus/{title}.z")),
            title: title.to_owned(),
            todo_marker: None,
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

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    fn ctrl_key(character: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
    }
}
