#![allow(dead_code)]

use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use zorg_core::SourceSpan;

#[cfg(test)]
use crate::model::Panel;
use crate::model::{
    DashboardFrame, DashboardPanelRowCounts, DashboardRenderState, DashboardSnapshot,
    DashboardTelemetry, DiagnosticFilters, GraphLinkRow, GraphLoadState, GraphNeighborhood,
    GraphSection, GraphZettelRow, IndexGeneration, IndexPanel, PanelRow, PanelRowId,
    PendingOperationKind, QueryBadge, QueryRow, SnapshotFreshness, ZettelRow,
};

const DASHBOARD_JSON_SCHEMA: &str = "zorg.dash.frame";
const DASHBOARD_JSON_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize)]
pub(crate) struct DashboardJsonFrame {
    schema: &'static str,
    schema_version: u32,
    root: String,
    database_path: String,
    selected_dashboard: Option<DashboardJsonSelectedDashboard>,
    active_panel: String,
    selection: DashboardJsonSelection,
    query: Option<String>,
    today_mode: &'static str,
    diagnostic_filters: DashboardJsonDiagnosticFilters,
    row_counts: DashboardJsonRowCounts,
    panels: Vec<DashboardJsonPanel>,
    active_panel_rows: Vec<DashboardJsonRow>,
    inspector: DashboardJsonInspector,
    telemetry: DashboardJsonTelemetry,
    health: DashboardJsonHealth,
    freshness: DashboardJsonFreshness,
    snapshot: DashboardJsonSnapshot,
}

