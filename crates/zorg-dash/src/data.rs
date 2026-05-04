use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use zorg_core::SourceSpan;
use zorg_query::{
    QueryContext, QueryDate, QueryDefinitionError, QueryDefinitionListing, QueryExecutionError,
    execute_query, list_query_definitions, query_definition_by_id,
};
use zorg_store::{Store, StoreOptions, StoredFile, StoredLink, StoredZettel};

use crate::model::{
    DashboardSnapshot, DiagnosticRow, GRAPH_SECTION_ROW_LIMIT, GraphLinkRow, GraphLoadState,
    GraphNeighborhood, GraphSection, GraphZettelRow, IndexGeneration, IndexPanel, IndexStatusRow,
    QueryBadge, QueryPanel, QueryRow, SearchPanel, SearchQueryInfo, SnapshotFreshness,
    TODAY_QUERY_SPECS, TodayQuery, ZettelRow,
};

#[cfg(test)]
thread_local! {
    static PREVIEW_COLLECTION_REQUESTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_preview_collection_requests() {
    PREVIEW_COLLECTION_REQUESTS.with(|requests| requests.set(0));
}

#[cfg(test)]
pub(crate) fn preview_collection_requests() -> usize {
    PREVIEW_COLLECTION_REQUESTS.with(std::cell::Cell::get)
}

pub(crate) fn load_snapshot(
    options: StoreOptions,
    search_query: Option<&str>,
) -> DashboardSnapshot {
    match Store::open_read_only_with_options(options) {
        Ok(store) => match load_ready_snapshot(&store, search_query) {
            Ok(snapshot) => snapshot,
            Err(message) => DashboardSnapshot::Degraded { message },
        },
        Err(error) => DashboardSnapshot::Degraded {
            message: error.to_string(),
        },
    }
}

fn load_ready_snapshot(
    store: &Store,
    search_query: Option<&str>,
) -> Result<DashboardSnapshot, String> {
    let schema_version = store.schema_version().map_err(|error| error.to_string())?;
    let status = store.index_status().map_err(|error| error.to_string())?;
    let diagnostics = load_diagnostics(store).map_err(|error| error.to_string())?;
    let context = SnapshotLoadContext::new(store)?;
    let today = load_today(&context, &diagnostics).map_err(|error| error.to_string())?;
    let inbox = query_zettel(&context, "#z/inbox")?;
    let queries = load_queries(&context)?;
    let search = search_query
        .map(|query| search_panel(&context, query))
        .unwrap_or_else(|| Ok(SearchPanel::empty("")))?;

    Ok(DashboardSnapshot::Ready {
        index: Box::new(IndexPanel::from_parts(schema_version, status)),
        diagnostics,
        today,
        inbox,
        queries,
        search,
    })
}

pub(crate) fn load_search_panel(options: StoreOptions, query: &str) -> Result<SearchPanel, String> {
    let store = Store::open_read_only_with_options(options).map_err(|error| error.to_string())?;
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(SearchPanel::empty(query));
    }

    let context = SnapshotLoadContext::new(&store)?;
    search_panel(&context, query)
}

#[allow(dead_code)]
pub(crate) fn load_graph_neighborhood(
    options: StoreOptions,
    selected: &ZettelRow,
) -> GraphLoadState {
    if selected.store_id <= 0 {
        return GraphLoadState::Unavailable;
    }

    match Store::open_read_only_with_options(options) {
        Ok(store) => load_graph_neighborhood_from_store(&store, selected, GRAPH_SECTION_ROW_LIMIT)
            .map(GraphLoadState::Ready)
            .unwrap_or_else(|message| GraphLoadState::Failed { message }),
        Err(error) => GraphLoadState::Failed {
            message: error.to_string(),
        },
    }
}

