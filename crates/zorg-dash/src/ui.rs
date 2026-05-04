use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::model::{
    CaptureDraft, CaptureField, CaptureTemplatePicker, ColorMode, DashboardFrame, DashboardOverlay,
    DashboardRenderState, DashboardSnapshot, DiagnosticFilterDraft, DiagnosticFilterField,
    FixPreviewOverlay, FixPreviewRow, Panel, PanelRow, PendingActivity, SeverityKind, StatusEvent,
    TodayMode, TodoActionOverlay, TodoPromptDraft, YankOverlay, format_duration,
    todo_date_field_label,
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
    render_dashboard_with_activity_and_color(
        frame_area,
        frame,
        render_state,
        overlay,
        latest_status,
        status_events,
        None,
        color_mode,
    );
}

pub(crate) fn render_dashboard_with_activity_and_color(
    frame_area: &mut ratatui::Frame<'_>,
    frame: &DashboardFrame,
    render_state: DashboardRenderState,
    overlay: &DashboardOverlay,
    latest_status: Option<&StatusEvent>,
    status_events: &[StatusEvent],
    pending_activity: Option<&PendingActivity>,
    color_mode: ColorMode,
) {
    let palette = DashTheme::new(color_mode);
    let root = frame_area.area();
    let areas = dashboard_areas(root);

    render_status(frame_area, areas.status, frame, pending_activity, &palette);

    if root.width < 72 {
        render_nav(frame_area, areas.nav, frame, &palette);
        render_main(frame_area, areas.main, frame, render_state, &palette);
        render_inspector(
            frame_area,
            areas.inspector,
            frame,
            render_state.selected_index,
            &palette,
        );
    } else {
        render_nav(frame_area, areas.nav, frame, &palette);
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
                Constraint::Length(8),
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
struct DashTheme {
    color_mode: ColorMode,
}

#[allow(dead_code)]
impl DashTheme {
    // Renderer styles should flow through semantic theme tokens. Tokens that set
    // colors in enabled mode must reset foreground and background in disabled
    // mode, and later renderer code should prefer the block/span helpers below
    // instead of adding raw color literals at call sites.
    const fn new(color_mode: ColorMode) -> Self {
        Self { color_mode }
    }

    fn enabled_style(self, style: Style) -> Style {
        if self.color_mode.is_enabled() {
            style
        } else {
            // Disabled mode must actively reset both color channels for every semantic token
            // that can set foreground or background when colors are enabled.
            Self::reset_colors(style)
        }
    }

    fn reset_colors(style: Style) -> Style {
        style.fg(Color::Reset).bg(Color::Reset)
    }

    fn app_background(self) -> Style {
        self.enabled_style(Style::default().bg(Color::Black))
    }

    fn panel_surface(self) -> Style {
        self.enabled_style(Style::default().bg(Color::Black))
    }

    fn elevated_overlay_surface(self) -> Style {
        self.enabled_style(Style::default().bg(Color::DarkGray))
    }

    fn subtle_border(self) -> Style {
        self.enabled_style(Style::default().fg(Color::DarkGray))
    }

    fn active_border(self) -> Style {
        self.enabled_style(Style::default().fg(Color::Cyan))
    }

    fn warning_border(self) -> Style {
        self.enabled_style(Style::default().fg(Color::Yellow))
    }

    fn error_border(self) -> Style {
        self.enabled_style(Style::default().fg(Color::Red))
    }

    fn title(self) -> Style {
        self.enabled_style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
    }

    fn active_title(self) -> Style {
        self.enabled_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    }

    fn body_text(self) -> Style {
        self.enabled_style(Style::default().fg(Color::Gray))
    }

    fn muted_text(self) -> Style {
        self.enabled_style(Style::default().fg(Color::DarkGray))
    }

    fn marked_row(self) -> Style {
        self.enabled_style(
            Style::default()
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
    }

    fn todo_accent(self) -> Style {
        self.enabled_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )
    }

    fn query_accent(self) -> Style {
        self.enabled_style(
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )
    }

    fn dashboard_accent(self) -> Style {
        self.enabled_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    }

    fn graph_link(self) -> Style {
        self.enabled_style(
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
    }

    fn path(self) -> Style {
        self.enabled_style(Style::default().fg(Color::Cyan))
    }

    fn key_hint(self) -> Style {
        self.enabled_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    }

    fn emphasis(self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    fn selection(self) -> Style {
        self.enabled_style(self.emphasis().bg(Color::DarkGray))
    }

    fn severity(self, severity: SeverityKind) -> Style {
        let style = self.emphasis();
        if !self.color_mode.is_enabled() {
            return Self::reset_colors(style);
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
            return Self::reset_colors(style);
        }

        match label {
            "current" => style.fg(Color::Green),
            "stale" | "newer" | "source-stale" => style.fg(Color::Yellow),
            "missing" | "degraded" | "failed" => style.fg(Color::Red),
            "loading" | "unknown" => style.fg(Color::Cyan),
            _ => style.fg(Color::Cyan),
        }
    }

    fn index_row(self, row: &crate::model::IndexStatusRow) -> Style {
        if row.value == 0 {
            return Style::default();
        }
        if !self.color_mode.is_enabled() {
            return Self::reset_colors(Style::default());
        }

        match row.label.as_str() {
            "Diagnostics" | "Deleted files" => self.severity(SeverityKind::Error),
            "New files" | "Changed files" => self.severity(SeverityKind::Warning),
            _ => Style::default(),
        }
    }

    fn status(self, severity: SeverityKind) -> Style {
        if !self.color_mode.is_enabled() {
            return Self::reset_colors(Style::default());
        }

        match severity {
            SeverityKind::Error => self.severity(SeverityKind::Error),
            SeverityKind::Warning => self.severity(SeverityKind::Warning),
            SeverityKind::Info => self.emphasis().fg(Color::Green),
            SeverityKind::Unknown => Style::default().fg(Color::Cyan),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum BlockRole {
    Subtle,
    Active,
    Status,
    Footer,
    Elevated,
}

impl BlockRole {
    fn surface_style(self, theme: DashTheme) -> Style {
        match self {
            Self::Elevated => theme.elevated_overlay_surface(),
            _ => theme.panel_surface(),
        }
    }

    fn border_style(self, theme: DashTheme) -> Style {
        match self {
            Self::Active | Self::Status => theme.active_border(),
            Self::Footer | Self::Subtle | Self::Elevated => theme.subtle_border(),
        }
    }

    fn title_style(self, theme: DashTheme) -> Style {
        match self {
            Self::Active | Self::Status => theme.active_title(),
            Self::Subtle | Self::Footer | Self::Elevated => theme.title(),
        }
    }
}

fn shell_block(title: impl Into<String>, role: BlockRole, theme: &DashTheme) -> Block<'static> {
    Block::default()
        .title(Line::from(Span::styled(
            title.into(),
            role.title_style(*theme),
        )))
        .borders(Borders::ALL)
        .style(role.surface_style(*theme))
        .border_style(role.border_style(*theme))
}

fn panel_block(title: impl Into<String>, role: BlockRole, theme: &DashTheme) -> Block<'static> {
    shell_block(title, role, theme)
}

fn status_block(theme: &DashTheme) -> Block<'static> {
    shell_block("Zorg Dash", BlockRole::Status, theme)
}

fn footer_block(title: impl Into<String>, theme: &DashTheme) -> Block<'static> {
    shell_block(title, BlockRole::Footer, theme)
}

fn overlay_block(title: impl Into<String>, role: BlockRole, theme: &DashTheme) -> Block<'static> {
    shell_block(title, role, theme)
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum DomainTone {
    Todo,
    Query,
    Dashboard,
    Link,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum BadgeTone {
    Severity(SeverityKind),
    Status(SeverityKind),
    Domain(DomainTone),
}

impl DomainTone {
    fn style(self, theme: DashTheme) -> Style {
        match self {
            Self::Todo => theme.todo_accent(),
            Self::Query => theme.query_accent(),
            Self::Dashboard => theme.dashboard_accent(),
            Self::Link => theme.graph_link(),
        }
    }
}

impl BadgeTone {
    fn style(self, theme: DashTheme) -> Style {
        match self {
            Self::Severity(severity) => theme.severity(severity),
            Self::Status(severity) => theme.status(severity),
            Self::Domain(domain) => domain.style(theme),
        }
    }
}

fn metadata_span(text: impl Into<String>, theme: &DashTheme) -> Span<'static> {
    Span::styled(text.into(), theme.muted_text())
}

fn label_span(text: impl Into<String>, theme: &DashTheme) -> Span<'static> {
    metadata_span(text, theme)
}

fn value_span(text: impl Into<String>, style: Style) -> Span<'static> {
    Span::styled(text.into(), style)
}

fn path_span(text: impl Into<String>, theme: &DashTheme) -> Span<'static> {
    value_span(text, theme.path())
}

