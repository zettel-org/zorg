use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::model::{
    CaptureDraft, CaptureField, CaptureTemplatePicker, ColorMode, DashboardFrame, DashboardOverlay,
    DashboardRenderState, DashboardSnapshot, DiagnosticFilterDraft, DiagnosticFilterField,
    FixPreviewOverlay, FixPreviewRow, Panel, PanelRow, PendingActivity, SeverityKind, StatusEvent,
    TodayMode, TodoActionOverlay, TodoPromptDraft, YankOverlay, YankValueKind, format_duration,
    query_output_kind_label, query_source_kind_label, todo_date_field_label,
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

    fn chrome_border(self) -> Style {
        self.enabled_style(Style::default().fg(Color::Blue))
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

    fn warning_title(self) -> Style {
        self.enabled_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    }

    fn error_title(self) -> Style {
        self.enabled_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
    }

    fn chrome_title(self) -> Style {
        self.enabled_style(
            Style::default()
                .fg(Color::White)
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
    Warning,
    Error,
}

impl BlockRole {
    fn surface_style(self, theme: DashTheme) -> Style {
        match self {
            Self::Elevated | Self::Warning | Self::Error => theme.elevated_overlay_surface(),
            _ => theme.panel_surface(),
        }
    }

    fn border_style(self, theme: DashTheme) -> Style {
        match self {
            Self::Active => theme.active_border(),
            Self::Status => theme.chrome_border(),
            Self::Warning => theme.warning_border(),
            Self::Error => theme.error_border(),
            Self::Footer | Self::Subtle | Self::Elevated => theme.subtle_border(),
        }
    }

    fn title_style(self, theme: DashTheme) -> Style {
        match self {
            Self::Active => theme.active_title(),
            Self::Status => theme.chrome_title(),
            Self::Warning => theme.warning_title(),
            Self::Error => theme.error_title(),
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

// Overlay renderers keep modal chrome centralized in OverlaySpec, compose body
// rows through semantic span helpers, and preserve explicit text affordances so
// disabled-color output remains readable without relying on styling.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum OverlayTone {
    Neutral,
    Warning,
    Error,
    Destructive,
}

impl OverlayTone {
    fn block_role(self) -> BlockRole {
        match self {
            Self::Neutral => BlockRole::Elevated,
            Self::Warning => BlockRole::Warning,
            Self::Error | Self::Destructive => BlockRole::Error,
        }
    }

    fn body_style(self, theme: DashTheme) -> Style {
        theme.elevated_overlay_surface()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct OverlaySpec {
    title: String,
    width_percent: u16,
    height_percent: u16,
    tone: OverlayTone,
}

impl OverlaySpec {
    fn new(title: impl Into<String>, width_percent: u16, height_percent: u16) -> Self {
        Self {
            title: title.into(),
            width_percent,
            height_percent,
            tone: OverlayTone::Neutral,
        }
    }

    fn tone(mut self, tone: OverlayTone) -> Self {
        self.tone = tone;
        self
    }

    fn block(&self, theme: &DashTheme) -> Block<'static> {
        overlay_block(self.title.clone(), self.tone.block_role(), theme)
    }
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

fn overlay_form_row_line(
    selected: bool,
    label: impl Into<String>,
    required: bool,
    value: impl Into<String>,
    theme: &DashTheme,
) -> Line<'static> {
    overlay_form_row_line_from_spans(
        selected,
        label,
        required,
        vec![value_span(value, theme.body_text())],
        theme,
    )
}

fn overlay_form_row_line_from_spans(
    selected: bool,
    label: impl Into<String>,
    required: bool,
    value_spans: Vec<Span<'static>>,
    theme: &DashTheme,
) -> Line<'static> {
    let prefix = if selected { "> " } else { "  " };
    let required_marker = if required { " *" } else { "" };
    let mut spans = vec![
        value_span(prefix.to_owned(), theme.body_text()),
        label_span(format!("{}{required_marker}: ", label.into()), theme),
    ];
    spans.extend(value_spans);
    overlay_selected_line(spans, selected, theme)
}

fn overlay_selected_line(
    mut spans: Vec<Span<'static>>,
    selected: bool,
    theme: &DashTheme,
) -> Line<'static> {
    if selected {
        apply_selected_style(&mut spans, theme);
    }
    Line::from(spans)
}

fn overlay_section_heading_line(text: impl Into<String>, theme: &DashTheme) -> Line<'static> {
    Line::from(value_span(text, theme.active_title()))
}

fn overlay_instruction_line(text: impl Into<String>, theme: &DashTheme) -> Line<'static> {
    Line::from(metadata_span(text, theme))
}

#[allow(dead_code)]
fn overlay_key_help_line(
    key: impl Into<String>,
    action: impl Into<String>,
    theme: &DashTheme,
) -> Line<'static> {
    Line::from(vec![
        key_hint_span(key, theme),
        metadata_span(" ", theme),
        value_span(action, theme.body_text()),
    ])
}

#[allow(dead_code)]
fn overlay_warning_line(text: impl Into<String>, theme: &DashTheme) -> Line<'static> {
    Line::from(vec![
        badge_span("Warning: ", BadgeTone::Status(SeverityKind::Warning), theme),
        value_span(text, theme.status(SeverityKind::Warning)),
    ])
}

fn overlay_error_line(text: impl Into<String>, theme: &DashTheme) -> Line<'static> {
    Line::from(vec![
        badge_span("Error: ", BadgeTone::Status(SeverityKind::Error), theme),
        value_span(text, theme.status(SeverityKind::Error)),
    ])
}

#[derive(Debug, Clone)]
struct RowRender {
    spans: Vec<Span<'static>>,
    base_style: Style,
    marked: bool,
}

impl RowRender {
    fn new(spans: Vec<Span<'static>>, base_style: Style, marked: bool) -> Self {
        Self {
            spans,
            base_style,
            marked,
        }
    }

    fn into_line(self, selected: bool, theme: &DashTheme) -> Line<'static> {
        let mut spans = Vec::with_capacity(self.spans.len() + 1);
        spans.push(value_span(
            row_prefix(selected, self.marked),
            self.base_style,
        ));
        spans.extend(self.spans);
        if self.marked {
            apply_marked_style(&mut spans, theme);
        }
        if selected {
            apply_selected_style(&mut spans, theme);
        }
        Line::from(spans)
    }
}

fn row_prefix(selected: bool, marked: bool) -> String {
    format!(
        "{}{}",
        if selected { ">" } else { " " },
        if marked { "*" } else { " " }
    )
}

fn apply_selected_style(spans: &mut [Span<'static>], theme: &DashTheme) {
    for span in spans {
        span.style = selected_style(span.style, theme);
    }
}

fn apply_marked_style(spans: &mut [Span<'static>], theme: &DashTheme) {
    let marked_style = theme.marked_row();
    for span in spans {
        span.style = span.style.patch(marked_style);
    }
}

fn render_status(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    frame: &DashboardFrame,
    pending_activity: Option<&PendingActivity>,
    palette: &DashTheme,
) {
    let content_width = area.width.saturating_sub(2);
    let show_detail_counts = pending_activity.is_none() || content_width >= 96;
    let show_row_counts = pending_activity.is_none() && content_width >= 92 || content_width >= 128;
    let health_label = frame.health_label();
    let mut spans = Vec::new();

    push_status_group(
        &mut spans,
        "index ",
        health_label,
        palette.health(health_label),
        palette,
    );
    push_status_group(
        &mut spans,
        "diagnostics ",
        frame.diagnostics_label(),
        match &frame.snapshot {
            DashboardSnapshot::Ready { index, .. } if index.diagnostic_count > 0 => {
                palette.severity(SeverityKind::Error)
            }
            _ => palette.emphasis(),
        },
        palette,
    );
    if show_detail_counts {
        push_status_group(
            &mut spans,
            "freshness ",
            frame.freshness_label(),
            palette.health(frame.freshness_label()),
            palette,
        );
        push_status_group(
            &mut spans,
            "marked ",
            frame.marked_diagnostic_count().to_string(),
            if frame.marked_diagnostic_count() > 0 {
                palette.severity(SeverityKind::Warning)
            } else {
                palette.emphasis()
            },
            palette,
        );
    }
    push_status_group(
        &mut spans,
        "panel ",
        frame.active_panel_id().key().to_owned(),
        palette.emphasis(),
        palette,
    );
    if let Some(activity) = pending_activity {
        push_status_group(
            &mut spans,
            "pending ",
            pending_activity_label(activity, content_width),
            palette.status(SeverityKind::Info),
            palette,
        );
    }
    if show_row_counts {
        push_status_group(
            &mut spans,
            "rows ",
            frame.telemetry.row_counts.status_label(),
            palette.emphasis(),
            palette,
        );
    }
    let status = vec![Line::from(spans)];
    terminal_frame.render_widget(Paragraph::new(status).block(status_block(palette)), area);
}

fn push_status_group(
    spans: &mut Vec<Span<'static>>,
    label: impl Into<String>,
    value: impl Into<String>,
    value_style: Style,
    theme: &DashTheme,
) {
    if !spans.is_empty() {
        spans.push(metadata_span(" ", theme));
    }
    spans.extend(label_value_spans(label, value, value_style, theme));
}

fn pending_activity_label(activity: &PendingActivity, content_width: u16) -> String {
    if content_width < 70 {
        activity.operation.label().to_owned()
    } else {
        format!(
            "{} {} {}",
            activity.spinner(),
            activity.operation.label(),
            format_duration(activity.elapsed)
        )
    }
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
                    selected_span("> ", palette.body_text(), palette),
                    selected_span(panel.label.clone(), palette.body_text(), palette),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    metadata_span("  ", palette),
                    metadata_span(panel.label.clone(), palette),
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
        .map(|line| inspector_line_for_render(line, palette))
        .collect::<Vec<_>>();

    terminal_frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(panel_block("Inspector", BlockRole::Subtle, palette)),
        area,
    );
}

fn inspector_line_for_render(text: String, theme: &DashTheme) -> Line<'static> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Line::from("");
    }

    let indent_len = text.len().saturating_sub(text.trim_start().len());
    let indent = &text[..indent_len];
    let content = &text[indent_len..];
    if let Some((label, value)) = content.split_once(':') {
        if is_inspector_section_heading(label.trim()) {
            return inspector_section_count_line(indent, label, value, theme);
        }
        if is_inspector_label(label.trim()) {
            return inspector_label_value_line(indent, label, value, theme);
        }
    }

    if is_inspector_section_heading(trimmed) {
        return Line::from(vec![
            Span::raw(indent.to_owned()),
            Span::styled(trimmed.to_owned(), inspector_heading_style(trimmed, theme)),
        ]);
    }

    if is_inspector_warning_or_unavailable(trimmed) {
        return Line::from(vec![
            Span::raw(indent.to_owned()),
            Span::styled(content.to_owned(), inspector_warning_style(trimmed, theme)),
        ]);
    }

    if inspector_text_contains_linkish_value(trimmed) {
        return Line::from(inspector_value_spans(indent, content, theme));
    }

    if indent.is_empty() && !trimmed.ends_with('.') {
        return Line::from(value_span(text, theme.emphasis()));
    }

    Line::from(text)
}

fn inspector_section_count_line(
    indent: &str,
    label: &str,
    value: &str,
    theme: &DashTheme,
) -> Line<'static> {
    let spacing = value
        .chars()
        .take_while(|character| character.is_whitespace())
        .collect::<String>();
    let value = value.trim_start();
    Line::from(vec![
        Span::raw(indent.to_owned()),
        Span::styled(
            label.to_owned(),
            inspector_heading_style(label.trim(), theme),
        ),
        metadata_span(format!(":{spacing}"), theme),
        metadata_span(value.to_owned(), theme),
    ])
}

fn inspector_label_value_line(
    indent: &str,
    label: &str,
    value: &str,
    theme: &DashTheme,
) -> Line<'static> {
    let spacing = value
        .chars()
        .take_while(|character| character.is_whitespace())
        .collect::<String>();
    let value = value.trim_start();
    let mut spans = vec![
        Span::raw(indent.to_owned()),
        label_span(format!("{label}:{spacing}"), theme),
    ];
    spans.extend(inspector_value_spans_for_label(label.trim(), value, theme));
    Line::from(spans)
}

fn inspector_value_spans_for_label(
    label: &str,
    value: &str,
    theme: &DashTheme,
) -> Vec<Span<'static>> {
    match label {
        "Absolute path" | "Database" | "Path" | "Root" => vec![path_span(value.to_owned(), theme)],
        "Health" | "Snapshot freshness" => {
            vec![value_span(value.to_owned(), theme.health(value))]
        }
        "ID" | "Zettel row" => vec![id_span(value.to_owned(), theme)],
        "Severity" => vec![value_span(
            value.to_owned(),
            theme.severity(SeverityKind::from_label(value)),
        )],
        _ if label.to_ascii_lowercase().contains("error") => {
            vec![value_span(
                value.to_owned(),
                theme.severity(SeverityKind::Error),
            )]
        }
        "Definition" | "Tags" | "Source" | "Output" => inspector_value_spans("", value, theme),
        _ => {
            if inspector_text_contains_linkish_value(value) {
                inspector_value_spans("", value, theme)
            } else {
                vec![value_span(value.to_owned(), theme.body_text())]
            }
        }
    }
}

