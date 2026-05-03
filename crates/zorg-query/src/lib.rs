//! SWOG query boundary for Zorg.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use zorg_core::{
    BodyBlock, DiagnosticCategory, SourceSpan, Zettel, ZettelId, ZorgError, ZorgResult,
};

/// Parsed SWOG filter expression query.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Query {
    /// Root boolean expression.
    pub expr: QueryExpr,
}

/// Parsed SWOG query with an output kind.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OutputQuery {
    /// Requested output contract.
    pub kind: QueryResultKind,
    /// Filter expression to evaluate.
    pub query: Query,
}

/// Query output contract selected by the input form.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum QueryResultKind {
    /// Stable LIST row output.
    List,
    /// Stable TABLE output with default columns.
    Table,
    /// Stable aggregate output with named values.
    Aggregate,
}

/// Parsed SWOG boolean expression.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum QueryExpr {
    /// Single filter atom.
    Filter(Filter),
    /// Expressions joined by implicit logical AND.
    And(Vec<QueryExpr>),
    /// Expressions joined by explicit OR.
    Or(Vec<QueryExpr>),
    /// Unary negation.
    Not(Box<QueryExpr>),
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

/// Structured column metadata for TABLE rendering.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TableColumn {
    /// Stable machine-readable column key.
    pub key: &'static str,
    /// Human-readable column heading.
    pub label: &'static str,
}

/// Structured row for human-facing TABLE rendering.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TableRow {
    /// Todo marker cell.
    pub todo: String,
    /// Zettel identity cell.
    pub id: String,
    /// Root-relative file path cell.
    pub file: String,
    /// Title cell.
    pub title: String,
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
    /// Source span for the matching zettel.
    pub source_span: SourceSpan,
    /// Source order within the file.
    pub source_order: i64,
    /// Earliest lifecycle date from `due` or `do`, when indexed.
    pub lifecycle_date: Option<QueryDate>,
    /// Effective tags available for display and refinement.
    pub tags: Vec<String>,
    /// Indexed properties available for display and refinement.
    pub properties: Vec<QueryResultProperty>,
}

/// Property included with a structured query result row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryResultProperty {
    /// Property key.
    pub key: String,
    /// Property value.
    pub value: String,
}

/// Planned, normalized query ready for evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryPlan {
    /// Normalized boolean expression.
    pub expr: NormalizedExpr,
    /// Deterministic ordering selected during planning.
    pub default_order: Vec<DefaultOrderKey>,
}

/// Store-backed query execution result with output contract metadata.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryExecutionResult {
    /// Requested output contract.
    pub kind: QueryResultKind,
    /// Evaluated rows.
    pub rows: Vec<QueryResultRow>,
}

/// Store-backed query execution error.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum QueryExecutionError {
    /// Parser or normalization failed.
    Parse(QueryError),
    /// Query zettel discovery or definition extraction failed.
    Definition(Box<QueryDefinitionError>),
    /// Store adapter failed while loading rows.
    Store(ZorgError),
    /// Evaluation failed after store rows were loaded.
    Evaluation(QueryEvaluationError),
}

impl fmt::Display for QueryExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "query parse failed: {error}"),
            Self::Definition(error) => write!(formatter, "{error}"),
            Self::Store(error) => write!(formatter, "query store failed: {error}"),
            Self::Evaluation(error) => write!(formatter, "query evaluation failed: {error}"),
        }
    }
}

impl Error for QueryExecutionError {}

/// Query definition extracted from an ordinary `#z/query` zettel.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct QueryDefinition {
    /// Canonical query zettel ID without the leading `@`.
    pub zettel_id: String,
    /// Source file containing the query zettel.
    pub source_path: PathBuf,
    /// Extracted SWOG query text.
    pub query: String,
    /// Source span for the extracted query text.
    pub span: SourceSpan,
}

/// Failure while locating or extracting a query zettel definition.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum QueryDefinitionError {
    /// CLI/user input was not an absolute canonical zettel ID.
    InvalidId { input: String, message: String },
    /// No indexed zettel had the requested canonical ID.
    NotFound { zettel_id: String },
    /// The indexed zettel points at a missing indexed file row.
    MissingFile { zettel_id: String, file_id: i64 },
    /// The source file could not be read.
    SourceRead {
        zettel_id: String,
        source_path: PathBuf,
        message: String,
    },
    /// The source file could not be parsed back into the semantic model.
    SourceParse {
        zettel_id: String,
        source_path: PathBuf,
        message: String,
    },
    /// The indexed query zettel was not found when reparsing its source.
    SourceMismatch {
        zettel_id: String,
        source_path: PathBuf,
    },
    /// The target zettel does not explicitly carry `#z/query`.
    NotQueryZettel {
        zettel_id: String,
        source_path: PathBuf,
    },
    /// The query zettel has no supported query definition.
    NoDefinition {
        zettel_id: String,
        source_path: PathBuf,
    },
    /// The query zettel has more than one `query::` property.
    MultipleQueryProperties {
        zettel_id: String,
        source_path: PathBuf,
    },
    /// The query zettel has more than one fenced `swog` block.
    MultipleSwogBlocks {
        zettel_id: String,
        source_path: PathBuf,
    },
    /// The query zettel has both supported definition forms.
    AmbiguousDefinition {
        zettel_id: String,
        source_path: PathBuf,
    },
    /// The extracted query definition is not valid SWOG MVP syntax.
    QueryParse {
        zettel_id: String,
        source_path: PathBuf,
        span: SourceSpan,
        error: Box<QueryError>,
    },
}

impl fmt::Display for QueryDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId { input, message } => {
                write!(formatter, "invalid query zettel ID `{input}`: {message}")
            }
            Self::NotFound { zettel_id } => {
                write!(formatter, "query zettel @{zettel_id} not found")
            }
            Self::MissingFile { zettel_id, file_id } => write!(
                formatter,
                "query zettel @{zettel_id} references missing indexed file row {file_id}"
            ),
            Self::SourceRead {
                zettel_id,
                source_path,
                message,
            } => write!(
                formatter,
                "query zettel @{zettel_id} in {} could not be read: {message}",
                source_path.display()
            ),
            Self::SourceParse {
                zettel_id,
                source_path,
                message,
            } => write!(
                formatter,
                "query zettel @{zettel_id} in {} could not be parsed: {message}",
                source_path.display()
            ),
            Self::SourceMismatch {
                zettel_id,
                source_path,
            } => write!(
                formatter,
                "query zettel @{zettel_id} was not found after reparsing {}",
                source_path.display()
            ),
            Self::NotQueryZettel {
                zettel_id,
                source_path,
            } => write!(
                formatter,
                "query zettel @{zettel_id} in {} is not explicitly tagged #z/query",
                source_path.display()
            ),
            Self::NoDefinition {
                zettel_id,
                source_path,
            } => write!(
                formatter,
                "query zettel @{zettel_id} in {} has no query:: property or fenced swog block",
                source_path.display()
            ),
            Self::MultipleQueryProperties {
                zettel_id,
                source_path,
            } => write!(
                formatter,
                "query zettel @{zettel_id} in {} has multiple query:: properties",
                source_path.display()
            ),
            Self::MultipleSwogBlocks {
                zettel_id,
                source_path,
            } => write!(
                formatter,
                "query zettel @{zettel_id} in {} has multiple fenced swog blocks",
                source_path.display()
            ),
            Self::AmbiguousDefinition {
                zettel_id,
                source_path,
            } => write!(
                formatter,
                "query zettel @{zettel_id} in {} has both query:: and fenced swog definitions",
                source_path.display()
            ),
            Self::QueryParse {
                zettel_id,
                source_path,
                error,
                ..
            } => write!(
                formatter,
                "query zettel @{zettel_id} in {} contains an invalid query definition: {error}",
                source_path.display()
            ),
        }
    }
}

