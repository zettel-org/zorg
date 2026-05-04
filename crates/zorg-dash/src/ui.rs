use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::model::{
    CaptureDraft, CaptureField, ColorMode, DashboardFrame, DashboardOverlay, DashboardRenderState,
    DashboardSnapshot, Panel, PanelRow, SeverityKind,
};

#[cfg(test)]
pub(crate) fn render_dashboard(frame_area: &mut ratatui::Frame<'_>, frame: &DashboardFrame) {
    render_dashboard_with_state(
        frame_area,
        frame,
        DashboardRenderState::for_frame(frame),
        &DashboardOverlay::None,
        "",
    );
}

#[cfg(test)]
pub(crate) fn render_dashboard_with_state(
    frame_area: &mut ratatui::Frame<'_>,
    frame: &DashboardFrame,
    render_state: DashboardRenderState,
    overlay: &DashboardOverlay,
    status: &str,
) {
    render_dashboard_with_state_and_color(
        frame_area,
        frame,
        render_state,
        overlay,
        status,
        ColorMode::Enabled,
    );
}

pub(crate) fn render_dashboard_with_state_and_color(
    frame_area: &mut ratatui::Frame<'_>,
    frame: &DashboardFrame,
    render_state: DashboardRenderState,
    overlay: &DashboardOverlay,
    status: &str,
    color_mode: ColorMode,
) {
    let palette = StylePalette::new(color_mode);
    let root = frame_area.area();
    let areas = dashboard_areas(root);

    render_status(frame_area, areas.status, frame, &palette);

    if root.width < 72 {
        render_nav(frame_area, areas.nav, frame.panel, &palette);
        render_main(frame_area, areas.main, frame, render_state, &palette);
        render_inspector(
            frame_area,
            areas.inspector,
            frame,
            render_state.selected_index,
            &palette,
        );
    } else {
        render_nav(frame_area, areas.nav, frame.panel, &palette);
        render_main(frame_area, areas.main, frame, render_state, &palette);
        render_inspector(
            frame_area,
            areas.inspector,
            frame,
            render_state.selected_index,
            &palette,
        );
    }
    render_footer(frame_area, areas.footer, status, &palette);

    render_overlay(frame_area, root, overlay, &palette);
}

pub(crate) fn main_visible_row_count(root: Rect, frame: &DashboardFrame) -> usize {
    let main = dashboard_areas(root).main;
    main_list_area(main, frame).height.max(1) as usize
}

