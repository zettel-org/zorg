//! Shared semantic types for Zorg crates.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Result type shared by Zorg crates.
pub type ZorgResult<T> = Result<T, ZorgError>;

/// Shared error type for crate-boundary failures.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum ZorgError {
    /// The requested behavior belongs to a later implementation phase.
    Unsupported(&'static str),
    /// A user-authored identifier or reference failed syntax checks.
    InvalidIdentifier {
        /// Human-readable error message.
        message: String,
    },
}

impl fmt::Display for ZorgError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(message) => formatter.write_str(message),
            Self::InvalidIdentifier { message } => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ZorgError {}

/// Source file path retained with parsed documents and diagnostics.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct SourcePath(PathBuf);

impl SourcePath {
    /// Creates a source path wrapper.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// Returns the wrapped path.
    #[must_use]
    pub fn as_path(&self) -> &std::path::Path {
        self.0.as_path()
    }

    /// Consumes this wrapper and returns the path.
    #[must_use]
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

impl From<PathBuf> for SourcePath {
    fn from(path: PathBuf) -> Self {
        Self(path)
    }
}

impl From<SourcePath> for PathBuf {
    fn from(path: SourcePath) -> Self {
        path.0
    }
}

/// One-based source position suitable for later LSP conversion.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct SourcePosition {
    /// Byte offset from the start of the source.
    pub byte: usize,
    /// One-based line number.
    pub line: usize,
    /// One-based column number.
    pub column: usize,
}

impl SourcePosition {
    /// Creates a source position.
    #[must_use]
    pub const fn new(byte: usize, line: usize, column: usize) -> Self {
        Self { byte, line, column }
    }
}

/// Byte-oriented source span with optional translated line and column data.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceSpan {
    /// Start byte offset, inclusive.
    pub start_byte: usize,
    /// End byte offset, exclusive.
    pub end_byte: usize,
    /// One-based start line when known.
    pub start_line: Option<usize>,
    /// One-based start column when known.
    pub start_column: Option<usize>,
    /// One-based end line when known.
    pub end_line: Option<usize>,
    /// One-based end column when known.
    pub end_column: Option<usize>,
}

impl SourceSpan {
    /// Constructs a byte-only source span.
    #[must_use]
    pub const fn bytes(start_byte: usize, end_byte: usize) -> Self {
        Self {
            start_byte,
            end_byte,
            start_line: None,
            start_column: None,
            end_line: None,
            end_column: None,
        }
    }

    /// Constructs a source span with byte and one-based line/column positions.
    #[must_use]
    pub const fn positioned(start: SourcePosition, end: SourcePosition) -> Self {
        Self {
            start_byte: start.byte,
            end_byte: end.byte,
            start_line: Some(start.line),
            start_column: Some(start.column),
            end_line: Some(end.line),
            end_column: Some(end.column),
        }
    }

    /// Translates a byte range in `source` into a positioned source span.
    ///
    /// Offsets past the end of the source are clamped to the end position.
    #[must_use]
    pub fn from_offsets(source: &str, start_byte: usize, end_byte: usize) -> Self {
        let start = position_for_offset(source, start_byte);
        let end = position_for_offset(source, end_byte);
        Self::positioned(start, end)
    }
}

/// Returns the one-based line and column for a byte offset in a UTF-8 source.
#[must_use]
pub fn position_for_offset(source: &str, offset: usize) -> SourcePosition {
    let target = offset.min(source.len());
    let mut line = 1;
    let mut column = 1;

    for (byte, character) in source.char_indices() {
        if byte >= target {
            return SourcePosition::new(target, line, column);
        }

        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    SourcePosition::new(target, line, column)
}

/// Canonical zettel ID without the declaration marker.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct ZettelId(String);

impl ZettelId {
    /// Creates an ID wrapper without validating syntax.
    #[must_use]
    pub fn unchecked(value: impl Into<String>) -> Self {
        let value = value.into();
        Self(value.strip_prefix('@').unwrap_or(&value).to_owned())
    }

    /// Parses an absolute ID declaration such as `@alpha/beta`.
    pub fn parse(value: &str) -> ZorgResult<Self> {
        let Some(canonical) = value.strip_prefix('@') else {
            return invalid_identifier("zettel ID declarations must start with '@'");
        };

        Self::parse_canonical(canonical)
    }

    /// Parses canonical ID text without the declaration marker.
    pub fn parse_canonical(value: &str) -> ZorgResult<Self> {
        validate_slash_path(value, "zettel ID")?;
        Ok(Self(value.to_owned()))
    }

    /// Returns canonical ID text without the declaration marker.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns declaration text with the leading `@` marker.
    #[must_use]
    pub fn declaration(&self) -> String {
        format!("@{}", self.0)
    }
}

impl fmt::Display for ZettelId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "@{}", self.0)
    }
}

