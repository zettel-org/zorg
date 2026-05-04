//! Shared fix-plan model used by the CLI and the LSP.
//!
//! A [`FixPlan`] is the deterministic, source-span-backed list of autofix
//! operations a single document needs. Phase 7.1 only emits the existing
//! unresolved-absolute-link typo rewrite, but the data shape and the
//! [`plan_fixes`] entry point are stable so later phases (7.2, 7.3) only need
//! to add new [`FixKind`] variants and rules behind the same surface.
//!
//! ## Invariants
//!
//! Every `FixPlan` returned by [`plan_fixes`] honors:
//!
//! - **Source-span backed** — every [`FixEdit`] points at a real byte range in
//!   the source the plan was derived from.
//! - **Idempotent planning** — calling [`plan_fixes`] twice on the same
//!   document with the same [`CorpusView`] yields equal output.
//! - **Deterministic ordering** — operations follow document source order
//!   (preorder zettel walk, then per-zettel link order).
//! - **Single-zettel scope** — no operation rewrites text outside the zettel
//!   that triggered it.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use zorg_core::{
    BodyBlock, Diagnostic, Reference, ReferenceTarget, Severity, SourcePath, SourceSpan, Zettel,
    ZettelDocument, ZettelId, ZettelKind, ZorgError, ZorgResult,
};

use crate::suggest_absolute_link_typo_fix;

/// Stable rule identifier used by both the CLI and the LSP.
pub type RuleCode = &'static str;

/// Autofix rule kinds known to the planner.
///
/// Phase 7.1 only emitted [`FixKind::UnresolvedAbsoluteLinkTypo`]. Phase 7.2
/// adds the conservative source-token normalizers while keeping downstream
/// surfaces on the same enum.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum FixKind {
    /// Rewrite an unresolved absolute link to its single one-edit candidate.
    UnresolvedAbsoluteLinkTypo,
    /// Normalize bullet-symbol glyphs to the canonical `-` marker.
    BulletSymbol,
    /// Collapse property whitespace around `::` and trailing value whitespace.
    PropertyWhitespace,
    /// Reserved for Phase 7.3: stamp a missing `@id` from canonical name.
    IdStamp,
    /// Reserved for Phase 7.3: maintain modified-date metadata.
    ModifiedStamp,
    /// Reserved for Phase 7.3: sort lines/bullets inside a SORT pragma region.
    SortPragmaRegion,
}

impl FixKind {
    /// Returns the stable rule code emitted to CLI consumers.
    #[must_use]
    pub const fn rule_code(self) -> RuleCode {
        match self {
            Self::UnresolvedAbsoluteLinkTypo => "fix.unresolved_absolute_link_typo",
            Self::BulletSymbol => "fix.bullet_symbol",
            Self::PropertyWhitespace => "fix.property_whitespace",
            Self::IdStamp => "fix.id_stamp",
            Self::ModifiedStamp => "fix.modified_stamp",
            Self::SortPragmaRegion => "fix.sort_pragma_region",
        }
    }
}

/// Returns the indexed diagnostic code that corresponds to a safe fix kind.
///
/// Source-token normalizers do not currently have parser diagnostics, so they
/// are selected by rule code and source span rather than by diagnostic code.
#[must_use]
pub const fn diagnostic_code_for_fix_kind(kind: FixKind) -> Option<&'static str> {
    match kind {
        FixKind::UnresolvedAbsoluteLinkTypo => Some("reference.unresolved_absolute"),
        FixKind::BulletSymbol
        | FixKind::PropertyWhitespace
        | FixKind::IdStamp
        | FixKind::ModifiedStamp
        | FixKind::SortPragmaRegion => None,
    }
}

const REPLACEMENT_PREVIEW_LIMIT: usize = 160;

/// One-based line/column span used when byte offsets are unavailable.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LineColumnSpan {
    /// One-based start line.
    pub start_line: usize,
    /// One-based start column.
    pub start_column: usize,
    /// One-based end line.
    pub end_line: usize,
    /// One-based end column.
    pub end_column: usize,
}

impl LineColumnSpan {
    /// Creates a line/column span.
    #[must_use]
    pub const fn new(
        start_line: usize,
        start_column: usize,
        end_line: usize,
        end_column: usize,
    ) -> Self {
        Self {
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }

    fn from_source_span(span: SourceSpan) -> Option<Self> {
        Some(Self {
            start_line: span.start_line?,
            start_column: span.start_column?,
            end_line: span.end_line?,
            end_column: span.end_column?,
        })
    }
}

/// Dashboard-facing selector for finding fix previews for one diagnostic row.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct DiagnosticFixSelector {
    /// Source path for the selected diagnostic.
    pub path: Option<SourcePath>,
    /// Indexed diagnostic code, such as `reference.unresolved_absolute`.
    pub diagnostic_code: Option<String>,
    /// Optional direct fix rule code, such as `fix.bullet_symbol`.
    pub rule_code: Option<String>,
    /// Selected diagnostic severity.
    pub severity: Option<Severity>,
    /// Selected diagnostic message.
    pub message: Option<String>,
    /// Preferred byte span for matching.
    pub byte_span: Option<(usize, usize)>,
    /// Fallback one-based line/column span for matching.
    pub line_column_span: Option<LineColumnSpan>,
}

impl DiagnosticFixSelector {
    /// Builds a selector from a shared Zorg diagnostic.
    #[must_use]
    pub fn from_diagnostic(diagnostic: &Diagnostic) -> Self {
        Self {
            path: diagnostic.path.clone(),
            diagnostic_code: diagnostic.code.clone(),
            rule_code: None,
            severity: Some(diagnostic.severity),
            message: Some(diagnostic.message.clone()),
            byte_span: diagnostic.span.map(|span| (span.start_byte, span.end_byte)),
            line_column_span: diagnostic.span.and_then(LineColumnSpan::from_source_span),
        }
    }
}

/// Summary of the selected diagnostic attached to a preview result.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DiagnosticSummary {
    /// Source path for the selected diagnostic, when known.
    pub path: Option<SourcePath>,
    /// Indexed diagnostic code, when known.
    pub diagnostic_code: Option<String>,
    /// Direct fix rule code requested by the caller, when any.
    pub rule_code: Option<String>,
    /// Selected diagnostic severity.
    pub severity: Option<Severity>,
    /// Selected diagnostic message.
    pub message: Option<String>,
    /// Preferred byte span used for matching.
    pub byte_span: Option<(usize, usize)>,
    /// Fallback line/column span used for matching.
    pub line_column_span: Option<LineColumnSpan>,
}

impl From<&DiagnosticFixSelector> for DiagnosticSummary {
    fn from(selector: &DiagnosticFixSelector) -> Self {
        Self {
            path: selector.path.clone(),
            diagnostic_code: selector.diagnostic_code.clone(),
            rule_code: selector.rule_code.clone(),
            severity: selector.severity,
            message: selector.message.clone(),
            byte_span: selector.byte_span,
            line_column_span: selector.line_column_span,
        }
    }
}

/// One safe replacement preview for a selected diagnostic.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FixPreview {
    /// Stable fix rule code.
    pub rule_code: RuleCode,
    /// Fix severity.
    pub severity: Severity,
    /// Source path for this preview.
    pub path: SourcePath,
    /// One-based primary start line.
    pub primary_line: Option<usize>,
    /// One-based primary start column.
    pub primary_column: Option<usize>,
    /// Primary source span metadata.
    pub source_span: SourceSpan,
    /// Bounded replacement text preview.
    pub replacement_preview: String,
    /// True when the preview was truncated to the fixed preview bound.
    pub replacement_truncated: bool,
    /// Whether this fix should be preferred by editor/dashboard surfaces.
    pub is_preferred: bool,
    /// Whether this preview is safe to present for explicit apply.
    pub is_safe: bool,
    /// One-line explanation for the preview and matching precision.
    pub explanation: String,
}

/// Explicit reason no safe preview is available for a selected diagnostic.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum FixUnavailableReason {
    /// Neither the selector nor plan carries a source path.
    SourcePathUnavailable,
    /// The selector has neither byte offsets nor line/column data.
    DiagnosticHasNoSourceSpan,
    /// The diagnostic is recognized, but this phase has no safe automatic fix.
    KnownUnavailable {
        /// Diagnostic code that is intentionally unavailable.
        diagnostic_code: String,
        /// Human-readable explanation.
        explanation: String,
    },
    /// No fix op matched the selector.
    NoMatchingFix,
    /// More than one fix op matched and applying one would be ambiguous.
    AmbiguousMatchingFixes {
        /// Number of matching operations.
        count: usize,
    },
}

/// Preview result for one selected diagnostic.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FixPreviewSet {
    /// Diagnostic selector summary.
    pub diagnostic: DiagnosticSummary,
    /// Zero or one safe previews for this phase.
    pub previews: Vec<FixPreview>,
    /// Structured reason when `previews` is empty.
    pub unavailable_reason: Option<FixUnavailableReason>,
}

/// One source-span-backed text replacement.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FixEdit {
    /// Source span the edit replaces.
    pub span: SourceSpan,
    /// Replacement text written in place of the span.
    pub replacement: String,
}

