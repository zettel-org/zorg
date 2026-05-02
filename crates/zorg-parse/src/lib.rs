//! Parser boundary for Zorg `.z` syntax.

use zorg_core::{ZorgError, ZorgResult};

/// Parsed document placeholder.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ParsedDocument {
    /// Original source text retained until the real parser is implemented.
    pub source: String,
}

/// Parses a `.z` source document.
///
/// Epic 1 only establishes the crate boundary. The real syntax parser belongs
/// to a later phase and must be implemented against `docs/syntax.md`.
pub fn parse_document(_source: &str) -> ZorgResult<ParsedDocument> {
    Err(ZorgError::Unsupported(
        "zorg-parse is a foundation stub; parser implementation is pending",
    ))
}

#[cfg(test)]
mod tests {
    use super::parse_document;

    #[test]
    fn parser_stub_is_explicit() {
        let error = parse_document("%%% @demo\n%%%").expect_err("stub should not parse yet");
        assert!(error.to_string().contains("foundation stub"));
    }
}
