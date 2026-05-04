use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use zorg_core::{BodyBlock, SourceSpan, Zettel, ZettelId};
use zorg_query::{
    QueryContext, QueryDate, QueryDefinitionError, QueryDefinitionListing, QueryExecutionError,
    execute_query, list_query_definitions, parse_output_query, query_definition_by_id,
};
use zorg_store::{Store, StoreOptions, StoredFile, StoredLink, StoredZettel};

use crate::model::{
    CustomPanel, DashboardDefinition, DashboardDefinitionDiagnostic, DashboardPanelDefinition,
    DashboardPanelQuerySource, DashboardSnapshot, DiagnosticRow, GRAPH_SECTION_ROW_LIMIT,
    GraphLinkRow, GraphLoadState, GraphNeighborhood, GraphSection, GraphZettelRow, IndexGeneration,
    IndexPanel, IndexStatusRow, QueryBadge, QueryPanel, QueryRow, SearchPanel, SearchQueryInfo,
    SelectedDashboard, SnapshotFreshness, TODAY_QUERY_SPECS, TodayQuery, ZettelRow,
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
    dashboard_id: Option<&str>,
) -> DashboardSnapshot {
    match Store::open_read_only_with_options(options) {
        Ok(store) => match load_ready_snapshot(&store, search_query, dashboard_id) {
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
    dashboard_id: Option<&str>,
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
    let selected_dashboard = dashboard_id
        .map(|dashboard_id| load_selected_dashboard_from_store(store, dashboard_id))
        .transpose()?;
    let custom_panels = load_custom_panels(&context, selected_dashboard.as_ref());

    Ok(DashboardSnapshot::Ready {
        index: Box::new(IndexPanel::from_parts(schema_version, status)),
        diagnostics,
        today,
        inbox,
        queries,
        search,
        selected_dashboard,
        custom_panels,
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
pub(crate) fn load_selected_dashboard(
    options: StoreOptions,
    dashboard_id: &str,
) -> Result<SelectedDashboard, String> {
    let store = Store::open_read_only_with_options(options).map_err(|error| error.to_string())?;
    load_selected_dashboard_from_store(&store, dashboard_id)
}

#[allow(dead_code)]
fn load_selected_dashboard_from_store(
    store: &Store,
    dashboard_id: &str,
) -> Result<SelectedDashboard, String> {
    let requested_id = dashboard_id.trim().to_owned();
    let canonical_id = match ZettelId::parse(&requested_id) {
        Ok(id) => id.as_str().to_owned(),
        Err(error) => {
            return Ok(SelectedDashboard::missing(
                requested_id,
                vec![DashboardDefinitionDiagnostic::error(
                    "dashboard.invalid_id",
                    format!("invalid dashboard ID `{dashboard_id}`: {error}"),
                )],
            ));
        }
    };
    let stored = match store
        .lookup_zettel_by_canonical_id(&canonical_id)
        .map_err(|error| error.to_string())?
    {
        Some(stored) => stored,
        None => {
            return Ok(SelectedDashboard::missing(
                requested_id,
                vec![DashboardDefinitionDiagnostic::error(
                    "dashboard.not_found",
                    format!("dashboard `{dashboard_id}` was not found in the index"),
                )],
            ));
        }
    };
    let file = file_for_zettel(store, &stored)?;
    let source_path = store.root().join(&file.relative_path);
    let source = std::fs::read_to_string(&source_path).map_err(|error| {
        format!(
            "failed to read dashboard source {}: {error}",
            source_path.display()
        )
    })?;
    let mut document = zorg_parse::parse_zettel_document_with_path(&source, source_path.clone())
        .map_err(|error| {
            format!(
                "failed to parse dashboard source {}: {error}",
                source_path.display()
            )
        })?;
    zorg_parse::resolve_document(&mut document);
    let source_zettel = match find_zettel_by_canonical_id(&document.root, &canonical_id) {
        Some(zettel) => zettel,
        None => {
            return Ok(SelectedDashboard::missing(
                requested_id,
                vec![
                    DashboardDefinitionDiagnostic::error(
                        "dashboard.source_mismatch",
                        format!(
                            "dashboard `{dashboard_id}` was indexed but was not found in source"
                        ),
                    )
                    .with_source(
                        display_source_path_for_store(store, &source_path),
                        Some(stored_span(&stored)),
                    ),
                ],
            ));
        }
    };
    let mut diagnostics = Vec::new();
    let display_path = display_source_path_for_store(store, &source_path);
    let effective_tags = store
        .list_effective_tags_for_zettel(stored.id)
        .map_err(|error| error.to_string())?;
    let indexed_tags = store
        .list_tags_for_zettel(stored.id)
        .map_err(|error| error.to_string())?;
    let has_dashboard_tag = effective_tags.iter().any(|tag| tag.tag == "z/dashboard")
        || indexed_tags.iter().any(|tag| tag.tag == "z/dashboard");
    if !has_dashboard_tag {
        diagnostics.push(
            DashboardDefinitionDiagnostic::error(
                "dashboard.wrong_tag",
                format!("dashboard `{dashboard_id}` must carry effective tag #z/dashboard"),
            )
            .with_source(display_path.clone(), source_zettel.span),
        );
        return Ok(SelectedDashboard::missing(requested_id, diagnostics));
    }

    let title = property_value(source_zettel, "title")
        .map(str::to_owned)
        .unwrap_or_else(|| canonical_id.clone());
    let panels = parse_dashboard_panels(store, source_zettel, &display_path, &mut diagnostics);
    let definition = DashboardDefinition {
        id: canonical_id,
        title,
        source_path: display_path,
        source_span: source_zettel.span.unwrap_or_else(|| stored_span(&stored)),
        panels,
    };

    Ok(SelectedDashboard {
        requested_id,
        definition: Some(definition),
        diagnostics,
    })
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

fn load_custom_panels(
    context: &SnapshotLoadContext<'_>,
    selected_dashboard: Option<&SelectedDashboard>,
) -> BTreeMap<String, CustomPanel> {
    let Some(definition) = selected_dashboard.and_then(|dashboard| dashboard.definition.as_ref())
    else {
        return BTreeMap::new();
    };

    definition
        .panels
        .iter()
        .map(|panel| {
            let custom_panel = execute_custom_panel(context, panel);
            (panel.key.clone(), custom_panel)
        })
        .collect()
}

fn execute_custom_panel(
    context: &SnapshotLoadContext<'_>,
    panel: &DashboardPanelDefinition,
) -> CustomPanel {
    let query_context = context.query_context();
    match execute_query(context.store, &query_context, panel.query_source.query()) {
        Ok(result) => {
            let rows = result
                .rows
                .into_iter()
                .map(|row| {
                    let mut row = zettel_row_from_query_result(context, row);
                    row.badges
                        .push(QueryBadge::new(&panel.key, panel.query_source.query()));
                    row
                })
                .collect();
            CustomPanel {
                definition: panel.clone(),
                rows,
                error: None,
            }
        }
        Err(error) => CustomPanel {
            definition: panel.clone(),
            rows: Vec::new(),
            error: Some(query_execution_error_message(error)),
        },
    }
}

fn parse_dashboard_panels(
    store: &Store,
    dashboard: &Zettel,
    source_path: &std::path::Path,
    diagnostics: &mut Vec<DashboardDefinitionDiagnostic>,
) -> Vec<DashboardPanelDefinition> {
    let mut panels = Vec::new();
    let mut seen_keys = std::collections::BTreeSet::<String>::new();

    for panel_zettel in dashboard.body.iter().filter_map(direct_panel_child) {
        let span = panel_zettel.span.unwrap_or_else(|| SourceSpan::bytes(0, 0));
        let mut panel_diagnostics = Vec::new();
        let key = match single_property_value(
            panel_zettel,
            "key",
            source_path,
            "dashboard.panel.duplicate_key_property",
            &mut panel_diagnostics,
        ) {
            Some(key) if is_valid_panel_key(key) => key.to_owned(),
            Some(key) => {
                panel_diagnostics.push(
                    DashboardDefinitionDiagnostic::error(
                        "dashboard.panel.invalid_key",
                        format!("dashboard panel key `{key}` must contain only letters, numbers, '-', '_', or '/'"),
                    )
                    .with_source(source_path, property_span(panel_zettel, "key").or(Some(span))),
                );
                panel_fallback_key(panel_zettel)
            }
            None => {
                panel_diagnostics.push(
                    DashboardDefinitionDiagnostic::error(
                        "dashboard.panel.missing_key",
                        "dashboard panel must define key::<panel-key>",
                    )
                    .with_source(source_path, Some(span)),
                );
                panel_fallback_key(panel_zettel)
            }
        };
        let title = match single_property_value(
            panel_zettel,
            "title",
            source_path,
            "dashboard.panel.duplicate_title_property",
            &mut panel_diagnostics,
        ) {
            Some(title) if !title.trim().is_empty() => title.to_owned(),
            _ => {
                panel_diagnostics.push(
                    DashboardDefinitionDiagnostic::error(
                        "dashboard.panel.missing_title",
                        format!("dashboard panel `{key}` must define title::<display-title>"),
                    )
                    .with_source(
                        source_path,
                        property_span(panel_zettel, "title").or(Some(span)),
                    )
                    .with_panel_key(key.clone()),
                );
                String::new()
            }
        };

        if !seen_keys.insert(key.clone()) {
            panel_diagnostics.push(
                DashboardDefinitionDiagnostic::error(
                    "dashboard.panel.duplicate_key",
                    format!("dashboard panel key `{key}` is already defined"),
                )
                .with_source(
                    source_path,
                    property_span(panel_zettel, "key").or(Some(span)),
                )
                .with_panel_key(key.clone()),
            );
        }

        let query_source = parse_panel_query_source(
            store,
            panel_zettel,
            source_path,
            &key,
            &mut panel_diagnostics,
        );
        if let Some(query_source) = query_source {
            if panel_diagnostics.is_empty() {
                panels.push(DashboardPanelDefinition {
                    key,
                    title,
                    query_source,
                    source_span: span,
                });
            }
        }
        diagnostics.extend(panel_diagnostics);
    }

    panels
}

fn direct_panel_child(block: &BodyBlock) -> Option<&Zettel> {
    let BodyBlock::ChildZettel(child) = block else {
        return None;
    };
    child
        .type_tags
        .iter()
        .any(|tag| tag.tag.as_str() == "z/panel")
        .then_some(child.as_ref())
}

fn parse_panel_query_source(
    store: &Store,
    panel: &Zettel,
    source_path: &std::path::Path,
    key: &str,
    diagnostics: &mut Vec<DashboardDefinitionDiagnostic>,
) -> Option<DashboardPanelQuerySource> {
    let query_properties = panel
        .properties
        .iter()
        .filter(|property| property.key == "query" || property.key == "query-id")
        .collect::<Vec<_>>();
    let swog_blocks = panel
        .body
        .iter()
        .filter_map(|block| match block {
            BodyBlock::FencedCode(block) if block.info.as_deref() == Some("swog") => Some(block),
            _ => None,
        })
        .collect::<Vec<_>>();
    let span = panel.span.unwrap_or_else(|| SourceSpan::bytes(0, 0));

    if query_properties.len() > 1 {
        diagnostics.push(
            DashboardDefinitionDiagnostic::error(
                "dashboard.panel.multiple_query_properties",
                format!("dashboard panel `{key}` must define only one query:: property"),
            )
            .with_source(source_path, Some(span))
            .with_panel_key(key.to_owned()),
        );
    }
    if swog_blocks.len() > 1 {
        diagnostics.push(
            DashboardDefinitionDiagnostic::error(
                "dashboard.panel.multiple_swog_blocks",
                format!("dashboard panel `{key}` must define only one fenced swog block"),
            )
            .with_source(source_path, Some(span))
            .with_panel_key(key.to_owned()),
        );
    }
    if !query_properties.is_empty() && !swog_blocks.is_empty() {
        diagnostics.push(
            DashboardDefinitionDiagnostic::error(
                "dashboard.panel.ambiguous_query_source",
                format!("dashboard panel `{key}` must use either query:: or fenced swog, not both"),
            )
            .with_source(source_path, Some(span))
            .with_panel_key(key.to_owned()),
        );
    }

    if query_properties.is_empty() && swog_blocks.is_empty() {
        diagnostics.push(
            DashboardDefinitionDiagnostic::error(
                "dashboard.panel.missing_query_source",
                format!(
                    "dashboard panel `{key}` must define query::@query/id or a fenced swog block"
                ),
            )
            .with_source(source_path, Some(span))
            .with_panel_key(key.to_owned()),
        );
        return None;
    }
    if query_properties.len() > 1
        || swog_blocks.len() > 1
        || (!query_properties.is_empty() && !swog_blocks.is_empty())
    {
        return None;
    }

    if let Some(property) = query_properties.first() {
        return parse_stored_panel_query_source(
            store,
            &property.value,
            source_path,
            property.value_span.or(property.span),
            key,
            diagnostics,
        );
    }

    swog_blocks.first().and_then(|block| {
        parse_inline_panel_query_source(
            &block.body,
            source_path,
            block.body_span.or(block.span),
            key,
            diagnostics,
        )
    })
}

fn parse_stored_panel_query_source(
    store: &Store,
    query_id: &str,
    source_path: &std::path::Path,
    span: Option<SourceSpan>,
    key: &str,
    diagnostics: &mut Vec<DashboardDefinitionDiagnostic>,
) -> Option<DashboardPanelQuerySource> {
    match query_definition_by_id(store, query_id) {
        Ok(definition) => Some(DashboardPanelQuerySource::StoredQuery {
            id: definition.zettel_id,
            query: definition.query,
            output_kind: definition.output_kind,
            source_path: display_source_path_for_store(store, &definition.source_path),
            source_span: definition.span,
        }),
        Err(error) => {
            diagnostics.push(
                DashboardDefinitionDiagnostic::error(
                    "dashboard.panel.invalid_stored_query",
                    format!(
                        "dashboard panel `{key}` references invalid stored query `{query_id}`: {}",
                        query_execution_error_message(error)
                    ),
                )
                .with_source(source_path, span)
                .with_panel_key(key.to_owned()),
            );
            None
        }
    }
}

fn parse_inline_panel_query_source(
    query: &str,
    source_path: &std::path::Path,
    span: Option<SourceSpan>,
    key: &str,
    diagnostics: &mut Vec<DashboardDefinitionDiagnostic>,
) -> Option<DashboardPanelQuerySource> {
    let (query, source_span) =
        trim_panel_query(query, span.unwrap_or_else(|| SourceSpan::bytes(0, 0)));
    match parse_output_query(&query) {
        Ok(output) => Some(DashboardPanelQuerySource::InlineSwog {
            query,
            output_kind: output.kind,
            source_span,
        }),
        Err(error) => {
            diagnostics.push(
                DashboardDefinitionDiagnostic::error(
                    "dashboard.panel.invalid_inline_swog",
                    format!(
                        "dashboard panel `{key}` has invalid inline SWOG:\n{}",
                        query_parse_error_message("inline SWOG parse failed", &error)
                    ),
                )
                .with_source(source_path, Some(source_span))
                .with_panel_key(key.to_owned()),
            );
            None
        }
    }
}

fn trim_panel_query(query: &str, span: SourceSpan) -> (String, SourceSpan) {
    let trimmed_start = query.len() - query.trim_start().len();
    let trimmed_end = query.len() - query.trim_end().len();
    let start_byte = span.start_byte.saturating_add(trimmed_start);
    let end_byte = span.end_byte.saturating_sub(trimmed_end);
    (
        query.trim().to_owned(),
        SourceSpan::bytes(start_byte, end_byte.max(start_byte)),
    )
}

fn single_property_value<'a>(
    zettel: &'a Zettel,
    key: &str,
    source_path: &std::path::Path,
    duplicate_code: &'static str,
    diagnostics: &mut Vec<DashboardDefinitionDiagnostic>,
) -> Option<&'a str> {
    let properties = zettel
        .properties
        .iter()
        .filter(|property| property.key == key)
        .collect::<Vec<_>>();
    if properties.len() > 1 {
        diagnostics.push(
            DashboardDefinitionDiagnostic::error(
                duplicate_code,
                format!("dashboard panel must define only one {key}:: property"),
            )
            .with_source(source_path, zettel.span),
        );
    }
    properties.first().map(|property| property.value.as_str())
}

fn property_value<'a>(zettel: &'a Zettel, key: &str) -> Option<&'a str> {
    zettel
        .properties
        .iter()
        .find(|property| property.key == key)
        .map(|property| property.value.as_str())
}

fn property_span(zettel: &Zettel, key: &str) -> Option<SourceSpan> {
    zettel
        .properties
        .iter()
        .find(|property| property.key == key)
        .and_then(|property| property.value_span.or(property.span))
}

fn is_valid_panel_key(key: &str) -> bool {
    !key.is_empty()
        && key.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '/')
        })
}

