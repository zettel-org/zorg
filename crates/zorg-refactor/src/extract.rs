use std::fs;
use std::path::{Path, PathBuf};

use zorg_core::{BodyBlock, SourceSpan, Zettel, ZettelDocument, ZettelId, ZorgResult};
use zorg_store::{Store, StoreOptions};

use crate::promote::{canonical_root, destination_path, normalize_path, validate_planned_corpus};
use crate::{
    LoadedSource, RefactorEdit, RefactorFilePlan, RefactorMode, RefactorPlan, SourceGuard,
    load_indexed_sources, operation_failed, source_slice, validate_and_sort_file_edits,
};

/// Source range accepted by `zorg extract`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExtractRange {
    /// One-based line and column range.
    LineColumn {
        /// One-based start line.
        start_line: usize,
        /// One-based start column.
        start_column: usize,
        /// One-based end line.
        end_line: usize,
        /// One-based end column.
        end_column: usize,
    },
    /// Zero-based byte range.
    Bytes {
        /// Start byte, inclusive.
        start: usize,
        /// End byte, exclusive.
        end: usize,
    },
}

/// Options for extracting a source range into a new zettel.
#[derive(Debug, Clone)]
pub struct ExtractRequest {
    /// Store options that identify the indexed corpus.
    pub store_options: StoreOptions,
    /// Source file containing the selection. Relative paths are resolved under the root.
    pub file: PathBuf,
    /// Source selection to extract.
    pub range: ExtractRange,
    /// New zettel ID declaration, such as `@project/extracted`.
    pub id: String,
    /// Refactor mode for the returned plan.
    pub mode: RefactorMode,
    /// Optional destination path. Relative paths are resolved under the root.
    pub destination: Option<PathBuf>,
    /// Allow replacing a non-paragraph-like selection with an absolute link.
    pub replace_with_link: bool,
}

/// Validates whether an editor selection is structurally extractable.
///
/// This is the LSP-facing subset of `plan_extract`: it deliberately avoids
/// choosing an ID or destination, but it uses the same range and structural
/// safety checks as the full planner.
pub fn validate_extract_selection(
    source: &str,
    document: &ZettelDocument,
    range: ExtractRange,
    replace_with_link: bool,
) -> ZorgResult<SourceSpan> {
    let selection = resolve_extract_range(source, range)?;
    let context = validate_structural_selection(&document.root, selection)?;
    let selected_text = source_slice(source, selection)?;
    if selected_text.trim().is_empty() {
        return Err(operation_failed(
            "extract range must contain non-whitespace text",
        ));
    }

    let paragraph_like = context.kind == ExtractSelectionKind::Paragraph
        && selection_covers_trimmed_block(source, selected_text, context.block_span)?;
    if !paragraph_like && !replace_with_link {
        return Err(operation_failed(
            "extract range is not paragraph-like; pass --replace-with-link to replace it with a link",
        ));
    }

    Ok(selection)
}

/// Plans extraction of a selected body range into a new file zettel.
pub fn plan_extract(request: &ExtractRequest) -> ZorgResult<RefactorPlan> {
    let new_id = ZettelId::parse(&request.id)?.as_str().to_owned();
    let store = Store::open_with_options(request.store_options.clone())?;
    reject_existing_id(&store, &new_id)?;

    let root = canonical_root(request.store_options.corpus_root())?;
    let destination = destination_path(&root, &new_id, request.destination.as_deref())?;
    if destination.exists() {
        return Err(operation_failed(format!(
            "destination {} already exists",
            destination.display()
        )));
    }

    let loaded = load_indexed_sources(request.store_options.clone())?;
    let source = source_for_file(&root, &loaded, &request.file)?;
    let selection = validate_extract_selection(
        &source.source,
        &source.document,
        request.range,
        request.replace_with_link,
    )?;
    let context = validate_structural_selection(&source.document.root, selection)?;
    let selected_text = source_slice(&source.source, selection)?;

    let paragraph_like = context.kind == ExtractSelectionKind::Paragraph
        && selection_covers_trimmed_block(&source.source, selected_text, context.block_span)?;

    let destination_relative = destination
        .strip_prefix(&root)
        .unwrap_or(&destination)
        .to_path_buf();
    let replacement = replacement_link(selected_text, &new_id, paragraph_like);
    let created_source = extracted_file_source(&new_id, selected_text);

    let mut source_file = RefactorFilePlan::new(
        &source.absolute_path,
        &source.relative_path,
        source.guard.clone(),
    );
    source_file.edits.push(RefactorEdit::new(
        selection,
        replacement,
        Some(format!("replace extracted range with `#{new_id}`")),
    ));
    validate_and_sort_file_edits(&source.source, &mut source_file.edits)?;

    let mut destination_file = RefactorFilePlan::new(
        &destination,
        destination_relative,
        SourceGuard::empty_file(),
    );
    destination_file.edits.push(RefactorEdit::new(
        SourceSpan::bytes(0, 0),
        created_source,
        Some(format!("create extracted `@{new_id}`")),
    ));

    let mut plan = RefactorPlan::new("extract", request.mode, &root, Some(new_id));
    plan.files.push(source_file);
    plan.files.push(destination_file);
    plan.sort_edits();
    validate_planned_corpus("extract", &loaded, &plan)?;
    Ok(plan)
}