impl Error for QueryDefinitionError {}

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
    /// Normalized boolean expression.
    pub expr: NormalizedExpr,
    /// Default deterministic order selected for this query.
    pub default_order: Vec<DefaultOrderKey>,
}

/// Normalized SWOG boolean expression.
#[derive(Debug, Clone, PartialEq)]
pub enum NormalizedExpr {
    /// Single normalized filter atom.
    Filter(NormalizedFilter),
    /// Expressions joined by implicit logical AND.
    And(Vec<NormalizedExpr>),
    /// Expressions joined by explicit OR.
    Or(Vec<NormalizedExpr>),
    /// Unary negation.
    Not(Box<NormalizedExpr>),
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
    fn query_zettel(&self, include_body_text: bool) -> ZorgResult<Vec<QueryZettel>>;
    /// Returns materialized effective tags.
    fn query_effective_tags(&self) -> ZorgResult<Vec<QueryEffectiveTag>>;
    /// Returns indexed properties.
    fn query_properties(&self) -> ZorgResult<Vec<QueryProperty>>;
    /// Returns indexed todos.
    fn query_todos(&self) -> ZorgResult<Vec<QueryTodo>>;
    /// Returns indexed links.
    fn query_links(&self) -> ZorgResult<Vec<QueryLink>>;
    /// Returns matching zettel IDs for a normalized text filter when the store
    /// can evaluate it without loading every body into the snapshot.
    fn query_text_matches(&self, _filter: &NormalizedTextFilter) -> ZorgResult<Option<Vec<i64>>> {
        Ok(None)
    }
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
    /// Source span for this zettel.
    pub source_span: SourceSpan,
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

    fn query_zettel(&self, include_body_text: bool) -> ZorgResult<Vec<QueryZettel>> {
        self.list_zettel_for_query(include_body_text).map(|zettel| {
            zettel
                .into_iter()
                .map(|zettel| {
                    let source_span = source_span_from_stored_zettel(&zettel);
                    QueryZettel {
                        id: zettel.id,
                        file_id: zettel.file_id,
                        source_order: zettel.source_order,
                        title: zettel.title,
                        canonical_id: zettel.canonical_id,
                        body_text: zettel.body_text,
                        source_span,
                    }
                })
                .collect()
        })
    }

    fn query_text_matches(&self, filter: &NormalizedTextFilter) -> ZorgResult<Option<Vec<i64>>> {
        if !self.supports_text_search()? {
            return Err(ZorgError::OperationFailed {
                message: "SQLite FTS5 support is required for text search".to_owned(),
            });
        }

        self.search_text(&fts_query_for_text_filter(filter))
            .map(|matches| Some(matches.into_iter().map(|row| row.zettel_id).collect()))
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

fn source_span_from_stored_zettel(zettel: &zorg_store::StoredZettel) -> SourceSpan {
    SourceSpan {
        start_byte: usize::try_from(zettel.start_byte).unwrap_or(0),
        end_byte: usize::try_from(zettel.end_byte).unwrap_or(0),
        start_line: zettel
            .start_line
            .and_then(|line| usize::try_from(line).ok()),
        start_column: zettel
            .start_column
            .and_then(|column| usize::try_from(column).ok()),
        end_line: zettel.end_line.and_then(|line| usize::try_from(line).ok()),
        end_column: zettel
            .end_column
            .and_then(|column| usize::try_from(column).ok()),
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
    kind: TokenKind,
    text: String,
    start: usize,
    end: usize,
    started_quoted: bool,
    had_quotes: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum TokenKind {
    Atom,
    LeftParen,
    RightParen,
    Or,
    Not,
}

/// Parses a SWOG LIST query string into a stable AST.
///
/// Whitespace between filters is implicit logical AND. Parentheses group
/// expressions, unary negation binds tighter than AND, and OR has the lowest
/// precedence.
pub fn parse_query(source: &str) -> Result<Query, QueryError> {
    let tokens = lex_query(source)?;
    if tokens.is_empty() {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            0,
            source.len(),
            "query must contain at least one filter",
        )));
    }

    let mut parser = ExprParser::new(source, &tokens);
    let expr = parser.parse_expression()?;
    if let Some(token) = parser.peek() {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            "unexpected token after query expression",
        )));
    }

    Ok(Query { expr })
}

/// Parses a SWOG query string into an output contract and filter expression.
///
/// Plain queries are LIST queries. `TABLE <query expression>` selects the
/// minimal TABLE contract, and `count(<query expression>)` selects the
/// aggregate contract while reusing the same expression grammar.
pub fn parse_output_query(source: &str) -> Result<OutputQuery, QueryError> {
    let start = skip_whitespace(source, 0);
    if let Some(output_query) = parse_count_output_query(source, start)? {
        return Ok(output_query);
    }

    let table_end = start.saturating_add("TABLE".len());
    if source[start..]
        .get(.."TABLE".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("TABLE"))
    {
        match source[table_end..].chars().next() {
            None => {
                return Err(QueryError::single(QueryDiagnostic::syntax(
                    source,
                    start,
                    table_end,
                    "TABLE must include a query expression",
                )));
            }
            Some(character) if character.is_whitespace() => {
                let expr_start = skip_whitespace(source, table_end);
                if expr_start >= source.len() {
                    return Err(QueryError::single(QueryDiagnostic::syntax(
                        source,
                        start,
                        table_end,
                        "TABLE must include a query expression",
                    )));
                }
                reject_unsupported_table_columns(source, expr_start)?;
                let query = parse_query(&source[expr_start..]).map_err(|error| {
                    map_query_error_span(error, SourceSpan::bytes(expr_start, expr_start))
                })?;
                return Ok(OutputQuery {
                    kind: QueryResultKind::Table,
                    query,
                });
            }
            Some('(') => {
                return Err(QueryError::single(QueryDiagnostic::unsupported(
                    source,
                    start,
                    table_end + 1,
                    "TABLE custom column/function syntax is not supported; use TABLE <query expression>",
                )));
            }
            Some(_) => {}
        }
    }

    Ok(OutputQuery {
        kind: QueryResultKind::List,
        query: parse_query(source)?,
    })
}

fn parse_count_output_query(source: &str, start: usize) -> Result<Option<OutputQuery>, QueryError> {
    let count_end = start.saturating_add("count".len());
    if !source[start..]
        .get(.."count".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("count"))
    {
        return Ok(None);
    }
    if !source[count_end..].starts_with('(') {
        return Ok(None);
    }

    let open = count_end;
    let close = matching_close_paren(source, open).ok_or_else(|| {
        QueryError::single(QueryDiagnostic::syntax(
            source,
            open,
            source.len(),
            "count() must close with `)`",
        ))
    })?;
    let trailing = skip_whitespace(source, close + 1);
    if trailing < source.len() {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            trailing,
            source.len(),
            "unexpected token after count() aggregate",
        )));
    }

    let expr_start = skip_whitespace(source, open + 1);
    let expr_end = trim_end_whitespace(source, close);
    if expr_start >= expr_end {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            open,
            close + 1,
            "count() must include a query expression",
        )));
    }

    let query = parse_query(&source[expr_start..expr_end])
        .map_err(|error| map_query_error_span(error, SourceSpan::bytes(expr_start, expr_start)))?;
    Ok(Some(OutputQuery {
        kind: QueryResultKind::Aggregate,
        query,
    }))
}

fn matching_close_paren(source: &str, open: usize) -> Option<usize> {
    debug_assert!(source[open..].starts_with('('));
    let mut depth = 0usize;
    let mut cursor = open;
    let mut in_quote = false;

    while cursor < source.len() {
        let character = source[cursor..].chars().next()?;
        if in_quote {
            match character {
                '\\' => {
                    cursor += character.len_utf8();
                    if cursor < source.len() {
                        let escaped = source[cursor..].chars().next()?;
                        cursor += escaped.len_utf8();
                    }
                    continue;
                }
                '"' => in_quote = false,
                _ => {}
            }
        } else {
            match character {
                '"' => in_quote = true,
                '(' => depth += 1,
                ')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(cursor);
                    }
                }
                _ => {}
            }
        }
        cursor += character.len_utf8();
    }

    None
}