fn panel_fallback_key(panel: &Zettel) -> String {
    panel
        .canonical_id
        .as_ref()
        .map(|id| id.as_str().to_owned())
        .unwrap_or_else(|| panel.key.as_str().to_owned())
}

fn file_for_zettel(store: &Store, zettel: &StoredZettel) -> Result<StoredFile, String> {
    store
        .list_files()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|file| file.id == zettel.file_id)
        .ok_or_else(|| {
            format!(
                "indexed zettel {} references missing file row {}",
                zettel
                    .canonical_id
                    .as_deref()
                    .unwrap_or(zettel.parser_key.as_str()),
                zettel.file_id
            )
        })
}

fn display_source_path_for_store(store: &Store, path: &std::path::Path) -> std::path::PathBuf {
    path.strip_prefix(store.root())
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}

fn stored_span(zettel: &StoredZettel) -> SourceSpan {
    SourceSpan {
        start_byte: optional_usize(Some(zettel.start_byte)).unwrap_or(0),
        end_byte: optional_usize(Some(zettel.end_byte)).unwrap_or(0),
        start_line: optional_usize(zettel.start_line),
        start_column: optional_usize(zettel.start_column),
        end_line: optional_usize(zettel.end_line),
        end_column: optional_usize(zettel.end_column),
    }
}

