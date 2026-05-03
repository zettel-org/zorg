use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use crate::model::{
    CaptureDraft, CaptureField, DashboardFrame, DashboardOverlay, DashboardSnapshot, Panel,
    PanelRow,
};

pub(crate) fn render_dashboard(frame_area: &mut ratatui::Frame<'_>, frame: &DashboardFrame) {
    render_dashboard_with_state(frame_area, frame, 0, &DashboardOverlay::None, "");
}

pub(crate) fn render_dashboard_with_state(
    frame_area: &mut ratatui::Frame<'_>,
    frame: &DashboardFrame,
    selected_index: usize,
    overlay: &DashboardOverlay,
    status: &str,
) {
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

    if root.width < 72 {
        let body = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),
                Constraint::Percentage(56),
                Constraint::Percentage(44),
            ])
            .split(vertical[1]);
        render_nav(frame_area, body[0], frame.panel);
        render_main(frame_area, body[1], frame, selected_index);
        render_inspector(frame_area, body[2], frame, selected_index);
    } else {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(18),
                Constraint::Percentage(54),
                Constraint::Percentage(46),
            ])
            .split(vertical[1]);
        render_nav(frame_area, body[0], frame.panel);
        render_main(frame_area, body[1], frame, selected_index);
        render_inspector(frame_area, body[2], frame, selected_index);
    }
    render_footer(frame_area, vertical[2], status);

    render_overlay(frame_area, root, overlay);
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
            frame.health_label(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  diagnostics "),
        Span::styled(
            frame.diagnostics_label(),
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

fn render_main(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    frame: &DashboardFrame,
    selected_index: usize,
) {
    let lines = match &frame.snapshot {
        DashboardSnapshot::Degraded { message } => vec![
            Line::from(Span::styled(
                "Index unavailable",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("Read-only index unavailable."),
            Line::from(message.as_str()),
        ],
        DashboardSnapshot::Ready { index, .. } if frame.panel == Panel::Index => {
            let mut lines = vec![
                Line::from(Span::styled(
                    "Index panel",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(format!("Health: {}", index.health_label())),
                Line::from(format!("Schema version: {}", index.schema_version)),
                Line::from(""),
            ];
            lines.extend(row_lines(frame.active_rows(), selected_index));
            lines
        }
        DashboardSnapshot::Ready { search, .. } => {
            let mut lines = vec![Line::from(Span::styled(
                format!("{} panel", frame.panel.label()),
                Style::default().add_modifier(Modifier::BOLD),
            ))];
            if frame.panel == Panel::Search {
                lines.push(Line::from(format!("Query: {}", search.input)));
                if let Some(error) = &search.error {
                    lines.push(Line::from(Span::styled(
                        format!("Error: {error}"),
                        Style::default().add_modifier(Modifier::BOLD),
                    )));
                }
            }
            lines.push(Line::from(""));
            let rows = frame.active_rows();
            if rows.is_empty() {
                lines.push(Line::from(empty_state(frame.panel)));
            } else {
                lines.extend(row_lines(rows, selected_index));
            }
            lines
        }
    };

    terminal_frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().title("Main").borders(Borders::ALL)),
        area,
    );
}

fn render_inspector(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    frame: &DashboardFrame,
    selected_index: usize,
) {
    let lines = frame
        .inspector_lines_for_selection(selected_index)
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();

    terminal_frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().title("Inspector").borders(Borders::ALL)),
        area,
    );
}

fn render_footer(terminal_frame: &mut ratatui::Frame<'_>, area: Rect, status: &str) {
    let text = if status.is_empty() {
        "q quit  c capture  r refresh  R reindex  enter open  / search/edit  esc cancel  ? help"
    } else {
        status
    };
    terminal_frame.render_widget(
        Paragraph::new(text).block(Block::default().title("Keys").borders(Borders::ALL)),
        area,
    );
}

fn row_lines(rows: Vec<PanelRow>, selected_index: usize) -> Vec<Line<'static>> {
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            let prefix = if index == selected_index { "> " } else { "  " };
            if index == selected_index {
                Line::from(vec![
                    Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        row.list_line(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                ])
            } else {
                Line::from(format!("{prefix}{}", row.list_line()))
            }
        })
        .collect()
}

fn render_overlay(terminal_frame: &mut ratatui::Frame<'_>, area: Rect, overlay: &DashboardOverlay) {
    let (title, lines) = match overlay {
        DashboardOverlay::None => return,
        DashboardOverlay::Help => (
            "Help",
            vec![
                Line::from("q/Esc quit or close overlay"),
                Line::from("tab/backtab switch panels"),
                Line::from("up/down/j/k move selection"),
                Line::from("g/G jump first or last row"),
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
        DashboardOverlay::Capture(draft) => ("Capture", capture_lines(draft)),
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

fn capture_lines(draft: &CaptureDraft) -> Vec<Line<'static>> {
    let mut lines = CaptureField::ALL
        .iter()
        .map(|field| {
            let prefix = if *field == draft.active { "> " } else { "  " };
            let value = draft.field_value(*field);
            let value = if value.is_empty() { "-" } else { value };
            if *field == draft.active {
                Line::from(vec![
                    Span::styled(prefix, Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!("{}: {}", field.label(), value),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
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
    use crate::model::{DashboardSnapshot, IndexPanel, IndexStatusRow, SearchPanel};
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
                    0,
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
    }
}
