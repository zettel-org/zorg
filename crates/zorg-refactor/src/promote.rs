use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use zorg_core::{
    BodyBlock, SourceSpan, Zettel, ZettelDocument, ZettelId, ZettelKind, ZorgError, ZorgResult,
};
use zorg_store::{Store, StoreOptions};

use crate::{
    RefactorEdit, RefactorFilePlan, RefactorMode, RefactorPlan, SourceGuard, apply_edits_to_source,
    load_indexed_sources, operation_failed, source_slice, validate_and_sort_file_edits,
};

/// Options for promoting a nested zettel into its own file zettel.
#[derive(Debug, Clone)]
pub struct PromoteRequest {
    /// Store options that identify the indexed corpus.
    pub store_options: StoreOptions,
    /// Target zettel ID declaration, such as `@project/plan`.
    pub id: String,
    /// Refactor mode for the returned plan.
    pub mode: RefactorMode,
    /// Optional destination path. Relative paths are resolved under the root.
    pub destination: Option<PathBuf>,
}

/// Plans promotion of a nested zettel into a file zettel.
pub fn plan_promote(request: &PromoteRequest) -> ZorgResult<RefactorPlan> {
    let canonical_id = ZettelId::parse(&request.id)?.as_str().to_owned();
    let store = Store::open_with_options(request.store_options.clone())?;
    let target = crate::resolve_exact_canonical_zettel(&store, &canonical_id)?;
    if target.kind != "nested" {
        return Err(operation_failed(format!(
            "`@{canonical_id}` is a {} zettel; only nested zettels can be promoted",
            target.kind
        )));
    }

    let root = canonical_root(request.store_options.corpus_root())?;
    let destination = destination_path(&root, &canonical_id, request.destination.as_deref())?;
    if destination.exists() {
        return Err(operation_failed(format!(
            "destination {} already exists",
            destination.display()
        )));
    }

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
    let target_zettel =
        find_zettel_by_canonical_id(&document.root, &canonical_id).ok_or_else(|| {
            operation_failed(format!(
                "could not find reparsed source for indexed zettel `@{canonical_id}`"
            ))
        })?;

    let full_span = zettel_subtree_span(target_zettel)
        .ok_or_else(|| operation_failed(format!("`@{canonical_id}` has no usable source span")))?;
    let promoted_source =
        promoted_file_source(&source.source, full_span, target_zettel, &canonical_id)?;
    let removal_span = source_removal_span(&source.source, full_span);
    let destination_relative = destination
        .strip_prefix(&root)
        .unwrap_or(&destination)
        .to_path_buf();

    let mut source_file = RefactorFilePlan::new(
        &source.absolute_path,
        &source.relative_path,
        source.guard.clone(),
    );
    source_file.edits.push(RefactorEdit::new(
        removal_span,
        "",
        Some(format!("remove nested `@{canonical_id}`")),
    ));
    validate_and_sort_file_edits(&source.source, &mut source_file.edits)?;

    let mut destination_file = RefactorFilePlan::new(
        &destination,
        destination_relative,
        SourceGuard::empty_file(),
    );
    destination_file.edits.push(RefactorEdit::new(
        SourceSpan::bytes(0, 0),
        promoted_source,
        Some(format!("create promoted `@{canonical_id}`")),
    ));

    let mut plan = RefactorPlan::new("promote", request.mode, &root, Some(canonical_id));
    plan.files.push(source_file);
    plan.files.push(destination_file);
    plan.sort_edits();
    validate_planned_corpus("promote", &loaded, &plan)?;
    Ok(plan)
}

pub(crate) fn canonical_root(root: &Path) -> ZorgResult<PathBuf> {
    fs::canonicalize(root).map_err(|error| {
        operation_failed(format!(
            "failed to resolve corpus root {}: {error}",
            root.display()
        ))
    })
}

pub(crate) fn destination_path(
    root: &Path,
    canonical_id: &str,
    explicit: Option<&Path>,
) -> ZorgResult<PathBuf> {
    let raw = explicit
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(format!("{canonical_id}.z")));
    let candidate = if raw.is_absolute() {
        raw
    } else {
        root.join(raw)
    };
    let normalized = normalize_path(&candidate);
    if !normalized.starts_with(root) {
        return Err(operation_failed(format!(
            "destination {} is outside corpus root {}",
            normalized.display(),
            root.display()
        )));
    }
    if normalized.extension().and_then(|value| value.to_str()) != Some("z") {
        return Err(operation_failed(format!(
            "destination {} must use the .z extension",
            normalized.display()
        )));
    }
    Ok(normalized)
}

pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

pub(crate) fn find_zettel_by_canonical_id<'a>(
    zettel: &'a Zettel,
    canonical_id: &str,
) -> Option<&'a Zettel> {
    if zettel
        .canonical_id
        .as_ref()
        .is_some_and(|id| id.as_str() == canonical_id)
    {
        return Some(zettel);
    }
    for block in &zettel.body {
        if let BodyBlock::ChildZettel(child) = block {
            if let Some(found) = find_zettel_by_canonical_id(child, canonical_id) {
                return Some(found);
            }
        }
    }
    None
}

pub(crate) fn zettel_subtree_span(zettel: &Zettel) -> Option<SourceSpan> {
    let mut start = zettel.span?.start_byte;
    let mut end = zettel.span?.end_byte;
    for block in &zettel.body {
        if let BodyBlock::ChildZettel(child) = block {
            if let Some(child_span) = zettel_subtree_span(child) {
                start = start.min(child_span.start_byte);
                end = end.max(child_span.end_byte);
            }
        }
    }
    Some(SourceSpan::bytes(start, end))
}

