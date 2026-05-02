//! SWOG LIST query boundary for Zorg.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

use zorg_core::{DiagnosticCategory, SourceSpan, ZorgError, ZorgResult};

/// Parsed SWOG LIST query.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Query {
    /// Filters joined by implicit logical AND.
    pub filters: Vec<Filter>,
}

/// One parsed SWOG filter.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Filter {
    /// Property filter such as `due:<=today` or `did:*`.
    Property(PropertyFilter),
    /// Reserved query field such as `links`, `file`, `todo`, `text`, or `modified`.
    SpecialField(SpecialFieldFilter),
    /// Effective tag filter such as `#z/todo`.
    Tag(TagFilter),
    /// Negated filter such as `-#z/inbox`.
    Negated(NegatedFilter),
    /// Text phrase search from `"quoted text"` or `text:phrase`.
    Text(TextFilter),
}

/// Comparison operator used by property and special-field filters.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ComparisonOp {
    /// Equality or exact match.
    Equals,
    /// Greater-than comparison.
    GreaterThan,
    /// Greater-than-or-equal comparison.
    GreaterThanOrEqual,
    /// Less-than comparison.
    LessThan,
    /// Less-than-or-equal comparison.
    LessThanOrEqual,
    /// Existence check, written as `key:*`.
    Exists,
}

/// Property filter over ordinary zettel properties.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PropertyFilter {
    /// Property key.
    pub key: String,
    /// Comparison operator.
    pub op: ComparisonOp,
    /// Raw query value. Existence filters have no value.
    pub value: Option<String>,
}

/// Reserved SWOG field filter.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SpecialFieldFilter {
    /// Reserved field kind.
    pub field: SpecialField,
    /// Comparison operator.
    pub op: ComparisonOp,
    /// Raw query value.
    pub value: String,
}

/// Reserved SWOG field names with semantics defined by later evaluation phases.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SpecialField {
    /// `links:#target/path`.
    Links,
    /// `file:projects/*.z`.
    File,
    /// `todo:[ ]`, `todo:[N]`, `todo:[X]`, or `todo:[?]`.
    Todo,
    /// `text:phrase`.
    Text,
    /// `modified:<7d` or `modified:>=30d`.
    Modified,
}

/// Effective tag filter.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TagFilter {
    /// Tag path without the leading `#`.
    pub tag: String,
}

/// A negated filter.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NegatedFilter {
    /// Filter being negated.
    pub filter: Box<Filter>,
}

/// Text phrase search.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TextFilter {
    /// Search phrase.
    pub phrase: String,
    /// True when written with `text:`, false when written as a quoted phrase.
    pub explicit: bool,
}

/// Structured row for human-facing LIST rendering.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ListRow {
    /// Canonical zettel ID when present.
    pub canonical_id: Option<String>,
    /// Root-relative source file path.
    pub file_path: PathBuf,
    /// Title or first meaningful body line.
    pub title: String,
    /// Todo marker when present.
    pub todo_marker: Option<String>,
}

/// Internal row returned by store-backed SWOG LIST evaluation.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryResultRow {
    /// Stable SQLite zettel row ID.
    pub zettel_store_id: i64,
    /// Canonical zettel ID when present.
    pub canonical_id: Option<String>,
    /// Root-relative source file path.
    pub file_path: PathBuf,
    /// Title or first meaningful body line.
    pub title: String,
    /// Todo marker when present.
    pub todo_marker: Option<String>,
    /// Source order within the file.
    pub source_order: i64,
    /// Earliest lifecycle date from `due` or `do`, when indexed.
    pub lifecycle_date: Option<QueryDate>,
}

/// Planned, normalized query ready for evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryPlan {
    /// Normalized filters joined by implicit AND.
    pub filters: Vec<NormalizedFilter>,
    /// Deterministic ordering selected during planning.
    pub default_order: Vec<DefaultOrderKey>,
}

/// Store-backed query execution error.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum QueryExecutionError {
    /// Parser or normalization failed.
    Parse(QueryError),
    /// Store adapter failed while loading rows.
    Store(ZorgError),
    /// Evaluation failed after store rows were loaded.
    Evaluation(QueryEvaluationError),
}

impl fmt::Display for QueryExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "query parse failed: {error}"),
            Self::Store(error) => write!(formatter, "query store failed: {error}"),
            Self::Evaluation(error) => write!(formatter, "query evaluation failed: {error}"),
        }
    }
}

impl Error for QueryExecutionError {}

/// Query evaluator failure after a store snapshot has been loaded.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum QueryEvaluationError {
    /// A zettel row references a missing file row.
    MissingFile {
        /// Zettel row ID.
        zettel_id: i64,
        /// Referenced file row ID.
        file_id: i64,
    },
}

impl fmt::Display for QueryEvaluationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFile { zettel_id, file_id } => write!(
                formatter,
                "zettel row {zettel_id} references missing file row {file_id}"
            ),
        }
    }
}

impl Error for QueryEvaluationError {}

/// Context supplied by the caller for deterministic query normalization.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryContext {
    /// Corpus root used for path-relative query behavior.
    pub root: PathBuf,
    /// Local calendar date used to resolve `today`.
    pub today: QueryDate,
    /// Current timestamp in Unix milliseconds used for modified-age filters.
    pub now_unix_ms: i64,
    /// Timezone interpretation for caller-supplied dates and timestamps.
    pub timezone: TimezonePolicy,
    /// Current zettel row ID for future relative query behavior.
    pub current_zettel_id: Option<i64>,
}

impl QueryContext {
    /// Creates a deterministic query context.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, today: QueryDate, now_unix_ms: i64) -> Self {
        Self {
            root: root.into(),
            today,
            now_unix_ms,
            timezone: TimezonePolicy::Local,
            current_zettel_id: None,
        }
    }
}

/// Calendar date used by query normalization.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub struct QueryDate {
    /// Four-digit year.
    pub year: i32,
    /// One-based month.
    pub month: u8,
    /// One-based day of month.
    pub day: u8,
}

impl QueryDate {
    /// Creates a date after validating the month/day combination.
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        if month == 0 || month > 12 {
            return None;
        }
        let max_day = days_in_month(year, month);
        if day == 0 || day > max_day {
            return None;
        }
        Some(Self { year, month, day })
    }
}

/// Clock time used by query normalization.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub struct QueryTime {
    /// Hour in 24-hour local time.
    pub hour: u8,
    /// Minute.
    pub minute: u8,
    /// Second.
    pub second: u8,
}

impl QueryTime {
    /// Creates a time after validating each component.
    pub fn new(hour: u8, minute: u8, second: u8) -> Option<Self> {
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        Some(Self {
            hour,
            minute,
            second,
        })
    }
}

/// Timezone policy for interpreting caller-supplied context.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TimezonePolicy {
    /// Treat query dates and `today` as local calendar values.
    Local,
    /// Treat timestamp-derived behavior as UTC.
    Utc,
}

/// Query normalized for store-backed evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedQuery {
    /// Filters joined by implicit logical AND.
    pub filters: Vec<NormalizedFilter>,
    /// Default deterministic order selected for this query.
    pub default_order: Vec<DefaultOrderKey>,
}

/// One normalized SWOG filter.
#[derive(Debug, Clone, PartialEq)]
pub enum NormalizedFilter {
    /// Property filter over indexed zettel properties.
    Property(NormalizedPropertyFilter),
    /// Query over a reserved non-text field.
    Special(NormalizedSpecialFilter),
    /// Effective tag filter.
    EffectiveTag(NormalizedTagFilter),
    /// Text search filter.
    Text(NormalizedTextFilter),
    /// Negated normalized filter.
    Negated(Box<NormalizedFilter>),
}

/// Normalized property filter with typed comparison value.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedPropertyFilter {
    /// Property key.
    pub key: String,
    /// Comparison operator.
    pub op: ComparisonOp,
    /// Typed query value. Existence filters have no value.
    pub value: Option<ComparisonLiteral>,
    /// Equality/comparison semantics for this property.
    pub semantics: PropertySemantics,
}

/// Normalized reserved-field filter.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedSpecialFilter {
    /// Reserved field being queried.
    pub field: NormalizedSpecialField,
    /// Comparison operator.
    pub op: ComparisonOp,
    /// Typed query value.
    pub value: ComparisonLiteral,
}

/// Normalized reserved field names.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NormalizedSpecialField {
    /// Outgoing link target.
    Links,
    /// Root-relative source path glob.
    File,
    /// Todo marker.
    Todo,
    /// File modified-age comparison.
    Modified,
}

/// Normalized effective-tag query.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NormalizedTagFilter {
    /// Effective tag text without the leading `#`.
    pub tag: String,
}

/// Normalized text search query.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NormalizedTextFilter {
    /// Search phrase.
    pub phrase: String,
    /// True when written with `text:`.
    pub explicit: bool,
}

/// Typed comparison literal.
#[derive(Debug, Clone, PartialEq)]
pub enum ComparisonLiteral {
    /// Numeric comparison value.
    Number(f64),
    /// Date comparison value.
    Date(QueryDate),
    /// Time comparison value.
    Time(QueryTime),
    /// String equality value.
    String(String),
    /// Relative age in whole days, used by `modified`.
    RelativeDays(u32),
}

/// Property comparison semantics selected during normalization.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PropertySemantics {
    /// Existence check.
    Exists,
    /// String equality, including slash-list segment matching.
    StringEquality,
    /// Numeric scalar comparison.
    Number,
    /// Date scalar comparison.
    Date,
    /// Time scalar comparison.
    Time,
}