/// Local zettel ID without the declaration marker.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct LocalId(String);

impl LocalId {
    /// Creates a local ID wrapper without validating syntax.
    #[must_use]
    pub fn unchecked(value: impl Into<String>) -> Self {
        let value = value.into();
        Self(value.strip_prefix('^').unwrap_or(&value).to_owned())
    }

    /// Parses a local ID declaration such as `^meeting-notes`.
    pub fn parse(value: &str) -> ZorgResult<Self> {
        let Some(canonical) = value.strip_prefix('^') else {
            return invalid_identifier("local ID declarations must start with '^'");
        };

        validate_segment(canonical, "local ID")?;
        Ok(Self(canonical.to_owned()))
    }

    /// Returns canonical local ID text without the declaration marker.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns declaration text with the leading `^` marker.
    #[must_use]
    pub fn declaration(&self) -> String {
        format!("^{}", self.0)
    }
}

impl fmt::Display for LocalId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "^{}", self.0)
    }
}

/// Relative link segment used by `+child` and `~sibling` references.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct RelativeId(String);

impl RelativeId {
    /// Parses a relative link body without its leading marker.
    pub fn parse(value: &str) -> ZorgResult<Self> {
        validate_segment(value, "relative ID")?;
        Ok(Self(value.to_owned()))
    }

    /// Returns the relative ID text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Unresolved link/reference form as written in source.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum ReferenceTarget {
    /// Absolute `#foo/bar` link.
    Absolute(ZettelId),
    /// Child-relative `+child` link.
    Child(RelativeId),
    /// Sibling-relative `~sibling` link.
    Sibling(RelativeId),
    /// Local declaration reference before ancestor resolution.
    LocalDeclaration(LocalId),
}

impl ReferenceTarget {
    /// Parses a link or local declaration marker.
    pub fn parse(value: &str) -> ZorgResult<Self> {
        if let Some(canonical) = value.strip_prefix('#') {
            return Ok(Self::Absolute(ZettelId::parse_canonical(canonical)?));
        }
        if let Some(canonical) = value.strip_prefix('+') {
            return Ok(Self::Child(RelativeId::parse(canonical)?));
        }
        if let Some(canonical) = value.strip_prefix('~') {
            return Ok(Self::Sibling(RelativeId::parse(canonical)?));
        }
        if value.starts_with('^') {
            return Ok(Self::LocalDeclaration(LocalId::parse(value)?));
        }

        invalid_identifier("references must start with '#', '+', '~', or '^'")
    }
}

/// A source-backed unresolved reference.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Reference {
    /// Reference target form.
    pub target: ReferenceTarget,
    /// Original source text.
    pub raw: String,
    /// Source span for the reference.
    pub span: Option<SourceSpan>,
}

/// Resolved reference target.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedReference {
    /// Reference as written before resolution.
    pub reference: Reference,
    /// Canonical target ID after resolution.
    pub target_id: ZettelId,
}

/// Explicit tag text without the leading `#`.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct Tag(String);

impl Tag {
    /// Parses a tag such as `#area/work` or `#z/todo`.
    pub fn parse(value: &str) -> ZorgResult<Self> {
        let Some(canonical) = value.strip_prefix('#') else {
            return invalid_identifier("tags must start with '#'");
        };

        validate_slash_path(canonical, "tag")?;
        Ok(Self(canonical.to_owned()))
    }

