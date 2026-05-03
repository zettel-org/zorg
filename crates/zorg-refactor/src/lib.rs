//! Shared structural refactor planning boundary for Zorg.
//!
//! This crate intentionally contains no user-facing commands. It provides the
//! plan, preview, source-loading, and edit-safety helpers consumed by later CLI
//! and editor integrations.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zorg_core::{SourceSpan, ZettelDocument, ZettelId, ZorgError, ZorgResult};
use zorg_store::{Store, StoreOptions, StoredFile, StoredZettel};

mod move_zettel;
mod promote;

const PREVIEW_SCHEMA_VERSION: u32 = 1;

pub use move_zettel::{MoveDestination, MoveRequest, plan_move};
pub use promote::{PromoteRequest, plan_promote};

/// Refactor execution mode. Write plans must be applied explicitly.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefactorMode {
    /// Validate inputs and report whether a command could plan work.
    Check,
    /// Build and serialize the edit plan without writing files.
    Preview,
    /// Apply a previously validated plan to disk.
    Write,
}

/// Top-level multi-file refactor plan.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RefactorPlan {
    /// Stable operation name such as `promote`, `move`, or `extract`.
    pub operation: String,
    /// Requested execution mode.
    pub mode: RefactorMode,
    /// Absolute corpus root used while planning.
    pub root: PathBuf,
    /// Canonical target ID without the leading `@`, when the operation has one.
    pub target_id: Option<String>,
    /// Non-fatal safety or behavior notes.
    pub warnings: Vec<String>,
    /// Fatal planning rejections. A plan with rejections must not be written.
    pub rejections: Vec<String>,
    /// Per-file plans in deterministic root-relative path order.
    pub files: Vec<RefactorFilePlan>,
}

impl RefactorPlan {
    /// Creates an empty plan envelope.
    #[must_use]
    pub fn new(
        operation: impl Into<String>,
        mode: RefactorMode,
        root: impl Into<PathBuf>,
        target_id: Option<String>,
    ) -> Self {
        Self {
            operation: operation.into(),
            mode,
            root: root.into(),
            target_id,
            warnings: Vec::new(),
            rejections: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Sorts files and edits into deterministic application order.
    pub fn sort_edits(&mut self) {
        self.files
            .sort_by(|left, right| left.root_relative_path.cmp(&right.root_relative_path));
        for file in &mut self.files {
            file.sort_edits();
        }
    }
}

/// Per-file part of a refactor plan.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RefactorFilePlan {
    /// Absolute source path to rewrite.
    pub absolute_path: PathBuf,
    /// Source path relative to the refactor root.
    pub root_relative_path: PathBuf,
    /// Guard captured from the original source content.
    pub original_guard: SourceGuard,
    /// Ordered byte-span edits for this file.
    pub edits: Vec<RefactorEdit>,
}

impl RefactorFilePlan {
    /// Creates a per-file plan with no edits.
    #[must_use]
    pub fn new(
        absolute_path: impl Into<PathBuf>,
        root_relative_path: impl Into<PathBuf>,
        original_guard: SourceGuard,
    ) -> Self {
        Self {
            absolute_path: absolute_path.into(),
            root_relative_path: root_relative_path.into(),
            original_guard,
            edits: Vec::new(),
        }
    }

    /// Sorts edits into deterministic source order.
    pub fn sort_edits(&mut self) {
        self.edits.sort_by(|left, right| {
            left.span
                .start_byte
                .cmp(&right.span.start_byte)
                .then(left.span.end_byte.cmp(&right.span.end_byte))
                .then(left.replacement.cmp(&right.replacement))
                .then(left.label.cmp(&right.label))
        });
    }
}

/// One byte-span source replacement.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RefactorEdit {
    /// Source span to replace.
    pub span: SourceSpan,
    /// Replacement text.
    pub replacement: String,
    /// Optional caller-facing label.
    pub label: Option<String>,
}

impl RefactorEdit {
    /// Creates an edit for an already translated span.
    #[must_use]
    pub fn new(span: SourceSpan, replacement: impl Into<String>, label: Option<String>) -> Self {
        Self {
            span,
            replacement: replacement.into(),
            label,
        }
    }
}

/// Stable JSON preview envelope for CLI and editor clients.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RefactorPreview {
    /// Preview schema version.
    pub schema_version: u32,
    /// Serialized refactor plan.
    pub plan: RefactorPlan,
}