fn inspector_value_spans(prefix: &str, text: &str, theme: &DashTheme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if !prefix.is_empty() {
        spans.push(Span::raw(prefix.to_owned()));
    }
    for token in text.split_inclusive(char::is_whitespace) {
        let token_text = token.to_owned();
        let token_trimmed = token.trim_end();
        if token_trimmed == "->" || token_trimmed == "<-" || token_trimmed.starts_with('@') {
            spans.push(link_span(token_text, theme));
        } else if token_trimmed.starts_with('#') {
            spans.push(id_span(token_text, theme));
        } else if looks_like_source_path(token_trimmed) {
            spans.push(path_span(token_text, theme));
        } else {
            spans.push(value_span(token_text, theme.body_text()));
        }
    }
    spans
}

fn is_inspector_section_heading(text: &str) -> bool {
    matches!(
        text,
        "Graph context"
            | "Outgoing links"
            | "Incoming backlinks"
            | "Ancestors"
            | "Descendants"
            | "Properties"
            | "Preview"
            | "Telemetry"
            | "Today queries"
            | "Rows"
            | "Stored query"
            | "Search query"
            | "Query error"
            | "Index metadata"
    ) || text.starts_with("Telemetry (")
}

fn is_inspector_label(text: &str) -> bool {
    matches!(
        text,
        "Absolute path"
            | "Bytes"
            | "Category"
            | "Code"
            | "Database"
            | "Definition"
            | "Definition error"
            | "Diagnostic"
            | "Error"
            | "Fix"
            | "Health"
            | "ID"
            | "Initial load"
            | "Input"
            | "Last indexed"
            | "Last refresh"
            | "Lifecycle"
            | "Output"
            | "Path"
            | "Position"
            | "Root"
            | "Row count preview"
            | "Row count preview error"
            | "Run"
            | "Schema version"
            | "Severity"
            | "Snapshot freshness"
            | "Source"
            | "Status"
            | "Tags"
            | "Title"
            | "Todo"
            | "Value"
            | "Zettel row"
    )
}

fn inspector_heading_style(text: &str, theme: &DashTheme) -> Style {
    match text {
        "Outgoing links" | "Incoming backlinks" | "Ancestors" | "Descendants" => {
            DomainTone::Link.style(*theme)
        }
        "Stored query" | "Search query" | "Query error" => theme.query_accent(),
        "Index metadata" | "Telemetry" => theme.chrome_title(),
        _ => theme.active_title(),
    }
}

fn is_inspector_warning_or_unavailable(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("unavailable")
        || lower.contains("error")
        || lower.contains("failed")
        || lower.contains("invalid")
        || lower.contains("warning")
}

fn inspector_warning_style(text: &str, theme: &DashTheme) -> Style {
    let lower = text.to_ascii_lowercase();
    if lower.contains("warning") {
        theme.severity(SeverityKind::Warning)
    } else {
        theme.severity(SeverityKind::Error)
    }
}

fn inspector_text_contains_linkish_value(text: &str) -> bool {
    text.split_whitespace().any(|token| {
        token == "->"
            || token == "<-"
            || token.starts_with('@')
            || token.starts_with('#')
            || looks_like_source_path(token)
    })
}

fn looks_like_source_path(text: &str) -> bool {
    let trimmed = text.trim_matches(|character: char| {
        matches!(character, ',' | ';' | ')' | '(' | '[' | ']' | '"' | '\'')
    });
    trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.ends_with(".z")
        || trimmed.ends_with(".zo")
        || trimmed.ends_with(".zot")
        || trimmed.ends_with(".zoq")
        || trimmed.ends_with(".sqlite3")
}

fn render_footer(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    latest_status: Option<&StatusEvent>,
    palette: &DashTheme,
) {
    let (key_width, latest_width) = footer_widths(area.width);
    let footer = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(key_width), Constraint::Length(latest_width)])
        .split(area);
    let key_help = footer_key_help_line(footer[0].width.saturating_sub(2), palette);
    terminal_frame.render_widget(
        Paragraph::new(key_help).block(footer_block("Keys", palette)),
        footer[0],
    );

    let (text, style) = latest_status
        .map(|event| (event.message.as_str(), palette.status(event.severity)))
        .unwrap_or(("idle", palette.muted_text()));
    terminal_frame.render_widget(
        Paragraph::new(value_span(text, style)).block(footer_block("Latest", palette)),
        footer[1],
    );
}

fn footer_widths(area_width: u16) -> (u16, u16) {
    let (minimum_keys, latest_target) = if area_width < 54 {
        (20, 14)
    } else if area_width < 72 {
        (36, 24)
    } else {
        (48, area_width.saturating_mul(30) / 100)
    };
    let latest_width = latest_target
        .min(area_width.saturating_sub(minimum_keys))
        .max(1);
    (minimum_keys, latest_width)
}

fn footer_key_help_line(content_width: u16, palette: &DashTheme) -> Line<'static> {
    if content_width < 28 {
        return Line::from(vec![
            key_hint_span("q", palette),
            metadata_span(" ", palette),
            key_hint_span("?", palette),
            metadata_span(" ", palette),
            key_hint_span("L", palette),
            metadata_span(" ", palette),
            key_hint_span("y", palette),
        ]);
    }
    if content_width < 58 {
        return Line::from(vec![
            key_hint_span("q", palette),
            metadata_span(" quit ", palette),
            key_hint_span("?", palette),
            metadata_span(" help ", palette),
            key_hint_span("L", palette),
            metadata_span(" log ", palette),
            key_hint_span("y", palette),
            metadata_span(" yank", palette),
        ]);
    }

    Line::from(vec![
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
    ])
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
            let selected = index == selected_index;
            ListItem::new(row_render(row, frame, palette).into_line(selected, palette))
        })
        .collect()
}

fn row_render(row: &PanelRow, frame: &DashboardFrame, palette: &DashTheme) -> RowRender {
    match row {
        PanelRow::Zettel(zettel) => zettel_row_render(zettel, frame.is_row_marked(row), palette),
        PanelRow::Diagnostic(row) => diagnostic_row_render(row, frame, palette),
        PanelRow::Query(row) => query_row_render(row, false, palette),
        PanelRow::IndexStatus(row) => index_status_row_render(row, palette),
    }
}

fn zettel_row_render(
    row: &crate::model::ZettelRow,
    marked: bool,
    palette: &DashTheme,
) -> RowRender {
    let marker = row.todo_marker.as_deref().unwrap_or("   ");
    let id = row
        .canonical_id
        .as_deref()
        .map(|id| format!("@{id}"))
        .unwrap_or_else(|| "-".to_owned());
    let title = if row.title.trim().is_empty() {
        row.preview.as_deref().unwrap_or("-")
    } else {
        row.title.as_str()
    };

    let mut spans = vec![
        value_span(format!("{marker:<3} "), palette.todo_accent()),
        if row.canonical_id.is_some() {
            id_span(format!("{id:<24} "), palette)
        } else {
            metadata_span(format!("{id:<24} "), palette)
        },
        value_span(title.to_owned(), palette.body_text()),
    ];
    for badge in &row.badges {
        spans.push(metadata_span(" ", palette));
        spans.push(badge_span(
            format!("[{}]", badge.label),
            BadgeTone::Domain(DomainTone::Query),
            palette,
        ));
    }
    spans.push(metadata_span("  ", palette));
    spans.push(path_span(source_path_text(&row.file_path), palette));
    if let Some(date) = &row.lifecycle_date {
        spans.push(metadata_span("  ", palette));
        spans.push(label_span("date ", palette));
        spans.push(metadata_span(date.clone(), palette));
    }

    RowRender::new(spans, palette.body_text(), marked)
}

fn query_row_render(row: &crate::model::QueryRow, marked: bool, palette: &DashTheme) -> RowRender {
    let (badge, tone) = query_row_badge(row);
    let mut spans = vec![
        badge_span(format!("{badge:<7} "), tone, palette),
        value_span(format!("@{:<24} ", row.id), palette.query_accent()),
        value_span(format!("{:<20} ", row.title), palette.body_text()),
        label_span("rows ", palette),
        value_span(query_row_count_text(row), palette.emphasis()),
    ];

    if let Some(path) = &row.source_path {
        spans.push(metadata_span("  ", palette));
        spans.push(path_span(source_path_text(path), palette));
    }

    if let Some(source_kind) = row.source_kind {
        spans.push(metadata_span("  ", palette));
        spans.push(metadata_span(query_source_kind_label(source_kind), palette));
    }
    if let Some(output_kind) = row.output_kind {
        spans.push(metadata_span(" / ", palette));
        spans.push(metadata_span(query_output_kind_label(output_kind), palette));
    }

    if let Some(error) = &row.error {
        spans.push(metadata_span("  ", palette));
        spans.push(badge_span(
            first_nonempty_line(error).to_owned(),
            BadgeTone::Severity(SeverityKind::Warning),
            palette,
        ));
    } else if let Some(error) = &row.row_count_error {
        spans.push(metadata_span("  ", palette));
        spans.push(badge_span(
            first_nonempty_line(error).to_owned(),
            BadgeTone::Severity(SeverityKind::Error),
            palette,
        ));
    }

    RowRender::new(spans, palette.body_text(), marked)
}

fn query_row_badge(row: &crate::model::QueryRow) -> (&'static str, BadgeTone) {
    if !row.valid {
        ("warning", BadgeTone::Severity(SeverityKind::Warning))
    } else if row.row_count_error.is_some() {
        ("error", BadgeTone::Severity(SeverityKind::Error))
    } else {
        ("ok", BadgeTone::Status(SeverityKind::Info))
    }
}

fn query_row_count_text(row: &crate::model::QueryRow) -> String {
    row.row_count_preview
        .map(|count| count.to_string())
        .unwrap_or_else(|| "-".to_owned())
}

fn diagnostic_row_render(
    row: &crate::model::DiagnosticRow,
    frame: &DashboardFrame,
    palette: &DashTheme,
) -> RowRender {
    let severity = row.severity_kind();
    let code = diagnostic_code(row);
    let marked = frame.is_diagnostic_marked(row);

    if active_builtin(frame, Panel::Today) && frame.custom_panel.is_none() {
        return RowRender::new(
            vec![
                badge_span(
                    format!("{:<7} ", row.severity),
                    BadgeTone::Severity(severity),
                    palette,
                ),
                badge_span(
                    format!("{code:<30} "),
                    BadgeTone::Severity(severity),
                    palette,
                ),
                value_span(row.message.clone(), palette.body_text()),
            ],
            palette.body_text(),
            marked,
        );
    }

    RowRender::new(
        vec![
            badge_span(
                format!("{:<7} ", row.severity),
                BadgeTone::Severity(severity),
                palette,
            ),
            badge_span(
                format!("{code:<28} "),
                BadgeTone::Severity(severity),
                palette,
            ),
            value_span(format!("{:<30} ", row.message), palette.body_text()),
            path_span(format!("{:<28} ", diagnostic_path(row)), palette),
            metadata_span(diagnostic_position(row), palette),
        ],
        palette.body_text(),
        marked,
    )
}

fn index_status_row_render(row: &crate::model::IndexStatusRow, palette: &DashTheme) -> RowRender {
    let attention_style = palette.index_row(row);
    let has_attention_style = attention_style.fg.is_some()
        || attention_style.bg.is_some()
        || !attention_style.add_modifier.is_empty()
        || !attention_style.sub_modifier.is_empty();
    let (label_style, value_style) = if has_attention_style {
        (attention_style, attention_style)
    } else {
        (palette.muted_text(), palette.emphasis())
    };

    RowRender::new(
        vec![
            value_span(format!("{:<18} ", row.label), label_style),
            value_span(row.value.to_string(), value_style),
        ],
        palette.body_text(),
        false,
    )
}

fn diagnostic_code(row: &crate::model::DiagnosticRow) -> &str {
    row.code.as_deref().unwrap_or(row.category.as_str())
}

fn diagnostic_path(row: &crate::model::DiagnosticRow) -> String {
    row.relative_path
        .as_ref()
        .or(row.absolute_path.as_ref())
        .map(|path| source_path_text(path))
        .unwrap_or_else(|| "-".to_owned())
}

fn source_path_text(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn diagnostic_position(row: &crate::model::DiagnosticRow) -> String {
    let start = location_text(row.start_line, row.start_column);
    let end = location_text(row.end_line, row.end_column);
    if start == "-" || end == "-" || start == end {
        start
    } else {
        format!("{start}-{end}")
    }
}

fn location_text(line: Option<usize>, column: Option<usize>) -> String {
    match (line, column) {
        (Some(line), Some(column)) => format!("{line}:{column}"),
        (Some(line), None) => line.to_string(),
        _ => "-".to_owned(),
    }
}

fn render_overlay(
    terminal_frame: &mut ratatui::Frame<'_>,
    area: Rect,
    overlay: &DashboardOverlay,
    status_events: &[StatusEvent],
    palette: &DashTheme,
) {
    let Some((spec, lines)) = overlay_spec_and_lines(overlay, status_events, palette) else {
        return;
    };

    let overlay_area = centered_rect(spec.width_percent, spec.height_percent, area);
    terminal_frame.render_widget(Clear, overlay_area);
    terminal_frame.render_widget(
        Paragraph::new(lines)
            .style(spec.tone.body_style(*palette))
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true })
            .block(spec.block(palette)),
        overlay_area,
    );
}

