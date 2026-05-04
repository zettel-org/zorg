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
use model::{DashboardFrame, Panel};
use ratatui::Terminal;
use ratatui::backend::{CrosstermBackend, TestBackend};
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

    let (frame, store_options) = load_frame(&options)?;
    if options.once {
        print!("{}", render_frame_to_string(&frame)?);
        return Ok(());
    }

    run_interactive(frame, store_options, &options)
}

fn load_frame(
    options: &DashOptions,
) -> Result<(DashboardFrame, zorg_store::StoreOptions), DashError> {
    let resolved = ResolvedConfig::from_env(ConfigOverrides {
        root: options.root.clone(),
        database_path: options.database_path.clone(),
    })
    .map_err(|error| DashError::Usage(error.to_string()))?;
    let store_options = resolved.store_options().clone();
    let root = store_options.corpus_root().to_path_buf();
    let database_path = store_options.database_path().to_path_buf();
    let snapshot = data::load_snapshot(store_options.clone(), options.query.as_deref());

    Ok((
        DashboardFrame::new(
            root,
            database_path,
            options.panel,
            options.query.clone(),
            snapshot,
        ),
        store_options,
    ))
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
            mouse: true,
            help: false,
        }
    }
}

impl DashOptions {
    fn parse(args: &[String]) -> Result<Self, DashError> {
        let mut options = Self::default();
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => options.help = true,
                "--once" => options.once = set_bool_once(options.once, "--once")?,
                "--no-alt-screen" => options.alt_screen = false,
                "--no-mouse" => options.mouse = false,
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

        Ok(options)
    }
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
    if !io::stdout().is_terminal() {
        print!("{}", render_frame_to_string(&frame)?);
        if let Some(exit_after) = options.exit_after {
            std::thread::sleep(exit_after);
        }
        return Ok(());
    }

    let mut guard = TerminalGuard::enter(options.alt_screen, options.mouse)?;
    let _panic_hook = PanicCleanupHook::install(options.alt_screen, options.mouse);
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(runtime_error)?;
    let started = Instant::now();
    let mut app = AppState::new(frame, store_options);

    loop {
        app.drain_worker_results();
        app.drive_search_debounce();
        terminal
            .draw(|area| {
                let visible_row_count = ui::main_visible_row_count(area.area(), app.frame());
                app.set_active_visible_row_count(visible_row_count);
                ui::render_dashboard_with_state(
                    area,
                    app.frame(),
                    app.active_render_state(),
                    app.overlay(),
                    app.status(),
                )
            })
            .map_err(runtime_error)?;

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
            }
        }
    }

    terminal.show_cursor().map_err(runtime_error)?;
    drop(guard);
    Ok(())
}

fn render_frame_to_string(frame: &DashboardFrame) -> Result<String, DashError> {
    let backend = TestBackend::new(ONCE_WIDTH, ONCE_HEIGHT);
    let mut terminal = Terminal::new(backend).map_err(runtime_error)?;
    terminal
        .draw(|area| ui::render_dashboard(area, frame))
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
                 [--no-mouse]

Launch the terminal dashboard for an indexed Zorg corpus.

Options:
  --root PATH        Override the corpus root
  --db PATH          Override the SQLite database path
  --panel PANEL      Select the initial panel
  --query QUERY      Preload the search query
  --once             Render one deterministic frame to stdout and exit
  --exit-after MS    Exit a bounded interactive run after milliseconds
  --no-alt-screen    Render without entering the terminal alt screen
  --no-mouse         Do not enable mouse capture
  -h, --help         Print help"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let rendered = render_frame_to_string(&frame).expect("render frame");

        assert!(rendered.contains("Zorg Dash"));
        assert!(rendered.contains("Panels"));
        assert!(rendered.contains("Index unavailable"));
        assert!(rendered.contains("Read-only index unavailable"));

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
        let rendered = render_frame_to_string(&frame).expect("render frame");

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
        let rendered = render_frame_to_string(&frame).expect("render frame");

        assert!(rendered.contains("Today"));
        assert!(rendered.contains("Main Today 1/"));
        assert!(rendered.contains("@work/due"));
        assert!(rendered.contains("@work/do"));

        let _ = std::fs::remove_dir_all(temp);
    }

    fn temp_path(label: &str) -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "zorg-dash-test-{}-{label}-{counter}",
            std::process::id()
        ))
    }
}
