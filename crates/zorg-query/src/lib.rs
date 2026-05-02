//! SWOG LIST query boundary for Zorg.

use std::error::Error;
use std::fmt;

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

/// Query result placeholder for LIST rows.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ListRow {
    /// Display text for the row.
    pub label: String,
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

/// Evaluates a SWOG LIST query.
///
/// Parsing is available in this phase, but store-backed evaluation remains
/// deferred until later Epic 5 work.
pub fn run_list_query(query: &str) -> ZorgResult<Vec<ListRow>> {
    parse_query(query).map_err(|error| ZorgError::OperationFailed {
        message: error.to_string(),
    })?;

    Err(ZorgError::Unsupported(
        "zorg-query parses SWOG, but query evaluation is pending",
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

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
            ZorgError::Unsupported("zorg-query parses SWOG, but query evaluation is pending")
        );
    }
}