    /// Creates a tag wrapper without validating syntax.
    #[must_use]
    pub fn unchecked(value: impl Into<String>) -> Self {
        let value = value.into();
        Self(value.strip_prefix('#').unwrap_or(&value).to_owned())
    }

    /// Returns tag text without the leading `#`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true when this tag is in the reserved `#z/...` type namespace.
    #[must_use]
    pub fn is_type_tag(&self) -> bool {
        self.0.starts_with("z/")
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "#{}", self.0)
    }
}

/// Tag plus source span and type-tag classification.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaggedValue {
    /// Parsed tag.
    pub tag: Tag,
    /// True for tags in the reserved `#z/...` namespace.
    pub is_type_tag: bool,
    /// Source span for the full tag token.
    pub span: Option<SourceSpan>,
}

impl TaggedValue {
    /// Creates a tagged value from a parsed tag.
    #[must_use]
    pub fn new(tag: Tag, span: Option<SourceSpan>) -> Self {
        let is_type_tag = tag.is_type_tag();
        Self {
            tag,
            is_type_tag,
            span,
        }
    }
}

/// Ordered key/value property with spans for the full token, key, and value.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Property {
    /// Parsed key.
    pub key: String,
    /// Raw trimmed value text.
    pub value: String,
    /// Source span for the complete property token.
    pub span: Option<SourceSpan>,
    /// Source span for the key.
    pub key_span: Option<SourceSpan>,
    /// Source span for the value.
    pub value_span: Option<SourceSpan>,
}

impl Property {
    /// Creates a checked property without source spans.
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> ZorgResult<Self> {
        let key = key.into();
        validate_property_key(&key)?;
        Ok(Self {
            key,
            value: value.into().trim().to_owned(),
            span: None,
            key_span: None,
            value_span: None,
        })
    }

    /// Parses `key::value` text into a property.
    pub fn parse(raw: &str) -> ZorgResult<Self> {
        let Some((key, value)) = raw.split_once("::") else {
            return invalid_identifier("properties must use 'key::value' syntax");
        };
        let key = key.trim();
        validate_property_key(key)?;
        Ok(Self {
            key: key.to_owned(),
            value: value.trim().to_owned(),
            span: None,
            key_span: None,
            value_span: None,
        })
    }
}

/// Todo marker attached to a zettel opening.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum TodoMarker {
    /// `[ ]` open marker.
    Open,
    /// `[N]` next/priority marker.
    Next,
    /// `[X]` done marker.
    Done,
    /// `[?]` in-progress or unknown marker.
    Unknown,
}

impl TodoMarker {
    /// Parses a todo marker token.
    pub fn parse(value: &str) -> ZorgResult<Self> {
        match value {
            "[ ]" => Ok(Self::Open),
            "[N]" => Ok(Self::Next),
            "[X]" => Ok(Self::Done),
            "[?]" => Ok(Self::Unknown),
            _ => invalid_identifier("todo markers must be one of '[ ]', '[N]', '[X]', or '[?]'"),
        }
    }
}

impl fmt::Display for TodoMarker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Open => "[ ]",
            Self::Next => "[N]",
            Self::Done => "[X]",
            Self::Unknown => "[?]",
        })
    }
}

/// Plain title part retained with its source span.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TitlePart {
    /// Plain title text.
    pub text: String,
    /// Source span for the title text.
    pub span: Option<SourceSpan>,
}

impl TitlePart {
    /// Creates an unspanned title part.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            span: None,
        }
    }
}

/// Parser-local zettel key used before persistence assigns store IDs.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct ZettelKey(String);

impl ZettelKey {
    /// Creates a parser-local zettel key.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the key text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ZettelKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Zettel role in the source hierarchy.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum ZettelKind {
    /// A normal `.z` file zettel.
    File,
    /// A directory zettel represented by `init.z`.
    Directory,
    /// A nested list zettel inside another zettel body.
    Nested,
}

/// Paragraph body block.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Paragraph {
    /// Raw paragraph text.
    pub text: String,
    /// Outgoing references found in paragraph text.
    pub links: Vec<Reference>,
    /// Source span for the paragraph.
    pub span: Option<SourceSpan>,
}