#[allow(dead_code)]
fn load_graph_neighborhood_from_store(
    store: &Store,
    selected: &ZettelRow,
    section_limit: usize,
) -> Result<GraphNeighborhood, String> {
    let file_paths = store
        .list_files()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|file| (file.id, file))
        .collect::<BTreeMap<_, _>>();
    let zettels = store
        .list_zettel()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|zettel| {
            let row = graph_zettel_row_from_stored(&zettel, &file_paths);
            (zettel.id, row)
        })
        .collect::<BTreeMap<_, _>>();
    let selected_row = zettels
        .get(&selected.store_id)
        .cloned()
        .unwrap_or_else(|| graph_zettel_row_from_dashboard(selected));
    let outgoing = store
        .list_outgoing_links(selected.store_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|link| graph_link_row(link, &zettels))
        .collect::<Vec<_>>();
    let incoming = store
        .list_incoming_links(selected.store_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|link| graph_link_row(link, &zettels))
        .collect::<Vec<_>>();
    let ancestors = store
        .list_zettel_ancestors(selected.store_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|zettel| graph_zettel_row_from_stored(&zettel, &file_paths))
        .collect::<Vec<_>>();
    let descendants = store
        .list_zettel_descendants(selected.store_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|zettel| graph_zettel_row_from_stored(&zettel, &file_paths))
        .collect::<Vec<_>>();

    Ok(GraphNeighborhood {
        selected: selected_row,
        outgoing: GraphSection::bounded(outgoing, section_limit),
        incoming: GraphSection::bounded(incoming, section_limit),
        ancestors: GraphSection::bounded(ancestors, section_limit),
        descendants: GraphSection::bounded(descendants, section_limit),
    })
}

#[allow(dead_code)]
fn graph_link_row(link: StoredLink, zettels: &BTreeMap<i64, GraphZettelRow>) -> GraphLinkRow {
    GraphLinkRow {
        link_id: link.id,
        source: zettels.get(&link.source_zettel_id).cloned(),
        target: link
            .target_zettel_id
            .and_then(|target_id| zettels.get(&target_id).cloned()),
        target_text: link.target_text,
        target_canonical_id: link.target_canonical_id,
        link_kind: link.link_kind,
        resolved: link.resolved,
        source_span: SourceSpan::bytes(
            optional_usize(Some(link.start_byte)).unwrap_or(0),
            optional_usize(Some(link.end_byte)).unwrap_or(0),
        ),
    }
}

#[allow(dead_code)]
fn graph_zettel_row_from_stored(
    zettel: &StoredZettel,
    file_paths: &BTreeMap<i64, StoredFile>,
) -> GraphZettelRow {
    GraphZettelRow {
        store_id: zettel.id,
        canonical_id: zettel.canonical_id.clone(),
        title: zettel_title(zettel),
        file_path: file_paths
            .get(&zettel.file_id)
            .map(|file| file.relative_path.clone())
            .unwrap_or_default(),
        source_order: zettel.source_order,
        start_line: optional_usize(zettel.start_line),
        start_column: optional_usize(zettel.start_column),
        source_span: SourceSpan {
            start_byte: optional_usize(Some(zettel.start_byte)).unwrap_or(0),
            end_byte: optional_usize(Some(zettel.end_byte)).unwrap_or(0),
            start_line: optional_usize(zettel.start_line),
            start_column: optional_usize(zettel.start_column),
            end_line: optional_usize(zettel.end_line),
            end_column: optional_usize(zettel.end_column),
        },
    }
}

#[allow(dead_code)]
fn graph_zettel_row_from_dashboard(row: &ZettelRow) -> GraphZettelRow {
    GraphZettelRow {
        store_id: row.store_id,
        canonical_id: row.canonical_id.clone(),
        title: row.title.clone(),
        file_path: row.file_path.clone(),
        source_order: row.source_order,
        start_line: row.start_line,
        start_column: row.start_column,
        source_span: row.source_span,
    }
}

#[allow(dead_code)]
fn zettel_title(zettel: &StoredZettel) -> String {
    zettel
        .title
        .clone()
        .or_else(|| zettel.canonical_id.clone())
        .unwrap_or_else(|| zettel.parser_key.clone())
}

