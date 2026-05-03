use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    Diagnostic, NumberOrString, Position, Range, TextEdit, Url, WorkspaceEdit,
};
use zorg_core::SourceSpan;
use zorg_fix::{CorpusView, FixKind, FixOp, plan_fixes};

use crate::navigation::LspIndex;

pub(crate) fn code_actions(
    index: Option<&LspIndex>,
    uri: &Url,
    text: Option<&str>,
    params: &CodeActionParams,
) -> CodeActionResponse {
    let Some(index) = index else {
        return Vec::new();
    };
    let Some(text) = text else {
        return Vec::new();
    };
    if !is_z_document(uri) {
        return Vec::new();
    }

    let document_path = match uri.to_file_path() {
        Ok(path) => path,
        Err(_) => return Vec::new(),
    };
    let Ok(document) = zorg_parse::parse_zettel_document_with_path(text, document_path) else {
        return Vec::new();
    };
    let canonical_ids = index.canonical_ids();
    let corpus = CorpusView::from_canonical_ids(canonical_ids);
    let plan = plan_fixes(&document, &corpus);

    plan.ops
        .iter()
        .filter_map(|op| code_action_for_op(uri, op, params))
        .collect()
}

fn code_action_for_op(
    uri: &Url,
    op: &FixOp,
    params: &CodeActionParams,
) -> Option<CodeActionOrCommand> {
    let primary_span = op.primary_span()?;
    let primary_range = source_span_to_range(primary_span)?;
    if !ranges_intersect(params.range, primary_range) {
        return None;
    }

    let diagnostic_code = diagnostic_code_for_kind(op.kind);
    let matched_diagnostic = params
        .context
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic_matches(diagnostic, diagnostic_code, primary_range))
        .cloned();

    let mut text_edits = Vec::with_capacity(op.edits.len());
    for edit in &op.edits {
        let range = source_span_to_range(edit.span)?;
        text_edits.push(TextEdit::new(range, edit.replacement.clone()));
    }

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), text_edits);

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: op.message.clone(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: matched_diagnostic.map(|diagnostic| vec![diagnostic]),
        edit: Some(WorkspaceEdit::new(changes)),
        command: None,
        is_preferred: Some(op.is_preferred),
        disabled: None,
        data: None,
    }))
}

fn diagnostic_code_for_kind(kind: FixKind) -> Option<&'static str> {
    match kind {
        FixKind::UnresolvedAbsoluteLinkTypo => Some("reference.unresolved_absolute"),
        FixKind::BulletSymbol
        | FixKind::PropertyWhitespace
        | FixKind::IdStamp
        | FixKind::ModifiedStamp
        | FixKind::SortPragmaRegion => None,
    }
}

fn diagnostic_matches(diagnostic: &Diagnostic, code: Option<&str>, range: Range) -> bool {
    let Some(expected) = code else {
        return false;
    };
    match diagnostic.code.as_ref() {
        Some(NumberOrString::String(actual)) => actual == expected && diagnostic.range == range,
        _ => false,
    }
}

fn is_z_document(uri: &Url) -> bool {
    uri.to_file_path()
        .ok()
        .and_then(|path| path.extension().map(|extension| extension == "z"))
        .unwrap_or(false)
}

fn ranges_intersect(left: Range, right: Range) -> bool {
    compare_position(left.start, right.end) < std::cmp::Ordering::Equal
        && compare_position(right.start, left.end) < std::cmp::Ordering::Equal
}

fn source_span_to_range(span: SourceSpan) -> Option<Range> {
    Some(Range {
        start: position(span.start_line?, span.start_column?),
        end: position(span.end_line?, span.end_column?),
    })
}

fn position(line: usize, column: usize) -> Position {
    Position::new(
        u32::try_from(line.saturating_sub(1)).unwrap_or(u32::MAX),
        u32::try_from(column.saturating_sub(1)).unwrap_or(u32::MAX),
    )
}

fn compare_position(left: Position, right: Position) -> std::cmp::Ordering {
    (left.line, left.character).cmp(&(right.line, right.character))
}
