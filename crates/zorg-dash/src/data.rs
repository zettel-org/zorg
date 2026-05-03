use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use zorg_query::{QueryContext, QueryDate, execute_query, execute_query_by_id};
use zorg_store::{Store, StoreOptions};

use crate::model::{
    DashboardSnapshot, DiagnosticRow, IndexPanel, IndexStatusRow, QueryBadge, SearchPanel,
    TODAY_QUERY_SPECS, TodayQuery, ZettelRow,
};

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
    let today = load_today(store, &diagnostics).map_err(|error| error.to_string())?;
    let inbox = query_zettel(store, "#z/inbox")?;
    let search = search_query
        .map(|query| search_panel(store, query))
        .unwrap_or_else(|| Ok(SearchPanel::empty("")))?;

    Ok(DashboardSnapshot::Ready {
        index: IndexPanel::from_parts(schema_version, status),
        diagnostics,
        today,
        inbox,
        search,
    })
}

pub(crate) fn load_search_panel(options: StoreOptions, query: &str) -> Result<SearchPanel, String> {
    let store = Store::open_read_only_with_options(options).map_err(|error| error.to_string())?;
    search_panel(&store, query)
}

fn load_today(
    store: &Store,
    diagnostics: &[DiagnosticRow],
) -> Result<Vec<crate::model::PanelRow>, String> {
    let mut rows_by_id = BTreeMap::<i64, ZettelRow>::new();
    let mut query_counts = Vec::new();

    for spec in TODAY_QUERY_SPECS {
        let rows = query_zettel(store, spec.query)?;
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

fn search_panel(store: &Store, query: &str) -> Result<SearchPanel, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(SearchPanel::empty(query));
    }

    match query_zettel(store, trimmed) {
        Ok(rows) => Ok(SearchPanel::with_rows(query, rows)),
        Err(error) => Ok(SearchPanel::with_error(query, error)),
    }
}

fn query_zettel(store: &Store, query: &str) -> Result<Vec<ZettelRow>, String> {
    let previews = store
        .list_zettel()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter_map(|zettel| preview_text(&zettel.body_text).map(|preview| (zettel.id, preview)))
        .collect::<BTreeMap<_, _>>();
    let context = QueryContext::new(
        store.root(),
        current_query_date(),
        current_unix_ms().try_into().unwrap_or(i64::MAX),
    );
    let result = if is_stored_query_id(query) {
        execute_query_by_id(store, &context, query)
    } else {
        execute_query(store, &context, query)
    };

    result.map_err(|error| error.to_string()).map(|result| {
        result
            .rows
            .into_iter()
            .map(|row| ZettelRow {
                store_id: row.zettel_store_id,
                canonical_id: row.canonical_id,
                file_path: row.file_path,
                title: row.title,
                todo_marker: row.todo_marker,
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
                preview: previews.get(&row.zettel_store_id).cloned(),
                badges: Vec::new(),
            })
            .collect()
    })
}

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
            relative_path: diagnostic.relative_path,
            start_line: diagnostic
                .start_line
                .and_then(|value| value.try_into().ok()),
            start_column: diagnostic
                .start_column
                .and_then(|value| value.try_into().ok()),
            zettel_id: diagnostic.zettel_id,
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(DiagnosticRow::sort_key);
    Ok(rows)
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
