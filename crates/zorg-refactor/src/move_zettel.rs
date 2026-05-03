use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use zorg_core::{BodyBlock, SourceSpan, Zettel, ZettelId, ZettelKind, ZorgError, ZorgResult};
use zorg_store::{Store, StoreOptions};

use crate::promote::{
    canonical_root, destination_path, find_zettel_by_canonical_id, normalize_path,
    promoted_file_source, source_removal_span, zettel_subtree_span,
};
use crate::{
    RefactorEdit, RefactorFilePlan, RefactorMode, RefactorPlan, SourceGuard, apply_edits_to_source,
    load_indexed_sources, operation_failed, source_slice, validate_and_sort_file_edits,
};

/// Destination accepted by `zorg move`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MoveDestination {
    /// Move a file zettel, or promote a nested zettel, to this `.z` file path.
    Path(PathBuf),
    /// Move a nested zettel under another zettel.
    ParentId(String),
}

/// Options for moving a zettel.
#[derive(Debug, Clone)]
pub struct MoveRequest {
    /// Store options that identify the indexed corpus.
    pub store_options: StoreOptions,
    /// Target zettel ID declaration, such as `@project/plan`.
    pub id: String,
    /// Refactor mode for the returned plan.
    pub mode: RefactorMode,
    /// Destination file path or parent zettel.
    pub destination: MoveDestination,
}

/// Plans a conservative structural zettel move.
pub fn plan_move(request: &MoveRequest) -> ZorgResult<RefactorPlan> {
    let canonical_id = ZettelId::parse(&request.id)?.as_str().to_owned();
    let store = Store::open_with_options(request.store_options.clone())?;
    let target = crate::resolve_exact_canonical_zettel(&store, &canonical_id)?;
    let root = canonical_root(request.store_options.corpus_root())?;

    let loaded = load_indexed_sources(request.store_options.clone())?;
    let mut documents = loaded
        .iter()
        .map(|source| source.document.clone())
        .collect::<Vec<_>>();
    zorg_parse::resolve_corpus(&mut documents);

    let source_index = loaded
        .iter()
        .position(|source| source.file.id == target.file_id)
        .ok_or_else(|| {
            operation_failed(format!(
                "indexed zettel `@{canonical_id}` references missing source file row {}",
                target.file_id
            ))
        })?;
    let source = &loaded[source_index];
    let document = &documents[source_index];
    let (target_zettel, current_parent) =
        find_zettel_and_parent_by_canonical_id(&document.root, &canonical_id, None).ok_or_else(
            || {
                operation_failed(format!(
                    "could not find reparsed source for indexed zettel `@{canonical_id}`"
                ))
            },
        )?;

    match &request.destination {
        MoveDestination::Path(path) => plan_move_to_path(
            request.mode,
            &root,
            &canonical_id,
            target_zettel,
            source,
            path,
            &loaded,
        ),
        MoveDestination::ParentId(parent_id) => plan_move_to_parent(
            request.mode,
            &store,
            &root,
            &canonical_id,
            target_zettel,
            current_parent,
            source_index,
            parent_id,
            &loaded,
            &documents,
        ),
    }
}