/// Deterministic default ordering keys.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DefaultOrderKey {
    /// Earliest `due`/`do` date first when lifecycle/todo filters are present.
    LifecycleDate,
    /// Root-relative source path.
    SourcePath,
    /// Source order within a file.
    SourceOrder,
    /// Stable store row ID final tie-breaker.
    StoreId,
}

/// Query-facing snapshot loaded from a store adapter.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryStoreSnapshot {
    /// Indexed source files.
    pub files: Vec<QueryFile>,
    /// Indexed zettel rows.
    pub zettel: Vec<QueryZettel>,
    /// Materialized effective tags.
    pub effective_tags: Vec<QueryEffectiveTag>,
    /// Indexed properties.
    pub properties: Vec<QueryProperty>,
    /// Indexed todos.
    pub todos: Vec<QueryTodo>,
    /// Indexed links.
    pub links: Vec<QueryLink>,
}

/// Query-facing store adapter.
pub trait QueryStore {
    /// Returns indexed source files.
    fn query_files(&self) -> ZorgResult<Vec<QueryFile>>;
    /// Returns indexed zettel rows.
    fn query_zettel(&self) -> ZorgResult<Vec<QueryZettel>>;
    /// Returns materialized effective tags.
    fn query_effective_tags(&self) -> ZorgResult<Vec<QueryEffectiveTag>>;
    /// Returns indexed properties.
    fn query_properties(&self) -> ZorgResult<Vec<QueryProperty>>;
    /// Returns indexed todos.
    fn query_todos(&self) -> ZorgResult<Vec<QueryTodo>>;
    /// Returns indexed links.
    fn query_links(&self) -> ZorgResult<Vec<QueryLink>>;
}

/// Query-facing source file row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryFile {
    /// Store row ID.
    pub id: i64,
    /// Root-relative source path.
    pub relative_path: PathBuf,
    /// File modified timestamp in Unix milliseconds.
    pub mtime_unix_ms: Option<i64>,
}

/// Query-facing zettel row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryZettel {
    /// Store row ID.
    pub id: i64,
    /// Owning file row ID.
    pub file_id: i64,
    /// Source order within the file.
    pub source_order: i64,
    /// Plain title text.
    pub title: Option<String>,
    /// Resolved canonical ID when present.
    pub canonical_id: Option<String>,
    /// Direct body text retained for text filters.
    pub body_text: String,
}

/// Query-facing effective tag row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryEffectiveTag {
    /// Owning zettel row ID.
    pub zettel_id: i64,
    /// Effective tag text without leading `#`.
    pub tag: String,
}

/// Query-facing property row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryProperty {
    /// Owning zettel row ID.
    pub zettel_id: i64,
    /// Property key.
    pub key: String,
    /// Property value.
    pub value: String,
}

/// Query-facing todo row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryTodo {
    /// Owning zettel row ID.
    pub zettel_id: i64,
    /// Todo marker text.
    pub marker: String,
}

/// Query-facing link row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryLink {
    /// Source zettel row ID.
    pub source_zettel_id: i64,
    /// Target zettel row ID for resolved links.
    pub target_zettel_id: Option<i64>,
    /// Canonical target ID when resolved.
    pub target_canonical_id: Option<String>,
    /// Link text as written.
    pub target_text: String,
    /// True when semantic resolution succeeded.
    pub resolved: bool,
}

impl QueryStore for zorg_store::Store {
    fn query_files(&self) -> ZorgResult<Vec<QueryFile>> {
        self.list_files().map(|files| {
            files
                .into_iter()
                .map(|file| QueryFile {
                    id: file.id,
                    relative_path: file.relative_path,
                    mtime_unix_ms: file.mtime_unix_ms,
                })
                .collect()
        })
    }

    fn query_zettel(&self) -> ZorgResult<Vec<QueryZettel>> {
        self.list_zettel().map(|zettel| {
            zettel
                .into_iter()
                .map(|zettel| QueryZettel {
                    id: zettel.id,
                    file_id: zettel.file_id,
                    source_order: zettel.source_order,
                    title: zettel.title,
                    canonical_id: zettel.canonical_id,
                    body_text: zettel.body_text,
                })
                .collect()
        })
    }

    fn query_effective_tags(&self) -> ZorgResult<Vec<QueryEffectiveTag>> {
        let mut tags = self
            .list_effective_tags()?
            .into_iter()
            .map(|tag| QueryEffectiveTag {
                zettel_id: tag.zettel_id,
                tag: tag.tag,
            })
            .collect::<Vec<_>>();

        tags.extend(
            self.list_tags()?
                .into_iter()
                .filter(|tag| tag.tag_kind == "type")
                .map(|tag| QueryEffectiveTag {
                    zettel_id: tag.zettel_id,
                    tag: tag.tag,
                }),
        );
        tags.sort_by(|left, right| {
            left.zettel_id
                .cmp(&right.zettel_id)
                .then_with(|| left.tag.cmp(&right.tag))
        });
        tags.dedup();

        Ok(tags)
    }

    fn query_properties(&self) -> ZorgResult<Vec<QueryProperty>> {
        self.list_properties().map(|properties| {
            properties
                .into_iter()
                .map(|property| QueryProperty {
                    zettel_id: property.zettel_id,
                    key: property.key,
                    value: property.value,
                })
                .collect()
        })
    }

    fn query_todos(&self) -> ZorgResult<Vec<QueryTodo>> {
        self.list_todos().map(|todos| {
            todos
                .into_iter()
                .map(|todo| QueryTodo {
                    zettel_id: todo.zettel_id,
                    marker: todo.marker,
                })
                .collect()
        })
    }

    fn query_links(&self) -> ZorgResult<Vec<QueryLink>> {
        self.list_links().map(|links| {
            links
                .into_iter()
                .map(|link| QueryLink {
                    source_zettel_id: link.source_zettel_id,
                    target_zettel_id: link.target_zettel_id,
                    target_canonical_id: link.target_canonical_id,
                    target_text: link.target_text,
                    resolved: link.resolved,
                })
                .collect()
        })
    }
}

/// Parser failure with one or more display-ready diagnostics.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryError {
    /// Query parser diagnostics.
    pub diagnostics: Vec<QueryDiagnostic>,
}

impl QueryError {
    fn single(diagnostic: QueryDiagnostic) -> Self {
        Self {
            diagnostics: vec![diagnostic],
        }
    }
}

impl fmt::Display for QueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(diagnostic) = self.diagnostics.first() else {
            return formatter.write_str("query parse failed");
        };

        let code = diagnostic.code.as_deref().unwrap_or("query.parse");
        write!(
            formatter,
            "{code} at byte {}: {}",
            diagnostic.span.start_byte, diagnostic.message
        )
    }
}

impl Error for QueryError {}

/// Query parser diagnostic.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryDiagnostic {
    /// Stable diagnostic category.
    pub category: DiagnosticCategory,
    /// Stable diagnostic code.
    pub code: Option<String>,
    /// Human-readable message.
    pub message: String,
    /// Source span in the query string.
    pub span: SourceSpan,
}

impl QueryDiagnostic {
    fn syntax(source: &str, start: usize, end: usize, message: impl Into<String>) -> Self {
        Self {
            category: DiagnosticCategory::Syntax,
            code: Some("query.syntax".to_owned()),
            message: message.into(),
            span: SourceSpan::from_offsets(source, start, end),
        }
    }

    fn unsupported(source: &str, start: usize, end: usize, message: impl Into<String>) -> Self {
        Self {
            category: DiagnosticCategory::Unsupported,
            code: Some("query.unsupported".to_owned()),
            message: message.into(),
            span: SourceSpan::from_offsets(source, start, end),
        }
    }

    fn semantic(message: impl Into<String>) -> Self {
        Self {
            category: DiagnosticCategory::Semantic,
            code: Some("query.semantic".to_owned()),
            message: message.into(),
            span: SourceSpan::bytes(0, 0),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct Token {
    text: String,
    start: usize,
    end: usize,
    started_quoted: bool,
    had_quotes: bool,
}

/// Parses a SWOG LIST query string into a stable AST.
///
/// Whitespace between filters is implicit logical AND. Evaluation is implemented
/// in later phases; this function only validates syntax and builds the query
/// contract used by those phases.
pub fn parse_query(source: &str) -> Result<Query, QueryError> {
    let tokens = lex_query(source)?;
    let mut filters = Vec::with_capacity(tokens.len());

    for token in tokens {
        filters.push(parse_filter(source, &token)?);
    }

    if filters.is_empty() {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            0,
            source.len(),
            "query must contain at least one filter",
        )));
    }

    Ok(Query { filters })
}

/// Normalizes a parsed query into typed filters and deterministic order keys.
pub fn normalize_query(
    query: &Query,
    context: &QueryContext,
) -> Result<NormalizedQuery, QueryError> {
    let filters = query
        .filters
        .iter()
        .map(|filter| normalize_filter(filter, context))
        .collect::<Result<Vec<_>, _>>()?;
    let default_order = default_order_for_filters(&filters);

    Ok(NormalizedQuery {
        filters,
        default_order,
    })
}

/// Parses and normalizes a query into a deterministic evaluation plan.
pub fn plan_query(query: &str, context: &QueryContext) -> Result<QueryPlan, QueryError> {
    let normalized = normalize_query(&parse_query(query)?, context)?;

    Ok(QueryPlan {
        filters: normalized.filters,
        default_order: normalized.default_order,
    })
}

/// Loads all query-facing rows through the store adapter.
pub fn load_query_snapshot(store: &impl QueryStore) -> ZorgResult<QueryStoreSnapshot> {
    Ok(QueryStoreSnapshot {
        files: store.query_files()?,
        zettel: store.query_zettel()?,
        effective_tags: store.query_effective_tags()?,
        properties: store.query_properties()?,
        todos: store.query_todos()?,
        links: store.query_links()?,
    })
}