fn trim_end_whitespace(source: &str, mut end: usize) -> usize {
    while end > 0 {
        let Some(character) = source[..end].chars().next_back() else {
            break;
        };
        if !character.is_whitespace() {
            break;
        }
        end = end.saturating_sub(character.len_utf8());
    }
    end
}

fn reject_unsupported_table_columns(source: &str, expr_start: usize) -> Result<(), QueryError> {
    let mut end = expr_start;
    while end < source.len() {
        let Some(character) = source[end..].chars().next() else {
            break;
        };
        if character.is_whitespace() || matches!(character, '(' | ')' | '|') {
            break;
        }
        end += character.len_utf8();
    }

    if source[expr_start..end].contains(',') {
        return Err(QueryError::single(QueryDiagnostic::unsupported(
            source,
            expr_start,
            end,
            "TABLE custom columns are not supported; use TABLE <query expression>",
        )));
    }

    Ok(())
}

struct ExprParser<'a> {
    source: &'a str,
    tokens: &'a [Token],
    cursor: usize,
}

impl<'a> ExprParser<'a> {
    fn new(source: &'a str, tokens: &'a [Token]) -> Self {
        Self {
            source,
            tokens,
            cursor: 0,
        }
    }

    fn parse_expression(&mut self) -> Result<QueryExpr, QueryError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<QueryExpr, QueryError> {
        let mut expressions = vec![self.parse_and()?];

        while self.peek_kind() == Some(TokenKind::Or) {
            self.advance();
            expressions.push(self.parse_and()?);
        }

        Ok(if expressions.len() == 1 {
            expressions.remove(0)
        } else {
            QueryExpr::Or(expressions)
        })
    }

    fn parse_and(&mut self) -> Result<QueryExpr, QueryError> {
        let mut expressions = Vec::new();

        while self.peek_kind().is_some_and(starts_expression) {
            expressions.push(self.parse_unary()?);
        }

        if expressions.is_empty() {
            let (start, end) = self
                .peek()
                .map(|token| (token.start, token.end))
                .unwrap_or_else(|| (self.source.len(), self.source.len()));
            return Err(QueryError::single(QueryDiagnostic::syntax(
                self.source,
                start,
                end,
                "expected a filter or parenthesized expression",
            )));
        }

        Ok(if expressions.len() == 1 {
            expressions.remove(0)
        } else {
            QueryExpr::And(expressions)
        })
    }

    fn parse_unary(&mut self) -> Result<QueryExpr, QueryError> {
        if self.peek_kind() == Some(TokenKind::Not) {
            self.advance();
            return Ok(QueryExpr::Not(Box::new(self.parse_unary()?)));
        }

        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<QueryExpr, QueryError> {
        let Some(token) = self.peek() else {
            return Err(QueryError::single(QueryDiagnostic::syntax(
                self.source,
                self.source.len(),
                self.source.len(),
                "expected a filter or parenthesized expression",
            )));
        };

        match token.kind {
            TokenKind::Atom => {
                if self.next_kind() == Some(TokenKind::LeftParen) {
                    reject_function_like(self.source, token)?;
                }
                let token = self.advance().expect("peeked token should exist");
                Ok(QueryExpr::Filter(parse_positive_filter(
                    self.source,
                    token,
                )?))
            }
            TokenKind::LeftParen => {
                self.advance();
                let expression = self.parse_expression()?;
                match self.peek() {
                    Some(token) if token.kind == TokenKind::RightParen => {
                        self.advance();
                        Ok(expression)
                    }
                    Some(token) => Err(QueryError::single(QueryDiagnostic::syntax(
                        self.source,
                        token.start,
                        token.end,
                        "expected `)` to close parenthesized expression",
                    ))),
                    None => Err(QueryError::single(QueryDiagnostic::syntax(
                        self.source,
                        self.source.len(),
                        self.source.len(),
                        "expected `)` to close parenthesized expression",
                    ))),
                }
            }
            TokenKind::RightParen => Err(QueryError::single(QueryDiagnostic::syntax(
                self.source,
                token.start,
                token.end,
                "unexpected `)`",
            ))),
            TokenKind::Or => Err(QueryError::single(QueryDiagnostic::syntax(
                self.source,
                token.start,
                token.end,
                "OR must appear between expressions",
            ))),
            TokenKind::Not => unreachable!("parse_unary handles negation"),
        }
    }

    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.cursor)
    }

    fn peek_kind(&self) -> Option<TokenKind> {
        self.peek().map(|token| token.kind)
    }

    fn next_kind(&self) -> Option<TokenKind> {
        self.tokens.get(self.cursor + 1).map(|token| token.kind)
    }

    fn advance(&mut self) -> Option<&'a Token> {
        let token = self.tokens.get(self.cursor)?;
        self.cursor += 1;
        Some(token)
    }
}

fn starts_expression(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Atom | TokenKind::LeftParen | TokenKind::Not
    )
}

/// Normalizes a parsed query into typed filters and deterministic order keys.
pub fn normalize_query(
    query: &Query,
    context: &QueryContext,
) -> Result<NormalizedQuery, QueryError> {
    let expr = normalize_expr(&query.expr, context)?;
    let default_order = default_order_for_expr(&expr);

    Ok(NormalizedQuery {
        expr,
        default_order,
    })
}

/// Parses and normalizes a query into a deterministic evaluation plan.
pub fn plan_query(query: &str, context: &QueryContext) -> Result<QueryPlan, QueryError> {
    plan_parsed_query(&parse_query(query)?, context)
}

fn plan_parsed_query(query: &Query, context: &QueryContext) -> Result<QueryPlan, QueryError> {
    let normalized = normalize_query(query, context)?;

    Ok(QueryPlan {
        expr: normalized.expr,
        default_order: normalized.default_order,
    })
}

/// Loads all query-facing rows through the store adapter.
pub fn load_query_snapshot(store: &impl QueryStore) -> ZorgResult<QueryStoreSnapshot> {
    load_query_snapshot_with_body_policy(store, true)
}

