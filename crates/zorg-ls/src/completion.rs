use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, Position, Range, TextEdit, Url,
};

use crate::navigation::{CompletionCandidate, LspIndex};

pub(crate) fn completion_items(
    index: &LspIndex,
    uri: &Url,
    position: Position,
    live_text: Option<&str>,
    trigger_character: Option<&str>,
) -> Vec<CompletionItem> {
    let Some(context) = live_text
        .and_then(|text| completion_context(text, position))
        .or_else(|| trigger_character.and_then(|trigger| trigger_context(trigger, position)))
    else {
        return Vec::new();
    };

    let candidates = match context.marker {
        '#' => {
            let mut candidates = index.absolute_link_completions(&context.typed_prefix);
            candidates.extend(index.tag_completions(&context.typed_prefix));
            candidates
        }
        '+' => index.child_link_completions(uri, position, &context.typed_prefix),
        '~' => index.sibling_link_completions(uri, position, &context.typed_prefix),
        _ => Vec::new(),
    };

    candidates
        .into_iter()
        .map(|candidate| completion_item(candidate, context.range, context.marker))
        .collect()
}

#[derive(Debug, Clone)]
struct CompletionContext {
    marker: char,
    typed_prefix: String,
    range: Range,
}

fn completion_context(text: &str, position: Position) -> Option<CompletionContext> {
    let line = text.lines().nth(usize::try_from(position.line).ok()?)?;
    let end_byte = char_to_byte_index(line, usize::try_from(position.character).ok()?)?;
    let prefix = &line[..end_byte];
    let token_start = token_start_byte(prefix);
    let token = &prefix[token_start..];
    let (marker_byte, marker) = token
        .char_indices()
        .find(|(_, character)| matches!(character, '#' | '+' | '~'))?;
    let typed_prefix = token[marker_byte + marker.len_utf8()..].to_owned();
    let start_character = prefix[..token_start + marker_byte].chars().count();

    Some(CompletionContext {
        marker,
        typed_prefix,
        range: Range {
            start: Position::new(position.line, u32::try_from(start_character).ok()?),
            end: position,
        },
    })
}

fn trigger_context(trigger: &str, position: Position) -> Option<CompletionContext> {
    let marker = match trigger {
        "#" => '#',
        "+" => '+',
        "~" => '~',
        "/" => return None,
        _ => return None,
    };

    Some(CompletionContext {
        marker,
        typed_prefix: String::new(),
        range: Range {
            start: position,
            end: position,
        },
    })
}

fn completion_item(candidate: CompletionCandidate, range: Range, marker: char) -> CompletionItem {
    let kind = if marker == '#' && candidate.is_tag {
        CompletionItemKind::KEYWORD
    } else {
        CompletionItemKind::REFERENCE
    };

    CompletionItem {
        label: candidate.label,
        kind: Some(kind),
        detail: Some(candidate.detail),
        sort_text: Some(candidate.sort_text),
        insert_text: Some(candidate.insert_text.clone()),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
            range,
            new_text: candidate.insert_text,
        })),
        ..CompletionItem::default()
    }
}

fn char_to_byte_index(text: &str, character: usize) -> Option<usize> {
    if character == text.chars().count() {
        return Some(text.len());
    }
    text.char_indices().nth(character).map(|(index, _)| index)
}

fn token_start_byte(prefix: &str) -> usize {
    prefix
        .char_indices()
        .rev()
        .find(|(_, character)| {
            character.is_whitespace()
                || matches!(
                    character,
                    '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'' | ',' | ';'
                )
        })
        .map(|(index, character)| index + character.len_utf8())
        .unwrap_or(0)
}