fn overlay_spec_and_lines(
    overlay: &DashboardOverlay,
    status_events: &[StatusEvent],
    palette: &DashTheme,
) -> Option<(OverlaySpec, Vec<Line<'static>>)> {
    let spec_and_lines = match overlay {
        DashboardOverlay::None => return None,
        DashboardOverlay::Help => (OverlaySpec::new("Help", 96, 70), help_lines(palette)),
        DashboardOverlay::SwogHelp => (
            OverlaySpec::new("SWOG Help", 78, 70),
            swog_help_lines(palette),
        ),
        DashboardOverlay::ConfirmReindex => (
            OverlaySpec::new("Confirm Reindex", 66, 44).tone(OverlayTone::Warning),
            confirm_reindex_lines(palette),
        ),
        DashboardOverlay::ConfirmFixApply(preview) => (
            OverlaySpec::new("Confirm Fix Apply", 78, 70).tone(OverlayTone::Destructive),
            confirm_fix_apply_lines(preview, palette),
        ),
        DashboardOverlay::ConfirmTodoApply(todo) => (
            OverlaySpec::new(todo.title.as_str(), 78, 70).tone(OverlayTone::Destructive),
            confirm_todo_lines(todo, palette),
        ),
        DashboardOverlay::TodoPrompt(draft) => (
            OverlaySpec::new(draft.action.title(), 72, 50),
            todo_prompt_lines(draft, palette),
        ),
        DashboardOverlay::Yank(overlay) => (
            OverlaySpec::new("Yank", 72, 44),
            yank_lines(overlay, palette),
        ),
        DashboardOverlay::CapturePicker(picker) => (
            OverlaySpec::new("Capture Templates", 78, 62),
            capture_template_picker_lines(picker, palette),
        ),
        DashboardOverlay::Capture(draft) => (
            OverlaySpec::new("Capture", 72, 62),
            capture_lines(draft, palette),
        ),
        DashboardOverlay::DiagnosticFilter(draft) => (
            OverlaySpec::new("Diagnostic Filters", 66, 44),
            diagnostic_filter_lines(draft, palette),
        ),
        DashboardOverlay::FixPreview(preview) => (
            OverlaySpec::new("Fix Preview", 78, 70),
            fix_preview_lines(preview, palette),
        ),
        DashboardOverlay::EventLog => (
            OverlaySpec::new("Log", 66, 44),
            status_event_lines(status_events, palette),
        ),
        DashboardOverlay::Log { title, message } => (
            OverlaySpec::new(title.as_str(), 66, 44).tone(log_overlay_tone(title, message)),
            generic_log_lines(message, palette),
        ),
    };
    Some(spec_and_lines)
}

fn log_overlay_tone(title: &str, message: &str) -> OverlayTone {
    let title = title.to_ascii_lowercase();
    let message = message.to_ascii_lowercase();
    if title.contains("error")
        || title.contains("failed")
        || title.contains("failure")
        || message.contains("error")
        || message.contains("failed")
        || message.contains("failure")
    {
        OverlayTone::Error
    } else {
        OverlayTone::Neutral
    }
}

fn confirm_reindex_lines(palette: &DashTheme) -> Vec<Line<'static>> {
    vec![
        overlay_section_heading_line("Operation", palette),
        Line::from(vec![
            badge_span("Reindex", BadgeTone::Status(SeverityKind::Warning), palette),
            value_span(
                " will write a fresh SQLite snapshot for this corpus.",
                palette.body_text(),
            ),
        ]),
        Line::from(""),
        overlay_section_heading_line("Confirmation", palette),
        Line::from(vec![
            metadata_span("Press ", palette),
            key_hint_span("y or enter", palette),
            metadata_span(" to continue, ", palette),
            key_hint_span("n or Esc", palette),
            metadata_span(" to cancel.", palette),
        ]),
    ]
}

fn fix_preview_lines(preview: &FixPreviewOverlay, palette: &DashTheme) -> Vec<Line<'static>> {
    let diagnostic = &preview.diagnostic;
    let mut lines = vec![
        overlay_section_heading_line("Diagnostic context", palette),
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
        lines.push(overlay_section_heading_line("Candidate fixes", palette));
        lines.push(Line::from(fix_unavailable_spans(
            preview
                .unavailable_reason
                .clone()
                .unwrap_or_else(|| "No safe fix preview is available.".to_owned()),
            palette,
        )));
    } else {
        lines.push(overlay_section_heading_line("Candidate fixes", palette));
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
        lines.push(overlay_section_heading_line(
            "Marked diagnostics summary",
            palette,
        ));
        lines.push(label_value_line(
            "Marked diagnostics: ",
            summary.marked_count.to_string(),
            palette.emphasis(),
            palette,
        ));
        if summary.selected_is_marked {
            lines.push(Line::from(value_span(
                "Selected diagnostic is marked.",
                palette.body_text(),
            )));
        }
        if let Some(count) = summary.selected_file_marked_count {
            lines.push(Line::from(value_span(
                format!("Selected file has {count} marked diagnostic(s)."),
                palette.body_text(),
            )));
        }
        lines.push(overlay_warning_line(
            "Bulk apply for marked diagnostics is unavailable; apply one safe preview at a time.",
            palette,
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
            lines.push(Line::from(value_span(
                format!("  {}", row.message),
                palette.body_text(),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(overlay_section_heading_line("Action", palette));
    if preview.can_apply_selected_fix() {
        lines.push(overlay_instruction_line(
            "F applies selected safe fix after confirmation. Esc/q/f closes preview.",
            palette,
        ));
    } else {
        lines.push(overlay_instruction_line("Esc/q/f closes preview.", palette));
    }
    lines
}

fn help_lines(palette: &DashTheme) -> Vec<Line<'static>> {
    vec![
        help_group_line(
            "Close",
            [("q/Esc", "quit or close overlay"), ("L", "log")],
            palette,
        ),
        help_group_line(
            "Nav",
            [
                ("tab/backtab", "panels"),
                ("up/down/j/k", "move"),
                ("g/G", "first/last"),
                ("page up/down", "page"),
                ("ctrl-u/ctrl-d", "half"),
            ],
            palette,
        ),
        help_group_line(
            "Todos",
            [
                ("c", "capture"),
                ("t", "Today modes"),
                ("d", "done confirm"),
                ("p", "postpone date/+1d/+1w"),
                ("s", "schedule do::date"),
            ],
            palette,
        ),
        help_group_line(
            "Fixes",
            [
                ("f", "preview"),
                ("space", "mark"),
                ("e", "severity"),
                (":", "code/path"),
                ("a", "clear"),
                ("F", "confirm/apply safe fix"),
            ],
            palette,
        ),
        help_group_line(
            "Search/SWOG",
            [
                ("/", "edit query"),
                ("F1", "help"),
                (
                    "search edit",
                    "arrows/Home/End, Up/Down history, Ctrl-W word, Ctrl-U prefix",
                ),
                ("search edit", "SWOG/@query/id, Enter run, Esc cancel"),
            ],
            palette,
        ),
        help_group_line(
            "Open/yank",
            [
                ("enter", "run query/open source"),
                ("o", "$EDITOR"),
                ("y", "yank id/link/msg"),
                ("r", "refresh"),
                ("R", "reindex y/enter"),
            ],
            palette,
        ),
    ]
}

fn help_group_line<const N: usize>(
    label: impl Into<String>,
    entries: [(&'static str, &'static str); N],
    palette: &DashTheme,
) -> Line<'static> {
    let mut spans = vec![label_span(format!("{}: ", label.into()), palette)];
    for (index, (key, description)) in entries.iter().enumerate() {
        if index > 0 {
            spans.push(metadata_span("; ", palette));
        }
        spans.push(key_hint_span(*key, palette));
        spans.push(metadata_span(" ", palette));
        spans.push(value_span(*description, palette.body_text()));
    }
    Line::from(spans)
}

fn swog_help_lines(palette: &DashTheme) -> Vec<Line<'static>> {
    vec![
        overlay_section_heading_line("Sources", palette),
        Line::from(vec![
            label_span("Tags: ", palette),
            badge_span("#z/todo", BadgeTone::Domain(DomainTone::Todo), palette),
            metadata_span(", ", palette),
            link_span("#project/work", palette),
        ]),
        swog_labeled_tokens_line(
            "Properties: ",
            [
                ("due:<=today", SwogTokenTone::Value),
                (", ", SwogTokenTone::Muted),
                ("did:*", SwogTokenTone::Value),
                (", ", SwogTokenTone::Muted),
                ("area:work/zorg", SwogTokenTone::Value),
            ],
            palette,
        ),
        swog_labeled_tokens_line(
            "Todos: ",
            [
                ("todo:[ ]", SwogTokenTone::Value),
                (", ", SwogTokenTone::Muted),
                ("-did:*", SwogTokenTone::Operator),
            ],
            palette,
        ),
        Line::from(vec![
            label_span("Links: ", palette),
            link_span("links:#project/reference", palette),
        ]),
        swog_labeled_tokens_line(
            "Files/text: ",
            [
                ("file:notes.z", SwogTokenTone::Path),
                (" ", SwogTokenTone::Muted),
                ("text:\"alpha text\"", SwogTokenTone::Value),
            ],
            palette,
        ),
        swog_labeled_tokens_line(
            "Modified: ",
            [("modified:<7d", SwogTokenTone::Value)],
            palette,
        ),
        overlay_section_heading_line("Operators and examples", palette),
        Line::from(vec![
            label_span("Boolean: ", palette),
            badge_span("#z/todo", BadgeTone::Domain(DomainTone::Todo), palette),
            value_span(" OR ", palette.emphasis()),
            badge_span("#z/query", BadgeTone::Domain(DomainTone::Query), palette),
        ]),
        Line::from(vec![
            label_span("Grouping: ", palette),
            metadata_span("(", palette),
            badge_span("#z/todo", BadgeTone::Domain(DomainTone::Todo), palette),
            value_span(" OR ", palette.emphasis()),
            badge_span("#z/query", BadgeTone::Domain(DomainTone::Query), palette),
            metadata_span(") ", palette),
            value_span("-did:*", palette.emphasis()),
        ]),
        Line::from(vec![
            label_span("Stored query IDs: ", palette),
            id_span("@queries/foo", palette),
        ]),
        Line::from(vec![
            label_span("Output: ", palette),
            value_span("TABLE", palette.emphasis()),
            metadata_span(" ", palette),
            badge_span("#z/todo", BadgeTone::Domain(DomainTone::Todo), palette),
        ]),
        Line::from(vec![
            label_span("Aggregate: ", palette),
            value_span("count", palette.emphasis()),
            metadata_span("(", palette),
            badge_span("#z/todo", BadgeTone::Domain(DomainTone::Todo), palette),
            value_span(" OR ", palette.emphasis()),
            badge_span("#z/query", BadgeTone::Domain(DomainTone::Query), palette),
            metadata_span(")", palette),
        ]),
    ]
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum SwogTokenTone {
    Muted,
    Operator,
    Path,
    Value,
}

fn swog_labeled_tokens_line<const N: usize>(
    label: impl Into<String>,
    tokens: [(&'static str, SwogTokenTone); N],
    palette: &DashTheme,
) -> Line<'static> {
    let mut spans = vec![label_span(label, palette)];
    for (text, tone) in tokens {
        spans.push(match tone {
            SwogTokenTone::Muted => metadata_span(text, palette),
            SwogTokenTone::Operator => value_span(text, palette.emphasis()),
            SwogTokenTone::Path => path_span(text, palette),
            SwogTokenTone::Value => value_span(text, palette.body_text()),
        });
    }
    Line::from(spans)
}

fn confirm_fix_apply_lines(preview: &FixPreviewOverlay, palette: &DashTheme) -> Vec<Line<'static>> {
    let diagnostic = &preview.diagnostic;
    let mut lines = vec![
        overlay_section_heading_line("Operation", palette),
        Line::from(vec![
            badge_span(
                "Apply fix",
                BadgeTone::Status(SeverityKind::Warning),
                palette,
            ),
            value_span(" to disk?", palette.body_text()),
        ]),
        Line::from(""),
        overlay_section_heading_line("Target", palette),
        path_line("Path: ", diagnostic.path.clone(), palette),
        label_value_line(
            "Position: ",
            diagnostic.position.clone(),
            palette.emphasis(),
            palette,
        ),
        id_line("Diagnostic: ", diagnostic.code.clone(), palette),
        label_value_line(
            "Message: ",
            diagnostic.message.clone(),
            palette.body_text(),
            palette,
        ),
    ];

    if let Some(row) = preview.previews.iter().find(|row| row.is_safe) {
        lines.push(Line::from(""));
        lines.push(overlay_section_heading_line("Selected fix", palette));
        lines.push(id_line("Fix: ", row.rule_code.clone(), palette));
        lines.push(Line::from(fix_state_badge_spans(row, palette)));
        lines.push(label_value_line(
            "Edits: ",
            row.explanation.clone(),
            palette.body_text(),
            palette,
        ));
        lines.push(overlay_section_heading_line(
            "Replacement preview:",
            palette,
        ));
        lines.extend(replacement_preview_lines(&row.replacement_preview, palette));
    }

    lines.push(Line::from(""));
    lines.push(overlay_section_heading_line("Confirmation", palette));
    lines.push(Line::from(vec![
        metadata_span("Press ", palette),
        key_hint_span("y or enter", palette),
        metadata_span(" to apply, ", palette),
        key_hint_span("n or Esc", palette),
        metadata_span(" to cancel.", palette),
    ]));
    lines
}

fn confirm_todo_lines(todo: &TodoActionOverlay, palette: &DashTheme) -> Vec<Line<'static>> {
    let row = &todo.row;
    let mut lines = vec![
        overlay_section_heading_line("Operation", palette),
        Line::from(vec![
            badge_span(
                "Apply todo change",
                BadgeTone::Status(SeverityKind::Warning),
                palette,
            ),
            value_span(" to disk?", palette.body_text()),
        ]),
        Line::from(""),
        overlay_section_heading_line("Target", palette),
        path_line("Path: ", row.file_path.display().to_string(), palette),
        label_value_line(
            "Target: ",
            row.canonical_id
                .as_deref()
                .map(|id| format!("@{id}"))
                .unwrap_or_else(|| row.title.clone()),
            palette.dashboard_accent(),
            palette,
        ),
        label_value_line("Title: ", row.title.clone(), palette.body_text(), palette),
        Line::from(""),
        overlay_section_heading_line("Planned changes:", palette),
    ];

    if todo.plan.changes.is_empty() {
        lines.push(Line::from(value_span("  <none>", palette.muted_text())));
    } else {
        lines.extend(todo.plan.changes.iter().map(|change| {
            let before = change.before.as_deref().unwrap_or("-");
            let after = change.after.as_deref().unwrap_or("-");
            Line::from(vec![
                metadata_span("  ", palette),
                label_span(format!("{}: ", change.field), palette),
                value_span(before.to_owned(), palette.body_text()),
                metadata_span(" -> ", palette),
                value_span(after.to_owned(), palette.status(SeverityKind::Info)),
            ])
        }));
    }

    if !todo.plan.warnings.is_empty() {
        lines.push(Line::from(""));
        lines.push(overlay_section_heading_line("Warnings:", palette));
        lines.extend(todo.plan.warnings.iter().map(|warning| {
            Line::from(vec![
                metadata_span("  ", palette),
                badge_span(
                    "Warning: ",
                    BadgeTone::Status(SeverityKind::Warning),
                    palette,
                ),
                value_span(warning.clone(), palette.status(SeverityKind::Warning)),
            ])
        }));
    }

    lines.push(Line::from(""));
    lines.push(overlay_section_heading_line("Confirmation", palette));
    lines.push(Line::from(vec![
        metadata_span("Press ", palette),
        key_hint_span("y or enter", palette),
        metadata_span(" to apply, ", palette),
        key_hint_span("n or Esc", palette),
        metadata_span(" to cancel.", palette),
    ]));
    lines
}

fn fix_preview_row_lines(
    index: usize,
    row: &FixPreviewRow,
    palette: &DashTheme,
) -> Vec<Line<'static>> {
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
            metadata_span("  ", palette),
            fix_state_span(if row.is_safe { "safe" } else { "unsafe" }, palette),
            if row.is_preferred {
                metadata_span(" ", palette)
            } else {
                metadata_span("", palette)
            },
            if row.is_preferred {
                fix_state_span("preferred", palette)
            } else {
                metadata_span("", palette)
            },
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
        overlay_section_heading_line(replacement_title, palette),
    ];
    lines.extend(replacement_preview_lines(&row.replacement_preview, palette));
    lines
}