pub(crate) fn buffer_to_string(buffer: &Buffer) -> String {
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct DashboardAreas {
    status: Rect,
    nav: Rect,
    main: Rect,
    inspector: Rect,
    footer: Rect,
}

fn dashboard_areas(root: Rect) -> DashboardAreas {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(root);

    if root.width < 72 {
        let body = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),
                Constraint::Percentage(56),
                Constraint::Percentage(44),
            ])
            .split(vertical[1]);
        DashboardAreas {
            status: vertical[0],
            nav: body[0],
            main: body[1],
            inspector: body[2],
            footer: vertical[2],
        }
    } else {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(18),
                Constraint::Percentage(54),
                Constraint::Percentage(46),
            ])
            .split(vertical[1]);
        DashboardAreas {
            status: vertical[0],
            nav: body[0],
            main: body[1],
            inspector: body[2],
            footer: vertical[2],
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct StylePalette {
    color_mode: ColorMode,
}

impl StylePalette {
    const fn new(color_mode: ColorMode) -> Self {
        Self { color_mode }
    }

    fn emphasis(self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    fn selection(self) -> Style {
        if self.color_mode.is_enabled() {
            self.emphasis().bg(Color::DarkGray)
        } else {
            self.emphasis()
        }
    }

    fn severity(self, severity: SeverityKind) -> Style {
        let style = self.emphasis();
        if !self.color_mode.is_enabled() {
            return style;
        }

        match severity {
            SeverityKind::Error => style.fg(Color::Red),
            SeverityKind::Warning => style.fg(Color::Yellow),
            SeverityKind::Info => style.fg(Color::Cyan),
            SeverityKind::Unknown => style.fg(Color::Magenta),
        }
    }

    fn health(self, label: &str) -> Style {
        let style = self.emphasis();
        if !self.color_mode.is_enabled() {
            return style;
        }

        match label {
            "current" => style.fg(Color::Green),
            "stale" => style.fg(Color::Yellow),
            "missing" | "degraded" => style.fg(Color::Red),
            _ => style.fg(Color::Cyan),
        }
    }

    fn index_row(self, row: &crate::model::IndexStatusRow) -> Style {
        if !self.color_mode.is_enabled() || row.value == 0 {
            return Style::default();
        }

        match row.label.as_str() {
            "Diagnostics" | "Deleted files" => self.severity(SeverityKind::Error),
            "New files" | "Changed files" => self.severity(SeverityKind::Warning),
            _ => Style::default(),
        }
    }

    fn status(self, text: &str) -> Style {
        if text.is_empty() || !self.color_mode.is_enabled() {
            return Style::default();
        }

        if text.contains("failed") || text.contains("error") {
            self.severity(SeverityKind::Error)
        } else if text.contains("canceled") || text.contains("unavailable") {
            self.severity(SeverityKind::Warning)
        } else if text.contains("complete") || text.contains("created") || text.contains("returned")
        {
            self.emphasis().fg(Color::Green)
        } else {
            Style::default().fg(Color::Cyan)
        }
    }
}

fn render_status(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    frame: &DashboardFrame,
    palette: &StylePalette,
) {
    let health_label = frame.health_label();
    let status = vec![Line::from(vec![
        Span::raw("root "),
        Span::styled(frame.root.display().to_string(), palette.emphasis()),
        Span::raw("  db "),
        Span::styled(
            frame.database_path.display().to_string(),
            palette.emphasis(),
        ),
        Span::raw("  index "),
        Span::styled(health_label, palette.health(health_label)),
        Span::raw("  diagnostics "),
        Span::styled(
            frame.diagnostics_label(),
            match &frame.snapshot {
                DashboardSnapshot::Ready { index, .. } if index.diagnostic_count > 0 => {
                    palette.severity(SeverityKind::Error)
                }
                _ => palette.emphasis(),
            },
        ),
        Span::raw("  panel "),
        Span::styled(frame.panel.value(), palette.emphasis()),
    ])];
    terminal_frame.render_widget(
        Paragraph::new(status).block(Block::default().title("Zorg Dash").borders(Borders::ALL)),
        area,
    );
}

fn render_nav(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    active: Panel,
    palette: &StylePalette,
) {
    let items = Panel::ALL
        .iter()
        .map(|panel| {
            if *panel == active {
                ListItem::new(Line::from(vec![
                    Span::raw("> "),
                    Span::styled(panel.label(), palette.selection()),
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

fn render_main(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    frame: &DashboardFrame,
    render_state: DashboardRenderState,
    palette: &StylePalette,
) {
    let title = format!(
        "Main {} {}",
        frame.panel.label(),
        render_state.position_text()
    );
    let block = Block::default().title(title).borders(Borders::ALL);
    let inner = block.inner(area);
    terminal_frame.render_widget(block, area);

    match &frame.snapshot {
        DashboardSnapshot::Degraded { message } => {
            let lines = vec![
                Line::from(Span::styled(
                    "Index unavailable",
                    palette.health("degraded"),
                )),
                Line::from(""),
                Line::from("Read-only index unavailable."),
                Line::from(message.as_str()),
            ];
            terminal_frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
        }
        DashboardSnapshot::Ready { index, .. } if frame.panel == Panel::Index => {
            let header = vec![Line::from(format!(
                "Health: {}  Schema version: {}",
                index.health_label(),
                index.schema_version
            ))];
            render_main_rows(terminal_frame, inner, frame, render_state, header, palette);
        }
        DashboardSnapshot::Ready { search, .. } => {
            let header = if frame.panel == Panel::Search {
                let mut lines = vec![Line::from(format!("Query: {}", search.input))];
                if let Some(error) = &search.error {
                    lines.push(Line::from(Span::styled(
                        format!("Error: {error}"),
                        palette.severity(SeverityKind::Error),
                    )));
                }
                lines
            } else {
                Vec::new()
            };
            render_main_rows(terminal_frame, inner, frame, render_state, header, palette);
        }
    }
}

fn render_main_rows(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    frame: &DashboardFrame,
    render_state: DashboardRenderState,
    header: Vec<Line<'static>>,
    palette: &StylePalette,
) {
    let rows = frame.active_rows();
    let list_area = render_main_header(terminal_frame, area, header);

    if rows.is_empty() {
        terminal_frame.render_widget(Paragraph::new(empty_state(frame.panel)), list_area);
        return;
    }

    let mut state = ListState::default()
        .with_selected(Some(render_state.selected_index.min(rows.len() - 1)))
        .with_offset(render_state.scroll_offset.min(rows.len() - 1));
    let items = row_items(&rows, frame.panel, render_state.selected_index, palette);
    terminal_frame.render_stateful_widget(List::new(items), list_area, &mut state);
}

fn render_main_header(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    header: Vec<Line<'static>>,
) -> Rect {
    if header.is_empty() {
        return area;
    }

    let header_height = (header.len() as u16).min(area.height);
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(header_height), Constraint::Min(0)])
        .split(area);
    terminal_frame.render_widget(Paragraph::new(header), areas[0]);
    areas[1]
}

fn main_list_area(area: Rect, frame: &DashboardFrame) -> Rect {
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let header_height = match &frame.snapshot {
        DashboardSnapshot::Ready { .. } if frame.panel == Panel::Index => 1,
        DashboardSnapshot::Ready { search, .. } if frame.panel == Panel::Search => {
            if search.error.is_some() { 2 } else { 1 }
        }
        _ => 0,
    };
    Rect {
        y: inner.y.saturating_add(header_height),
        height: inner.height.saturating_sub(header_height),
        ..inner
    }
}

fn render_inspector(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    frame: &DashboardFrame,
    selected_index: usize,
    palette: &StylePalette,
) {
    let lines = frame
        .inspector_lines_for_selection(selected_index)
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                Line::from(Span::styled(line, palette.emphasis()))
            } else {
                Line::from(line)
            }
        })
        .collect::<Vec<_>>();

    terminal_frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().title("Inspector").borders(Borders::ALL)),
        area,
    );
}

fn render_footer(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    status: &str,
    palette: &StylePalette,
) {
    let text = if status.is_empty() {
        "q quit  c capture  r refresh  R reindex  enter open  / search/edit  esc cancel  ? help"
    } else {
        status
    };
    terminal_frame.render_widget(
        Paragraph::new(Span::styled(text, palette.status(status)))
            .block(Block::default().title("Keys").borders(Borders::ALL)),
        area,
    );
}

fn row_items(
    rows: &[PanelRow],
    panel: Panel,
    selected_index: usize,
    palette: &StylePalette,
) -> Vec<ListItem<'static>> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let prefix = if index == selected_index { "> " } else { "  " };
            let row_style = row_style(row, palette);
            if index == selected_index {
                let style = row_style.patch(palette.selection());
                ListItem::new(Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(row_list_line(row, panel), style),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(row_list_line(row, panel), row_style),
                ]))
            }
        })
        .collect()
}