fn load_query_snapshot_with_body_policy(
    store: &impl QueryStore,
    include_body_text: bool,
) -> ZorgResult<QueryStoreSnapshot> {
    Ok(QueryStoreSnapshot {
        files: store.query_files()?,
        zettel: store.query_zettel(include_body_text)?,
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
    execute_query_plan_rows(store, context, &plan)
}

fn execute_query_plan_rows(
    store: &impl QueryStore,
    context: &QueryContext,
    plan: &QueryPlan,
) -> Result<Vec<QueryResultRow>, QueryExecutionError> {
    let text_search =
        collect_store_text_search_matches(store, plan).map_err(QueryExecutionError::Store)?;
    let include_body_text = text_search.requires_snapshot_body_text();
    let snapshot = load_query_snapshot_with_body_policy(store, include_body_text)
        .map_err(QueryExecutionError::Store)?;
    evaluate_query_plan_with_text_search(&snapshot, plan, context, &text_search)
        .map_err(QueryExecutionError::Evaluation)
}

/// Parses, plans, loads store rows, and evaluates a SWOG query with its output kind.
pub fn execute_query(
    store: &impl QueryStore,
    context: &QueryContext,
    query: &str,
) -> Result<QueryExecutionResult, QueryExecutionError> {
    let output_query = parse_output_query(query).map_err(QueryExecutionError::Parse)?;
    let plan =
        plan_parsed_query(&output_query.query, context).map_err(QueryExecutionError::Parse)?;
    let rows = execute_query_plan_rows(store, context, &plan)?;
    Ok(QueryExecutionResult {
        kind: output_query.kind,
        rows,
    })
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

/// Parses, evaluates, and renders a SWOG query using its selected text renderer.
pub fn execute_and_render_query(
    store: &impl QueryStore,
    context: &QueryContext,
    query: &str,
) -> Result<String, QueryExecutionError> {
    let result = execute_query(store, context, query)?;
    Ok(match result.kind {
        QueryResultKind::List => render_list_results(&result.rows),
        QueryResultKind::Table => render_table_results(&result.rows),
        QueryResultKind::Aggregate => render_count_result(result.rows.len()),
    })
}

/// Looks up a `#z/query` zettel by canonical ID and extracts its SWOG definition.
pub fn query_definition_by_id(
    store: &zorg_store::Store,
    query_id: &str,
) -> Result<QueryDefinition, QueryExecutionError> {
    let canonical_id = parse_query_zettel_id(query_id).map_err(QueryExecutionError::Definition)?;
    let stored = store
        .lookup_zettel_by_canonical_id(&canonical_id)
        .map_err(QueryExecutionError::Store)?
        .ok_or_else(|| {
            QueryExecutionError::Definition(Box::new(QueryDefinitionError::NotFound {
                zettel_id: canonical_id.clone(),
            }))
        })?;
    let files = store.list_files().map_err(QueryExecutionError::Store)?;
    let file = files
        .iter()
        .find(|file| file.id == stored.file_id)
        .ok_or_else(|| {
            QueryExecutionError::Definition(Box::new(QueryDefinitionError::MissingFile {
                zettel_id: canonical_id.clone(),
                file_id: stored.file_id,
            }))
        })?;
    let source_path = store.root().join(&file.relative_path);
    let source = std::fs::read_to_string(&source_path).map_err(|error| {
        QueryExecutionError::Definition(Box::new(QueryDefinitionError::SourceRead {
            zettel_id: canonical_id.clone(),
            source_path: source_path.clone(),
            message: error.to_string(),
        }))
    })?;
    let mut document = zorg_parse::parse_zettel_document_with_path(&source, source_path.clone())
        .map_err(|error| {
            QueryExecutionError::Definition(Box::new(QueryDefinitionError::SourceParse {
                zettel_id: canonical_id.clone(),
                source_path: source_path.clone(),
                message: error.to_string(),
            }))
        })?;
    zorg_parse::resolve_document(&mut document);
    let zettel = find_zettel_by_canonical_id(&document.root, &canonical_id).ok_or_else(|| {
        QueryExecutionError::Definition(Box::new(QueryDefinitionError::SourceMismatch {
            zettel_id: canonical_id.clone(),
            source_path: source_path.clone(),
        }))
    })?;

    extract_query_definition(zettel, &canonical_id, &source_path)
        .map_err(QueryExecutionError::Definition)
}

/// Looks up a query zettel by canonical ID, then evaluates its stored SWOG definition.
pub fn execute_list_query_by_id(
    store: &zorg_store::Store,
    context: &QueryContext,
    query_id: &str,
) -> Result<Vec<QueryResultRow>, QueryExecutionError> {
    let definition = query_definition_by_id(store, query_id)?;
    execute_list_query(store, context, &definition.query)
}

/// Looks up a query zettel by canonical ID, then evaluates its stored SWOG definition.
pub fn execute_query_by_id(
    store: &zorg_store::Store,
    context: &QueryContext,
    query_id: &str,
) -> Result<QueryExecutionResult, QueryExecutionError> {
    let definition = query_definition_by_id(store, query_id)?;
    execute_query(store, context, &definition.query)
}

/// Looks up a query zettel by canonical ID, evaluates it, and renders LIST output.
pub fn execute_and_render_list_query_by_id(
    store: &zorg_store::Store,
    context: &QueryContext,
    query_id: &str,
) -> Result<String, QueryExecutionError> {
    let rows = execute_list_query_by_id(store, context, query_id)?;
    Ok(render_list_results(&rows))
}

/// Looks up a query zettel by canonical ID, evaluates it, and renders text output.
pub fn execute_and_render_query_by_id(
    store: &zorg_store::Store,
    context: &QueryContext,
    query_id: &str,
) -> Result<String, QueryExecutionError> {
    let result = execute_query_by_id(store, context, query_id)?;
    Ok(match result.kind {
        QueryResultKind::List => render_list_results(&result.rows),
        QueryResultKind::Table => render_table_results(&result.rows),
        QueryResultKind::Aggregate => render_count_result(result.rows.len()),
    })
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

/// Returns the default TABLE columns.
#[must_use]
pub fn table_columns() -> Vec<TableColumn> {
    vec![
        TableColumn {
            key: "todo",
            label: "Todo",
        },
        TableColumn {
            key: "id",
            label: "ID",
        },
        TableColumn {
            key: "file",
            label: "File",
        },
        TableColumn {
            key: "title",
            label: "Title",
        },
    ]
}

/// Converts internal query result rows into structured TABLE rows.
#[must_use]
pub fn table_rows_from_query_results(rows: &[QueryResultRow]) -> Vec<TableRow> {
    rows.iter().map(TableRow::from_query_result).collect()
}

/// Renders internal query result rows as a deterministic TABLE.
#[must_use]
pub fn render_table_results(rows: &[QueryResultRow]) -> String {
    render_table_rows(&table_rows_from_query_results(rows))
}

/// Renders the minimal aggregate text result.
#[must_use]
pub fn render_count_result(count: usize) -> String {
    format!("count {count}")
}

/// Renders structured TABLE rows in the stable text format.
///
/// Empty result sets render as an empty string.
#[must_use]
pub fn render_table_rows(rows: &[TableRow]) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let headers = table_columns()
        .into_iter()
        .map(|column| column.label.to_owned())
        .collect::<Vec<_>>();
    let body = rows
        .iter()
        .map(|row| {
            vec![
                normalize_rendered_cell(&row.todo),
                normalize_rendered_cell(&row.id),
                normalize_rendered_cell(&row.file),
                normalize_rendered_cell(&row.title),
            ]
        })
        .collect::<Vec<_>>();
    let mut widths = headers.iter().map(String::len).collect::<Vec<_>>();
    for row in &body {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.len());
        }
    }

    let mut lines = Vec::with_capacity(body.len() + 2);
    lines.push(render_table_line(&headers, &widths));
    lines.push(
        widths
            .iter()
            .map(|width| "-".repeat((*width).max(3)))
            .collect::<Vec<_>>()
            .join("  "),
    );
    lines.extend(body.iter().map(|row| render_table_line(row, &widths)));
    lines.join("\n")
}

fn render_table_line(cells: &[String], widths: &[usize]) -> String {
    cells
        .iter()
        .enumerate()
        .map(|(index, cell)| format!("{cell:<width$}", width = widths[index]))
        .collect::<Vec<_>>()
        .join("  ")
        .trim_end()
        .to_owned()
}

/// Evaluates a planned SWOG LIST query over query-facing store rows.
pub fn evaluate_query_plan(
    snapshot: &QueryStoreSnapshot,
    plan: &QueryPlan,
    context: &QueryContext,
) -> Result<Vec<QueryResultRow>, QueryEvaluationError> {
    evaluate_query_plan_with_text_search(snapshot, plan, context, &TextSearchMatches::default())
}

fn evaluate_query_plan_with_text_search(
    snapshot: &QueryStoreSnapshot,
    plan: &QueryPlan,
    context: &QueryContext,
    text_search: &TextSearchMatches,
) -> Result<Vec<QueryResultRow>, QueryEvaluationError> {
    let index = SnapshotIndex::new(snapshot);
    let mut rows = Vec::new();

    for zettel in &snapshot.zettel {
        if expr_matches(&plan.expr, zettel.id, &index, context, text_search) {
            rows.push(result_row(zettel, &index)?);
        }
    }

    rows.sort_by(|left, right| compare_result_rows(left, right, &plan.default_order));
    Ok(rows)
}

#[derive(Debug, Clone, Default)]
struct TextSearchMatches {
    matches: Vec<(NormalizedTextFilter, BTreeSet<i64>)>,
    has_snapshot_text_filter: bool,
}

impl TextSearchMatches {
    fn lookup(&self, filter: &NormalizedTextFilter) -> Option<&BTreeSet<i64>> {
        self.matches
            .iter()
            .find(|(candidate, _)| candidate == filter)
            .map(|(_, zettel_ids)| zettel_ids)
    }

    fn requires_snapshot_body_text(&self) -> bool {
        self.has_snapshot_text_filter
    }
}

fn collect_store_text_search_matches(
    store: &impl QueryStore,
    plan: &QueryPlan,
) -> ZorgResult<TextSearchMatches> {
    let filters = collect_text_filters(&plan.expr);
    let mut text_search = TextSearchMatches::default();

    for filter in filters {
        if text_search.lookup(filter).is_some() {
            continue;
        }
        match store.query_text_matches(filter)? {
            Some(zettel_ids) => {
                text_search
                    .matches
                    .push((filter.clone(), zettel_ids.into_iter().collect()));
            }
            None => {
                text_search.has_snapshot_text_filter = true;
            }
        }
    }

    Ok(text_search)
}

fn collect_text_filters(expr: &NormalizedExpr) -> Vec<&NormalizedTextFilter> {
    let mut text_filters = Vec::new();
    collect_text_filters_from_expr(expr, &mut text_filters);
    text_filters
}

fn collect_text_filters_from_expr<'a>(
    expr: &'a NormalizedExpr,
    text_filters: &mut Vec<&'a NormalizedTextFilter>,
) {
    match expr {
        NormalizedExpr::Filter(filter) => collect_text_filters_from_filter(filter, text_filters),
        NormalizedExpr::And(expressions) | NormalizedExpr::Or(expressions) => {
            for expr in expressions {
                collect_text_filters_from_expr(expr, text_filters);
            }
        }
        NormalizedExpr::Not(expr) => collect_text_filters_from_expr(expr, text_filters),
    }
}