pub(crate) fn check_snapshot_freshness(
    options: StoreOptions,
    captured: IndexGeneration,
) -> SnapshotFreshness {
    match Store::open_read_only_with_options(options)
        .and_then(|store| {
            let schema_version = store.schema_version()?;
            let status = store.index_status()?;
            Ok((schema_version, status))
        })
        .map_err(|error| error.to_string())
    {
        Ok((schema_version, status)) => {
            let current = IndexGeneration::from_status(schema_version, &status);
            if status.new_files > 0 || status.changed_files > 0 || status.deleted_files > 0 {
                SnapshotFreshness::StaleSources {
                    generation: current,
                    new_files: status.new_files,
                    changed_files: status.changed_files,
                    deleted_files: status.deleted_files,
                }
            } else if current != captured {
                SnapshotFreshness::NewerIndexAvailable { captured, current }
            } else {
                SnapshotFreshness::Current {
                    generation: current,
                }
            }
        }
        Err(message) => SnapshotFreshness::CheckFailed { message },
    }
}

fn load_today(
    context: &SnapshotLoadContext<'_>,
    diagnostics: &[DiagnosticRow],
) -> Result<Vec<crate::model::PanelRow>, String> {
    let mut rows_by_id = BTreeMap::<i64, ZettelRow>::new();
    let mut query_counts = Vec::new();

    for spec in TODAY_QUERY_SPECS {
        let rows = query_zettel(context, spec.query)?;
        query_counts.push(TodayQuery {
            label: spec.label.to_owned(),
            query: spec.query.to_owned(),
            count: rows.len(),
        });
        for mut row in rows {
            row.badges.push(QueryBadge::new(spec.label, spec.query));
            rows_by_id
                .entry(row.store_id)
                .and_modify(|existing| {
                    if !existing
                        .badges
                        .iter()
                        .any(|badge| badge.query == spec.query)
                    {
                        existing
                            .badges
                            .push(QueryBadge::new(spec.label, spec.query));
                    }
                })
                .or_insert(row);
        }
    }

    let mut rows = rows_by_id
        .into_values()
        .map(crate::model::PanelRow::Zettel)
        .collect::<Vec<_>>();
    rows.extend(
        diagnostics
            .iter()
            .cloned()
            .map(crate::model::PanelRow::Diagnostic),
    );
    rows.sort_by_key(crate::model::PanelRow::sort_key);

    if rows.is_empty() && query_counts.iter().any(|query| query.count == 0) {
        return Ok(rows);
    }

    Ok(rows)
}

fn search_panel(context: &SnapshotLoadContext<'_>, query: &str) -> Result<SearchPanel, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(SearchPanel::empty(query));
    }

    if is_stored_query_id(trimmed) {
        return stored_query_search_panel(context, query, trimmed);
    }

    match query_zettel(context, trimmed) {
        Ok(rows) => Ok(SearchPanel::with_rows(query, rows)),
        Err(error) => Ok(SearchPanel::with_error(query, error)),
    }
}

fn stored_query_search_panel(
    context: &SnapshotLoadContext<'_>,
    input: &str,
    query_id: &str,
) -> Result<SearchPanel, String> {
    let query_context = context.query_context();
    let definition = match query_definition_by_id(context.store, query_id) {
        Ok(definition) => definition,
        Err(error) => {
            let query_id = query_id.trim_start_matches('@').to_owned();
            let title = context
                .store
                .lookup_zettel_by_canonical_id(&query_id)
                .ok()
                .flatten()
                .and_then(|zettel| zettel.title);
            let source_path =
                definition_error_source_path(&error).map(|path| display_source_path(context, path));
            let message = query_execution_error_message(error);
            let info = SearchQueryInfo::invalid(query_id, title, source_path, message.clone());
            return Ok(SearchPanel::with_error_and_info(input, message, info));
        }
    };

    let title = context
        .store
        .lookup_zettel_by_canonical_id(&definition.zettel_id)
        .ok()
        .flatten()
        .and_then(|zettel| zettel.title);
    let source_path = display_source_path(context, definition.source_path.clone());
    let info = SearchQueryInfo::valid(
        definition.zettel_id.clone(),
        title,
        source_path,
        definition.source_kind,
        definition.output_kind,
        definition.query.clone(),
    );

    match execute_query(context.store, &query_context, &definition.query) {
        Ok(result) => Ok(SearchPanel::with_rows_and_info(
            input,
            result
                .rows
                .into_iter()
                .map(|row| zettel_row_from_query_result(context, row))
                .collect(),
            info,
        )),
        Err(error) => Ok(SearchPanel::with_error_and_info(
            input,
            query_execution_error_message(error),
            info,
        )),
    }
}

