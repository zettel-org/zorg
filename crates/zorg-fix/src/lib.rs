//! Strict check and autofix boundary for Zorg.

use zorg_core::{Diagnostic, ZorgError, ZorgResult};

/// Result of a strict check operation.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CheckReport {
    /// Diagnostics produced by the check.
    pub diagnostics: Vec<Diagnostic>,
}

/// Runs strict checks on a `.z` source string.
pub fn check_strict(_source: &str) -> ZorgResult<CheckReport> {
    Err(ZorgError::Unsupported(
        "zorg-fix is a foundation stub; strict checks are pending",
    ))
}
