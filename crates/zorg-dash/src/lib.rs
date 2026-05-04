//! Terminal dashboard foundation for Zorg.

mod actions;
mod app;
mod data;
mod json;
mod model;
mod state;
mod ui;

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::panic;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use app::{AppCommand, AppState};
use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, poll, read};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use model::{AutoRefreshConfig, ColorMode, DashboardFrame, Panel, PanelId};
use ratatui::Terminal;
use ratatui::backend::{Backend, CrosstermBackend, TestBackend};
use zorg_store::{ConfigOverrides, ResolvedConfig};

const ONCE_WIDTH: u16 = 100;
const ONCE_HEIGHT: u16 = 28;
const MIN_AUTO_REFRESH_INTERVAL: Duration = Duration::from_millis(1_000);

/// Runs `zorg dash` and returns the intended process exit code.
#[must_use]
pub fn run(args: Vec<String>) -> i32 {
    match run_inner(args) {
        Ok(()) => 0,
        Err(DashError::Usage(message)) => {
            eprintln!("{message}");
            2
        }
        Err(DashError::Runtime(message)) => {
            eprintln!("{message}");
            1
        }
    }
}

fn run_inner(args: Vec<String>) -> Result<(), DashError> {
    let options = DashOptions::parse(&args)?;
    if options.help {
        print_help();
        return Ok(());
    }
    if options.auto_refresh.is_some() && options.once {
        return Err(DashError::Usage(
            "zorg dash --auto-refresh requires interactive mode; omit --once".to_owned(),
        ));
    }
    if options.json && !options.once {
        return Err(DashError::Usage(
            "zorg dash --json requires --once; dashboard JSON is a one-shot frame export"
                .to_owned(),
        ));
    }
    if options.auto_refresh.is_some() && !io::stdout().is_terminal() {
        return Err(DashError::Usage(
            "zorg dash --auto-refresh requires an interactive terminal".to_owned(),
        ));
    }

    let config = resolve_dashboard_config(&options)?;
    if options.once || !io::stdout().is_terminal() {
        let frame = load_frame_from_config(&config, &options)?;
        if options.json {
            println!("{}", render_frame_to_json(&frame)?);
        } else {
            print!("{}", render_frame_to_string(&frame, options.color_mode)?);
        }
        if !options.once
            && let Some(exit_after) = options.exit_after
        {
            std::thread::sleep(exit_after);
        }
        return Ok(());
    }

    let mut options = options;
    let (state_path, state_notice) = if options.no_state {
        (None, None)
    } else {
        match resolve_state_path(&options) {
            Ok(path) => {
                let notice = apply_dashboard_state(&path, &mut options);
                (Some(path), notice)
            }
            Err(message) => (None, Some(format!("dashboard state disabled: {message}"))),
        }
    };

    let frame = loading_frame_from_config(&config, &options);
    run_interactive(
        frame,
        config.store_options,
        &options,
        state_path,
        state_notice,
    )
}

#[cfg(test)]
fn load_frame(
    options: &DashOptions,
) -> Result<(DashboardFrame, zorg_store::StoreOptions), DashError> {
    let config = resolve_dashboard_config(options)?;
    let frame = load_frame_from_config(&config, options)?;
    Ok((frame, config.store_options))
}

#[derive(Debug, Clone)]
struct DashboardConfig {
    store_options: zorg_store::StoreOptions,
    root: PathBuf,
    database_path: PathBuf,
}

fn resolve_dashboard_config(options: &DashOptions) -> Result<DashboardConfig, DashError> {
    let resolved = ResolvedConfig::from_env(ConfigOverrides {
        root: options.root.clone(),
        database_path: options.database_path.clone(),
    })
    .map_err(|error| DashError::Usage(error.to_string()))?;
    let store_options = resolved.store_options().clone();
    let root = store_options.corpus_root().to_path_buf();
    let database_path = store_options.database_path().to_path_buf();
    Ok(DashboardConfig {
        store_options,
        root,
        database_path,
    })
}

fn load_frame_from_config(
    config: &DashboardConfig,
    options: &DashOptions,
) -> Result<DashboardFrame, DashError> {
    let started = Instant::now();
    let snapshot = data::load_snapshot(
        config.store_options.clone(),
        options.query.as_deref(),
        options.dashboard_id.as_deref(),
    );

    let mut frame = DashboardFrame::new_with_dashboard(
        config.root.clone(),
        config.database_path.clone(),
        options.panel,
        options.custom_panel.clone(),
        options.dashboard_id.clone(),
        options.query.clone(),
        snapshot,
    );
    validate_selected_panel(&frame, options)?;
    frame.set_auto_refresh_config(options.auto_refresh);
    frame.record_initial_load_duration(started.elapsed());
    enrich_selected_graph_context(&mut frame, config.store_options.clone(), 0);
    Ok(frame)
}

fn enrich_selected_graph_context(
    frame: &mut DashboardFrame,
    store_options: zorg_store::StoreOptions,
    selected_index: usize,
) {
    if let Some(row) = frame.selected_zettel_row(selected_index) {
        let row_id = row.row_id();
        let graph = data::load_graph_neighborhood(store_options, &row);
        frame.set_graph_context(row_id, graph);
    }
}