fn fix_state_badge_spans(row: &FixPreviewRow, palette: &DashTheme) -> Vec<Span<'static>> {
    let mut spans = vec![
        label_span("State: ", palette),
        fix_state_span(if row.is_safe { "safe" } else { "unsafe" }, palette),
    ];
    if row.is_preferred {
        spans.push(metadata_span(" ", palette));
        spans.push(fix_state_span("preferred", palette));
    }
    spans
}

fn fix_unavailable_spans(reason: String, palette: &DashTheme) -> Vec<Span<'static>> {
    vec![
        fix_state_span("unavailable", palette),
        metadata_span(" ", palette),
        value_span(reason, palette.status(SeverityKind::Warning)),
    ]
}

fn fix_state_span(state: &'static str, palette: &DashTheme) -> Span<'static> {
    let style = match state {
        "safe" | "preferred" => palette.status(SeverityKind::Info),
        "unsafe" | "unavailable" => palette.status(SeverityKind::Warning),
        _ => palette.body_text(),
    };
    value_span(state, style)
}

fn replacement_preview_lines(replacement_preview: &str, palette: &DashTheme) -> Vec<Line<'static>> {
    if replacement_preview.is_empty() {
        return vec![Line::from(value_span("  <empty>", palette.muted_text()))];
    }

    replacement_preview
        .lines()
        .map(|line| Line::from(value_span(format!("  {line}"), palette.body_text())))
        .collect()
}

fn status_event_lines(events: &[StatusEvent], palette: &DashTheme) -> Vec<Line<'static>> {
    if events.is_empty() {
        return vec![overlay_instruction_line("No status events yet.", palette)];
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
                value_span(event.message.clone(), palette.body_text()),
            ])];
            if let Some(detail) = &event.detail {
                lines.extend(detail.lines().map(|line| {
                    Line::from(vec![
                        metadata_span("      ", palette),
                        value_span(
                            line.to_owned(),
                            status_detail_style(event.severity, palette),
                        ),
                    ])
                }));
            }
            lines
        })
        .collect()
}

fn status_detail_style(severity: SeverityKind, palette: &DashTheme) -> Style {
    match severity {
        SeverityKind::Error | SeverityKind::Warning => palette.status(severity),
        SeverityKind::Info | SeverityKind::Unknown => palette.muted_text(),
    }
}

fn generic_log_lines(message: &str, palette: &DashTheme) -> Vec<Line<'static>> {
    if message.is_empty() {
        return vec![Line::from(value_span("", palette.body_text()))];
    }

    message
        .lines()
        .map(|line| Line::from(value_span(line.to_owned(), palette.body_text())))
        .collect()
}

fn capture_lines(draft: &CaptureDraft, palette: &DashTheme) -> Vec<Line<'static>> {
    let mut lines = draft
        .editable_fields()
        .iter()
        .map(|field| {
            let value = draft.field_value(*field);
            let value = if value.is_empty() { "-" } else { value };
            let required = if matches!(*field, CaptureField::Title | CaptureField::Body)
                && draft.requires_variable(field.variable_name().unwrap_or_default())
            {
                " *"
            } else {
                ""
            };
            overlay_form_row_line(
                *field == draft.active,
                field.label(),
                !required.is_empty(),
                value.to_owned(),
                palette,
            )
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
    lines.push(overlay_instruction_line(
        "Tab moves fields. Enter creates. Destination may be blank to use template dest::.",
        palette,
    ));
    lines.push(overlay_instruction_line(
        "Esc cancels. Ctrl-u clears the active field.",
        palette,
    ));
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
        lines.push(overlay_form_row_line_from_spans(
            index == picker.selected,
            "Template",
            false,
            capture_template_label_spans(row, palette),
            palette,
        ));

        let destination = row.destination.as_deref().unwrap_or("-");
        let variables = if row.variables.is_empty() {
            "-".to_owned()
        } else {
            row.variables.join(", ")
        };
        lines.push(Line::from(vec![
            label_span("    dest: ", palette),
            metadata_span(destination.to_owned(), palette),
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
    lines.push(overlay_instruction_line(
        "Up/down moves. Enter selects. Esc cancels.",
        palette,
    ));
    lines
}

fn capture_template_label_spans(
    row: &crate::model::CaptureTemplateRow,
    palette: &DashTheme,
) -> Vec<Span<'static>> {
    match (&row.id, &row.title) {
        (Some(id), Some(title)) => vec![
            id_span(id.clone(), palette),
            metadata_span(" - ", palette),
            value_span(title.clone(), palette.body_text()),
        ],
        (Some(id), None) => vec![id_span(id.clone(), palette)],
        (None, Some(title)) => vec![value_span(title.clone(), palette.body_text())],
        (None, None) => vec![metadata_span(row.label(), palette)],
    }
}

fn diagnostic_filter_lines(
    draft: &DiagnosticFilterDraft,
    palette: &DashTheme,
) -> Vec<Line<'static>> {
    let mut lines = DiagnosticFilterField::ALL
        .iter()
        .map(|field| {
            let value = draft.field_value(*field);
            let value = if value.is_empty() { "-" } else { value };
            overlay_form_row_line(
                *field == draft.active,
                field.label(),
                false,
                value.to_owned(),
                palette,
            )
        })
        .collect::<Vec<_>>();
    lines.push(Line::from(""));
    lines.push(overlay_instruction_line(
        "Enter applies. Tab switches fields. Ctrl-u clears the active field.",
        palette,
    ));
    lines.push(overlay_instruction_line(
        "Esc cancels. Use e for severity and a to clear all filters.",
        palette,
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
        let value = draft.field_value(field);
        overlay_form_row_line(
            field == draft.active,
            field.label(),
            false,
            value.to_owned(),
            palette,
        )
    }));

    if draft.field_options.len() > 1 {
        let mut spans = vec![label_span("Available fields: ", palette)];
        for (index, field) in draft.field_options.iter().enumerate() {
            if index > 0 {
                spans.push(metadata_span(", ", palette));
            }
            spans.push(metadata_span(todo_date_field_label(*field), palette));
        }
        lines.push(Line::from(spans));
    }

    if let Some(error) = &draft.error {
        lines.push(Line::from(""));
        lines.push(overlay_error_line(error.clone(), palette));
    }

    lines.push(Line::from(""));
    lines.push(overlay_instruction_line(
        "Date accepts YYYY-MM-DD, +1d, or +1w.",
        palette,
    ));
    lines.push(overlay_instruction_line(
        "Enter applies. Esc cancels. Tab switches fields. Left/right changes field.",
        palette,
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
        let selected = index == overlay.selected_index;
        let prefix = if selected { "> " } else { "  " };
        let mut spans = vec![
            value_span(prefix.to_owned(), palette.body_text()),
            key_hint_span(format!("{}. ", index + 1), palette),
            label_span(format!("{}: ", option.kind.label()), palette),
        ];
        if let Some(value) = &option.value {
            spans.push(yank_value_span(option.kind, value.clone(), palette));
        } else {
            spans.push(value_span(
                option
                    .unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "unavailable".to_owned()),
                palette.status(SeverityKind::Warning),
            ));
        }
        overlay_selected_line(spans, selected, palette)
    }));

    lines.push(Line::from(""));
    lines.push(overlay_instruction_line(
        "Enter/y copies selected. 1-3 copies directly. Tab moves. Esc cancels.",
        palette,
    ));
    lines.push(overlay_instruction_line(
        "If clipboard transport is unavailable, the value stays visible in the log.",
        palette,
    ));
    lines
}

