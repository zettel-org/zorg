use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::model::{
    CaptureDraft, CaptureField, ColorMode, DashboardFrame, DashboardOverlay, DashboardRenderState,
    DashboardSnapshot, DiagnosticFilterDraft, DiagnosticFilterField, FixPreviewOverlay,
    FixPreviewRow, Panel, PanelRow, SeverityKind, StatusEvent,
};

#[cfg(test)]
pub(crate) fn render_dashboard(frame_area: &mut ratatui::Frame<'_>, frame: &DashboardFrame) {
    render_dashboard_with_state(
        frame_area,
        frame,
        DashboardRenderState::for_frame(frame),
        &DashboardOverlay::None,
        None,
        &[],
    );
}

#[cfg(test)]
pub(crate) fn render_dashboard_with_state(
    frame_area: &mut ratatui::Frame<'_>,
    frame: &DashboardFrame,
    render_state: DashboardRenderState,
    overlay: &DashboardOverlay,
    latest_status: Option<&StatusEvent>,
    status_events: &[StatusEvent],
) {
    render_dashboard_with_state_and_color(
        frame_area,
        frame,
        render_state,
        overlay,
        latest_status,
        status_events,
        ColorMode::Enabled,
    );
}

pub(crate) fn render_dashboard_with_state_and_color(
    frame_area: &mut ratatui::Frame<'_>,
    frame: &DashboardFrame,
    render_state: DashboardRenderState,
    overlay: &DashboardOverlay,
    latest_status: Option<&StatusEvent>,
    status_events: &[StatusEvent],
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
    render_footer(frame_area, areas.footer, latest_status, &palette);

    render_overlay(frame_area, root, overlay, status_events, &palette);
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

    fn status(self, severity: SeverityKind) -> Style {
        if !self.color_mode.is_enabled() {
            return Style::default();
        }

        match severity {
            SeverityKind::Error => self.severity(SeverityKind::Error),
            SeverityKind::Warning => self.severity(SeverityKind::Warning),
            SeverityKind::Info => self.emphasis().fg(Color::Green),
            SeverityKind::Unknown => Style::default().fg(Color::Cyan),
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
            let lines = degraded_guidance_lines(frame, message, palette);
            terminal_frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
        }
        DashboardSnapshot::Ready { index, .. } if frame.panel == Panel::Index => {
            let mut header = vec![Line::from(format!(
                "Health: {}  Schema version: {}",
                index.health_label(),
                index.schema_version
            ))];
            if index.discovered_files == 0 {
                header.push(Line::from("No .z files are indexed under the root."));
                header.push(Line::from("Run zorg db reindex after creating files."));
            }
            render_main_rows(terminal_frame, inner, frame, render_state, header, palette);
        }
        DashboardSnapshot::Ready { search, .. } => {
            let header = panel_header_lines(frame, search, palette);
            render_main_rows(terminal_frame, inner, frame, render_state, header, palette);
        }
    }
}