fn query_zettel(context: &SnapshotLoadContext<'_>, query: &str) -> Result<Vec<ZettelRow>, String> {
    let query_context = context.query_context();
    execute_query(context.store, &query_context, query)
        .map_err(query_execution_error_message)
        .map(|result| {
            result
                .rows
                .into_iter()
                .map(|row| zettel_row_from_query_result(context, row))
                .collect()
        })
}

fn zettel_row_from_query_result(
    context: &SnapshotLoadContext<'_>,
    row: zorg_query::QueryResultRow,
) -> ZettelRow {
    ZettelRow {
        store_id: row.zettel_store_id,
        canonical_id: row.canonical_id,
        file_path: row.file_path,
        title: row.title,
        todo_marker: row.todo_marker,
        todo_span: row.todo_span,
        source_span: row.source_span,
        source_order: row.source_order,
        start_line: row.source_span.start_line,
        start_column: row.source_span.start_column,
        lifecycle_date: row
            .lifecycle_date
            .map(|date| format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)),
        tags: row.tags,
        properties: row
            .properties
            .into_iter()
            .map(|property| (property.key, property.value))
            .collect(),
        preview: context.preview_for(row.zettel_store_id).cloned(),
        badges: Vec::new(),
    }
}

fn load_queries(context: &SnapshotLoadContext<'_>) -> Result<QueryPanel, String> {
    let query_context = context.query_context();
    let rows = list_query_definitions(context.store)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|listing| query_row_from_listing(context, &query_context, listing))
        .collect::<Vec<_>>();
    Ok(QueryPanel { rows })
}

fn query_row_from_listing(
    context: &SnapshotLoadContext<'_>,
    query_context: &QueryContext,
    listing: QueryDefinitionListing,
) -> QueryRow {
    match listing {
        QueryDefinitionListing::Valid(summary) => {
            let preview = definition_preview(&summary.query);
            let source_path = display_source_path(context, summary.source_path);
            let (row_count_preview, row_count_error) =
                match execute_query(context.store, query_context, &summary.query) {
                    Ok(result) => (Some(result.rows.len()), None),
                    Err(error) => (None, Some(error.to_string())),
                };
            QueryRow {
                id: summary.zettel_id.clone(),
                title: summary.title.unwrap_or_else(|| summary.zettel_id.clone()),
                source_path: Some(source_path),
                source_kind: Some(summary.source_kind),
                output_kind: Some(summary.output_kind),
                definition_preview: Some(preview),
                valid: true,
                error: None,
                row_count_preview,
                row_count_error,
                start_line: summary.span.start_line,
                start_column: summary.span.start_column,
            }
        }
        QueryDefinitionListing::Invalid(row) => {
            let (start_line, start_column) = query_definition_error_position(row.error.as_ref());
            QueryRow {
                id: row.zettel_id.clone(),
                title: row.title.unwrap_or_else(|| row.zettel_id.clone()),
                source_path: row
                    .source_path
                    .map(|path| display_source_path(context, path)),
                source_kind: None,
                output_kind: None,
                definition_preview: None,
                valid: false,
                error: Some(query_definition_error_message(row.error.as_ref())),
                row_count_preview: None,
                row_count_error: None,
                start_line,
                start_column,
            }
        }
    }
}

fn display_source_path(
    context: &SnapshotLoadContext<'_>,
    path: std::path::PathBuf,
) -> std::path::PathBuf {
    path.strip_prefix(context.store.root())
        .map(std::path::Path::to_path_buf)
        .unwrap_or(path)
}

fn definition_preview(query: &str) -> String {
    query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(96)
        .collect()
}

fn query_definition_error_position(error: &QueryDefinitionError) -> (Option<usize>, Option<usize>) {
    match error {
        QueryDefinitionError::QueryParse { span, .. } => (span.start_line, span.start_column),
        _ => (None, None),
    }
}