fn plan_move_to_path(
    mode: RefactorMode,
    root: &Path,
    canonical_id: &str,
    target_zettel: &Zettel,
    source: &crate::LoadedSource,
    destination: &Path,
    loaded: &[crate::LoadedSource],
) -> ZorgResult<RefactorPlan> {
    let destination = destination_path(root, canonical_id, Some(destination))?;
    if normalize_path(&destination) == normalize_path(&source.absolute_path) {
        let mut plan = RefactorPlan::new("move", mode, root, Some(canonical_id.to_owned()));
        plan.warnings.push(format!(
            "`@{canonical_id}` is already at {}",
            source.relative_path.display()
        ));
        return Ok(plan);
    }
    if destination.exists() {
        return Err(operation_failed(format!(
            "destination {} already exists",
            destination.display()
        )));
    }

    let destination_relative = destination
        .strip_prefix(root)
        .unwrap_or(&destination)
        .to_path_buf();
    let moved_source = match target_zettel.kind {
        ZettelKind::File => source.source.clone(),
        ZettelKind::Nested => {
            let full_span = zettel_subtree_span(target_zettel).ok_or_else(|| {
                operation_failed(format!("`@{canonical_id}` has no usable source span"))
            })?;
            promoted_file_source(&source.source, full_span, target_zettel, canonical_id)?
        }
        ZettelKind::Directory => {
            return Err(operation_failed(
                "moving directory zettels is not supported",
            ));
        }
    };

    let mut plan = RefactorPlan::new("move", mode, root, Some(canonical_id.to_owned()));
    match target_zettel.kind {
        ZettelKind::File => {
            let mut source_file = RefactorFilePlan::new(
                &source.absolute_path,
                &source.relative_path,
                source.guard.clone(),
            );
            source_file.edits.push(RefactorEdit::new(
                SourceSpan::from_offsets(&source.source, 0, source.source.len()),
                "",
                Some(format!("remove moved `@{canonical_id}` file")),
            ));
            validate_and_sort_file_edits(&source.source, &mut source_file.edits)?;
            plan.files.push(source_file);
        }
        ZettelKind::Nested => {
            let full_span = zettel_subtree_span(target_zettel).ok_or_else(|| {
                operation_failed(format!("`@{canonical_id}` has no usable source span"))
            })?;
            let mut source_file = RefactorFilePlan::new(
                &source.absolute_path,
                &source.relative_path,
                source.guard.clone(),
            );
            source_file.edits.push(RefactorEdit::new(
                source_removal_span(&source.source, full_span),
                "",
                Some(format!("remove nested `@{canonical_id}`")),
            ));
            validate_and_sort_file_edits(&source.source, &mut source_file.edits)?;
            plan.files.push(source_file);
        }
        ZettelKind::Directory => unreachable!("directory handled above"),
    }

    let mut destination_file = RefactorFilePlan::new(
        &destination,
        destination_relative,
        SourceGuard::empty_file(),
    );
    destination_file.edits.push(RefactorEdit::new(
        SourceSpan::bytes(0, 0),
        moved_source,
        Some(format!("create moved `@{canonical_id}`")),
    ));
    plan.files.push(destination_file);
    plan.sort_edits();
    validate_planned_move_corpus(loaded, &plan)?;
    Ok(plan)
}

#[allow(clippy::too_many_arguments)]
fn plan_move_to_parent(
    mode: RefactorMode,
    store: &Store,
    root: &Path,
    canonical_id: &str,
    target_zettel: &Zettel,
    current_parent: Option<&Zettel>,
    source_index: usize,
    parent_id: &str,
    loaded: &[crate::LoadedSource],
    documents: &[zorg_core::ZettelDocument],
) -> ZorgResult<RefactorPlan> {
    if target_zettel.kind != ZettelKind::Nested {
        return Err(operation_failed(
            "moving a file zettel under a parent is not supported; move it to a .z path",
        ));
    }
    if target_zettel.id.is_none() {
        return Err(operation_failed(
            "moving a local or anonymous nested zettel under another parent is not supported",
        ));
    }

    let parent_canonical_id = ZettelId::parse(parent_id)?.as_str().to_owned();
    if parent_canonical_id == canonical_id {
        return Err(operation_failed("cannot move a zettel under itself"));
    }
    if current_parent
        .and_then(|parent| parent.canonical_id.as_ref())
        .is_some_and(|id| id.as_str() == parent_canonical_id)
    {
        let mut plan = RefactorPlan::new("move", mode, root, Some(canonical_id.to_owned()));
        plan.warnings.push(format!(
            "`@{canonical_id}` is already nested under `@{parent_canonical_id}`"
        ));
        return Ok(plan);
    }
    if zettel_subtree_contains_canonical(target_zettel, &parent_canonical_id) {
        return Err(operation_failed(format!(
            "cannot move `@{canonical_id}` under descendant `@{parent_canonical_id}`"
        )));
    }

    let parent = crate::resolve_exact_canonical_zettel(store, &parent_canonical_id)?;
    let parent_source_index = loaded
        .iter()
        .position(|source| source.file.id == parent.file_id)
        .ok_or_else(|| {
            operation_failed(format!(
                "indexed parent `@{parent_canonical_id}` references missing source file row {}",
                parent.file_id
            ))
        })?;
    let parent_source = &loaded[parent_source_index];
    let parent_document = &documents[parent_source_index];
    let parent_zettel = find_zettel_by_canonical_id(&parent_document.root, &parent_canonical_id)
        .ok_or_else(|| {
            operation_failed(format!(
                "could not find reparsed source for indexed parent `@{parent_canonical_id}`"
            ))
        })?;

    let source = &loaded[source_index];
    let full_span = zettel_subtree_span(target_zettel)
        .ok_or_else(|| operation_failed(format!("`@{canonical_id}` has no usable source span")))?;
    let raw_moved = source_slice(&source.source, full_span)?;
    let moved = reindent_nested_source(
        raw_moved,
        leading_indent(raw_moved),
        destination_child_indent(&parent_source.source, parent_zettel)?,
    );
    let insertion_span = insertion_span_for_parent(&parent_source.source, parent_zettel)?;
    let insertion = insertion_text(&parent_source.source, insertion_span.start_byte, &moved);

    let mut plan = RefactorPlan::new("move", mode, root, Some(canonical_id.to_owned()));
    push_file_edit(
        &mut plan.files,
        &source.absolute_path,
        &source.relative_path,
        source.guard.clone(),
        RefactorEdit::new(
            source_removal_span(&source.source, full_span),
            "",
            Some(format!("remove nested `@{canonical_id}`")),
        ),
    );
    push_file_edit(
        &mut plan.files,
        &parent_source.absolute_path,
        &parent_source.relative_path,
        parent_source.guard.clone(),
        RefactorEdit::new(
            insertion_span,
            insertion,
            Some(format!(
                "insert `@{canonical_id}` under `@{parent_canonical_id}`"
            )),
        ),
    );

    for file in &mut plan.files {
        let loaded_source = loaded
            .iter()
            .find(|source| source.absolute_path == file.absolute_path)
            .ok_or_else(|| {
                operation_failed(format!(
                    "planned file {} is not an indexed source",
                    file.absolute_path.display()
                ))
            })?;
        validate_and_sort_file_edits(&loaded_source.source, &mut file.edits)?;
    }
    plan.sort_edits();
    validate_planned_move_corpus(loaded, &plan)?;
    Ok(plan)
}