/// Parses, plans, loads store rows, and evaluates a SWOG LIST query.
pub fn execute_list_query(
    store: &impl QueryStore,
    context: &QueryContext,
    query: &str,
) -> Result<Vec<QueryResultRow>, QueryExecutionError> {
    let plan = plan_query(query, context).map_err(QueryExecutionError::Parse)?;
    let snapshot = load_query_snapshot(store).map_err(QueryExecutionError::Store)?;
    evaluate_query_plan(&snapshot, &plan, context).map_err(QueryExecutionError::Evaluation)
}

/// Parses, evaluates, and renders a SWOG LIST query in the stable text format.
pub fn execute_and_render_list_query(
    store: &impl QueryStore,
    context: &QueryContext,
    query: &str,
) -> Result<String, QueryExecutionError> {
    let rows = execute_list_query(store, context, query)?;
    Ok(render_list_results(&rows))
}

/// Converts internal query result rows into structured LIST rows.
#[must_use]
pub fn list_rows_from_query_results(rows: &[QueryResultRow]) -> Vec<ListRow> {
    rows.iter().map(ListRow::from_query_result).collect()
}

/// Renders internal query result rows in the stable LIST text format.
#[must_use]
pub fn render_list_results(rows: &[QueryResultRow]) -> String {
    render_list_rows(&list_rows_from_query_results(rows))
}