fn id_span(text: impl Into<String>, theme: &DashTheme) -> Span<'static> {
    value_span(text, theme.dashboard_accent())
}

fn link_span(text: impl Into<String>, theme: &DashTheme) -> Span<'static> {
    value_span(text, DomainTone::Link.style(*theme))
}

fn key_hint_span(text: impl Into<String>, theme: &DashTheme) -> Span<'static> {
    value_span(text, theme.key_hint())
}

fn badge_span(text: impl Into<String>, tone: BadgeTone, theme: &DashTheme) -> Span<'static> {
    value_span(text, tone.style(*theme))
}

fn label_value_spans(
    label: impl Into<String>,
    value: impl Into<String>,
    value_style: Style,
    theme: &DashTheme,
) -> Vec<Span<'static>> {
    vec![label_span(label, theme), value_span(value, value_style)]
}

fn label_value_line(
    label: impl Into<String>,
    value: impl Into<String>,
    value_style: Style,
    theme: &DashTheme,
) -> Line<'static> {
    Line::from(label_value_spans(label, value, value_style, theme))
}

fn path_line(
    label: impl Into<String>,
    path: impl Into<String>,
    theme: &DashTheme,
) -> Line<'static> {
    Line::from(vec![label_span(label, theme), path_span(path, theme)])
}

fn id_line(label: impl Into<String>, id: impl Into<String>, theme: &DashTheme) -> Line<'static> {
    Line::from(vec![label_span(label, theme), id_span(id, theme)])
}

fn selected_style(base_style: Style, theme: &DashTheme) -> Style {
    // Selection composes by patching selection foreground/background onto the caller's
    // semantic style, preserving modifiers such as severity emphasis for future rich rows.
    base_style.patch(theme.selection())
}

fn selected_span(text: impl Into<String>, base_style: Style, theme: &DashTheme) -> Span<'static> {
    value_span(text, selected_style(base_style, theme))
}

fn render_status(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    frame: &DashboardFrame,
    pending_activity: Option<&PendingActivity>,
    palette: &DashTheme,
) {
    let health_label = frame.health_label();
    let mut spans = Vec::new();
    spans.extend(label_value_spans(
        "  index ",
        health_label,
        palette.health(health_label),
        palette,
    ));
    spans.extend(label_value_spans(
        "  diagnostics ",
        frame.diagnostics_label(),
        match &frame.snapshot {
            DashboardSnapshot::Ready { index, .. } if index.diagnostic_count > 0 => {
                palette.severity(SeverityKind::Error)
            }
            _ => palette.emphasis(),
        },
        palette,
    ));
    spans.extend(label_value_spans(
        "  freshness ",
        frame.freshness_label(),
        palette.health(frame.freshness_label()),
        palette,
    ));
    spans.extend(label_value_spans(
        "  marked ",
        frame.marked_diagnostic_count().to_string(),
        if frame.marked_diagnostic_count() > 0 {
            palette.severity(SeverityKind::Warning)
        } else {
            palette.emphasis()
        },
        palette,
    ));
    spans.extend(label_value_spans(
        "  panel ",
        frame.active_panel_id().key().to_owned(),
        palette.emphasis(),
        palette,
    ));
    spans.extend(label_value_spans(
        "  rows ",
        frame.telemetry.row_counts.status_label(),
        palette.emphasis(),
        palette,
    ));
    spans.extend(label_value_spans(
        "  root ",
        frame.root.display().to_string(),
        palette.path(),
        palette,
    ));
    spans.extend(label_value_spans(
        "  db ",
        frame.database_path.display().to_string(),
        palette.path(),
        palette,
    ));
    if let Some(activity) = pending_activity {
        spans.extend(label_value_spans(
            "  pending ",
            format!(
                "{} {} {}",
                activity.spinner(),
                activity.operation.label(),
                format_duration(activity.elapsed)
            ),
            palette.status(SeverityKind::Info),
            palette,
        ));
    }
    let status = vec![Line::from(spans)];
    terminal_frame.render_widget(Paragraph::new(status).block(status_block(palette)), area);
}

fn render_nav(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    frame: &DashboardFrame,
    palette: &DashTheme,
) {
    let active = frame.active_panel_id();
    let items = frame
        .panels
        .iter()
        .map(|panel| {
            if panel.id == active {
                ListItem::new(Line::from(vec![
                    Span::raw("> "),
                    selected_span(panel.label.clone(), Style::default(), palette),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::raw(panel.label.clone()),
                ]))
            }
        })
        .collect::<Vec<_>>();
    terminal_frame.render_widget(
        List::new(items).block(panel_block("Panels", BlockRole::Subtle, palette)),
        area,
    );
}

fn render_main(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    frame: &DashboardFrame,
    render_state: DashboardRenderState,
    palette: &DashTheme,
) {
    let title = format!(
        "Main {} {} marked {}",
        frame.active_panel_label(),
        render_state.position_text(),
        frame.marked_diagnostic_count()
    );
    let block = panel_block(title, BlockRole::Active, palette);
    let inner = block.inner(area);
    terminal_frame.render_widget(block, area);

    match &frame.snapshot {
        DashboardSnapshot::Loading => {
            terminal_frame.render_widget(
                Paragraph::new(loading_lines(frame, palette)).wrap(Wrap { trim: true }),
                inner,
            );
        }
        DashboardSnapshot::Degraded { message } => {
            let lines = degraded_guidance_lines(frame, message, palette);
            terminal_frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
        }
        DashboardSnapshot::Ready { index, .. } if active_builtin(frame, Panel::Index) => {
            let mut header = vec![Line::from(vec![
                label_span("Health: ", palette),
                value_span(index.health_label(), palette.health(index.health_label())),
                label_span("  Freshness: ", palette),
                value_span(
                    frame.freshness_label(),
                    palette.health(frame.freshness_label()),
                ),
                label_span("  Schema version: ", palette),
                value_span(index.schema_version.to_string(), palette.emphasis()),
            ])];
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
    palette: &DashTheme,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(panel) = frame.active_custom_panel() {
        lines.push(Line::from(vec![
            label_span("Custom: ", palette),
            badge_span(
                panel.definition.title.clone(),
                BadgeTone::Domain(DomainTone::Dashboard),
                palette,
            ),
            label_span("  query ", palette),
            badge_span(
                panel.definition.query_source.source_label(),
                BadgeTone::Domain(DomainTone::Query),
                palette,
            ),
            label_span("  rows ", palette),
            value_span(panel.row_count().to_string(), palette.emphasis()),
        ]));
        if let Some(error) = &panel.error {
            lines.push(Line::from(Span::styled(
                format!("Error: {}", first_nonempty_line(error)),
                palette.severity(SeverityKind::Error),
            )));
        }
    }
    if active_builtin(frame, Panel::Today)
        && let Some(counts) = frame.today_counts()
    {
        lines.push(Line::from(vec![
            label_span("Today: ", palette),
            badge_span(
                frame.today_mode.label(),
                BadgeTone::Domain(DomainTone::Todo),
                palette,
            ),
            label_span("  rows ", palette),
            value_span(counts.visible.to_string(), palette.emphasis()),
            label_span("  todos ", palette),
            value_span(counts.todos.to_string(), palette.emphasis()),
            label_span("  diagnostics ", palette),
            value_span(
                format!(
                    "{}/{}",
                    counts.diagnostics_visible, counts.diagnostics_total
                ),
                palette.emphasis(),
            ),
        ]));
    }

    if active_builtin(frame, Panel::Search) {
        lines.push(label_value_line(
            "Query: ",
            search.input.clone(),
            palette.query_accent(),
            palette,
        ));
        if let Some(query_info) = &search.query_info {
            lines.extend(query_info.header_lines().into_iter().map(Line::from));
        }
        if let Some(error) = search.error_summary() {
            lines.push(Line::from(vec![
                badge_span("Error: ", BadgeTone::Severity(SeverityKind::Error), palette),
                badge_span(error, BadgeTone::Severity(SeverityKind::Error), palette),
            ]));
        }
    }

    if (active_builtin(frame, Panel::Diagnostics) || active_builtin(frame, Panel::Today))
        && frame.diagnostic_filters.is_active()
        && let Some(counts) = frame.diagnostic_filter_counts(frame.panel)
    {
        lines.push(Line::from(vec![
            label_span("Filters ", palette),
            value_span(
                format!("{}/{}", counts.visible, counts.total),
                palette.emphasis(),
            ),
            label_span(": ", palette),
            metadata_span(frame.diagnostic_filters.active_labels().join(" "), palette),
        ]));
    }

    lines
}

fn render_main_rows(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    frame: &DashboardFrame,
    render_state: DashboardRenderState,
    header: Vec<Line<'static>>,
    palette: &DashTheme,
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
    let items = row_items(&rows, frame, render_state.selected_index, palette);
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
        DashboardSnapshot::Ready { index, .. } if active_builtin(frame, Panel::Index) => {
            if index.discovered_files == 0 { 3 } else { 1 }
        }
        DashboardSnapshot::Ready { search, .. } => {
            panel_header_lines(frame, search, &DashTheme::new(ColorMode::Disabled)).len() as u16
        }
        _ => 0,
    };
    Rect {
        y: inner.y.saturating_add(header_height),
        height: inner.height.saturating_sub(header_height),
        ..inner
    }
}

fn active_builtin(frame: &DashboardFrame, panel: Panel) -> bool {
    frame.custom_panel.is_none() && frame.panel == panel
}

fn render_inspector(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    frame: &DashboardFrame,
    selected_index: usize,
    palette: &DashTheme,
) {
    let lines = frame
        .inspector_lines_for_selection(selected_index)
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                Line::from(value_span(line, palette.emphasis()))
            } else {
                Line::from(line)
            }
        })
        .collect::<Vec<_>>();

    terminal_frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(panel_block("Inspector", BlockRole::Subtle, palette)),
        area,
    );
}