fn reject_existing_id(store: &Store, canonical_id: &str) -> ZorgResult<()> {
    if store
        .list_zettel()?
        .into_iter()
        .any(|zettel| zettel.canonical_id.as_deref() == Some(canonical_id))
    {
        return Err(operation_failed(format!(
            "indexed zettel `@{canonical_id}` already exists"
        )));
    }
    Ok(())
}

fn source_for_file<'a>(
    root: &Path,
    loaded: &'a [LoadedSource],
    file: &Path,
) -> ZorgResult<&'a LoadedSource> {
    let candidate = if file.is_absolute() {
        normalize_path(file)
    } else {
        normalize_path(&root.join(file))
    };
    let canonical_candidate = fs::canonicalize(&candidate).map_err(|error| {
        operation_failed(format!(
            "failed to resolve extract source file {}: {error}",
            candidate.display()
        ))
    })?;

    loaded
        .iter()
        .find(|source| {
            source.absolute_path == canonical_candidate
                || fs::canonicalize(&source.absolute_path)
                    .is_ok_and(|path| path == canonical_candidate)
        })
        .ok_or_else(|| {
            operation_failed(format!(
                "extract source file {} is not present in the current index",
                candidate.display()
            ))
        })
}

fn resolve_extract_range(source: &str, range: ExtractRange) -> ZorgResult<SourceSpan> {
    let (start, end) = match range {
        ExtractRange::Bytes { start, end } => (start, end),
        ExtractRange::LineColumn {
            start_line,
            start_column,
            end_line,
            end_column,
        } => (
            offset_for_line_column(source, start_line, start_column)?,
            offset_for_line_column(source, end_line, end_column)?,
        ),
    };

    if start >= end {
        return Err(operation_failed(format!(
            "extract range must be non-empty and ordered; got {start}..{end}"
        )));
    }
    if end > source.len() {
        return Err(operation_failed(format!(
            "extract range {start}..{end} is out of bounds for source length {}",
            source.len()
        )));
    }
    if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        return Err(operation_failed(format!(
            "extract range {start}..{end} does not align with UTF-8 boundaries"
        )));
    }
    Ok(SourceSpan::from_offsets(source, start, end))
}