fn panel_header_lines(
    frame: &DashboardFrame,
    search: &crate::model::SearchPanel,
    palette: &StylePalette,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if frame.panel == Panel::Search {
        lines.push(Line::from(format!("Query: {}", search.input)));
        if let Some(error) = &search.error {
            lines.push(Line::from(Span::styled(
                format!("Error: {error}"),
                palette.severity(SeverityKind::Error),
            )));
        }
    }

    if matches!(frame.panel, Panel::Diagnostics | Panel::Today)
        && frame.diagnostic_filters.is_active()
        && let Some(counts) = frame.diagnostic_filter_counts(frame.panel)
    {
        lines.push(Line::from(format!(
            "Filters {}/{}: {}",
            counts.visible,
            counts.total,
            frame.diagnostic_filters.active_labels().join(" ")
        )));
    }

    lines
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
        terminal_frame.render_widget(Paragraph::new(empty_state(frame)), list_area);
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
        DashboardSnapshot::Ready { index, .. } if frame.panel == Panel::Index => {
            if index.discovered_files == 0 { 3 } else { 1 }
        }
        DashboardSnapshot::Ready { search, .. } => {
            panel_header_lines(frame, search, &StylePalette::new(ColorMode::Disabled)).len() as u16
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
    latest_status: Option<&StatusEvent>,
    palette: &StylePalette,
) {
    let footer = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);
    let key_help = "q quit f fix e sev :filt a clear c cap r/R ref / search L log ? help";
    terminal_frame.render_widget(
        Paragraph::new(key_help).block(Block::default().title("Keys").borders(Borders::ALL)),
        footer[0],
    );

    let (text, style) = latest_status
        .map(|event| (event.message.as_str(), palette.status(event.severity)))
        .unwrap_or(("", Style::default()));
    terminal_frame.render_widget(
        Paragraph::new(Span::styled(text, style))
            .block(Block::default().title("Latest").borders(Borders::ALL)),
        footer[1],
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
    status_events: &[StatusEvent],
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
                Line::from("f preview a safe fix for selected diagnostic row"),
                Line::from("e cycle diagnostic severity filter"),
                Line::from(": edit diagnostic code/path filters"),
                Line::from("a clear diagnostic filters"),
                Line::from("r refresh index snapshot"),
                Line::from("R reindex, then y/enter confirms"),
                Line::from("enter open selected source in $EDITOR"),
                Line::from("/ switch to Search and edit the query"),
                Line::from("L open recent status log"),
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
        DashboardOverlay::DiagnosticFilter(draft) => (
            "Diagnostic Filters",
            diagnostic_filter_lines(draft, palette),
        ),
        DashboardOverlay::FixPreview(preview) => {
            ("Fix Preview", fix_preview_lines(preview, palette))
        }
        DashboardOverlay::EventLog => ("Log", status_event_lines(status_events, palette)),
        DashboardOverlay::Log { title, message } => (
            title.as_str(),
            message.lines().map(Line::from).collect::<Vec<_>>(),
        ),
    };

    let (width_percent, height_percent) = match overlay {
        DashboardOverlay::FixPreview(_) => (78, 70),
        _ => (66, 44),
    };
    let overlay_area = centered_rect(width_percent, height_percent, area);
    terminal_frame.render_widget(Clear, overlay_area);
    terminal_frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true })
            .block(Block::default().title(title).borders(Borders::ALL)),
        overlay_area,
    );
}

fn fix_preview_lines(preview: &FixPreviewOverlay, palette: &StylePalette) -> Vec<Line<'static>> {
    let diagnostic = &preview.diagnostic;
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{:<7} ", diagnostic.severity),
                palette.severity(SeverityKind::from_label(&diagnostic.severity)),
            ),
            Span::styled(diagnostic.code.clone(), palette.emphasis()),
        ]),
        Line::from(format!("Path: {}", diagnostic.path)),
        Line::from(format!("Position: {}", diagnostic.position)),
        Line::from(format!("Message: {}", diagnostic.message)),
        Line::from(""),
    ];

    if preview.previews.is_empty() {
        lines.push(Line::from(Span::styled(
            "Unavailable",
            palette.severity(SeverityKind::Warning),
        )));
        lines.push(Line::from(
            preview
                .unavailable_reason
                .clone()
                .unwrap_or_else(|| "No safe fix preview is available.".to_owned()),
        ));
    } else {
        for (index, row) in preview.previews.iter().enumerate() {
            if index > 0 {
                lines.push(Line::from(""));
            }
            lines.extend(fix_preview_row_lines(index + 1, row, palette));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from("Esc/q/f closes preview."));
    lines
}

fn fix_preview_row_lines(
    index: usize,
    row: &FixPreviewRow,
    palette: &StylePalette,
) -> Vec<Line<'static>> {
    let state = match (row.is_safe, row.is_preferred) {
        (true, true) => "safe preferred",
        (true, false) => "safe",
        (false, true) => "unsafe preferred",
        (false, false) => "unsafe",
    };
    let location = match (row.primary_line, row.primary_column) {
        (Some(line), Some(column)) => format!("{}:{line}:{column}", row.path.display()),
        (Some(line), None) => format!("{}:{line}", row.path.display()),
        _ => row.path.display().to_string(),
    };
    let replacement_title = if row.replacement_truncated {
        "Replacement preview (truncated):"
    } else {
        "Replacement preview:"
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{index}. "), palette.emphasis()),
            Span::styled(row.rule_code.clone(), palette.emphasis()),
            Span::raw(format!("  {state}")),
        ]),
        Line::from(format!("Severity: {}", row.severity)),
        Line::from(format!("Path: {location}")),
        Line::from(format!("Why: {}", row.explanation)),
        Line::from(replacement_title),
    ];
    lines.extend(
        row.replacement_preview
            .lines()
            .map(|line| Line::from(format!("  {line}"))),
    );
    if row.replacement_preview.is_empty() {
        lines.push(Line::from("  <empty>"));
    }
    lines
}