fn render_footer(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    latest_status: Option<&StatusEvent>,
    palette: &DashTheme,
) {
    let footer = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);
    let key_help = Line::from(vec![
        key_hint_span("q", palette),
        metadata_span(" quit ", palette),
        key_hint_span("/", palette),
        metadata_span(" search ", palette),
        key_hint_span("F1", palette),
        metadata_span(" swog ", palette),
        key_hint_span("?", palette),
        metadata_span(" help ", palette),
        key_hint_span("L", palette),
        metadata_span(" log ", palette),
        key_hint_span("y", palette),
        metadata_span(" yank ", palette),
        key_hint_span("o", palette),
        metadata_span(" open ", palette),
        key_hint_span("r/R", palette),
        metadata_span(" ref", palette),
    ]);
    terminal_frame.render_widget(
        Paragraph::new(key_help).block(footer_block("Keys", palette)),
        footer[0],
    );

    let (text, style) = latest_status
        .map(|event| (event.message.as_str(), palette.status(event.severity)))
        .unwrap_or(("", Style::default()));
    terminal_frame.render_widget(
        Paragraph::new(value_span(text, style)).block(footer_block("Latest", palette)),
        footer[1],
    );
}

fn row_items(
    rows: &[PanelRow],
    frame: &DashboardFrame,
    selected_index: usize,
    palette: &DashTheme,
) -> Vec<ListItem<'static>> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let prefix = format!(
                "{}{}",
                if index == selected_index { ">" } else { " " },
                if frame.is_row_marked(row) { "*" } else { " " }
            );
            let row_style = row_style(row, palette);
            if index == selected_index {
                ListItem::new(Line::from(vec![
                    selected_span(prefix, row_style, palette),
                    selected_span(row_list_line(row, frame), row_style, palette),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw(prefix),
                    value_span(row_list_line(row, frame), row_style),
                ]))
            }
        })
        .collect()
}

fn row_list_line(row: &PanelRow, frame: &DashboardFrame) -> String {
    match (frame.panel, row) {
        (Panel::Today, PanelRow::Diagnostic(row)) if frame.custom_panel.is_none() => {
            let code = row.code.as_deref().unwrap_or(row.category.as_str());
            format!("{:<7} {:<30} {}", row.severity, code, row.message)
        }
        _ => row.list_line(),
    }
}

fn row_style(row: &PanelRow, palette: &DashTheme) -> Style {
    match row {
        PanelRow::Diagnostic(row) => palette.severity(row.severity_kind()),
        PanelRow::Zettel(_) => Style::default(),
        PanelRow::Query(row) if !row.valid => palette.severity(SeverityKind::Warning),
        PanelRow::Query(_) => Style::default(),
        PanelRow::IndexStatus(row) => palette.index_row(row),
    }
}

fn render_overlay(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    overlay: &DashboardOverlay,
    status_events: &[StatusEvent],
    palette: &DashTheme,
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
                Line::from("t cycle Today mode: combined, todos, diagnostics"),
                Line::from("d mark selected Today todo done after confirmation"),
                Line::from("p postpone selected due/do todo with YYYY-MM-DD, +1d, or +1w"),
                Line::from("s schedule selected open todo by setting do::YYYY-MM-DD"),
                Line::from("y yank row id, source link, or diagnostic message"),
                Line::from("f preview a safe fix for selected diagnostic row"),
                Line::from("space mark or unmark a diagnostic row for later review"),
                Line::from("e cycle diagnostic severity filter"),
                Line::from(": edit diagnostic code/path filters"),
                Line::from("a clear diagnostic filters"),
                Line::from("r refresh index snapshot"),
                Line::from("R reindex, then y/enter confirms"),
                Line::from("F from a fix preview confirms and applies the selected safe fix"),
                Line::from("enter runs selected Queries row or opens selected source elsewhere"),
                Line::from("o open selected source in $EDITOR"),
                Line::from("/ switch to Search and edit the query"),
                Line::from("F1 from Search opens SWOG help"),
                Line::from("L open recent status log"),
                Line::from(
                    "search edit: arrows/Home/End move, Up/Down history, Ctrl-W word, Ctrl-U prefix",
                ),
                Line::from("search edit: type SWOG or @query/id, Enter runs, Esc cancels"),
            ],
        ),
        DashboardOverlay::SwogHelp => ("SWOG Help", swog_help_lines(palette)),
        DashboardOverlay::ConfirmReindex => (
            "Confirm Reindex",
            vec![
                Line::from("Reindex will write a fresh SQLite snapshot for this corpus."),
                Line::from("Press y or enter to continue, n or Esc to cancel."),
            ],
        ),
        DashboardOverlay::ConfirmFixApply(preview) => {
            ("Confirm Fix Apply", confirm_fix_apply_lines(preview))
        }
        DashboardOverlay::ConfirmTodoApply(todo) => (todo.title.as_str(), confirm_todo_lines(todo)),
        DashboardOverlay::TodoPrompt(draft) => {
            (draft.action.title(), todo_prompt_lines(draft, palette))
        }
        DashboardOverlay::Yank(overlay) => ("Yank", yank_lines(overlay, palette)),
        DashboardOverlay::CapturePicker(picker) => (
            "Capture Templates",
            capture_template_picker_lines(picker, palette),
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
        DashboardOverlay::FixPreview(_)
        | DashboardOverlay::ConfirmFixApply(_)
        | DashboardOverlay::ConfirmTodoApply(_)
        | DashboardOverlay::SwogHelp => (78, 70),
        DashboardOverlay::TodoPrompt(_) => (72, 50),
        DashboardOverlay::CapturePicker(_) => (78, 62),
        DashboardOverlay::Capture(_) => (72, 62),
        DashboardOverlay::Yank(_) => (72, 44),
        _ => (66, 44),
    };
    let overlay_area = centered_rect(width_percent, height_percent, area);
    terminal_frame.render_widget(Clear, overlay_area);
    terminal_frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true })
            .block(overlay_block(title, BlockRole::Elevated, palette)),
        overlay_area,
    );
}