/// Fenced code block body.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FencedCodeBlock {
    /// Optional fence info string.
    pub info: Option<String>,
    /// Code block body text.
    pub body: String,
    /// Source span for the whole fenced code block.
    pub span: Option<SourceSpan>,
    /// Source span for the info string when present.
    pub info_span: Option<SourceSpan>,
    /// Source span for the body.
    pub body_span: Option<SourceSpan>,
}

/// Body content for a zettel.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum BodyBlock {
    /// Paragraph text.
    Paragraph(Paragraph),
    /// Markdown fenced code block.
    FencedCode(FencedCodeBlock),
    /// Nested child zettel.
    ChildZettel(Box<Zettel>),
}

/// A zettel semantic object.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Zettel {
    /// Parser-local identity.
    pub key: ZettelKey,
    /// Zettel source role.
    pub kind: ZettelKind,
    /// Parent parser-local identity when known.
    pub parent: Option<ZettelKey>,
    /// Child parser-local identities in source order.
    pub children: Vec<ZettelKey>,
    /// Optional absolute ID declaration.
    pub id: Option<ZettelId>,
    /// Optional local ID declaration.
    pub local_id: Option<LocalId>,
    /// Canonical ID after resolution, including local ID canonicalization.
    pub canonical_id: Option<ZettelId>,
    /// Plain title parts in source order.
    pub title: Vec<TitlePart>,
    /// Explicit non-type tags.
    pub tags: Vec<TaggedValue>,
    /// Explicit `#z/...` type tags.
    pub type_tags: Vec<TaggedValue>,
    /// Ordered properties.
    pub properties: Vec<Property>,
    /// Optional todo marker.
    pub todo: Option<TodoMarker>,
    /// Outgoing unresolved references.
    pub links: Vec<Reference>,
    /// Outgoing resolved references.
    pub resolved_links: Vec<ResolvedReference>,
    /// Source-backed body blocks.
    pub body: Vec<BodyBlock>,
    /// Source path when known.
    pub path: Option<SourcePath>,
    /// Source span for the zettel opening/header.
    pub span: Option<SourceSpan>,
    /// Diagnostics attached to this zettel.
    pub diagnostics: Vec<Diagnostic>,
}

impl Zettel {
    /// Creates an empty zettel with validation-light fields for parser lowering.
    #[must_use]
    pub fn new(key: ZettelKey, kind: ZettelKind) -> Self {
        Self {
            key,
            kind,
            parent: None,
            children: Vec::new(),
            id: None,
            local_id: None,
            canonical_id: None,
            title: Vec::new(),
            tags: Vec::new(),
            type_tags: Vec::new(),
            properties: Vec::new(),
            todo: None,
            links: Vec::new(),
            resolved_links: Vec::new(),
            body: Vec::new(),
            path: None,
            span: None,
            diagnostics: Vec::new(),
        }
    }

    /// Returns the plain title text, joining title parts with single spaces.
    #[must_use]
    pub fn plain_title(&self) -> Option<String> {
        if self.title.is_empty() {
            return None;
        }

        Some(
            self.title
                .iter()
                .map(|part| part.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        )
    }
}

/// A parsed semantic document rooted at one file or directory zettel.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ZettelDocument {
    /// Source path when known.
    pub path: Option<SourcePath>,
    /// Original UTF-8 source text when retained by the parser.
    pub source: String,
    /// Root file or directory zettel.
    pub root: Zettel,
    /// Document-level diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

impl ZettelDocument {
    /// Creates a parsed document model.
    #[must_use]
    pub fn new(source: impl Into<String>, root: Zettel) -> Self {
        Self {
            path: None,
            source: source.into(),
            root,
            diagnostics: Vec::new(),
        }
    }
}

/// Diagnostic severity shared by parser, model, fix, and LSP code.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum Severity {
    /// Error-level diagnostic.
    Error,
    /// Warning-level diagnostic.
    Warning,
    /// Informational diagnostic.
    Info,
}

/// Stable diagnostic category for downstream consumers.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum DiagnosticCategory {
    /// Syntax recovery or parse-boundary issue.
    Syntax,
    /// Semantic validation issue.
    Semantic,
    /// Unsupported legacy-looking input.
    Legacy,
    /// Unsupported feature that is intentionally outside the current phase.
    Unsupported,
}