fn definition_error_source_path(error: &QueryExecutionError) -> Option<std::path::PathBuf> {
    let QueryExecutionError::Definition(error) = error else {
        return None;
    };
    match error.as_ref() {
        QueryDefinitionError::SourceRead { source_path, .. }
        | QueryDefinitionError::SourceParse { source_path, .. }
        | QueryDefinitionError::SourceMismatch { source_path, .. }
        | QueryDefinitionError::NotQueryZettel { source_path, .. }
        | QueryDefinitionError::NoDefinition { source_path, .. }
        | QueryDefinitionError::MultipleQueryProperties { source_path, .. }
        | QueryDefinitionError::MultipleSwogBlocks { source_path, .. }
        | QueryDefinitionError::AmbiguousDefinition { source_path, .. }
        | QueryDefinitionError::QueryParse { source_path, .. } => Some(source_path.clone()),
        QueryDefinitionError::InvalidId { .. }
        | QueryDefinitionError::NotFound { .. }
        | QueryDefinitionError::MissingFile { .. } => None,
    }
}

fn query_execution_error_message(error: QueryExecutionError) -> String {
    match error {
        QueryExecutionError::Parse(error) => {
            query_parse_error_message("query parse failed", &error)
        }
        QueryExecutionError::Definition(error) => query_definition_error_message(error.as_ref()),
        QueryExecutionError::Store(error) => format!("query store failed: {error}"),
        QueryExecutionError::Evaluation(error) => format!("query evaluation failed: {error}"),
    }
}

fn query_definition_error_message(error: &QueryDefinitionError) -> String {
    match error {
        QueryDefinitionError::QueryParse { error: parse, .. } => {
            format!(
                "{}\n{}",
                error,
                query_parse_error_message("definition parse failed", parse)
            )
        }
        _ => error.to_string(),
    }
}

fn query_parse_error_message(prefix: &str, error: &zorg_query::QueryError) -> String {
    if error.diagnostics.is_empty() {
        return prefix.to_owned();
    }
    let mut lines = vec![prefix.to_owned()];
    lines.extend(error.diagnostics.iter().map(|diagnostic| {
        let code = diagnostic.code.as_deref().unwrap_or("query.parse");
        format!(
            "{code} at byte {}: {}",
            diagnostic.span.start_byte, diagnostic.message
        )
    }));
    lines.join("\n")
}

struct SnapshotLoadContext<'store> {
    store: &'store Store,
    previews: BTreeMap<i64, String>,
}

impl<'store> SnapshotLoadContext<'store> {
    fn new(store: &'store Store) -> Result<Self, String> {
        Ok(Self {
            store,
            previews: collect_preview_texts(store)?,
        })
    }

    fn preview_for(&self, zettel_store_id: i64) -> Option<&String> {
        self.previews.get(&zettel_store_id)
    }

    fn query_context(&self) -> QueryContext {
        QueryContext::new(
            self.store.root(),
            current_query_date(),
            current_unix_ms().try_into().unwrap_or(i64::MAX),
        )
    }
}

fn collect_preview_texts(store: &Store) -> Result<BTreeMap<i64, String>, String> {
    record_preview_collection_request();
    Ok(store
        .list_zettel()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter_map(|zettel| preview_text(&zettel.body_text).map(|preview| (zettel.id, preview)))
        .collect::<BTreeMap<_, _>>())
}

#[cfg(test)]
fn record_preview_collection_request() {
    PREVIEW_COLLECTION_REQUESTS.with(|requests| {
        requests.set(requests.get().saturating_add(1));
    });
}

#[cfg(not(test))]
fn record_preview_collection_request() {}

fn is_stored_query_id(query: &str) -> bool {
    query.starts_with('@') && !query.chars().any(char::is_whitespace)
}

fn preview_text(body_text: &str) -> Option<String> {
    body_text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(96).collect())
}