fn fix_preview_lines(preview: &FixPreviewOverlay, palette: &DashTheme) -> Vec<Line<'static>> {
    let diagnostic = &preview.diagnostic;
    let mut lines = vec![
        Line::from(vec![
            badge_span(
                format!("{:<7} ", diagnostic.severity),
                BadgeTone::Severity(SeverityKind::from_label(&diagnostic.severity)),
                palette,
            ),
            id_span(diagnostic.code.clone(), palette),
        ]),
        path_line("Path: ", diagnostic.path.clone(), palette),
        label_value_line(
            "Position: ",
            diagnostic.position.clone(),
            palette.emphasis(),
            palette,
        ),
        label_value_line(
            "Message: ",
            diagnostic.message.clone(),
            palette.body_text(),
            palette,
        ),
        Line::from(""),
    ];

    if preview.previews.is_empty() {
        lines.push(Line::from(badge_span(
            "Unavailable",
            BadgeTone::Severity(SeverityKind::Warning),
            palette,
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

    if let Some(summary) = &preview.marked_summary
        && !summary.is_empty()
    {
        lines.push(Line::from(""));
        lines.push(label_value_line(
            "Marked diagnostics: ",
            summary.marked_count.to_string(),
            palette.emphasis(),
            palette,
        ));
        if summary.selected_is_marked {
            lines.push(Line::from("Selected diagnostic is marked."));
        }
        if let Some(count) = summary.selected_file_marked_count {
            lines.push(Line::from(format!(
                "Selected file has {count} marked diagnostic(s)."
            )));
        }
        lines.push(Line::from(
            "Bulk apply for marked diagnostics is unavailable; apply one safe preview at a time.",
        ));
        for row in &summary.rows {
            lines.push(Line::from(vec![
                metadata_span("* ", palette),
                badge_span(
                    format!("{:<7} ", row.severity),
                    BadgeTone::Severity(SeverityKind::from_label(&row.severity)),
                    palette,
                ),
                id_span(format!("{:<28} ", row.code), palette),
                path_span(format!("{} ", row.path), palette),
                metadata_span(row.position.clone(), palette),
            ]));
            lines.push(Line::from(format!("  {}", row.message)));
        }
    }

    lines.push(Line::from(""));
    if preview.can_apply_selected_fix() {
        lines.push(Line::from(
            "F applies selected safe fix after confirmation. Esc/q/f closes preview.",
        ));
    } else {
        lines.push(Line::from("Esc/q/f closes preview."));
    }
    lines
}

fn swog_help_lines(palette: &DashTheme) -> Vec<Line<'static>> {
    vec![
        Line::from(vec![
            label_span("Tags: ", palette),
            badge_span("#z/todo", BadgeTone::Domain(DomainTone::Todo), palette),
            metadata_span(", ", palette),
            link_span("#project/work", palette),
        ]),
        Line::from("Properties: due:<=today, did:*, area:work/zorg"),
        Line::from("Todos: todo:[ ], -did:*"),
        Line::from(vec![
            label_span("Links: ", palette),
            link_span("links:#project/reference", palette),
        ]),
        Line::from("Files/text: file:notes.z text:\"alpha text\""),
        Line::from("Modified: modified:<7d"),
        Line::from(vec![
            label_span("Boolean: ", palette),
            badge_span("#z/todo", BadgeTone::Domain(DomainTone::Todo), palette),
            metadata_span(" OR ", palette),
            badge_span("#z/query", BadgeTone::Domain(DomainTone::Query), palette),
        ]),
        Line::from("Grouping: (#z/todo OR #z/query) -did:*"),
        Line::from(vec![
            label_span("Stored query IDs: ", palette),
            id_span("@queries/foo", palette),
        ]),
        Line::from("Output: TABLE #z/todo"),
        Line::from("Aggregate: count(#z/todo OR #z/query)"),
    ]
}

fn confirm_fix_apply_lines(preview: &FixPreviewOverlay) -> Vec<Line<'static>> {
    let diagnostic = &preview.diagnostic;
    let mut lines = vec![
        Line::from("Apply the selected safe fix to disk?"),
        Line::from(""),
        Line::from(format!("Path: {}", diagnostic.path)),
        Line::from(format!("Position: {}", diagnostic.position)),
        Line::from(format!("Diagnostic: {}", diagnostic.code)),
        Line::from(format!("Message: {}", diagnostic.message)),
    ];

    if let Some(row) = preview.previews.iter().find(|row| row.is_safe) {
        lines.push(Line::from(""));
        lines.push(Line::from(format!("Fix: {}", row.rule_code)));
        lines.push(Line::from(format!("Edits: {}", row.explanation)));
        lines.push(Line::from("Replacement preview:"));
        lines.extend(
            row.replacement_preview
                .lines()
                .map(|line| Line::from(format!("  {line}"))),
        );
        if row.replacement_preview.is_empty() {
            lines.push(Line::from("  <empty>"));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from("Press y or enter to apply, n or Esc to cancel."));
    lines
}

fn confirm_todo_lines(todo: &TodoActionOverlay) -> Vec<Line<'static>> {
    let row = &todo.row;
    let mut lines = vec![
        Line::from("Apply this todo change to disk?"),
        Line::from(""),
        Line::from(format!("Path: {}", row.file_path.display())),
        Line::from(format!(
            "Target: {}",
            row.canonical_id
                .as_deref()
                .map(|id| format!("@{id}"))
                .unwrap_or_else(|| row.title.clone())
        )),
        Line::from(format!("Title: {}", row.title)),
        Line::from(""),
        Line::from("Planned changes:"),
    ];

    if todo.plan.changes.is_empty() {
        lines.push(Line::from("  <none>"));
    } else {
        lines.extend(todo.plan.changes.iter().map(|change| {
            let before = change.before.as_deref().unwrap_or("-");
            let after = change.after.as_deref().unwrap_or("-");
            Line::from(format!("  {}: {} -> {}", change.field, before, after))
        }));
    }

    if !todo.plan.warnings.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from("Warnings:"));
        lines.extend(
            todo.plan
                .warnings
                .iter()
                .map(|warning| Line::from(format!("  {warning}"))),
        );
    }

    lines.push(Line::from(""));
    lines.push(Line::from("Press y or enter to apply, n or Esc to cancel."));
    lines
}

fn fix_preview_row_lines(
    index: usize,
    row: &FixPreviewRow,
    palette: &DashTheme,
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
            value_span(format!("{index}. "), palette.emphasis()),
            id_span(row.rule_code.clone(), palette),
            metadata_span(format!("  {state}"), palette),
        ]),
        label_value_line(
            "Severity: ",
            row.severity.clone(),
            palette.severity(SeverityKind::from_label(&row.severity)),
            palette,
        ),
        path_line("Path: ", location, palette),
        label_value_line(
            "Why: ",
            row.explanation.clone(),
            palette.body_text(),
            palette,
        ),
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

