//! SWOG LIST query boundary for Zorg.

use zorg_core::{ZorgError, ZorgResult};

/// Query result placeholder for LIST rows.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ListRow {
    /// Display text for the row.
    pub label: String,
}

/// Evaluates a SWOG LIST query.
pub fn run_list_query(_query: &str) -> ZorgResult<Vec<ListRow>> {
    Err(ZorgError::Unsupported(
        "zorg-query is a foundation stub; query evaluation is pending",
    ))
}
