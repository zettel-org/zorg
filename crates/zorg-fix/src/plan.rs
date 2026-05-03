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

use std::collections::BTreeSet;

use zorg_core::{
    BodyBlock, Reference, ReferenceTarget, Severity, SourcePath, SourceSpan, Zettel,
    ZettelDocument, ZettelId, ZorgError, ZorgResult,
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

/// Summary returned after applying a fix plan to source text.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ApplySummary {
    /// Rewritten source text.
    pub source: String,
    /// Number of source edits applied.
    pub applied_edits: usize,
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
    plan_zettel(&document.root, corpus_view, &mut ops);
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
    let mut edits = plan
        .ops
        .iter()
        .flat_map(|op| op.edits.iter())
        .collect::<Vec<_>>();
    edits.sort_by_key(|edit| (edit.span.start_byte, edit.span.end_byte));

    let mut previous_end = 0;
    for edit in &edits {
        if edit.span.start_byte > edit.span.end_byte || edit.span.end_byte > source.len() {
            return Err(ZorgError::OperationFailed {
                message: "fix plan contains an out-of-bounds edit".to_owned(),
            });
        }
        if edit.span.start_byte < previous_end {
            return Err(ZorgError::OperationFailed {
                message: "fix plan contains overlapping edits".to_owned(),
            });
        }
        previous_end = edit.span.end_byte;
    }

    let mut rewritten = source.to_owned();
    let mut applied_edits = 0;
    for edit in edits.into_iter().rev() {
        if &source[edit.span.start_byte..edit.span.end_byte] != edit.replacement.as_str() {
            rewritten.replace_range(edit.span.start_byte..edit.span.end_byte, &edit.replacement);
            applied_edits += 1;
        }
    }

    Ok(ApplySummary {
        source: rewritten,
        applied_edits,
    })
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
            vec![17, 52, 73]
        );
    }

    #[test]
    fn applies_plan_and_is_idempotent_after_reparse() {
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
            "\
%%% @root #z/ref area::work/research
Root
%%%

- @root/task #z/todo due::2026-05-15
"
        );
        assert_eq!(applied.applied_edits, 3);

        let reparsed = zorg_parse::parse_zettel_document_with_path(&applied.source, "tokens.z")
            .expect("reparse fixed source");
        let second = plan_fixes(&reparsed, &CorpusView::empty());
        assert!(second.is_empty(), "fixed source should not need more fixes");
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