/// Source-backed validation finding.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Diagnostic severity.
    pub severity: Severity,
    /// Stable diagnostic category.
    pub category: DiagnosticCategory,
    /// Stable diagnostic code.
    pub code: Option<String>,
    /// Human-readable diagnostic message.
    pub message: String,
    /// Source file path when known.
    pub path: Option<SourcePath>,
    /// Source span when known.
    pub span: Option<SourceSpan>,
}

impl Diagnostic {
    /// Creates an error diagnostic without source location.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            category: DiagnosticCategory::Semantic,
            code: None,
            message: message.into(),
            path: None,
            span: None,
        }
    }

    /// Creates a syntax recovery diagnostic.
    #[must_use]
    pub fn syntax_recovery(message: impl Into<String>, span: Option<SourceSpan>) -> Self {
        Self {
            severity: Severity::Error,
            category: DiagnosticCategory::Syntax,
            code: Some("syntax.recovery".to_owned()),
            message: message.into(),
            path: None,
            span,
        }
    }

    /// Creates a semantic validation diagnostic.
    #[must_use]
    pub fn semantic_validation(
        code: impl Into<String>,
        message: impl Into<String>,
        span: Option<SourceSpan>,
    ) -> Self {
        Self {
            severity: Severity::Error,
            category: DiagnosticCategory::Semantic,
            code: Some(code.into()),
            message: message.into(),
            path: None,
            span,
        }
    }

    /// Creates a diagnostic for unsupported legacy-looking input.
    #[must_use]
    pub fn unsupported_legacy(message: impl Into<String>, span: Option<SourceSpan>) -> Self {
        Self {
            severity: Severity::Error,
            category: DiagnosticCategory::Legacy,
            code: Some("legacy.unsupported".to_owned()),
            message: message.into(),
            path: None,
            span,
        }
    }

    /// Returns this diagnostic with a source path attached.
    #[must_use]
    pub fn with_path(mut self, path: SourcePath) -> Self {
        self.path = Some(path);
        self
    }
}

fn validate_slash_path(value: &str, label: &str) -> ZorgResult<()> {
    if value.is_empty() {
        return invalid_identifier(format!("{label} must not be empty"));
    }

    for segment in value.split('/') {
        validate_segment(segment, label)?;
    }

    Ok(())
}

fn validate_segment(value: &str, label: &str) -> ZorgResult<()> {
    if value.is_empty() {
        return invalid_identifier(format!("{label} segments must not be empty"));
    }

    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return invalid_identifier(format!("{label} segments must not be empty"));
    };

    if !first.is_ascii_alphanumeric() {
        return invalid_identifier(format!(
            "{label} segments must start with an ASCII letter or digit"
        ));
    }

    if characters.any(|character| {
        !(character.is_ascii_alphanumeric() || character == '_' || character == '-')
    }) {
        return invalid_identifier(format!(
            "{label} segments may only contain ASCII letters, digits, '_' or '-'"
        ));
    }

    Ok(())
}

fn validate_property_key(key: &str) -> ZorgResult<()> {
    if key.is_empty() {
        return invalid_identifier("property keys must not be empty");
    }

    let mut characters = key.chars();
    let Some(first) = characters.next() else {
        return invalid_identifier("property keys must not be empty");
    };

    if !first.is_ascii_alphabetic() {
        return invalid_identifier("property keys must start with an ASCII letter");
    }

    if characters.any(|character| {
        !(character.is_ascii_alphanumeric() || character == '_' || character == '-')
    }) {
        return invalid_identifier(
            "property keys may only contain ASCII letters, digits, '_' or '-'",
        );
    }

    Ok(())
}