fn collect_text_filters_from_filter<'a>(
    filter: &'a NormalizedFilter,
    text_filters: &mut Vec<&'a NormalizedTextFilter>,
) {
    match filter {
        NormalizedFilter::Text(filter) => text_filters.push(filter),
        NormalizedFilter::Property(_)
        | NormalizedFilter::Special(_)
        | NormalizedFilter::EffectiveTag(_) => {}
    }
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

fn parse_query_zettel_id(input: &str) -> Result<String, Box<QueryDefinitionError>> {
    ZettelId::parse(input)
        .map(|id| id.as_str().to_owned())
        .map_err(|error| {
            Box::new(QueryDefinitionError::InvalidId {
                input: input.to_owned(),
                message: error.to_string(),
            })
        })
}

fn extract_query_definition(
    zettel: &Zettel,
    canonical_id: &str,
    source_path: &Path,
) -> Result<QueryDefinition, Box<QueryDefinitionError>> {
    if !zettel
        .type_tags
        .iter()
        .any(|tag| tag.tag.as_str() == "z/query")
    {
        return Err(Box::new(QueryDefinitionError::NotQueryZettel {
            zettel_id: canonical_id.to_owned(),
            source_path: source_path.to_path_buf(),
        }));
    }

    let query_properties = zettel
        .properties
        .iter()
        .filter(|property| property.key == "query")
        .collect::<Vec<_>>();
    let swog_blocks = direct_swog_blocks(zettel);

    if query_properties.len() > 1 {
        return Err(Box::new(QueryDefinitionError::MultipleQueryProperties {
            zettel_id: canonical_id.to_owned(),
            source_path: source_path.to_path_buf(),
        }));
    }
    if swog_blocks.len() > 1 {
        return Err(Box::new(QueryDefinitionError::MultipleSwogBlocks {
            zettel_id: canonical_id.to_owned(),
            source_path: source_path.to_path_buf(),
        }));
    }
    if query_properties.len() == 1 && swog_blocks.len() == 1 {
        return Err(Box::new(QueryDefinitionError::AmbiguousDefinition {
            zettel_id: canonical_id.to_owned(),
            source_path: source_path.to_path_buf(),
        }));
    }

    let (query, span) = if let Some(property) = query_properties.first() {
        (
            property.value.clone(),
            property
                .value_span
                .or(property.span)
                .unwrap_or_else(|| zettel.span.unwrap_or(SourceSpan::bytes(0, 0))),
        )
    } else if let Some(block) = swog_blocks.first() {
        (
            block.body.clone(),
            block
                .body_span
                .or(block.span)
                .unwrap_or_else(|| zettel.span.unwrap_or(SourceSpan::bytes(0, 0))),
        )
    } else {
        return Err(Box::new(QueryDefinitionError::NoDefinition {
            zettel_id: canonical_id.to_owned(),
            source_path: source_path.to_path_buf(),
        }));
    };

    let (query, span) = trim_query_definition(query, span);
    parse_output_query(&query).map_err(|error| {
        Box::new(QueryDefinitionError::QueryParse {
            zettel_id: canonical_id.to_owned(),
            source_path: source_path.to_path_buf(),
            span,
            error: Box::new(map_query_error_span(error, span)),
        })
    })?;

    Ok(QueryDefinition {
        zettel_id: canonical_id.to_owned(),
        source_path: source_path.to_path_buf(),
        query,
        span,
    })
}

fn direct_swog_blocks(zettel: &Zettel) -> Vec<&zorg_core::FencedCodeBlock> {
    zettel
        .body
        .iter()
        .filter_map(|block| match block {
            BodyBlock::FencedCode(block) if block.info.as_deref() == Some("swog") => Some(block),
            _ => None,
        })
        .collect()
}

fn trim_query_definition(query: String, span: SourceSpan) -> (String, SourceSpan) {
    let trimmed_start = query.len() - query.trim_start().len();
    let trimmed_end = query.len() - query.trim_end().len();
    let start_byte = span.start_byte.saturating_add(trimmed_start);
    let end_byte = span.end_byte.saturating_sub(trimmed_end);

    (
        query.trim().to_owned(),
        SourceSpan::bytes(start_byte, end_byte.max(start_byte)),
    )
}

fn map_query_error_span(error: QueryError, definition_span: SourceSpan) -> QueryError {
    QueryError {
        diagnostics: error
            .diagnostics
            .into_iter()
            .map(|diagnostic| QueryDiagnostic {
                span: SourceSpan::bytes(
                    definition_span
                        .start_byte
                        .saturating_add(diagnostic.span.start_byte),
                    definition_span
                        .start_byte
                        .saturating_add(diagnostic.span.end_byte),
                ),
                ..diagnostic
            })
            .collect(),
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

impl TableRow {
    /// Creates a structured TABLE row from an evaluated query result row.
    #[must_use]
    pub fn from_query_result(row: &QueryResultRow) -> Self {
        Self {
            todo: row.todo_marker.clone().unwrap_or_default(),
            id: row
                .canonical_id
                .as_deref()
                .map(|id| format!("@{id}"))
                .unwrap_or_else(|| "-".to_owned()),
            file: row.file_path.to_string_lossy().replace('\\', "/"),
            title: row.title.clone(),
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
    text_search: &TextSearchMatches,
) -> bool {
    match filter {
        NormalizedFilter::Property(filter) => property_matches(filter, zettel_id, index),
        NormalizedFilter::Special(filter) => special_matches(filter, zettel_id, index, context),
        NormalizedFilter::EffectiveTag(filter) => index
            .effective_tags_by_zettel
            .get(&zettel_id)
            .is_some_and(|tags| tags.iter().any(|tag| tag.tag == filter.tag)),
        NormalizedFilter::Text(filter) => text_matches(filter, zettel_id, index, text_search),
    }
}

fn expr_matches(
    expr: &NormalizedExpr,
    zettel_id: i64,
    index: &SnapshotIndex<'_>,
    context: &QueryContext,
    text_search: &TextSearchMatches,
) -> bool {
    match expr {
        NormalizedExpr::Filter(filter) => {
            filter_matches(filter, zettel_id, index, context, text_search)
        }
        NormalizedExpr::And(expressions) => expressions
            .iter()
            .all(|expr| expr_matches(expr, zettel_id, index, context, text_search)),
        NormalizedExpr::Or(expressions) => expressions
            .iter()
            .any(|expr| expr_matches(expr, zettel_id, index, context, text_search)),
        NormalizedExpr::Not(expr) => !expr_matches(expr, zettel_id, index, context, text_search),
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

fn text_matches(
    filter: &NormalizedTextFilter,
    zettel_id: i64,
    index: &SnapshotIndex<'_>,
    text_search: &TextSearchMatches,
) -> bool {
    if let Some(matches) = text_search.lookup(filter) {
        return matches.contains(&zettel_id);
    }

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

fn fts_query_for_text_filter(filter: &NormalizedTextFilter) -> String {
    format!("\"{}\"", filter.phrase.replace('"', "\"\""))
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
        source_span: zettel.source_span,
        source_order: zettel.source_order,
        lifecycle_date: lifecycle_date(zettel.id, index),
        tags: index
            .effective_tags_by_zettel
            .get(&zettel.id)
            .map(|tags| tags.iter().map(|tag| tag.tag.clone()).collect())
            .unwrap_or_default(),
        properties: index
            .properties_by_zettel
            .get(&zettel.id)
            .map(|properties| {
                properties
                    .iter()
                    .map(|property| QueryResultProperty {
                        key: property.key.clone(),
                        value: property.value.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
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

fn normalize_expr(expr: &QueryExpr, context: &QueryContext) -> Result<NormalizedExpr, QueryError> {
    match expr {
        QueryExpr::Filter(filter) => normalize_filter(filter, context).map(NormalizedExpr::Filter),
        QueryExpr::And(expressions) => expressions
            .iter()
            .map(|expr| normalize_expr(expr, context))
            .collect::<Result<Vec<_>, _>>()
            .map(NormalizedExpr::And),
        QueryExpr::Or(expressions) => expressions
            .iter()
            .map(|expr| normalize_expr(expr, context))
            .collect::<Result<Vec<_>, _>>()
            .map(NormalizedExpr::Or),
        QueryExpr::Not(expr) => normalize_expr(expr, context)
            .map(Box::new)
            .map(NormalizedExpr::Not),
    }
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

fn default_order_for_expr(expr: &NormalizedExpr) -> Vec<DefaultOrderKey> {
    let mut order = Vec::new();
    if expr_references_lifecycle_or_todo(expr) {
        order.push(DefaultOrderKey::LifecycleDate);
    }
    order.extend([
        DefaultOrderKey::SourcePath,
        DefaultOrderKey::SourceOrder,
        DefaultOrderKey::StoreId,
    ]);
    order
}

fn expr_references_lifecycle_or_todo(expr: &NormalizedExpr) -> bool {
    match expr {
        NormalizedExpr::Filter(filter) => filter_references_lifecycle_or_todo(filter),
        NormalizedExpr::And(expressions) | NormalizedExpr::Or(expressions) => {
            expressions.iter().any(expr_references_lifecycle_or_todo)
        }
        NormalizedExpr::Not(expr) => expr_references_lifecycle_or_todo(expr),
    }
}

fn filter_references_lifecycle_or_todo(filter: &NormalizedFilter) -> bool {
    match filter {
        NormalizedFilter::Property(property) => is_lifecycle_date_key(&property.key),
        NormalizedFilter::Special(special) => special.field == NormalizedSpecialField::Todo,
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
        let Some(character) = source[cursor..].chars().next() else {
            break;
        };

        match character {
            '(' => {
                cursor += 1;
                tokens.push(Token {
                    kind: TokenKind::LeftParen,
                    text: "(".to_owned(),
                    start,
                    end: cursor,
                    started_quoted: false,
                    had_quotes: false,
                });
                continue;
            }
            ')' => {
                cursor += 1;
                tokens.push(Token {
                    kind: TokenKind::RightParen,
                    text: ")".to_owned(),
                    start,
                    end: cursor,
                    started_quoted: false,
                    had_quotes: false,
                });
                continue;
            }
            '-' => {
                cursor += 1;
                tokens.push(Token {
                    kind: TokenKind::Not,
                    text: "-".to_owned(),
                    start,
                    end: cursor,
                    started_quoted: false,
                    had_quotes: false,
                });
                continue;
            }
            '|' => {
                cursor += 1;
                if source[cursor..].starts_with('|') {
                    cursor += 1;
                }
                tokens.push(Token {
                    kind: TokenKind::Or,
                    text: source[start..cursor].to_owned(),
                    start,
                    end: cursor,
                    started_quoted: false,
                    had_quotes: false,
                });
                continue;
            }
            _ => {}
        }

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
            } else if character.is_whitespace() || matches!(character, '(' | ')' | '|') {
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

        if text.is_empty() {
            return Err(QueryError::single(QueryDiagnostic::syntax(
                source,
                start,
                cursor,
                "empty query token",
            )));
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

        let kind = if !started_quoted && text.eq_ignore_ascii_case("OR") {
            TokenKind::Or
        } else {
            TokenKind::Atom
        };

        tokens.push(Token {
            kind,
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

fn parse_positive_filter(source: &str, token: &Token) -> Result<Filter, QueryError> {
    reject_unsupported(source, token)?;

    if token.text.is_empty() {
        return Err(QueryError::single(QueryDiagnostic::syntax(
            source,
            token.start,
            token.end,
            "empty quoted filters are not supported",
        )));
    }

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
            "TABLE must be the output prefix: TABLE <query expression>",
        )));
    }

    if lower.contains("count(") || lower == "count()" {
        return Err(QueryError::single(QueryDiagnostic::unsupported(
            source,
            token.start,
            token.end,
            "count() must be the whole query: count(<query expression>)",
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
            "aggregation functions other than count() are not supported",
        )));
    }

    if text.contains('(') || text.contains(')') {
        return Err(QueryError::single(QueryDiagnostic::unsupported(
            source,
            token.start,
            token.end,
            "function-like syntax is not supported in query expressions",
        )));
    }

    Ok(())
}

fn reject_function_like(source: &str, token: &Token) -> Result<(), QueryError> {
    let lower = token.text.to_ascii_lowercase();
    if lower == "count" {
        return Err(QueryError::single(QueryDiagnostic::unsupported(
            source,
            token.start,
            token.end,
            "count() must be the whole query: count(<query expression>)",
        )));
    }

    if matches!(lower.as_str(), "sum" | "avg" | "min" | "max") {
        return Err(QueryError::single(QueryDiagnostic::unsupported(
            source,
            token.start,
            token.end,
            "aggregation functions other than count() are not supported",
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
            query.expr,
            query_and(vec![
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
            ])
        );
    }

    #[test]
    fn parses_tags_links_file_todos_text_and_modified_ranges() {
        let query = parse_query(
            "#z/todo links:#foo/bar file:projects/*.z todo:\"[ ]\" text:\"alpha beta\" modified:<7d",
        )
        .unwrap();

        assert_eq!(
            query.expr,
            query_and(vec![
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
            ])
        );
    }

    #[test]
    fn parses_open_todo_marker_with_literal_space() {
        let query = parse_query("todo:[ ]").unwrap();

        assert_eq!(
            query.expr,
            QueryExpr::Filter(Filter::SpecialField(SpecialFieldFilter {
                field: SpecialField::Todo,
                op: ComparisonOp::Equals,
                value: "[ ]".to_owned(),
            }))
        );
    }

    #[test]
    fn parses_negated_filters() {
        let query = parse_query("-#z/inbox -did:*").unwrap();

        assert_eq!(
            query.expr,
            QueryExpr::And(vec![
                QueryExpr::Not(Box::new(QueryExpr::Filter(Filter::Tag(TagFilter {
                    tag: "z/inbox".to_owned(),
                })))),
                QueryExpr::Not(Box::new(QueryExpr::Filter(Filter::Property(
                    PropertyFilter {
                        key: "did".to_owned(),
                        op: ComparisonOp::Exists,
                        value: None,
                    }
                )))),
            ])
        );
    }

    #[test]
    fn parses_or_and_parenthesized_boolean_expressions() {
        let query = parse_query("#z/todo (#area/work OR #area/personal) | -did:*").unwrap();

        assert_eq!(
            query.expr,
            QueryExpr::Or(vec![
                QueryExpr::And(vec![
                    QueryExpr::Filter(Filter::Tag(TagFilter {
                        tag: "z/todo".to_owned(),
                    })),
                    QueryExpr::Or(vec![
                        QueryExpr::Filter(Filter::Tag(TagFilter {
                            tag: "area/work".to_owned(),
                        })),
                        QueryExpr::Filter(Filter::Tag(TagFilter {
                            tag: "area/personal".to_owned(),
                        })),
                    ]),
                ]),
                QueryExpr::Not(Box::new(QueryExpr::Filter(Filter::Property(
                    PropertyFilter {
                        key: "did".to_owned(),
                        op: ComparisonOp::Exists,
                        value: None,
                    }
                )))),
            ])
        );
    }

    #[test]
    fn parses_quoted_phrase_text_with_escapes() {
        let query = parse_query("\"alpha \\\"beta\\\"\"").unwrap();

        assert_eq!(
            query.expr,
            QueryExpr::Filter(Filter::Text(TextFilter {
                phrase: "alpha \"beta\"".to_owned(),
                explicit: false,
            }))
        );
    }

    #[test]
    fn parses_table_output_query() {
        let query = parse_output_query("TABLE (#z/todo OR #z/query)").unwrap();

        assert_eq!(query.kind, QueryResultKind::Table);
        assert_eq!(
            query.query.expr,
            QueryExpr::Or(vec![
                QueryExpr::Filter(Filter::Tag(TagFilter {
                    tag: "z/todo".to_owned(),
                })),
                QueryExpr::Filter(Filter::Tag(TagFilter {
                    tag: "z/query".to_owned(),
                })),
            ])
        );
    }

    #[test]
    fn parses_count_output_query() {
        let query = parse_output_query("count(#z/todo OR #z/query)").unwrap();

        assert_eq!(query.kind, QueryResultKind::Aggregate);
        assert_eq!(
            query.query.expr,
            QueryExpr::Or(vec![
                QueryExpr::Filter(Filter::Tag(TagFilter {
                    tag: "z/todo".to_owned(),
                })),
                QueryExpr::Filter(Filter::Tag(TagFilter {
                    tag: "z/query".to_owned(),
                })),
            ])
        );
    }

    #[test]
    fn rejects_deferred_features_with_offsets() {
        let error = parse_output_query("TABLE todo,id #z/todo").unwrap_err();
        assert_eq!(error.diagnostics[0].span.start_byte, 6);
        assert!(error.diagnostics[0].message.contains("custom columns"));

        let error = parse_output_query("count()").unwrap_err();
        assert_eq!(error.diagnostics[0].category, DiagnosticCategory::Syntax);
        assert_eq!(error.diagnostics[0].span.start_byte, 5);
        assert!(error.diagnostics[0].message.contains("query expression"));

        let error = parse_query("#z/todo OR sum()").unwrap_err();
        assert_eq!(
            error.diagnostics[0].category,
            DiagnosticCategory::Unsupported
        );
        assert_eq!(error.diagnostics[0].span.start_byte, 11);
        assert!(error.diagnostics[0].message.contains("aggregation"));
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

        let error = parse_query("#z/todo OR").unwrap_err();
        assert!(error.diagnostics[0].message.contains("expected a filter"));

        let error = parse_query("(#z/todo OR #z/ref").unwrap_err();
        assert!(error.diagnostics[0].message.contains("expected `)`"));

        let error = parse_query("#z/todo)").unwrap_err();
        assert!(error.diagnostics[0].message.contains("unexpected token"));
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
            normalized.expr,
            NormalizedExpr::And(vec![
                NormalizedExpr::Filter(NormalizedFilter::EffectiveTag(NormalizedTagFilter {
                    tag: "z/todo".to_owned(),
                })),
                NormalizedExpr::Filter(NormalizedFilter::Special(NormalizedSpecialFilter {
                    field: NormalizedSpecialField::Links,
                    op: ComparisonOp::Equals,
                    value: ComparisonLiteral::String("#project/plan".to_owned()),
                })),
                NormalizedExpr::Filter(NormalizedFilter::Special(NormalizedSpecialFilter {
                    field: NormalizedSpecialField::File,
                    op: ComparisonOp::Equals,
                    value: ComparisonLiteral::String("projects/*.z".to_owned()),
                })),
                NormalizedExpr::Filter(NormalizedFilter::Special(NormalizedSpecialFilter {
                    field: NormalizedSpecialField::Todo,
                    op: ComparisonOp::Equals,
                    value: ComparisonLiteral::String("[ ]".to_owned()),
                })),
                NormalizedExpr::Filter(NormalizedFilter::Text(NormalizedTextFilter {
                    phrase: "alpha beta".to_owned(),
                    explicit: true,
                })),
                NormalizedExpr::Filter(NormalizedFilter::Special(NormalizedSpecialFilter {
                    field: NormalizedSpecialField::Modified,
                    op: ComparisonOp::LessThan,
                    value: ComparisonLiteral::RelativeDays(7),
                })),
                NormalizedExpr::Not(Box::new(NormalizedExpr::Filter(
                    NormalizedFilter::Property(NormalizedPropertyFilter {
                        key: "did".to_owned(),
                        op: ComparisonOp::Exists,
                        value: None,
                        semantics: PropertySemantics::Exists,
                    }),
                ))),
            ])
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
            normalized.expr,
            NormalizedExpr::And(vec![
                NormalizedExpr::Filter(NormalizedFilter::Property(NormalizedPropertyFilter {
                    key: "p".to_owned(),
                    op: ComparisonOp::GreaterThan,
                    value: Some(ComparisonLiteral::Number(3.0)),
                    semantics: PropertySemantics::Number,
                })),
                NormalizedExpr::Filter(NormalizedFilter::Property(NormalizedPropertyFilter {
                    key: "due".to_owned(),
                    op: ComparisonOp::LessThanOrEqual,
                    value: Some(ComparisonLiteral::Date(QueryDate::new(2026, 5, 2).unwrap())),
                    semantics: PropertySemantics::Date,
                })),
                NormalizedExpr::Filter(NormalizedFilter::Property(NormalizedPropertyFilter {
                    key: "start".to_owned(),
                    op: ComparisonOp::GreaterThanOrEqual,
                    value: Some(ComparisonLiteral::Time(QueryTime::new(9, 30, 0).unwrap())),
                    semantics: PropertySemantics::Time,
                })),
                NormalizedExpr::Filter(NormalizedFilter::Property(NormalizedPropertyFilter {
                    key: "area".to_owned(),
                    op: ComparisonOp::Equals,
                    value: Some(ComparisonLiteral::String("work/research".to_owned())),
                    semantics: PropertySemantics::StringEquality,
                })),
            ])
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

        let NormalizedFilter::Property(priority) = normalized_filter_at(&normalized, 0) else {
            panic!("expected property filter");
        };
        assert!(property_filter_matches_value(priority, "4"));
        assert!(!property_filter_matches_value(priority, "2"));
        assert!(!property_filter_matches_value(priority, "4/5"));

        let NormalizedFilter::Property(due) = normalized_filter_at(&normalized, 1) else {
            panic!("expected property filter");
        };
        assert!(property_filter_matches_value(due, "2026-05-01"));
        assert!(!property_filter_matches_value(due, "2026-05-03"));

        let NormalizedFilter::Property(start) = normalized_filter_at(&normalized, 2) else {
            panic!("expected property filter");
        };
        assert!(property_filter_matches_value(start, "09:30"));
        assert!(!property_filter_matches_value(start, "10:30"));

        let NormalizedFilter::Property(area) = normalized_filter_at(&normalized, 3) else {
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
        let NormalizedFilter::Property(did) = normalized_filter_at(&normalized, 0) else {
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

        let NormalizedFilter::Special(recent) = normalized_filter_at(&recent, 0) else {
            panic!("expected modified filter");
        };
        let NormalizedFilter::Special(older) = normalized_filter_at(&older, 0) else {
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
                source_span: SourceSpan::bytes(0, 12),
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
            text_matches: Vec::new(),
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
    fn evaluates_text_filters_from_store_match_sets() {
        let context = fixed_context();
        let text_filter = NormalizedTextFilter {
            phrase: "alpha body".to_owned(),
            explicit: true,
        };
        let store = FakeStore {
            files: vec![QueryFile {
                id: 1,
                relative_path: PathBuf::from("project.z"),
                mtime_unix_ms: None,
            }],
            zettel: vec![
                QueryZettel {
                    id: 10,
                    file_id: 1,
                    source_order: 0,
                    title: Some("Alpha".to_owned()),
                    canonical_id: Some("alpha".to_owned()),
                    body_text: String::new(),
                    source_span: SourceSpan::bytes(0, 12),
                },
                QueryZettel {
                    id: 20,
                    file_id: 1,
                    source_order: 1,
                    title: Some("Beta".to_owned()),
                    canonical_id: Some("beta".to_owned()),
                    body_text: "alpha body".to_owned(),
                    source_span: SourceSpan::bytes(13, 25),
                },
            ],
            text_matches: vec![(text_filter, vec![10])],
            ..FakeStore::default()
        };

        assert_ids(
            execute_list_query(&store, &context, "text:\"alpha body\"").unwrap(),
            &["alpha"],
        );
        assert_ids(
            execute_list_query(&store, &context, "-text:\"alpha body\"").unwrap(),
            &["beta"],
        );
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
            execute_list_query(&store, &context, "\"Alpha phrase\"").unwrap(),
            &["root"],
        );
        assert_ids(
            execute_list_query(
                &store,
                &context,
                "file:projects/main.z text:\"alpha implementation\" #z/todo",
            )
            .unwrap(),
            &["root/plan/task"],
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
    fn evaluates_or_grouping_and_boolean_precedence() {
        let (_temp, store, context) = indexed_query_store();

        assert_ids(
            execute_list_query(&store, &context, "#z/todo OR #area/archive").unwrap(),
            &["root/plan/task", "root/plan", "root/review", "root/archive"],
        );
        assert_ids(
            execute_list_query(&store, &context, "(#z/todo OR #z/ref) -did:*").unwrap(),
            &["root/plan/task", "root/plan", "root/review", "target"],
        );
        assert_ids(
            execute_list_query(&store, &context, "#z/todo (#area/root OR #area/archive)").unwrap(),
            &["root/plan/task", "root/plan", "root/review"],
        );
        assert_ids(
            execute_list_query(&store, &context, "#z/todo OR #z/ref -did:*").unwrap(),
            &["root/plan/task", "root/plan", "root/review", "target"],
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
        assert_eq!(render_table_rows(&[]), "");
        assert_eq!(render_table_results(&[]), "");
    }

    #[test]
    fn renders_structured_table_rows_with_stable_columns() {
        let rows = vec![
            TableRow {
                todo: "[ ]".to_owned(),
                id: "@project/plan".to_owned(),
                file: "projects/nested.z".to_owned(),
                title: "Plan the next Zorg milestone.".to_owned(),
            },
            TableRow {
                todo: String::new(),
                id: "-".to_owned(),
                file: "minimal.z".to_owned(),
                title: "Minimal fixture".to_owned(),
            },
        ];

        assert_eq!(
            render_table_rows(&rows),
            "\
Todo  ID             File               Title
----  -------------  -----------------  -----------------------------
[ ]   @project/plan  projects/nested.z  Plan the next Zorg milestone.
      -              minimal.z          Minimal fixture"
        );
    }

    #[test]
    fn renders_count_results_as_simple_text() {
        assert_eq!(render_count_result(3), "count 3");
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
    fn executes_queries_against_read_only_store() {
        let (_temp, store, context) = indexed_query_store();
        let options =
            StoreOptions::new(store.root(), store.database_path()).expect("read-only options");
        drop(store);
        let read_only =
            Store::open_read_only_with_options(options).expect("open read-only query store");

        assert_ids(
            execute_list_query(
                &read_only,
                &context,
                "file:projects/main.z text:\"alpha implementation\" #z/todo",
            )
            .unwrap(),
            &["root/plan/task"],
        );
        assert_eq!(
            load_query_snapshot(&read_only)
                .expect("load read-only snapshot")
                .files
                .len(),
            2
        );
    }

    #[test]
    fn executes_count_output_query() {
        let (_temp, store, context) = indexed_query_store();

        let output = execute_and_render_query(&store, &context, "count(#z/todo)").unwrap();

        assert_eq!(output, "count 3");
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
                source_span: SourceSpan::bytes(0, 43),
            }],
            effective_tags: Vec::new(),
            properties: Vec::new(),
            todos: Vec::new(),
            links: Vec::new(),
        };
        let plan = QueryPlan {
            expr: NormalizedExpr::Filter(NormalizedFilter::Text(NormalizedTextFilter {
                phrase: "meaningful".to_owned(),
                explicit: true,
            })),
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
                source_span: SourceSpan::bytes(0, 0),
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

    fn query_and(filters: Vec<Filter>) -> QueryExpr {
        QueryExpr::And(filters.into_iter().map(QueryExpr::Filter).collect())
    }

    fn normalized_filter_at(query: &NormalizedQuery, index: usize) -> &NormalizedFilter {
        match &query.expr {
            NormalizedExpr::Filter(filter) if index == 0 => filter,
            NormalizedExpr::And(expressions) => {
                let Some(NormalizedExpr::Filter(filter)) = expressions.get(index) else {
                    panic!("expected normalized filter at index {index}");
                };
                filter
            }
            _ => panic!("expected normalized filter at index {index}"),
        }
    }

    #[derive(Debug, Clone, Default)]
    struct FakeStore {
        files: Vec<QueryFile>,
        zettel: Vec<QueryZettel>,
        effective_tags: Vec<QueryEffectiveTag>,
        properties: Vec<QueryProperty>,
        todos: Vec<QueryTodo>,
        links: Vec<QueryLink>,
        text_matches: Vec<(NormalizedTextFilter, Vec<i64>)>,
    }

    impl QueryStore for FakeStore {
        fn query_files(&self) -> ZorgResult<Vec<QueryFile>> {
            Ok(self.files.clone())
        }

        fn query_zettel(&self, _include_body_text: bool) -> ZorgResult<Vec<QueryZettel>> {
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

        fn query_text_matches(
            &self,
            filter: &NormalizedTextFilter,
        ) -> ZorgResult<Option<Vec<i64>>> {
            Ok(self
                .text_matches
                .iter()
                .find(|(candidate, _)| candidate == filter)
                .map(|(_, zettel_ids)| zettel_ids.clone()))
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