fn push_file_edit(
    files: &mut Vec<RefactorFilePlan>,
    absolute_path: &Path,
    relative_path: &Path,
    guard: SourceGuard,
    edit: RefactorEdit,
) {
    if let Some(file) = files
        .iter_mut()
        .find(|file| file.absolute_path == absolute_path)
    {
        file.edits.push(edit);
        return;
    }

    let mut file = RefactorFilePlan::new(absolute_path, relative_path, guard);
    file.edits.push(edit);
    files.push(file);
}

fn find_zettel_and_parent_by_canonical_id<'a>(
    zettel: &'a Zettel,
    canonical_id: &str,
    parent: Option<&'a Zettel>,
) -> Option<(&'a Zettel, Option<&'a Zettel>)> {
    if zettel
        .canonical_id
        .as_ref()
        .is_some_and(|id| id.as_str() == canonical_id)
    {
        return Some((zettel, parent));
    }
    for block in &zettel.body {
        if let BodyBlock::ChildZettel(child) = block {
            if let Some(found) =
                find_zettel_and_parent_by_canonical_id(child, canonical_id, Some(zettel))
            {
                return Some(found);
            }
        }
    }
    None
}

fn zettel_subtree_contains_canonical(zettel: &Zettel, canonical_id: &str) -> bool {
    zettel
        .canonical_id
        .as_ref()
        .is_some_and(|id| id.as_str() == canonical_id)
        || zettel.body.iter().any(|block| match block {
            BodyBlock::ChildZettel(child) => zettel_subtree_contains_canonical(child, canonical_id),
            BodyBlock::Paragraph(_) | BodyBlock::FencedCode(_) => false,
        })
}

fn insertion_span_for_parent(source: &str, parent: &Zettel) -> ZorgResult<SourceSpan> {
    let span = zettel_subtree_span(parent)
        .ok_or_else(|| operation_failed("destination parent has no usable source span"))?;
    Ok(SourceSpan::from_offsets(
        source,
        span.end_byte,
        span.end_byte,
    ))
}

fn destination_child_indent(source: &str, parent: &Zettel) -> ZorgResult<usize> {
    match parent.kind {
        ZettelKind::File | ZettelKind::Directory => Ok(0),
        ZettelKind::Nested => {
            let span = parent
                .span
                .ok_or_else(|| operation_failed("destination parent has no usable source span"))?;
            let line = source[span.start_byte..].lines().next().unwrap_or_default();
            Ok(leading_indent(line) + 2)
        }
    }
}

fn leading_indent(source: &str) -> usize {
    source
        .chars()
        .take_while(|character| *character == ' ')
        .count()
}