/// Renders structured LIST rows in the stable text format.
///
/// Empty result sets render as an empty string.
#[must_use]
pub fn render_list_rows(rows: &[ListRow]) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let rendered = rows
        .iter()
        .map(RenderedListRow::from_list_row)
        .collect::<Vec<_>>();
    let id_width = rendered
        .iter()
        .map(|row| row.identity.len())
        .max()
        .unwrap_or(0);
    let path_width = rendered
        .iter()
        .map(|row| row.file_path.len())
        .max()
        .unwrap_or(0);

    rendered
        .iter()
        .map(|row| {
            let mut line = format!(
                "{:<3} {:<id_width$}  {:<path_width$}",
                row.todo_marker, row.identity, row.file_path
            );
            if !row.title.is_empty() {
                line.push_str("  ");
                line.push_str(&row.title);
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Evaluates a planned SWOG LIST query over query-facing store rows.
pub fn evaluate_query_plan(
    snapshot: &QueryStoreSnapshot,
    plan: &QueryPlan,
    context: &QueryContext,
) -> Result<Vec<QueryResultRow>, QueryEvaluationError> {
    let index = SnapshotIndex::new(snapshot);
    let mut rows = Vec::new();

    for zettel in &snapshot.zettel {
        if plan
            .filters
            .iter()
            .all(|filter| filter_matches(filter, zettel.id, &index, context))
        {
            rows.push(result_row(zettel, &index)?);
        }
    }

    rows.sort_by(|left, right| compare_result_rows(left, right, &plan.default_order));
    Ok(rows)
}

/// Returns true when any stored property value satisfies the normalized filter.
#[must_use]
pub fn property_filter_matches_values(
    filter: &NormalizedPropertyFilter,
    stored_values: &[&str],
) -> bool {
    if filter.op == ComparisonOp::Exists {
        return !stored_values.is_empty();
    }

    stored_values
        .iter()
        .any(|stored_value| property_filter_matches_value(filter, stored_value))
}

/// Returns true when one stored property value satisfies the normalized filter.
#[must_use]
pub fn property_filter_matches_value(
    filter: &NormalizedPropertyFilter,
    stored_value: &str,
) -> bool {
    let Some(value) = &filter.value else {
        return filter.op == ComparisonOp::Exists;
    };

    match (filter.semantics, value) {
        (PropertySemantics::StringEquality, ComparisonLiteral::String(expected)) => {
            filter.op == ComparisonOp::Equals && slash_list_equals(stored_value, expected)
        }
        (PropertySemantics::Number, ComparisonLiteral::Number(expected)) => {
            if stored_value.contains('/') {
                return false;
            }
            parse_number(stored_value)
                .is_some_and(|actual| compare_order(actual, *expected, filter.op))
        }
        (PropertySemantics::Date, ComparisonLiteral::Date(expected)) => parse_date(stored_value)
            .is_some_and(|actual| compare_order(actual, *expected, filter.op)),
        (PropertySemantics::Time, ComparisonLiteral::Time(expected)) => parse_time(stored_value)
            .is_some_and(|actual| compare_order(actual, *expected, filter.op)),
        _ => false,
    }
}

/// Returns true when a file modification timestamp satisfies a `modified` filter.
#[must_use]
pub fn modified_filter_matches(
    filter: &NormalizedSpecialFilter,
    mtime_unix_ms: Option<i64>,
    context: &QueryContext,
) -> bool {
    if filter.field != NormalizedSpecialField::Modified {
        return false;
    }
    let (Some(mtime_unix_ms), ComparisonLiteral::RelativeDays(days)) =
        (mtime_unix_ms, &filter.value)
    else {
        return false;
    };
    let age_ms = context.now_unix_ms.saturating_sub(mtime_unix_ms);
    let threshold_ms = i64::from(*days).saturating_mul(24 * 60 * 60 * 1000);

    compare_order(age_ms, threshold_ms, filter.op)
}

/// Evaluates a SWOG LIST query.
///
/// Parsing is available in this phase, but store-backed evaluation remains
/// deferred until later Epic 5 work.
pub fn run_list_query(query: &str) -> ZorgResult<Vec<ListRow>> {
    parse_query(query).map_err(|error| ZorgError::OperationFailed {
        message: error.to_string(),
    })?;

    Err(ZorgError::Unsupported(
        "store-backed query evaluation requires execute_list_query",
    ))
}

impl ListRow {
    /// Creates a structured LIST row from an evaluated query result row.
    #[must_use]
    pub fn from_query_result(row: &QueryResultRow) -> Self {
        Self {
            canonical_id: row.canonical_id.clone(),
            file_path: row.file_path.clone(),
            title: row.title.clone(),
            todo_marker: row.todo_marker.clone(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct RenderedListRow {
    todo_marker: String,
    identity: String,
    file_path: String,
    title: String,
}

impl RenderedListRow {
    fn from_list_row(row: &ListRow) -> Self {
        Self {
            todo_marker: row.todo_marker.clone().unwrap_or_else(|| "   ".to_owned()),
            identity: row
                .canonical_id
                .as_deref()
                .map(|id| format!("@{id}"))
                .unwrap_or_else(|| "-".to_owned()),
            file_path: row.file_path.to_string_lossy().replace('\\', "/"),
            title: normalize_rendered_cell(&row.title),
        }
    }
}

fn normalize_rendered_cell(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug)]
struct SnapshotIndex<'a> {
    files_by_id: BTreeMap<i64, &'a QueryFile>,
    zettel_by_id: BTreeMap<i64, &'a QueryZettel>,
    properties_by_zettel: BTreeMap<i64, Vec<&'a QueryProperty>>,
    effective_tags_by_zettel: BTreeMap<i64, Vec<&'a QueryEffectiveTag>>,
    todos_by_zettel: BTreeMap<i64, Vec<&'a QueryTodo>>,
    links_by_source: BTreeMap<i64, Vec<&'a QueryLink>>,
}

impl<'a> SnapshotIndex<'a> {
    fn new(snapshot: &'a QueryStoreSnapshot) -> Self {
        let files_by_id = snapshot
            .files
            .iter()
            .map(|file| (file.id, file))
            .collect::<BTreeMap<_, _>>();
        let zettel_by_id = snapshot
            .zettel
            .iter()
            .map(|zettel| (zettel.id, zettel))
            .collect::<BTreeMap<_, _>>();
        let mut properties_by_zettel: BTreeMap<i64, Vec<&QueryProperty>> = BTreeMap::new();
        let mut effective_tags_by_zettel: BTreeMap<i64, Vec<&QueryEffectiveTag>> = BTreeMap::new();
        let mut todos_by_zettel: BTreeMap<i64, Vec<&QueryTodo>> = BTreeMap::new();
        let mut links_by_source: BTreeMap<i64, Vec<&QueryLink>> = BTreeMap::new();

        for property in &snapshot.properties {
            properties_by_zettel
                .entry(property.zettel_id)
                .or_default()
                .push(property);
        }
        for tag in &snapshot.effective_tags {
            effective_tags_by_zettel
                .entry(tag.zettel_id)
                .or_default()
                .push(tag);
        }
        for todo in &snapshot.todos {
            todos_by_zettel
                .entry(todo.zettel_id)
                .or_default()
                .push(todo);
        }
        for link in &snapshot.links {
            links_by_source
                .entry(link.source_zettel_id)
                .or_default()
                .push(link);
        }

        Self {
            files_by_id,
            zettel_by_id,
            properties_by_zettel,
            effective_tags_by_zettel,
            todos_by_zettel,
            links_by_source,
        }
    }
}

fn filter_matches(
    filter: &NormalizedFilter,
    zettel_id: i64,
    index: &SnapshotIndex<'_>,
    context: &QueryContext,
) -> bool {
    match filter {
        NormalizedFilter::Property(filter) => property_matches(filter, zettel_id, index),
        NormalizedFilter::Special(filter) => special_matches(filter, zettel_id, index, context),
        NormalizedFilter::EffectiveTag(filter) => index
            .effective_tags_by_zettel
            .get(&zettel_id)
            .is_some_and(|tags| tags.iter().any(|tag| tag.tag == filter.tag)),
        NormalizedFilter::Text(filter) => text_matches(filter, zettel_id, index),
        NormalizedFilter::Negated(filter) => !filter_matches(filter, zettel_id, index, context),
    }
}

fn property_matches(
    filter: &NormalizedPropertyFilter,
    zettel_id: i64,
    index: &SnapshotIndex<'_>,
) -> bool {
    let values = index
        .properties_by_zettel
        .get(&zettel_id)
        .map(|properties| {
            properties
                .iter()
                .filter(|property| property.key == filter.key)
                .map(|property| property.value.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    property_filter_matches_values(filter, &values)
}

fn special_matches(
    filter: &NormalizedSpecialFilter,
    zettel_id: i64,
    index: &SnapshotIndex<'_>,
    context: &QueryContext,
) -> bool {
    match filter.field {
        NormalizedSpecialField::Links => link_filter_matches(filter, zettel_id, index),
        NormalizedSpecialField::File => file_filter_matches(filter, zettel_id, index),
        NormalizedSpecialField::Todo => todo_filter_matches(filter, zettel_id, index),
        NormalizedSpecialField::Modified => modified_filter_matches(
            filter,
            file_for_zettel(zettel_id, index).and_then(|file| file.mtime_unix_ms),
            context,
        ),
    }
}

fn link_filter_matches(
    filter: &NormalizedSpecialFilter,
    zettel_id: i64,
    index: &SnapshotIndex<'_>,
) -> bool {
    let ComparisonLiteral::String(expected) = &filter.value else {
        return false;
    };
    let Some(expected_canonical) = expected.strip_prefix('#') else {
        return false;
    };

    filter.op == ComparisonOp::Equals
        && index.links_by_source.get(&zettel_id).is_some_and(|links| {
            links.iter().any(|link| {
                if link.target_text == *expected {
                    return true;
                }
                link.resolved
                    && link
                        .target_canonical_id
                        .as_deref()
                        .is_some_and(|target| target == expected_canonical)
            })
        })
}

fn file_filter_matches(
    filter: &NormalizedSpecialFilter,
    zettel_id: i64,
    index: &SnapshotIndex<'_>,
) -> bool {
    let ComparisonLiteral::String(pattern) = &filter.value else {
        return false;
    };
    let Some(file) = file_for_zettel(zettel_id, index) else {
        return false;
    };
    let path = file.relative_path.to_string_lossy().replace('\\', "/");

    filter.op == ComparisonOp::Equals && glob_matches(pattern, &path)
}

fn todo_filter_matches(
    filter: &NormalizedSpecialFilter,
    zettel_id: i64,
    index: &SnapshotIndex<'_>,
) -> bool {
    let ComparisonLiteral::String(expected) = &filter.value else {
        return false;
    };

    filter.op == ComparisonOp::Equals
        && index
            .todos_by_zettel
            .get(&zettel_id)
            .is_some_and(|todos| todos.iter().any(|todo| todo.marker == *expected))
}

fn text_matches(filter: &NormalizedTextFilter, zettel_id: i64, index: &SnapshotIndex<'_>) -> bool {
    let Some(zettel) = index_zettel(zettel_id, index) else {
        return false;
    };
    let mut haystack = String::new();
    if let Some(title) = &zettel.title {
        haystack.push_str(title);
        haystack.push('\n');
    }
    haystack.push_str(&zettel.body_text);

    contains_phrase(&haystack, &filter.phrase)
}

fn result_row(
    zettel: &QueryZettel,
    index: &SnapshotIndex<'_>,
) -> Result<QueryResultRow, QueryEvaluationError> {
    let Some(file) = index.files_by_id.get(&zettel.file_id) else {
        return Err(QueryEvaluationError::MissingFile {
            zettel_id: zettel.id,
            file_id: zettel.file_id,
        });
    };

    Ok(QueryResultRow {
        zettel_store_id: zettel.id,
        canonical_id: zettel.canonical_id.clone(),
        file_path: file.relative_path.clone(),
        title: title_or_first_body_line(zettel),
        todo_marker: index
            .todos_by_zettel
            .get(&zettel.id)
            .and_then(|todos| todos.first())
            .map(|todo| todo.marker.clone()),
        source_order: zettel.source_order,
        lifecycle_date: lifecycle_date(zettel.id, index),
    })
}

fn compare_result_rows(
    left: &QueryResultRow,
    right: &QueryResultRow,
    order: &[DefaultOrderKey],
) -> std::cmp::Ordering {
    for key in order {
        let ordering = match key {
            DefaultOrderKey::LifecycleDate => {
                compare_optional_dates(left.lifecycle_date.as_ref(), right.lifecycle_date.as_ref())
            }
            DefaultOrderKey::SourcePath => left.file_path.cmp(&right.file_path),
            DefaultOrderKey::SourceOrder => left.source_order.cmp(&right.source_order),
            DefaultOrderKey::StoreId => left.zettel_store_id.cmp(&right.zettel_store_id),
        };
        if !ordering.is_eq() {
            return ordering;
        }
    }

    std::cmp::Ordering::Equal
}

fn compare_optional_dates(
    left: Option<&QueryDate>,
    right: Option<&QueryDate>,
) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn lifecycle_date(zettel_id: i64, index: &SnapshotIndex<'_>) -> Option<QueryDate> {
    index
        .properties_by_zettel
        .get(&zettel_id)?
        .iter()
        .filter(|property| matches!(property.key.as_str(), "due" | "do"))
        .filter_map(|property| parse_date(&property.value))
        .min()
}

fn title_or_first_body_line(zettel: &QueryZettel) -> String {
    zettel
        .title
        .clone()
        .or_else(|| {
            zettel
                .body_text
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

fn file_for_zettel<'a>(zettel_id: i64, index: &SnapshotIndex<'a>) -> Option<&'a QueryFile> {
    let zettel = index_zettel(zettel_id, index)?;
    index.files_by_id.get(&zettel.file_id).copied()
}

fn index_zettel<'a>(zettel_id: i64, index: &SnapshotIndex<'a>) -> Option<&'a QueryZettel> {
    index.zettel_by_id.get(&zettel_id).copied()
}

fn contains_phrase(haystack: &str, phrase: &str) -> bool {
    haystack.to_lowercase().contains(&phrase.to_lowercase())
}

fn glob_matches(pattern: &str, text: &str) -> bool {
    glob_matches_bytes(pattern.as_bytes(), text.as_bytes())
}

fn glob_matches_bytes(pattern: &[u8], text: &[u8]) -> bool {
    let mut pattern_index = 0;
    let mut text_index = 0;
    let mut star_index = None;
    let mut star_text_index = 0;

    while text_index < text.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == text[text_index])
        {
            pattern_index += 1;
            text_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            star_text_index = text_index;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            star_text_index += 1;
            text_index = star_text_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
}

fn normalize_filter(
    filter: &Filter,
    context: &QueryContext,
) -> Result<NormalizedFilter, QueryError> {
    match filter {
        Filter::Property(property) => {
            normalize_property_filter(property, context).map(NormalizedFilter::Property)
        }
        Filter::SpecialField(special) => {
            normalize_special_filter(special).map(NormalizedFilter::Special)
        }
        Filter::Tag(tag) => Ok(NormalizedFilter::EffectiveTag(NormalizedTagFilter {
            tag: tag.tag.clone(),
        })),
        Filter::Negated(negated) => Ok(NormalizedFilter::Negated(Box::new(normalize_filter(
            &negated.filter,
            context,
        )?))),
        Filter::Text(text) => Ok(NormalizedFilter::Text(NormalizedTextFilter {
            phrase: text.phrase.clone(),
            explicit: text.explicit,
        })),
    }
}

fn normalize_property_filter(
    filter: &PropertyFilter,
    context: &QueryContext,
) -> Result<NormalizedPropertyFilter, QueryError> {
    if filter.op == ComparisonOp::Exists {
        return Ok(NormalizedPropertyFilter {
            key: filter.key.clone(),
            op: filter.op,
            value: None,
            semantics: PropertySemantics::Exists,
        });
    }

    let Some(value) = &filter.value else {
        return Err(QueryError::single(QueryDiagnostic::semantic(format!(
            "property filter `{}` is missing a comparison value",
            filter.key
        ))));
    };

    let (literal, semantics) = if is_lifecycle_date_key(&filter.key) {
        (
            ComparisonLiteral::Date(parse_date_literal(value, context)?),
            PropertySemantics::Date,
        )
    } else if is_time_key(&filter.key) {
        (
            ComparisonLiteral::Time(parse_time(value).ok_or_else(|| {
                QueryError::single(QueryDiagnostic::semantic(format!(
                    "property `{}` must compare against a HH:MM or HH:MM:SS time",
                    filter.key
                )))
            })?),
            PropertySemantics::Time,
        )
    } else if filter.key == "p" || parse_number(value).is_some() {
        (
            ComparisonLiteral::Number(parse_number(value).ok_or_else(|| {
                QueryError::single(QueryDiagnostic::semantic(format!(
                    "property `{}` must compare against a numeric value",
                    filter.key
                )))
            })?),
            PropertySemantics::Number,
        )
    } else {
        if filter.op != ComparisonOp::Equals {
            return Err(QueryError::single(QueryDiagnostic::semantic(format!(
                "property `{}` supports range comparisons only for numeric, date, or time values",
                filter.key
            ))));
        }
        (
            ComparisonLiteral::String(value.clone()),
            PropertySemantics::StringEquality,
        )
    };

    Ok(NormalizedPropertyFilter {
        key: filter.key.clone(),
        op: filter.op,
        value: Some(literal),
        semantics,
    })
}

fn normalize_special_filter(
    filter: &SpecialFieldFilter,
) -> Result<NormalizedSpecialFilter, QueryError> {
    let (field, value) = match filter.field {
        SpecialField::Links => (
            NormalizedSpecialField::Links,
            ComparisonLiteral::String(filter.value.clone()),
        ),
        SpecialField::File => (
            NormalizedSpecialField::File,
            ComparisonLiteral::String(filter.value.clone()),
        ),
        SpecialField::Todo => (
            NormalizedSpecialField::Todo,
            ComparisonLiteral::String(filter.value.clone()),
        ),
        SpecialField::Text => {
            return Err(QueryError::single(QueryDiagnostic::semantic(
                "`text` filters normalize as text filters, not special fields",
            )));
        }
        SpecialField::Modified => (
            NormalizedSpecialField::Modified,
            ComparisonLiteral::RelativeDays(parse_relative_days(&filter.value).ok_or_else(
                || {
                    QueryError::single(QueryDiagnostic::semantic(
                        "modified filters must compare against a relative day value",
                    ))
                },
            )?),
        ),
    };

    Ok(NormalizedSpecialFilter {
        field,
        op: filter.op,
        value,
    })
}

fn default_order_for_filters(filters: &[NormalizedFilter]) -> Vec<DefaultOrderKey> {
    let mut order = Vec::new();
    if filters.iter().any(filter_references_lifecycle_or_todo) {
        order.push(DefaultOrderKey::LifecycleDate);
    }
    order.extend([
        DefaultOrderKey::SourcePath,
        DefaultOrderKey::SourceOrder,
        DefaultOrderKey::StoreId,
    ]);
    order
}

fn filter_references_lifecycle_or_todo(filter: &NormalizedFilter) -> bool {
    match filter {
        NormalizedFilter::Property(property) => is_lifecycle_date_key(&property.key),
        NormalizedFilter::Special(special) => special.field == NormalizedSpecialField::Todo,
        NormalizedFilter::Negated(inner) => filter_references_lifecycle_or_todo(inner),
        NormalizedFilter::EffectiveTag(tag) => tag.tag == "z/todo",
        NormalizedFilter::Text(_) => false,
    }
}

fn lex_query(source: &str) -> Result<Vec<Token>, QueryError> {
    let mut tokens = Vec::new();
    let mut cursor = 0;

    while cursor < source.len() {
        cursor = skip_whitespace(source, cursor);
        if cursor >= source.len() {
            break;
        }

        let start = cursor;
        let mut text = String::new();
        let mut had_quotes = false;
        let mut started_quoted = false;
        let mut in_quote = false;

        while cursor < source.len() {
            let Some((byte, character)) = source[cursor..].char_indices().next() else {
                break;
            };
            let absolute = cursor + byte;

            if in_quote {
                match character {
                    '\\' => {
                        cursor = absolute + character.len_utf8();
                        let Some((next_offset, escaped)) = source[cursor..].char_indices().next()
                        else {
                            text.push('\\');
                            break;
                        };
                        debug_assert_eq!(next_offset, 0);
                        text.push(escaped);
                        cursor += escaped.len_utf8();
                    }
                    '"' => {
                        in_quote = false;
                        cursor = absolute + character.len_utf8();
                    }
                    _ => {
                        text.push(character);
                        cursor = absolute + character.len_utf8();
                    }
                }
            } else if character.is_whitespace() {
                break;
            } else if character == '"' {
                if text.is_empty() {
                    started_quoted = true;
                }
                had_quotes = true;
                in_quote = true;
                cursor = absolute + character.len_utf8();
            } else {
                text.push(character);
                cursor = absolute + character.len_utf8();
            }
        }

        if in_quote {
            return Err(QueryError::single(QueryDiagnostic::syntax(
                source,
                start,
                source.len(),
                "unterminated quoted string",
            )));
        }

        if text == "todo:[" || text == "-todo:[" {
            let close_cursor = skip_whitespace(source, cursor);
            if source[close_cursor..].starts_with(']') {
                text.push(' ');
                text.push(']');
                cursor = close_cursor + 1;
            }
        }

        tokens.push(Token {
            text,
            start,
            end: cursor,
            started_quoted,
            had_quotes,
        });
    }

    Ok(tokens)
}

fn skip_whitespace(source: &str, mut cursor: usize) -> usize {
    while cursor < source.len() {
        let Some(character) = source[cursor..].chars().next() else {
            break;
        };
        if !character.is_whitespace() {
            break;
        }
        cursor += character.len_utf8();
    }
    cursor
}

fn parse_filter(source: &str, token: &Token) -> Result<Filter, QueryError> {
    if token.text.is_empty() {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            "empty quoted filters are not supported",
        )));
    }

    if let Some(inner) = token.text.strip_prefix('-') {
        if inner.is_empty() {
            return Err(QueryError::single(QueryDiagnostic::syntax(
                source,
                token.start,
                token.end,
                "negation must be followed by a filter",
            )));
        }
        if inner.starts_with('-') {
            return Err(QueryError::single(QueryDiagnostic::syntax(
                source,
                token.start,
                token.end,
                "double negation is not part of SWOG MVP syntax",
            )));
        }

        let inner_token = Token {
            text: inner.to_owned(),
            start: token.start + 1,
            end: token.end,
            started_quoted: token.started_quoted,
            had_quotes: token.had_quotes,
        };
        let filter = parse_positive_filter(source, &inner_token)?;
        return Ok(Filter::Negated(NegatedFilter {
            filter: Box::new(filter),
        }));
    }

    parse_positive_filter(source, token)
}

fn parse_positive_filter(source: &str, token: &Token) -> Result<Filter, QueryError> {
    reject_unsupported(source, token)?;

    if let Some(tag) = token.text.strip_prefix('#') {
        validate_slash_path(source, token, tag, "tag")?;
        return Ok(Filter::Tag(TagFilter {
            tag: tag.to_owned(),
        }));
    }

    if token.started_quoted && token.had_quotes && !token.text.contains(':') {
        return Ok(Filter::Text(TextFilter {
            phrase: token.text.clone(),
            explicit: false,
        }));
    }

    let Some((key, value)) = token.text.split_once(':') else {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            "expected a tag, quoted text phrase, or key:value filter",
        )));
    };

    validate_key(source, token, key)?;
    if value.is_empty() {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            format!("filter `{key}` must include a value"),
        )));
    }

    match key {
        "links" => parse_links_filter(source, token, value),
        "file" => Ok(Filter::SpecialField(SpecialFieldFilter {
            field: SpecialField::File,
            op: ComparisonOp::Equals,
            value: value.to_owned(),
        })),
        "todo" => parse_todo_filter(source, token, value),
        "text" => Ok(Filter::Text(TextFilter {
            phrase: value.to_owned(),
            explicit: true,
        })),
        "modified" => parse_modified_filter(source, token, value),
        _ => parse_property_filter(source, token, key, value),
    }
}