/// One logical autofix operation.
///
/// A `FixOp` carries the rule metadata plus the in-zettel edits required to
/// apply it. All edits must lie within the zettel that triggered the op.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FixOp {
    /// Rule kind for this operation.
    pub kind: FixKind,
    /// Stable rule code (matches `kind.rule_code()`).
    pub rule_code: RuleCode,
    /// Severity for downstream surfaces (CLI exit handling, LSP diagnostics).
    pub severity: Severity,
    /// Whether the LSP should mark the matching code action as preferred.
    pub is_preferred: bool,
    /// One-line human-readable description shown to operators.
    pub message: String,
    /// Edits to apply for this operation, in document source order.
    pub edits: Vec<FixEdit>,
}

impl FixOp {
    /// Returns the primary span this op rewrites (the first edit's span).
    #[must_use]
    pub fn primary_span(&self) -> Option<SourceSpan> {
        self.edits.first().map(|edit| edit.span)
    }
}

/// Deterministic list of autofix operations for one parsed document.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct FixPlan {
    /// Source path the plan was derived from, when known.
    pub path: Option<SourcePath>,
    /// Operations in document source order.
    pub ops: Vec<FixOp>,
}

/// Returns a bounded, read-only fix preview for a selected diagnostic.
#[must_use]
pub fn preview_diagnostic_fix(plan: &FixPlan, selector: &DiagnosticFixSelector) -> FixPreviewSet {
    let diagnostic = DiagnosticSummary::from(selector);
    let Some(path) = selector.path.clone().or_else(|| plan.path.clone()) else {
        return FixPreviewSet {
            diagnostic,
            previews: Vec::new(),
            unavailable_reason: Some(FixUnavailableReason::SourcePathUnavailable),
        };
    };

    if selector.byte_span.is_none() && selector.line_column_span.is_none() {
        return FixPreviewSet {
            diagnostic,
            previews: Vec::new(),
            unavailable_reason: Some(FixUnavailableReason::DiagnosticHasNoSourceSpan),
        };
    }

    if let Some(reason) = known_unavailable_reason(selector) {
        return FixPreviewSet {
            diagnostic,
            previews: Vec::new(),
            unavailable_reason: Some(reason),
        };
    }

    let matching = matching_ops(plan, selector);
    match matching.as_slice() {
        [] => FixPreviewSet {
            diagnostic,
            previews: Vec::new(),
            unavailable_reason: Some(FixUnavailableReason::NoMatchingFix),
        },
        [matched] => FixPreviewSet {
            diagnostic,
            previews: vec![preview_for_op(matched.op, path, matched.precision)],
            unavailable_reason: None,
        },
        _ => FixPreviewSet {
            diagnostic,
            previews: Vec::new(),
            unavailable_reason: Some(FixUnavailableReason::AmbiguousMatchingFixes {
                count: matching.len(),
            }),
        },
    }
}

/// Summary returned after applying fixes to source text.
///
/// Source-level helpers return a successful summary with rewritten source.
/// Document-level helpers also reparse and validate the rewritten source before
/// reporting success. On failure they return the original source and attach
/// either validation diagnostics or an error message.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ApplySummary {
    /// Source path affected by the operation, when known.
    pub path: Option<SourcePath>,
    /// Rewritten source text, or the original source when the apply failed.
    pub source: String,
    /// True when the returned source differs from the input source.
    pub changed: bool,
    /// Number of source edits applied.
    pub applied_edits: usize,
    /// Fix rule codes that changed source text.
    pub applied_rule_codes: Vec<RuleCode>,
    /// Strict validation diagnostics that caused an apply refusal.
    pub failure_diagnostics: Vec<Diagnostic>,
    /// Operational error text that caused an apply refusal.
    pub error: Option<String>,
}

impl ApplySummary {
    /// Returns true when the shared apply path accepted the rewritten source.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.error.is_none() && self.failure_diagnostics.is_empty()
    }

    /// Returns true when the shared apply path refused to apply the fixes.
    #[must_use]
    pub fn is_failure(&self) -> bool {
        !self.is_success()
    }
}

impl FixPlan {
    /// Returns true when the plan has no pending operations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Returns the number of pending operations.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ops.len()
    }
}

/// Corpus information required to plan fixes deterministically.
///
/// The view borrows the canonical IDs the planner is allowed to consult, so
/// callers pay no allocation cost when they already maintain a corpus index
/// (the LSP) and only pay the cost of materializing IDs when they don't.
#[derive(Debug, Default, Clone)]
pub struct CorpusView<'a> {
    canonical_ids: BTreeSet<&'a str>,
}

impl<'a> CorpusView<'a> {
    /// Returns an empty corpus view (no canonical IDs known).
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Builds a corpus view from a borrowed iterator of canonical IDs.
    pub fn from_canonical_ids<I>(ids: I) -> Self
    where
        I: IntoIterator<Item = &'a str>,
    {
        Self {
            canonical_ids: ids.into_iter().collect(),
        }
    }

    /// Returns true when the corpus knows the given canonical ID.
    #[must_use]
    pub fn contains_canonical_id(&self, id: &str) -> bool {
        self.canonical_ids.contains(id)
    }

    /// Returns the canonical IDs known to this view, in canonical order.
    pub fn canonical_ids(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.canonical_ids.iter().copied()
    }
}

/// Convenience: plan fixes for a single document with no surrounding corpus.
///
/// Equivalent to calling [`plan_fixes`] with the document's own absolute IDs
/// as the corpus view.
#[must_use]
pub fn plan_document_fixes(document: &ZettelDocument) -> FixPlan {
    let mut ids = Vec::new();
    collect_canonical_ids(&document.root, &mut ids);
    let view = CorpusView::from_canonical_ids(ids.iter().map(String::as_str));
    plan_fixes(document, &view)
}

/// Plans the deterministic list of autofix operations for a parsed document.
///
/// The planner walks zettels in source order. For each unresolved absolute
/// link it consults [`CorpusView`] for a single one-edit canonical candidate
/// and, when one exists, emits a [`FixKind::UnresolvedAbsoluteLinkTypo`] op.
///
/// This is the only entry point downstream surfaces should call; later phases
/// will add new [`FixKind`] variants to the same returned `FixPlan` without
/// changing the call site.
#[must_use]
pub fn plan_fixes(document: &ZettelDocument, corpus_view: &CorpusView<'_>) -> FixPlan {
    let mut ops = Vec::new();
    plan_source_token_fixes(document, &mut ops);
    plan_sort_pragma_fixes(document, &mut ops);
    plan_zettel(&document.root, corpus_view, &mut ops);
    plan_id_stamp_fixes(document, corpus_view, &mut ops);
    plan_modified_stamp_fixes(document, &mut ops);
    ops.sort_by_key(|op| {
        op.primary_span()
            .map(|span| (span.start_byte, span.end_byte, op.rule_code))
            .unwrap_or((usize::MAX, usize::MAX, op.rule_code))
    });
    FixPlan {
        path: document.path.clone(),
        ops,
    }
}

/// Applies every edit in a plan to the supplied source.
///
/// Edits are applied from the end of the file toward the front after validating
/// that they are in-bounds and non-overlapping. The helper is intentionally
/// source-only so the CLI and later editor surfaces can share the same edit
/// semantics.
pub fn apply_plan_to_source(source: &str, plan: &FixPlan) -> ZorgResult<ApplySummary> {
    apply_ops_to_source(source, plan.path.clone(), plan.ops.iter().collect())
}

/// Applies the single safe operation selected by a diagnostic selector.
///
/// The selector must match exactly one planned operation by rule/diagnostic code
/// and source span. This helper only rewrites source text; use
/// [`apply_selected_fix_to_document`] when the caller also needs shared reparse
/// and validation failure semantics.
pub fn apply_selected_fix_to_source(
    source: &str,
    plan: &FixPlan,
    selector: &DiagnosticFixSelector,
) -> ZorgResult<ApplySummary> {
    let matching = matching_ops(plan, selector);
    match matching.as_slice() {
        [matched] => apply_ops_to_source(source, plan.path.clone(), vec![matched.op]),
        [] => Err(ZorgError::OperationFailed {
            message: "no matching safe fix op for selector".to_owned(),
        }),
        _ => Err(ZorgError::OperationFailed {
            message: "selector matched more than one safe fix op".to_owned(),
        }),
    }
}

/// Applies one safe operation for each selector in a single file.
///
/// Selectors must each match exactly one planned operation. Duplicate matches
/// are applied once, and overlapping selected edits are rejected by the same
/// edit validator used by whole-plan application.
pub fn apply_selected_fixes_to_source(
    source: &str,
    plan: &FixPlan,
    selectors: &[DiagnosticFixSelector],
) -> ZorgResult<ApplySummary> {
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    for selector in selectors {
        let matching = matching_ops(plan, selector);
        match matching.as_slice() {
            [matched] => {
                let key = op_identity(matched.op);
                if seen.insert(key) {
                    selected.push(matched.op);
                }
            }
            [] => {
                return Err(ZorgError::OperationFailed {
                    message: "no matching safe fix op for selector".to_owned(),
                });
            }
            _ => {
                return Err(ZorgError::OperationFailed {
                    message: "selector matched more than one safe fix op".to_owned(),
                });
            }
        }
    }

    apply_ops_to_source(source, plan.path.clone(), selected)
}

