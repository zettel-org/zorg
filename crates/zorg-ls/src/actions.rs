use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    Diagnostic, NumberOrString, Position, Range, TextEdit, Url, WorkspaceEdit,
};

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

    params
        .context
        .diagnostics
        .iter()
        .filter(|diagnostic| is_unresolved_absolute_link(diagnostic))
        .filter(|diagnostic| ranges_intersect(params.range, diagnostic.range))
        .filter_map(|diagnostic| unresolved_absolute_link_action(index, uri, text, diagnostic))
        .collect()
}

fn unresolved_absolute_link_action(
    index: &LspIndex,
    uri: &Url,
    text: &str,
    diagnostic: &Diagnostic,
) -> Option<CodeActionOrCommand> {
    let raw_link = text_for_range(text, diagnostic.range)?;
    let replacement = zorg_fix::suggest_absolute_link_typo_fix(&raw_link, index.canonical_ids())?;
    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit::new(diagnostic.range, replacement.clone())],
    );

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Rewrite unresolved link to {replacement}"),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit::new(changes)),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    }))
}

fn is_unresolved_absolute_link(diagnostic: &Diagnostic) -> bool {
    diagnostic.code.as_ref().is_some_and(|code| match code {
        NumberOrString::String(code) => code == "reference.unresolved_absolute",
        NumberOrString::Number(_) => false,
    })
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

fn text_for_range(text: &str, range: Range) -> Option<String> {
    let start = offset_for_position(text, range.start)?;
    let end = offset_for_position(text, range.end)?;
    (start < end).then(|| text[start..end].to_owned())
}

fn offset_for_position(text: &str, position: Position) -> Option<usize> {
    let target_line = usize::try_from(position.line).ok()?;
    let target_character = usize::try_from(position.character).ok()?;
    let mut line = 0;
    let mut character = 0;

    for (offset, value) in text.char_indices() {
        if line == target_line && character == target_character {
            return Some(offset);
        }
        if value == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }

    (line == target_line && character == target_character).then_some(text.len())
}

fn compare_position(left: Position, right: Position) -> std::cmp::Ordering {
    (left.line, left.character).cmp(&(right.line, right.character))
}