fn invalid_identifier<T>(message: impl Into<String>) -> ZorgResult<T> {
    Err(ZorgError::InvalidIdentifier {
        message: message.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_offsets_convert_to_one_based_line_and_column() {
        let source = "alpha\nbravo\ncafe";

        assert_eq!(position_for_offset(source, 0), SourcePosition::new(0, 1, 1));
        assert_eq!(position_for_offset(source, 6), SourcePosition::new(6, 2, 1));
        assert_eq!(
            SourceSpan::from_offsets(source, 6, 11),
            SourceSpan::positioned(SourcePosition::new(6, 2, 1), SourcePosition::new(11, 2, 6))
        );
    }

    #[test]
    fn source_offsets_handle_multibyte_utf8() {
        let source = "a\néx";

        assert_eq!(position_for_offset(source, 2), SourcePosition::new(2, 2, 1));
        assert_eq!(position_for_offset(source, 4), SourcePosition::new(4, 2, 2));
    }

    #[test]
    fn zettel_ids_parse_and_format_canonically() {
        let id = ZettelId::parse("@alpha/beta-2").expect("valid ID");

        assert_eq!(id.as_str(), "alpha/beta-2");
        assert_eq!(id.declaration(), "@alpha/beta-2");
        assert_eq!(id.to_string(), "@alpha/beta-2");
        assert_eq!(
            ZettelId::parse_canonical("alpha_1")
                .expect("valid")
                .as_str(),
            "alpha_1"
        );
    }

    #[test]
    fn zettel_ids_reject_invalid_forms() {
        assert!(ZettelId::parse("alpha").is_err());
        assert!(ZettelId::parse("@").is_err());
        assert!(ZettelId::parse("@alpha//beta").is_err());
        assert!(ZettelId::parse("@-alpha").is_err());
        assert!(ZettelId::parse("@alpha.beta").is_err());
    }

    #[test]
    fn local_ids_and_references_parse_canonical_forms() {
        let local = LocalId::parse("^meeting-notes").expect("valid local ID");
        assert_eq!(local.as_str(), "meeting-notes");
        assert_eq!(local.to_string(), "^meeting-notes");

        assert_eq!(
            ReferenceTarget::parse("#foo/bar").expect("absolute link"),
            ReferenceTarget::Absolute(ZettelId::parse_canonical("foo/bar").expect("valid"))
        );
        assert_eq!(
            ReferenceTarget::parse("+child").expect("child link"),
            ReferenceTarget::Child(RelativeId::parse("child").expect("valid"))
        );
        assert_eq!(
            ReferenceTarget::parse("~sibling").expect("sibling link"),
            ReferenceTarget::Sibling(RelativeId::parse("sibling").expect("valid"))
        );
        assert_eq!(
            ReferenceTarget::parse("^local").expect("local declaration"),
            ReferenceTarget::LocalDeclaration(LocalId::parse("^local").expect("valid"))
        );
    }

    #[test]
    fn references_reject_invalid_forms() {
        assert!(ReferenceTarget::parse("foo").is_err());
        assert!(ReferenceTarget::parse("#foo//bar").is_err());
        assert!(ReferenceTarget::parse("+bad/child").is_err());
        assert!(ReferenceTarget::parse("~").is_err());
    }

    #[test]
    fn tags_classify_type_tags() {
        let normal = Tag::parse("#area/work").expect("valid tag");
        let type_tag = Tag::parse("#z/todo").expect("valid type tag");

        assert!(!normal.is_type_tag());
        assert!(type_tag.is_type_tag());
        assert_eq!(normal.to_string(), "#area/work");
    }

    #[test]
    fn properties_parse_key_value_and_reject_bad_keys() {
        let property = Property::parse("due::2026-05-15").expect("valid property");
        assert_eq!(property.key, "due");
        assert_eq!(property.value, "2026-05-15");

        assert!(Property::parse("1due::2026-05-15").is_err());
        assert!(Property::parse("due:2026-05-15").is_err());
        assert!(Property::parse("bad.key::value").is_err());
    }

    #[test]
    fn todo_markers_parse_known_tokens() {
        assert_eq!(TodoMarker::parse("[ ]").expect("open"), TodoMarker::Open);
        assert_eq!(TodoMarker::parse("[N]").expect("next"), TodoMarker::Next);
        assert_eq!(TodoMarker::parse("[X]").expect("done"), TodoMarker::Done);
        assert_eq!(
            TodoMarker::parse("[?]").expect("unknown"),
            TodoMarker::Unknown
        );
        assert!(TodoMarker::parse("[x]").is_err());
    }

    #[test]
    fn constructs_representative_file_directory_nested_query_template_and_todo_models() {
        let mut file = Zettel::new(
            ZettelKey::new("file:fixtures/corpus/minimal.z"),
            ZettelKind::File,
        );
        file.id = Some(ZettelId::parse("@minimal").expect("valid ID"));
        file.title.push(TitlePart::new("Minimal fixture"));
        file.tags.push(TaggedValue::new(
            Tag::parse("#area/work").expect("valid tag"),
            None,
        ));
        file.type_tags.push(TaggedValue::new(
            Tag::parse("#z/ref").expect("valid type tag"),
            None,
        ));
        file.properties
            .push(Property::parse("area::work/research").expect("valid property"));
        file.body.push(BodyBlock::Paragraph(Paragraph {
            text: "This file zettel demonstrates the model.".to_owned(),
            links: Vec::new(),
            span: None,
        }));
        assert_eq!(file.plain_title().as_deref(), Some("Minimal fixture"));

        let mut directory = Zettel::new(ZettelKey::new("dir:init"), ZettelKind::Directory);
        directory.id = Some(ZettelId::parse("@dir-example").expect("valid ID"));

        let mut todo = Zettel::new(ZettelKey::new("nested:task"), ZettelKind::Nested);
        todo.parent = Some(file.key.clone());
        todo.local_id = Some(LocalId::parse("^task").expect("valid local ID"));
        todo.todo = Some(TodoMarker::Open);
        todo.type_tags.push(TaggedValue::new(
            Tag::parse("#z/todo").expect("valid type tag"),
            None,
        ));

        let mut query = Zettel::new(ZettelKey::new("nested:query"), ZettelKind::Nested);
        query.id = Some(ZettelId::parse("@system/queries/today").expect("valid ID"));
        query.type_tags.push(TaggedValue::new(
            Tag::parse("#z/query").expect("valid type tag"),
            None,
        ));
        query
            .properties
            .push(Property::parse("query::due:<=today").expect("valid query property"));

        let mut template = Zettel::new(ZettelKey::new("nested:template"), ZettelKind::Nested);
        template.id = Some(ZettelId::parse("@system/templates/todo").expect("valid ID"));
        template.type_tags.push(TaggedValue::new(
            Tag::parse("#z/tmpl").expect("valid type tag"),
            None,
        ));
        template.body.push(BodyBlock::FencedCode(FencedCodeBlock {
            info: Some("zorg-template".to_owned()),
            body: "- @{{id}} #z/todo [ ] {{title}}".to_owned(),
            span: None,
            info_span: None,
            body_span: None,
        }));

        file.children.push(todo.key.clone());
        file.body
            .push(BodyBlock::ChildZettel(Box::new(todo.clone())));

        let document = ZettelDocument::new("%%% @minimal\n%%%", file);
        assert_eq!(document.root.kind, ZettelKind::File);
        assert_eq!(directory.kind, ZettelKind::Directory);
        assert_eq!(todo.todo, Some(TodoMarker::Open));
        assert!(
            query
                .type_tags
                .iter()
                .any(|tagged| tagged.tag.as_str() == "z/query")
        );
        assert!(
            template
                .body
                .iter()
                .any(|block| matches!(block, BodyBlock::FencedCode(_)))
        );
    }

    #[test]
    fn diagnostic_constructors_cover_expected_categories() {
        let span = SourceSpan::bytes(3, 12);
        let syntax = Diagnostic::syntax_recovery("parser recovered", Some(span));
        let semantic = Diagnostic::semantic_validation("id.duplicate", "duplicate ID", Some(span));
        let legacy = Diagnostic::unsupported_legacy("ID:: is not v1 syntax", Some(span))
            .with_path(SourcePath::new("fixtures/corpus/legacy_invalid.z"));

        assert_eq!(syntax.category, DiagnosticCategory::Syntax);
        assert_eq!(semantic.code.as_deref(), Some("id.duplicate"));
        assert_eq!(legacy.category, DiagnosticCategory::Legacy);
        assert!(legacy.path.is_some());
    }
}
