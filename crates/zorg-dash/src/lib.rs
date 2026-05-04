//! Terminal dashboard foundation for Zorg.

mod actions;
mod app;
mod data;
mod model;
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
use model::{ColorMode, DashboardFrame, Panel};
use ratatui::Terminal;
use ratatui::backend::{Backend, CrosstermBackend, TestBackend};
use zorg_store::{ConfigOverrides, ResolvedConfig};

const ONCE_WIDTH: u16 = 100;
const ONCE_HEIGHT: u16 = 28;

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

    let config = resolve_dashboard_config(&options)?;
    if options.once || !io::stdout().is_terminal() {
        let frame = load_frame_from_config(&config, &options);
        print!("{}", render_frame_to_string(&frame, options.color_mode)?);
        if !options.once
            && let Some(exit_after) = options.exit_after
        {
            std::thread::sleep(exit_after);
        }
        return Ok(());
    }

    let frame = loading_frame_from_config(&config, &options);
    run_interactive(frame, config.store_options, &options)
}

#[cfg(test)]
fn load_frame(
    options: &DashOptions,
) -> Result<(DashboardFrame, zorg_store::StoreOptions), DashError> {
    let config = resolve_dashboard_config(options)?;
    let frame = load_frame_from_config(&config, options);
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

fn load_frame_from_config(config: &DashboardConfig, options: &DashOptions) -> DashboardFrame {
    let snapshot = data::load_snapshot(config.store_options.clone(), options.query.as_deref());

    DashboardFrame::new(
        config.root.clone(),
        config.database_path.clone(),
        options.panel,
        options.query.clone(),
        snapshot,
    )
}

fn loading_frame_from_config(config: &DashboardConfig, options: &DashOptions) -> DashboardFrame {
    DashboardFrame::new(
        config.root.clone(),
        config.database_path.clone(),
        options.panel,
        options.query.clone(),
        model::DashboardSnapshot::Loading,
    )
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct DashOptions {
    root: Option<PathBuf>,
    database_path: Option<PathBuf>,
    panel: Panel,
    query: Option<String>,
    once: bool,
    exit_after: Option<Duration>,
    alt_screen: bool,
    mouse: bool,
    color_mode: ColorMode,
    help: bool,
}

impl Default for DashOptions {
    fn default() -> Self {
        Self {
            root: None,
            database_path: None,
            panel: Panel::Today,
            query: None,
            once: false,
            exit_after: None,
            alt_screen: true,
            mouse: false,
            color_mode: ColorMode::Enabled,
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
                "--no-alt-screen" => options.alt_screen = false,
                "--mouse" => options.mouse = true,
                "--no-mouse" => options.mouse = false,
                "--no-color" => {
                    if no_color_flag_seen {
                        return Err(DashError::Usage(
                            "zorg dash accepts at most one --no-color value".to_owned(),
                        ));
                    }
                    no_color_flag_seen = true;
                    no_color = true;
                }
                "--root" | "--db" | "--panel" | "--query" | "--exit-after" => {
                    let flag = args[index].as_str();
                    index += 1;
                    let Some(value) = args.get(index) else {
                        return Err(DashError::Usage(format!("missing value for {flag}")));
                    };
                    match flag {
                        "--root" => set_path_once(&mut options.root, value, "--root")?,
                        "--db" => set_path_once(&mut options.database_path, value, "--db")?,
                        "--panel" => options.panel = parse_panel(value)?,
                        "--query" => set_string_once(&mut options.query, value, "--query")?,
                        "--exit-after" => {
                            if options.exit_after.is_some() {
                                return Err(DashError::Usage(
                                    "zorg dash accepts at most one --exit-after value".to_owned(),
                                ));
                            }
                            options.exit_after = Some(Duration::from_millis(parse_millis(value)?));
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

fn parse_millis(value: &str) -> Result<u64, DashError> {
    value.parse::<u64>().map_err(|error| {
        DashError::Usage(format!(
            "zorg dash --exit-after must be an unsigned millisecond value: {error}"
        ))
    })
}

fn parse_panel(value: &str) -> Result<Panel, DashError> {
    match value {
        "today" => Ok(Panel::Today),
        "inbox" => Ok(Panel::Inbox),
        "search" => Ok(Panel::Search),
        "diagnostics" => Ok(Panel::Diagnostics),
        "index" => Ok(Panel::Index),
        other => Err(DashError::Usage(format!(
            "unsupported dashboard panel `{other}`; expected today, inbox, search, diagnostics, or index"
        ))),
    }
}

fn run_interactive(
    frame: DashboardFrame,
    store_options: zorg_store::StoreOptions,
    options: &DashOptions,
) -> Result<(), DashError> {
    let mut guard = TerminalGuard::enter(options.alt_screen, options.mouse)?;
    let _panic_hook = PanicCleanupHook::install(options.alt_screen, options.mouse);
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(runtime_error)?;
    let started = Instant::now();
    let mut app = AppState::new(frame, store_options);
    draw_app(&mut terminal, &mut app, options.color_mode)?;
    app.start_initial_load();

    loop {
        app.drain_worker_results();
        app.drive_search_debounce();
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
    Ok(())
}

fn draw_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut AppState,
    color_mode: ColorMode,
) -> Result<(), DashError> {
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
                 [--panel today|inbox|search|diagnostics|index]
                 [--query @id|SWOG]
                 [--once]
                 [--exit-after MS]
                 [--no-alt-screen]
                 [--mouse]
                 [--no-mouse]
                 [--no-color]

Launch the terminal dashboard for an indexed Zorg corpus.

Options:
  --root PATH        Override the corpus root
  --db PATH          Override the SQLite database path
  --panel PANEL      Select the initial panel
  --query QUERY      Preload the search query
  --once             Render one deterministic frame to stdout and exit
  --exit-after MS    Exit a bounded interactive run after milliseconds
  --no-alt-screen    Render without entering the terminal alt screen
  --mouse            Enable terminal mouse capture
  --no-mouse         Keep terminal mouse capture disabled
  --no-color         Disable foreground and background colors
  -h, --help         Print help

Interactive keys include y to yank a row ID, source link, or diagnostic message."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use std::fmt::Write as _;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zorg_store::{Store, StoreOptions};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

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
    fn parse_rejects_duplicate_no_color() {
        let error = DashOptions::parse(&["--no-color".to_owned(), "--no-color".to_owned()])
            .expect_err("duplicate --no-color should fail");
        assert!(error.to_string().contains("at most one --no-color"));
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
            assert!(narrow.contains(panel.label()));
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
            5,
            "baseline load should collect previews for three Today queries, Inbox, and Search"
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

    struct OverflowCorpus {
        temp: PathBuf,
        root: PathBuf,
        db: PathBuf,
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

    fn temp_path(label: &str) -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "zorg-dash-test-{}-{label}-{counter}",
            std::process::id()
        ))
    }
}
