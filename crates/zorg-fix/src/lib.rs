//! Strict check and autofix boundary for Zorg.
//!
//! This crate owns the shared fix-plan model that the CLI (`zorg fix`,
//! `zorg check`) and the LSP (`zorg-ls` quickfix code actions) consume. New
//! autofix rules added in later phases should plug into [`plan_fixes`] without
//! requiring downstream surfaces to learn the rule.

mod plan;

pub use plan::{
    ApplySummary, CorpusView, FixEdit, FixKind, FixOp, FixPlan, RuleCode, apply_plan_to_source,
    plan_document_fixes, plan_fixes,
};

use zorg_core::{Diagnostic, ReferenceTarget, ZorgError, ZorgResult};

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

/// Suggests a deterministic replacement for an unresolved absolute link typo.
///
/// The MVP rule is intentionally small: the raw link must be a syntactically
/// valid absolute link, and exactly one known canonical ID may differ from the
/// requested target by one ASCII insertion, deletion, or substitution.
pub fn suggest_absolute_link_typo_fix<'a>(
    raw_link: &str,
    canonical_ids: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let requested = match ReferenceTarget::parse(raw_link).ok()? {
        ReferenceTarget::Absolute(id) => id.as_str().to_owned(),
        ReferenceTarget::Child(_)
        | ReferenceTarget::Sibling(_)
        | ReferenceTarget::LocalDeclaration(_) => return None,
    };

    let mut matches = canonical_ids
        .into_iter()
        .filter(|candidate| *candidate != requested)
        .filter(|candidate| is_single_ascii_edit(&requested, candidate))
        .map(str::to_owned);

    let first = matches.next()?;
    matches.next().is_none().then(|| format!("#{first}"))
}

pub(crate) fn is_single_ascii_edit(left: &str, right: &str) -> bool {
    if !left.is_ascii() || !right.is_ascii() {
        return false;
    }

    let left = left.as_bytes();
    let right = right.as_bytes();
    match left.len().abs_diff(right.len()) {
        0 => {
            left.iter()
                .zip(right)
                .filter(|(left, right)| left != right)
                .count()
                == 1
        }
        1 => {
            let (short, long) = if left.len() < right.len() {
                (left, right)
            } else {
                (right, left)
            };
            let mut short_index = 0;
            let mut long_index = 0;
            let mut edits = 0;

            while short_index < short.len() && long_index < long.len() {
                if short[short_index] == long[long_index] {
                    short_index += 1;
                    long_index += 1;
                } else {
                    edits += 1;
                    long_index += 1;
                    if edits > 1 {
                        return false;
                    }
                }
            }

            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::suggest_absolute_link_typo_fix;

    #[test]
    fn suggests_single_absolute_link_typo_fix() {
        let ids = ["project/plan", "project/review"];

        assert_eq!(
            suggest_absolute_link_typo_fix("#poject/plan", ids),
            Some("#project/plan".to_owned())
        );
    }

    #[test]
    fn declines_ambiguous_absolute_link_typo_fix() {
        let ids = ["alpha", "alphi"];

        assert_eq!(suggest_absolute_link_typo_fix("#alphx", ids), None);
    }

    #[test]
    fn declines_non_absolute_or_non_typo_fixes() {
        let ids = ["project/plan"];

        assert_eq!(suggest_absolute_link_typo_fix("+plan", ids), None);
        assert_eq!(
            suggest_absolute_link_typo_fix("#totally/different", ids),
            None
        );
    }
}
