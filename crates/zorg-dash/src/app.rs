use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use zorg_store::StoreOptions;

use crate::actions::{self, CaptureOutcome, ReindexOutcome};
use crate::model::{
    CaptureDraft, DashboardFrame, DashboardOverlay, DashboardRenderState, DashboardSnapshot, Panel,
    PanelRow, PanelRowId, SearchPanel, SourceLocation,
};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
const DEFAULT_VISIBLE_ROW_COUNT: usize = 10;

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
    status: String,
    generation: usize,
    pending_refresh: Option<usize>,
    pending_reindex: Option<usize>,
    pending_capture: Option<usize>,
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
            status: String::new(),
            generation: 0,
            pending_refresh: None,
            pending_reindex: None,
            pending_capture: None,
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

    pub(crate) fn status(&self) -> &str {
        &self.status
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

        if matches!(self.overlay, DashboardOverlay::Capture(_)) {
            return self.handle_capture_key(key);
        }

        if self.search_editing {
            return self.handle_search_key(key);
        }

        if !matches!(self.overlay, DashboardOverlay::None) {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')
            ) {
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
                self.status =
                    "search edit: type SWOG or @query/id, enter runs, esc stops".to_owned();
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
                self.status = "editor returned".to_owned();
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
                self.status = "reindex canceled".to_owned();
            }
            _ => {}
        }
        AppCommand::Continue
    }

    fn handle_capture_key(&mut self, key: KeyEvent) -> AppCommand {
        match key.code {
            KeyCode::Esc => {
                self.overlay = DashboardOverlay::None;
                self.status = "capture canceled".to_owned();
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
                self.status = "search edit stopped".to_owned();
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

    fn open_capture_flow(&mut self) {
        match actions::capture_defaults(self.store_options.corpus_root()) {
            Ok(defaults) => {
                self.overlay = DashboardOverlay::Capture(CaptureDraft::new(
                    defaults.template,
                    defaults.destination,
                ));
                self.status = "capture edit: tab fields, enter creates, esc cancels".to_owned();
            }
            Err(message) => {
                self.show_log("Capture unavailable", message);
            }
        }
    }

    fn with_capture_draft(&mut self, update: impl FnOnce(&mut CaptureDraft)) {
        if let DashboardOverlay::Capture(draft) = &mut self.overlay {
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
            self.status = "refresh already running".to_owned();
            return;
        }
        let generation = self.next_generation();
        self.pending_refresh = Some(generation);
        self.status = "refresh running".to_owned();
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
            self.status = "search cleared".to_owned();
        } else {
            self.search_due_at = Some(Instant::now() + SEARCH_DEBOUNCE);
            self.status = "search pending".to_owned();
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
            self.status = "search cleared".to_owned();
            return;
        }

        let generation = self.next_generation();
        self.pending_search = Some(generation);
        self.status = "search running".to_owned();
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
            self.status = "reindex already running".to_owned();
            return;
        }
        let generation = self.next_generation();
        self.pending_reindex = Some(generation);
        self.status = "reindex running".to_owned();
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
            self.status = "capture already running".to_owned();
            return;
        }
        let generation = self.next_generation();
        self.pending_capture = Some(generation);
        self.status = "capture running".to_owned();
        let options = self.store_options.clone();
        let query = self.frame.query.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            let result = actions::capture(options, query, draft);
            let _ = sender.send(AsyncResult::Capture { generation, result });
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
                self.status = "refresh complete".to_owned();
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
                        self.status = actions::reindex_summary_line(outcome.summary);
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
                        self.status = format!("capture complete: {id}");
                        self.show_log(
                            "Capture complete",
                            format!("destination: {destination}\nzettel_id: {id}"),
                        );
                    }
                    Err(message) => {
                        self.show_log("Capture failed", message);
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
                        self.status = if has_error {
                            "search error".to_owned()
                        } else {
                            format!("search complete: {row_count} rows")
                        };
                    }
                    Err(message) => {
                        self.show_log("Search failed", message);
                    }
                }
            }
        }
    }

    fn show_log(&mut self, title: &str, message: String) {
        self.status = title.to_owned();
        self.overlay = DashboardOverlay::Log {
            title: title.to_owned(),
            message,
        };
    }
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
        DiagnosticRow {
            id,
            severity: "warning".to_owned(),
            category: "semantic".to_owned(),
            code: Some("reference.missing".to_owned()),
            message: message.to_owned(),
            relative_path: Some(PathBuf::from(path)),
            start_line: Some(1),
            start_column: Some(1),
            zettel_id: None,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    fn ctrl_key(character: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
    }
}