fn loading_frame_from_config(config: &DashboardConfig, options: &DashOptions) -> DashboardFrame {
    let mut frame = DashboardFrame::new_with_dashboard(
        config.root.clone(),
        config.database_path.clone(),
        options.panel,
        options.custom_panel.clone(),
        options.dashboard_id.clone(),
        options.query.clone(),
        model::DashboardSnapshot::Loading,
    );
    frame.set_auto_refresh_config(options.auto_refresh);
    frame
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct DashOptions {
    root: Option<PathBuf>,
    database_path: Option<PathBuf>,
    panel: Panel,
    custom_panel: Option<String>,
    dashboard_id: Option<String>,
    query: Option<String>,
    once: bool,
    exit_after: Option<Duration>,
    alt_screen: bool,
    mouse: bool,
    auto_refresh: Option<AutoRefreshConfig>,
    color_mode: ColorMode,
    json: bool,
    no_state: bool,
    state_path: Option<PathBuf>,
    panel_seen: bool,
    dashboard_id_seen: bool,
    query_seen: bool,
    mouse_seen: bool,
    auto_refresh_seen: bool,
    restored_search_history: Vec<String>,
    help: bool,
}

impl Default for DashOptions {
    fn default() -> Self {
        Self {
            root: None,
            database_path: None,
            panel: Panel::Today,
            custom_panel: None,
            dashboard_id: None,
            query: None,
            once: false,
            exit_after: None,
            alt_screen: true,
            mouse: false,
            auto_refresh: None,
            color_mode: ColorMode::Enabled,
            json: false,
            no_state: false,
            state_path: None,
            panel_seen: false,
            dashboard_id_seen: false,
            query_seen: false,
            mouse_seen: false,
            auto_refresh_seen: false,
            restored_search_history: Vec::new(),
            help: false,
        }
    }
}

impl DashOptions {
    fn parse(args: &[String]) -> Result<Self, DashError> {
        let mut options = Self::default();
        let mut no_color = no_color_env_is_set();
        let mut no_color_flag_seen = false;
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => options.help = true,
                "--once" => options.once = set_bool_once(options.once, "--once")?,
                "--json" => options.json = set_bool_once(options.json, "--json")?,
                "--no-state" => options.no_state = set_bool_once(options.no_state, "--no-state")?,
                "--no-alt-screen" => options.alt_screen = false,
                "--mouse" => {
                    options.mouse_seen = true;
                    options.mouse = true;
                }
                "--no-mouse" => {
                    options.mouse_seen = true;
                    options.mouse = false;
                }
                "--no-auto-refresh" => {
                    options.auto_refresh_seen = true;
                    options.auto_refresh = None;
                }
                "--no-color" => {
                    if no_color_flag_seen {
                        return Err(DashError::Usage(
                            "zorg dash accepts at most one --no-color value".to_owned(),
                        ));
                    }
                    no_color_flag_seen = true;
                    no_color = true;
                }
                "--root" | "--db" | "--panel" | "--as" | "--query" | "--state" | "--exit-after"
                | "--auto-refresh" => {
                    let flag = args[index].as_str();
                    index += 1;
                    let Some(value) = args.get(index) else {
                        return Err(DashError::Usage(format!("missing value for {flag}")));
                    };
                    match flag {
                        "--root" => set_path_once(&mut options.root, value, "--root")?,
                        "--db" => set_path_once(&mut options.database_path, value, "--db")?,
                        "--panel" => match parse_panel(value) {
                            Some(panel) => {
                                if options.panel_seen {
                                    return Err(DashError::Usage(
                                        "zorg dash accepts at most one --panel value".to_owned(),
                                    ));
                                }
                                options.panel_seen = true;
                                options.panel = panel;
                                options.custom_panel = None;
                            }
                            None => {
                                if options.panel_seen {
                                    return Err(DashError::Usage(
                                        "zorg dash accepts at most one --panel value".to_owned(),
                                    ));
                                }
                                options.panel_seen = true;
                                set_string_once(&mut options.custom_panel, value, "--panel")?;
                            }
                        },
                        "--as" => {
                            options.dashboard_id_seen = true;
                            set_string_once(&mut options.dashboard_id, value, "--as")?;
                        }
                        "--query" => {
                            options.query_seen = true;
                            set_string_once(&mut options.query, value, "--query")?;
                        }
                        "--state" => set_path_once(&mut options.state_path, value, "--state")?,
                        "--exit-after" => {
                            if options.exit_after.is_some() {
                                return Err(DashError::Usage(
                                    "zorg dash accepts at most one --exit-after value".to_owned(),
                                ));
                            }
                            options.exit_after =
                                Some(Duration::from_millis(parse_millis(value, flag)?));
                        }
                        "--auto-refresh" => {
                            if options.auto_refresh.is_some() {
                                return Err(DashError::Usage(
                                    "zorg dash accepts at most one --auto-refresh value".to_owned(),
                                ));
                            }
                            options.auto_refresh_seen = true;
                            let interval = Duration::from_millis(parse_millis(value, flag)?);
                            if interval < MIN_AUTO_REFRESH_INTERVAL {
                                return Err(DashError::Usage(format!(
                                    "zorg dash --auto-refresh must be at least {}ms",
                                    MIN_AUTO_REFRESH_INTERVAL.as_millis()
                                )));
                            }
                            options.auto_refresh = Some(AutoRefreshConfig::new(interval));
                        }
                        _ => unreachable!("matched dash flag"),
                    }
                }
                argument if argument.starts_with('-') => {
                    return Err(DashError::Usage(format!(
                        "unexpected argument for `zorg dash`: {argument}"
                    )));
                }
                argument => {
                    return Err(DashError::Usage(format!(
                        "unexpected positional argument for `zorg dash`: {argument}"
                    )));
                }
            }
            index += 1;
        }

        if options.custom_panel.is_some() && options.dashboard_id.is_none() {
            return Err(DashError::Usage(format!(
                "unsupported dashboard panel `{}`; expected today, inbox, queries, search, diagnostics, or index; custom panel keys require --as @dashboard/id",
                options.custom_panel.as_deref().unwrap_or_default()
            )));
        }
        options.color_mode = ColorMode::from_disabled(no_color);
        Ok(options)
    }
}

fn no_color_env_is_set() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|value| !value.as_os_str().is_empty())
}

fn set_bool_once(current: bool, flag: &str) -> Result<bool, DashError> {
    if current {
        return Err(DashError::Usage(format!(
            "zorg dash accepts at most one {flag} value"
        )));
    }
    Ok(true)
}

fn set_path_once(slot: &mut Option<PathBuf>, value: &str, flag: &str) -> Result<(), DashError> {
    if slot.replace(PathBuf::from(value)).is_some() {
        return Err(DashError::Usage(format!(
            "zorg dash accepts at most one {flag} value"
        )));
    }
    Ok(())
}

fn set_string_once(slot: &mut Option<String>, value: &str, flag: &str) -> Result<(), DashError> {
    if slot.replace(value.to_owned()).is_some() {
        return Err(DashError::Usage(format!(
            "zorg dash accepts at most one {flag} value"
        )));
    }
    Ok(())
}

fn parse_millis(value: &str, flag: &str) -> Result<u64, DashError> {
    value.parse::<u64>().map_err(|error| {
        DashError::Usage(format!(
            "zorg dash {flag} must be an unsigned millisecond value: {error}"
        ))
    })
}

fn parse_panel(value: &str) -> Option<Panel> {
    match value {
        "today" => Some(Panel::Today),
        "inbox" => Some(Panel::Inbox),
        "queries" => Some(Panel::Queries),
        "search" => Some(Panel::Search),
        "diagnostics" => Some(Panel::Diagnostics),
        "index" => Some(Panel::Index),
        _ => None,
    }
}