fn status_event_lines(events: &[StatusEvent], palette: &StylePalette) -> Vec<Line<'static>> {
    if events.is_empty() {
        return vec![Line::from("No status events yet.")];
    }

    events
        .iter()
        .rev()
        .flat_map(|event| {
            let mut lines = vec![Line::from(vec![
                Span::styled(
                    format!("#{:03} {:<7} ", event.order, event.severity.label()),
                    palette.status(event.severity),
                ),
                Span::raw(event.message.clone()),
            ])];
            if let Some(detail) = &event.detail {
                lines.extend(
                    detail
                        .lines()
                        .map(|line| Line::from(format!("      {line}"))),
                );
            }
            lines
        })
        .collect()
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

fn diagnostic_filter_lines(
    draft: &DiagnosticFilterDraft,
    palette: &StylePalette,
) -> Vec<Line<'static>> {
    let mut lines = DiagnosticFilterField::ALL
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
        "Enter applies. Tab switches fields. Ctrl-u clears the active field.",
    ));
    lines.push(Line::from(
        "Esc cancels. Use e for severity and a to clear all filters.",
    ));
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

fn degraded_guidance_lines<'a>(
    frame: &DashboardFrame,
    message: &'a str,
    palette: &StylePalette,
) -> Vec<Line<'a>> {
    vec![
        Line::from(Span::styled(
            "Index unavailable",
            palette.health("degraded"),
        )),
        Line::from(""),
        Line::from("Read-only index unavailable."),
        Line::from("The dashboard opens the SQLite index read-only."),
        Line::from(format!("Root: {}", frame.root.display())),
        Line::from(format!("Database: {}", frame.database_path.display())),
        Line::from(format!("Run: {}", frame.reindex_command())),
        Line::from(""),
        Line::from(message),
    ]
}