/// Applies all planned safe fixes for one document and validates the rewrite.
#[must_use]
pub fn apply_fix_plan_to_document(document: &ZettelDocument, plan: &FixPlan) -> ApplySummary {
    apply_fix_plans_to_documents(&[document.clone()], &[plan.clone()])
        .into_iter()
        .next()
        .expect("single document returns one summary")
}

/// Applies one selected safe fix for a document and validates the rewrite.
#[must_use]
pub fn apply_selected_fix_to_document(
    document: &ZettelDocument,
    plan: &FixPlan,
    selector: &DiagnosticFixSelector,
) -> ApplySummary {
    match apply_selected_fix_to_source(&document.source, plan, selector) {
        Ok(summary) => validate_single_rewrite(document, summary),
        Err(error) => failed_summary(document, Some(error.to_string()), Vec::new()),
    }
}

/// Applies a selected set of safe fixes for a document and validates the rewrite.
#[must_use]
pub fn apply_selected_fixes_to_document(
    document: &ZettelDocument,
    plan: &FixPlan,
    selectors: &[DiagnosticFixSelector],
) -> ApplySummary {
    match apply_selected_fixes_to_source(&document.source, plan, selectors) {
        Ok(summary) => validate_single_rewrite(document, summary),
        Err(error) => failed_summary(document, Some(error.to_string()), Vec::new()),
    }
}

/// Applies whole-file plans for a corpus and validates rewritten documents.
#[must_use]
pub fn apply_fix_plans_to_documents(
    documents: &[ZettelDocument],
    plans: &[FixPlan],
) -> Vec<ApplySummary> {
    apply_fix_plans_to_documents_with_validator(documents, plans, validate_rewritten_documents)
}

/// Applies whole-file plans for a corpus with caller-supplied validation.
///
/// The caller-owned validator receives reparsed rewritten documents. Returning
/// any error-severity diagnostic refuses the batch and every returned summary
/// contains the original source. This lets downstream surfaces share selection,
/// edit application, and failure semantics while preserving their own strict
/// validation extensions.
#[must_use]
pub fn apply_fix_plans_to_documents_with_validator<F>(
    documents: &[ZettelDocument],
    plans: &[FixPlan],
    mut validator: F,
) -> Vec<ApplySummary>
where
    F: FnMut(&mut [ZettelDocument]) -> Vec<Diagnostic>,
{
    if documents.len() != plans.len() {
        return failure_summaries(
            documents,
            Some("document and fix plan counts differ".to_owned()),
            Vec::new(),
        );
    }

    let mut applied = Vec::with_capacity(documents.len());
    for (document, plan) in documents.iter().zip(plans) {
        match apply_plan_to_source(&document.source, plan) {
            Ok(summary) => applied.push(summary),
            Err(error) => {
                return failure_summaries(documents, Some(error.to_string()), Vec::new());
            }
        }
    }

    let mut rewritten = Vec::with_capacity(documents.len());
    for (document, summary) in documents.iter().zip(&applied) {
        let Some(path) = document.path.as_ref() else {
            return failure_summaries(
                documents,
                Some("cannot validate a rewritten document without a source path".to_owned()),
                Vec::new(),
            );
        };
        match zorg_parse::parse_zettel_document_with_path(&summary.source, path.as_path()) {
            Ok(document) => rewritten.push(document),
            Err(error) => {
                return failure_summaries(documents, Some(error.to_string()), Vec::new());
            }
        }
    }

    let diagnostics = validator(&mut rewritten);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return failure_summaries(
            documents,
            Some("rewritten sources failed strict validation".to_owned()),
            diagnostics,
        );
    }

    applied
}

/// Default strict validation used by shared document apply helpers.
#[must_use]
pub fn validate_rewritten_documents(documents: &mut [ZettelDocument]) -> Vec<Diagnostic> {
    let validation = zorg_parse::validate_corpus(documents);
    let resolution = zorg_parse::resolve_corpus(documents);
    let mut diagnostics = validation.diagnostics;
    diagnostics.extend(resolution.diagnostics);
    diagnostics
}

#[derive(Debug, Clone, Copy)]
struct SelectedEdit<'a> {
    op: &'a FixOp,
    edit: &'a FixEdit,
}

fn apply_ops_to_source(
    source: &str,
    path: Option<SourcePath>,
    ops: Vec<&FixOp>,
) -> ZorgResult<ApplySummary> {
    let mut edits = ops
        .iter()
        .flat_map(|op| op.edits.iter().map(|edit| SelectedEdit { op, edit }))
        .collect::<Vec<_>>();
    edits.sort_by_key(|selected| (selected.edit.span.start_byte, selected.edit.span.end_byte));

    let mut previous_end = 0;
    for selected in &edits {
        let span = selected.edit.span;
        let range = span.start_byte..span.end_byte;
        if span.start_byte > span.end_byte || span.end_byte > source.len() {
            return Err(ZorgError::OperationFailed {
                message: "fix plan contains an out-of-bounds edit".to_owned(),
            });
        }
        if source.get(range).is_none() {
            return Err(ZorgError::OperationFailed {
                message: "fix plan contains an edit that is not on UTF-8 boundaries".to_owned(),
            });
        }
        if span.start_byte < previous_end {
            return Err(ZorgError::OperationFailed {
                message: "fix plan contains overlapping edits".to_owned(),
            });
        }
        previous_end = span.end_byte;
    }

    let mut rewritten = source.to_owned();
    let mut applied_edits = 0;
    let mut applied_rule_codes = BTreeSet::new();
    for selected in edits.into_iter().rev() {
        let span = selected.edit.span;
        let original = &source[span.start_byte..span.end_byte];
        if original != selected.edit.replacement.as_str() {
            rewritten.replace_range(span.start_byte..span.end_byte, &selected.edit.replacement);
            applied_edits += 1;
            applied_rule_codes.insert(selected.op.rule_code);
        }
    }

    let changed = rewritten != source;
    Ok(ApplySummary {
        path,
        source: rewritten,
        changed,
        applied_edits,
        applied_rule_codes: applied_rule_codes.into_iter().collect(),
        failure_diagnostics: Vec::new(),
        error: None,
    })
}

fn validate_single_rewrite(document: &ZettelDocument, summary: ApplySummary) -> ApplySummary {
    let Some(path) = document.path.as_ref() else {
        return failed_summary(
            document,
            Some("cannot validate a rewritten document without a source path".to_owned()),
            Vec::new(),
        );
    };
    let mut rewritten =
        match zorg_parse::parse_zettel_document_with_path(&summary.source, path.as_path()) {
            Ok(document) => vec![document],
            Err(error) => return failed_summary(document, Some(error.to_string()), Vec::new()),
        };
    let diagnostics = validate_rewritten_documents(&mut rewritten);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return failed_summary(
            document,
            Some("rewritten source failed strict validation".to_owned()),
            diagnostics,
        );
    }

    summary
}

fn failure_summaries(
    documents: &[ZettelDocument],
    error: Option<String>,
    diagnostics: Vec<Diagnostic>,
) -> Vec<ApplySummary> {
    documents
        .iter()
        .map(|document| failed_summary(document, error.clone(), diagnostics.clone()))
        .collect()
}

fn failed_summary(
    document: &ZettelDocument,
    error: Option<String>,
    diagnostics: Vec<Diagnostic>,
) -> ApplySummary {
    ApplySummary {
        path: document.path.clone(),
        source: document.source.clone(),
        changed: false,
        applied_edits: 0,
        applied_rule_codes: Vec::new(),
        failure_diagnostics: diagnostics,
        error,
    }
}

fn op_identity(op: &FixOp) -> (RuleCode, Option<(usize, usize)>) {
    (
        op.rule_code,
        op.primary_span()
            .map(|span| (span.start_byte, span.end_byte)),
    )
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum MatchPrecision {
    ByteSpan,
    LineColumn,
}

#[derive(Debug, Clone, Copy)]
struct MatchedOp<'a> {
    op: &'a FixOp,
    precision: MatchPrecision,
}

fn matching_ops<'a>(plan: &'a FixPlan, selector: &DiagnosticFixSelector) -> Vec<MatchedOp<'a>> {
    if let (Some(selector_path), Some(plan_path)) = (&selector.path, &plan.path)
        && selector_path != plan_path
    {
        return Vec::new();
    }

    plan.ops
        .iter()
        .filter(|op| selector_matches_rule(selector, op))
        .filter_map(|op| {
            matching_precision(selector, op).map(|precision| MatchedOp { op, precision })
        })
        .collect()
}

fn selector_matches_rule(selector: &DiagnosticFixSelector, op: &FixOp) -> bool {
    if let Some(rule_code) = selector.rule_code.as_deref() {
        return op.rule_code == rule_code;
    }

    if let Some(diagnostic_code) = selector.diagnostic_code.as_deref() {
        return diagnostic_code_for_fix_kind(op.kind)
            .is_some_and(|expected| diagnostic_code == expected);
    }

    true
}