fn validate_selected_panel(frame: &DashboardFrame, options: &DashOptions) -> Result<(), DashError> {
    let Some(custom_key) = options.custom_panel.as_deref() else {
        return Ok(());
    };
    if frame
        .panels
        .iter()
        .any(|panel| panel.id == PanelId::Custom(custom_key.to_owned()))
    {
        return Ok(());
    }
    Err(DashError::Usage(format!(
        "unsupported dashboard panel `{custom_key}` for {}; available panels: {}",
        options
            .dashboard_id
            .as_deref()
            .unwrap_or("selected dashboard"),
        frame
            .panels
            .iter()
            .map(|panel| panel.key().to_owned())
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

fn resolve_state_path(options: &DashOptions) -> Result<PathBuf, String> {
    options
        .state_path
        .clone()
        .map(Ok)
        .unwrap_or_else(state::default_state_path)
}

fn apply_dashboard_state(path: &std::path::Path, options: &mut DashOptions) -> Option<String> {
    match state::load(path) {
        state::StateLoadOutcome::Missing => None,
        state::StateLoadOutcome::Ignored(message) => {
            Some(format!("dashboard state ignored: {message}"))
        }
        state::StateLoadOutcome::Restored(restored) => {
            if !options.dashboard_id_seen && options.dashboard_id.is_none() {
                options.dashboard_id = restored.selected_dashboard_id;
            }
            if !options.panel_seen
                && let Some(panel_id) = restored.selected_panel
            {
                match panel_id {
                    PanelId::BuiltIn(panel) => {
                        options.panel = panel;
                        options.custom_panel = None;
                    }
                    PanelId::Custom(key) if options.dashboard_id.is_some() => {
                        options.custom_panel = Some(key);
                    }
                    PanelId::Custom(_) => {}
                }
            }
            if !options.query_seen && options.query.is_none() {
                options.query = restored.search_query;
            }
            if !options.mouse_seen
                && let Some(mouse) = restored.mouse
            {
                options.mouse = mouse;
            }
            if !options.auto_refresh_seen && options.auto_refresh.is_none() {
                options.auto_refresh = restored
                    .auto_refresh
                    .filter(|config| config.interval >= MIN_AUTO_REFRESH_INTERVAL);
            }
            options.restored_search_history = restored.search_history;
            None
        }
    }
}

fn run_interactive(
    frame: DashboardFrame,
    store_options: zorg_store::StoreOptions,
    options: &DashOptions,
    state_path: Option<PathBuf>,
    state_notice: Option<String>,
) -> Result<(), DashError> {
    let mut guard = TerminalGuard::enter(options.alt_screen, options.mouse)?;
    let _panic_hook = PanicCleanupHook::install(options.alt_screen, options.mouse);
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(runtime_error)?;
    let started = Instant::now();
    let mut app = AppState::new(frame, store_options);
    app.restore_search_history(options.restored_search_history.clone());
    if let Some(notice) = state_notice {
        app.record_state_notice(notice);
    }
    draw_app(&mut terminal, &mut app, options.color_mode)?;
    app.start_initial_load();

    loop {
        app.drain_worker_results();
        app.drive_search_debounce();
        app.drive_freshness_check();
        app.drive_auto_refresh();
        draw_app(&mut terminal, &mut app, options.color_mode)?;
        app.advance_activity_tick();

        if options
            .exit_after
            .is_some_and(|limit| started.elapsed() >= limit)
        {
            break;
        }

        let timeout = options
            .exit_after
            .map(|limit| {
                limit
                    .saturating_sub(started.elapsed())
                    .min(Duration::from_millis(50))
            })
            .unwrap_or_else(|| Duration::from_millis(100));
        if poll(timeout).map_err(runtime_error)?
            && let Event::Key(key) = read().map_err(runtime_error)?
        {
            match app.handle_key(key) {
                AppCommand::Continue => {}
                AppCommand::Quit => break,
                AppCommand::Open(location) => {
                    terminal.show_cursor().map_err(runtime_error)?;
                    guard.suspend()?;
                    let result = actions::open_in_editor(&location);
                    guard.resume()?;
                    app.record_open_result(result);
                }
                AppCommand::Yank(request) => {
                    let result =
                        actions::copy_to_clipboard(&request.value, io::stdout().is_terminal());
                    app.record_yank_result(request, result);
                }
            }
        }
    }

    terminal.show_cursor().map_err(runtime_error)?;
    drop(guard);
    if let Some(path) = state_path {
        let state = app.persisted_state(options.mouse);
        if let Err(message) = state::save(&path, &state) {
            eprintln!("zorg dash: {message}");
        }
    }
    Ok(())
}

fn draw_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut AppState,
    color_mode: ColorMode,
) -> Result<(), DashError> {
    app.ensure_selected_graph_context();
    terminal
        .draw(|area| {
            let visible_row_count = ui::main_visible_row_count(area.area(), app.frame());
            app.set_active_visible_row_count(visible_row_count);
            let pending_activity = app.pending_activity();
            ui::render_dashboard_with_activity_and_color(
                area,
                app.frame(),
                app.active_render_state(),
                app.overlay(),
                app.latest_status_event(),
                app.status_events(),
                pending_activity.as_ref(),
                color_mode,
            )
        })
        .map(|_| ())
        .map_err(runtime_error)
}

fn render_frame_to_string(
    frame: &DashboardFrame,
    color_mode: ColorMode,
) -> Result<String, DashError> {
    let backend = TestBackend::new(ONCE_WIDTH, ONCE_HEIGHT);
    let mut terminal = Terminal::new(backend).map_err(runtime_error)?;
    terminal
        .draw(|area| {
            ui::render_dashboard_with_state_and_color(
                area,
                frame,
                model::DashboardRenderState::for_frame(frame),
                &model::DashboardOverlay::None,
                None,
                &[],
                color_mode,
            )
        })
        .map_err(runtime_error)?;
    Ok(ui::buffer_to_string(terminal.backend().buffer()))
}

fn render_frame_to_json(frame: &DashboardFrame) -> Result<String, DashError> {
    let render_state = model::DashboardRenderState::for_frame(frame);
    json::serialize_frame(
        frame,
        render_state,
        frame.graph_context_for_selection(render_state.selected_index),
    )
    .map_err(runtime_error)
}

struct TerminalGuard {
    alt_screen: bool,
    mouse: bool,
    active: bool,
}

impl TerminalGuard {
    fn enter(alt_screen: bool, mouse: bool) -> Result<Self, DashError> {
        enable_raw_mode().map_err(runtime_error)?;
        let mut stdout = io::stdout();
        if alt_screen {
            execute!(stdout, EnterAlternateScreen, Hide).map_err(runtime_error)?;
        }
        if mouse {
            execute!(stdout, EnableMouseCapture).map_err(runtime_error)?;
        }
        Ok(Self {
            alt_screen,
            mouse,
            active: true,
        })
    }

    fn cleanup(&mut self) {
        if !self.active {
            return;
        }
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        if self.mouse {
            let _ = execute!(stdout, DisableMouseCapture);
        }
        if self.alt_screen {
            let _ = execute!(stdout, Show, LeaveAlternateScreen);
        }
        let _ = stdout.flush();
        self.active = false;
    }

    fn suspend(&mut self) -> Result<(), DashError> {
        self.cleanup();
        Ok(())
    }

    fn resume(&mut self) -> Result<(), DashError> {
        if self.active {
            return Ok(());
        }
        enable_raw_mode().map_err(runtime_error)?;
        let mut stdout = io::stdout();
        if self.alt_screen {
            execute!(stdout, EnterAlternateScreen, Hide).map_err(runtime_error)?;
        }
        if self.mouse {
            execute!(stdout, EnableMouseCapture).map_err(runtime_error)?;
        }
        stdout.flush().map_err(runtime_error)?;
        self.active = true;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.cleanup();
    }
}

type PanicHook = Box<dyn Fn(&panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

struct PanicCleanupHook {
    previous: Option<PanicHook>,
}

impl PanicCleanupHook {
    fn install(alt_screen: bool, mouse: bool) -> Self {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let mut stdout = io::stdout();
            if mouse {
                let _ = execute!(stdout, DisableMouseCapture);
            }
            if alt_screen {
                let _ = execute!(stdout, Show, LeaveAlternateScreen);
            }
            let _ = stdout.flush();
            eprintln!("{info}");
        }));
        Self {
            previous: Some(previous),
        }
    }
}

impl Drop for PanicCleanupHook {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            panic::set_hook(previous);
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum DashError {
    Usage(String),
    Runtime(String),
}

impl fmt::Display for DashError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) | Self::Runtime(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for DashError {}

fn runtime_error(error: impl fmt::Display) -> DashError {
    DashError::Runtime(error.to_string())
}

fn print_help() {
    println!(
        "\
Usage: zorg dash [--root PATH] [--db PATH]
                 [--panel today|inbox|queries|search|diagnostics|index]
                 [--as @dashboard/id]
                 [--query @id|SWOG]
                 [--once]
                 [--json]
                 [--exit-after MS]
                 [--auto-refresh MS]
                 [--no-auto-refresh]
                 [--no-state]
                 [--state PATH]
                 [--no-alt-screen]
                 [--mouse]
                 [--no-mouse]
                 [--no-color]

Launch the terminal dashboard for an indexed Zorg corpus.

Options:
  --root PATH        Override the corpus root
  --db PATH          Override the SQLite database path
  --panel PANEL      Select the initial panel; custom keys require --as
  --as ID            Load a #z/dashboard zettel by canonical ID
  --query QUERY      Preload the search query
  --once             Render one deterministic frame to stdout and exit
  --json             With --once, emit the frame as compact JSON
  --exit-after MS    Exit a bounded interactive run after milliseconds
  --auto-refresh MS  Refresh while idle at a conservative interval (minimum 1000)
  --no-auto-refresh  Disable idle auto-refresh
  --no-state         Disable interactive dashboard state load and save
  --state PATH       Use an alternate interactive dashboard state file
  --no-alt-screen    Render without entering the terminal alt screen
  --mouse            Enable terminal mouse capture
  --no-mouse         Keep terminal mouse capture disabled
  --no-color         Disable foreground and background colors
  -h, --help         Print help

Interactive keys include y to yank a row ID, source link, or diagnostic message and F1 from Search for SWOG help."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use std::fmt::Write as _;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use zorg_store::{Store, StoreOptions};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);
    static PANIC_HOOK_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn parse_rejects_unknown_panel() {
        let error = DashOptions::parse(&["--panel".to_owned(), "bad".to_owned()])
            .expect_err("invalid panel should fail");
        assert!(error.to_string().contains("unsupported dashboard panel"));
    }

    #[test]
    fn parse_accepts_no_color() {
        let options = DashOptions::parse(&["--no-color".to_owned()]).expect("parse --no-color");
        assert_eq!(options.color_mode, ColorMode::Disabled);
    }

    #[test]
    fn parse_accepts_queries_panel() {
        let options =
            DashOptions::parse(&["--panel".to_owned(), "queries".to_owned()]).expect("parse");
        assert_eq!(options.panel, Panel::Queries);
    }

    #[test]
    fn parse_accepts_state_flags() {
        let options = DashOptions::parse(&[
            "--state".to_owned(),
            "dash-state.json".to_owned(),
            "--no-state".to_owned(),
        ])
        .expect("parse state flags");

        assert_eq!(options.state_path, Some(PathBuf::from("dash-state.json")));
        assert!(options.no_state);
    }

    #[test]
    fn parse_accepts_custom_panel_only_with_dashboard_selector() {
        let options = DashOptions::parse(&[
            "--as".to_owned(),
            "@dashboards/daily".to_owned(),
            "--panel".to_owned(),
            "open".to_owned(),
        ])
        .expect("parse custom panel");

        assert_eq!(options.dashboard_id.as_deref(), Some("@dashboards/daily"));
        assert_eq!(options.custom_panel.as_deref(), Some("open"));

        let error = DashOptions::parse(&["--panel".to_owned(), "open".to_owned()])
            .expect_err("custom panel without --as should fail");
        assert!(error.to_string().contains("custom panel keys require --as"));
    }

    #[test]
    fn parse_defaults_mouse_capture_off_and_accepts_mouse_flags() {
        let options = DashOptions::parse(&[]).expect("parse defaults");
        assert!(!options.mouse);

        let options = DashOptions::parse(&["--mouse".to_owned()]).expect("parse --mouse");
        assert!(options.mouse);

        let options = DashOptions::parse(&["--mouse".to_owned(), "--no-mouse".to_owned()])
            .expect("parse --no-mouse after --mouse");
        assert!(!options.mouse);
    }

    #[test]
    fn parse_accepts_auto_refresh_with_minimum_interval() {
        let options =
            DashOptions::parse(&["--auto-refresh".to_owned(), "1000".to_owned()]).expect("parse");

        assert_eq!(
            options.auto_refresh.map(|config| config.interval),
            Some(Duration::from_millis(1_000))
        );
    }

    #[test]
    fn parse_rejects_too_small_auto_refresh_interval() {
        let error = DashOptions::parse(&["--auto-refresh".to_owned(), "999".to_owned()])
            .expect_err("small auto refresh interval should fail");

        assert!(error.to_string().contains("must be at least 1000ms"));
    }

    #[test]
    fn state_restore_supplies_interactive_defaults() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("state.json");
        fs::write(
            &path,
            r##"{
              "schema":"zorg.dash.state",
              "version":1,
              "selected_panel":"open",
              "search_query":"#z/inbox",
              "search_history":["#z/todo"],
              "selected_dashboard_id":"@dashboards/daily",
              "preferences":{"mouse":true,"auto_refresh_ms":2000}
            }"##,
        )
        .expect("write state");
        let mut options = DashOptions::parse(&[]).expect("parse");

        let notice = apply_dashboard_state(&path, &mut options);

        assert!(notice.is_none());
        assert_eq!(options.dashboard_id.as_deref(), Some("@dashboards/daily"));
        assert_eq!(options.custom_panel.as_deref(), Some("open"));
        assert_eq!(options.query.as_deref(), Some("#z/inbox"));
        assert_eq!(options.restored_search_history, vec!["#z/todo".to_owned()]);
        assert!(options.mouse);
        assert_eq!(
            options.auto_refresh.map(|config| config.interval),
            Some(Duration::from_millis(2_000))
        );
    }

    #[test]
    fn state_restore_respects_cli_overrides() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("state.json");
        fs::write(
            &path,
            r##"{
              "schema":"zorg.dash.state",
              "version":1,
              "selected_panel":"open",
              "search_query":"#z/inbox",
              "search_history":["#z/todo"],
              "selected_dashboard_id":"@dashboards/daily",
              "preferences":{"mouse":true,"auto_refresh_ms":2000}
            }"##,
        )
        .expect("write state");
        let mut options = DashOptions::parse(&[
            "--panel".to_owned(),
            "search".to_owned(),
            "--query".to_owned(),
            "#z/ref".to_owned(),
            "--as".to_owned(),
            "@dashboards/explicit".to_owned(),
            "--no-mouse".to_owned(),
            "--no-auto-refresh".to_owned(),
        ])
        .expect("parse");

        let notice = apply_dashboard_state(&path, &mut options);

        assert!(notice.is_none());
        assert_eq!(
            options.dashboard_id.as_deref(),
            Some("@dashboards/explicit")
        );
        assert_eq!(options.panel, Panel::Search);
        assert_eq!(options.custom_panel, None);
        assert_eq!(options.query.as_deref(), Some("#z/ref"));
        assert!(!options.mouse);
        assert_eq!(options.auto_refresh, None);
    }

    #[test]
    fn once_rejects_auto_refresh() {
        let error = run_inner(vec![
            "--once".to_owned(),
            "--auto-refresh".to_owned(),
            "1000".to_owned(),
        ])
        .expect_err("one-shot auto refresh should fail");

        assert!(
            error
                .to_string()
                .contains("--auto-refresh requires interactive mode")
        );
    }

    #[test]
    fn parse_rejects_duplicate_no_color() {
        let error = DashOptions::parse(&["--no-color".to_owned(), "--no-color".to_owned()])
            .expect_err("duplicate --no-color should fail");
        assert!(error.to_string().contains("at most one --no-color"));
    }

    #[test]
    fn parse_accepts_json_flag() {
        let options = DashOptions::parse(&["--json".to_owned()]).expect("parse --json");
        assert!(options.json);
    }

    #[test]
    fn parse_rejects_duplicate_json() {
        let error = DashOptions::parse(&["--json".to_owned(), "--json".to_owned()])
            .expect_err("duplicate --json should fail");
        assert!(error.to_string().contains("at most one --json"));
    }

    #[test]
    fn json_requires_once() {
        let error =
            run_inner(vec!["--json".to_owned()]).expect_err("interactive json export should fail");

        assert!(error.to_string().contains("--json requires --once"));
    }

    #[test]
    fn once_frame_renders_degraded_index_state() {
        let temp = temp_path("degraded");
        let root = temp.join("corpus");
        let db = temp.join("zorg.sqlite3");
        std::fs::create_dir_all(&root).expect("create root");

        let options = DashOptions {
            root: Some(root),
            database_path: Some(db),
            panel: Panel::Index,
            once: true,
            ..DashOptions::default()
        };
        let (frame, _) = load_frame(&options).expect("load degraded frame");
        let rendered = render_frame_to_string(&frame, ColorMode::Enabled).expect("render frame");

        assert!(rendered.contains("Zorg Dash"));
        assert!(rendered.contains("Panels"));
        assert!(rendered.contains("Index unavailable"));
        assert!(rendered.contains("Read-only index unavailable"));
        assert!(rendered.contains("Root:"));
        assert!(rendered.contains("Database:"));
        assert!(rendered.contains("zorg db reindex"));
        assert!(rendered.contains("read-only"));

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn once_frame_serializes_degraded_index_state_as_json() {
        let temp = temp_path("degraded-json");
        let root = temp.join("corpus");
        let db = temp.join("zorg.sqlite3");
        std::fs::create_dir_all(&root).expect("create root");

        let options = DashOptions {
            root: Some(root),
            database_path: Some(db.clone()),
            panel: Panel::Index,
            once: true,
            json: true,
            ..DashOptions::default()
        };
        let (frame, _) = load_frame(&options).expect("load degraded frame");
        let json = render_frame_to_json(&frame).expect("render json frame");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("dashboard json should parse");

        assert_eq!(value["schema"], "zorg.dash.frame");
        assert_eq!(value["active_panel"], "index");
        assert_eq!(
            value["database_path"],
            db.to_string_lossy().replace('\\', "/")
        );
        assert_eq!(value["snapshot"]["state"], "degraded");
        assert!(
            value["snapshot"]["error"]
                .as_str()
                .is_some_and(|error| !error.is_empty())
        );

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn loading_frame_resolves_config_without_loading_snapshot() {
        let temp = temp_path("loading");
        let root = temp.join("corpus");
        let db = temp.join("zorg.sqlite3");
        std::fs::create_dir_all(&root).expect("create root");

        let options = DashOptions {
            root: Some(root),
            database_path: Some(db),
            panel: Panel::Today,
            ..DashOptions::default()
        };
        data::reset_preview_collection_requests();
        let config = resolve_dashboard_config(&options).expect("resolve config");
        let frame = loading_frame_from_config(&config, &options);

        assert!(matches!(frame.snapshot, model::DashboardSnapshot::Loading));
        assert_eq!(data::preview_collection_requests(), 0);

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn once_frame_renders_read_only_index_metadata() {
        let temp = temp_path("indexed");
        let root = temp.join("corpus");
        let db = temp.join("zorg.sqlite3");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(
            root.join("daily.z"),
            "%%% @daily #z/ref\nDaily\n%%%\n\n- @daily/task #z/todo [ ]\n",
        )
        .expect("write source");
        let options = StoreOptions::new(&root, &db).expect("store options");
        let mut store = Store::open_with_options(options).expect("open writable store");
        store.reindex().expect("reindex");

        let options = DashOptions {
            root: Some(root),
            database_path: Some(db),
            panel: Panel::Index,
            once: true,
            ..DashOptions::default()
        };
        let (frame, _) = load_frame(&options).expect("load indexed frame");
        assert!(frame.is_ready());
        let rendered = render_frame_to_string(&frame, ColorMode::Enabled).expect("render frame");

        assert!(rendered.contains("Discovered files"));
        assert!(rendered.contains("1"));
        assert!(rendered.contains("Schema version"));
        assert!(rendered.contains("2"));
        assert!(
            !rendered.contains("\x1b["),
            "--once output should remain ANSI-free"
        );

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn once_frame_renders_today_rows_and_diagnostics() {
        let temp = temp_path("today");
        let root = temp.join("corpus");
        let db = temp.join("zorg.sqlite3");
        std::fs::create_dir_all(&root).expect("create root");
        let today = data::current_query_date();
        let today = format!("{:04}-{:02}-{:02}", today.year, today.month, today.day);
        std::fs::write(
            root.join("work.z"),
            format!(
                "\
%%% @work #z/ref
Work
%%%

- @work/due #z/todo [ ] due::{today}
  Due today.

- @work/do #z/todo [N] do::{today}
  Do today.

See #missing.
"
            ),
        )
        .expect("write source");
        let options = StoreOptions::new(&root, &db).expect("store options");
        let mut store = Store::open_with_options(options).expect("open writable store");
        store.reindex().expect("reindex");

        let options = DashOptions {
            root: Some(root),
            database_path: Some(db),
            panel: Panel::Today,
            once: true,
            ..DashOptions::default()
        };
        let (frame, _) = load_frame(&options).expect("load today frame");
        let rendered = render_frame_to_string(&frame, ColorMode::Enabled).expect("render frame");

        assert!(rendered.contains("Today"));
        assert!(rendered.contains("Main Today 1/"));
        assert!(rendered.contains("@work/due"));
        assert!(rendered.contains("@work/do"));

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn overflow_corpus_indexes_and_renders_all_panels() {
        let corpus = OverflowCorpus::generate("render", 96);
        let mut frame = load_overflow_frame(&corpus, Panel::Today);

        let today_visible = ui::main_visible_row_count(Rect::new(0, 0, 100, 28), &frame);
        assert!(
            frame.rows_for_panel(Panel::Today).len() > today_visible,
            "overflow fixture should exceed the default once-frame main area"
        );

        for panel in Panel::ALL {
            frame.panel = panel;
            let rendered = render_frame_at_size(&frame, 100, 28).expect("render standard frame");
            assert_panel_render(&rendered, panel);

            let narrow = render_frame_at_size(&frame, 64, 28).expect("render narrow frame");
            assert!(narrow.contains("Zorg Dash"));
            assert!(narrow.contains("Panels"));
            assert!(narrow.contains("Main"));
            assert!(narrow.contains("Inspector"));
            assert!(narrow.contains("Keys"));
            assert!(narrow.contains(panel.label()));
        }
    }

    #[test]
    fn overflow_corpus_panel_frames_match_golden_regions() {
        let corpus = OverflowCorpus::generate("golden", 72);
        let mut frame = load_overflow_frame(&corpus, Panel::Today);
        let cases: [(Panel, &[&str]); 6] = [
            (
                Panel::Today,
                &[
                    "index current diagnostics 2 freshness current marked 0 panel today rows T/I/Q/S/D/X",
                    "Main Today 1/",
                    "> Today",
                    "Today: combined rows",
                    "> [ ] @dash-overflow/inbox-due-000",
                    "Today queries:",
                ],
            ),
            (
                Panel::Inbox,
                &[
                    "freshness current marked 0 panel inbox rows T/I/Q/S/D/X",
                    "Main Inbox 1/",
                    "> Inbox",
                    "> [ ] @dash-overflow/inbox-due-000",
                    "Tags: #z/inbox, #z/todo",
                ],
            ),
            (
                Panel::Queries,
                &[
                    "freshness current marked 0 panel queries rows T/I/Q/S/D/X",
                    "Main Queries 1/2",
                    "> Queries",
                    "> ok @dash-overflow/queries/inbox",
                    "Source: query:: property",
                    "Definition: #z/inbox",
                ],
            ),
            (
                Panel::Search,
                &[
                    "freshness current marked 0 panel search rows T/I/Q/S/D/X",
                    "Main Search 1/",
                    "> Search",
                    "Query: #z/inbox",
                    "> [ ] @dash-overflow/inbox-due-000",
                ],
            ),
            (
                Panel::Diagnostics,
                &[
                    "freshness current marked 0 panel diagnostics rows T/I/Q/S/D/X",
                    "Main Diagnostics 1/2",
                    "> Diagnostics",
                    "> error reference.unresolved_absolute",
                    "Severity: error",
                    "Code: reference.unresolved_absolute",
                ],
            ),
            (
                Panel::Index,
                &[
                    "freshness current marked 0 panel index rows T/I/Q/S/D/X",
                    "Main Index 1/8",
                    "> Index",
                    "Schema version: 2",
                    "Freshness: current",
                    "Telemetry",
                ],
            ),
        ];

        for (panel, expected_regions) in cases {
            frame.panel = panel;
            let rendered = render_frame_at_size(&frame, 100, 28).expect("render golden frame");
            let normalized = compact_frame_text(&rendered);
            assert_contains_regions(&normalized, expected_regions, panel);
            assert!(normalized.contains("q quit"), "{panel:?}\n{normalized}");
            assert!(normalized.contains("y yank"), "{panel:?}\n{normalized}");
        }
    }

    #[test]
    fn snapshot_load_counts_preview_collection_requests() {
        let corpus = OverflowCorpus::generate("instrument", 48);
        data::reset_preview_collection_requests();

        let frame = load_overflow_frame(&corpus, Panel::Today);

        assert!(frame.is_ready());
        assert_eq!(
            data::preview_collection_requests(),
            1,
            "snapshot load should reuse one preview collection for Today, Inbox, and Search"
        );
        assert!(frame.rows_for_panel(Panel::Today).len() > 24);
        assert!(frame.rows_for_panel(Panel::Inbox).len() > 0);
        assert!(frame.rows_for_panel(Panel::Search).len() > 0);
        let diagnostics = frame.rows_for_panel(Panel::Diagnostics);
        assert!(diagnostics.len() > 0);
        assert!(
            diagnostics
                .iter()
                .any(|row| row.list_line().contains("reference.unresolved"))
        );
        assert_eq!(frame.rows_for_panel(Panel::Index).len(), 8);
    }

    #[test]
    fn standalone_search_counts_one_preview_collection_request() {
        let corpus = OverflowCorpus::generate("standalone-search", 32);
        data::reset_preview_collection_requests();

        let options = StoreOptions::new(&corpus.root, &corpus.db).expect("store options");
        let search =
            data::load_search_panel(options, "#z/inbox").expect("load standalone search panel");

        assert!(search.error.is_none());
        assert!(!search.rows.is_empty());
        assert_eq!(
            data::preview_collection_requests(),
            1,
            "standalone search should build at most one preview collection"
        );
    }

    #[test]
    fn queries_panel_renders_valid_and_invalid_query_rows() {
        let temp = temp_path("queries-panel");
        let root = temp.join("corpus");
        let db = temp.join("zorg.sqlite3");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(
            root.join("queries.z"),
            "\
%%% @root #z/ref
Root
%%%

- @tasks/open #z/todo [ ] Open task.
- @tasks/inbox #z/inbox Inbox task.
- @queries/open #z/query title::Open query query::#z/todo
- @queries/fenced #z/query title::Inbox query
  ```swog
  #z/inbox
  ```
- @queries/bad #z/query title::Broken query query::todo:[A]
",
        )
        .expect("write query source");
        let options = StoreOptions::new(&root, &db).expect("store options");
        let mut store = Store::open_with_options(options).expect("open writable store");
        store.reindex().expect("reindex");

        let search = data::load_search_panel(
            StoreOptions::new(&root, &db).expect("search options"),
            "@queries/bad",
        )
        .expect("load stored query search panel");
        let query_info = search.query_info.expect("stored query metadata");
        assert_eq!(query_info.id, "queries/bad");
        assert_eq!(
            query_info.source_path.as_deref(),
            Some(std::path::Path::new("queries.z"))
        );
        assert!(
            query_info
                .definition_error
                .as_deref()
                .is_some_and(|error| error.contains("@queries/bad") && error.contains("queries.z"))
        );

        let options = DashOptions {
            root: Some(root.clone()),
            database_path: Some(db),
            panel: Panel::Queries,
            once: true,
            ..DashOptions::default()
        };
        let (frame, _) = load_frame(&options).expect("load frame");
        let rows = frame.rows_for_panel(Panel::Queries);
        assert_eq!(rows.len(), 3);
        assert!(matches!(&rows[0], model::PanelRow::Query(row) if row.valid));
        assert!(matches!(&rows[1], model::PanelRow::Query(row) if row.valid));
        assert!(matches!(&rows[2], model::PanelRow::Query(row) if !row.valid));

        let rendered = render_frame_at_size(&frame, 100, 28).expect("render queries panel");
        assert!(rendered.contains("> Queries"));
        assert!(rendered.contains("@queries/open"));
        assert!(rendered.contains("@queries/fenced"));
        assert!(rendered.contains("warning @queries/bad"));
        assert!(rendered.contains("Source: query:: property"));
        assert!(rendered.contains("Definition: #z/todo"));

        let fenced = frame.inspector_lines_for_selection(1).join("\n");
        assert!(fenced.contains("Source: fenced swog"));
        assert!(fenced.contains("Definition: #z/inbox"));

        let invalid = frame.inspector_lines_for_selection(2).join("\n");
        assert!(invalid.contains("Status: invalid"));
        assert!(invalid.contains("invalid query definition"));

        let narrow = render_frame_at_size(&frame, 58, 20).expect("render narrow queries panel");
        assert!(narrow.contains("Queries"));
        assert!(narrow.contains("q quit"));

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn dashboard_selector_builds_dynamic_panel_registry() {
        let corpus = DashboardCorpus::generate("dynamic-registry");
        let options = DashOptions {
            root: Some(corpus.root.clone()),
            database_path: Some(corpus.db.clone()),
            dashboard_id: Some("@dashboards/daily".to_owned()),
            once: true,
            ..DashOptions::default()
        };
        let (frame, _) = load_frame(&options).expect("load dashboard frame");

        let keys = frame
            .panels
            .iter()
            .map(|panel| panel.key())
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec![
                "today",
                "inbox",
                "queries",
                "search",
                "diagnostics",
                "index",
                "open",
                "table"
            ]
        );
        let rendered = render_frame_at_size(&frame, 100, 28).expect("render dynamic frame");
        assert!(rendered.contains("Open"));
        assert!(rendered.contains("Table"));
    }

    #[test]
    fn dashboard_selector_accepts_custom_initial_panel_and_json_exports_it() {
        let corpus = DashboardCorpus::generate("custom-panel");
        let options = DashOptions {
            root: Some(corpus.root.clone()),
            database_path: Some(corpus.db.clone()),
            dashboard_id: Some("@dashboards/daily".to_owned()),
            custom_panel: Some("open".to_owned()),
            once: true,
            json: true,
            ..DashOptions::default()
        };
        let (frame, _) = load_frame(&options).expect("load custom panel frame");
        assert_eq!(
            frame.active_panel_id(),
            model::PanelId::Custom("open".to_owned())
        );

        let json = render_frame_to_json(&frame).expect("render dashboard json");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("dashboard json should parse");
        assert_eq!(value["active_panel"], "open");
        assert_eq!(value["selected_dashboard"]["id"], "dashboards/daily");
        assert!(
            value["panels"]
                .as_array()
                .expect("panels array")
                .iter()
                .any(|panel| panel["panel"] == "open"
                    && panel["title"] == "Open"
                    && panel["custom"]["query_source"] == "@queries/open"
                    && panel["custom"]["has_error"] == false)
        );
        assert_eq!(value["active_panel_rows"][0]["kind"], "zettel");
        assert_eq!(value["active_panel_rows"][0]["canonical_id"], "todos/one");
        assert_eq!(value["snapshot"]["metrics"]["custom_rows"], 2);
    }

    #[test]
    fn dashboard_selector_rejects_unknown_custom_panel_with_available_keys() {
        let corpus = DashboardCorpus::generate("invalid-custom-panel");
        let options = DashOptions {
            root: Some(corpus.root.clone()),
            database_path: Some(corpus.db.clone()),
            dashboard_id: Some("@dashboards/daily".to_owned()),
            custom_panel: Some("missing".to_owned()),
            once: true,
            ..DashOptions::default()
        };
        let error = load_frame(&options).expect_err("unknown custom panel should fail");

        assert!(
            error
                .to_string()
                .contains("unsupported dashboard panel `missing`")
        );
        assert!(error.to_string().contains("today"));
        assert!(error.to_string().contains("open"));
        assert!(error.to_string().contains("table"));
    }

    #[test]
    fn panic_cleanup_hook_restores_previous_hook_on_drop() {
        let _lock = PANIC_HOOK_TEST_LOCK.lock().expect("lock panic hook test");
        let original = panic::take_hook();
        let calls = Arc::new(AtomicUsize::new(0));
        let hook_calls = Arc::clone(&calls);
        panic::set_hook(Box::new(move |_| {
            hook_calls.fetch_add(1, Ordering::SeqCst);
        }));

        {
            let _cleanup = PanicCleanupHook::install(false, false);
        }

        let panic_result = panic::catch_unwind(|| panic!("panic hook restore probe"));
        let calls_seen = calls.load(Ordering::SeqCst);
        let _restored_test_hook = panic::take_hook();
        panic::set_hook(original);

        assert!(panic_result.is_err());
        assert_eq!(calls_seen, 1);
    }

    struct OverflowCorpus {
        temp: PathBuf,
        root: PathBuf,
        db: PathBuf,
    }

    struct DashboardCorpus {
        temp: PathBuf,
        root: PathBuf,
        db: PathBuf,
    }

    impl DashboardCorpus {
        fn generate(label: &str) -> Self {
            let temp = temp_path(label);
            let root = temp.join("corpus");
            let db = temp.join("zorg.sqlite3");
            std::fs::create_dir_all(&root).expect("create dashboard root");
            std::fs::write(root.join("dashboard.z"), dashboard_source())
                .expect("write dashboard source");

            let options = StoreOptions::new(&root, &db).expect("store options");
            let mut store = Store::open_with_options(options).expect("open writable store");
            store.reindex().expect("reindex dashboard corpus");

            Self { temp, root, db }
        }
    }

    impl Drop for DashboardCorpus {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.temp);
        }
    }

    fn dashboard_source() -> &'static str {
        "\
%%% @dashboards/daily #z/dashboard title::Daily
Daily dashboard.
%%%

- @dashboards/daily/open #z/panel key::open title::Open query::@queries/open

- @dashboards/daily/table #z/panel key::table title::Table
  ```swog
  TABLE #z/todo
  ```

- @queries/open #z/query title::Open query::#z/todo

- @todos/one #z/todo [ ] One task.
"
    }

    impl OverflowCorpus {
        fn generate(label: &str, row_count: usize) -> Self {
            let temp = temp_path(label);
            let root = temp.join("corpus");
            let db = temp.join("zorg.sqlite3");
            std::fs::create_dir_all(&root).expect("create overflow root");
            std::fs::write(
                root.join("overflow.z"),
                overflow_source(row_count, data::current_query_date()),
            )
            .expect("write overflow source");

            let options = StoreOptions::new(&root, &db).expect("store options");
            let mut store = Store::open_with_options(options).expect("open writable store");
            store.reindex().expect("reindex overflow corpus");

            Self { temp, root, db }
        }
    }

    impl Drop for OverflowCorpus {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.temp);
        }
    }

    fn overflow_source(row_count: usize, today: zorg_query::QueryDate) -> String {
        let today = format!("{:04}-{:02}-{:02}", today.year, today.month, today.day);
        let mut source = String::from(
            "\
%%% @dash-overflow #z/ref area::test
Dashboard overflow fixture
%%%

Root text references #dash-overflow/missing-link to produce a deterministic diagnostic.

- @dash-overflow/diagnostic #z/todo [ ] due::not-a-date area::test
  Diagnostic row with an invalid date property.

",
        );

        for index in 0..row_count {
            match index % 4 {
                0 => {
                    writeln!(
                        source,
                        "- @dash-overflow/inbox-due-{index:03} #z/inbox #z/todo [ ] due::{today} area::test\n  Inbox due task {index:03} with alpha overflow preview text.\n"
                    )
                    .expect("write source");
                }
                1 => {
                    writeln!(
                        source,
                        "- @dash-overflow/do-{index:03} #z/todo [N] do::{today} area::test\n  Scheduled do task {index:03} with beta overflow preview text.\n"
                    )
                    .expect("write source");
                }
                2 => {
                    writeln!(
                        source,
                        "- @dash-overflow/open-{index:03} #z/todo [ ] area::test\n  Open todo task {index:03} with gamma overflow preview text.\n"
                    )
                    .expect("write source");
                }
                _ => {
                    writeln!(
                        source,
                        "- @dash-overflow/inbox-ref-{index:03} #z/inbox #z/ref area::test\n  Inbox reference {index:03} with delta overflow preview text.\n"
                    )
                    .expect("write source");
                }
            }
        }

        source.push_str(
            "\
- @dash-overflow/queries/inbox #z/query title::Inbox query query::#z/inbox

- @dash-overflow/queries/todos #z/query title::Todo table
  ```swog
  TABLE #z/todo
  ```

",
        );

        source
    }

    fn load_overflow_frame(corpus: &OverflowCorpus, panel: Panel) -> DashboardFrame {
        let options = DashOptions {
            root: Some(corpus.root.clone()),
            database_path: Some(corpus.db.clone()),
            panel,
            query: Some("#z/inbox".to_owned()),
            once: true,
            ..DashOptions::default()
        };
        let (frame, _) = load_frame(&options).expect("load overflow frame");
        frame
    }

    fn render_frame_at_size(
        frame: &DashboardFrame,
        width: u16,
        height: u16,
    ) -> Result<String, DashError> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).map_err(runtime_error)?;
        terminal
            .draw(|area| {
                ui::render_dashboard_with_state_and_color(
                    area,
                    frame,
                    model::DashboardRenderState::for_frame(frame),
                    &model::DashboardOverlay::None,
                    None,
                    &[],
                    ColorMode::Enabled,
                )
            })
            .map_err(runtime_error)?;
        Ok(ui::buffer_to_string(terminal.backend().buffer()))
    }

    fn assert_panel_render(rendered: &str, panel: Panel) {
        assert!(rendered.contains("Zorg Dash"));
        assert!(rendered.contains("Panels"));
        assert!(rendered.contains(panel.label()));

        match panel {
            Panel::Today => {
                assert!(rendered.contains("Today: combined"));
                assert!(rendered.contains("@dash-overflow"));
            }
            Panel::Inbox => {
                assert!(rendered.contains("@dash-overflow/inbox"));
            }
            Panel::Queries => {
                assert!(rendered.contains("@dash-overflow/queries"));
            }
            Panel::Search => {
                assert!(rendered.contains("Query: #z/inbox"));
                assert!(rendered.contains("@dash-overflow/inbox"));
            }
            Panel::Diagnostics => {
                assert!(rendered.contains("reference.unresolved"));
            }
            Panel::Index => {
                assert!(rendered.contains("Schema version"));
                assert!(rendered.contains("Discovered files"));
            }
        }
    }

    fn compact_frame_text(rendered: &str) -> String {
        rendered
            .lines()
            .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn assert_contains_regions(rendered: &str, expected: &[&str], panel: Panel) {
        for needle in expected {
            if !rendered.contains(needle) {
                panic!("missing golden region for {panel:?}: {needle}\n{rendered}");
            }
        }
    }

    fn temp_path(label: &str) -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "zorg-dash-test-{}-{label}-{counter}",
            std::process::id()
        ))
    }
}