impl DashboardJsonFrame {
    pub(crate) fn from_frame(
        frame: &DashboardFrame,
        render_state: DashboardRenderState,
        graph: Option<&GraphLoadState>,
    ) -> Self {
        let active_rows = frame.active_rows();
        let selected_row_id = active_rows
            .get(render_state.selected_index)
            .map(|row| json_row_id(&row.row_id()));

        Self {
            schema: DASHBOARD_JSON_SCHEMA,
            schema_version: DASHBOARD_JSON_SCHEMA_VERSION,
            root: path_json(&frame.root),
            database_path: path_json(&frame.database_path),
            selected_dashboard: selected_dashboard_json(frame),
            active_panel: frame.active_panel_id().key().to_owned(),
            selection: DashboardJsonSelection {
                selected_index: if active_rows.is_empty() {
                    None
                } else {
                    Some(render_state.selected_index.min(active_rows.len() - 1))
                },
                selected_row_id,
                scroll_offset: render_state.scroll_offset,
                visible_row_count: render_state.total_row_count,
            },
            query: frame.query.clone(),
            today_mode: frame.today_mode.label(),
            diagnostic_filters: DashboardJsonDiagnosticFilters::from(&frame.diagnostic_filters),
            row_counts: DashboardJsonRowCounts::from(frame.telemetry.row_counts),
            panels: frame
                .panels
                .iter()
                .map(|panel| DashboardJsonPanel {
                    panel: panel.key().to_owned(),
                    title: panel.label.clone(),
                    row_count: frame.rows_for_panel_id(&panel.id).len(),
                    active: panel.id == frame.active_panel_id(),
                })
                .collect(),
            active_panel_rows: active_rows
                .iter()
                .enumerate()
                .map(|(index, row)| DashboardJsonRow::from_panel_row(index, row))
                .collect(),
            inspector: DashboardJsonInspector {
                lines: frame.inspector_lines_for_selection(render_state.selected_index),
                graph: graph.map(DashboardJsonGraph::from_state),
            },
            telemetry: DashboardJsonTelemetry::from(&frame.telemetry),
            health: DashboardJsonHealth::from_frame(frame),
            freshness: DashboardJsonFreshness::from(&frame.snapshot_freshness),
            snapshot: DashboardJsonSnapshot::from(&frame.snapshot),
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonSelectedDashboard {
    requested_id: String,
    id: Option<String>,
    title: Option<String>,
    source_path: Option<String>,
    diagnostics: usize,
}

fn selected_dashboard_json(frame: &DashboardFrame) -> Option<DashboardJsonSelectedDashboard> {
    let dashboard = match &frame.snapshot {
        DashboardSnapshot::Ready {
            selected_dashboard, ..
        } => selected_dashboard.as_ref()?,
        DashboardSnapshot::Loading | DashboardSnapshot::Degraded { .. } => return None,
    };
    Some(DashboardJsonSelectedDashboard {
        requested_id: dashboard.requested_id.clone(),
        id: dashboard
            .definition
            .as_ref()
            .map(|definition| definition.id.clone()),
        title: dashboard
            .definition
            .as_ref()
            .map(|definition| definition.title.clone()),
        source_path: dashboard
            .definition
            .as_ref()
            .map(|definition| path_json(&definition.source_path)),
        diagnostics: dashboard.diagnostics.len(),
    })
}

pub(crate) fn serialize_frame(
    frame: &DashboardFrame,
    render_state: DashboardRenderState,
    graph: Option<&GraphLoadState>,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&DashboardJsonFrame::from_frame(frame, render_state, graph))
}

#[derive(Debug, Serialize)]
struct DashboardJsonSelection {
    selected_index: Option<usize>,
    selected_row_id: Option<String>,
    scroll_offset: usize,
    visible_row_count: usize,
}

#[derive(Debug, Serialize)]
struct DashboardJsonDiagnosticFilters {
    severity: &'static str,
    code: String,
    path: String,
    active: bool,
}

impl From<&DiagnosticFilters> for DashboardJsonDiagnosticFilters {
    fn from(filters: &DiagnosticFilters) -> Self {
        Self {
            severity: filters.severity.label(),
            code: filters.code.clone(),
            path: filters.path.clone(),
            active: filters.is_active(),
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonPanel {
    panel: String,
    title: String,
    row_count: usize,
    active: bool,
}

#[derive(Debug, Serialize)]
struct DashboardJsonRowCounts {
    today: usize,
    inbox: usize,
    queries: usize,
    search: usize,
    diagnostics: usize,
    index: usize,
}

impl From<DashboardPanelRowCounts> for DashboardJsonRowCounts {
    fn from(counts: DashboardPanelRowCounts) -> Self {
        Self {
            today: counts.today,
            inbox: counts.inbox,
            queries: counts.queries,
            search: counts.search,
            diagnostics: counts.diagnostics,
            index: counts.index,
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonInspector {
    lines: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    graph: Option<DashboardJsonGraph>,
}

#[derive(Debug, Serialize)]
struct DashboardJsonTelemetry {
    refresh_count: usize,
    last_initial_load_ms: Option<u64>,
    last_refresh_ms: Option<u64>,
    last_search_ms: Option<u64>,
    last_action: Option<DashboardJsonTelemetryAction>,
    row_counts: DashboardJsonRowCounts,
}

impl From<&DashboardTelemetry> for DashboardJsonTelemetry {
    fn from(telemetry: &DashboardTelemetry) -> Self {
        Self {
            refresh_count: telemetry.refresh_count,
            last_initial_load_ms: duration_ms(telemetry.last_initial_load),
            last_refresh_ms: duration_ms(telemetry.last_refresh),
            last_search_ms: duration_ms(telemetry.last_search),
            last_action: telemetry
                .last_action
                .map(DashboardJsonTelemetryAction::from),
            row_counts: DashboardJsonRowCounts::from(telemetry.row_counts),
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonTelemetryAction {
    kind: &'static str,
    elapsed_ms: u64,
}

impl From<crate::model::DashboardTelemetryAction> for DashboardJsonTelemetryAction {
    fn from(action: crate::model::DashboardTelemetryAction) -> Self {
        Self {
            kind: action_kind_json(action.kind),
            elapsed_ms: duration_ms(Some(action.elapsed)).unwrap_or(u64::MAX),
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonHealth {
    label: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<DashboardJsonIndexHealth>,
}

impl DashboardJsonHealth {
    fn from_frame(frame: &DashboardFrame) -> Self {
        Self {
            label: frame.health_label(),
            index: match &frame.snapshot {
                DashboardSnapshot::Ready { index, .. } => {
                    Some(DashboardJsonIndexHealth::from(index.as_ref()))
                }
                DashboardSnapshot::Loading | DashboardSnapshot::Degraded { .. } => None,
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonIndexHealth {
    schema_version: i64,
    discovered_files: usize,
    indexed_files: usize,
    new_files: usize,
    changed_files: usize,
    deleted_files: usize,
    diagnostic_count: usize,
    last_indexed_at_unix_ms: Option<i64>,
}

impl From<&IndexPanel> for DashboardJsonIndexHealth {
    fn from(index: &IndexPanel) -> Self {
        Self {
            schema_version: index.schema_version,
            discovered_files: index.discovered_files,
            indexed_files: index.indexed_files,
            new_files: index.new_files,
            changed_files: index.changed_files,
            deleted_files: index.deleted_files,
            diagnostic_count: index.diagnostic_count,
            last_indexed_at_unix_ms: index.last_indexed_at_unix_ms,
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonSnapshot {
    state: &'static str,
    metrics: DashboardJsonSnapshotMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl From<&DashboardSnapshot> for DashboardJsonSnapshot {
    fn from(snapshot: &DashboardSnapshot) -> Self {
        Self {
            state: match snapshot {
                DashboardSnapshot::Loading => "loading",
                DashboardSnapshot::Ready { .. } => "ready",
                DashboardSnapshot::Degraded { .. } => "degraded",
            },
            metrics: DashboardJsonSnapshotMetrics::from(snapshot),
            error: match snapshot {
                DashboardSnapshot::Degraded { message } => Some(message.clone()),
                DashboardSnapshot::Loading | DashboardSnapshot::Ready { .. } => None,
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonSnapshotMetrics {
    today_rows: usize,
    inbox_rows: usize,
    query_rows: usize,
    search_rows: usize,
    diagnostic_rows: usize,
    index_diagnostics: usize,
}

impl From<&DashboardSnapshot> for DashboardJsonSnapshotMetrics {
    fn from(snapshot: &DashboardSnapshot) -> Self {
        let metrics = snapshot.metrics();
        Self {
            today_rows: metrics.today_rows,
            inbox_rows: metrics.inbox_rows,
            query_rows: metrics.query_rows,
            search_rows: metrics.search_rows,
            diagnostic_rows: metrics.diagnostic_rows,
            index_diagnostics: metrics.index_diagnostics,
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonFreshness {
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation: Option<DashboardJsonIndexGeneration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    captured: Option<DashboardJsonIndexGeneration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<DashboardJsonIndexGeneration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_changes: Option<DashboardJsonSourceChanges>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl From<&SnapshotFreshness> for DashboardJsonFreshness {
    fn from(freshness: &SnapshotFreshness) -> Self {
        match freshness {
            SnapshotFreshness::Unknown => Self {
                state: "unknown",
                generation: None,
                captured: None,
                current: None,
                source_changes: None,
                error: None,
            },
            SnapshotFreshness::Current { generation } => Self {
                state: "current",
                generation: Some(DashboardJsonIndexGeneration::from(*generation)),
                captured: None,
                current: None,
                source_changes: None,
                error: None,
            },
            SnapshotFreshness::NewerIndexAvailable { captured, current } => Self {
                state: "newer_index_available",
                generation: None,
                captured: Some(DashboardJsonIndexGeneration::from(*captured)),
                current: Some(DashboardJsonIndexGeneration::from(*current)),
                source_changes: None,
                error: None,
            },
            SnapshotFreshness::StaleSources {
                generation,
                new_files,
                changed_files,
                deleted_files,
            } => Self {
                state: "stale_sources",
                generation: Some(DashboardJsonIndexGeneration::from(*generation)),
                captured: None,
                current: None,
                source_changes: Some(DashboardJsonSourceChanges {
                    new_files: *new_files,
                    changed_files: *changed_files,
                    deleted_files: *deleted_files,
                }),
                error: None,
            },
            SnapshotFreshness::CheckFailed { message } => Self {
                state: "check_failed",
                generation: None,
                captured: None,
                current: None,
                source_changes: None,
                error: Some(message.clone()),
            },
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonSourceChanges {
    new_files: usize,
    changed_files: usize,
    deleted_files: usize,
}

#[derive(Debug, Serialize)]
struct DashboardJsonIndexGeneration {
    schema_version: i64,
    last_indexed_at_unix_ms: Option<i64>,
    indexed_files: usize,
    diagnostic_count: usize,
}

impl From<IndexGeneration> for DashboardJsonIndexGeneration {
    fn from(generation: IndexGeneration) -> Self {
        Self {
            schema_version: generation.schema_version,
            last_indexed_at_unix_ms: generation.last_indexed_at_unix_ms,
            indexed_files: generation.indexed_files,
            diagnostic_count: generation.diagnostic_count,
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonRow {
    index: usize,
    kind: &'static str,
    row_id: String,
    #[serde(flatten)]
    fields: DashboardJsonRowFields,
}

impl DashboardJsonRow {
    fn from_panel_row(index: usize, row: &PanelRow) -> Self {
        Self {
            index,
            kind: row_kind(row),
            row_id: json_row_id(&row.row_id()),
            fields: DashboardJsonRowFields::from(row),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum DashboardJsonRowFields {
    Zettel(DashboardJsonZettelRow),
    Query(DashboardJsonQueryRow),
    Diagnostic(DashboardJsonDiagnosticRow),
    IndexStatus(DashboardJsonIndexStatusRow),
}

impl From<&PanelRow> for DashboardJsonRowFields {
    fn from(row: &PanelRow) -> Self {
        match row {
            PanelRow::Zettel(row) => Self::Zettel(DashboardJsonZettelRow::from(row)),
            PanelRow::Query(row) => Self::Query(DashboardJsonQueryRow::from(row)),
            PanelRow::Diagnostic(row) => Self::Diagnostic(DashboardJsonDiagnosticRow {
                id: row.id,
                severity: row.severity.clone(),
                category: row.category.clone(),
                code: row.code.clone(),
                message: row.message.clone(),
                absolute_path: row.absolute_path.as_ref().map(path_json),
                path: row.relative_path.as_ref().map(path_json),
                start_byte: row.start_byte,
                end_byte: row.end_byte,
                start_line: row.start_line,
                start_column: row.start_column,
                end_line: row.end_line,
                end_column: row.end_column,
                zettel_id: row.zettel_id,
            }),
            PanelRow::IndexStatus(row) => Self::IndexStatus(DashboardJsonIndexStatusRow {
                label: row.label.clone(),
                value: row.value,
            }),
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonZettelRow {
    store_id: i64,
    canonical_id: Option<String>,
    title: String,
    path: String,
    todo_marker: Option<String>,
    lifecycle_date: Option<String>,
    tags: Vec<String>,
    properties: Vec<DashboardJsonProperty>,
    badges: Vec<DashboardJsonBadge>,
    preview: Option<String>,
    source_order: i64,
    start_line: Option<usize>,
    start_column: Option<usize>,
    source_span: SourceSpan,
}

impl From<&ZettelRow> for DashboardJsonZettelRow {
    fn from(row: &ZettelRow) -> Self {
        Self {
            store_id: row.store_id,
            canonical_id: row.canonical_id.clone(),
            title: row.title.clone(),
            path: path_json(&row.file_path),
            todo_marker: row.todo_marker.clone(),
            lifecycle_date: row.lifecycle_date.clone(),
            tags: row.tags.clone(),
            properties: row
                .properties
                .iter()
                .map(|(key, value)| DashboardJsonProperty {
                    key: key.clone(),
                    value: value.clone(),
                })
                .collect(),
            badges: row.badges.iter().map(DashboardJsonBadge::from).collect(),
            preview: row.preview.clone(),
            source_order: row.source_order,
            start_line: row.start_line,
            start_column: row.start_column,
            source_span: row.source_span,
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonProperty {
    key: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct DashboardJsonBadge {
    label: String,
    query: String,
}

impl From<&QueryBadge> for DashboardJsonBadge {
    fn from(badge: &QueryBadge) -> Self {
        Self {
            label: badge.label.clone(),
            query: badge.query.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonQueryRow {
    id: String,
    title: String,
    source_path: Option<String>,
    source_kind: Option<&'static str>,
    output_kind: Option<&'static str>,
    definition_preview: Option<String>,
    valid: bool,
    error: Option<String>,
    row_count_preview: Option<usize>,
    row_count_error: Option<String>,
    start_line: Option<usize>,
    start_column: Option<usize>,
}

impl From<&QueryRow> for DashboardJsonQueryRow {
    fn from(row: &QueryRow) -> Self {
        Self {
            id: row.id.clone(),
            title: row.title.clone(),
            source_path: row.source_path.as_ref().map(path_json),
            source_kind: row.source_kind.map(query_source_kind_json),
            output_kind: row.output_kind.map(query_output_kind_json),
            definition_preview: row.definition_preview.clone(),
            valid: row.valid,
            error: row.error.clone(),
            row_count_preview: row.row_count_preview,
            row_count_error: row.row_count_error.clone(),
            start_line: row.start_line,
            start_column: row.start_column,
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonDiagnosticRow {
    id: i64,
    severity: String,
    category: String,
    code: Option<String>,
    message: String,
    absolute_path: Option<String>,
    path: Option<String>,
    start_byte: Option<usize>,
    end_byte: Option<usize>,
    start_line: Option<usize>,
    start_column: Option<usize>,
    end_line: Option<usize>,
    end_column: Option<usize>,
    zettel_id: Option<i64>,
}

#[derive(Debug, Serialize)]
struct DashboardJsonIndexStatusRow {
    label: String,
    value: usize,
}

#[derive(Debug, Serialize)]
struct DashboardJsonGraph {
    state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected: Option<DashboardJsonGraphZettel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    outgoing: Option<DashboardJsonGraphSection<DashboardJsonGraphLink>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    incoming: Option<DashboardJsonGraphSection<DashboardJsonGraphLink>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ancestors: Option<DashboardJsonGraphSection<DashboardJsonGraphZettel>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    descendants: Option<DashboardJsonGraphSection<DashboardJsonGraphZettel>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl DashboardJsonGraph {
    fn from_state(state: &GraphLoadState) -> Self {
        match state {
            GraphLoadState::Unavailable => Self::empty("unavailable", None),
            GraphLoadState::Loading => Self::empty("loading", None),
            GraphLoadState::Failed { message } => Self::empty("failed", Some(message.clone())),
            GraphLoadState::Ready(graph) => Self::from_neighborhood(graph),
        }
    }

    fn empty(state: &'static str, error: Option<String>) -> Self {
        Self {
            state,
            selected: None,
            outgoing: None,
            incoming: None,
            ancestors: None,
            descendants: None,
            error,
        }
    }

    fn from_neighborhood(graph: &GraphNeighborhood) -> Self {
        Self {
            state: "ready",
            selected: Some(DashboardJsonGraphZettel::from(&graph.selected)),
            outgoing: Some(DashboardJsonGraphSection::from_link_section(
                &graph.outgoing,
            )),
            incoming: Some(DashboardJsonGraphSection::from_link_section(
                &graph.incoming,
            )),
            ancestors: Some(DashboardJsonGraphSection::from_zettel_section(
                &graph.ancestors,
            )),
            descendants: Some(DashboardJsonGraphSection::from_zettel_section(
                &graph.descendants,
            )),
            error: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonGraphSection<T> {
    total_count: usize,
    truncated_count: usize,
    rows: Vec<T>,
}

impl DashboardJsonGraphSection<DashboardJsonGraphLink> {
    fn from_link_section(section: &GraphSection<GraphLinkRow>) -> Self {
        Self {
            total_count: section.total_count,
            truncated_count: section.truncated_count,
            rows: section
                .rows
                .iter()
                .map(DashboardJsonGraphLink::from)
                .collect(),
        }
    }
}

impl DashboardJsonGraphSection<DashboardJsonGraphZettel> {
    fn from_zettel_section(section: &GraphSection<GraphZettelRow>) -> Self {
        Self {
            total_count: section.total_count,
            truncated_count: section.truncated_count,
            rows: section
                .rows
                .iter()
                .map(DashboardJsonGraphZettel::from)
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonGraphLink {
    link_id: i64,
    source: Option<DashboardJsonGraphZettel>,
    target: Option<DashboardJsonGraphZettel>,
    target_text: String,
    target_canonical_id: Option<String>,
    link_kind: String,
    resolved: bool,
    source_span: SourceSpan,
}

impl From<&GraphLinkRow> for DashboardJsonGraphLink {
    fn from(row: &GraphLinkRow) -> Self {
        Self {
            link_id: row.link_id,
            source: row.source.as_ref().map(DashboardJsonGraphZettel::from),
            target: row.target.as_ref().map(DashboardJsonGraphZettel::from),
            target_text: row.target_text.clone(),
            target_canonical_id: row.target_canonical_id.clone(),
            link_kind: row.link_kind.clone(),
            resolved: row.resolved,
            source_span: row.source_span,
        }
    }
}

#[derive(Debug, Serialize)]
struct DashboardJsonGraphZettel {
    store_id: i64,
    canonical_id: Option<String>,
    title: String,
    path: String,
    source_order: i64,
    start_line: Option<usize>,
    start_column: Option<usize>,
    source_span: SourceSpan,
}

impl From<&GraphZettelRow> for DashboardJsonGraphZettel {
    fn from(row: &GraphZettelRow) -> Self {
        Self {
            store_id: row.store_id,
            canonical_id: row.canonical_id.clone(),
            title: row.title.clone(),
            path: path_json(&row.file_path),
            source_order: row.source_order,
            start_line: row.start_line,
            start_column: row.start_column,
            source_span: row.source_span,
        }
    }
}

fn row_kind(row: &PanelRow) -> &'static str {
    match row {
        PanelRow::Zettel(_) => "zettel",
        PanelRow::Query(_) => "query",
        PanelRow::Diagnostic(_) => "diagnostic",
        PanelRow::IndexStatus(_) => "index_status",
    }
}

fn json_row_id(row_id: &PanelRowId) -> String {
    match row_id {
        PanelRowId::ZettelCanonical(id) => format!("@{id}"),
        PanelRowId::ZettelStore(id) => format!("store:{id}"),
        PanelRowId::Query(id) => format!("@{id}"),
        PanelRowId::DiagnosticDb(id) => format!("diagnostic:{id}"),
        PanelRowId::DiagnosticFallback {
            code,
            path,
            start_line,
            start_column,
        } => match (start_line, start_column) {
            (Some(line), Some(column)) => format!("diagnostic:{code}:{path}:{line}:{column}"),
            (Some(line), None) => format!("diagnostic:{code}:{path}:{line}"),
            _ => format!("diagnostic:{code}:{path}"),
        },
        PanelRowId::IndexStatus(label) => format!("index:{label}"),
    }
}

fn query_source_kind_json(kind: zorg_query::QueryDefinitionSourceKind) -> &'static str {
    match kind {
        zorg_query::QueryDefinitionSourceKind::Property => "property",
        zorg_query::QueryDefinitionSourceKind::FencedSwog => "fenced_swog",
    }
}

fn query_output_kind_json(kind: zorg_query::QueryResultKind) -> &'static str {
    match kind {
        zorg_query::QueryResultKind::List => "list",
        zorg_query::QueryResultKind::Table => "table",
        zorg_query::QueryResultKind::Aggregate => "aggregate",
    }
}

fn action_kind_json(kind: PendingOperationKind) -> &'static str {
    match kind {
        PendingOperationKind::InitialLoad => "initial_load",
        PendingOperationKind::Refresh => "refresh",
        PendingOperationKind::Reindex => "reindex",
        PendingOperationKind::Search => "search",
        PendingOperationKind::Capture => "capture",
        PendingOperationKind::FixPreview => "fix_preview",
        PendingOperationKind::FixApply => "fix_apply",
        PendingOperationKind::TodoApply => "todo_apply",
        PendingOperationKind::FreshnessCheck => "freshness_check",
    }
}

fn duration_ms(duration: Option<Duration>) -> Option<u64> {
    duration.map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
}

fn path_json(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use serde_json::Value;
    use tempfile::TempDir;
    use zorg_store::{Store, StoreOptions};

    use super::*;
    use crate::data;

    #[test]
    fn serializes_degraded_frame_with_error_context() {
        let frame = DashboardFrame::new(
            PathBuf::from("/tmp/corpus"),
            PathBuf::from("/tmp/missing.sqlite3"),
            Panel::Index,
            None,
            DashboardSnapshot::Degraded {
                message: "index is unavailable".to_owned(),
            },
        );

        let json = serialize_frame(&frame, DashboardRenderState::for_frame(&frame), None)
            .expect("serialize degraded frame");
        let value: Value = serde_json::from_str(&json).expect("parse dashboard json");

        assert_eq!(value["schema"], "zorg.dash.frame");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["snapshot"]["state"], "degraded");
        assert_eq!(value["snapshot"]["error"], "index is unavailable");
        assert_eq!(value["active_panel"], "index");
        assert_eq!(value["active_panel_rows"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn serializes_ready_frame_rows_health_freshness_and_graph() {
        let fixture = JsonFixture::new(
            "\
%%% @root #z/ref
Root
%%%

- @today #z/inbox #z/todo [ ] due::2026-05-04 project::dash
  Today task links #target and #missing.

- @target #z/ref
  Target row.
",
        );
        let options = DashOptionsForJson {
            root: fixture.root.clone(),
            db: fixture.db.clone(),
            panel: Panel::Today,
            query: None,
        };
        let mut frame = options.load_frame();
        frame.record_initial_load_duration(Duration::from_millis(17));
        let selected = frame
            .active_rows()
            .into_iter()
            .find_map(|row| match row {
                PanelRow::Zettel(row) => Some(row),
                _ => None,
            })
            .expect("selected zettel row");
        let graph = data::load_graph_neighborhood(fixture.store_options(), &selected);

        let json = serialize_frame(
            &frame,
            DashboardRenderState::for_frame(&frame),
            Some(&graph),
        )
        .expect("serialize ready frame");
        assert!(!json.contains('\u{1b}'));
        let value: Value = serde_json::from_str(&json).expect("parse dashboard json");

        assert_eq!(value["snapshot"]["state"], "ready");
        assert_eq!(value["health"]["label"], "current");
        assert_eq!(value["freshness"]["state"], "current");
        assert_eq!(value["telemetry"]["last_initial_load_ms"], 17);
        assert!(value["row_counts"]["today"].as_u64().unwrap() >= 1);
        assert_eq!(value["active_panel_rows"][0]["kind"], "zettel");
        assert_eq!(value["active_panel_rows"][0]["canonical_id"], "today");
        assert!(
            value["active_panel_rows"][0]["properties"]
                .as_array()
                .unwrap()
                .iter()
                .any(|property| property["key"] == "project" && property["value"] == "dash")
        );
        assert_eq!(value["inspector"]["graph"]["state"], "ready");
        assert!(
            value["inspector"]["graph"]["outgoing"]["total_count"]
                .as_u64()
                .unwrap()
                >= 2
        );
    }

    #[test]
    fn preserves_active_panel_row_order() {
        let fixture = JsonFixture::new(
            "\
%%% @root #z/ref
Root
%%%

- @alpha #z/inbox Alpha.
- @beta #z/inbox Beta.
",
        );
        let frame = DashOptionsForJson {
            root: fixture.root.clone(),
            db: fixture.db.clone(),
            panel: Panel::Inbox,
            query: None,
        }
        .load_frame();

        let json = serialize_frame(&frame, DashboardRenderState::for_frame(&frame), None)
            .expect("serialize inbox frame");
        let value: Value = serde_json::from_str(&json).expect("parse dashboard json");
        let row_ids = value["active_panel_rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["row_id"].as_str().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(row_ids, vec!["@alpha", "@beta"]);
    }

    struct DashOptionsForJson {
        root: PathBuf,
        db: PathBuf,
        panel: Panel,
        query: Option<String>,
    }

    impl DashOptionsForJson {
        fn load_frame(&self) -> DashboardFrame {
            let snapshot = data::load_snapshot(self.store_options(), self.query.as_deref(), None);
            DashboardFrame::new(
                self.root.clone(),
                self.db.clone(),
                self.panel,
                self.query.clone(),
                snapshot,
            )
        }

        fn store_options(&self) -> StoreOptions {
            StoreOptions::new(&self.root, &self.db).expect("store options")
        }
    }

    struct JsonFixture {
        _temp: TempDir,
        root: PathBuf,
        db: PathBuf,
    }

    impl JsonFixture {
        fn new(source: &str) -> Self {
            let temp = TempDir::new().expect("temp dir");
            let root = temp.path().join("corpus");
            let db = temp.path().join("zorg.sqlite3");
            std::fs::create_dir_all(&root).expect("create corpus");
            std::fs::write(root.join("main.z"), source).expect("write source");
            let options = StoreOptions::new(&root, &db).expect("store options");
            let mut store = Store::open_with_options(options).expect("open store");
            store.reindex().expect("reindex fixture");
            Self {
                _temp: temp,
                root,
                db,
            }
        }

        fn store_options(&self) -> StoreOptions {
            StoreOptions::new(&self.root, &self.db).expect("store options")
        }
    }

    use crate::model::DashboardFrame;
}