fn row_list_line(row: &PanelRow, panel: Panel) -> String {
    match (panel, row) {
        (Panel::Today, PanelRow::Diagnostic(row)) => {
            let code = row.code.as_deref().unwrap_or(row.category.as_str());
            format!("{:<7} {:<30} {}", row.severity, code, row.message)
        }
        _ => row.list_line(),
    }
}

fn row_style(row: &PanelRow, palette: &StylePalette) -> Style {
    match row {
        PanelRow::Diagnostic(row) => palette.severity(row.severity_kind()),
        PanelRow::IndexStatus(row) => palette.index_row(row),
        PanelRow::Zettel(_) => Style::default(),
    }
}

fn render_overlay(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    overlay: &DashboardOverlay,
    palette: &StylePalette,
) {
    let (title, lines) = match overlay {
        DashboardOverlay::None => return,
        DashboardOverlay::Help => (
            "Help",
            vec![
                Line::from("q/Esc quit or close overlay"),
                Line::from("tab/backtab switch panels"),
                Line::from("up/down/j/k move selection"),
                Line::from("g/G jump first or last row"),
                Line::from("page up/down move one page"),
                Line::from("ctrl-u/ctrl-d move half page"),
                Line::from("c capture a new zettel through zorg-capture"),
                Line::from("r refresh index snapshot"),
                Line::from("R reindex, then y/enter confirms"),
                Line::from("enter open selected source in $EDITOR"),
                Line::from("/ switch to Search and edit the query"),
                Line::from("search edit: type SWOG or @query/id, enter runs, Esc stops"),
            ],
        ),
        DashboardOverlay::ConfirmReindex => (
            "Confirm Reindex",
            vec![
                Line::from("Reindex will write a fresh SQLite snapshot for this corpus."),
                Line::from("Press y or enter to continue, n or Esc to cancel."),
            ],
        ),
        DashboardOverlay::Capture(draft) => ("Capture", capture_lines(draft, palette)),
        DashboardOverlay::Log { title, message } => (
            title.as_str(),
            message.lines().map(Line::from).collect::<Vec<_>>(),
        ),
    };

    let overlay_area = centered_rect(66, 44, area);
    terminal_frame.render_widget(Clear, overlay_area);
    terminal_frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true })
            .block(Block::default().title(title).borders(Borders::ALL)),
        overlay_area,
    );
}