pub(crate) fn promoted_file_source(
    source: &str,
    full_span: SourceSpan,
    zettel: &Zettel,
    canonical_id: &str,
) -> ZorgResult<String> {
    if zettel.kind != ZettelKind::Nested {
        return Err(operation_failed(
            "only nested zettel source can be promoted",
        ));
    }
    let raw = source_slice(source, full_span)?;
    let (opening_line, body) = split_first_line(raw);
    let opening = opening_line.trim_end_matches(['\r', '\n']);
    let trimmed = opening.trim_start();
    let indent_len = opening.len() - trimmed.len();
    let Some(opening_body) = trimmed.strip_prefix("- ") else {
        return Err(operation_failed(
            "nested zettel opening is missing a list marker",
        ));
    };
    let title = zettel.plain_title().unwrap_or_default();
    let mut metadata = if !title.is_empty() && opening_body.ends_with(&title) {
        opening_body[..opening_body.len() - title.len()]
            .trim_end()
            .to_owned()
    } else {
        opening_body.trim_end().to_owned()
    };
    metadata = absolute_promoted_metadata(&metadata, canonical_id)?;

    let mut promoted = format!("%%% {metadata}\n{title}\n%%%\n");
    let deindented_body = deindent_nested_body(body, indent_len + 2);
    if !deindented_body.trim().is_empty() {
        promoted.push('\n');
        promoted.push_str(&deindented_body);
        if !promoted.ends_with('\n') {
            promoted.push('\n');
        }
    }
    Ok(promoted)
}

fn absolute_promoted_metadata(metadata: &str, canonical_id: &str) -> ZorgResult<String> {
    let Some((first, rest)) = metadata.split_once(char::is_whitespace) else {
        if metadata.starts_with('@') {
            return Ok(metadata.to_owned());
        }
        if metadata.starts_with('^') {
            return Ok(format!("@{canonical_id}"));
        }
        return Err(operation_failed(
            "promoted nested zettel must have an absolute or local ID declaration",
        ));
    };

    if first.starts_with('@') {
        Ok(metadata.to_owned())
    } else if first.starts_with('^') {
        Ok(format!("@{canonical_id} {rest}"))
    } else {
        Err(operation_failed(
            "promoted nested zettel must have an absolute or local ID declaration",
        ))
    }
}

fn split_first_line(source: &str) -> (&str, &str) {
    if let Some(index) = source.find('\n') {
        source.split_at(index + 1)
    } else {
        (source, "")
    }
}

fn deindent_nested_body(body: &str, columns: usize) -> String {
    let prefix = " ".repeat(columns);
    let mut output = String::with_capacity(body.len());
    for line in body.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix(&prefix) {
            output.push_str(rest);
        } else {
            output.push_str(line.trim_start_matches(' '));
        }
    }
    output
}

pub(crate) fn source_removal_span(source: &str, span: SourceSpan) -> SourceSpan {
    let mut start = span.start_byte;
    let end = span.end_byte;
    if start > 0 && source.as_bytes().get(start - 1) == Some(&b'\n') {
        start -= 1;
        if start > 0 && source.as_bytes().get(start - 1) == Some(&b'\r') {
            start -= 1;
        }
    }
    SourceSpan::from_offsets(source, start, end)
}

pub(crate) fn validate_planned_corpus(
    operation: &str,
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
        let next = replacements
            .get(&source.absolute_path)
            .cloned()
            .unwrap_or_else(|| source.source.clone());
        documents.push(parse_planned_source(&next, &source.absolute_path)?);
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
            "{operation} plan would produce invalid source at {path}: {}",
            diagnostic.message
        )));
    }
    Ok(())
}

fn parse_planned_source(source: &str, path: &Path) -> ZorgResult<ZettelDocument> {
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
    fn promotes_nested_source_to_file_source() {
        let source = "\
%%% @root #z/ref
Root
%%%

- @root/child #z/todo [ ] due::2026-05-15 Child title.
  Child paragraph.

  - ^task #z/ref Task title.
    Task body.
";
        let mut document =
            zorg_parse::parse_zettel_document_with_path(source, "/tmp/root.z").expect("parse");
        zorg_parse::resolve_document(&mut document);
        let child = find_zettel_by_canonical_id(&document.root, "root/child").expect("child");
        let full_span = zettel_subtree_span(child).expect("span");

        let promoted =
            promoted_file_source(source, full_span, child, "root/child").expect("promote");

        assert_eq!(
            promoted,
            "\
%%% @root/child #z/todo [ ] due::2026-05-15
Child title.
%%%

Child paragraph.

- ^task #z/ref Task title.
  Task body.
"
        );
    }

    #[test]
    fn refuses_outside_destination() {
        let root = Path::new("/tmp/zorg-root");
        let error = destination_path(root, "alpha", Some(Path::new("../alpha.z")))
            .expect_err("outside root");
        assert!(error.to_string().contains("outside corpus root"));
    }

    #[test]
    fn default_destination_uses_canonical_id() {
        let root = Path::new("/tmp/zorg-root");
        let destination = destination_path(root, "alpha/beta", None).expect("destination");
        assert_eq!(destination, PathBuf::from("/tmp/zorg-root/alpha/beta.z"));
    }

    #[test]
    fn request_is_cloneable_for_cli() {
        let options =
            StoreOptions::new("/tmp/root", "/tmp/root/.zorg/zorg.sqlite3").expect("options");
        let request = PromoteRequest {
            store_options: options,
            id: "@alpha".to_owned(),
            mode: RefactorMode::Preview,
            destination: None,
        };
        let cloned = request.clone();
        assert_eq!(cloned.id, "@alpha");
    }
}