fn find_zettel_by_canonical_id<'a>(zettel: &'a Zettel, canonical_id: &str) -> Option<&'a Zettel> {
    if zettel
        .canonical_id
        .as_ref()
        .is_some_and(|id| id.as_str() == canonical_id)
    {
        return Some(zettel);
    }

    zettel.body.iter().find_map(|block| match block {
        BodyBlock::ChildZettel(child) => find_zettel_by_canonical_id(child, canonical_id),
        BodyBlock::Paragraph(_) | BodyBlock::FencedCode(_) => None,
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
    fn loads_dashboard_definition_with_stored_and_inline_panels() {
        let (_temp, options) = indexed_dashboard_corpus(
            "valid",
            "\
%%% @dashboards/daily #z/dashboard title::Daily
Daily dashboard.
%%%

- @dashboards/daily/open #z/panel key::open title::Open query::@queries/open

- @dashboards/daily/table #z/panel key::table title::Table
  ```swog
  TABLE #z/todo
  ```

- @queries/open #z/query title::Open query::#z/todo

- @todos/one #z/todo [ ] One task.
",
        );

        let selected =
            load_selected_dashboard(options, "@dashboards/daily").expect("load dashboard");
        let definition = selected.definition.as_ref().expect("dashboard definition");

        assert_eq!(selected.requested_id, "@dashboards/daily");
        assert_eq!(definition.id, "dashboards/daily");
        assert_eq!(definition.title, "Daily");
        assert!(selected.diagnostics.is_empty());
        assert_eq!(
            definition
                .panels
                .iter()
                .map(|panel| panel.key.as_str())
                .collect::<Vec<_>>(),
            vec!["open", "table"]
        );
        match &definition.panels[0].query_source {
            DashboardPanelQuerySource::StoredQuery {
                id,
                query,
                output_kind,
                ..
            } => {
                assert_eq!(id, "queries/open");
                assert_eq!(query, "#z/todo");
                assert_eq!(*output_kind, zorg_query::QueryResultKind::List);
            }
            other => panic!("expected stored query panel, got {other:?}"),
        }
        match &definition.panels[1].query_source {
            DashboardPanelQuerySource::InlineSwog {
                query, output_kind, ..
            } => {
                assert_eq!(query, "TABLE #z/todo");
                assert_eq!(*output_kind, zorg_query::QueryResultKind::Table);
            }
            other => panic!("expected inline SWOG panel, got {other:?}"),
        }
    }

    #[test]
    fn dashboard_loader_reports_missing_and_wrong_tag_as_local_diagnostics() {
        let (_temp, options) = indexed_dashboard_corpus(
            "wrong-tag",
            "\
%%% @dashboards/plain #z/ref title::Plain
Not a dashboard.
%%%
",
        );

        let missing =
            load_selected_dashboard(options.clone(), "@dashboards/missing").expect("missing load");
        assert!(missing.definition.is_none());
        assert_eq!(diagnostic_codes(&missing), vec!["dashboard.not_found"]);

        let wrong_tag =
            load_selected_dashboard(options, "@dashboards/plain").expect("wrong tag load");
        assert!(wrong_tag.definition.is_none());
        assert_eq!(diagnostic_codes(&wrong_tag), vec!["dashboard.wrong_tag"]);
    }

    #[test]
    fn dashboard_loader_keeps_valid_panels_with_mixed_panel_errors() {
        let (_temp, options) = indexed_dashboard_corpus(
            "mixed",
            "\
%%% @dashboards/mixed #z/dashboard title::Mixed
Mixed dashboard.
%%%

- @dashboards/mixed/valid #z/panel key::valid title::Valid query::@queries/open

- @dashboards/mixed/duplicate #z/panel key::valid title::Duplicate query::@queries/open

- @dashboards/mixed/missing-title #z/panel key::missing-title query::@queries/open

- @dashboards/mixed/bad-query #z/panel key::bad-query title::Bad Query query::@queries/missing

- @dashboards/mixed/bad-inline #z/panel key::bad-inline title::Bad Inline
  ```swog
  #z/todo OR
  ```

- @queries/open #z/query title::Open query::#z/todo
",
        );

        let selected =
            load_selected_dashboard(options, "@dashboards/mixed").expect("load dashboard");
        let definition = selected.definition.as_ref().expect("dashboard definition");

        assert_eq!(
            definition
                .panels
                .iter()
                .map(|panel| panel.key.as_str())
                .collect::<Vec<_>>(),
            vec!["valid"]
        );
        assert_eq!(
            diagnostic_codes(&selected),
            vec![
                "dashboard.panel.duplicate_key",
                "dashboard.panel.missing_title",
                "dashboard.panel.invalid_stored_query",
                "dashboard.panel.invalid_inline_swog",
            ]
        );
    }

    #[test]
    fn dashboard_loader_reports_invalid_dashboard_id_without_store_lookup() {
        let (_temp, options) = indexed_dashboard_corpus(
            "invalid-id",
            "%%% @dashboards/daily #z/dashboard title::Daily\nDaily.\n%%%\n",
        );

        let selected = load_selected_dashboard(options, "dashboards/daily").expect("load");

        assert!(selected.definition.is_none());
        assert_eq!(diagnostic_codes(&selected), vec!["dashboard.invalid_id"]);
    }

    #[test]
    fn snapshot_executes_custom_dashboard_panel_queries() {
        let (_temp, options) = indexed_dashboard_corpus(
            "custom-rows",
            "\
%%% @dashboards/daily #z/dashboard title::Daily
Daily dashboard.
%%%

- @dashboards/daily/open #z/panel key::open title::Open query::@queries/open

- @dashboards/daily/inline-empty #z/panel key::inline-empty title::Inline Empty
  ```swog
  #missing/tag
  ```

- @queries/open #z/query title::Open query::#z/todo

- @todos/one #z/todo [ ] One task.
",
        );

        let snapshot = load_snapshot(options, None, Some("@dashboards/daily"));
        let DashboardSnapshot::Ready { custom_panels, .. } = snapshot else {
            panic!("expected ready snapshot");
        };

        let open = custom_panels.get("open").expect("open custom panel");
        assert_eq!(open.definition.title, "Open");
        assert_eq!(open.rows.len(), 1);
        assert_eq!(open.rows[0].canonical_id.as_deref(), Some("todos/one"));
        assert!(open.error.is_none());

        let empty = custom_panels
            .get("inline-empty")
            .expect("inline empty custom panel");
        assert!(empty.rows.is_empty());
        assert!(empty.error.is_none());
    }

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
        let captured = captured_generation(&load_snapshot(options.clone(), None, None));

        assert!(matches!(
            check_snapshot_freshness(options, captured),
            SnapshotFreshness::Current { .. }
        ));
    }

    #[test]
    fn freshness_check_reports_newer_index_after_external_reindex() {
        let (temp, options) = indexed_corpus("newer", "%%% @root #z/ref\nRoot\n%%%\n");
        let captured = captured_generation(&load_snapshot(options.clone(), None, None));

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
        let captured = captured_generation(&load_snapshot(options.clone(), None, None));

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

    fn indexed_dashboard_corpus(label: &str, source: &str) -> (tempfile::TempDir, StoreOptions) {
        let temp = tempfile::Builder::new()
            .prefix(&format!("zorg-dash-dashboard-{label}-"))
            .tempdir()
            .expect("tempdir");
        fs::write(temp.path().join("dashboard.z"), source).expect("write dashboard source");
        let db = temp.path().join("zorg.sqlite3");
        let options = StoreOptions::new(temp.path(), &db).expect("store options");
        let mut store = Store::open_with_options(options.clone()).expect("open writable store");
        store.reindex().expect("reindex");
        drop(store);
        (temp, options)
    }

    fn diagnostic_codes(selected: &SelectedDashboard) -> Vec<&'static str> {
        selected
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect()
    }

    fn captured_generation(snapshot: &DashboardSnapshot) -> IndexGeneration {
        match snapshot {
            DashboardSnapshot::Ready { index, .. } => index.generation(),
            other => panic!("expected ready snapshot, got {other:?}"),
        }
    }
}