impl RefactorPreview {
    /// Wraps a plan in the current preview schema.
    #[must_use]
    pub const fn new(plan: RefactorPlan) -> Self {
        Self {
            schema_version: PREVIEW_SCHEMA_VERSION,
            plan,
        }
    }
}

/// Source guard used to reject stale writes.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceGuard {
    /// FNV-1a hash of the original UTF-8 source bytes.
    pub content_hash: String,
    /// Source modified time in Unix milliseconds when available.
    pub mtime_unix_ms: Option<i64>,
    /// Source byte length.
    pub byte_len: u64,
}

impl SourceGuard {
    /// Guard value used for planned creation of a file that must not already exist.
    #[must_use]
    pub fn empty_file() -> Self {
        Self {
            content_hash: content_hash(&[]),
            mtime_unix_ms: None,
            byte_len: 0,
        }
    }

    /// Returns true when this guard represents planned new-file creation.
    #[must_use]
    pub fn is_empty_file(&self) -> bool {
        self == &Self::empty_file()
    }
}

/// Indexed source loaded and reparsed from disk.
#[derive(Debug)]
pub struct LoadedSource {
    /// Indexed file metadata.
    pub file: StoredFile,
    /// Current absolute source path.
    pub absolute_path: PathBuf,
    /// Current root-relative source path.
    pub relative_path: PathBuf,
    /// Current source content.
    pub source: String,
    /// Current source guard.
    pub guard: SourceGuard,
    /// Reparsed semantic document.
    pub document: ZettelDocument,
}

/// Read-only source location for an indexed zettel.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ZettelLocation {
    /// Canonical zettel ID without `@`.
    pub canonical_id: String,
    /// Absolute source file path.
    pub absolute_path: PathBuf,
    /// Source file path relative to the corpus root.
    pub root_relative_path: PathBuf,
    /// Source span for the zettel opening.
    pub source_span: SourceSpan,
    /// Plain title text when indexed.
    pub title: Option<String>,
    /// Indexed zettel kind.
    pub kind: String,
}

/// Link/reference form used by shared rename rewrite helpers.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RenameReferenceKind {
    /// Absolute `#target` link.
    Absolute,
    /// Child-relative `+target` link.
    Child,
    /// Sibling-relative `~target` link.
    Sibling,
    /// Local `^target` reference.
    Local,
}

/// Declaration form used by shared rename rewrite helpers.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RenameDeclarationKind {
    /// Absolute `@target` declaration.
    Absolute,
    /// Local `^target` declaration.
    Local,
}

/// Context required to rewrite a reference deterministically.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RenameReferenceContext<'a> {
    /// Reference form in source.
    pub kind: RenameReferenceKind,
    /// Canonical target ID before the rename.
    pub old_target_id: &'a str,
    /// Canonical ID of the containing zettel, when needed by relative links.
    pub containing_id: Option<&'a str>,
    /// Parent/base canonical ID for sibling references.
    pub sibling_base_id: Option<&'a str>,
}

/// Loads all indexed sources for a store root, reparses them from disk, and
/// keeps only files whose current content still matches the indexed guard.
pub fn load_indexed_sources(options: StoreOptions) -> ZorgResult<Vec<LoadedSource>> {
    let store = Store::open_with_options(options)?;
    let mut loaded = Vec::new();

    for file in store.list_files()? {
        let source = fs::read_to_string(&file.absolute_path).map_err(|error| {
            operation_failed(format!(
                "failed to read indexed source {}: {error}",
                file.absolute_path.display()
            ))
        })?;
        let guard = source_guard(&file.absolute_path, &source)?;
        let indexed_guard = source_guard_from_stored_file(&file)?;
        if guard.content_hash != indexed_guard.content_hash
            || guard.byte_len != indexed_guard.byte_len
        {
            return Err(operation_failed(format!(
                "indexed source {} has changed since the store snapshot; run `zorg db reindex` before planning refactors",
                file.absolute_path.display()
            )));
        }

        let document = zorg_parse::parse_zettel_document_with_path(&source, &file.absolute_path)
            .map_err(|error| {
                operation_failed(format!(
                    "failed to parse indexed source {}: {error}",
                    file.absolute_path.display()
                ))
            })?;
        loaded.push(LoadedSource {
            absolute_path: file.absolute_path.clone(),
            relative_path: file.relative_path.clone(),
            file,
            source,
            guard,
            document,
        });
    }

    Ok(loaded)
}

