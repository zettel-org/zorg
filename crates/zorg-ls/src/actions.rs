use std::collections::HashMap;
use std::path::Path;

use serde_json::json;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse, Command,
    CreateFile, CreateFileOptions, Diagnostic, DocumentChangeOperation, DocumentChanges,
    NumberOrString, OneOf, OptionalVersionedTextDocumentIdentifier, Position, Range, ResourceOp,
    TextDocumentEdit, TextEdit, Url, WorkspaceEdit,
};
use zorg_core::SourceSpan;
use zorg_fix::{CorpusView, FixOp, diagnostic_code_for_fix_kind, plan_fixes};
use zorg_refactor::{
    ExtractRange, PromoteRequest, RefactorFilePlan, RefactorMode, RefactorPlan,
    validate_extract_selection,
};
use zorg_store::StoreOptions;

use crate::navigation::LspIndex;

pub(crate) fn code_actions(
    index: Option<&LspIndex>,
    root_path: &Path,
    database_path: &Path,
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

    let mut actions = plan
        .ops
        .iter()
        .filter_map(|op| code_action_for_op(uri, op, params))
        .collect::<Vec<_>>();

    if wants_kind(params, &CodeActionKind::REFACTOR_REWRITE) {
        actions.extend(promote_code_action(
            index,
            root_path,
            database_path,
            uri,
            params,
        ));
    }
    if wants_kind(params, &CodeActionKind::REFACTOR_EXTRACT) {
        actions.extend(extract_code_action(uri, text, &document, params));
    }

    actions
}

fn code_action_for_op(
    uri: &Url,
    op: &FixOp,
    params: &CodeActionParams,
) -> Option<CodeActionOrCommand> {
    if !wants_kind(params, &CodeActionKind::QUICKFIX) {
        return None;
    }

    let primary_span = op.primary_span()?;
    let primary_range = source_span_to_range(primary_span)?;
    if !ranges_intersect(params.range, primary_range) {
        return None;
    }

    let diagnostic_code = diagnostic_code_for_fix_kind(op.kind);
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

fn promote_code_action(
    index: &LspIndex,
    root_path: &Path,
    database_path: &Path,
    uri: &Url,
    params: &CodeActionParams,
) -> Option<CodeActionOrCommand> {
    let canonical_id = index.promotable_nested_id_at_range(uri, params.range)?;
    let store_options = StoreOptions::new(root_path, database_path).ok()?;
    let request = PromoteRequest {
        store_options,
        id: format!("@{canonical_id}"),
        mode: RefactorMode::Preview,
        destination: None,
    };
    let plan = zorg_refactor::plan_promote(&request).ok()?;
    let edit = workspace_edit_for_plan(&plan)?;

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Promote @{canonical_id} to file zettel"),
        kind: Some(CodeActionKind::REFACTOR_REWRITE),
        diagnostics: None,
        edit: Some(edit),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    }))
}

fn extract_code_action(
    uri: &Url,
    text: &str,
    document: &zorg_core::ZettelDocument,
    params: &CodeActionParams,
) -> Option<CodeActionOrCommand> {
    if params.range.start == params.range.end {
        return None;
    }
    let file = uri.to_file_path().ok()?;
    let extract_range = extract_range_from_lsp(params.range)?;
    validate_extract_selection(text, document, extract_range, false).ok()?;
    let range_arg = format_extract_range(extract_range);
    let file_arg = file.display().to_string();

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: "Extract selection with zorg extract".to_owned(),
        kind: Some(CodeActionKind::REFACTOR_EXTRACT),
        diagnostics: None,
        edit: None,
        command: Some(Command {
            title: "Extract selection with zorg extract".to_owned(),
            command: "zorg.extract.preview".to_owned(),
            arguments: Some(vec![json!({
                "argv": [
                    "extract",
                    "--file",
                    file_arg,
                    "--range",
                    range_arg,
                    "--id",
                    "@new/id",
                    "--json"
                ],
                "idPlaceholder": "@new/id",
                "uri": uri,
                "range": params.range
            })]),
        }),
        is_preferred: Some(false),
        disabled: None,
        data: None,
    }))
}

fn workspace_edit_for_plan(plan: &RefactorPlan) -> Option<WorkspaceEdit> {
    let mut operations = Vec::new();
    for file in &plan.files {
        let uri = Url::from_file_path(&file.absolute_path).ok()?;
        if file.original_guard.is_empty_file() {
            operations.push(DocumentChangeOperation::Op(ResourceOp::Create(
                CreateFile {
                    uri: uri.clone(),
                    options: Some(CreateFileOptions {
                        overwrite: Some(false),
                        ignore_if_exists: Some(false),
                    }),
                    annotation_id: None,
                },
            )));
        }
        operations.push(DocumentChangeOperation::Edit(text_document_edit(
            uri, file,
        )?));
    }

    Some(WorkspaceEdit {
        changes: None,
        document_changes: Some(DocumentChanges::Operations(operations)),
        change_annotations: None,
    })
}

fn text_document_edit(uri: Url, file: &RefactorFilePlan) -> Option<TextDocumentEdit> {
    Some(TextDocumentEdit {
        text_document: OptionalVersionedTextDocumentIdentifier { uri, version: None },
        edits: file
            .edits
            .iter()
            .map(|edit| {
                Some(OneOf::Left(TextEdit::new(
                    source_span_to_range(edit.span)?,
                    edit.replacement.clone(),
                )))
            })
            .collect::<Option<Vec<_>>>()?,
    })
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
    if span.start_byte == 0
        && span.end_byte == 0
        && span.start_line.is_none()
        && span.start_column.is_none()
        && span.end_line.is_none()
        && span.end_column.is_none()
    {
        return Some(Range::new(Position::new(0, 0), Position::new(0, 0)));
    }

    Some(Range::new(
        position(span.start_line?, span.start_column?),
        position(span.end_line?, span.end_column?),
    ))
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

fn wants_kind(params: &CodeActionParams, action_kind: &CodeActionKind) -> bool {
    params.context.only.as_ref().is_none_or(|requested| {
        requested
            .iter()
            .any(|candidate| kind_matches(candidate, action_kind))
    })
}

fn kind_matches(requested: &CodeActionKind, action_kind: &CodeActionKind) -> bool {
    let requested = requested.as_str();
    let action = action_kind.as_str();
    action == requested
        || action
            .strip_prefix(requested)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

fn extract_range_from_lsp(range: Range) -> Option<ExtractRange> {
    Some(ExtractRange::LineColumn {
        start_line: usize::try_from(range.start.line).ok()?.checked_add(1)?,
        start_column: usize::try_from(range.start.character)
            .ok()?
            .checked_add(1)?,
        end_line: usize::try_from(range.end.line).ok()?.checked_add(1)?,
        end_column: usize::try_from(range.end.character).ok()?.checked_add(1)?,
    })
}

fn format_extract_range(range: ExtractRange) -> String {
    match range {
        ExtractRange::LineColumn {
            start_line,
            start_column,
            end_line,
            end_column,
        } => format!("{start_line}:{start_column}-{end_line}:{end_column}"),
        ExtractRange::Bytes { start, end } => format!("{start}..{end}"),
    }
}
