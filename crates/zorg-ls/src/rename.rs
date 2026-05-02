use std::collections::HashMap;

use tower_lsp::lsp_types::{Position, PrepareRenameResponse, TextEdit, Url, WorkspaceEdit};
use zorg_core::ZettelId;

use crate::navigation::{
    DeclarationKind, LspIndex, ReferenceKind, SymbolAtPosition, ZettelReference, ZettelSymbol,
};

pub(crate) fn prepare_rename(
    index: &LspIndex,
    uri: &Url,
    position: Position,
) -> Option<PrepareRenameResponse> {
    let occurrence = rename_occurrence(index, uri, position)?;
    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: occurrence.range,
        placeholder: occurrence.target_id,
    })
}

pub(crate) fn plan_rename(
    index: &LspIndex,
    uri: &Url,
    position: Position,
    new_name: &str,
) -> Result<WorkspaceEdit, String> {
    let occurrence = rename_occurrence(index, uri, position).ok_or_else(|| {
        "rename is only available on source-backed zettel IDs and links".to_owned()
    })?;
    let declarations = index.declarations_for_id(&occurrence.target_id);
    let [declaration] = declarations.as_slice() else {
        return Err(format!(
            "rename requires exactly one declaration for `@{}`",
            occurrence.target_id
        ));
    };
    let declaration_range = declaration.declaration_range.ok_or_else(|| {
        format!(
            "declaration `@{}` has no source range",
            occurrence.target_id
        )
    })?;
    let declaration_kind = declaration
        .declaration_kind
        .ok_or_else(|| format!("declaration `@{}` is anonymous", occurrence.target_id))?;
    let new_id = normalize_new_id(new_name, &occurrence.target_id, declaration_kind)?;

    if new_id != occurrence.target_id && index.has_canonical_id(&new_id) {
        return Err(format!("target zettel ID `@{new_id}` already exists"));
    }

    let mut changes = HashMap::<Url, Vec<TextEdit>>::new();
    let declaration_text = declaration_replacement(declaration, &occurrence.target_id, &new_id)?;
    changes
        .entry(declaration.uri.clone())
        .or_default()
        .push(TextEdit::new(declaration_range, declaration_text));

    for reference in index.references_to_id(&occurrence.target_id) {
        let new_text = reference_replacement(reference, &new_id)?;
        changes
            .entry(reference.uri.clone())
            .or_default()
            .push(TextEdit::new(reference.range, new_text));
    }

    Ok(WorkspaceEdit::new(changes))
}

struct RenameOccurrence {
    target_id: String,
    range: tower_lsp::lsp_types::Range,
}

fn rename_occurrence(index: &LspIndex, uri: &Url, position: Position) -> Option<RenameOccurrence> {
    match index.symbol_at_position(uri, position)? {
        SymbolAtPosition::Declaration(declaration) => Some(RenameOccurrence {
            target_id: declaration.canonical_id.clone()?,
            range: declaration.declaration_range?,
        }),
        SymbolAtPosition::Reference(reference) => Some(RenameOccurrence {
            target_id: reference.target_id.clone(),
            range: reference.range,
        }),
    }
}

fn normalize_new_id(
    new_name: &str,
    old_id: &str,
    declaration_kind: DeclarationKind,
) -> Result<String, String> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err("new zettel ID cannot be empty".to_owned());
    }

    let canonical = if let Some(stripped) = trimmed.strip_prefix('@').or(trimmed.strip_prefix('#'))
    {
        stripped.to_owned()
    } else if let Some(stripped) = trimmed.strip_prefix('^') {
        let Some(base) = parent_id(old_id) else {
            return Err("local zettel ID rename requires an ancestor ID".to_owned());
        };
        format!("{base}/{stripped}")
    } else if declaration_kind == DeclarationKind::Local && !trimmed.contains('/') {
        let Some(base) = parent_id(old_id) else {
            return Err("local zettel ID rename requires an ancestor ID".to_owned());
        };
        format!("{base}/{trimmed}")
    } else {
        trimmed.to_owned()
    };

    ZettelId::parse_canonical(&canonical)
        .map_err(|error| format!("invalid target zettel ID `{trimmed}`: {error}"))?;
    Ok(canonical)
}

fn declaration_replacement(
    declaration: &ZettelSymbol,
    old_id: &str,
    new_id: &str,
) -> Result<String, String> {
    match declaration.declaration_kind {
        Some(DeclarationKind::Absolute) => Ok(format!("@{new_id}")),
        Some(DeclarationKind::Local) => {
            let suffix = same_parent_suffix(old_id, new_id).ok_or_else(|| {
                "local zettel ID rename must stay under the same absolute ancestor".to_owned()
            })?;
            Ok(format!("^{suffix}"))
        }
        None => Err(format!("declaration `@{old_id}` is anonymous")),
    }
}

fn reference_replacement(reference: &ZettelReference, new_id: &str) -> Result<String, String> {
    match reference.kind {
        ReferenceKind::Absolute => Ok(format!("#{new_id}")),
        ReferenceKind::Child => {
            let current = reference.containing_id.as_deref().ok_or_else(|| {
                "child-relative reference has no containing canonical ID".to_owned()
            })?;
            let suffix = direct_child_suffix(current, new_id).ok_or_else(|| {
                format!(
                    "child-relative reference to `@{}` cannot be rewritten deterministically as `+...`",
                    reference.target_id
                )
            })?;
            Ok(format!("+{suffix}"))
        }
        ReferenceKind::Sibling => {
            let base = reference.sibling_base_id.as_deref().ok_or_else(|| {
                "sibling-relative reference has no deterministic parent ID".to_owned()
            })?;
            let suffix = direct_child_suffix(base, new_id).ok_or_else(|| {
                format!(
                    "sibling-relative reference to `@{}` cannot be rewritten deterministically as `~...`",
                    reference.target_id
                )
            })?;
            Ok(format!("~{suffix}"))
        }
        ReferenceKind::Local => {
            let current = reference
                .containing_id
                .as_deref()
                .ok_or_else(|| "local reference has no containing canonical ID".to_owned())?;
            let suffix = direct_child_suffix(current, new_id).ok_or_else(|| {
                format!(
                    "local reference to `@{}` cannot be rewritten deterministically as `^...`",
                    reference.target_id
                )
            })?;
            Ok(format!("^{suffix}"))
        }
    }
}

fn same_parent_suffix(old_id: &str, new_id: &str) -> Option<String> {
    let (old_base, _) = old_id.rsplit_once('/')?;
    direct_child_suffix(old_base, new_id).map(str::to_owned)
}

fn direct_child_suffix<'a>(base: &str, canonical_id: &'a str) -> Option<&'a str> {
    let suffix = canonical_id.strip_prefix(base)?.strip_prefix('/')?;
    (!suffix.is_empty() && !suffix.contains('/')).then_some(suffix)
}

fn parent_id(canonical_id: &str) -> Option<&str> {
    canonical_id.rsplit_once('/').map(|(base, _)| base)
}