/// Resolves exactly one indexed zettel by canonical ID.
pub fn resolve_exact_canonical_zettel(
    store: &Store,
    canonical_id: &str,
) -> ZorgResult<StoredZettel> {
    let matches = store
        .list_zettel()?
        .into_iter()
        .filter(|zettel| zettel.canonical_id.as_deref() == Some(canonical_id))
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [zettel] => Ok(zettel.clone()),
        [] => Err(operation_failed(format!(
            "no indexed zettel found for `@{canonical_id}`"
        ))),
        _ => Err(operation_failed(format!(
            "multiple indexed zettels found for `@{canonical_id}`"
        ))),
    }
}

/// Resolves an absolute zettel declaration such as `@project/plan` to its
/// indexed source location without refreshing or mutating the store.
pub fn locate_zettel(store: &Store, id: &str) -> ZorgResult<ZettelLocation> {
    let canonical_id = ZettelId::parse(id)?.as_str().to_owned();
    let zettel = resolve_exact_canonical_zettel(store, &canonical_id)?;
    let file = store
        .list_files()?
        .into_iter()
        .find(|file| file.id == zettel.file_id)
        .ok_or_else(|| {
            operation_failed(format!(
                "indexed zettel `@{canonical_id}` references missing source file row {}",
                zettel.file_id
            ))
        })?;
    let source_span = source_span_from_stored_zettel(&zettel, &canonical_id)?;

    Ok(ZettelLocation {
        canonical_id,
        absolute_path: file.absolute_path,
        root_relative_path: file.relative_path,
        source_span,
        title: zettel.title,
        kind: zettel.kind,
    })
}

/// Extracts a source slice for a known-good span.
pub fn source_slice(source: &str, span: SourceSpan) -> ZorgResult<&str> {
    validate_span_bounds(source, span)?;
    Ok(&source[span.start_byte..span.end_byte])
}

/// Validates edit bounds and non-overlap, then sorts edits deterministically.
pub fn validate_and_sort_file_edits(source: &str, edits: &mut [RefactorEdit]) -> ZorgResult<()> {
    edits.sort_by(|left, right| {
        left.span
            .start_byte
            .cmp(&right.span.start_byte)
            .then(left.span.end_byte.cmp(&right.span.end_byte))
            .then(left.replacement.cmp(&right.replacement))
            .then(left.label.cmp(&right.label))
    });

    let mut previous_end = 0;
    for edit in edits {
        validate_span_bounds(source, edit.span)?;
        if edit.span.start_byte < previous_end {
            return Err(operation_failed(format!(
                "refactor edits overlap at byte {}",
                edit.span.start_byte
            )));
        }
        previous_end = edit.span.end_byte;
    }

    Ok(())
}

/// Applies one file's edits to source after validation.
pub fn apply_edits_to_source(source: &str, edits: &[RefactorEdit]) -> ZorgResult<String> {
    let mut edits = edits.to_vec();
    validate_and_sort_file_edits(source, &mut edits)?;

    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    for edit in edits {
        output.push_str(&source[cursor..edit.span.start_byte]);
        output.push_str(&edit.replacement);
        cursor = edit.span.end_byte;
    }
    output.push_str(&source[cursor..]);
    Ok(output)
}