fn capture_lines(draft: &CaptureDraft, palette: &StylePalette) -> Vec<Line<'static>> {
    let mut lines = CaptureField::ALL
        .iter()
        .map(|field| {
            let prefix = if *field == draft.active { "> " } else { "  " };
            let value = draft.field_value(*field);
            let value = if value.is_empty() { "-" } else { value };
            if *field == draft.active {
                Line::from(vec![
                    Span::styled(prefix, palette.selection()),
                    Span::styled(format!("{}: {}", field.label(), value), palette.selection()),
                ])
            } else {
                Line::from(format!("{prefix}{}: {value}", field.label()))
            }
        })
        .collect::<Vec<_>>();
    lines.push(Line::from(""));
    lines.push(Line::from(
        "Tab moves fields. Enter creates. Destination may be blank to use template dest::.",
    ));
    lines.push(Line::from("Esc cancels. Ctrl-u clears the active field."));
    lines
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

fn empty_state(panel: Panel) -> &'static str {
    match panel {
        Panel::Today => "No due, do, todo, or diagnostic attention rows.",
        Panel::Inbox => "No #z/inbox rows.",
        Panel::Search => "No search rows. Type / to edit a SWOG query or @query/id.",
        Panel::Diagnostics => "No indexed diagnostics.",
        Panel::Index => "No index rows.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        DashboardSnapshot, DiagnosticRow, IndexPanel, IndexStatusRow, PanelRow, QueryBadge,
        SearchPanel, ZettelRow,
    };
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;

    #[test]
    fn render_includes_status_nav_and_index_lines() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            None,
            DashboardSnapshot::Ready {
                index: Box::new(IndexPanel {
                    schema_version: 2,
                    rows: vec![IndexStatusRow::new("Discovered files", 3)],
                    discovered_files: 3,
                    indexed_files: 3,
                    changed_files: 0,
                    new_files: 0,
                    deleted_files: 0,
                    diagnostic_count: 0,
                    last_indexed_at_unix_ms: Some(42),
                }),
                diagnostics: Vec::new(),
                today: Vec::new(),
                inbox: Vec::new(),
                search: SearchPanel::empty(""),
            },
        );
        let backend = TestBackend::new(100, 28);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Zorg Dash"));
        assert!(rendered.contains("> Index"));
        assert!(rendered.contains("Discovered files"));
        assert!(rendered.contains("Schema version: 2"));
    }

    #[test]
    fn render_search_panel_includes_query_errors_inline() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Search,
            Some("OR".to_owned()),
            DashboardSnapshot::Ready {
                index: Box::new(IndexPanel {
                    schema_version: 2,
                    rows: Vec::new(),
                    discovered_files: 1,
                    indexed_files: 1,
                    changed_files: 0,
                    new_files: 0,
                    deleted_files: 0,
                    diagnostic_count: 0,
                    last_indexed_at_unix_ms: Some(42),
                }),
                diagnostics: Vec::new(),
                today: Vec::new(),
                inbox: Vec::new(),
                search: SearchPanel::with_error("OR", "query parse failed"),
            },
        );
        let backend = TestBackend::new(100, 28);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Query: OR"));
        assert!(rendered.contains("Error: query parse failed"));
    }

    #[test]
    fn render_capture_overlay_lists_form_fields() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            DashboardSnapshot::Degraded {
                message: "missing db".to_owned(),
            },
        );
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::Capture(CaptureDraft::new("@tmpl/todo", None)),
                    "",
                )
            })
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Capture"));
        assert!(rendered.contains("Template: @tmpl/todo"));
        assert!(rendered.contains("Title: -"));
        assert!(rendered.contains("Enter creates"));
    }

    #[test]
    fn render_narrow_terminal_does_not_panic_or_overlap_sections() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            None,
            DashboardSnapshot::Ready {
                index: Box::new(IndexPanel {
                    schema_version: 2,
                    rows: vec![IndexStatusRow::new("Discovered files", 3)],
                    discovered_files: 3,
                    indexed_files: 3,
                    changed_files: 0,
                    new_files: 0,
                    deleted_files: 0,
                    diagnostic_count: 0,
                    last_indexed_at_unix_ms: Some(42),
                }),
                diagnostics: Vec::new(),
                today: Vec::new(),
                inbox: Vec::new(),
                search: SearchPanel::empty(""),
            },
        );
        let backend = TestBackend::new(48, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Panels"));
        assert!(rendered.contains("Main"));
        assert!(rendered.contains("Inspector"));
        assert!(rendered.contains("Keys"));
    }

    #[test]
    fn render_main_uses_viewport_offset_and_shows_position_count() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(
                (0..12)
                    .map(|index| PanelRow::Zettel(zettel(index, &format!("row-{index}"))))
                    .collect(),
                Vec::new(),
                Vec::new(),
            ),
        );
        let backend = TestBackend::new(100, 16);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::new(10, 5, 12),
                    &DashboardOverlay::None,
                    "",
                )
            })
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Main Today 11/12"));
        assert!(rendered.contains("@row-10"));
        assert!(rendered.contains("@row-5"));
        assert!(!rendered.contains("@row-0"));
    }

    #[test]
    fn render_long_diagnostic_rows_are_clipped_to_one_row() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(
                Vec::new(),
                vec![
                    DiagnosticRow {
                        id: 1,
                        severity: "warning".to_owned(),
                        category: "semantic".to_owned(),
                        code: Some("reference.with.a.very.long.code".to_owned()),
                        message: format!(
                            "{} WRAP-SENTINEL-A",
                            "long diagnostic message ".repeat(8)
                        ),
                        relative_path: Some(PathBuf::from(
                            "notes/with/a/very/long/path/that/must/clip.z",
                        )),
                        start_line: Some(1),
                        start_column: Some(1),
                        zettel_id: None,
                    },
                    DiagnosticRow {
                        id: 2,
                        severity: "error".to_owned(),
                        category: "semantic".to_owned(),
                        code: Some("SECOND-DIAGNOSTIC".to_owned()),
                        message: "SECOND-DIAGNOSTIC".to_owned(),
                        relative_path: Some(PathBuf::from("b.z")),
                        start_line: Some(2),
                        start_column: Some(1),
                        zettel_id: None,
                    },
                ],
                Vec::new(),
            ),
        );
        let backend = TestBackend::new(120, 16);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::new(1, 0, 2),
                    &DashboardOverlay::None,
                    "",
                )
            })
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Main Diagnostics 2/2"));
        assert!(rendered.contains("b.z"));
        assert!(!rendered.contains("WRAP-SENTINEL-A"));
    }

    #[test]
    fn render_diagnostic_severities_use_distinct_enabled_colors() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(
                Vec::new(),
                vec![
                    diagnostic(1, "error", "err.code"),
                    diagnostic(2, "warning", "warn.code"),
                    diagnostic(3, "info", "info.code"),
                    diagnostic(4, "notice", "unknown.code"),
                ],
                Vec::new(),
            ),
        );
        let backend = TestBackend::new(120, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state_and_color(
                    area,
                    &frame,
                    DashboardRenderState::new(0, 0, 4),
                    &DashboardOverlay::None,
                    "",
                    ColorMode::Enabled,
                )
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();

        assert_eq!(cell_for_text(buffer, "err.code").fg, Color::Red);
        assert_eq!(cell_for_text(buffer, "warn.code").fg, Color::Yellow);
        assert_eq!(cell_for_text(buffer, "info.code").fg, Color::Cyan);
        assert_eq!(cell_for_text(buffer, "unknown.code").fg, Color::Magenta);
    }

    #[test]
    fn render_no_color_mode_has_no_foreground_or_background_colors() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(
                Vec::new(),
                vec![diagnostic(1, "error", "err.code")],
                vec![IndexStatusRow::new("Diagnostics", 1)],
            ),
        );
        let backend = TestBackend::new(100, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state_and_color(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::None,
                    "refresh failed",
                    ColorMode::Disabled,
                )
            })
            .expect("draw");

        for y in terminal.backend().buffer().area.top()..terminal.backend().buffer().area.bottom() {
            for x in
                terminal.backend().buffer().area.left()..terminal.backend().buffer().area.right()
            {
                let cell = &terminal.backend().buffer()[(x, y)];
                assert_eq!(cell.fg, Color::Reset);
                assert_eq!(cell.bg, Color::Reset);
            }
        }
    }

    #[test]
    fn render_index_health_and_attention_counts_are_colored() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            None,
            DashboardSnapshot::Ready {
                index: Box::new(IndexPanel {
                    schema_version: 2,
                    rows: vec![
                        IndexStatusRow::new("Discovered files", 3),
                        IndexStatusRow::new("Indexed files", 3),
                        IndexStatusRow::new("New files", 1),
                        IndexStatusRow::new("Changed files", 1),
                        IndexStatusRow::new("Deleted files", 0),
                        IndexStatusRow::new("Diagnostics", 1),
                    ],
                    discovered_files: 3,
                    indexed_files: 3,
                    changed_files: 1,
                    new_files: 1,
                    deleted_files: 0,
                    diagnostic_count: 1,
                    last_indexed_at_unix_ms: Some(42),
                }),
                diagnostics: Vec::new(),
                today: Vec::new(),
                inbox: Vec::new(),
                search: SearchPanel::empty(""),
            },
        );
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let buffer = terminal.backend().buffer();

        assert_eq!(cell_for_text(buffer, "stale").fg, Color::Yellow);
        assert_text_has_color(buffer, "New files", Color::Yellow);
        assert_text_has_color(buffer, "Diagnostics", Color::Red);
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
                last_indexed_at_unix_ms: Some(42),
            }),
            diagnostics,
            today,
            inbox: Vec::new(),
            search: SearchPanel::empty(""),
        }
    }

    fn zettel(store_id: i64, title: &str) -> ZettelRow {
        ZettelRow {
            store_id,
            canonical_id: Some(title.to_owned()),
            file_path: PathBuf::from(format!("notes/{title}.z")),
            title: title.to_owned(),
            todo_marker: Some("[ ]".to_owned()),
            start_line: Some(1),
            start_column: Some(1),
            lifecycle_date: None,
            tags: Vec::new(),
            properties: Vec::new(),
            preview: None,
            badges: vec![QueryBadge::new("due", "#z/todo")],
        }
    }

    fn diagnostic(id: i64, severity: &str, code: &str) -> DiagnosticRow {
        DiagnosticRow {
            id,
            severity: severity.to_owned(),
            category: "semantic".to_owned(),
            code: Some(code.to_owned()),
            message: format!("{code} message"),
            relative_path: Some(PathBuf::from(format!("{code}.z"))),
            start_line: Some(1),
            start_column: Some(1),
            zettel_id: None,
        }
    }

    fn cell_for_text<'a>(buffer: &'a Buffer, text: &str) -> &'a ratatui::buffer::Cell {
        for y in buffer.area.top()..buffer.area.bottom() {
            let mut line = String::new();
            for x in buffer.area.left()..buffer.area.right() {
                line.push_str(buffer[(x, y)].symbol());
            }
            if let Some(offset) = line.find(text) {
                return &buffer[(buffer.area.left() + offset as u16, y)];
            }
        }
        panic!("buffer should contain {text:?}");
    }

    fn assert_text_has_color(buffer: &Buffer, text: &str, color: Color) {
        for y in buffer.area.top()..buffer.area.bottom() {
            let mut line = String::new();
            for x in buffer.area.left()..buffer.area.right() {
                line.push_str(buffer[(x, y)].symbol());
            }
            let mut start = 0;
            while let Some(offset) = line[start..].find(text) {
                let x = buffer.area.left() + (start + offset) as u16;
                if buffer[(x, y)].fg == color {
                    return;
                }
                start += offset + text.len();
            }
        }
        panic!("buffer should contain {text:?} with foreground {color:?}");
    }
}