fn parse_links_filter(source: &str, token: &Token, value: &str) -> Result<Filter, QueryError> {
    let Some(target) = value.strip_prefix('#') else {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            "links filters must use a #target/path value",
        )));
    };
    validate_slash_path(source, token, target, "link target")?;

    Ok(Filter::SpecialField(SpecialFieldFilter {
        field: SpecialField::Links,
        op: ComparisonOp::Equals,
        value: value.to_owned(),
    }))
}

fn parse_todo_filter(source: &str, token: &Token, value: &str) -> Result<Filter, QueryError> {
    match value {
        "[ ]" | "[N]" | "[X]" | "[?]" => Ok(Filter::SpecialField(SpecialFieldFilter {
            field: SpecialField::Todo,
            op: ComparisonOp::Equals,
            value: value.to_owned(),
        })),
        _ => Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            "todo filters must be one of todo:[ ], todo:[N], todo:[X], or todo:[?]",
        ))),
    }
}

fn parse_modified_filter(source: &str, token: &Token, value: &str) -> Result<Filter, QueryError> {
    let (op, relative_age) = parse_comparison_value(value);
    if op == ComparisonOp::Equals || op == ComparisonOp::Exists {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            "modified filters must use a range comparison such as modified:<7d",
        )));
    }
    if !is_relative_day_value(relative_age) {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            "modified filters must compare against a relative day value such as 7d or 30d",
        )));
    }

    Ok(Filter::SpecialField(SpecialFieldFilter {
        field: SpecialField::Modified,
        op,
        value: relative_age.to_owned(),
    }))
}

fn parse_property_filter(
    source: &str,
    token: &Token,
    key: &str,
    value: &str,
) -> Result<Filter, QueryError> {
    let (op, parsed_value) = parse_comparison_value(value);

    if op == ComparisonOp::Exists {
        return Ok(Filter::Property(PropertyFilter {
            key: key.to_owned(),
            op,
            value: None,
        }));
    }

    if parsed_value.is_empty() {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            format!("property filter `{key}` must include a comparison value"),
        )));
    }

    Ok(Filter::Property(PropertyFilter {
        key: key.to_owned(),
        op,
        value: Some(parsed_value.to_owned()),
    }))
}

fn parse_comparison_value(value: &str) -> (ComparisonOp, &str) {
    if value == "*" {
        return (ComparisonOp::Exists, "");
    }
    for (prefix, op) in [
        ("<=", ComparisonOp::LessThanOrEqual),
        (">=", ComparisonOp::GreaterThanOrEqual),
        ("<", ComparisonOp::LessThan),
        (">", ComparisonOp::GreaterThan),
    ] {
        if let Some(value) = value.strip_prefix(prefix) {
            return (op, value);
        }
    }
    (ComparisonOp::Equals, value)
}