fn offset_for_line_column(
    source: &str,
    target_line: usize,
    target_column: usize,
) -> ZorgResult<usize> {
    if target_line == 0 || target_column == 0 {
        return Err(operation_failed(
            "extract line and column positions are one-based",
        ));
    }

    let mut line = 1;
    let mut column = 1;
    for (byte, character) in source.char_indices() {
        if line == target_line && column == target_column {
            return Ok(byte);
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    if line == target_line && column == target_column {
        return Ok(source.len());
    }

    Err(operation_failed(format!(
        "extract position {target_line}:{target_column} is outside the source"
    )))
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ExtractSelectionKind {
    Paragraph,
    FencedCodeBody,
}

#[derive(Debug, Clone, Copy)]
struct ExtractSelectionContext {
    kind: ExtractSelectionKind,
    block_span: SourceSpan,
}

fn validate_structural_selection(
    zettel: &Zettel,
    selection: SourceSpan,
) -> ZorgResult<ExtractSelectionContext> {
    find_structural_selection(zettel, selection).ok_or_else(|| {
        operation_failed(
            "extract range must stay inside one paragraph or fenced-code body and must not cross zettel boundaries",
        )
    })
}

fn find_structural_selection(
    zettel: &Zettel,
    selection: SourceSpan,
) -> Option<ExtractSelectionContext> {
    for block in &zettel.body {
        match block {
            BodyBlock::Paragraph(paragraph) => {
                let span = paragraph.span?;
                if span_contains(span, selection) {
                    return Some(ExtractSelectionContext {
                        kind: ExtractSelectionKind::Paragraph,
                        block_span: span,
                    });
                }
            }
            BodyBlock::FencedCode(block) => {
                let span = block.body_span?;
                if span_contains(span, selection) {
                    return Some(ExtractSelectionContext {
                        kind: ExtractSelectionKind::FencedCodeBody,
                        block_span: span,
                    });
                }
            }
            BodyBlock::ChildZettel(child) => {
                if let Some(context) = find_structural_selection(child, selection) {
                    return Some(context);
                }
            }
        }
    }
    None
}

fn span_contains(outer: SourceSpan, inner: SourceSpan) -> bool {
    outer.start_byte <= inner.start_byte && inner.end_byte <= outer.end_byte
}

fn selection_covers_trimmed_block(
    source: &str,
    selected_text: &str,
    block_span: SourceSpan,
) -> ZorgResult<bool> {
    let block = source_slice(source, block_span)?;
    Ok(selected_text.trim() == block.trim())
}

fn replacement_link(selected_text: &str, canonical_id: &str, paragraph_like: bool) -> String {
    let link = format!("#{canonical_id}");
    if !paragraph_like {
        return link;
    }

    let leading = selected_text
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect::<String>();
    let trailing = if selected_text.ends_with("\r\n") {
        "\r\n"
    } else if selected_text.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    format!("{leading}{link}{trailing}")
}

fn extracted_file_source(canonical_id: &str, selected_text: &str) -> String {
    let title = title_from_id(canonical_id);
    let mut source = format!("%%% @{canonical_id}\n{title}\n%%%\n\n");
    source.push_str(selected_text);
    if !source.ends_with('\n') {
        source.push('\n');
    }
    source
}

fn title_from_id(canonical_id: &str) -> String {
    canonical_id
        .rsplit('/')
        .next()
        .unwrap_or(canonical_id)
        .replace(['-', '_'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use zorg_store::StoreOptions;

    #[test]
    fn line_column_range_uses_one_based_character_columns() {
        let source = "%%% @root\nRoot\n%%%\n\nAlpha café.\n";
        let span = resolve_extract_range(
            source,
            ExtractRange::LineColumn {
                start_line: 5,
                start_column: 7,
                end_line: 5,
                end_column: 11,
            },
        )
        .expect("range");

        assert_eq!(source_slice(source, span).expect("slice"), "café");
    }

    #[test]
    fn byte_range_rejects_utf8_boundaries() {
        let source = "café";
        let error = resolve_extract_range(source, ExtractRange::Bytes { start: 0, end: 4 })
            .expect_err("utf8 boundary");
        assert!(error.to_string().contains("UTF-8 boundaries"));
    }

    #[test]
    fn replacement_preserves_paragraph_line_shape() {
        assert_eq!(
            replacement_link("  Alpha.\n", "root/ex", true),
            "  #root/ex\n"
        );
        assert_eq!(replacement_link("Alpha", "root/ex", false), "#root/ex");
    }

    #[test]
    fn request_is_cloneable_for_cli() {
        let options =
            StoreOptions::new("/tmp/root", "/tmp/root/.zorg/zorg.sqlite3").expect("options");
        let request = ExtractRequest {
            store_options: options,
            file: PathBuf::from("main.z"),
            range: ExtractRange::Bytes { start: 0, end: 1 },
            id: "@alpha".to_owned(),
            mode: RefactorMode::Preview,
            destination: None,
            replace_with_link: false,
        };
        let cloned = request.clone();
        assert_eq!(cloned.id, "@alpha");
    }
}