fn status_event_lines(events: &[StatusEvent], palette: &DashTheme) -> Vec<Line<'static>> {
    if events.is_empty() {
        return vec![Line::from("No status events yet.")];
    }

    events
        .iter()
        .rev()
        .flat_map(|event| {
            let mut lines = vec![Line::from(vec![
                badge_span(
                    format!("#{:03} {:<7} ", event.order, event.severity.label()),
                    BadgeTone::Status(event.severity),
                    palette,
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

fn capture_lines(draft: &CaptureDraft, palette: &DashTheme) -> Vec<Line<'static>> {
    let mut lines = draft
        .editable_fields()
        .iter()
        .map(|field| {
            let prefix = if *field == draft.active { "> " } else { "  " };
            let value = draft.field_value(*field);
            let value = if value.is_empty() { "-" } else { value };
            let required = if matches!(*field, CaptureField::Title | CaptureField::Body)
                && draft.requires_variable(field.variable_name().unwrap_or_default())
            {
                " *"
            } else {
                ""
            };
            if *field == draft.active {
                Line::from(vec![
                    selected_span(prefix, Style::default(), palette),
                    selected_span(
                        format!("{}{}: {}", field.label(), required, value),
                        Style::default(),
                        palette,
                    ),
                ])
            } else {
                Line::from(format!("{prefix}{}{required}: {value}", field.label()))
            }
        })
        .collect::<Vec<_>>();
    lines.push(Line::from(""));
    if let Some(title) = &draft.template_title {
        lines.push(label_value_line(
            "Template title: ",
            title.clone(),
            palette.body_text(),
            palette,
        ));
    }
    if let Some(id) = &draft.template_id {
        lines.push(id_line("Template ID: ", id.clone(), palette));
    }
    let variables = if draft.template_variables.is_empty() {
        "unknown; title and body are available for compatibility".to_owned()
    } else {
        draft.template_variables.join(", ")
    };
    lines.push(label_value_line(
        "Required variables: ",
        variables,
        palette.body_text(),
        palette,
    ));
    let automatic = draft.automatic_variables();
    if !automatic.is_empty() {
        lines.push(label_value_line(
            "Auto-filled variables: ",
            automatic.join(", "),
            palette.body_text(),
            palette,
        ));
    }
    if let Some(path) = &draft.template_path {
        lines.push(path_line(
            "Template path: ",
            path.display().to_string(),
            palette,
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(
        "Tab moves fields. Enter creates. Destination may be blank to use template dest::.",
    ));
    lines.push(Line::from("Esc cancels. Ctrl-u clears the active field."));
    lines
}

fn capture_template_picker_lines(
    picker: &CaptureTemplatePicker,
    palette: &DashTheme,
) -> Vec<Line<'static>> {
    if picker.rows.is_empty() {
        return vec![Line::from("No #z/tmpl templates were found.")];
    }

    let mut lines = Vec::new();
    for (index, row) in picker.visible_rows(6) {
        let prefix = if index == picker.selected { "> " } else { "  " };
        let label = row.label();
        let template_line = if index == picker.selected {
            Line::from(vec![
                selected_span(prefix, Style::default(), palette),
                selected_span(label, Style::default(), palette),
            ])
        } else {
            Line::from(format!("{prefix}{label}"))
        };
        lines.push(template_line);

        let destination = row.destination.as_deref().unwrap_or("-");
        let variables = if row.variables.is_empty() {
            "-".to_owned()
        } else {
            row.variables.join(", ")
        };
        lines.push(Line::from(vec![
            label_span("    dest: ", palette),
            value_span(destination.to_owned(), palette.body_text()),
            label_span("  vars: ", palette),
            metadata_span(variables, palette),
        ]));

        let path = row
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_owned());
        lines.push(path_line("    path: ", path, palette));
    }

    if picker.rows.len() > 6 {
        lines.push(Line::from(format!(
            "Showing {} of {} templates.",
            picker.visible_rows(6).len(),
            picker.rows.len()
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from("Up/down moves. Enter selects. Esc cancels."));
    lines
}

fn diagnostic_filter_lines(
    draft: &DiagnosticFilterDraft,
    palette: &DashTheme,
) -> Vec<Line<'static>> {
    let mut lines = DiagnosticFilterField::ALL
        .iter()
        .map(|field| {
            let prefix = if *field == draft.active { "> " } else { "  " };
            let value = draft.field_value(*field);
            let value = if value.is_empty() { "-" } else { value };
            if *field == draft.active {
                Line::from(vec![
                    selected_span(prefix, Style::default(), palette),
                    selected_span(
                        format!("{}: {}", field.label(), value),
                        Style::default(),
                        palette,
                    ),
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

fn todo_prompt_lines(draft: &TodoPromptDraft, palette: &DashTheme) -> Vec<Line<'static>> {
    let row = &draft.row;
    let mut lines = vec![
        label_value_line(
            "Target: ",
            row.canonical_id
                .as_deref()
                .map(|id| format!("@{id}"))
                .unwrap_or_else(|| row.title.clone()),
            palette.dashboard_accent(),
            palette,
        ),
        path_line("Path: ", row.file_path.display().to_string(), palette),
        Line::from(""),
    ];

    lines.extend(draft.visible_fields().into_iter().map(|field| {
        let prefix = if field == draft.active { "> " } else { "  " };
        let value = draft.field_value(field);
        if field == draft.active {
            Line::from(vec![
                selected_span(prefix, Style::default(), palette),
                selected_span(
                    format!("{}: {value}", field.label()),
                    Style::default(),
                    palette,
                ),
            ])
        } else {
            Line::from(format!("{prefix}{}: {value}", field.label()))
        }
    }));

    if draft.field_options.len() > 1 {
        lines.push(Line::from(format!(
            "Available fields: {}",
            draft
                .field_options
                .iter()
                .map(|field| todo_date_field_label(*field))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    if let Some(error) = &draft.error {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            badge_span("Error: ", BadgeTone::Status(SeverityKind::Error), palette),
            badge_span(
                error.clone(),
                BadgeTone::Status(SeverityKind::Error),
                palette,
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from("Date accepts YYYY-MM-DD, +1d, or +1w."));
    lines.push(Line::from(
        "Enter applies. Esc cancels. Tab switches fields. Left/right changes field.",
    ));
    lines
}

fn yank_lines(overlay: &YankOverlay, palette: &DashTheme) -> Vec<Line<'static>> {
    let mut lines = vec![
        label_value_line(
            "Target: ",
            overlay.target_summary.clone(),
            palette.body_text(),
            palette,
        ),
        Line::from(""),
    ];

    lines.extend(overlay.options.iter().enumerate().map(|(index, option)| {
        let prefix = if index == overlay.selected_index {
            "> "
        } else {
            "  "
        };
        let value = option.value.as_deref().unwrap_or_else(|| {
            option
                .unavailable_reason
                .as_deref()
                .unwrap_or("unavailable")
        });
        let text = format!("{}. {}: {value}", index + 1, option.kind.label());
        if index == overlay.selected_index {
            Line::from(vec![
                selected_span(prefix, Style::default(), palette),
                selected_span(text, Style::default(), palette),
            ])
        } else if option.value.is_some() {
            Line::from(format!("{prefix}{text}"))
        } else {
            Line::from(vec![
                Span::raw(prefix.to_owned()),
                badge_span(text, BadgeTone::Status(SeverityKind::Warning), palette),
            ])
        }
    }));

    lines.push(Line::from(""));
    lines.push(Line::from(
        "Enter/y copies selected. 1-3 copies directly. Tab moves. Esc cancels.",
    ));
    lines.push(Line::from(
        "If clipboard transport is unavailable, the value stays visible in the log.",
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
    palette: &DashTheme,
) -> Vec<Line<'a>> {
    vec![
        Line::from(value_span("Index unavailable", palette.health("degraded"))),
        Line::from(""),
        Line::from("Read-only index unavailable."),
        Line::from("The dashboard opens the SQLite index read-only."),
        path_line("Root: ", frame.root.display().to_string(), palette),
        path_line(
            "Database: ",
            frame.database_path.display().to_string(),
            palette,
        ),
        Line::from(format!("Run: {}", frame.reindex_command())),
        Line::from(""),
        Line::from(message),
    ]
}

fn loading_lines(frame: &DashboardFrame, palette: &DashTheme) -> Vec<Line<'static>> {
    vec![
        Line::from(value_span(
            "Loading dashboard snapshot",
            palette.health("loading"),
        )),
        Line::from(""),
        Line::from("Opening the configured SQLite index read-only."),
        path_line("Root: ", frame.root.display().to_string(), palette),
        path_line(
            "Database: ",
            frame.database_path.display().to_string(),
            palette,
        ),
    ]
}

fn empty_state(frame: &DashboardFrame) -> String {
    if let Some(custom_key) = &frame.custom_panel {
        return format!(
            "No rows for custom panel {}.\nThe panel query returned no matching zettels.",
            custom_key
        );
    }
    match frame.panel {
        Panel::Today => today_empty_state(frame),
        Panel::Inbox => format!(
            "No #z/inbox rows.\nAdd inbox zettels under {} or run {} after changes.",
            frame.root.display(),
            frame.reindex_command()
        ),
        Panel::Queries => format!(
            "No saved #z/query zettels.\nAdd query:: properties or fenced swog blocks, then run {}.",
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

fn first_nonempty_line(message: &str) -> &str {
    message
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(message)
}

fn today_empty_state(frame: &DashboardFrame) -> String {
    match frame.today_mode {
        TodayMode::TodosOnly => format!(
            "No Today todo rows.\nAdd due, do, or open todo zettels under {} or run {} after changes.",
            frame.root.display(),
            frame.reindex_command()
        ),
        TodayMode::DiagnosticsOnly if frame.diagnostic_filters.is_active() => {
            "No Today diagnostics match active filters.\nPress a to clear filters or : to edit code/path filters."
                .to_owned()
        }
        TodayMode::DiagnosticsOnly => "No Today diagnostic rows.".to_owned(),
        TodayMode::Combined => format!(
            "No Today todo or diagnostic rows.\nAdd dated/todo zettels under {} or run {} after changes.",
            frame.root.display(),
            frame.reindex_command()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        CaptureTemplateRow, DashboardSnapshot, DiagnosticRow, GraphLinkRow, GraphLoadState,
        GraphNeighborhood, GraphSection, GraphZettelRow, IndexPanel, IndexStatusRow, PanelRow,
        PendingOperationKind, QueryBadge, QueryPanel, SearchPanel, ZettelRow,
    };
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::time::Duration;
    use zorg_core::SourceSpan;

    #[test]
    fn render_includes_status_nav_and_index_lines() {
        let mut frame = DashboardFrame::new(
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
                queries: QueryPanel::empty(),
                search: SearchPanel::empty(""),
                selected_dashboard: None,
                custom_panels: BTreeMap::new(),
            },
        );
        frame.record_initial_load_duration(Duration::from_millis(42));
        frame.record_refresh_duration(Duration::from_millis(125));
        let backend = TestBackend::new(140, 28);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Zorg Dash"));
        assert!(rendered.contains("Panels"));
        assert!(rendered.contains("Main"));
        assert!(rendered.contains("Inspector"));
        assert!(rendered.contains("Keys"));
        assert!(rendered.contains("Latest"));
        assert!(rendered.contains("> Index"));
        assert!(rendered.contains("Discovered files"));
        assert!(rendered.contains("Schema version: 2"));
        assert!(rendered.contains("Telemetry"));
        assert!(rendered.contains("auto-refresh off"));
        assert!(rendered.contains("Initial load: 42ms"));
        assert!(rendered.contains("Last refresh: 125ms"));
        assert!(rendered.contains("Rows: today 0 inbox 0 queries 0"));
    }

    #[test]
    fn render_loading_snapshot_shows_initial_loading_frame() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            DashboardSnapshot::Loading,
        );
        let backend = TestBackend::new(100, 28);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Zorg Dash"));
        assert!(rendered.contains("index loading"));
        assert!(rendered.contains("Loading dashboard snapshot"));
        assert!(rendered.contains("Root: /tmp/corpus"));
    }

    #[test]
    fn render_search_panel_includes_query_errors_inline_and_in_inspector() {
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
                queries: QueryPanel::empty(),
                search: SearchPanel::with_error(
                    "OR",
                    "query parse failed\nquery.syntax at byte 0: expected a filter before OR",
                ),
                selected_dashboard: None,
                custom_panels: BTreeMap::new(),
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
        assert!(rendered.contains("Search query"));
        assert!(rendered.contains("query.syntax at byte 0"));
    }

    #[test]
    fn render_zettel_inspector_includes_graph_sections() {
        let selected = zettel(1, "task");
        let mut frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(
                vec![PanelRow::Zettel(selected.clone())],
                Vec::new(),
                vec![IndexStatusRow::new("Diagnostics", 0)],
            ),
        );
        frame.set_graph_context(
            selected.row_id(),
            GraphLoadState::Ready(graph_neighborhood()),
        );

        let backend = TestBackend::new(120, 32);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Graph context"));
        assert!(rendered.contains("Outgoing links: 3"));
        assert!(rendered.contains("unresolved @missing-id"));
        assert!(rendered.contains("Incoming backlinks: 1"));
        assert!(rendered.contains("Ancestors: 1"));
        assert!(rendered.contains("Descendants: 1"));
        assert!(rendered.contains("... 1 more"));
    }

    #[test]
    fn render_narrow_zettel_graph_keeps_footer_visible() {
        let selected = zettel(1, "task");
        let mut frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(
                vec![PanelRow::Zettel(selected.clone())],
                Vec::new(),
                vec![IndexStatusRow::new("Diagnostics", 0)],
            ),
        );
        frame.set_graph_context(
            selected.row_id(),
            GraphLoadState::Ready(graph_neighborhood()),
        );

        let backend = TestBackend::new(56, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Graph context"));
        assert!(rendered.contains("Outgoing links"));
        assert!(rendered.contains("q quit"));
    }

    #[test]
    fn render_swog_help_overlay_at_normal_and_narrow_widths() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Search,
            None,
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
                queries: QueryPanel::empty(),
                search: SearchPanel::empty(""),
                selected_dashboard: None,
                custom_panels: BTreeMap::new(),
            },
        );
        for (width, height) in [(100, 28), (56, 22)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("terminal");
            terminal
                .draw(|area| {
                    render_dashboard_with_state(
                        area,
                        &frame,
                        DashboardRenderState::for_frame(&frame),
                        &DashboardOverlay::SwogHelp,
                        None,
                        &[],
                    )
                })
                .expect("draw");
            let rendered = buffer_to_string(terminal.backend().buffer());

            assert!(rendered.contains("SWOG Help"), "{rendered}");
            assert!(rendered.contains("#z/todo"), "{rendered}");
            assert!(rendered.contains("due:<=today"), "{rendered}");
            assert!(rendered.contains("@queries/foo"), "{rendered}");
            assert!(rendered.contains("TABLE #z/todo"), "{rendered}");
            assert!(rendered.contains("Keys"), "{rendered}");
        }
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
    fn render_capture_overlay_lists_template_inspector() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            DashboardSnapshot::Degraded {
                message: "missing db".to_owned(),
            },
        );
        let draft = CaptureTemplateRow {
            selector: Some("@system/templates/todo".to_owned()),
            id: Some("@system/templates/todo".to_owned()),
            title: Some("Todo capture".to_owned()),
            destination: Some("inbox.z".to_owned()),
            path: Some(PathBuf::from("templates.z")),
            variables: vec!["id".to_owned(), "title".to_owned()],
        }
        .draft()
        .expect("template draft");
        let backend = TestBackend::new(88, 28);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::Capture(draft),
                    None,
                    &[],
                )
            })
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Title *: -"), "{rendered}");
        assert!(!rendered.contains("Body:"), "{rendered}");
        assert!(
            rendered.contains("Template title: Todo capture"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Required variables: id, title"),
            "{rendered}"
        );
        assert!(rendered.contains("Auto-filled variables: id"), "{rendered}");
        assert!(
            rendered.contains("Template path: templates.z"),
            "{rendered}"
        );
    }

    #[test]
    fn render_capture_template_picker_lists_metadata() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            DashboardSnapshot::Degraded {
                message: "missing db".to_owned(),
            },
        );
        let picker = CaptureTemplatePicker::new(vec![
            CaptureTemplateRow {
                selector: Some("@system/templates/project".to_owned()),
                id: Some("@system/templates/project".to_owned()),
                title: Some("Project note".to_owned()),
                destination: Some("projects".to_owned()),
                path: Some(PathBuf::from("templates.z")),
                variables: vec!["title".to_owned(), "body".to_owned()],
            },
            CaptureTemplateRow {
                selector: None,
                id: None,
                title: None,
                destination: Some("misc.z".to_owned()),
                path: Some(PathBuf::from("misc.z")),
                variables: Vec::new(),
            },
        ]);
        let backend = TestBackend::new(96, 28);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::CapturePicker(picker),
                    None,
                    &[],
                )
            })
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Capture Templates"), "{rendered}");
        assert!(rendered.contains("@system/templates/project - Project note"));
        assert!(rendered.contains("dest: projects"));
        assert!(rendered.contains("vars: title, body"));
        assert!(rendered.contains("path: templates.z"));
        assert!(rendered.contains("Enter selects"));
    }

    #[test]
    fn render_yank_overlay_lists_supported_values_and_fallback_hint() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let overlay = DashboardOverlay::Yank(
            PanelRow::Diagnostic(diagnostic(7, "warning", "reference.missing"))
                .yank_overlay(&frame.root),
        );
        let backend = TestBackend::new(100, 26);
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

        assert!(rendered.contains("Yank"));
        assert!(rendered.contains("row id"));
        assert!(rendered.contains("source link"));
        assert!(rendered.contains("diagnostic message"));
        assert!(rendered.contains("If clipboard transport is unavailable"));
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
        assert!(rendered.contains("y yank"));
        assert!(rendered.contains("L log"));
        assert!(rendered.contains("? help"));
        assert!(rendered.contains("refresh complete"));
    }

    #[test]
    fn render_status_shows_deterministic_pending_activity() {
        let frame = DashboardFrame::new(
            PathBuf::from("/r"),
            PathBuf::from("/d"),
            Panel::Today,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let activity = PendingActivity::new(
            PendingOperationKind::TodoApply,
            Duration::from_millis(1_250),
            2,
        );
        let backend = TestBackend::new(220, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_activity_and_color(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::None,
                    None,
                    &[],
                    Some(&activity),
                    ColorMode::Enabled,
                )
            })
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("pending"));
        assert!(rendered.contains("- todo apply"));
        assert!(rendered.contains("1.250s"));
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
            selector: Default::default(),
            marked_summary: None,
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
        assert!(rendered.contains("F applies selected safe fix"));
    }

    #[test]
    fn render_fix_apply_confirmation_shows_selected_fix() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let overlay = DashboardOverlay::ConfirmFixApply(FixPreviewOverlay {
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
            selector: Default::default(),
            marked_summary: None,
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

        assert!(rendered.contains("Confirm Fix Apply"));
        assert!(rendered.contains("fix.unresolved_absolute_link_typo"));
        assert!(rendered.contains("Press y or enter to apply"));
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
                queries: QueryPanel::empty(),
                search: SearchPanel::empty(""),
                selected_dashboard: None,
                custom_panels: BTreeMap::new(),
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
    fn render_narrow_overlays_keep_titles_and_footer_visible() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(
                vec![PanelRow::Zettel(zettel(1, "task"))],
                vec![diagnostic(1, "error", "reference.missing")],
                vec![IndexStatusRow::new("Diagnostics", 1)],
            ),
        );
        let fix_preview = FixPreviewOverlay {
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
            selector: Default::default(),
            marked_summary: None,
        };
        let todo_row = zettel(2, "schedule");
        let events = vec![StatusEvent::new(
            1,
            SeverityKind::Error,
            "Open failed",
            Some("open failed: $EDITOR is not set".to_owned()),
        )];
        let cases = vec![
            (DashboardOverlay::Help, "Help"),
            (DashboardOverlay::SwogHelp, "SWOG Help"),
            (
                DashboardOverlay::Capture(CaptureDraft::new("@tmpl/todo", Some("inbox.z".into()))),
                "Capture",
            ),
            (
                DashboardOverlay::CapturePicker(CaptureTemplatePicker::new(vec![
                    CaptureTemplateRow {
                        selector: Some("@tmpl/todo".to_owned()),
                        id: Some("@tmpl/todo".to_owned()),
                        title: Some("Todo".to_owned()),
                        destination: Some("inbox.z".to_owned()),
                        path: Some(PathBuf::from("templates.z")),
                        variables: vec!["title".to_owned()],
                    },
                ])),
                "Capture Templates",
            ),
            (
                DashboardOverlay::FixPreview(fix_preview.clone()),
                "Fix Preview",
            ),
            (
                DashboardOverlay::ConfirmFixApply(fix_preview),
                "Confirm Fix Apply",
            ),
            (
                DashboardOverlay::TodoPrompt(TodoPromptDraft::schedule(todo_row)),
                "Schedule Todo",
            ),
            (
                DashboardOverlay::Yank(
                    PanelRow::Diagnostic(diagnostic(7, "warning", "reference.missing"))
                        .yank_overlay(&frame.root),
                ),
                "Yank",
            ),
            (
                DashboardOverlay::DiagnosticFilter(DiagnosticFilterDraft {
                    code: "reference".to_owned(),
                    path: "notes".to_owned(),
                    active: DiagnosticFilterField::Code,
                }),
                "Diagnostic Filters",
            ),
            (DashboardOverlay::EventLog, "Log"),
        ];

        for (overlay, title) in cases {
            let backend = TestBackend::new(56, 22);
            let mut terminal = Terminal::new(backend).expect("terminal");
            terminal
                .draw(|area| {
                    render_dashboard_with_state(
                        area,
                        &frame,
                        DashboardRenderState::for_frame(&frame),
                        &overlay,
                        events.last(),
                        &events,
                    )
                })
                .expect("draw");
            let rendered = buffer_to_string(terminal.backend().buffer());

            assert!(rendered.contains("Zorg Dash"), "{title}\n{rendered}");
            assert!(rendered.contains("Keys"), "{title}\n{rendered}");
            assert!(rendered.contains(title), "{title}\n{rendered}");
        }
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

        let backend = TestBackend::new(52, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Filters 1/2: severity=error code=reference"));
        assert!(!rendered.contains("syntax.sort"));
    }

    #[test]
    fn render_narrow_today_shows_mode_counts_without_hidden_rows() {
        let mut frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(
                vec![
                    PanelRow::Zettel(zettel(1, "task")),
                    PanelRow::Diagnostic(diagnostic(1, "error", "reference.missing")),
                ],
                Vec::new(),
                vec![IndexStatusRow::new("Diagnostics", 1)],
            ),
        );
        frame.today_mode = TodayMode::TodosOnly;

        let backend = TestBackend::new(56, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Today: todos  rows 1  todos 1  diagnostics 1/1"));
        assert!(rendered.contains("@task"));
        assert!(!rendered.contains("reference.missing"));
    }

    #[test]
    fn render_today_empty_state_distinguishes_mode() {
        let mut frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(
                vec![PanelRow::Diagnostic(diagnostic(
                    1,
                    "warning",
                    "reference.missing",
                ))],
                Vec::new(),
                Vec::new(),
            ),
        );
        frame.today_mode = TodayMode::TodosOnly;

        let backend = TestBackend::new(72, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("No Today todo rows."));
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
    fn render_marked_diagnostic_count_and_row_glyph() {
        let mut frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(
                Vec::new(),
                vec![diagnostic(1, "warning", "reference.missing")],
                vec![IndexStatusRow::new("Diagnostics", 1)],
            ),
        );
        let diagnostic = match frame.active_rows().first() {
            Some(PanelRow::Diagnostic(row)) => row.clone(),
            _ => panic!("diagnostic row"),
        };
        frame.toggle_diagnostic_mark(&diagnostic);

        let backend = TestBackend::new(100, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("marked 1"));
        assert!(rendered.contains(">*warning"));
    }

    #[test]
    fn render_fix_preview_overlay_summarizes_marked_diagnostics() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let overlay = DashboardOverlay::FixPreview(FixPreviewOverlay {
            diagnostic: crate::model::DiagnosticPreviewContext {
                severity: "info".to_owned(),
                code: "marked.diagnostics".to_owned(),
                message: "Marked diagnostics are queued for review.".to_owned(),
                path: "-".to_owned(),
                position: "-".to_owned(),
            },
            previews: Vec::new(),
            unavailable_reason: Some("Bulk apply is unavailable.".to_owned()),
            selector: Default::default(),
            marked_summary: Some(crate::model::MarkedDiagnosticsSummary {
                marked_count: 1,
                selected_is_marked: false,
                selected_file_marked_count: None,
                rows: vec![crate::model::MarkedDiagnosticSummaryRow {
                    severity: "warning".to_owned(),
                    code: "reference.missing".to_owned(),
                    path: "notes/a.z".to_owned(),
                    position: "1:1-1:2".to_owned(),
                    message: "missing link".to_owned(),
                }],
            }),
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

        assert!(rendered.contains("Marked diagnostics: 1"));
        assert!(rendered.contains("Bulk apply for marked diagnostics is unavailable"));
        assert!(rendered.contains("reference.missing"));
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
        let theme = DashTheme::new(ColorMode::Enabled);
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

        assert_text_has_semantic_style(
            buffer,
            "err.code",
            "diagnostic error severity",
            theme.severity(SeverityKind::Error),
        );
        assert_text_has_semantic_style(
            buffer,
            "warn.code",
            "diagnostic warning severity",
            theme.severity(SeverityKind::Warning),
        );
        assert_text_has_semantic_style(
            buffer,
            "info.code",
            "diagnostic info severity",
            theme.severity(SeverityKind::Info),
        );
        assert_text_has_semantic_style(
            buffer,
            "unknown.code",
            "diagnostic unknown severity",
            theme.severity(SeverityKind::Unknown),
        );
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

        assert_buffer_has_no_colors(
            terminal.backend().buffer(),
            "disabled dashboard render should reset every cell",
        );
    }

    #[test]
    fn dash_theme_disabled_tokens_reset_foreground_and_background() {
        let theme = DashTheme::new(ColorMode::Disabled);
        let index_attention = IndexStatusRow::new("Diagnostics", 1);

        for (name, style) in [
            ("app_background", theme.app_background()),
            ("panel_surface", theme.panel_surface()),
            ("elevated_overlay_surface", theme.elevated_overlay_surface()),
            ("subtle_border", theme.subtle_border()),
            ("active_border", theme.active_border()),
            ("warning_border", theme.warning_border()),
            ("error_border", theme.error_border()),
            ("title", theme.title()),
            ("active_title", theme.active_title()),
            ("body_text", theme.body_text()),
            ("muted_text", theme.muted_text()),
            ("selection", theme.selection()),
            ("marked_row", theme.marked_row()),
            ("severity", theme.severity(SeverityKind::Error)),
            ("health", theme.health("degraded")),
            ("index_row", theme.index_row(&index_attention)),
            ("status", theme.status(SeverityKind::Info)),
            ("todo_accent", theme.todo_accent()),
            ("query_accent", theme.query_accent()),
            ("dashboard_accent", theme.dashboard_accent()),
            ("graph_link", theme.graph_link()),
            ("path", theme.path()),
            ("key_hint", theme.key_hint()),
        ] {
            assert_disabled_style_resets(name, style);
        }
    }

    #[test]
    fn render_index_health_and_attention_counts_are_colored() {
        let theme = DashTheme::new(ColorMode::Enabled);
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
                queries: QueryPanel::empty(),
                search: SearchPanel::empty(""),
                selected_dashboard: None,
                custom_panels: BTreeMap::new(),
            },
        );
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let buffer = terminal.backend().buffer();

        assert_text_has_semantic_style(
            buffer,
            "stale",
            "index stale health",
            theme.health("stale"),
        );
        assert_text_has_semantic_style(
            buffer,
            "New files",
            "index new files attention",
            theme.index_row(&IndexStatusRow::new("New files", 1)),
        );
        assert_text_has_semantic_style(
            buffer,
            "Diagnostics",
            "index diagnostics attention",
            theme.index_row(&IndexStatusRow::new("Diagnostics", 1)),
        );
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
            queries: QueryPanel::empty(),
            search: SearchPanel::empty(""),
            selected_dashboard: None,
            custom_panels: BTreeMap::new(),
        }
    }

    fn zettel(store_id: i64, title: &str) -> ZettelRow {
        ZettelRow {
            store_id,
            canonical_id: Some(title.to_owned()),
            file_path: PathBuf::from(format!("notes/{title}.z")),
            title: title.to_owned(),
            todo_marker: Some("[ ]".to_owned()),
            todo_span: Some(SourceSpan::bytes(0, 3)),
            source_span: SourceSpan::bytes(0, 24),
            source_order: store_id,
            start_line: Some(1),
            start_column: Some(1),
            lifecycle_date: None,
            tags: Vec::new(),
            properties: Vec::new(),
            preview: None,
            badges: vec![QueryBadge::new("due", "#z/todo")],
        }
    }

    fn graph_neighborhood() -> GraphNeighborhood {
        let selected = graph_zettel(1, "task");
        let target = graph_zettel(2, "target");
        let source = graph_zettel(3, "source");
        let ancestor = graph_zettel(4, "ancestor");
        let descendant = graph_zettel(5, "descendant");
        GraphNeighborhood {
            selected: selected.clone(),
            outgoing: GraphSection {
                total_count: 3,
                rows: vec![
                    GraphLinkRow {
                        link_id: 1,
                        source: Some(selected.clone()),
                        target: Some(target),
                        target_text: "@target".to_owned(),
                        target_canonical_id: Some("target".to_owned()),
                        link_kind: "id".to_owned(),
                        resolved: true,
                        source_span: span_at(2, 8),
                    },
                    GraphLinkRow {
                        link_id: 2,
                        source: Some(selected.clone()),
                        target: None,
                        target_text: "@missing-id".to_owned(),
                        target_canonical_id: Some("missing-id".to_owned()),
                        link_kind: "id".to_owned(),
                        resolved: false,
                        source_span: span_at(3, 4),
                    },
                ],
                truncated_count: 1,
            },
            incoming: GraphSection {
                total_count: 1,
                rows: vec![GraphLinkRow {
                    link_id: 3,
                    source: Some(source),
                    target: Some(selected),
                    target_text: "@task".to_owned(),
                    target_canonical_id: Some("task".to_owned()),
                    link_kind: "id".to_owned(),
                    resolved: true,
                    source_span: span_at(5, 2),
                }],
                truncated_count: 0,
            },
            ancestors: GraphSection {
                total_count: 1,
                rows: vec![ancestor],
                truncated_count: 0,
            },
            descendants: GraphSection {
                total_count: 1,
                rows: vec![descendant],
                truncated_count: 0,
            },
        }
    }

    fn graph_zettel(store_id: i64, title: &str) -> GraphZettelRow {
        GraphZettelRow {
            store_id,
            canonical_id: Some(title.to_owned()),
            title: title.to_owned(),
            file_path: PathBuf::from(format!("notes/{title}.z")),
            source_order: store_id,
            start_line: Some(store_id as usize),
            start_column: Some(1),
            source_span: span_at(store_id as usize, 1),
        }
    }

    fn span_at(line: usize, column: usize) -> SourceSpan {
        SourceSpan {
            start_byte: 0,
            end_byte: 1,
            start_line: Some(line),
            start_column: Some(column),
            end_line: Some(line),
            end_column: Some(column + 1),
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

    fn first_cell_for_text<'a>(buffer: &'a Buffer, text: &str) -> &'a ratatui::buffer::Cell {
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

    fn assert_text_has_semantic_style(
        buffer: &Buffer,
        text: &str,
        semantic_name: &str,
        expected: Style,
    ) {
        let first_cell = first_cell_for_text(buffer, text);
        if cell_matches_style(first_cell, expected) {
            return;
        }

        let mut found_text = false;
        for y in buffer.area.top()..buffer.area.bottom() {
            let mut line = String::new();
            for x in buffer.area.left()..buffer.area.right() {
                line.push_str(buffer[(x, y)].symbol());
            }

            let mut start = 0;
            while let Some(offset) = line[start..].find(text) {
                found_text = true;
                let x = buffer.area.left() + (start + offset) as u16;
                if cell_matches_style(&buffer[(x, y)], expected) {
                    return;
                }
                start += offset + text.len();
            }
        }

        if found_text {
            panic!("{text:?} should include semantic style {semantic_name}");
        }
        panic!("buffer should contain {text:?}");
    }

    fn cell_matches_style(cell: &ratatui::buffer::Cell, expected: Style) -> bool {
        if let Some(fg) = expected.fg {
            if cell.fg != fg {
                return false;
            }
        }
        if let Some(bg) = expected.bg {
            if cell.bg != bg {
                return false;
            }
        }
        cell.modifier == expected.add_modifier
    }

    fn assert_buffer_has_no_colors(buffer: &Buffer, context: &str) {
        for y in buffer.area.top()..buffer.area.bottom() {
            for x in buffer.area.left()..buffer.area.right() {
                let cell = &buffer[(x, y)];
                assert_eq!(cell.fg, Color::Reset, "{context} fg at ({x}, {y})");
                assert_eq!(cell.bg, Color::Reset, "{context} bg at ({x}, {y})");
            }
        }
    }

    fn assert_disabled_style_resets(semantic_name: &str, style: Style) {
        assert_eq!(style.fg, Some(Color::Reset), "{semantic_name} fg");
        assert_eq!(style.bg, Some(Color::Reset), "{semantic_name} bg");
    }
}