fn reindent_nested_source(source: &str, from: usize, to: usize) -> String {
    let from_prefix = " ".repeat(from);
    let to_prefix = " ".repeat(to);
    let mut output = String::with_capacity(source.len() + to.saturating_sub(from));
    for line in source.split_inclusive('\n') {
        if line.trim().is_empty() {
            output.push_str(line);
        } else if let Some(rest) = line.strip_prefix(&from_prefix) {
            output.push_str(&to_prefix);
            output.push_str(rest);
        } else {
            output.push_str(line);
        }
    }
    output
}

fn insertion_text(source: &str, insertion_byte: usize, moved: &str) -> String {
    let mut insertion = String::new();
    if insertion_byte > 0 && !source[..insertion_byte].ends_with('\n') {
        insertion.push('\n');
    }
    insertion.push_str(moved);
    if !insertion.ends_with('\n') {
        insertion.push('\n');
    }
    insertion
}

fn validate_planned_move_corpus(
    loaded: &[crate::LoadedSource],
    plan: &RefactorPlan,
) -> ZorgResult<()> {
    let replacements = plan
        .files
        .iter()
        .map(|file| {
            let current = if file.original_guard.is_empty_file() {
                String::new()
            } else {
                loaded
                    .iter()
                    .find(|source| source.absolute_path == file.absolute_path)
                    .map(|source| source.source.clone())
                    .ok_or_else(|| {
                        operation_failed(format!(
                            "planned file {} is not an indexed source",
                            file.absolute_path.display()
                        ))
                    })?
            };
            apply_edits_to_source(&current, &file.edits)
                .map(|next| (file.absolute_path.clone(), next))
        })
        .collect::<ZorgResult<BTreeMap<_, _>>>()?;

    let mut documents = Vec::new();
    for source in loaded {
        match replacements.get(&source.absolute_path) {
            Some(next)
                if next.is_empty()
                    && is_file_deletion_edit(&source.source, plan, &source.absolute_path) => {}
            Some(next) => documents.push(parse_planned_source(next, &source.absolute_path)?),
            None => documents.push(parse_planned_source(&source.source, &source.absolute_path)?),
        }
    }
    for file in &plan.files {
        if file.original_guard.is_empty_file() {
            let next = replacements
                .get(&file.absolute_path)
                .expect("planned destination replacement");
            documents.push(parse_planned_source(next, &file.absolute_path)?);
        }
    }

    zorg_parse::resolve_corpus(&mut documents);
    let validation = zorg_parse::validate_corpus(&documents);
    if let Some(diagnostic) = validation.diagnostics.into_iter().next() {
        let path = diagnostic
            .path
            .as_ref()
            .map(|path| path.as_path().display().to_string())
            .unwrap_or_else(|| "<unknown>".to_owned());
        return Err(operation_failed(format!(
            "move plan would produce invalid source at {path}: {}",
            diagnostic.message
        )));
    }
    Ok(())
}

fn is_file_deletion_edit(original: &str, plan: &RefactorPlan, path: &Path) -> bool {
    plan.files
        .iter()
        .find(|file| file.absolute_path == path)
        .is_some_and(|file| {
            matches!(
                file.edits.as_slice(),
                [RefactorEdit {
                    span,
                    replacement,
                    ..
                }] if span.start_byte == 0
                    && span.end_byte == original.len()
                    && replacement.is_empty()
            )
        })
}

fn parse_planned_source(source: &str, path: &Path) -> ZorgResult<zorg_core::ZettelDocument> {
    zorg_parse::parse_zettel_document_with_path(source, path).map_err(|error| {
        ZorgError::OperationFailed {
            message: format!("failed to parse planned {}: {error}", path.display()),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zorg_store::StoreOptions;

    #[test]
    fn reindents_nested_source_between_parents() {
        let source = "  - @root/child #z/ref Child.\n    Body.\n";
        assert_eq!(
            reindent_nested_source(source, 2, 4),
            "    - @root/child #z/ref Child.\n      Body.\n"
        );
    }

    #[test]
    fn request_is_cloneable_for_cli() {
        let options =
            StoreOptions::new("/tmp/root", "/tmp/root/.zorg/zorg.sqlite3").expect("options");
        let request = MoveRequest {
            store_options: options,
            id: "@alpha".to_owned(),
            mode: RefactorMode::Preview,
            destination: MoveDestination::Path(PathBuf::from("alpha.z")),
        };
        let cloned = request.clone();
        assert_eq!(cloned.id, "@alpha");
    }
}
