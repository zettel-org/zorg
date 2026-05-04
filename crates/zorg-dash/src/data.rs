use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use zorg_query::{
    QueryContext, QueryDate, QueryDefinitionError, QueryDefinitionListing, execute_query,
    list_query_definitions, query_definition_by_id,
};
use zorg_store::{Store, StoreOptions};

use crate::model::{
    DashboardSnapshot, DiagnosticRow, IndexPanel, IndexStatusRow, QueryBadge, QueryPanel, QueryRow,
    SearchPanel, SearchQueryInfo, TODAY_QUERY_SPECS, TodayQuery, ZettelRow,
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
            let info = SearchQueryInfo::invalid(
                query_id.trim_start_matches('@').to_owned(),
                error.to_string(),
            );
            return Ok(SearchPanel::with_error_and_info(
                input,
                error.to_string(),
                info,
            ));
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
            error.to_string(),
            info,
        )),
    }
}

fn query_zettel(context: &SnapshotLoadContext<'_>, query: &str) -> Result<Vec<ZettelRow>, String> {
    let query_context = context.query_context();
    execute_query(context.store, &query_context, query)
        .map_err(|error| error.to_string())
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
                error: Some(row.error.to_string()),
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