fn load_diagnostics(store: &Store) -> Result<Vec<DiagnosticRow>, String> {
    let mut rows = store
        .list_diagnostics()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|diagnostic| DiagnosticRow {
            id: diagnostic.id,
            severity: diagnostic.severity,
            category: diagnostic.category,
            code: diagnostic.code,
            message: diagnostic.message,
            absolute_path: diagnostic.absolute_path,
            relative_path: diagnostic.relative_path,
            start_byte: optional_usize(diagnostic.start_byte),
            end_byte: optional_usize(diagnostic.end_byte),
            start_line: diagnostic
                .start_line
                .and_then(|value| value.try_into().ok()),
            start_column: diagnostic
                .start_column
                .and_then(|value| value.try_into().ok()),
            end_line: diagnostic.end_line.and_then(|value| value.try_into().ok()),
            end_column: diagnostic
                .end_column
                .and_then(|value| value.try_into().ok()),
            zettel_id: diagnostic.zettel_id,
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(DiagnosticRow::sort_key);
    Ok(rows)
}

fn optional_usize(value: Option<i64>) -> Option<usize> {
    value.and_then(|value| value.try_into().ok())
}

pub(crate) fn current_query_date() -> QueryDate {
    let days = (current_unix_ms() / 1000).div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    QueryDate::new(year, month, day).expect("civil date should be valid")
}

fn current_unix_ms() -> i128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i128)
        .unwrap_or(0)
}

fn civil_from_days(days_since_epoch: i128) -> (i32, u8, u8) {
    let days = days_since_epoch + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u8, day as u8)
}