fn yank_value_span(kind: YankValueKind, value: String, palette: &DashTheme) -> Span<'static> {
    match kind {
        YankValueKind::RowId if value.starts_with('@') => id_span(value, palette),
        YankValueKind::SourceLink => path_span(value, palette),
        YankValueKind::DiagnosticMessage => value_span(value, palette.body_text()),
        YankValueKind::RowId => value_span(value, palette.emphasis()),
    }
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
        CaptureTemplateRow, CustomPanel, DashboardDefinition, DashboardPanelDefinition,
        DashboardPanelQuerySource, DashboardSnapshot, DiagnosticRow, GraphLinkRow, GraphLoadState,
        GraphNeighborhood, GraphSection, GraphZettelRow, IndexPanel, IndexStatusRow, PanelRow,
        PendingOperationKind, QueryBadge, QueryPanel, QueryRow, SearchPanel, SelectedDashboard,
        ZettelRow,
    };
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::time::Duration;
    use zorg_core::SourceSpan;
    use zorg_refactor::{
        RefactorMode, RefactorPlan, TodoActionDate, TodoActionKind, TodoActionPlan, TodoChange,
        TodoDateField,
    };

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
        assert!(rendered.contains("Database: /tmp/zorg.sqlite3"));
    }

    #[test]
    fn render_ready_status_omits_paths_and_keeps_core_cockpit_fields() {
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
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());
        let status_line = rendered
            .lines()
            .nth(1)
            .expect("status content line should render");

        assert!(rendered.contains("Zorg Dash"));
        assert!(status_line.contains("index current"), "{status_line}");
        assert!(status_line.contains("diagnostics 0"), "{status_line}");
        assert!(status_line.contains("freshness current"), "{status_line}");
        assert!(status_line.contains("marked 0"), "{status_line}");
        assert!(status_line.contains("panel index"), "{status_line}");
        assert!(!status_line.contains("/tmp/corpus"), "{status_line}");
        assert!(!status_line.contains("/tmp/zorg.sqlite3"), "{status_line}");
        assert!(!status_line.contains("root "), "{status_line}");
        assert!(!status_line.contains("db "), "{status_line}");
    }

    #[test]
    fn render_degraded_guidance_keeps_root_and_database_visible() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            None,
            DashboardSnapshot::Degraded {
                message: "missing database".to_owned(),
            },
        );
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("index degraded"));
        assert!(rendered.contains("diagnostics unknown"));
        assert!(rendered.contains("Read-only index unavailable"));
        assert!(rendered.contains("Root: /tmp/corpus"));
        assert!(rendered.contains("Database: /tmp/zorg.sqlite3"));
        assert!(rendered.contains("zorg db reindex"));
    }

    #[test]
    fn render_narrow_status_retains_critical_fields_and_pending_activity() {
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
        let backend = TestBackend::new(64, 18);
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
        let status_line = rendered
            .lines()
            .nth(1)
            .expect("status content line should render");

        assert!(rendered.contains("Zorg Dash"));
        assert!(status_line.contains("index current"), "{status_line}");
        assert!(status_line.contains("diagnostics 0"), "{status_line}");
        assert!(status_line.contains("panel today"), "{status_line}");
        assert!(status_line.contains("pending todo apply"), "{status_line}");
        assert!(!status_line.contains("rows T/I/Q/S/D/X"), "{status_line}");
        assert!(!status_line.contains("/r"), "{status_line}");
        assert!(!status_line.contains("/d"), "{status_line}");
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
    fn render_inspector_styles_sections_labels_paths_and_links() {
        let theme = DashTheme::new(ColorMode::Enabled);
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
        let buffer = terminal.backend().buffer();

        assert_text_has_semantic_style(
            buffer,
            "Graph context",
            "inspector graph heading",
            theme.active_title(),
        );
        assert_text_has_semantic_style(
            buffer,
            "Outgoing links",
            "inspector link section heading",
            DomainTone::Link.style(theme),
        );
        assert_text_has_semantic_style(buffer, "Path:", "inspector path label", theme.muted_text());
        assert_text_has_semantic_style(
            buffer,
            "notes/task.z",
            "inspector path value",
            theme.path(),
        );
        assert_text_has_semantic_style(
            buffer,
            "@task",
            "inspector id value",
            theme.dashboard_accent(),
        );
        assert_text_has_semantic_style(
            buffer,
            "->",
            "inspector graph link marker",
            DomainTone::Link.style(theme),
        );
    }

    #[test]
    fn render_inspector_styles_unavailable_lines_as_errors() {
        let theme = DashTheme::new(ColorMode::Enabled);
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
        frame.set_graph_context(selected.row_id(), GraphLoadState::Unavailable);

        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let buffer = terminal.backend().buffer();

        assert_text_has_semantic_style(
            buffer,
            "Unavailable",
            "inspector unavailable state",
            theme.severity(SeverityKind::Error),
        );
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
    fn render_help_overlay_groups_documented_keys_at_normal_and_narrow_widths() {
        let theme = DashTheme::new(ColorMode::Enabled);
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

        for (width, height) in [(120, 32), (56, 22)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("terminal");
            terminal
                .draw(|area| {
                    render_dashboard_with_state(
                        area,
                        &frame,
                        DashboardRenderState::for_frame(&frame),
                        &DashboardOverlay::Help,
                        None,
                        &[],
                    )
                })
                .expect("draw");
            let rendered = buffer_to_string(terminal.backend().buffer());

            assert!(rendered.contains("Help"), "{rendered}");
            assert!(rendered.contains("Keys"), "{rendered}");
            assert!(rendered.contains("q/Esc"), "{rendered}");
            assert!(rendered.contains("tab/backtab"), "{rendered}");
            assert!(rendered.contains("up/down/j/k"), "{rendered}");
            assert!(rendered.contains("c"), "{rendered}");
            assert!(rendered.contains("t"), "{rendered}");
            assert!(rendered.contains("d"), "{rendered}");
            assert!(rendered.contains("p"), "{rendered}");
            assert!(rendered.contains("s"), "{rendered}");
            assert!(rendered.contains("f"), "{rendered}");
            assert!(rendered.contains("space"), "{rendered}");
            assert!(rendered.contains("e"), "{rendered}");
            assert!(rendered.contains(":"), "{rendered}");
            assert!(rendered.contains("a"), "{rendered}");
            assert!(rendered.contains("F"), "{rendered}");
            assert!(rendered.contains("/"), "{rendered}");
            assert!(rendered.contains("F1"), "{rendered}");
            assert!(rendered.contains("search edit"), "{rendered}");
            assert!(rendered.contains("enter"), "{rendered}");
            assert!(rendered.contains("o"), "{rendered}");
            assert!(rendered.contains("y"), "{rendered}");
            assert!(rendered.contains("r"), "{rendered}");
            assert!(rendered.contains("R"), "{rendered}");
            assert_text_has_semantic_style(
                terminal.backend().buffer(),
                "q/Esc",
                "help close key",
                theme.key_hint(),
            );
            assert_text_has_semantic_style(
                terminal.backend().buffer(),
                "quit or close overlay",
                "help close description",
                theme.body_text(),
            );
        }
    }

    #[test]
    fn render_swog_help_overlay_styles_reference_tokens() {
        let theme = DashTheme::new(ColorMode::Enabled);
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
        let backend = TestBackend::new(120, 32);
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
        let buffer = terminal.backend().buffer();

        assert_text_has_semantic_style(buffer, "Tags:", "swog source label", theme.muted_text());
        assert_text_has_semantic_style(buffer, "#z/todo", "swog todo tag", theme.todo_accent());
        assert_text_has_semantic_style(
            buffer,
            "#project/work",
            "swog graph tag",
            DomainTone::Link.style(theme),
        );
        assert_text_has_semantic_style(
            buffer,
            "links:#project/reference",
            "swog link query",
            DomainTone::Link.style(theme),
        );
        assert_text_has_semantic_style(buffer, "file:notes.z", "swog file query", theme.path());
        assert_text_has_semantic_style(
            buffer,
            "@queries/foo",
            "swog query id",
            theme.dashboard_accent(),
        );
        assert_text_has_semantic_style(buffer, "OR", "swog boolean operator", theme.emphasis());
    }

    #[test]
    fn render_capture_overlay_lists_form_fields() {
        let theme = DashTheme::new(ColorMode::Enabled);
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
        assert!(
            cell_matches_style(
                first_cell_for_text(terminal.backend().buffer(), "> Template"),
                selected_style(theme.body_text(), &theme),
            ),
            "active capture marker should use selection style"
        );
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "Template:",
            "active capture label",
            selected_style(theme.muted_text(), &theme),
        );
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "@tmpl/todo",
            "active capture value",
            selected_style(theme.body_text(), &theme),
        );
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "Title:",
            "inactive capture label",
            theme.muted_text(),
        );
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
        let theme = DashTheme::new(ColorMode::Enabled);
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
        assert!(
            cell_matches_style(
                first_cell_for_text(terminal.backend().buffer(), "> Template"),
                selected_style(theme.body_text(), &theme),
            ),
            "selected template picker marker should use selection style"
        );
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "@system/templates/project",
            "selected template picker id",
            selected_style(theme.dashboard_accent(), &theme),
        );
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "templates.z",
            "template picker path",
            theme.path(),
        );
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "title, body",
            "template variables metadata",
            theme.muted_text(),
        );
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
    fn render_yank_overlay_styles_selection_values_and_unavailable_reason() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let overlay = DashboardOverlay::Yank(
            PanelRow::IndexStatus(IndexStatusRow::new("Discovered files", 1))
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
        let buffer = terminal.backend().buffer();

        assert!(
            cell_matches_style(
                first_cell_for_text(buffer, "> 1."),
                selected_style(theme.body_text(), &theme)
            ),
            "selected yank marker should use selection styling"
        );
        assert_text_has_semantic_style(
            buffer,
            "source link",
            "yank kind label",
            theme.muted_text(),
        );
        assert_text_has_semantic_style(
            buffer,
            "selected row has no source location",
            "unavailable yank reason",
            theme.status(SeverityKind::Warning),
        );
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
    fn render_nav_rows_use_selection_and_muted_styles() {
        let theme = DashTheme::new(ColorMode::Enabled);
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
        let backend = TestBackend::new(120, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let buffer = terminal.backend().buffer();

        assert!(
            cell_matches_style(
                first_cell_for_text(buffer, "> Index"),
                selected_style(theme.body_text(), &theme)
            ),
            "active nav marker should use the selected row style"
        );
        assert_text_has_semantic_style(
            buffer,
            "Index",
            "active nav label selection",
            selected_style(theme.body_text(), &theme),
        );
        assert_text_has_semantic_style(buffer, "Today", "inactive nav label", theme.muted_text());
    }

    #[test]
    fn render_custom_nav_row_uses_selection_style() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let source_span = SourceSpan::bytes(0, 10);
        let panel_definition = DashboardPanelDefinition {
            key: "open".to_owned(),
            title: "Open Tasks".to_owned(),
            query_source: DashboardPanelQuerySource::InlineSwog {
                query: "#z/todo".to_owned(),
                output_kind: zorg_query::QueryResultKind::List,
                source_span,
            },
            source_span,
        };
        let definition = DashboardDefinition {
            id: "work".to_owned(),
            title: "Work".to_owned(),
            source_path: PathBuf::from("dashboards/work.z"),
            source_span,
            panels: vec![panel_definition.clone()],
        };
        let mut custom_panels = BTreeMap::new();
        custom_panels.insert(
            panel_definition.key.clone(),
            CustomPanel {
                definition: panel_definition,
                rows: Vec::new(),
                error: None,
            },
        );
        let frame = DashboardFrame::new_with_dashboard(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            Some("open".to_owned()),
            Some("work".to_owned()),
            None,
            DashboardSnapshot::Ready {
                index: Box::new(IndexPanel {
                    schema_version: 2,
                    rows: vec![IndexStatusRow::new("Discovered files", 1)],
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
                selected_dashboard: Some(SelectedDashboard {
                    requested_id: "work".to_owned(),
                    definition: Some(definition),
                    diagnostics: Vec::new(),
                }),
                custom_panels,
            },
        );
        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let buffer = terminal.backend().buffer();

        assert_text_has_semantic_style(
            buffer,
            "Open Tasks",
            "custom active nav selection",
            selected_style(theme.body_text(), &theme),
        );
    }

    #[test]
    fn render_footer_key_latest_and_empty_state_use_semantic_styles() {
        let theme = DashTheme::new(ColorMode::Enabled);
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
        let event = StatusEvent::new(1, SeverityKind::Error, "refresh failed", None);
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
        let buffer = terminal.backend().buffer();

        assert!(
            cell_matches_style(first_cell_for_text(buffer, "q quit"), theme.key_hint()),
            "footer key token should use key hint style"
        );
        assert_text_has_semantic_style(
            buffer,
            "refresh failed",
            "latest error status",
            theme.status(SeverityKind::Error),
        );

        let backend = TestBackend::new(120, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "idle",
            "latest empty state",
            theme.muted_text(),
        );
    }

    #[test]
    fn render_narrow_footer_keeps_core_keys_and_latest_visible() {
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
        let event = StatusEvent::new(
            1,
            SeverityKind::Info,
            "refresh completed after indexing",
            None,
        );
        let backend = TestBackend::new(64, 18);
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

        assert!(rendered.contains("Keys"));
        assert!(rendered.contains("Latest"));
        assert!(rendered.contains("q quit"));
        assert!(rendered.contains("? help"));
        assert!(rendered.contains("L log"));
        assert!(rendered.contains("y yank"));
        assert!(rendered.contains("refresh completed"));
    }

    #[test]
    fn render_status_shows_deterministic_pending_activity() {
        let theme = DashTheme::new(ColorMode::Enabled);
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
        let rendered = render_test_dashboard_with_activity(
            &frame,
            DashboardRenderState::for_frame(&frame),
            &DashboardOverlay::None,
            None,
            &[],
            Some(&activity),
            ColorMode::Enabled,
            140,
            18,
        );

        assert!(rendered.text.contains("pending"));
        assert!(rendered.text.contains("- todo apply"));
        assert!(rendered.text.contains("1.250s"));
        assert_text_has_semantic_style(
            &rendered.buffer,
            "todo apply",
            "pending activity value",
            theme.status(SeverityKind::Info),
        );
        assert_text_has_semantic_style(
            &rendered.buffer,
            "pending",
            "pending activity label",
            theme.muted_text(),
        );
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
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "#002 error",
            "event severity badge",
            DashTheme::new(ColorMode::Enabled).status(SeverityKind::Error),
        );
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "open failed: $EDITOR is not set",
            "event error detail",
            DashTheme::new(ColorMode::Enabled).status(SeverityKind::Error),
        );
    }

    #[test]
    fn render_neutral_overlay_uses_elevated_surface_and_subtle_border() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let backend = TestBackend::new(100, 28);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::Log {
                        title: "Neutral Overlay".to_owned(),
                        message: "neutral body".to_owned(),
                    },
                    None,
                    &[],
                )
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let root = Rect::new(0, 0, 100, 28);
        let overlay_area = centered_rect(66, 44, root);

        assert_text_has_semantic_style(
            buffer,
            "Neutral Overlay",
            "neutral overlay title",
            theme.title(),
        );
        assert!(
            cell_matches_style(
                &buffer[(overlay_area.x, overlay_area.y)],
                theme.subtle_border()
            ),
            "neutral overlay border should use the subtle/elevated border"
        );
        assert!(
            cell_matches_style(
                first_cell_for_text(buffer, "neutral body"),
                theme.elevated_overlay_surface()
            ),
            "neutral overlay body should use elevated surface styling"
        );
    }

    #[test]
    fn render_confirmation_overlays_use_warning_or_error_borders() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::ConfirmReindex,
                    None,
                    &[],
                )
            })
            .expect("draw");
        let reindex_area = centered_rect(66, 44, Rect::new(0, 0, 120, 30));
        assert!(
            cell_matches_style(
                &terminal.backend().buffer()[(reindex_area.x, reindex_area.y)],
                theme.warning_border()
            ),
            "reindex confirmation should use a warning border"
        );

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::ConfirmFixApply(sample_fix_preview()),
                    None,
                    &[],
                )
            })
            .expect("draw");
        let fix_area = centered_rect(78, 70, Rect::new(0, 0, 120, 30));
        assert!(
            cell_matches_style(
                &terminal.backend().buffer()[(fix_area.x, fix_area.y)],
                theme.error_border()
            ),
            "write confirmation should use an error/destructive border"
        );
    }

    #[test]
    fn render_no_color_overlay_has_no_foreground_or_background_colors() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state_and_color(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::ConfirmReindex,
                    None,
                    &[],
                    ColorMode::Disabled,
                )
            })
            .expect("draw");

        assert_buffer_has_no_colors(
            terminal.backend().buffer(),
            "disabled overlay render should reset every cell",
        );
    }

    #[test]
    fn render_log_and_confirm_reindex_overlays_keep_expected_text() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );

        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::Log {
                        title: "Custom Log".to_owned(),
                        message: "first line\nsecond line".to_owned(),
                    },
                    None,
                    &[],
                )
            })
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());
        assert!(rendered.contains("Custom Log"));
        assert!(rendered.contains("first line"));
        assert!(rendered.contains("second line"));
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "first line",
            "generic log detail",
            DashTheme::new(ColorMode::Enabled).body_text(),
        );

        let backend = TestBackend::new(100, 24);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::ConfirmReindex,
                    None,
                    &[],
                )
            })
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());
        assert!(rendered.contains("Confirm Reindex"));
        assert!(rendered.contains("Reindex will write a fresh SQLite snapshot for this corpus."));
        assert!(rendered.contains("Press y or enter to continue, n or Esc to cancel."));
    }

    #[test]
    fn render_no_color_form_yank_and_log_overlays_have_no_foreground_or_background_colors() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let overlays = vec![
            (
                "Capture",
                DashboardOverlay::Capture(CaptureDraft::new("@tmpl/todo", None)),
            ),
            (
                "Yank",
                DashboardOverlay::Yank(
                    PanelRow::IndexStatus(IndexStatusRow::new("Discovered files", 1))
                        .yank_overlay(&frame.root),
                ),
            ),
            (
                "first line",
                DashboardOverlay::Log {
                    title: "Custom Log".to_owned(),
                    message: "first line\nsecond line".to_owned(),
                },
            ),
        ];

        for (expected_text, overlay) in overlays {
            let backend = TestBackend::new(100, 24);
            let mut terminal = Terminal::new(backend).expect("terminal");
            terminal
                .draw(|area| {
                    render_dashboard_with_state_and_color(
                        area,
                        &frame,
                        DashboardRenderState::for_frame(&frame),
                        &overlay,
                        None,
                        &[],
                        ColorMode::Disabled,
                    )
                })
                .expect("draw");
            let rendered = buffer_to_string(terminal.backend().buffer());

            assert!(rendered.contains("Zorg Dash"));
            assert!(
                rendered.contains(expected_text),
                "{expected_text}\n{rendered}"
            );
            assert_buffer_has_no_colors(
                terminal.backend().buffer(),
                "disabled form/yank/log overlay should reset every cell",
            );
        }
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
    fn render_fix_preview_overlay_styles_state_path_and_diagnostic_code() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let mut preview = sample_fix_preview();
        preview.previews.push(FixPreviewRow {
            rule_code: "fix.unsafe_candidate".to_owned(),
            severity: "warning".to_owned(),
            path: PathBuf::from("other.z"),
            primary_line: Some(8),
            primary_column: None,
            replacement_preview: "replacement".to_owned(),
            replacement_truncated: false,
            is_preferred: false,
            is_safe: false,
            explanation: "Unsafe candidate".to_owned(),
        });
        let backend = TestBackend::new(140, 32);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::FixPreview(preview),
                    None,
                    &[],
                )
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();

        assert_text_has_semantic_style(
            buffer,
            "safe",
            "safe fix badge",
            theme.status(SeverityKind::Info),
        );
        assert_text_has_semantic_style(
            buffer,
            "preferred",
            "preferred fix badge",
            theme.status(SeverityKind::Info),
        );
        assert_text_has_semantic_style(
            buffer,
            "unsafe",
            "unsafe fix badge",
            theme.status(SeverityKind::Warning),
        );
        assert_text_has_semantic_style(buffer, "links.z:5:5", "fix preview path", theme.path());
        assert_text_has_semantic_style(
            buffer,
            "reference.unresolved_absolute",
            "diagnostic code",
            theme.dashboard_accent(),
        );

        let unavailable = DashboardOverlay::FixPreview(FixPreviewOverlay {
            diagnostic: crate::model::DiagnosticPreviewContext {
                severity: "warning".to_owned(),
                code: "reference.unavailable".to_owned(),
                message: "no rewrite candidate".to_owned(),
                path: "missing.z".to_owned(),
                position: "1:1".to_owned(),
            },
            previews: Vec::new(),
            unavailable_reason: Some("No safe rewrite is available.".to_owned()),
            selector: Default::default(),
            marked_summary: None,
        });
        let backend = TestBackend::new(140, 32);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &unavailable,
                    None,
                    &[],
                )
            })
            .expect("draw");
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "unavailable",
            "unavailable fix badge",
            theme.status(SeverityKind::Warning),
        );
    }

    #[test]
    fn render_fix_apply_confirmation_shows_selected_fix() {
        let theme = DashTheme::new(ColorMode::Enabled);
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
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "Apply fix",
            "confirm fix operation",
            theme.status(SeverityKind::Warning),
        );
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "y or enter",
            "confirm fix key instruction",
            theme.key_hint(),
        );
    }

    #[test]
    fn render_todo_confirmation_styles_changes_and_warnings() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let overlay = DashboardOverlay::ConfirmTodoApply(sample_todo_overlay());
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

        assert!(rendered.contains("Apply todo change to disk?"));
        assert!(rendered.contains("do: - -> 2026-05-05"));
        assert!(rendered.contains("Warning: Existing due date remains."));
        assert!(rendered.contains("Press y or enter to apply"));
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "2026-05-05",
            "todo after value",
            theme.status(SeverityKind::Info),
        );
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "Warning:",
            "todo warning label",
            theme.status(SeverityKind::Warning),
        );
    }

    #[test]
    fn render_no_color_confirmation_overlays_remain_text_readable() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        for overlay in [
            DashboardOverlay::ConfirmReindex,
            DashboardOverlay::ConfirmFixApply(sample_fix_preview()),
            DashboardOverlay::ConfirmTodoApply(sample_todo_overlay()),
        ] {
            let backend = TestBackend::new(120, 30);
            let mut terminal = Terminal::new(backend).expect("terminal");
            terminal
                .draw(|area| {
                    render_dashboard_with_state_and_color(
                        area,
                        &frame,
                        DashboardRenderState::for_frame(&frame),
                        &overlay,
                        None,
                        &[],
                        ColorMode::Disabled,
                    )
                })
                .expect("draw");
            let rendered = buffer_to_string(terminal.backend().buffer());

            assert!(rendered.contains("Press y or enter"));
            assert!(rendered.contains("n or Esc"));
            assert_buffer_has_no_colors(
                terminal.backend().buffer(),
                "disabled confirmation overlay should reset every cell",
            );
        }
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
            (DashboardOverlay::ConfirmReindex, "Confirm Reindex"),
            (
                DashboardOverlay::TodoPrompt(TodoPromptDraft::schedule(todo_row)),
                "Schedule Todo",
            ),
            (
                DashboardOverlay::ConfirmTodoApply(sample_todo_overlay()),
                "Confirm Todo Apply",
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
            (
                DashboardOverlay::Log {
                    title: "Generic Log".to_owned(),
                    message: "generic detail".to_owned(),
                },
                "Generic Log",
            ),
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
        let theme = DashTheme::new(ColorMode::Enabled);
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
        assert!(
            cell_matches_style(
                first_cell_for_text(terminal.backend().buffer(), "> Path"),
                selected_style(theme.body_text(), &theme),
            ),
            "active diagnostic filter row should use selection style"
        );
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "Path:",
            "active diagnostic filter label",
            selected_style(theme.muted_text(), &theme),
        );
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "Code:",
            "inactive diagnostic filter label",
            theme.muted_text(),
        );
    }

    #[test]
    fn render_todo_prompt_styles_error_and_available_fields() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let mut draft = TodoPromptDraft::postpone(
            zettel(3, "todo"),
            vec![TodoDateField::Due, TodoDateField::Do],
        );
        draft.error = Some("invalid date".to_owned());
        let backend = TestBackend::new(88, 28);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::for_frame(&frame),
                    &DashboardOverlay::TodoPrompt(draft),
                    None,
                    &[],
                )
            })
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Postpone Todo"), "{rendered}");
        assert!(rendered.contains("Available fields: due, do"), "{rendered}");
        assert!(rendered.contains("Error: invalid date"), "{rendered}");
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "Available fields:",
            "todo available fields label",
            theme.muted_text(),
        );
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "Error:",
            "todo prompt error badge",
            theme.status(SeverityKind::Error),
        );
        assert_text_has_semantic_style(
            terminal.backend().buffer(),
            "invalid date",
            "todo prompt error value",
            theme.status(SeverityKind::Error),
        );
    }

    #[test]
    fn render_narrow_form_overlays_keep_core_instructions() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(Vec::new(), Vec::new(), Vec::new()),
        );
        let picker = CaptureTemplatePicker::new(vec![CaptureTemplateRow {
            selector: Some("@tmpl/todo".to_owned()),
            id: Some("@tmpl/todo".to_owned()),
            title: Some("Todo".to_owned()),
            destination: Some("inbox.z".to_owned()),
            path: Some(PathBuf::from("templates.z")),
            variables: vec!["title".to_owned()],
        }]);
        let cases = vec![
            (
                DashboardOverlay::Capture(CaptureDraft::new("@tmpl/todo", None)),
                "Capture",
                "Enter creates",
            ),
            (
                DashboardOverlay::CapturePicker(picker),
                "Capture Templates",
                "Enter selects",
            ),
            (
                DashboardOverlay::DiagnosticFilter(DiagnosticFilterDraft {
                    code: "reference".to_owned(),
                    path: "notes".to_owned(),
                    active: DiagnosticFilterField::Code,
                }),
                "Diagnostic Filters",
                "Enter applies",
            ),
            (
                DashboardOverlay::TodoPrompt(TodoPromptDraft::schedule(zettel(4, "later"))),
                "Schedule Todo",
                "Enter applies",
            ),
        ];

        for (overlay, title, instruction) in cases {
            let backend = TestBackend::new(56, 22);
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

            assert!(rendered.contains(title), "{title}\n{rendered}");
            assert!(rendered.contains(instruction), "{title}\n{rendered}");
        }
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
    fn render_selected_main_row_applies_selection_to_prefix_and_content() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(
                Vec::new(),
                vec![diagnostic(1, "error", "err.code")],
                Vec::new(),
            ),
        );
        let backend = TestBackend::new(100, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let buffer = terminal.backend().buffer();

        assert!(
            cell_matches_style(
                first_cell_for_text(buffer, "> error"),
                selected_style(theme.body_text(), &theme),
            ),
            "selected row prefix should receive selected row style"
        );
        assert_text_has_semantic_style(
            buffer,
            "err.code",
            "selected diagnostic content",
            selected_style(theme.severity(SeverityKind::Error), &theme),
        );
    }

    #[test]
    fn render_marked_main_row_applies_marked_style_to_prefix_and_content() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let mut frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(
                Vec::new(),
                vec![
                    diagnostic(1, "error", "err.code"),
                    diagnostic(2, "warning", "warn.code"),
                ],
                Vec::new(),
            ),
        );
        let diagnostic = match frame.active_rows().get(1) {
            Some(PanelRow::Diagnostic(row)) => row.clone(),
            _ => panic!("diagnostic row"),
        };
        frame.toggle_diagnostic_mark(&diagnostic);

        let backend = TestBackend::new(100, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::new(0, 0, 2),
                    &DashboardOverlay::None,
                    None,
                    &[],
                )
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let marked_warning = theme
            .severity(SeverityKind::Warning)
            .patch(theme.marked_row());
        let marked_prefix = theme.body_text().patch(theme.marked_row());

        assert!(
            cell_matches_style(first_cell_for_text(buffer, " *warning"), marked_prefix),
            "marked row prefix should receive marked row style"
        );
        assert_text_has_semantic_style(
            buffer,
            "warn.code",
            "marked diagnostic content",
            marked_warning,
        );
    }

    #[test]
    fn render_diagnostic_rows_style_message_and_metadata_separately() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let mut diagnostic_row = diagnostic(2, "warning", "warn.structured");
        diagnostic_row.message = "MESSAGE-BODY-NOT-SEVERITY".to_owned();
        diagnostic_row.relative_path = Some(PathBuf::from("notes/structured.z"));
        diagnostic_row.start_line = Some(12);
        diagnostic_row.start_column = Some(3);
        diagnostic_row.end_line = Some(12);
        diagnostic_row.end_column = Some(8);
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(
                Vec::new(),
                vec![diagnostic(1, "error", "err.selected"), diagnostic_row],
                Vec::new(),
            ),
        );

        let backend = TestBackend::new(260, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::new(0, 0, 2),
                    &DashboardOverlay::None,
                    None,
                    &[],
                )
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();

        assert_text_has_semantic_style(
            buffer,
            "warning",
            "diagnostic severity badge",
            theme.severity(SeverityKind::Warning),
        );
        assert_text_has_semantic_style(
            buffer,
            "warn.structured",
            "diagnostic code emphasis",
            theme.severity(SeverityKind::Warning),
        );
        assert_text_has_semantic_style(
            buffer,
            "MESSAGE-BODY-NOT-SEVERITY",
            "diagnostic message body",
            theme.body_text(),
        );
        assert_text_has_semantic_style(
            buffer,
            "notes/structured.z",
            "diagnostic path metadata",
            theme.path(),
        );
        assert_text_has_semantic_style(
            buffer,
            "12:3-12:8",
            "diagnostic position metadata",
            theme.muted_text(),
        );
    }

    #[test]
    fn render_today_diagnostic_rows_keep_compact_badged_shape() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(
                vec![
                    PanelRow::Zettel(zettel(1, "task")),
                    PanelRow::Diagnostic(diagnostic(2, "error", "today.diagnostic")),
                ],
                Vec::new(),
                Vec::new(),
            ),
        );

        let backend = TestBackend::new(120, 18);
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
        let buffer = terminal.backend().buffer();
        let rendered = buffer_to_string(buffer);

        assert!(rendered.contains("@task"));
        assert!(rendered.contains("today.diagnostic"));
        assert_text_has_semantic_style(
            buffer,
            "today.diagnostic",
            "today diagnostic code",
            selected_style(theme.severity(SeverityKind::Error), &theme),
        );
    }

    #[test]
    fn render_zettel_todo_rows_style_marker_id_badges_and_metadata() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let mut rich_row = zettel(2, "rich");
        rich_row.title = "RICH-TITLE".to_owned();
        rich_row.todo_marker = Some("[N]".to_owned());
        rich_row.file_path = PathBuf::from("notes/rich.z");
        rich_row.lifecycle_date = Some("2026-05-04".to_owned());
        rich_row.badges = vec![
            QueryBadge::new("due", "#z/todo"),
            QueryBadge::new("waiting", "#z/waiting"),
        ];
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(
                vec![
                    PanelRow::Zettel(zettel(1, "selected")),
                    PanelRow::Zettel(rich_row),
                ],
                Vec::new(),
                Vec::new(),
            ),
        );

        let backend = TestBackend::new(240, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::new(0, 0, 2),
                    &DashboardOverlay::None,
                    None,
                    &[],
                )
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let rendered = buffer_to_string(buffer);

        assert!(rendered.contains("[N]"));
        assert!(rendered.contains("@rich"));
        assert!(rendered.contains("RICH-TITLE"));
        assert!(rendered.contains("[due]"));
        assert!(rendered.contains("[waiting]"));
        assert!(rendered.contains("notes/rich.z"));
        assert!(rendered.contains("date 2026-05-04"));
        assert_text_has_semantic_style(buffer, "[N]", "todo marker", theme.todo_accent());
        assert_text_has_semantic_style(
            buffer,
            "@rich",
            "canonical zettel id",
            theme.dashboard_accent(),
        );
        assert_text_has_semantic_style(buffer, "RICH-TITLE", "zettel title", theme.body_text());
        assert_text_has_semantic_style(buffer, "[due]", "query badge", theme.query_accent());
        assert_text_has_semantic_style(
            buffer,
            "notes/rich.z",
            "zettel path metadata",
            theme.path(),
        );
        assert_text_has_semantic_style(
            buffer,
            "2026-05-04",
            "lifecycle metadata",
            theme.muted_text(),
        );
    }

    #[test]
    fn render_selected_todo_row_preserves_marker_id_and_text_semantics() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let mut selected = zettel(1, "selected");
        selected.title = "SELECTED-TITLE".to_owned();
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(vec![PanelRow::Zettel(selected)], Vec::new(), Vec::new()),
        );

        let backend = TestBackend::new(140, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let buffer = terminal.backend().buffer();

        assert_text_has_semantic_style(
            buffer,
            "[ ]",
            "selected todo marker",
            selected_style(theme.todo_accent(), &theme),
        );
        assert_text_has_semantic_style(
            buffer,
            "@selected",
            "selected zettel id",
            selected_style(theme.dashboard_accent(), &theme),
        );
        assert_text_has_semantic_style(
            buffer,
            "SELECTED-TITLE",
            "selected zettel title",
            selected_style(theme.body_text(), &theme),
        );
    }

    #[test]
    fn render_valid_query_rows_style_badge_id_count_and_metadata() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let mut row = query_row("queries/open", "Open Work");
        row.row_count_preview = Some(37);
        row.source_path = Some(PathBuf::from("notes/queries.z"));
        row.source_kind = Some(zorg_query::QueryDefinitionSourceKind::Property);
        row.output_kind = Some(zorg_query::QueryResultKind::List);
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Queries,
            None,
            ready_snapshot_with_queries(vec![
                invalid_query_row("queries/selected", "Selected"),
                row,
            ]),
        );

        let backend = TestBackend::new(260, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::new(0, 0, 2),
                    &DashboardOverlay::None,
                    None,
                    &[],
                )
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let rendered = buffer_to_string(buffer);

        assert!(rendered.contains("@queries/open"));
        assert!(rendered.contains("Open Work"));
        assert!(rendered.contains("rows 37"));
        assert!(rendered.contains("notes/queries.z"));
        assert!(rendered.contains("query:: property / list"));
        assert_text_has_semantic_style(
            buffer,
            "ok",
            "valid query badge",
            theme.status(SeverityKind::Info),
        );
        assert_text_has_semantic_style(buffer, "@queries/open", "query id", theme.query_accent());
        assert_text_has_semantic_style(buffer, "Open Work", "query title", theme.body_text());
        assert_text_has_semantic_style(buffer, "37", "row count preview", theme.emphasis());
        assert_text_has_semantic_style(
            buffer,
            "notes/queries.z",
            "query source path",
            theme.path(),
        );
        assert_text_has_semantic_style(
            buffer,
            "query:: property",
            "query source kind",
            theme.muted_text(),
        );
    }

    #[test]
    fn render_invalid_query_rows_keep_error_source_and_title_readable() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let mut row = query_row("queries/bad", "Bad Query");
        row.valid = false;
        row.error = Some("query parse failed\nquery.syntax at byte 0".to_owned());
        row.source_path = Some(PathBuf::from("notes/bad-query.z"));
        row.source_kind = None;
        row.output_kind = None;
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Queries,
            None,
            ready_snapshot_with_queries(vec![query_row("queries/selected", "Selected"), row]),
        );

        let backend = TestBackend::new(260, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::new(0, 0, 2),
                    &DashboardOverlay::None,
                    None,
                    &[],
                )
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let rendered = buffer_to_string(buffer);

        assert!(rendered.contains("warning"));
        assert!(rendered.contains("@queries/bad"));
        assert!(rendered.contains("Bad Query"));
        assert!(rendered.contains("notes/bad-query.z"));
        assert!(rendered.contains("query parse failed"));
        assert_text_has_semantic_style(
            buffer,
            "warning",
            "invalid query badge",
            theme.severity(SeverityKind::Warning),
        );
        assert_text_has_semantic_style(
            buffer,
            "Bad Query",
            "invalid query title",
            theme.body_text(),
        );
        assert_text_has_semantic_style(
            buffer,
            "notes/bad-query.z",
            "invalid query source path",
            theme.path(),
        );
        assert_text_has_semantic_style(
            buffer,
            "query parse failed",
            "invalid query error",
            theme.severity(SeverityKind::Warning),
        );
    }

    #[test]
    fn render_search_panel_rows_use_zettel_renderer() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let mut row = zettel(9, "search-row");
        row.todo_marker = Some("[S]".to_owned());
        row.title = "SEARCH RESULT".to_owned();
        row.file_path = PathBuf::from("notes/search.z");
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Search,
            Some("#z/todo".to_owned()),
            ready_snapshot_with_search(SearchPanel::with_rows("#z/todo", vec![row])),
        );

        let backend = TestBackend::new(160, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let rendered = buffer_to_string(buffer);

        assert!(rendered.contains("Query: #z/todo"));
        assert!(rendered.contains("[S]"));
        assert!(rendered.contains("@search-row"));
        assert!(rendered.contains("SEARCH RESULT"));
        assert_text_has_semantic_style(
            buffer,
            "[S]",
            "search zettel todo marker",
            selected_style(theme.todo_accent(), &theme),
        );
        assert_text_has_semantic_style(
            buffer,
            "@search-row",
            "search zettel id",
            selected_style(theme.dashboard_accent(), &theme),
        );
    }

    #[test]
    fn render_custom_panel_query_errors_use_diagnostic_renderer() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let source_span = span_at(4, 7);
        let panel_definition = DashboardPanelDefinition {
            key: "broken".to_owned(),
            title: "Broken Panel".to_owned(),
            query_source: DashboardPanelQuerySource::StoredQuery {
                id: "queries/broken".to_owned(),
                query: "OR".to_owned(),
                output_kind: zorg_query::QueryResultKind::List,
                source_path: PathBuf::from("dashboards/work.z"),
                source_span,
            },
            source_span,
        };
        let definition = DashboardDefinition {
            id: "work".to_owned(),
            title: "Work".to_owned(),
            source_path: PathBuf::from("dashboards/work.z"),
            source_span,
            panels: vec![panel_definition.clone()],
        };
        let mut custom_panels = BTreeMap::new();
        custom_panels.insert(
            "broken".to_owned(),
            CustomPanel {
                definition: panel_definition,
                rows: Vec::new(),
                error: Some("query parse failed\nquery.syntax at byte 0".to_owned()),
            },
        );
        let frame = DashboardFrame::new_with_dashboard(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            Some("broken".to_owned()),
            Some("work".to_owned()),
            None,
            ready_snapshot_with_dashboard(
                Some(SelectedDashboard {
                    requested_id: "work".to_owned(),
                    definition: Some(definition),
                    diagnostics: Vec::new(),
                }),
                custom_panels,
            ),
        );

        let backend = TestBackend::new(180, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let rendered = buffer_to_string(buffer);

        assert!(rendered.contains("dashboard.panel.query_error"));
        assert!(rendered.contains("custom panel `broken` query failed"));
        assert_text_has_semantic_style(
            buffer,
            "dashboard.panel.query_error",
            "custom panel diagnostic code",
            selected_style(theme.severity(SeverityKind::Error), &theme),
        );
        assert_text_has_semantic_style(
            buffer,
            "custom panel `broken` query failed",
            "custom panel diagnostic message",
            selected_style(theme.body_text(), &theme),
        );
    }

    #[test]
    fn render_fallback_diagnostic_rows_remain_readable() {
        let mut row = diagnostic(-1, "notice", "unused");
        row.category = "fallback.category".to_owned();
        row.code = None;
        row.message = "fallback diagnostic message".to_owned();
        row.absolute_path = None;
        row.relative_path = None;
        row.start_line = None;
        row.start_column = None;
        row.end_line = None;
        row.end_column = None;
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Diagnostics,
            None,
            ready_snapshot(Vec::new(), vec![row], Vec::new()),
        );

        let backend = TestBackend::new(140, 18);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("fallback.category"));
        assert!(rendered.contains("fallback diagnostic message"));
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
    fn render_no_color_mode_resets_main_dashboard_and_overlay_cells() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(
                vec![PanelRow::Zettel(zettel(1, "task"))],
                vec![diagnostic(1, "warning", "reference.missing")],
                vec![IndexStatusRow::new("Diagnostics", 1)],
            ),
        );
        let overlay = DashboardOverlay::DiagnosticFilter(DiagnosticFilterDraft {
            code: "reference".to_owned(),
            path: "notes".to_owned(),
            active: DiagnosticFilterField::Path,
        });
        let rendered = render_test_dashboard_with_activity(
            &frame,
            DashboardRenderState::for_frame(&frame),
            &overlay,
            None,
            &[],
            None,
            ColorMode::Disabled,
            100,
            24,
        );

        assert!(rendered.text.contains("Zorg Dash"), "{}", rendered.text);
        assert!(rendered.text.contains("@task"), "{}", rendered.text);
        assert!(
            rendered.text.contains("Diagnostic Filters"),
            "{}",
            rendered.text
        );
        assert!(rendered.text.contains("> Path"), "{}", rendered.text);
        assert_buffer_has_no_colors(
            &rendered.buffer,
            "disabled overlay render should reset every cell",
        );
    }

    #[test]
    fn render_frame_block_hierarchy_uses_main_as_active_block() {
        let theme = DashTheme::new(ColorMode::Enabled);
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
        let root = Rect::new(0, 0, 140, 28);
        let areas = dashboard_areas(root);
        let backend = TestBackend::new(root.width, root.height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let rendered = buffer_to_string(buffer);

        assert!(rendered.contains("Zorg Dash"));
        assert!(rendered.contains("Panels"));
        assert!(rendered.contains("Main"));
        assert!(rendered.contains("Inspector"));
        assert!(rendered.contains("Keys"));
        assert!(rendered.contains("Latest"));

        assert!(
            cell_matches_style(&buffer[(areas.main.x, areas.main.y)], theme.active_border()),
            "main border should use the active border style"
        );
        assert!(
            cell_matches_style(&buffer[(areas.nav.x, areas.nav.y)], theme.subtle_border()),
            "nav border should stay lower-emphasis"
        );
        assert!(
            cell_matches_style(
                &buffer[(areas.inspector.x, areas.inspector.y)],
                theme.subtle_border()
            ),
            "inspector border should stay lower-emphasis"
        );
        assert!(
            cell_matches_style(
                &buffer[(areas.status.x, areas.status.y)],
                theme.chrome_border()
            ),
            "status border should use the app chrome style"
        );
        assert!(
            !cell_matches_style(
                &buffer[(areas.status.x, areas.status.y)],
                theme.active_border()
            ),
            "status border should not compete with the active main border"
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
            ("chrome_border", theme.chrome_border()),
            ("active_border", theme.active_border()),
            ("warning_border", theme.warning_border()),
            ("error_border", theme.error_border()),
            ("title", theme.title()),
            ("chrome_title", theme.chrome_title()),
            ("active_title", theme.active_title()),
            ("warning_title", theme.warning_title()),
            ("error_title", theme.error_title()),
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

    #[test]
    fn render_index_rows_style_quiet_and_attention_counts_separately() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let rows = vec![
            IndexStatusRow::new("Discovered files", 3),
            IndexStatusRow::new("Indexed files", 3),
            IndexStatusRow::new("New files", 1),
            IndexStatusRow::new("Changed files", 1),
            IndexStatusRow::new("Deleted files", 1),
            IndexStatusRow::new("Diagnostics", 2),
        ];
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            None,
            DashboardSnapshot::Ready {
                index: Box::new(IndexPanel {
                    schema_version: 2,
                    rows,
                    discovered_files: 3,
                    indexed_files: 3,
                    changed_files: 1,
                    new_files: 1,
                    deleted_files: 1,
                    diagnostic_count: 2,
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

        let backend = TestBackend::new(120, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_state(
                    area,
                    &frame,
                    DashboardRenderState::new(1, 0, 6),
                    &DashboardOverlay::None,
                    None,
                    &[],
                )
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        let rendered = buffer_to_string(buffer);

        assert!(rendered.contains("Discovered files"));
        assert!(rendered.contains("Diagnostics"));
        assert_text_has_semantic_style(
            buffer,
            "Discovered files",
            "quiet index label",
            theme.muted_text(),
        );
        assert_text_has_semantic_style(buffer, "3", "quiet index value", theme.emphasis());
        assert_text_has_semantic_style(
            buffer,
            "New files",
            "new files attention label",
            theme.index_row(&IndexStatusRow::new("New files", 1)),
        );
        assert_text_has_semantic_style(
            buffer,
            "1",
            "attention count",
            theme.index_row(&IndexStatusRow::new("Deleted files", 1)),
        );
        assert_text_has_semantic_style(
            buffer,
            "Diagnostics",
            "diagnostic attention label",
            theme.index_row(&IndexStatusRow::new("Diagnostics", 2)),
        );
    }

    #[test]
    fn render_selected_index_attention_rows_compose_selection() {
        let theme = DashTheme::new(ColorMode::Enabled);
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            None,
            ready_snapshot(
                Vec::new(),
                Vec::new(),
                vec![
                    IndexStatusRow::new("Discovered files", 3),
                    IndexStatusRow::new("Diagnostics", 2),
                ],
            ),
        );

        let backend = TestBackend::new(100, 18);
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
        let buffer = terminal.backend().buffer();

        assert!(
            cell_matches_style(
                first_cell_for_text(buffer, "> Diagnostics"),
                selected_style(theme.body_text(), &theme),
            ),
            "selected index prefix should receive selected row style"
        );
        assert_text_has_semantic_style(
            buffer,
            "Diagnostics",
            "selected diagnostic index label",
            selected_style(
                theme.index_row(&IndexStatusRow::new("Diagnostics", 2)),
                &theme,
            ),
        );
        assert_text_has_semantic_style(
            buffer,
            "2",
            "selected diagnostic index count",
            selected_style(
                theme.index_row(&IndexStatusRow::new("Diagnostics", 2)),
                &theme,
            ),
        );
    }

    #[test]
    fn render_narrow_dashboard_keeps_structural_labels_visible() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Today,
            None,
            ready_snapshot(
                vec![PanelRow::Zettel(zettel(1, "task"))],
                vec![diagnostic(1, "warning", "reference.missing")],
                vec![IndexStatusRow::new("Diagnostics", 1)],
            ),
        );
        let rendered = render_test_dashboard(&frame, 64, 28);

        for expected in [
            "Zorg Dash",
            "Panels",
            "Main",
            "Inspector",
            "Keys",
            "> Today",
            "@task",
        ] {
            assert!(
                rendered.text.contains(expected),
                "narrow dashboard should keep {expected:?} visible\n{}",
                rendered.text
            );
        }
    }

    #[test]
    fn render_narrow_builtin_and_custom_panels_keep_core_rows_visible() {
        let mut search_row = zettel(5, "search");
        search_row.title = "SEARCHROW".to_owned();
        let snapshot = DashboardSnapshot::Ready {
            index: Box::new(IndexPanel {
                schema_version: 2,
                rows: vec![IndexStatusRow::new("Diagnostics", 1)],
                discovered_files: 1,
                indexed_files: 1,
                changed_files: 0,
                new_files: 0,
                deleted_files: 0,
                diagnostic_count: 1,
                last_indexed_at_unix_ms: Some(42),
            }),
            diagnostics: vec![diagnostic(3, "warning", "narrow.diagnostic")],
            today: vec![PanelRow::Zettel(zettel(1, "today"))],
            inbox: vec![zettel(2, "inbox")],
            queries: QueryPanel {
                rows: vec![query_row("queries/narrow", "Narrow")],
            },
            search: SearchPanel::with_rows("#z/todo", vec![search_row]),
            selected_dashboard: None,
            custom_panels: BTreeMap::new(),
        };

        for (panel, expected) in [
            (Panel::Today, "@today"),
            (Panel::Inbox, "@inbox"),
            (Panel::Queries, "@queries/narrow"),
            (Panel::Search, "SEARCHROW"),
            (Panel::Diagnostics, "narrow.diagnostic"),
            (Panel::Index, "Diagnostics"),
        ] {
            let frame = DashboardFrame::new(
                PathBuf::from("/tmp/corpus"),
                PathBuf::from("/tmp/zorg.sqlite3"),
                panel,
                Some("#z/todo".to_owned()),
                snapshot.clone(),
            );
            let backend = TestBackend::new(68, 30);
            let mut terminal = Terminal::new(backend).expect("terminal");
            terminal
                .draw(|area| render_dashboard(area, &frame))
                .expect("draw");
            let rendered = buffer_to_string(terminal.backend().buffer());
            assert!(
                rendered.contains(expected),
                "{panel:?} should keep {expected} visible at narrow width\n{rendered}"
            );
        }

        let custom_definition = DashboardPanelDefinition {
            key: "work".to_owned(),
            title: "Work".to_owned(),
            query_source: DashboardPanelQuerySource::InlineSwog {
                query: "#z/todo".to_owned(),
                output_kind: zorg_query::QueryResultKind::List,
                source_span: SourceSpan::bytes(0, 7),
            },
            source_span: SourceSpan::bytes(0, 7),
        };
        let mut custom_panels = BTreeMap::new();
        custom_panels.insert(
            "work".to_owned(),
            CustomPanel {
                definition: custom_definition,
                rows: vec![zettel(7, "custom")],
                error: None,
            },
        );
        let frame = DashboardFrame::new_with_dashboard(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/zorg.sqlite3"),
            Panel::Index,
            Some("work".to_owned()),
            Some("dash".to_owned()),
            None,
            ready_snapshot_with_dashboard(None, custom_panels),
        );
        let backend = TestBackend::new(68, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| render_dashboard(area, &frame))
            .expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());

        assert!(rendered.contains("Custom: Work"));
        assert!(rendered.contains("@custom"));
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

    fn ready_snapshot_with_queries(rows: Vec<QueryRow>) -> DashboardSnapshot {
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
            queries: QueryPanel { rows },
            search: SearchPanel::empty(""),
            selected_dashboard: None,
            custom_panels: BTreeMap::new(),
        }
    }

    fn ready_snapshot_with_search(search: SearchPanel) -> DashboardSnapshot {
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
            search,
            selected_dashboard: None,
            custom_panels: BTreeMap::new(),
        }
    }

    fn ready_snapshot_with_dashboard(
        selected_dashboard: Option<SelectedDashboard>,
        custom_panels: BTreeMap<String, CustomPanel>,
    ) -> DashboardSnapshot {
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
            selected_dashboard,
            custom_panels,
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

    fn query_row(id: &str, title: &str) -> QueryRow {
        QueryRow {
            id: id.to_owned(),
            title: title.to_owned(),
            source_path: Some(PathBuf::from(format!("{id}.z"))),
            source_kind: Some(zorg_query::QueryDefinitionSourceKind::FencedSwog),
            output_kind: Some(zorg_query::QueryResultKind::List),
            definition_preview: Some("#z/todo".to_owned()),
            valid: true,
            error: None,
            row_count_preview: Some(1),
            row_count_error: None,
            start_line: Some(1),
            start_column: Some(1),
        }
    }

    fn invalid_query_row(id: &str, title: &str) -> QueryRow {
        let mut row = query_row(id, title);
        row.valid = false;
        row.error = Some("query parse failed".to_owned());
        row.row_count_preview = None;
        row
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

    fn sample_fix_preview() -> FixPreviewOverlay {
        FixPreviewOverlay {
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
        }
    }

    fn sample_todo_overlay() -> TodoActionOverlay {
        let mut plan = TodoActionPlan {
            operation: "todo.schedule".to_owned(),
            action: TodoActionKind::Schedule {
                date: TodoActionDate::parse("2026-05-05").expect("valid date"),
            },
            target_summary: "schedule".to_owned(),
            zettel_store_id: Some(2),
            warnings: vec!["Existing due date remains.".to_owned()],
            rejections: Vec::new(),
            changes: vec![TodoChange {
                field: "do".to_owned(),
                before: None,
                after: Some("2026-05-05".to_owned()),
            }],
            refactor_plan: RefactorPlan::new(
                "todo.schedule",
                RefactorMode::Write,
                "/tmp/corpus",
                Some("schedule".to_owned()),
            ),
        };
        plan.refactor_plan.warnings = plan.warnings.clone();
        TodoActionOverlay::new("Confirm Todo Apply", zettel(2, "schedule"), plan)
    }

    #[derive(Debug)]
    struct RenderedTestDashboard {
        buffer: Buffer,
        text: String,
    }

    fn render_test_dashboard(
        frame: &DashboardFrame,
        width: u16,
        height: u16,
    ) -> RenderedTestDashboard {
        render_test_dashboard_with_activity(
            frame,
            DashboardRenderState::for_frame(frame),
            &DashboardOverlay::None,
            None,
            &[],
            None,
            ColorMode::Enabled,
            width,
            height,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_test_dashboard_with_activity(
        frame: &DashboardFrame,
        render_state: DashboardRenderState,
        overlay: &DashboardOverlay,
        latest_status: Option<&StatusEvent>,
        status_events: &[StatusEvent],
        pending_activity: Option<&PendingActivity>,
        color_mode: ColorMode,
        width: u16,
        height: u16,
    ) -> RenderedTestDashboard {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|area| {
                render_dashboard_with_activity_and_color(
                    area,
                    frame,
                    render_state,
                    overlay,
                    latest_status,
                    status_events,
                    pending_activity,
                    color_mode,
                )
            })
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        let text = buffer_to_string(&buffer);
        RenderedTestDashboard { buffer, text }
    }

    fn first_cell_for_text<'a>(buffer: &'a Buffer, text: &str) -> &'a ratatui::buffer::Cell {
        for y in buffer.area.top()..buffer.area.bottom() {
            let mut line = String::new();
            for x in buffer.area.left()..buffer.area.right() {
                line.push_str(buffer[(x, y)].symbol());
            }
            if let Some(offset) = line.find(text) {
                let x_offset = line[..offset].chars().count() as u16;
                return &buffer[(buffer.area.left() + x_offset, y)];
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
                let byte_offset = start + offset;
                let x = buffer.area.left() + line[..byte_offset].chars().count() as u16;
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
