use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use zorg_store::StoreOptions;

use crate::actions::{self, ReindexOutcome};
use crate::model::{
    DashboardFrame, DashboardOverlay, DashboardSnapshot, Panel, PanelRow, SearchPanel,
    SourceLocation,
};

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum AppCommand {
    Continue,
    Quit,
    Open(SourceLocation),
}

#[derive(Debug)]
pub(crate) struct AppState {
    frame: DashboardFrame,
    store_options: StoreOptions,
    selected_by_panel: [usize; Panel::ALL.len()],
    overlay: DashboardOverlay,
    status: String,
    generation: usize,
    pending_refresh: Option<usize>,
    pending_reindex: Option<usize>,
    pending_search: Option<usize>,
    search_due_at: Option<Instant>,
    search_editing: bool,
    sender: Sender<AsyncResult>,
    receiver: Receiver<AsyncResult>,
}

impl AppState {
    pub(crate) fn new(frame: DashboardFrame, store_options: StoreOptions) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            frame,
            store_options,
            selected_by_panel: [0; Panel::ALL.len()],
            overlay: DashboardOverlay::None,
            status: String::new(),
            generation: 0,
            pending_refresh: None,
            pending_reindex: None,
            pending_search: None,
            search_due_at: None,
            search_editing: false,
            sender,
            receiver,
        }
    }

    pub(crate) fn frame(&self) -> &DashboardFrame {
        &self.frame
    }

    pub(crate) fn selected_index(&self) -> usize {
        self.selected_by_panel[self.frame.panel.index()]
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

    fn switch_panel(&mut self, panel: Panel) {
        self.frame.panel = panel;
        self.clamp_selection();
    }

    fn move_selection(&mut self, delta: isize) {
        let count = self.active_row_count();
        if count == 0 {
            self.set_selection(0);
            return;
        }
        let current = self.selected_index() as isize;
        let max = count.saturating_sub(1) as isize;
        self.set_selection((current + delta).clamp(0, max) as usize);
    }

    fn set_selection(&mut self, index: usize) {
        self.selected_by_panel[self.frame.panel.index()] = index;
        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        let count = self.active_row_count();
        let selection = &mut self.selected_by_panel[self.frame.panel.index()];
        if count == 0 {
            *selection = 0;
        } else if *selection >= count {
            *selection = count - 1;
        }
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
        self.clamp_selection();

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
            self.clamp_selection();
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
                self.clamp_selection();
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
                        self.clamp_selection();
                        self.status = actions::reindex_summary_line(outcome.summary);
                    }
                    Err(message) => {
                        self.show_log("Reindex failed", message);
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
                        let previous = self.selected_zettel_key();
                        let row_count = search.rows.len();
                        let has_error = search.error.is_some();
                        self.frame.set_query(
                            (!search.input.trim().is_empty()).then(|| search.input.clone()),
                        );
                        self.frame.set_search(search);
                        if let Some(key) = previous.and_then(|key| self.find_zettel_key(&key)) {
                            self.set_selection(key);
                        } else {
                            self.clamp_selection();
                        }
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

    fn selected_zettel_key(&self) -> Option<(Option<String>, i64)> {
        self.frame
            .active_rows()
            .get(self.selected_index())
            .and_then(|row| {
                if let PanelRow::Zettel(row) = row {
                    Some((row.canonical_id.clone(), row.store_id))
                } else {
                    None
                }
            })
    }

    fn find_zettel_key(&self, key: &(Option<String>, i64)) -> Option<usize> {
        self.frame
            .active_rows()
            .iter()
            .position(|row| matches!(row, PanelRow::Zettel(row) if (&row.canonical_id, row.store_id) == (&key.0, key.1)))
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
        DashboardSnapshot, IndexPanel, IndexStatusRow, PanelRow, QueryBadge, SearchPanel, ZettelRow,
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

    fn test_app(panel: Panel) -> AppState {
        let root = PathBuf::from("/tmp/corpus");
        let db = PathBuf::from("/tmp/zorg.sqlite3");
        let frame = DashboardFrame::new(
            root.clone(),
            db.clone(),
            panel,
            None,
            DashboardSnapshot::Ready {
                index: IndexPanel {
                    schema_version: 2,
                    rows: vec![IndexStatusRow::new("Discovered files", 1)],
                    discovered_files: 1,
                    indexed_files: 1,
                    changed_files: 0,
                    new_files: 0,
                    deleted_files: 0,
                    diagnostic_count: 0,
                    last_indexed_at_unix_ms: Some(1),
                },
                diagnostics: Vec::new(),
                today: vec![
                    PanelRow::Zettel(zettel(1, "a")),
                    PanelRow::Zettel(zettel(2, "b")),
                ],
                inbox: vec![zettel(3, "inbox")],
                search: SearchPanel::empty(""),
            },
        );
        let options = StoreOptions::new(root, db).expect("store options");
        AppState::new(frame, options)
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

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }
}