fn matching_precision(selector: &DiagnosticFixSelector, op: &FixOp) -> Option<MatchPrecision> {
    let span = op.primary_span()?;
    if let Some((start_byte, end_byte)) = selector.byte_span {
        if spans_overlap(start_byte, end_byte, span.start_byte, span.end_byte) {
            return Some(MatchPrecision::ByteSpan);
        }
        return None;
    }

    let selector_span = selector.line_column_span?;
    let op_span = LineColumnSpan::from_source_span(span)?;
    line_column_spans_overlap(selector_span, op_span).then_some(MatchPrecision::LineColumn)
}

fn spans_overlap(left_start: usize, left_end: usize, right_start: usize, right_end: usize) -> bool {
    if left_start == left_end || right_start == right_end {
        left_start == right_start && left_end == right_end
    } else {
        left_start < right_end && right_start < left_end
    }
}

fn line_column_spans_overlap(left: LineColumnSpan, right: LineColumnSpan) -> bool {
    let left_start = (left.start_line, left.start_column);
    let left_end = (left.end_line, left.end_column);
    let right_start = (right.start_line, right.start_column);
    let right_end = (right.end_line, right.end_column);

    if left_start == left_end || right_start == right_end {
        left_start == right_start && left_end == right_end
    } else {
        left_start < right_end && right_start < left_end
    }
}

fn known_unavailable_reason(selector: &DiagnosticFixSelector) -> Option<FixUnavailableReason> {
    let diagnostic_code = selector.diagnostic_code.as_deref()?;
    let explanation = match diagnostic_code {
        "reference.unresolved_child" => {
            "Child links require local context; no safe automatic fix is available."
        }
        "reference.unresolved_sibling" => {
            "Sibling links require local context; no safe automatic fix is available."
        }
        "reference.unresolved_local" => {
            "Local declarations require local context; no safe automatic fix is available."
        }
        "reference.ambiguous" => {
            "Ambiguous references cannot be fixed without choosing an intended target."
        }
        _ => return None,
    };

    Some(FixUnavailableReason::KnownUnavailable {
        diagnostic_code: diagnostic_code.to_owned(),
        explanation: explanation.to_owned(),
    })
}

fn preview_for_op(op: &FixOp, path: SourcePath, precision: MatchPrecision) -> FixPreview {
    let span = op.primary_span().expect("matched ops have a primary span");
    let (replacement_preview, replacement_truncated) =
        bounded_replacement_preview(op.edits.iter().map(|edit| edit.replacement.as_str()));
    let explanation = match precision {
        MatchPrecision::ByteSpan => op.message.clone(),
        MatchPrecision::LineColumn => {
            format!("{} (matched by line/column fallback)", op.message)
        }
    };

    FixPreview {
        rule_code: op.rule_code,
        severity: op.severity,
        path,
        primary_line: span.start_line,
        primary_column: span.start_column,
        source_span: span,
        replacement_preview,
        replacement_truncated,
        is_preferred: op.is_preferred,
        is_safe: true,
        explanation,
    }
}

fn bounded_replacement_preview<'a>(
    replacements: impl IntoIterator<Item = &'a str>,
) -> (String, bool) {
    let raw = replacements.into_iter().collect::<Vec<_>>().join("\n---\n");
    let total_chars = raw.chars().count();
    if total_chars <= REPLACEMENT_PREVIEW_LIMIT {
        return (raw, false);
    }

    let preview = raw
        .chars()
        .take(REPLACEMENT_PREVIEW_LIMIT)
        .collect::<String>();
    (format!("{preview}..."), true)
}

fn plan_source_token_fixes(document: &ZettelDocument, ops: &mut Vec<FixOp>) {
    let mut line_start = 0;
    let mut in_fence = false;

    for line in document.source.split_inclusive('\n') {
        let line_end = line_start + line.len();
        let content_end = line_start + line.trim_end_matches(['\r', '\n']).len();
        let content = &document.source[line_start..content_end];

        if is_fence_line(content) {
            in_fence = !in_fence;
            line_start = line_end;
            continue;
        }

        if !in_fence {
            if let Some(op) = plan_bullet_symbol_fix(&document.source, line_start, content) {
                ops.push(op);
            }
            plan_property_whitespace_fixes(&document.source, line_start, content, ops);
        }

        line_start = line_end;
    }

    if line_start < document.source.len() {
        let content = &document.source[line_start..];
        if !in_fence {
            if let Some(op) = plan_bullet_symbol_fix(&document.source, line_start, content) {
                ops.push(op);
            }
            plan_property_whitespace_fixes(&document.source, line_start, content, ops);
        }
    }
}

fn plan_sort_pragma_fixes(document: &ZettelDocument, ops: &mut Vec<FixOp>) {
    let mut line_start = 0;
    let mut in_fence = false;
    let mut region_start = None;

    for line in document.source.split_inclusive('\n') {
        let line_end = line_start + line.len();
        let content_end = line_start + line.trim_end_matches(['\r', '\n']).len();
        let content = &document.source[line_start..content_end];
        let trimmed = content.trim();

        if is_fence_line(content) {
            in_fence = !in_fence;
            line_start = line_end;
            continue;
        }

        if !in_fence {
            match trimmed {
                "zorg-sort:start" if region_start.is_none() => {
                    region_start = Some(line_end);
                }
                "zorg-sort:end" => {
                    if let Some(start) = region_start.take() {
                        if let Some(op) = plan_sort_region(document, start, line_start) {
                            ops.push(op);
                        }
                    }
                }
                _ => {}
            }
        }

        line_start = line_end;
    }
}

fn plan_sort_region(document: &ZettelDocument, start: usize, end: usize) -> Option<FixOp> {
    let original = &document.source[start..end];
    let replacement = sorted_region(original)?;
    if replacement == original {
        return None;
    }

    let span = SourceSpan::from_offsets(&document.source, start, end);
    let kind = FixKind::SortPragmaRegion;
    Some(FixOp {
        kind,
        rule_code: kind.rule_code(),
        severity: Severity::Warning,
        is_preferred: true,
        message: "Sort SORT pragma region".to_owned(),
        edits: vec![FixEdit { span, replacement }],
    })
}

fn sorted_region(region: &str) -> Option<String> {
    let mut lines = region.split_inclusive('\n').collect::<Vec<_>>();
    if lines.len() < 2 {
        return None;
    }

    let mut sortable = Vec::new();
    let mut expected = None;
    for (index, line) in lines.iter().enumerate() {
        let content = line.trim_end_matches(['\r', '\n']);
        if content.trim().is_empty() {
            continue;
        }
        let shape = SortLineShape::for_line(content)?;
        if let Some(expected) = expected {
            if expected != shape {
                return None;
            }
        } else {
            expected = Some(shape);
        }
        sortable.push((index, sort_key(content, shape), *line));
    }

    if sortable.len() < 2 {
        return None;
    }

    let mut sorted = sortable.clone();
    sorted.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.2.cmp(right.2)));
    if sorted
        .iter()
        .map(|(_, _, line)| *line)
        .eq(sortable.iter().map(|(_, _, line)| *line))
    {
        return None;
    }

    for ((index, _, _), (_, _, line)) in sortable.into_iter().zip(sorted) {
        lines[index] = line;
    }

    Some(lines.concat())
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum SortLineShape {
    Plain,
    Bullet { indent: usize },
}