/// Applies a write-mode multi-file plan. Each file is guarded against stale
/// source content before any temporary file is written.
pub fn apply_refactor_plan(plan: &RefactorPlan) -> ZorgResult<()> {
    if plan.mode != RefactorMode::Write {
        return Err(operation_failed(
            "refactor writes require a plan in write mode",
        ));
    }
    if !plan.rejections.is_empty() {
        return Err(operation_failed(
            "refactor plan has rejections and cannot be written",
        ));
    }

    enum PreparedRefactorWrite {
        Write(PathBuf, String),
        Delete(PathBuf),
    }

    let mut prepared = Vec::new();
    for file in &plan.files {
        let current_source = match fs::read_to_string(&file.absolute_path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if file.original_guard.is_empty_file() {
                    String::new()
                } else {
                    return Err(operation_failed(format!(
                        "source {} is missing before refactor write",
                        file.absolute_path.display()
                    )));
                }
            }
            Err(error) => {
                return Err(operation_failed(format!(
                    "failed to read {} before refactor write: {error}",
                    file.absolute_path.display()
                )));
            }
        };
        if current_source.is_empty() && file.original_guard.is_empty_file() {
            if file.absolute_path.exists() {
                return Err(operation_failed(format!(
                    "destination {} already exists; refusing stale refactor write",
                    file.absolute_path.display()
                )));
            }
        } else {
            let current_guard = source_guard(&file.absolute_path, &current_source)?;
            if current_guard != file.original_guard {
                return Err(operation_failed(format!(
                    "source guard mismatch for {}; refusing stale refactor write",
                    file.absolute_path.display()
                )));
            }
        }
        let next_source = apply_edits_to_source(&current_source, &file.edits)?;
        if is_file_deletion_edit(&current_source, &file.edits, &next_source) {
            prepared.push(PreparedRefactorWrite::Delete(file.absolute_path.clone()));
            continue;
        }
        zorg_parse::parse_zettel_document_with_path(&next_source, &file.absolute_path).map_err(
            |error| {
                operation_failed(format!(
                    "refactor write would produce unparsable {}: {error}",
                    file.absolute_path.display()
                ))
            },
        )?;
        prepared.push(PreparedRefactorWrite::Write(
            file.absolute_path.clone(),
            next_source,
        ));
    }

    let mut temp_paths = Vec::new();
    for (index, item) in prepared.iter().enumerate() {
        let PreparedRefactorWrite::Write(path, next_source) = item else {
            continue;
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                operation_failed(format!(
                    "failed to create refactor destination directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let temp_path = temp_path_for(path, index);
        fs::write(&temp_path, next_source).map_err(|error| {
            operation_failed(format!(
                "failed to write temporary refactor file {}: {error}",
                temp_path.display()
            ))
        })?;
        temp_paths.push(temp_path);
    }

    let mut temp_index = 0;
    for item in &prepared {
        match item {
            PreparedRefactorWrite::Write(path, _) => {
                let temp_path = &temp_paths[temp_index];
                temp_index += 1;
                fs::rename(temp_path, path).map_err(|error| {
                    cleanup_temp_paths(&temp_paths);
                    operation_failed(format!(
                        "failed to replace {} during refactor write: {error}",
                        path.display()
                    ))
                })?;
            }
            PreparedRefactorWrite::Delete(path) => {
                fs::remove_file(path).map_err(|error| {
                    cleanup_temp_paths(&temp_paths);
                    operation_failed(format!(
                        "failed to remove {} during refactor write: {error}",
                        path.display()
                    ))
                })?;
            }
        }
    }

    Ok(())
}

/// Computes a deterministic source guard for text read from `path`.
pub fn source_guard(path: &Path, source: &str) -> ZorgResult<SourceGuard> {
    let metadata = fs::metadata(path).map_err(|error| {
        operation_failed(format!("failed to stat source {}: {error}", path.display()))
    })?;

    Ok(SourceGuard {
        content_hash: content_hash(source.as_bytes()),
        mtime_unix_ms: metadata.modified().ok().and_then(system_time_unix_ms),
        byte_len: u64::try_from(source.len())
            .map_err(|_| operation_failed("source length does not fit in u64"))?,
    })
}

/// Returns the text that should replace a zettel ID declaration during rename.
pub fn declaration_replacement(
    kind: RenameDeclarationKind,
    old_id: &str,
    new_id: &str,
) -> ZorgResult<String> {
    match kind {
        RenameDeclarationKind::Absolute => Ok(format!("@{new_id}")),
        RenameDeclarationKind::Local => {
            let suffix = same_parent_suffix(old_id, new_id).ok_or_else(|| {
                operation_failed(
                    "local zettel ID rename must stay under the same absolute ancestor",
                )
            })?;
            Ok(format!("^{suffix}"))
        }
    }
}

/// Returns the text that should replace a reference during rename.
pub fn reference_replacement(
    context: RenameReferenceContext<'_>,
    new_id: &str,
) -> ZorgResult<String> {
    match context.kind {
        RenameReferenceKind::Absolute => Ok(format!("#{new_id}")),
        RenameReferenceKind::Child => {
            let current = context.containing_id.ok_or_else(|| {
                operation_failed("child-relative reference has no containing canonical ID")
            })?;
            let suffix = direct_child_suffix(current, new_id).ok_or_else(|| {
                operation_failed(format!(
                    "child-relative reference to `@{}` cannot be rewritten deterministically as `+...`",
                    context.old_target_id
                ))
            })?;
            Ok(format!("+{suffix}"))
        }
        RenameReferenceKind::Sibling => {
            let base = context.sibling_base_id.ok_or_else(|| {
                operation_failed("sibling-relative reference has no deterministic parent ID")
            })?;
            let suffix = direct_child_suffix(base, new_id).ok_or_else(|| {
                operation_failed(format!(
                    "sibling-relative reference to `@{}` cannot be rewritten deterministically as `~...`",
                    context.old_target_id
                ))
            })?;
            Ok(format!("~{suffix}"))
        }
        RenameReferenceKind::Local => {
            let current = context.containing_id.ok_or_else(|| {
                operation_failed("local reference has no containing canonical ID")
            })?;
            let suffix = direct_child_suffix(current, new_id).ok_or_else(|| {
                operation_failed(format!(
                    "local reference to `@{}` cannot be rewritten deterministically as `^...`",
                    context.old_target_id
                ))
            })?;
            Ok(format!("^{suffix}"))
        }
    }
}

fn source_guard_from_stored_file(file: &StoredFile) -> ZorgResult<SourceGuard> {
    Ok(SourceGuard {
        content_hash: file.content_hash.clone(),
        mtime_unix_ms: file.mtime_unix_ms,
        byte_len: u64::try_from(file.byte_len)
            .map_err(|_| operation_failed("indexed source length is negative"))?,
    })
}

fn source_span_from_stored_zettel(
    zettel: &StoredZettel,
    canonical_id: &str,
) -> ZorgResult<SourceSpan> {
    let start_byte = usize::try_from(zettel.start_byte).map_err(|_| {
        operation_failed(format!(
            "indexed zettel `@{canonical_id}` has an invalid negative start byte"
        ))
    })?;
    let end_byte = usize::try_from(zettel.end_byte).map_err(|_| {
        operation_failed(format!(
            "indexed zettel `@{canonical_id}` has an invalid negative end byte"
        ))
    })?;
    if start_byte > end_byte {
        return Err(operation_failed(format!(
            "indexed zettel `@{canonical_id}` has an invalid source byte span"
        )));
    }

    let Some(start_line) = zettel
        .start_line
        .and_then(|value| usize::try_from(value).ok())
    else {
        return Err(operation_failed(format!(
            "indexed zettel `@{canonical_id}` has no usable source start line"
        )));
    };
    let Some(start_column) = zettel
        .start_column
        .and_then(|value| usize::try_from(value).ok())
    else {
        return Err(operation_failed(format!(
            "indexed zettel `@{canonical_id}` has no usable source start column"
        )));
    };
    let Some(end_line) = zettel
        .end_line
        .and_then(|value| usize::try_from(value).ok())
    else {
        return Err(operation_failed(format!(
            "indexed zettel `@{canonical_id}` has no usable source end line"
        )));
    };
    let Some(end_column) = zettel
        .end_column
        .and_then(|value| usize::try_from(value).ok())
    else {
        return Err(operation_failed(format!(
            "indexed zettel `@{canonical_id}` has no usable source end column"
        )));
    };

    Ok(SourceSpan {
        start_byte,
        end_byte,
        start_line: Some(start_line),
        start_column: Some(start_column),
        end_line: Some(end_line),
        end_column: Some(end_column),
    })
}

fn validate_span_bounds(source: &str, span: SourceSpan) -> ZorgResult<()> {
    if span.start_byte > span.end_byte {
        return Err(operation_failed(format!(
            "invalid refactor span {}..{}",
            span.start_byte, span.end_byte
        )));
    }
    if span.end_byte > source.len() {
        return Err(operation_failed(format!(
            "refactor span {}..{} is out of bounds for source length {}",
            span.start_byte,
            span.end_byte,
            source.len()
        )));
    }
    if !source.is_char_boundary(span.start_byte) || !source.is_char_boundary(span.end_byte) {
        return Err(operation_failed(format!(
            "refactor span {}..{} does not align with UTF-8 boundaries",
            span.start_byte, span.end_byte
        )));
    }
    Ok(())
}

fn temp_path_for(path: &Path, index: usize) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("source.z");
    parent.join(format!(
        ".{file_name}.zorg-refactor.{}.{}.tmp",
        std::process::id(),
        index
    ))
}

fn cleanup_temp_paths(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

fn is_file_deletion_edit(source: &str, edits: &[RefactorEdit], next_source: &str) -> bool {
    next_source.is_empty()
        && matches!(
            edits,
            [RefactorEdit {
                span,
                replacement,
                ..
            }] if span.start_byte == 0 && span.end_byte == source.len() && replacement.is_empty()
        )
}

fn content_hash(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}

fn same_parent_suffix(old_id: &str, new_id: &str) -> Option<String> {
    let (old_base, _) = old_id.rsplit_once('/')?;
    direct_child_suffix(old_base, new_id).map(str::to_owned)
}

fn direct_child_suffix<'a>(base: &str, canonical_id: &'a str) -> Option<&'a str> {
    let suffix = canonical_id.strip_prefix(base)?.strip_prefix('/')?;
    (!suffix.is_empty() && !suffix.contains('/')).then_some(suffix)
}

