//! Shared semantic types for Zorg crates.

use std::fmt;
use std::path::PathBuf;

/// Result type shared by foundation crates.
pub type ZorgResult<T> = Result<T, ZorgError>;

/// Foundation-phase error type.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ZorgError {
    /// The requested behavior belongs to a later implementation phase.
    Unsupported(&'static str),
}

impl fmt::Display for ZorgError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ZorgError {}

/// Canonical zettel ID text, including its declaration marker when displayed.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ZettelId(String);

impl ZettelId {
    /// Creates an ID wrapper without validating syntax.
    #[must_use]
    pub fn unchecked(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the stored ID text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Byte-oriented source span with optional translated line and column data.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SourceSpan {
    /// Start byte offset, inclusive.
    pub start_byte: usize,
    /// End byte offset, exclusive.
    pub end_byte: usize,
    /// One-based start line when known.
    pub start_line: Option<usize>,
    /// One-based start column when known.
    pub start_column: Option<usize>,
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
        }
    }
}

/// Diagnostic severity shared by parser, model, fix, and LSP code.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Severity {
    /// Error-level diagnostic.
    Error,
    /// Warning-level diagnostic.
    Warning,
    /// Informational diagnostic.
    Info,
}

/// Source-backed validation finding.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Diagnostic {
    /// Diagnostic severity.
    pub severity: Severity,
    /// Human-readable diagnostic message.
    pub message: String,
    /// Source file path when known.
    pub path: Option<PathBuf>,
    /// Source span when known.
    pub span: Option<SourceSpan>,
}

impl Diagnostic {
    /// Creates an error diagnostic without source location.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            path: None,
            span: None,
        }
    }
}