impl IndexPanel {
    fn from_parts(schema_version: i64, status: zorg_store::IndexStatus) -> Self {
        Self {
            schema_version,
            rows: vec![
                IndexStatusRow::new("Discovered files", status.discovered_files),
                IndexStatusRow::new("Indexed files", status.indexed_files),
                IndexStatusRow::new("Unchanged files", status.unchanged_files),
                IndexStatusRow::new("New files", status.new_files),
                IndexStatusRow::new("Changed files", status.changed_files),
                IndexStatusRow::new("Deleted files", status.deleted_files),
                IndexStatusRow::new("Diagnostics", status.diagnostic_count),
                IndexStatusRow::new("Effective tags", status.effective_tag_count),
            ],
            discovered_files: status.discovered_files,
            indexed_files: status.indexed_files,
            changed_files: status.changed_files,
            new_files: status.new_files,
            deleted_files: status.deleted_files,
            diagnostic_count: status.diagnostic_count,
            last_indexed_at_unix_ms: status.last_indexed_at_unix_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::thread;
    use std::time::Duration;

    use tempfile::TempDir;

    #[test]
    fn graph_neighborhood_maps_links_and_hierarchy() {
        let fixture = GraphFixture::new();
        let section = fixture.dashboard_row("a/section");
        let graph =
            load_graph_neighborhood_from_store(&fixture.store, &section, 8).expect("section graph");

        assert_eq!(graph.selected.canonical_id.as_deref(), Some("a/section"));
        assert!(graph.outgoing.total_count >= 4);
        let resolved = graph
            .outgoing
            .rows
            .iter()
            .find(|link| link.target_text == "#target")
            .expect("resolved outgoing target");
        assert!(resolved.resolved);
        assert_eq!(
            resolved
                .source
                .as_ref()
                .and_then(|row| row.canonical_id.as_deref()),
            Some("a/section")
        );
        assert_eq!(
            resolved
                .target
                .as_ref()
                .and_then(|row| row.canonical_id.as_deref()),
            Some("target")
        );
        assert_eq!(
            resolved.target.as_ref().map(|row| row.file_path.as_path()),
            Some(Path::new("target.z"))
        );

        let unresolved = graph
            .outgoing
            .rows
            .iter()
            .find(|link| link.target_text == "#missing")
            .expect("unresolved outgoing target");
        assert!(!unresolved.resolved);
        assert!(unresolved.target.is_none());
        assert_eq!(
            unresolved
                .source
                .as_ref()
                .and_then(|row| row.canonical_id.as_deref()),
            Some("a/section")
        );

        let target = fixture.dashboard_row("target");
        let graph =
            load_graph_neighborhood_from_store(&fixture.store, &target, 8).expect("target graph");
        assert_eq!(graph.incoming.total_count, 2);
        assert_eq!(
            graph
                .incoming
                .rows
                .iter()
                .map(|link| link
                    .source
                    .as_ref()
                    .and_then(|row| row.canonical_id.as_deref()))
                .collect::<Vec<_>>(),
            vec![Some("a/section"), Some("a/section/leaf")]
        );

        let leaf = fixture.dashboard_row("a/section/leaf");
        let graph =
            load_graph_neighborhood_from_store(&fixture.store, &leaf, 8).expect("leaf graph");
        assert_eq!(
            graph
                .ancestors
                .rows
                .iter()
                .map(|row| row.canonical_id.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("a"), Some("a/section")]
        );

        let root = fixture.dashboard_row("a");
        let graph =
            load_graph_neighborhood_from_store(&fixture.store, &root, 8).expect("root graph");
        assert_eq!(
            graph
                .descendants
                .rows
                .iter()
                .map(|row| row.canonical_id.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("a/section"), Some("a/section/leaf")]
        );
    }

    #[test]
    fn graph_neighborhood_bounds_high_degree_sections() {
        let fixture = GraphFixture::new();
        let section = fixture.dashboard_row("a/section");
        let graph =
            load_graph_neighborhood_from_store(&fixture.store, &section, 2).expect("section graph");

        assert!(graph.outgoing.total_count >= 4);
        assert_eq!(graph.outgoing.rows.len(), 2);
        assert_eq!(
            graph.outgoing.truncated_count,
            graph.outgoing.total_count - graph.outgoing.rows.len()
        );
    }

    #[test]
    fn graph_neighborhood_failure_stays_local_to_graph_state() {
        let temp = TempDir::new().expect("temp dir");
        let root = temp.path().join("corpus");
        std::fs::create_dir_all(&root).expect("create corpus");
        let options =
            StoreOptions::new(&root, temp.path().join("missing.sqlite3")).expect("options");
        let row = ZettelRow {
            store_id: 1,
            canonical_id: Some("missing".to_owned()),
            file_path: PathBuf::from("missing.z"),
            title: "Missing".to_owned(),
            todo_marker: None,
            todo_span: None,
            source_span: SourceSpan::bytes(0, 0),
            source_order: 0,
            start_line: Some(1),
            start_column: Some(1),
            lifecycle_date: None,
            tags: Vec::new(),
            properties: Vec::new(),
            preview: None,
            badges: Vec::new(),
        };

        assert!(matches!(
            load_graph_neighborhood(options, &row),
            GraphLoadState::Failed { .. }
        ));
    }

    struct GraphFixture {
        _temp: TempDir,
        store: Store,
    }

    impl GraphFixture {
        fn new() -> Self {
            let temp = TempDir::new().expect("temp dir");
            let root = temp.path().join("corpus");
            std::fs::create_dir_all(&root).expect("create corpus");
            write_source(
                &root,
                "a.z",
                "\
%%% @a #area/root
A
%%%

- @a/section #area/section Section.
  This links to #target and #missing and #ghost and #another.

  - @a/section/leaf #area/leaf Leaf.
    Leaf links #target.
",
            );
            write_source(&root, "target.z", "%%% @target #area/target\nTarget\n%%%\n");
            let db = temp.path().join("zorg.sqlite3");
            let mut store =
                Store::open_with_options(StoreOptions::new(&root, db).expect("options"))
                    .expect("open store");
            store.reindex().expect("reindex graph fixture");
            Self { _temp: temp, store }
        }

        fn dashboard_row(&self, canonical_id: &str) -> ZettelRow {
            let zettel = self
                .store
                .lookup_zettel_by_canonical_id(canonical_id)
                .expect("lookup zettel")
                .expect("zettel exists");
            let file_paths = self
                .store
                .list_files()
                .expect("list files")
                .into_iter()
                .map(|file| (file.id, file.relative_path))
                .collect::<BTreeMap<_, _>>();
            ZettelRow {
                store_id: zettel.id,
                canonical_id: zettel.canonical_id.clone(),
                file_path: file_paths.get(&zettel.file_id).cloned().unwrap_or_default(),
                title: zettel_title(&zettel),
                todo_marker: None,
                todo_span: None,
                source_span: SourceSpan {
                    start_byte: optional_usize(Some(zettel.start_byte)).unwrap_or(0),
                    end_byte: optional_usize(Some(zettel.end_byte)).unwrap_or(0),
                    start_line: optional_usize(zettel.start_line),
                    start_column: optional_usize(zettel.start_column),
                    end_line: optional_usize(zettel.end_line),
                    end_column: optional_usize(zettel.end_column),
                },
                source_order: zettel.source_order,
                start_line: optional_usize(zettel.start_line),
                start_column: optional_usize(zettel.start_column),
                lifecycle_date: None,
                tags: Vec::new(),
                properties: Vec::new(),
                preview: None,
                badges: Vec::new(),
            }
        }
    }

    fn write_source(root: &Path, name: &str, source: &str) {
        fs::write(root.join(name), source).expect("write source");
    }

    #[test]
    fn freshness_check_reports_current_snapshot_generation() {
        let (_temp, options) = indexed_corpus("current", "%%% @root #z/ref\nRoot\n%%%\n");
        let captured = captured_generation(&load_snapshot(options.clone(), None));

        assert!(matches!(
            check_snapshot_freshness(options, captured),
            SnapshotFreshness::Current { .. }
        ));
    }

    #[test]
    fn freshness_check_reports_newer_index_after_external_reindex() {
        let (temp, options) = indexed_corpus("newer", "%%% @root #z/ref\nRoot\n%%%\n");
        let captured = captured_generation(&load_snapshot(options.clone(), None));

        thread::sleep(Duration::from_millis(2));
        fs::write(
            temp.path().join("note.z"),
            "%%% @root #z/ref\nRoot changed\n%%%\n",
        )
        .expect("modify source");
        let mut store = Store::open_with_options(options.clone()).expect("open writable store");
        store.reindex().expect("external reindex");
        drop(store);

        assert!(matches!(
            check_snapshot_freshness(options, captured),
            SnapshotFreshness::NewerIndexAvailable { .. }
        ));
    }

    #[test]
    fn freshness_check_keeps_source_staleness_separate_from_newer_index() {
        let (temp, options) = indexed_corpus("stale-source", "%%% @root #z/ref\nRoot\n%%%\n");
        let captured = captured_generation(&load_snapshot(options.clone(), None));

        fs::write(temp.path().join("new.z"), "%%% @new #z/ref\nNew\n%%%\n")
            .expect("write new source");

        assert!(matches!(
            check_snapshot_freshness(options, captured),
            SnapshotFreshness::StaleSources { new_files: 1, .. }
        ));
    }

    #[test]
    fn freshness_check_failure_is_non_fatal_state() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("corpus");
        let db = temp.path().join("missing").join("zorg.sqlite3");
        fs::create_dir_all(&root).expect("create root");
        let options = StoreOptions::new(root, db).expect("store options");
        let captured = IndexGeneration::new(2, Some(1), 1, 0);

        assert!(matches!(
            check_snapshot_freshness(options, captured),
            SnapshotFreshness::CheckFailed { .. }
        ));
    }

    fn indexed_corpus(label: &str, source: &str) -> (tempfile::TempDir, StoreOptions) {
        let temp = tempfile::Builder::new()
            .prefix(&format!("zorg-dash-freshness-{label}-"))
            .tempdir()
            .expect("tempdir");
        fs::write(temp.path().join("note.z"), source).expect("write source");
        let db = temp.path().join("zorg.sqlite3");
        let options = StoreOptions::new(temp.path(), &db).expect("store options");
        let mut store = Store::open_with_options(options.clone()).expect("open writable store");
        store.reindex().expect("reindex");
        drop(store);
        (temp, options)
    }

    fn captured_generation(snapshot: &DashboardSnapshot) -> IndexGeneration {
        match snapshot {
            DashboardSnapshot::Ready { index, .. } => index.generation(),
            other => panic!("expected ready snapshot, got {other:?}"),
        }
    }
}