fn system_time_unix_ms(time: std::time::SystemTime) -> Option<i64> {
    let duration = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

fn operation_failed(message: impl Into<String>) -> ZorgError {
    ZorgError::OperationFailed {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_overlapping_edits() {
        let source = "abcdef";
        let mut edits = vec![
            RefactorEdit::new(SourceSpan::bytes(1, 4), "x", Some("first".to_owned())),
            RefactorEdit::new(SourceSpan::bytes(3, 5), "y", Some("second".to_owned())),
        ];

        let error = validate_and_sort_file_edits(source, &mut edits).expect_err("overlap");
        assert!(error.to_string().contains("overlap"));
    }

    #[test]
    fn validation_rejects_out_of_bounds_edits() {
        let source = "abc";
        let mut edits = vec![RefactorEdit::new(SourceSpan::bytes(2, 4), "x", None)];

        let error = validate_and_sort_file_edits(source, &mut edits).expect_err("bounds");
        assert!(error.to_string().contains("out of bounds"));
    }

    #[test]
    fn apply_sorts_edits_deterministically() {
        let source = "abcdef";
        let edits = vec![
            RefactorEdit::new(SourceSpan::bytes(4, 6), "EF", Some("second".to_owned())),
            RefactorEdit::new(SourceSpan::bytes(0, 2), "AB", Some("first".to_owned())),
        ];

        let applied = apply_edits_to_source(source, &edits).expect("apply");
        assert_eq!(applied, "ABcdEF");
    }

    #[test]
    fn write_rejects_source_guard_mismatch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("source.z");
        fs::write(&path, "abcdef").expect("write original");
        let guard = source_guard(&path, "abcdef").expect("guard");
        fs::write(&path, "abcxyz").expect("mutate source");

        let mut plan = RefactorPlan::new("test", RefactorMode::Write, temp.path(), None);
        let mut file = RefactorFilePlan::new(&path, "source.z", guard);
        file.edits
            .push(RefactorEdit::new(SourceSpan::bytes(0, 3), "ABC", None));
        plan.files.push(file);

        let error = apply_refactor_plan(&plan).expect_err("guard mismatch");
        assert!(error.to_string().contains("source guard mismatch"));
        assert_eq!(fs::read_to_string(&path).expect("source"), "abcxyz");
    }

    #[test]
    fn preview_serializes_schema_and_plan() {
        let mut plan = RefactorPlan::new(
            "promote",
            RefactorMode::Preview,
            "/tmp/zorg-root",
            Some("alpha".to_owned()),
        );
        plan.warnings.push("dry run".to_owned());
        let preview = RefactorPreview::new(plan);

        let json = serde_json::to_value(preview).expect("json");
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["plan"]["operation"], "promote");
        assert_eq!(json["plan"]["mode"], "preview");
        assert_eq!(json["plan"]["target_id"], "alpha");
        assert_eq!(json["plan"]["warnings"][0], "dry run");
    }

    #[test]
    fn rename_rewrites_relative_forms_only_when_deterministic() {
        let child = reference_replacement(
            RenameReferenceContext {
                kind: RenameReferenceKind::Child,
                old_target_id: "root/old",
                containing_id: Some("root"),
                sibling_base_id: None,
            },
            "root/new",
        )
        .expect("child rewrite");
        assert_eq!(child, "+new");

        let nested = reference_replacement(
            RenameReferenceContext {
                kind: RenameReferenceKind::Child,
                old_target_id: "root/old",
                containing_id: Some("root"),
                sibling_base_id: None,
            },
            "root/new/deeper",
        )
        .expect_err("nested child refusal");
        assert!(nested.to_string().contains("cannot be rewritten"));
    }
}