fn reject_unsupported(source: &str, token: &Token) -> Result<(), QueryError> {
    let text = token.text.as_str();
    let lower = text.to_ascii_lowercase();

    if text.eq_ignore_ascii_case("TABLE") {
        return Err(QueryError::single(QueryDiagnostic::unsupported(
            source,
            token.start,
            token.end,
            "TABLE output is not supported by the SWOG MVP; LIST is the only v1 output",
        )));
    }

    if text.eq_ignore_ascii_case("OR") || text == "|" || text == "||" {
        return Err(QueryError::single(QueryDiagnostic::unsupported(
            source,
            token.start,
            token.end,
            "OR expressions are not supported by the SWOG MVP; whitespace means AND",
        )));
    }

    if lower.contains("count(") || lower == "count()" {
        return Err(QueryError::single(QueryDiagnostic::unsupported(
            source,
            token.start,
            token.end,
            "count() aggregation is not supported by the SWOG MVP",
        )));
    }

    if ["sum(", "avg(", "min(", "max("]
        .iter()
        .any(|prefix| lower.contains(prefix))
    {
        return Err(QueryError::single(QueryDiagnostic::unsupported(
            source,
            token.start,
            token.end,
            "aggregation functions are not supported by the SWOG MVP",
        )));
    }

    if text.contains('(') || text.contains(')') {
        return Err(QueryError::single(QueryDiagnostic::unsupported(
            source,
            token.start,
            token.end,
            "parenthesized groups and functions are not supported by the SWOG MVP",
        )));
    }

    Ok(())
}

fn validate_key(source: &str, token: &Token, key: &str) -> Result<(), QueryError> {
    let mut characters = key.chars();
    let Some(first) = characters.next() else {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            "filter key must not be empty",
        )));
    };

    if !first.is_ascii_alphabetic() {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            "filter key must start with an ASCII letter",
        )));
    }

    if !characters
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
    {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            "filter key may contain only ASCII letters, digits, `_`, or `-`",
        )));
    }

    Ok(())
}

fn validate_slash_path(
    source: &str,
    token: &Token,
    value: &str,
    label: &str,
) -> Result<(), QueryError> {
    if value.is_empty() {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            format!("{label} must not be empty"),
        )));
    }

    for segment in value.split('/') {
        let mut characters = segment.chars();
        let Some(first) = characters.next() else {
            return Err(QueryError::single(QueryDiagnostic::syntax(
                source,
                token.start,
                token.end,
                format!("{label} must not contain empty path segments"),
            )));
        };

        if !first.is_ascii_alphanumeric() {
            return Err(QueryError::single(QueryDiagnostic::syntax(
                source,
                token.start,
                token.end,
                format!("{label} segments must start with an ASCII letter or digit"),
            )));
        }

        if !characters.all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        }) {
            return Err(QueryError::single(QueryDiagnostic::syntax(
                source,
                token.start,
                token.end,
                format!("{label} segments may contain only ASCII letters, digits, `_`, or `-`"),
            )));
        }
    }

    Ok(())
}

fn is_relative_day_value(value: &str) -> bool {
    let Some(days) = value.strip_suffix('d') else {
        return false;
    };

    !days.is_empty() && days.chars().all(|character| character.is_ascii_digit())
}

fn parse_relative_days(value: &str) -> Option<u32> {
    let days = value.strip_suffix('d')?;
    if days.is_empty() {
        return None;
    }
    days.parse().ok()
}

fn is_lifecycle_date_key(key: &str) -> bool {
    matches!(key, "do" | "due" | "did")
}

fn is_time_key(key: &str) -> bool {
    matches!(key, "start" | "end")
}

fn parse_date_literal(value: &str, context: &QueryContext) -> Result<QueryDate, QueryError> {
    if value == "today" {
        return Ok(context.today);
    }
    parse_date(value).ok_or_else(|| {
        QueryError::single(QueryDiagnostic::semantic(
            "date comparisons must use YYYY-MM-DD or today",
        ))
    })
}

fn parse_date(value: &str) -> Option<QueryDate> {
    let mut parts = value.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    QueryDate::new(year, month, day)
}

fn parse_time(value: &str) -> Option<QueryTime> {
    let mut parts = value.split(':');
    let hour = parts.next()?.parse().ok()?;
    let minute = parts.next()?.parse().ok()?;
    let second = match parts.next() {
        Some(second) => second.parse().ok()?,
        None => 0,
    };
    if parts.next().is_some() {
        return None;
    }
    QueryTime::new(hour, minute, second)
}

fn parse_number(value: &str) -> Option<f64> {
    let number = value.parse::<f64>().ok()?;
    number.is_finite().then_some(number)
}

fn compare_order<T>(actual: T, expected: T, op: ComparisonOp) -> bool
where
    T: PartialOrd + PartialEq,
{
    match op {
        ComparisonOp::Equals => actual == expected,
        ComparisonOp::GreaterThan => actual > expected,
        ComparisonOp::GreaterThanOrEqual => actual >= expected,
        ComparisonOp::LessThan => actual < expected,
        ComparisonOp::LessThanOrEqual => actual <= expected,
        ComparisonOp::Exists => true,
    }
}

fn slash_list_equals(stored_value: &str, expected: &str) -> bool {
    stored_value == expected || stored_value.split('/').any(|segment| segment == expected)
}

const fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

const fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zorg_store::{Store, StoreOptions};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn parses_property_filters() {
        let query = parse_query("foo:bar p:>3 due:<=today did:*").unwrap();

        assert_eq!(
            query.filters,
            vec![
                Filter::Property(PropertyFilter {
                    key: "foo".to_owned(),
                    op: ComparisonOp::Equals,
                    value: Some("bar".to_owned()),
                }),
                Filter::Property(PropertyFilter {
                    key: "p".to_owned(),
                    op: ComparisonOp::GreaterThan,
                    value: Some("3".to_owned()),
                }),
                Filter::Property(PropertyFilter {
                    key: "due".to_owned(),
                    op: ComparisonOp::LessThanOrEqual,
                    value: Some("today".to_owned()),
                }),
                Filter::Property(PropertyFilter {
                    key: "did".to_owned(),
                    op: ComparisonOp::Exists,
                    value: None,
                }),
            ]
        );
    }

    #[test]
    fn parses_tags_links_file_todos_text_and_modified_ranges() {
        let query = parse_query(
            "#z/todo links:#foo/bar file:projects/*.z todo:\"[ ]\" text:\"alpha beta\" modified:<7d",
        )
        .unwrap();

        assert_eq!(
            query.filters,
            vec![
                Filter::Tag(TagFilter {
                    tag: "z/todo".to_owned(),
                }),
                Filter::SpecialField(SpecialFieldFilter {
                    field: SpecialField::Links,
                    op: ComparisonOp::Equals,
                    value: "#foo/bar".to_owned(),
                }),
                Filter::SpecialField(SpecialFieldFilter {
                    field: SpecialField::File,
                    op: ComparisonOp::Equals,
                    value: "projects/*.z".to_owned(),
                }),
                Filter::SpecialField(SpecialFieldFilter {
                    field: SpecialField::Todo,
                    op: ComparisonOp::Equals,
                    value: "[ ]".to_owned(),
                }),
                Filter::Text(TextFilter {
                    phrase: "alpha beta".to_owned(),
                    explicit: true,
                }),
                Filter::SpecialField(SpecialFieldFilter {
                    field: SpecialField::Modified,
                    op: ComparisonOp::LessThan,
                    value: "7d".to_owned(),
                }),
            ]
        );
    }

    #[test]
    fn parses_open_todo_marker_with_literal_space() {
        let query = parse_query("todo:[ ]").unwrap();

        assert_eq!(
            query.filters,
            vec![Filter::SpecialField(SpecialFieldFilter {
                field: SpecialField::Todo,
                op: ComparisonOp::Equals,
                value: "[ ]".to_owned(),
            })]
        );
    }

    #[test]
    fn parses_negated_filters() {
        let query = parse_query("-#z/inbox -did:*").unwrap();

        assert_eq!(
            query.filters,
            vec![
                Filter::Negated(NegatedFilter {
                    filter: Box::new(Filter::Tag(TagFilter {
                        tag: "z/inbox".to_owned(),
                    })),
                }),
                Filter::Negated(NegatedFilter {
                    filter: Box::new(Filter::Property(PropertyFilter {
                        key: "did".to_owned(),
                        op: ComparisonOp::Exists,
                        value: None,
                    })),
                }),
            ]
        );
    }

    #[test]
    fn parses_quoted_phrase_text_with_escapes() {
        let query = parse_query("\"alpha \\\"beta\\\"\"").unwrap();

        assert_eq!(
            query.filters,
            vec![Filter::Text(TextFilter {
                phrase: "alpha \"beta\"".to_owned(),
                explicit: false,
            })]
        );
    }

    #[test]
    fn rejects_unsupported_features_with_offsets() {
        let error = parse_query("#z/todo OR count()").unwrap_err();

        assert_eq!(
            error.diagnostics[0].category,
            DiagnosticCategory::Unsupported
        );
        assert_eq!(error.diagnostics[0].span.start_byte, 8);
        assert!(error.diagnostics[0].message.contains("OR"));

        let error = parse_query("TABLE #z/todo").unwrap_err();
        assert_eq!(error.diagnostics[0].span.start_byte, 0);
        assert!(error.diagnostics[0].message.contains("TABLE"));

        let error = parse_query("(#z/todo)").unwrap_err();
        assert!(error.diagnostics[0].message.contains("parenthesized"));
    }

    #[test]
    fn rejects_malformed_inputs_with_offsets() {
        let error = parse_query("#z/").unwrap_err();
        assert_eq!(error.diagnostics[0].category, DiagnosticCategory::Syntax);
        assert_eq!(error.diagnostics[0].span.start_byte, 0);

        let error = parse_query("todo:[A]").unwrap_err();
        assert!(error.diagnostics[0].message.contains("todo filters"));

        let error = parse_query("modified:7d").unwrap_err();
        assert!(error.diagnostics[0].message.contains("range comparison"));

        let error = parse_query("\"unterminated").unwrap_err();
        assert_eq!(error.diagnostics[0].span.start_byte, 0);
        assert!(error.diagnostics[0].message.contains("unterminated"));
    }

    #[test]
    fn run_list_query_keeps_evaluation_deferred_after_successful_parse() {
        let error = run_list_query("#z/todo").unwrap_err();

        assert_eq!(
            error,
            ZorgError::Unsupported("store-backed query evaluation requires execute_list_query")
        );
    }

    #[test]
    fn normalizes_each_supported_filter_family() {
        let context = fixed_context();
        let query = parse_query(
            "#z/todo links:#project/plan file:projects/*.z todo:[ ] text:\"alpha beta\" modified:<7d -did:*",
        )
        .unwrap();

        let normalized = normalize_query(&query, &context).unwrap();

        assert_eq!(
            normalized.filters,
            vec![
                NormalizedFilter::EffectiveTag(NormalizedTagFilter {
                    tag: "z/todo".to_owned(),
                }),
                NormalizedFilter::Special(NormalizedSpecialFilter {
                    field: NormalizedSpecialField::Links,
                    op: ComparisonOp::Equals,
                    value: ComparisonLiteral::String("#project/plan".to_owned()),
                }),
                NormalizedFilter::Special(NormalizedSpecialFilter {
                    field: NormalizedSpecialField::File,
                    op: ComparisonOp::Equals,
                    value: ComparisonLiteral::String("projects/*.z".to_owned()),
                }),
                NormalizedFilter::Special(NormalizedSpecialFilter {
                    field: NormalizedSpecialField::Todo,
                    op: ComparisonOp::Equals,
                    value: ComparisonLiteral::String("[ ]".to_owned()),
                }),
                NormalizedFilter::Text(NormalizedTextFilter {
                    phrase: "alpha beta".to_owned(),
                    explicit: true,
                }),
                NormalizedFilter::Special(NormalizedSpecialFilter {
                    field: NormalizedSpecialField::Modified,
                    op: ComparisonOp::LessThan,
                    value: ComparisonLiteral::RelativeDays(7),
                }),
                NormalizedFilter::Negated(Box::new(NormalizedFilter::Property(
                    NormalizedPropertyFilter {
                        key: "did".to_owned(),
                        op: ComparisonOp::Exists,
                        value: None,
                        semantics: PropertySemantics::Exists,
                    },
                ))),
            ]
        );
        assert_eq!(
            normalized.default_order,
            vec![
                DefaultOrderKey::LifecycleDate,
                DefaultOrderKey::SourcePath,
                DefaultOrderKey::SourceOrder,
                DefaultOrderKey::StoreId,
            ]
        );
    }

    #[test]
    fn normalizes_typed_property_comparisons() {
        let context = fixed_context();
        let query = parse_query("p:>3 due:<=today start:>=09:30 area:work/research").unwrap();

        let normalized = normalize_query(&query, &context).unwrap();

        assert_eq!(
            normalized.filters,
            vec![
                NormalizedFilter::Property(NormalizedPropertyFilter {
                    key: "p".to_owned(),
                    op: ComparisonOp::GreaterThan,
                    value: Some(ComparisonLiteral::Number(3.0)),
                    semantics: PropertySemantics::Number,
                }),
                NormalizedFilter::Property(NormalizedPropertyFilter {
                    key: "due".to_owned(),
                    op: ComparisonOp::LessThanOrEqual,
                    value: Some(ComparisonLiteral::Date(QueryDate::new(2026, 5, 2).unwrap())),
                    semantics: PropertySemantics::Date,
                }),
                NormalizedFilter::Property(NormalizedPropertyFilter {
                    key: "start".to_owned(),
                    op: ComparisonOp::GreaterThanOrEqual,
                    value: Some(ComparisonLiteral::Time(QueryTime::new(9, 30, 0).unwrap())),
                    semantics: PropertySemantics::Time,
                }),
                NormalizedFilter::Property(NormalizedPropertyFilter {
                    key: "area".to_owned(),
                    op: ComparisonOp::Equals,
                    value: Some(ComparisonLiteral::String("work/research".to_owned())),
                    semantics: PropertySemantics::StringEquality,
                }),
            ]
        );
    }

    #[test]
    fn rejects_untyped_range_property_comparisons() {
        let context = fixed_context();
        let query = parse_query("area:>work").unwrap();

        let error = normalize_query(&query, &context).unwrap_err();

        assert_eq!(error.diagnostics[0].category, DiagnosticCategory::Semantic);
        assert!(error.diagnostics[0].message.contains("range comparisons"));
    }

    #[test]
    fn compares_numeric_date_time_string_and_slash_list_properties() {
        let context = fixed_context();
        let normalized = normalize_query(
            &parse_query("p:>3 due:<=today start:<10:00 area:work").unwrap(),
            &context,
        )
        .unwrap();

        let NormalizedFilter::Property(priority) = &normalized.filters[0] else {
            panic!("expected property filter");
        };
        assert!(property_filter_matches_value(priority, "4"));
        assert!(!property_filter_matches_value(priority, "2"));
        assert!(!property_filter_matches_value(priority, "4/5"));

        let NormalizedFilter::Property(due) = &normalized.filters[1] else {
            panic!("expected property filter");
        };
        assert!(property_filter_matches_value(due, "2026-05-01"));
        assert!(!property_filter_matches_value(due, "2026-05-03"));

        let NormalizedFilter::Property(start) = &normalized.filters[2] else {
            panic!("expected property filter");
        };
        assert!(property_filter_matches_value(start, "09:30"));
        assert!(!property_filter_matches_value(start, "10:30"));

        let NormalizedFilter::Property(area) = &normalized.filters[3] else {
            panic!("expected property filter");
        };
        assert!(property_filter_matches_value(area, "work/research"));
        assert!(property_filter_matches_value(area, "work"));
        assert!(!property_filter_matches_value(area, "workflow"));
    }

    #[test]
    fn compares_property_existence_against_value_sets() {
        let context = fixed_context();
        let normalized = normalize_query(&parse_query("did:*").unwrap(), &context).unwrap();
        let NormalizedFilter::Property(did) = &normalized.filters[0] else {
            panic!("expected property filter");
        };

        assert!(property_filter_matches_values(did, &["2026-05-01"]));
        assert!(!property_filter_matches_values(did, &[]));
    }

    #[test]
    fn compares_relative_modified_ranges_from_context_time() {
        let context = fixed_context();
        let recent = normalize_query(&parse_query("modified:<7d").unwrap(), &context).unwrap();
        let older = normalize_query(&parse_query("modified:>=30d").unwrap(), &context).unwrap();

        let NormalizedFilter::Special(recent) = &recent.filters[0] else {
            panic!("expected modified filter");
        };
        let NormalizedFilter::Special(older) = &older.filters[0] else {
            panic!("expected modified filter");
        };

        let one_day_ago = context.now_unix_ms - 24 * 60 * 60 * 1000;
        let forty_days_ago = context.now_unix_ms - 40 * 24 * 60 * 60 * 1000;

        assert!(modified_filter_matches(recent, Some(one_day_ago), &context));
        assert!(!modified_filter_matches(
            recent,
            Some(forty_days_ago),
            &context
        ));
        assert!(modified_filter_matches(
            older,
            Some(forty_days_ago),
            &context
        ));
        assert!(!modified_filter_matches(older, Some(one_day_ago), &context));
    }

    #[test]
    fn loads_query_snapshot_through_store_adapter_trait() {
        let store = FakeStore {
            files: vec![QueryFile {
                id: 1,
                relative_path: PathBuf::from("project.z"),
                mtime_unix_ms: Some(1_775_000_000_000),
            }],
            zettel: vec![QueryZettel {
                id: 10,
                file_id: 1,
                source_order: 0,
                title: Some("Project".to_owned()),
                canonical_id: Some("project/plan".to_owned()),
                body_text: "Plan body".to_owned(),
            }],
            effective_tags: vec![QueryEffectiveTag {
                zettel_id: 10,
                tag: "z/todo".to_owned(),
            }],
            properties: vec![QueryProperty {
                zettel_id: 10,
                key: "area".to_owned(),
                value: "work/zorg".to_owned(),
            }],
            todos: vec![QueryTodo {
                zettel_id: 10,
                marker: "[ ]".to_owned(),
            }],
            links: vec![QueryLink {
                source_zettel_id: 10,
                target_zettel_id: None,
                target_canonical_id: None,
                target_text: "#other".to_owned(),
                resolved: false,
            }],
        };

        let snapshot = load_query_snapshot(&store).unwrap();

        assert_eq!(snapshot.files.len(), 1);
        assert_eq!(
            snapshot.zettel[0].canonical_id.as_deref(),
            Some("project/plan")
        );
        assert_eq!(snapshot.effective_tags[0].tag, "z/todo");
        assert_eq!(snapshot.properties[0].value, "work/zorg");
        assert_eq!(snapshot.todos[0].marker, "[ ]");
        assert_eq!(snapshot.links[0].target_text, "#other");
    }

    #[test]
    fn evaluates_sqlite_backed_store_filters() {
        let (_temp, store, context) = indexed_query_store();

        assert_ids(
            execute_list_query(&store, &context, "#z/todo").unwrap(),
            &["root/plan/task", "root/plan", "root/review"],
        );
        assert_ids(
            execute_list_query(&store, &context, "p:>3").unwrap(),
            &["root/plan"],
        );
        assert_ids(
            execute_list_query(&store, &context, "area:archive").unwrap(),
            &["root/archive"],
        );
        assert_ids(
            execute_list_query(&store, &context, "due:<=today").unwrap(),
            &[],
        );
        assert_ids(
            execute_list_query(&store, &context, "do:<=today").unwrap(),
            &["root/plan/task"],
        );
        assert_ids(
            execute_list_query(&store, &context, "file:projects/*.z todo:[ ]").unwrap(),
            &["root/plan/task"],
        );
        assert_ids(
            execute_list_query(&store, &context, "text:alpha").unwrap(),
            &["root", "root/plan/task"],
        );
        assert_ids(
            execute_list_query(&store, &context, "modified:<7d").unwrap(),
            &[
                "root",
                "root/plan",
                "root/plan/task",
                "root/review",
                "root/archive",
                "target",
            ],
        );
    }

    #[test]
    fn evaluates_links_and_negated_filters() {
        let (_temp, store, context) = indexed_query_store();

        assert_ids(
            execute_list_query(&store, &context, "links:#root/plan/task").unwrap(),
            &["root/plan"],
        );
        assert_ids(
            execute_list_query(&store, &context, "links:#missing").unwrap(),
            &["root/plan"],
        );
        assert_ids(
            execute_list_query(&store, &context, "links:#target").unwrap(),
            &["root", "root/review"],
        );
        assert_ids(
            execute_list_query(&store, &context, "file:projects/main.z -#area/archive").unwrap(),
            &["root", "root/plan", "root/plan/task", "root/review"],
        );
        assert_ids(
            execute_list_query(&store, &context, "file:projects/main.z -did:*").unwrap(),
            &["root/plan/task", "root/plan", "root", "root/review"],
        );
        assert_ids(
            execute_list_query(&store, &context, "file:projects/main.z -todo:[?]").unwrap(),
            &["root/plan/task", "root/plan", "root", "root/archive"],
        );
        assert_ids(
            execute_list_query(&store, &context, "file:projects/main.z -links:#target").unwrap(),
            &["root/plan", "root/plan/task", "root/archive"],
        );
    }

    #[test]
    fn orders_results_deterministically_across_repeated_runs() {
        let (_temp, store, context) = indexed_query_store();

        let first = execute_list_query(&store, &context, "#z/todo").unwrap();
        let second = execute_list_query(&store, &context, "#z/todo").unwrap();

        assert_eq!(first, second);
        assert_ids(first, &["root/plan/task", "root/plan", "root/review"]);
    }

    #[test]
    fn renders_structured_list_rows_with_stable_alignment() {
        let rows = vec![
            ListRow {
                canonical_id: Some("project/plan".to_owned()),
                file_path: PathBuf::from("projects/nested.z"),
                title: "Plan the next Zorg milestone.".to_owned(),
                todo_marker: Some("[ ]".to_owned()),
            },
            ListRow {
                canonical_id: None,
                file_path: PathBuf::from("minimal.z"),
                title: "Minimal fixture with a long title that remains intact.".to_owned(),
                todo_marker: None,
            },
        ];

        assert_eq!(
            render_list_rows(&rows),
            "\
[ ] @project/plan  projects/nested.z  Plan the next Zorg milestone.
    -              minimal.z          Minimal fixture with a long title that remains intact."
        );
    }

    #[test]
    fn renders_empty_result_sets_as_no_rows() {
        assert_eq!(render_list_rows(&[]), "");
        assert_eq!(render_list_results(&[]), "");
    }

    #[test]
    fn renders_fixture_query_results_as_list_output() {
        let (_temp, store, context) = indexed_query_store();

        let output = execute_and_render_list_query(&store, &context, "#z/todo").unwrap();

        assert_eq!(
            output,
            "\
[ ] @root/plan/task  projects/main.z  Write alpha implementation note.
[N] @root/plan       projects/main.z  Plan next milestone.
[?] @root/review     projects/main.z  Review target."
        );
    }

    #[test]
    fn renders_missing_ids_and_first_body_line_titles() {
        let context = fixed_context();
        let snapshot = QueryStoreSnapshot {
            files: vec![QueryFile {
                id: 1,
                relative_path: PathBuf::from("notes/body.z"),
                mtime_unix_ms: None,
            }],
            zettel: vec![QueryZettel {
                id: 10,
                file_id: 1,
                source_order: 0,
                title: None,
                canonical_id: None,
                body_text: "\n\n  First meaningful body line.\nSecond line.".to_owned(),
            }],
            effective_tags: Vec::new(),
            properties: Vec::new(),
            todos: Vec::new(),
            links: Vec::new(),
        };
        let plan = QueryPlan {
            filters: vec![NormalizedFilter::Text(NormalizedTextFilter {
                phrase: "meaningful".to_owned(),
                explicit: true,
            })],
            default_order: vec![
                DefaultOrderKey::SourcePath,
                DefaultOrderKey::SourceOrder,
                DefaultOrderKey::StoreId,
            ],
        };

        let rows = evaluate_query_plan(&snapshot, &plan, &context).unwrap();

        assert_eq!(
            render_list_results(&rows),
            "    -  notes/body.z  First meaningful body line."
        );
    }

    #[test]
    fn distinguishes_parse_store_and_evaluation_errors() {
        let context = fixed_context();
        let parse_error =
            execute_list_query(&FakeStore::default(), &context, "OR").expect_err("parse error");
        assert!(matches!(parse_error, QueryExecutionError::Parse(_)));

        let broken = FakeStore {
            zettel: vec![QueryZettel {
                id: 10,
                file_id: 999,
                source_order: 0,
                title: Some("Broken".to_owned()),
                canonical_id: Some("broken".to_owned()),
                body_text: String::new(),
            }],
            ..FakeStore::default()
        };
        let evaluation_error =
            execute_list_query(&broken, &context, "text:broken").expect_err("evaluation error");
        assert!(matches!(
            evaluation_error,
            QueryExecutionError::Evaluation(QueryEvaluationError::MissingFile {
                zettel_id: 10,
                file_id: 999,
            })
        ));
    }

    fn fixed_context() -> QueryContext {
        QueryContext::new(
            "/tmp/zorg",
            QueryDate::new(2026, 5, 2).unwrap(),
            1_777_680_000_000,
        )
    }

    #[derive(Debug, Clone, Default)]
    struct FakeStore {
        files: Vec<QueryFile>,
        zettel: Vec<QueryZettel>,
        effective_tags: Vec<QueryEffectiveTag>,
        properties: Vec<QueryProperty>,
        todos: Vec<QueryTodo>,
        links: Vec<QueryLink>,
    }

    impl QueryStore for FakeStore {
        fn query_files(&self) -> ZorgResult<Vec<QueryFile>> {
            Ok(self.files.clone())
        }

        fn query_zettel(&self) -> ZorgResult<Vec<QueryZettel>> {
            Ok(self.zettel.clone())
        }

        fn query_effective_tags(&self) -> ZorgResult<Vec<QueryEffectiveTag>> {
            Ok(self.effective_tags.clone())
        }

        fn query_properties(&self) -> ZorgResult<Vec<QueryProperty>> {
            Ok(self.properties.clone())
        }

        fn query_todos(&self) -> ZorgResult<Vec<QueryTodo>> {
            Ok(self.todos.clone())
        }

        fn query_links(&self) -> ZorgResult<Vec<QueryLink>> {
            Ok(self.links.clone())
        }
    }

    #[derive(Debug)]
    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("zorg-query-test-{}-{counter}", std::process::id()));
            if path.exists() {
                std::fs::remove_dir_all(&path).expect("clear stale temp workspace");
            }
            std::fs::create_dir_all(&path).expect("create temp workspace");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn indexed_query_store() -> (TempWorkspace, Store, QueryContext) {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        std::fs::create_dir_all(root.join("projects")).expect("create corpus");
        write_source(
            &root,
            "projects/main.z",
            "\
%%% @root #area/root area::work/zorg
Root
%%%

Root body contains Alpha phrase and links to #target.

- @root/plan #z/todo [N] due::2026-05-15 p::4 area::work/zorg Plan next milestone.
  Plan body links to +task and #missing.

  - ^task #z/todo [ ] do::2026-05-02 p::2 area::work/research Write alpha implementation note.

- @root/review #z/todo [?] start::09:30 end::10:30 area::work/review Review target.
  Review body links to #target.

- @root/archive #z/ref #area/archive [X] did::2026-05-01 area::archive Archived work.
",
        );
        write_source(
            &root,
            "target.z",
            "\
%%% @target #z/ref area::work/research
Target
%%%

Target body.
",
        );

        let db = temp.path().join("zorg.sqlite3");
        let mut store = Store::open_with_options(StoreOptions::new(&root, db).expect("options"))
            .expect("open store");
        store.reindex().expect("reindex");
        let newest_mtime = store
            .list_files()
            .expect("files")
            .iter()
            .filter_map(|file| file.mtime_unix_ms)
            .max()
            .expect("file mtime");
        let context = QueryContext::new(
            root,
            QueryDate::new(2026, 5, 2).unwrap(),
            newest_mtime + 2 * 24 * 60 * 60 * 1000,
        );

        (temp, store, context)
    }

    fn write_source(root: &Path, name: &str, source: &str) {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create source parent");
        }
        std::fs::write(path, source).expect("write source");
    }

    fn assert_ids(rows: Vec<QueryResultRow>, expected: &[&str]) {
        let actual = rows
            .iter()
            .map(|row| row.canonical_id.as_deref().unwrap_or("<missing>"))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
}