fn empty_state(frame: &DashboardFrame) -> String {
    match frame.panel {
        Panel::Today => format!(
            "No due, do, todo, or diagnostic attention rows.\nAdd dated/todo zettels under {} or run {} after changes.",
            frame.root.display(),
            frame.reindex_command()
        ),
        Panel::Inbox => format!(
            "No #z/inbox rows.\nAdd inbox zettels under {} or run {} after changes.",
            frame.root.display(),
            frame.reindex_command()
        ),
        Panel::Search => format!(
            "No search rows. Type / to edit a SWOG query or @query/id.\nRun {} after changing indexed files.",
            frame.reindex_command()
        ),
        Panel::Diagnostics if frame.diagnostic_filters.is_active() => {
            "No diagnostics match active filters.\nPress a to clear filters or : to edit code/path filters.".to_owned()
        }
        Panel::Diagnostics => "No indexed diagnostics.".to_owned(),
        Panel::Index => format!(
            "No index rows.\nRun {} to rebuild the read-only dashboard index.",
            frame.reindex_command()
        ),
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
                    None,
                    &[],
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
    fn render_footer_keeps_keys_visible_with_latest_status() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            None,
            ready_snapshot(
                Vec::new(),
                Vec::new(),
                vec![IndexStatusRow::new("Discovered files", 1)],
            ),
        );
        let event = StatusEvent::new(1, SeverityKind::Info, "refresh complete", None);
        let backend = TestBackend::new(120, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::None,
                    Some(&event),
                    std::slice::from_ref(&event),
                )
            })
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("q quit"));
        assert!(rendered.contains("L log"));
        assert!(rendered.contains("? help"));
        assert!(rendered.contains("refresh complete"));
    }

    #[test]
    fn render_log_overlay_shows_recent_events_with_details() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let events = vec![
            StatusEvent::new(1, SeverityKind::Info, "refresh complete", None),
            StatusEvent::new(
                2,
                SeverityKind::Error,
                "Open failed",
                Some("open failed: $EDITOR is not set".to_owned()),
            ),
        ];
        let backend = TestBackend::new(100, 28);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::EventLog,
                    events.last(),
                    &events,
                )
            })
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Log"));
        assert!(rendered.contains("#002 error"));
        assert!(rendered.contains("Open failed"));
        assert!(rendered.contains("open failed: $EDITOR is not set"));
        assert!(rendered.contains("#001 info"));
    }

    #[test]
    fn render_fix_preview_overlay_shows_context_and_replacement() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let overlay = DashboardOverlay::FixPreview(FixPreviewOverlay {
            diagnostic: crate::model::DiagnosticPreviewContext {
                severity: "error".to_owned(),
                code: "reference.unresolved_absolute".to_owned(),
                message: "unresolved absolute reference".to_owned(),
                path: "links.z".to_owned(),
                position: "5:5-5:17".to_owned(),
            },
            previews: vec![FixPreviewRow {
                rule_code: "fix.unresolved_absolute_link_typo".to_owned(),
                severity: "error".to_owned(),
                path: PathBuf::from("links.z"),
                primary_line: Some(5),
                primary_column: Some(5),
                replacement_preview: "#project/plan".to_owned(),
                replacement_truncated: false,
                is_preferred: true,
                is_safe: true,
                explanation: "Rewrite unresolved link to #project/plan".to_owned(),
            }],
            unavailable_reason: None,
        });
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &overlay,
                    None,
                    &[],
                )
            })
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Fix Preview"));
        assert!(rendered.contains("reference.unresolved_absolute"));
        assert!(rendered.contains("fix.unresolved_absolute_link_typo"));
        assert!(rendered.contains("#project/plan"));
        assert!(rendered.contains("Esc/q/f closes preview"));
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
    fn render_narrow_diagnostics_shows_active_filter_counts() {
        let mut frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(
                Vec::new(),
                vec![
                    diagnostic(1, "error", "reference.missing"),
                    diagnostic(2, "warning", "syntax.sort"),
                ],
                vec![IndexStatusRow::new("Diagnostics", 2)],
            ),
        );
        frame.diagnostic_filters.severity = crate::model::DiagnosticSeverityFilter::Error;
        frame.diagnostic_filters.code = "reference".to_owned();

        let backend = TestBackend::new(52, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Filters 1/2: severity=error code=reference"));
        assert!(!rendered.contains("syntax.sort"));
    }

    #[test]
    fn render_diagnostic_filter_overlay_lists_editable_fields() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let overlay = DashboardOverlay::DiagnosticFilter(DiagnosticFilterDraft {
            code: "reference".to_owned(),
            path: "notes".to_owned(),
            active: DiagnosticFilterField::Path,
        });
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &overlay,
                    None,
                    &[],
                )
            })
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Diagnostic Filters"));
        assert!(rendered.contains("Code: reference"));
        assert!(rendered.contains("Path: notes"));
        assert!(rendered.contains("Enter applies"));
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
                    None,
                    &[],
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
                        absolute_path: None,
                        relative_path: Some(PathBuf::from(
                            "notes/with/a/very/long/path/that/must/clip.z",
                        )),
                        start_byte: Some(0),
                        end_byte: Some(1),
                        start_line: Some(1),
                        start_column: Some(1),
                        end_line: Some(1),
                        end_column: Some(2),
                        zettel_id: None,
                    },
                    DiagnosticRow {
                        id: 2,
                        severity: "error".to_owned(),
                        category: "semantic".to_owned(),
                        code: Some("SECOND-DIAGNOSTIC".to_owned()),
                        message: "SECOND-DIAGNOSTIC".to_owned(),
                        absolute_path: None,
                        relative_path: Some(PathBuf::from("b.z")),
                        start_byte: Some(10),
                        end_byte: Some(12),
                        start_line: Some(2),
                        start_column: Some(1),
                        end_line: Some(2),
                        end_column: Some(3),
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
                    None,
                    &[],
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
                    None,
                    &[],
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
                    Some(&StatusEvent::new(
                        1,
                        SeverityKind::Error,
                        "refresh failed",
                        None,
                    )),
                    &[],
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
            absolute_path: None,
            relative_path: Some(PathBuf::from(format!("{code}.z"))),
            start_byte: Some(0),
            end_byte: Some(1),
            start_line: Some(1),
            start_column: Some(1),
            end_line: Some(1),
            end_column: Some(2),
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