impl SortLineShape {
    fn for_line(line: &str) -> Option<Self> {
        let indent = line
            .char_indices()
            .find_map(|(index, character)| (!matches!(character, ' ' | '\t')).then_some(index))
            .unwrap_or(line.len());
        let rest = &line[indent..];
        let marker = rest.chars().next()?;
        if matches!(marker, '-' | '*' | '+' | '•') {
            let marker_end = marker.len_utf8();
            if rest[marker_end..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
            {
                return Some(Self::Bullet { indent });
            }
        }
        Some(Self::Plain)
    }
}

fn sort_key(line: &str, shape: SortLineShape) -> String {
    match shape {
        SortLineShape::Plain => line.trim().to_owned(),
        SortLineShape::Bullet { indent } => {
            let rest = &line[indent..];
            let marker_len = rest.chars().next().map(char::len_utf8).unwrap_or(0);
            rest[marker_len..].trim().to_owned()
        }
    }
}

fn is_fence_line(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

fn plan_bullet_symbol_fix(source: &str, line_start: usize, line: &str) -> Option<FixOp> {
    let marker_start = line
        .char_indices()
        .find_map(|(index, character)| (!matches!(character, ' ' | '\t')).then_some(index))?;
    let marker = line[marker_start..].chars().next()?;
    if !matches!(marker, '*' | '+' | '•') {
        return None;
    }

    let marker_end = marker_start + marker.len_utf8();
    if line[marker_end..]
        .chars()
        .next()
        .is_some_and(|character| !character.is_whitespace())
    {
        return None;
    }

    let span = SourceSpan::from_offsets(source, line_start + marker_start, line_start + marker_end);
    let kind = FixKind::BulletSymbol;
    Some(FixOp {
        kind,
        rule_code: kind.rule_code(),
        severity: Severity::Warning,
        is_preferred: true,
        message: "Normalize list bullet marker to -".to_owned(),
        edits: vec![FixEdit {
            span,
            replacement: "-".to_owned(),
        }],
    })
}

fn plan_property_whitespace_fixes(
    source: &str,
    line_start: usize,
    line: &str,
    ops: &mut Vec<FixOp>,
) {
    let bytes = line.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let Some(key_start) = find_property_key_start(bytes, index) else {
            break;
        };
        let key_end = scan_property_key(bytes, key_start);
        let mut cursor = key_end;
        cursor = skip_horizontal_space(bytes, cursor);

        if bytes.get(cursor..cursor + 2) != Some(b"::") {
            index = key_end.max(index + 1);
            continue;
        }
        cursor += 2;
        cursor = skip_horizontal_space(bytes, cursor);

        let value_start = cursor;
        while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if value_start == cursor {
            index = cursor.max(index + 1);
            continue;
        }

        let key = &line[key_start..key_end];
        if matches!(key, "ID" | "LID" | "tick") {
            index = cursor;
            continue;
        }

        let mut replacement_end = cursor;
        if line[cursor..].trim().is_empty() {
            replacement_end = line.len();
        }

        let replacement = format!("{key}::{}", &line[value_start..cursor]);
        let original = &line[key_start..replacement_end];
        if original != replacement {
            let span = SourceSpan::from_offsets(
                source,
                line_start + key_start,
                line_start + replacement_end,
            );
            let kind = FixKind::PropertyWhitespace;
            ops.push(FixOp {
                kind,
                rule_code: kind.rule_code(),
                severity: Severity::Warning,
                is_preferred: true,
                message: format!("Normalize whitespace for {key} property"),
                edits: vec![FixEdit { span, replacement }],
            });
        }

        index = cursor;
    }
}

fn find_property_key_start(bytes: &[u8], mut index: usize) -> Option<usize> {
    while index < bytes.len() {
        if is_property_key_initial(bytes[index])
            && (index == 0 || is_property_boundary_before(bytes[index - 1]))
        {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn scan_property_key(bytes: &[u8], mut index: usize) -> usize {
    index += 1;
    while index < bytes.len() && is_property_key_continue(bytes[index]) {
        index += 1;
    }
    index
}

fn skip_horizontal_space(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && matches!(bytes[index], b' ' | b'\t') {
        index += 1;
    }
    index
}

fn is_property_key_initial(byte: u8) -> bool {
    byte.is_ascii_alphabetic()
}

fn is_property_key_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

fn is_property_boundary_before(byte: u8) -> bool {
    byte.is_ascii_whitespace() || matches!(byte, b'%' | b'-' | b'*' | b'+')
}

fn plan_zettel(zettel: &Zettel, corpus_view: &CorpusView<'_>, ops: &mut Vec<FixOp>) {
    for link in &zettel.links {
        if let Some(op) = plan_unresolved_absolute_link_typo(link, corpus_view) {
            ops.push(op);
        }
    }

    for child in child_zettels(zettel) {
        plan_zettel(child, corpus_view, ops);
    }
}

fn plan_id_stamp_fixes(
    document: &ZettelDocument,
    corpus_view: &CorpusView<'_>,
    ops: &mut Vec<FixOp>,
) {
    plan_id_stamp_for_zettel(document, &document.root, corpus_view, ops);
}

fn plan_id_stamp_for_zettel(
    document: &ZettelDocument,
    zettel: &Zettel,
    corpus_view: &CorpusView<'_>,
    ops: &mut Vec<FixOp>,
) {
    if zettel.id.is_none() {
        if let Some(op) = plan_local_id_stamp(document, zettel) {
            ops.push(op);
        } else if let Some(op) = plan_path_id_stamp(document, zettel, corpus_view) {
            ops.push(op);
        }
    }

    for child in child_zettels(zettel) {
        plan_id_stamp_for_zettel(document, child, corpus_view, ops);
    }
}

fn plan_local_id_stamp(document: &ZettelDocument, zettel: &Zettel) -> Option<FixOp> {
    let local_id = zettel.local_id.as_ref()?;
    let canonical_id = zettel.canonical_id.as_ref()?;
    let (line_start, line_end) = opening_line_bounds(document, zettel)?;
    let line = &document.source[line_start..line_end];
    let token = local_id.declaration();
    let relative_start = find_token(line, &token)?;
    let start = line_start + relative_start;
    let end = start + token.len();
    let replacement = canonical_id.declaration();

    let span = SourceSpan::from_offsets(&document.source, start, end);
    let kind = FixKind::IdStamp;
    Some(FixOp {
        kind,
        rule_code: kind.rule_code(),
        severity: Severity::Warning,
        is_preferred: true,
        message: format!("Stamp missing absolute ID as {replacement}"),
        edits: vec![FixEdit { span, replacement }],
    })
}

fn plan_path_id_stamp(
    document: &ZettelDocument,
    zettel: &Zettel,
    corpus_view: &CorpusView<'_>,
) -> Option<FixOp> {
    if !matches!(zettel.kind, ZettelKind::File | ZettelKind::Directory) || zettel.local_id.is_some()
    {
        return None;
    }
    let path = document.path.as_ref()?.as_path();
    let canonical = path_derived_id(path)?;
    if corpus_view.contains_canonical_id(&canonical) {
        return None;
    }
    let (line_start, line_end) = opening_line_bounds(document, zettel)?;
    let line = &document.source[line_start..line_end];
    let insert_at = header_insert_offset(line).map(|offset| line_start + offset)?;
    let replacement = format!(" @{}", canonical);

    let span = SourceSpan::from_offsets(&document.source, insert_at, insert_at);
    let kind = FixKind::IdStamp;
    Some(FixOp {
        kind,
        rule_code: kind.rule_code(),
        severity: Severity::Warning,
        is_preferred: true,
        message: format!("Stamp missing absolute ID as @{canonical}"),
        edits: vec![FixEdit { span, replacement }],
    })
}

fn path_derived_id(path: &Path) -> Option<String> {
    let raw = if path.file_name().is_some_and(|name| name == "init.z") {
        path.parent()?.file_name()?.to_str()?
    } else {
        path.file_stem()?.to_str()?
    };
    ZettelId::parse_canonical(raw).ok()?;
    Some(raw.to_owned())
}

fn plan_modified_stamp_fixes(document: &ZettelDocument, ops: &mut Vec<FixOp>) {
    let date = current_utc_date();
    let mut modified_ops = Vec::new();
    plan_modified_stamp_for_zettel(document, &document.root, ops, &date, &mut modified_ops);
    ops.extend(modified_ops);
}

fn plan_modified_stamp_for_zettel(
    document: &ZettelDocument,
    zettel: &Zettel,
    existing_ops: &[FixOp],
    date: &str,
    modified_ops: &mut Vec<FixOp>,
) {
    if zettel_has_content_edit(document, zettel, existing_ops)
        && let Some(op) = plan_modified_stamp_for_edited_zettel(document, zettel, date)
    {
        modified_ops.push(op);
    }

    for child in child_zettels(zettel) {
        plan_modified_stamp_for_zettel(document, child, existing_ops, date, modified_ops);
    }
}

fn zettel_has_content_edit(
    document: &ZettelDocument,
    zettel: &Zettel,
    existing_ops: &[FixOp],
) -> bool {
    let Some((start, end)) = zettel_bounds(document, zettel) else {
        return false;
    };

    existing_ops.iter().any(|op| {
        op.kind != FixKind::ModifiedStamp
            && op.primary_span().is_some_and(|span| {
                span.start_byte >= start
                    && span.start_byte <= end
                    && !child_contains_byte(zettel, span.start_byte)
            })
    })
}

fn child_contains_byte(zettel: &Zettel, byte: usize) -> bool {
    child_zettels(zettel).any(|child| {
        child
            .span
            .is_some_and(|span| byte >= span.start_byte && byte <= span.end_byte)
            || child_contains_byte(child, byte)
    })
}

fn plan_modified_stamp_for_edited_zettel(
    document: &ZettelDocument,
    zettel: &Zettel,
    date: &str,
) -> Option<FixOp> {
    if let Some(property) = zettel
        .properties
        .iter()
        .find(|property| property.key == "modified")
    {
        if property.value == date {
            return None;
        }
        let span = property.value_span.or(property.span)?;
        let replacement = if property.value_span.is_some() {
            date.to_owned()
        } else {
            format!("modified::{date}")
        };
        let kind = FixKind::ModifiedStamp;
        return Some(FixOp {
            kind,
            rule_code: kind.rule_code(),
            severity: Severity::Warning,
            is_preferred: true,
            message: format!("Update modified stamp to {date}"),
            edits: vec![FixEdit { span, replacement }],
        });
    }

    let (line_start, line_end) = opening_line_bounds(document, zettel)?;
    let line = &document.source[line_start..line_end];
    let offset = metadata_insert_offset(line, zettel).or_else(|| {
        matches!(zettel.kind, ZettelKind::File | ZettelKind::Directory)
            .then(|| header_insert_offset(line))
            .flatten()
    })?;
    let insert_at = line_start + offset;
    let replacement = format!(" modified::{date}");
    let span = SourceSpan::from_offsets(&document.source, insert_at, insert_at);
    let kind = FixKind::ModifiedStamp;
    Some(FixOp {
        kind,
        rule_code: kind.rule_code(),
        severity: Severity::Warning,
        is_preferred: true,
        message: format!("Add modified stamp {date}"),
        edits: vec![FixEdit { span, replacement }],
    })
}

fn opening_line_bounds(document: &ZettelDocument, zettel: &Zettel) -> Option<(usize, usize)> {
    let start = zettel.span?.start_byte;
    let relative_end = document.source[start..]
        .find('\n')
        .map(|offset| start + offset)
        .unwrap_or(document.source.len());
    Some((start, relative_end))
}

fn zettel_bounds(document: &ZettelDocument, zettel: &Zettel) -> Option<(usize, usize)> {
    if zettel.key.as_str() == document.root.key.as_str() {
        return Some((0, document.source.len()));
    }
    let span = zettel.span?;
    Some((span.start_byte, span.end_byte))
}

fn metadata_insert_offset(line: &str, zettel: &Zettel) -> Option<usize> {
    if let Some(id) = zettel.id.as_ref() {
        let token = id.declaration();
        return find_token(line, &token).map(|start| start + token.len());
    }
    if let Some(local_id) = zettel.local_id.as_ref() {
        let token = local_id.declaration();
        return find_token(line, &token).map(|start| start + token.len());
    }
    None
}

fn header_insert_offset(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("%%%") {
        return Some(line.len() - trimmed.len() + 3);
    }

    let marker_start = line
        .char_indices()
        .find_map(|(index, character)| (!matches!(character, ' ' | '\t')).then_some(index))?;
    let marker = line[marker_start..].chars().next()?;
    if marker == '-' {
        return Some(marker_start + marker.len_utf8());
    }
    None
}

fn find_token(line: &str, token: &str) -> Option<usize> {
    let mut search_start = 0;
    while let Some(relative) = line[search_start..].find(token) {
        let start = search_start + relative;
        let end = start + token.len();
        let before = start == 0
            || line[..start]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        let after =
            end == line.len() || line[end..].chars().next().is_some_and(char::is_whitespace);
        if before && after {
            return Some(start);
        }
        search_start = end;
    }
    None
}

fn current_utc_date() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() / 86_400)
        .unwrap_or(0);
    let (year, month, day) = civil_date_from_unix_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_date_from_unix_days(days_since_epoch: i64) -> (i32, u8, u8) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }

    (year as i32, month as u8, day as u8)
}

fn plan_unresolved_absolute_link_typo(
    link: &Reference,
    corpus_view: &CorpusView<'_>,
) -> Option<FixOp> {
    let ReferenceTarget::Absolute(id) = &link.target else {
        return None;
    };
    if corpus_view.contains_canonical_id(id.as_str()) {
        return None;
    }
    let span = link.span?;
    let replacement = suggest_absolute_link_typo_fix(&link.raw, corpus_view.canonical_ids())?;

    let kind = FixKind::UnresolvedAbsoluteLinkTypo;
    Some(FixOp {
        kind,
        rule_code: kind.rule_code(),
        severity: Severity::Error,
        is_preferred: true,
        message: format!("Rewrite unresolved link to {replacement}"),
        edits: vec![FixEdit { span, replacement }],
    })
}

fn collect_canonical_ids(zettel: &Zettel, ids: &mut Vec<String>) {
    if let Some(canonical) = zettel
        .canonical_id
        .as_ref()
        .or(zettel.id.as_ref())
        .map(ZettelId::as_str)
    {
        ids.push(canonical.to_owned());
    }

    for child in child_zettels(zettel) {
        collect_canonical_ids(child, ids);
    }
}

fn child_zettels(zettel: &Zettel) -> impl Iterator<Item = &Zettel> {
    zettel.body.iter().filter_map(|block| match block {
        BodyBlock::ChildZettel(child) => Some(child.as_ref()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zorg_core::{
        BodyBlock, Paragraph, Reference, ReferenceTarget, SourcePath, SourceSpan, Zettel,
        ZettelDocument, ZettelId, ZettelKey, ZettelKind,
    };

    fn make_document_with_unresolved_links() -> ZettelDocument {
        let mut root = Zettel::new(ZettelKey::new("root"), ZettelKind::File);
        root.path = Some(SourcePath::new("links.z"));
        root.id = Some(ZettelId::parse("@links").expect("valid root ID"));
        root.canonical_id = root.id.clone();

        let source = "%%% @links #z/ref\nLinks\n%%%\n\nSee #poject/plan and +nope and #project/plan and #poject/plan again.\n";

        let mention_a_start = source.find("#poject/plan").expect("link a");
        let mention_a_end = mention_a_start + "#poject/plan".len();
        let mention_b_start = source.rfind("#poject/plan").expect("link b");
        let mention_b_end = mention_b_start + "#poject/plan".len();
        let resolved_start = source.find("#project/plan").expect("resolved link");
        let resolved_end = resolved_start + "#project/plan".len();
        let child_link_start = source.find("+nope").expect("child link");
        let child_link_end = child_link_start + "+nope".len();

        root.links.push(Reference {
            target: ReferenceTarget::parse("#poject/plan").expect("ref a"),
            raw: "#poject/plan".to_owned(),
            span: Some(SourceSpan::from_offsets(
                source,
                mention_a_start,
                mention_a_end,
            )),
        });
        root.links.push(Reference {
            target: ReferenceTarget::parse("+nope").expect("child"),
            raw: "+nope".to_owned(),
            span: Some(SourceSpan::from_offsets(
                source,
                child_link_start,
                child_link_end,
            )),
        });
        root.links.push(Reference {
            target: ReferenceTarget::parse("#project/plan").expect("known"),
            raw: "#project/plan".to_owned(),
            span: Some(SourceSpan::from_offsets(
                source,
                resolved_start,
                resolved_end,
            )),
        });
        root.links.push(Reference {
            target: ReferenceTarget::parse("#poject/plan").expect("ref b"),
            raw: "#poject/plan".to_owned(),
            span: Some(SourceSpan::from_offsets(
                source,
                mention_b_start,
                mention_b_end,
            )),
        });

        root.body.push(BodyBlock::Paragraph(Paragraph {
            text: source.to_owned(),
            links: root.links.clone(),
            span: None,
        }));

        ZettelDocument {
            path: Some(SourcePath::new("links.z")),
            source: source.to_owned(),
            root,
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn fix_kind_rule_codes_are_stable_and_unique() {
        let kinds = [
            FixKind::UnresolvedAbsoluteLinkTypo,
            FixKind::BulletSymbol,
            FixKind::PropertyWhitespace,
            FixKind::IdStamp,
            FixKind::ModifiedStamp,
            FixKind::SortPragmaRegion,
        ];
        let mut codes = kinds
            .iter()
            .map(|kind| kind.rule_code())
            .collect::<Vec<_>>();
        let original = codes.clone();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(
            codes.len(),
            original.len(),
            "rule codes must be unique across FixKind variants"
        );
        for code in original {
            assert!(
                code.starts_with("fix."),
                "rule codes must use the `fix.` namespace: {code}"
            );
        }
    }

    #[test]
    fn plans_typo_fix_for_unresolved_absolute_link_with_single_candidate() {
        let document = make_document_with_unresolved_links();
        let view = CorpusView::from_canonical_ids(["project/plan", "project/review"]);
        let plan = plan_fixes(&document, &view);

        assert_eq!(
            plan.path
                .as_ref()
                .expect("path")
                .as_path()
                .to_string_lossy(),
            "links.z"
        );
        assert_eq!(plan.len(), 2);
        for op in &plan.ops {
            assert_eq!(op.kind, FixKind::UnresolvedAbsoluteLinkTypo);
            assert_eq!(op.rule_code, "fix.unresolved_absolute_link_typo");
            assert_eq!(op.severity, Severity::Error);
            assert!(op.is_preferred);
            assert_eq!(op.message, "Rewrite unresolved link to #project/plan");
            assert_eq!(op.edits.len(), 1);
            assert_eq!(op.edits[0].replacement, "#project/plan");
        }

        let spans = plan
            .ops
            .iter()
            .filter_map(|op| op.primary_span().map(|span| span.start_byte))
            .collect::<Vec<_>>();
        assert!(spans.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn plan_fixes_is_idempotent() {
        let document = make_document_with_unresolved_links();
        let view = CorpusView::from_canonical_ids(["project/plan", "project/review"]);
        let first = plan_fixes(&document, &view);
        let second = plan_fixes(&document, &view);
        assert_eq!(first, second);
    }

    #[test]
    fn plan_fixes_skips_when_corpus_already_contains_target() {
        let document = make_document_with_unresolved_links();
        let view = CorpusView::from_canonical_ids(["poject/plan", "project/plan"]);
        let plan = plan_fixes(&document, &view);
        assert!(
            plan.is_empty(),
            "no fix expected when target already exists"
        );
    }

    #[test]
    fn plan_fixes_returns_empty_for_ambiguous_typos() {
        let document = make_document_with_unresolved_links();
        let view = CorpusView::from_canonical_ids(["project/plan", "poject/play"]);
        let plan = plan_fixes(&document, &view);
        assert!(plan.is_empty(), "ambiguous candidates should not auto-fix");
    }

    #[test]
    fn plan_document_fixes_uses_document_canonical_ids() {
        let document = make_document_with_unresolved_links();
        let plan = plan_document_fixes(&document);
        assert!(
            plan.is_empty(),
            "single-document planning should not invent corpus-only candidates"
        );
    }

    #[test]
    fn previews_unresolved_absolute_link_fix_by_byte_span() {
        let document = make_document_with_unresolved_links();
        let view = CorpusView::from_canonical_ids(["project/plan", "project/review"]);
        let plan = plan_fixes(&document, &view);
        let span = plan.ops[0].primary_span().expect("primary span");

        let preview_set = preview_diagnostic_fix(
            &plan,
            &DiagnosticFixSelector {
                path: document.path.clone(),
                diagnostic_code: Some("reference.unresolved_absolute".to_owned()),
                severity: Some(Severity::Error),
                message: Some("unresolved link".to_owned()),
                byte_span: Some((span.start_byte, span.end_byte)),
                line_column_span: LineColumnSpan::from_source_span(span),
                ..DiagnosticFixSelector::default()
            },
        );

        assert_eq!(preview_set.unavailable_reason, None);
        assert_eq!(preview_set.previews.len(), 1);
        let preview = &preview_set.previews[0];
        assert_eq!(preview.rule_code, "fix.unresolved_absolute_link_typo");
        assert_eq!(preview.severity, Severity::Error);
        assert_eq!(preview.path.as_path().to_string_lossy(), "links.z");
        assert_eq!(preview.source_span, span);
        assert_eq!(preview.replacement_preview, "#project/plan");
        assert!(!preview.replacement_truncated);
        assert!(preview.is_preferred);
        assert!(preview.is_safe);
        assert_eq!(
            preview.explanation,
            "Rewrite unresolved link to #project/plan"
        );
    }

    #[test]
    fn previews_source_token_normalizer_by_rule_and_span() {
        let source = "\
%%% @root #z/ref
Root
%%%

+ @root/task #z/todo Task
";
        let document = zorg_parse::parse_zettel_document_with_path(source, "tokens.z")
            .expect("parse source token fixture");
        let plan = plan_fixes(&document, &CorpusView::empty());
        let bullet = plan
            .ops
            .iter()
            .find(|op| op.kind == FixKind::BulletSymbol)
            .expect("bullet fix");
        let span = bullet.primary_span().expect("primary span");

        let preview_set = preview_diagnostic_fix(
            &plan,
            &DiagnosticFixSelector {
                path: document.path.clone(),
                rule_code: Some("fix.bullet_symbol".to_owned()),
                byte_span: Some((span.start_byte, span.end_byte)),
                line_column_span: LineColumnSpan::from_source_span(span),
                ..DiagnosticFixSelector::default()
            },
        );

        assert_eq!(preview_set.unavailable_reason, None);
        assert_eq!(preview_set.previews.len(), 1);
        assert_eq!(preview_set.previews[0].rule_code, "fix.bullet_symbol");
        assert_eq!(preview_set.previews[0].replacement_preview, "-");
    }

    #[test]
    fn preview_reports_no_matching_fix_for_noop_selection() {
        let plan = FixPlan {
            path: Some(SourcePath::new("noop.z")),
            ops: Vec::new(),
        };
        let preview_set = preview_diagnostic_fix(
            &plan,
            &DiagnosticFixSelector {
                path: Some(SourcePath::new("noop.z")),
                diagnostic_code: Some("reference.unresolved_absolute".to_owned()),
                byte_span: Some((0, 4)),
                ..DiagnosticFixSelector::default()
            },
        );

        assert_eq!(
            preview_set.unavailable_reason,
            Some(FixUnavailableReason::NoMatchingFix)
        );
        assert!(preview_set.previews.is_empty());
    }

    #[test]
    fn preview_reports_ambiguous_line_column_matches() {
        let document = make_document_with_unresolved_links();
        let view = CorpusView::from_canonical_ids(["project/plan", "project/review"]);
        let plan = plan_fixes(&document, &view);
        assert_eq!(plan.ops.len(), 2);
        let line = plan.ops[0]
            .primary_span()
            .and_then(|span| span.start_line)
            .expect("line");

        let preview_set = preview_diagnostic_fix(
            &plan,
            &DiagnosticFixSelector {
                path: document.path.clone(),
                diagnostic_code: Some("reference.unresolved_absolute".to_owned()),
                line_column_span: Some(LineColumnSpan::new(line, 1, line, 200)),
                ..DiagnosticFixSelector::default()
            },
        );

        assert_eq!(
            preview_set.unavailable_reason,
            Some(FixUnavailableReason::AmbiguousMatchingFixes { count: 2 })
        );
        assert!(preview_set.previews.is_empty());
    }

    #[test]
    fn preview_supports_line_column_fallback() {
        let source = "\
%%% @root #z/ref
Root
%%%

+ @root/task #z/todo Task
";
        let document = zorg_parse::parse_zettel_document_with_path(source, "tokens.z")
            .expect("parse source token fixture");
        let plan = plan_fixes(&document, &CorpusView::empty());
        let bullet = plan
            .ops
            .iter()
            .find(|op| op.kind == FixKind::BulletSymbol)
            .expect("bullet fix");
        let span = bullet.primary_span().expect("primary span");

        let preview_set = preview_diagnostic_fix(
            &plan,
            &DiagnosticFixSelector {
                path: document.path.clone(),
                rule_code: Some("fix.bullet_symbol".to_owned()),
                line_column_span: LineColumnSpan::from_source_span(span),
                ..DiagnosticFixSelector::default()
            },
        );

        assert_eq!(preview_set.unavailable_reason, None);
        assert_eq!(preview_set.previews.len(), 1);
        assert!(
            preview_set.previews[0]
                .explanation
                .contains("line/column fallback")
        );
    }

    #[test]
    fn preview_truncates_replacement_text_deterministically() {
        let span = SourceSpan::from_offsets("x", 0, 1);
        let kind = FixKind::PropertyWhitespace;
        let plan = FixPlan {
            path: Some(SourcePath::new("long.z")),
            ops: vec![FixOp {
                kind,
                rule_code: kind.rule_code(),
                severity: Severity::Warning,
                is_preferred: true,
                message: "long replacement".to_owned(),
                edits: vec![FixEdit {
                    span,
                    replacement: "a".repeat(200),
                }],
            }],
        };

        let preview_set = preview_diagnostic_fix(
            &plan,
            &DiagnosticFixSelector {
                path: Some(SourcePath::new("long.z")),
                rule_code: Some("fix.property_whitespace".to_owned()),
                byte_span: Some((0, 1)),
                ..DiagnosticFixSelector::default()
            },
        );

        let preview = &preview_set.previews[0];
        assert!(preview.replacement_truncated);
        assert_eq!(preview.replacement_preview.chars().count(), 163);
        assert!(preview.replacement_preview.ends_with("..."));
    }

    #[test]
    fn preview_reports_spanless_and_pathless_selectors() {
        let plan = FixPlan {
            path: Some(SourcePath::new("spanless.z")),
            ops: Vec::new(),
        };
        let spanless = preview_diagnostic_fix(
            &plan,
            &DiagnosticFixSelector {
                path: Some(SourcePath::new("spanless.z")),
                diagnostic_code: Some("reference.unresolved_absolute".to_owned()),
                ..DiagnosticFixSelector::default()
            },
        );
        assert_eq!(
            spanless.unavailable_reason,
            Some(FixUnavailableReason::DiagnosticHasNoSourceSpan)
        );

        let pathless = preview_diagnostic_fix(
            &FixPlan::default(),
            &DiagnosticFixSelector {
                diagnostic_code: Some("reference.unresolved_absolute".to_owned()),
                byte_span: Some((0, 1)),
                ..DiagnosticFixSelector::default()
            },
        );
        assert_eq!(
            pathless.unavailable_reason,
            Some(FixUnavailableReason::SourcePathUnavailable)
        );
    }

    #[test]
    fn preview_reports_known_unavailable_diagnostic_codes() {
        let preview_set = preview_diagnostic_fix(
            &FixPlan {
                path: Some(SourcePath::new("child.z")),
                ops: Vec::new(),
            },
            &DiagnosticFixSelector {
                path: Some(SourcePath::new("child.z")),
                diagnostic_code: Some("reference.unresolved_child".to_owned()),
                byte_span: Some((5, 11)),
                ..DiagnosticFixSelector::default()
            },
        );

        assert_eq!(
            preview_set.unavailable_reason,
            Some(FixUnavailableReason::KnownUnavailable {
                diagnostic_code: "reference.unresolved_child".to_owned(),
                explanation:
                    "Child links require local context; no safe automatic fix is available."
                        .to_owned(),
            })
        );
    }

    #[test]
    fn plans_source_token_normalizers_in_source_order() {
        let source = "\
%%% @root #z/ref area :: work/research   
Root
%%%

* @root/task #z/todo due :: 2026-05-15   

```zorg-template
* {{title}} due :: unchanged   
```
";
        let document = zorg_parse::parse_zettel_document_with_path(source, "tokens.z")
            .expect("parse source with recoverable tokens");
        let plan = plan_fixes(&document, &CorpusView::empty());

        let rule_codes = plan.ops.iter().map(|op| op.rule_code).collect::<Vec<_>>();
        assert_eq!(
            rule_codes,
            vec![
                "fix.modified_stamp",
                "fix.property_whitespace",
                "fix.bullet_symbol",
                "fix.property_whitespace"
            ]
        );
        assert_eq!(
            plan.ops
                .iter()
                .filter_map(FixOp::primary_span)
                .map(|span| span.start_byte)
                .collect::<Vec<_>>(),
            vec![9, 17, 52, 73]
        );
    }

    #[test]
    fn applies_plan_and_is_idempotent_after_reparse() {
        let date = current_utc_date();
        let source = "\
%%% @root #z/ref area :: work/research   
Root
%%%

+ @root/task #z/todo due :: 2026-05-15   
";
        let document = zorg_parse::parse_zettel_document_with_path(source, "tokens.z")
            .expect("parse source with recoverable tokens");
        let plan = plan_fixes(&document, &CorpusView::empty());
        let applied = apply_plan_to_source(source, &plan).expect("apply plan");

        assert_eq!(
            applied.source,
            format!(
                "\
%%% @root modified::{date} #z/ref area::work/research
Root
%%%

- @root/task #z/todo due::2026-05-15
"
            )
        );
        assert_eq!(applied.applied_edits, 4);

        let reparsed = zorg_parse::parse_zettel_document_with_path(&applied.source, "tokens.z")
            .expect("reparse fixed source");
        let second = plan_fixes(&reparsed, &CorpusView::empty());
        assert!(second.is_empty(), "fixed source should not need more fixes");
    }

    #[test]
    fn applies_selected_fix_without_touching_other_spans() {
        let source = "\
+ @root/task #z/todo due :: 2026-05-15
+ @root/other #z/todo Other
";
        let document = zorg_parse::parse_zettel_document_with_path(source, "selected.z")
            .expect("parse selected source");
        let plan = plan_fixes(&document, &CorpusView::empty());
        let bullet = plan
            .ops
            .iter()
            .find(|op| op.kind == FixKind::BulletSymbol)
            .expect("bullet fix");
        let span = bullet.primary_span().expect("bullet span");

        let applied = apply_selected_fix_to_source(
            source,
            &plan,
            &DiagnosticFixSelector {
                path: document.path.clone(),
                rule_code: Some("fix.bullet_symbol".to_owned()),
                byte_span: Some((span.start_byte, span.end_byte)),
                line_column_span: LineColumnSpan::from_source_span(span),
                ..DiagnosticFixSelector::default()
            },
        )
        .expect("apply selected bullet");

        assert_eq!(
            applied.source,
            "\
- @root/task #z/todo due :: 2026-05-15
+ @root/other #z/todo Other
"
        );
        assert_eq!(applied.applied_edits, 1);
        assert_eq!(applied.applied_rule_codes, vec!["fix.bullet_symbol"]);
        assert!(applied.changed);
    }

    #[test]
    fn applies_selected_set_for_one_file() {
        let source = "\
+ @root/task #z/todo due :: 2026-05-15
";
        let document = zorg_parse::parse_zettel_document_with_path(source, "selected-set.z")
            .expect("parse selected set source");
        let plan = plan_fixes(&document, &CorpusView::empty());
        let selectors = plan
            .ops
            .iter()
            .filter(|op| matches!(op.kind, FixKind::BulletSymbol | FixKind::PropertyWhitespace))
            .map(|op| {
                let span = op.primary_span().expect("selected span");
                DiagnosticFixSelector {
                    path: document.path.clone(),
                    rule_code: Some(op.rule_code.to_owned()),
                    byte_span: Some((span.start_byte, span.end_byte)),
                    line_column_span: LineColumnSpan::from_source_span(span),
                    ..DiagnosticFixSelector::default()
                }
            })
            .collect::<Vec<_>>();

        let applied =
            apply_selected_fixes_to_source(source, &plan, &selectors).expect("apply selected set");

        assert_eq!(applied.source, "- @root/task #z/todo due::2026-05-15\n");
        assert_eq!(applied.applied_edits, 2);
        assert_eq!(
            applied.applied_rule_codes,
            vec!["fix.bullet_symbol", "fix.property_whitespace"]
        );
    }

    #[test]
    fn document_apply_validation_failure_returns_original_source() {
        let source = "\
+ @root/task #z/todo Task
";
        let document = zorg_parse::parse_zettel_document_with_path(source, "failure.z")
            .expect("parse failure source");
        let plan = plan_fixes(&document, &CorpusView::empty());
        let failure = Diagnostic::semantic_validation(
            "fixture.failure",
            "synthetic validation failure",
            None,
        );

        let summaries = apply_fix_plans_to_documents_with_validator(
            std::slice::from_ref(&document),
            std::slice::from_ref(&plan),
            |_| vec![failure.clone()],
        );

        assert_eq!(summaries.len(), 1);
        let summary = &summaries[0];
        assert!(summary.is_failure());
        assert_eq!(summary.source, source);
        assert!(!summary.changed);
        assert_eq!(summary.applied_edits, 0);
        assert_eq!(summary.failure_diagnostics, vec![failure]);
        assert_eq!(
            summary.error.as_deref(),
            Some("rewritten sources failed strict validation")
        );
    }

    #[test]
    fn plans_path_id_modified_stamp_and_sort_pragma_region() {
        let date = current_utc_date();
        let source = "\
%%% #z/ref
Stamping fixture
%%%

zorg-sort:start
- beta
- alpha
zorg-sort:end
";
        let document = zorg_parse::parse_zettel_document_with_path(source, "stamp-root.z")
            .expect("parse stamp source");
        let plan = plan_fixes(&document, &CorpusView::empty());
        let rule_codes = plan.ops.iter().map(|op| op.rule_code).collect::<Vec<_>>();

        assert_eq!(
            rule_codes,
            vec![
                "fix.id_stamp",
                "fix.modified_stamp",
                "fix.sort_pragma_region"
            ]
        );

        let applied = apply_plan_to_source(source, &plan).expect("apply stamp plan");
        assert_eq!(
            applied.source,
            format!(
                "\
%%% @stamp-root modified::{date} #z/ref
Stamping fixture
%%%

zorg-sort:start
- alpha
- beta
zorg-sort:end
"
            )
        );
    }

    #[test]
    fn plans_local_id_stamp_from_resolved_canonical_id() {
        let date = current_utc_date();
        let source = "\
%%% @root #z/ref
Root
%%%

- ^child #z/ref Child.
";
        let mut document = zorg_parse::parse_zettel_document_with_path(source, "local-stamp.z")
            .expect("parse local stamp source");
        zorg_parse::resolve_document(&mut document);

        let plan = plan_fixes(&document, &CorpusView::empty());
        let rule_codes = plan.ops.iter().map(|op| op.rule_code).collect::<Vec<_>>();
        assert_eq!(rule_codes, vec!["fix.id_stamp", "fix.modified_stamp"]);

        let applied = apply_plan_to_source(source, &plan).expect("apply local stamp");
        assert_eq!(
            applied.source,
            format!(
                "\
%%% @root #z/ref
Root
%%%

- @root/child modified::{date} #z/ref Child.
"
            )
        );
    }

    #[test]
    fn sort_pragma_declines_mixed_or_nested_regions() {
        let source = "\
%%% @sort-skip #z/ref
Sort skip
%%%

zorg-sort:start
- beta
  - alpha
zorg-sort:end
";
        let document = zorg_parse::parse_zettel_document_with_path(source, "sort-skip.z")
            .expect("parse sort skip source");
        let plan = plan_fixes(&document, &CorpusView::empty());

        assert!(plan.is_empty());
    }

    #[test]
    fn apply_plan_rejects_overlapping_edits() {
        let span = SourceSpan::from_offsets("abcdef", 1, 4);
        let overlap = SourceSpan::from_offsets("abcdef", 3, 5);
        let kind = FixKind::PropertyWhitespace;
        let plan = FixPlan {
            path: None,
            ops: vec![
                FixOp {
                    kind,
                    rule_code: kind.rule_code(),
                    severity: Severity::Warning,
                    is_preferred: true,
                    message: "first".to_owned(),
                    edits: vec![FixEdit {
                        span,
                        replacement: "x".to_owned(),
                    }],
                },
                FixOp {
                    kind,
                    rule_code: kind.rule_code(),
                    severity: Severity::Warning,
                    is_preferred: true,
                    message: "second".to_owned(),
                    edits: vec![FixEdit {
                        span: overlap,
                        replacement: "y".to_owned(),
                    }],
                },
            ],
        };

        assert!(apply_plan_to_source("abcdef", &plan).is_err());
    }
}
