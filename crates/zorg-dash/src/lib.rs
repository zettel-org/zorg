//! Terminal dashboard foundation for Zorg.

use std::fmt;
use std::io::{self, IsTerminal, Write};
use std::panic;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode, poll, read};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use zorg_store::{ConfigOverrides, ResolvedConfig, Store};

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

    let frame = DashboardFrame::load(&options)?;
    if options.once {
        print!("{}", render_frame_to_string(&frame)?);
        return Ok(());
    }

    run_interactive(&frame, &options)
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Panel {
    Today,
    Inbox,
    Search,
    Diagnostics,
    Index,
}

impl Panel {
    const ALL: [Self; 5] = [
        Self::Today,
        Self::Inbox,
        Self::Search,
        Self::Diagnostics,
        Self::Index,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Inbox => "Inbox",
            Self::Search => "Search",
            Self::Diagnostics => "Diagnostics",
            Self::Index => "Index",
        }
    }

    const fn value(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Inbox => "inbox",
            Self::Search => "search",
            Self::Diagnostics => "diagnostics",
            Self::Index => "index",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct DashboardFrame {
    root: PathBuf,
    database_path: PathBuf,
    panel: Panel,
    query: Option<String>,
    store_state: StoreState,
}

impl DashboardFrame {
    fn load(options: &DashOptions) -> Result<Self, DashError> {
        let resolved = ResolvedConfig::from_env(ConfigOverrides {
            root: options.root.clone(),
            database_path: options.database_path.clone(),
        })
        .map_err(|error| DashError::Usage(error.to_string()))?;
        let store_options = resolved.store_options().clone();
        let root = store_options.corpus_root().to_path_buf();
        let database_path = store_options.database_path().to_path_buf();
        let store_state = match Store::open_read_only_with_options(store_options) {
            Ok(store) => StoreState::load(&store),
            Err(error) => StoreState::Degraded {
                message: error.to_string(),
            },
        };

        Ok(Self {
            root,
            database_path,
            panel: options.panel,
            query: options.query.clone(),
            store_state,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum StoreState {
    Ready {
        schema_version: i64,
        discovered_files: usize,
        indexed_files: usize,
        changed_files: usize,
        new_files: usize,
        deleted_files: usize,
        diagnostic_count: usize,
        last_indexed_at_unix_ms: Option<i64>,
    },
    Degraded {
        message: String,
    },
}

impl StoreState {
    fn load(store: &Store) -> Self {
        let schema_version = match store.schema_version() {
            Ok(version) => version,
            Err(error) => {
                return Self::Degraded {
                    message: error.to_string(),
                };
            }
        };
        let status = match store.index_status() {
            Ok(status) => status,
            Err(error) => {
                return Self::Degraded {
                    message: error.to_string(),
                };
            }
        };

        Self::Ready {
            schema_version,
            discovered_files: status.discovered_files,
            indexed_files: status.indexed_files,
            changed_files: status.changed_files,
            new_files: status.new_files,
            deleted_files: status.deleted_files,
            diagnostic_count: status.diagnostic_count,
            last_indexed_at_unix_ms: status.last_indexed_at_unix_ms,
        }
    }

    fn health_label(&self) -> &'static str {
        match self {
            Self::Degraded { .. } => "degraded",
            Self::Ready {
                last_indexed_at_unix_ms: None,
                ..
            } => "missing",
            Self::Ready {
                changed_files,
                new_files,
                deleted_files,
                ..
            } if *changed_files > 0 || *new_files > 0 || *deleted_files > 0 => "stale",
            Self::Ready { .. } => "current",
        }
    }

    fn diagnostics_label(&self) -> String {
        match self {
            Self::Ready {
                diagnostic_count, ..
            } => diagnostic_count.to_string(),
            Self::Degraded { .. } => "unknown".to_owned(),
        }
    }
}

fn run_interactive(frame: &DashboardFrame, options: &DashOptions) -> Result<(), DashError> {
    if !io::stdout().is_terminal() {
        print!("{}", render_frame_to_string(frame)?);
        if let Some(exit_after) = options.exit_after {
            std::thread::sleep(exit_after);
        }
        return Ok(());
    }

    let guard = TerminalGuard::enter(options.alt_screen, options.mouse)?;
    let _panic_hook = PanicCleanupHook::install(options.alt_screen, options.mouse);
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(runtime_error)?;
    let started = Instant::now();

    loop {
        terminal
            .draw(|area| render_dashboard(area, frame))
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
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        {
            break;
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
        .draw(|area| render_dashboard(area, frame))
        .map_err(runtime_error)?;
    Ok(buffer_to_string(terminal.backend().buffer()))
}

fn buffer_to_string(buffer: &Buffer) -> String {
    let mut output = String::new();
    for y in buffer.area.top()..buffer.area.bottom() {
        let mut line = String::new();
        for x in buffer.area.left()..buffer.area.right() {
            let cell = &buffer[(x, y)];
            line.push_str(cell.symbol());
        }
        output.push_str(line.trim_end());
        output.push('\n');
    }
    output
}

fn render_dashboard(frame_area: &mut ratatui::Frame<'_>, frame: &DashboardFrame) {
    let root = frame_area.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(root);

    render_status(frame_area, vertical[0], frame);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(18),
            Constraint::Percentage(54),
            Constraint::Percentage(46),
        ])
        .split(vertical[1]);

    render_nav(frame_area, body[0], frame.panel);
    render_main(frame_area, body[1], frame);
    render_inspector(frame_area, body[2], frame);
    render_footer(frame_area, vertical[2]);
}

fn render_status(terminal_frame: &mut ratatui::Frame<'_>, area: Rect, frame: &DashboardFrame) {
    let status = vec![Line::from(vec![
        Span::raw("root "),
        Span::styled(
            frame.root.display().to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  db "),
        Span::styled(
            frame.database_path.display().to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  index "),
        Span::styled(
            frame.store_state.health_label(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  diagnostics "),
        Span::styled(
            frame.store_state.diagnostics_label(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  panel "),
        Span::styled(
            frame.panel.value(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
    ])];
    terminal_frame.render_widget(
        Paragraph::new(status).block(Block::default().title("Zorg Dash").borders(Borders::ALL)),
        area,
    );
}

fn render_nav(terminal_frame: &mut ratatui::Frame<'_>, area: Rect, active: Panel) {
    let items = Panel::ALL
        .iter()
        .map(|panel| {
            if *panel == active {
                ListItem::new(Line::from(vec![
                    Span::raw("> "),
                    Span::styled(panel.label(), Style::default().add_modifier(Modifier::BOLD)),
                ]))
            } else {
                ListItem::new(Line::from(vec![Span::raw("  "), Span::raw(panel.label())]))
            }
        })
        .collect::<Vec<_>>();
    terminal_frame.render_widget(
        List::new(items).block(Block::default().title("Panels").borders(Borders::ALL)),
        area,
    );
}

fn render_main(terminal_frame: &mut ratatui::Frame<'_>, area: Rect, frame: &DashboardFrame) {
    let mut lines = vec![
        Line::from(Span::styled(
            format!("{} panel", frame.panel.label()),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Panel data arrives in the next dashboard phase."),
    ];
    if frame.panel == Panel::Search {
        lines.push(Line::from(format!(
            "Initial query: {}",
            frame.query.as_deref().unwrap_or("")
        )));
    }
    if frame.panel == Panel::Index {
        lines.push(Line::from(""));
        match &frame.store_state {
            StoreState::Ready {
                discovered_files,
                indexed_files,
                changed_files,
                new_files,
                deleted_files,
                ..
            } => {
                lines.push(Line::from(format!("Discovered files: {discovered_files}")));
                lines.push(Line::from(format!("Indexed files: {indexed_files}")));
                lines.push(Line::from(format!(
                    "Pending changes: new={new_files} changed={changed_files} deleted={deleted_files}"
                )));
            }
            StoreState::Degraded { message } => {
                lines.push(Line::from("Read-only index unavailable."));
                lines.push(Line::from(message.as_str()));
            }
        }
    }

    terminal_frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().title("Main").borders(Borders::ALL)),
        area,
    );
}

fn render_inspector(terminal_frame: &mut ratatui::Frame<'_>, area: Rect, frame: &DashboardFrame) {
    let lines = match &frame.store_state {
        StoreState::Ready {
            schema_version,
            discovered_files,
            indexed_files,
            changed_files,
            new_files,
            deleted_files,
            diagnostic_count,
            last_indexed_at_unix_ms,
        } => vec![
            Line::from("Index metadata"),
            Line::from(format!("Schema version: {schema_version}")),
            Line::from(format!("Discovered files: {discovered_files}")),
            Line::from(format!("Indexed files: {indexed_files}")),
            Line::from(format!("New files: {new_files}")),
            Line::from(format!("Changed files: {changed_files}")),
            Line::from(format!("Deleted files: {deleted_files}")),
            Line::from(format!("Diagnostics: {diagnostic_count}")),
            Line::from(format!(
                "Last indexed: {}",
                last_indexed_at_unix_ms
                    .map(|timestamp| timestamp.to_string())
                    .unwrap_or_else(|| "never".to_owned())
            )),
        ],
        StoreState::Degraded { message } => vec![
            Line::from("Read-only index unavailable"),
            Line::from(""),
            Line::from(message.as_str()),
        ],
    };

    terminal_frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().title("Inspector").borders(Borders::ALL)),
        area,
    );
}

fn render_footer(terminal_frame: &mut ratatui::Frame<'_>, area: Rect) {
    terminal_frame.render_widget(
        Paragraph::new("q quit  r refresh  R reindex  enter open  / search  c capture")
            .block(Block::default().title("Keys").borders(Borders::ALL)),
        area,
    );
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
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.cleanup();
    }
}

struct PanicCleanupHook {
    previous: Option<Box<dyn Fn(&panic::PanicHookInfo<'_>) + Sync + Send + 'static>>,
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
        let frame = DashboardFrame::load(&options).expect("load degraded frame");
        let rendered = render_frame_to_string(&frame).expect("render frame");

        assert!(rendered.contains("Zorg Dash"));
        assert!(rendered.contains("Panels"));
        assert!(rendered.contains("Index panel"));
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
            "%%% @daily #z/ref\nDaily\n%%%\n\n- @daily/task #z/todo\n",
        )
        .expect("write source");
        let options = zorg_store::StoreOptions::new(&root, &db).expect("store options");
        let mut store = Store::open_with_options(options).expect("open writable store");
        store.reindex().expect("reindex");

        let options = DashOptions {
            root: Some(root),
            database_path: Some(db),
            panel: Panel::Index,
            once: true,
            ..DashOptions::default()
        };
        let frame = DashboardFrame::load(&options).expect("load indexed frame");
        assert!(matches!(
            frame.store_state,
            StoreState::Ready {
                schema_version: 2,
                discovered_files: 1,
                indexed_files: 1,
                ..
            }
        ));
        let rendered = render_frame_to_string(&frame).expect("render frame");

        assert!(rendered.contains("Discovered files: 1"));
        assert!(rendered.contains("Indexed files: 1"));
        assert!(rendered.contains("Schema version: 2"));

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
