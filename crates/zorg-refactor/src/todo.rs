use std::fmt;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zorg_core::{BodyBlock, Property, SourceSpan, TodoMarker, Zettel, ZettelDocument, ZorgResult};

use crate::promote::{canonical_root, validate_planned_corpus};
use crate::{
    RefactorEdit, RefactorFilePlan, RefactorMode, RefactorPlan, SourceGuard, apply_refactor_plan,
    operation_failed, source_guard, source_slice, validate_and_sort_file_edits,
};

/// Todo lifecycle action to plan.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TodoActionKind {
    /// Mark the target todo done and set `did`.
    MarkDone { did: TodoActionDate },
    /// Move an existing `due` or `do` date.
    Postpone {
        field: TodoDateField,
        date: TodoActionDate,
    },
    /// Add or update the target `do` date.
    Schedule { date: TodoActionDate },
}

/// Date-bearing todo property field.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoDateField {
    /// `due::YYYY-MM-DD`.
    Due,
    /// `do::YYYY-MM-DD`.
    Do,
    /// `did::YYYY-MM-DD`.
    Did,
}

impl TodoDateField {
    fn key(self) -> &'static str {
        match self {
            Self::Due => "due",
            Self::Do => "do",
            Self::Did => "did",
        }
    }
}

/// Strict ISO calendar date accepted by the shared lifecycle planner.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct TodoActionDate {
    /// Normalized `YYYY-MM-DD` text.
    pub iso: String,
}

impl TodoActionDate {
    /// Parses strict `YYYY-MM-DD` text with basic calendar validation.
    pub fn parse(value: &str) -> ZorgResult<Self> {
        if value.len() != 10 {
            return Err(operation_failed("todo dates must use YYYY-MM-DD"));
        }
        let bytes = value.as_bytes();
        if bytes.get(4) != Some(&b'-')
            || bytes.get(7) != Some(&b'-')
            || !bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
        {
            return Err(operation_failed("todo dates must use YYYY-MM-DD"));
        }

        let year = value[0..4]
            .parse::<u16>()
            .map_err(|_| operation_failed("todo date year is invalid"))?;
        let month = value[5..7]
            .parse::<u8>()
            .map_err(|_| operation_failed("todo date month is invalid"))?;
        let day = value[8..10]
            .parse::<u8>()
            .map_err(|_| operation_failed("todo date day is invalid"))?;
        if !(1..=12).contains(&month) {
            return Err(operation_failed("todo date month must be 01 through 12"));
        }
        if day == 0 || day > days_in_month(year, month) {
            return Err(operation_failed("todo date day is outside the month"));
        }

        Ok(Self {
            iso: value.to_owned(),
        })
    }
}

impl fmt::Display for TodoActionDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.iso)
    }
}

/// Source-backed target selected from an indexed todo row.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TodoActionTarget {
    /// Stable store row ID for diagnostics and dashboard row identity.
    pub zettel_store_id: Option<i64>,
    /// Canonical zettel ID without `@`, when indexed.
    pub canonical_id: Option<String>,
    /// Absolute source path to rewrite.
    pub absolute_path: PathBuf,
    /// Source path relative to the corpus root.
    pub root_relative_path: PathBuf,
    /// Indexed source span for the target zettel.
    pub zettel_span: SourceSpan,
    /// Guard captured from the indexed source content.
    pub source_guard: SourceGuard,
}

/// Todo lifecycle planning request.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TodoActionRequest {
    /// Corpus root for the returned refactor plan.
    pub root: PathBuf,
    /// Selected source-backed target.
    pub target: TodoActionTarget,
    /// Requested refactor mode.
    pub mode: RefactorMode,
    /// Action to plan.
    pub action: TodoActionKind,
}

/// Displayable field change in a todo lifecycle plan.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TodoChange {
    /// Field changed, such as `todo`, `did`, `due`, or `do`.
    pub field: String,
    /// Previous display value, or `None` when adding a field.
    pub before: Option<String>,
    /// New display value, or `None` when removing a field.
    pub after: Option<String>,
}

/// Shared todo lifecycle plan with a guarded refactor plan payload.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TodoActionPlan {
    /// Stable operation name.
    pub operation: String,
    /// Requested action.
    pub action: TodoActionKind,
    /// Short display label for the selected zettel.
    pub target_summary: String,
    /// Selected zettel row ID when supplied by the caller.
    pub zettel_store_id: Option<i64>,
    /// Non-fatal planner notes.
    pub warnings: Vec<String>,
    /// Fatal planner rejections. Plans with rejections must not be written.
    pub rejections: Vec<String>,
    /// Displayable deterministic changes.
    pub changes: Vec<TodoChange>,
    /// Guarded file edit plan.
    pub refactor_plan: RefactorPlan,
}

/// Outcome after applying a write-mode todo plan.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TodoActionApplyOutcome {
    /// Changed source path.
    pub changed_path: PathBuf,
    /// Selected zettel row ID when supplied by the caller.
    pub zettel_store_id: Option<i64>,
    /// Selected canonical ID when supplied by the caller.
    pub canonical_id: Option<String>,
    /// Applied action.
    pub action: TodoActionKind,
    /// Fields changed by the write.
    pub changed_fields: Vec<String>,
}

/// Builds a guarded one-file todo lifecycle plan.
pub fn plan_todo_action(request: &TodoActionRequest) -> ZorgResult<TodoActionPlan> {
    let root = canonical_root(&request.root)?;
    let source = fs::read_to_string(&request.target.absolute_path).map_err(|error| {
        operation_failed(format!(
            "failed to read todo source {}: {error}",
            request.target.absolute_path.display()
        ))
    })?;
    let current_guard = source_guard(&request.target.absolute_path, &source)?;

    let mut refactor_plan = RefactorPlan::new(
        "todo",
        request.mode,
        &root,
        request.target.canonical_id.clone(),
    );
    let mut plan = TodoActionPlan {
        operation: "todo".to_owned(),
        action: request.action.clone(),
        target_summary: target_summary(&request.target),
        zettel_store_id: request.target.zettel_store_id,
        warnings: Vec::new(),
        rejections: Vec::new(),
        changes: Vec::new(),
        refactor_plan: refactor_plan.clone(),
    };

    if current_guard != request.target.source_guard {
        plan.rejections.push(format!(
            "source guard mismatch for {}; run reindex before planning todo writes",
            request.target.absolute_path.display()
        ));
        return Ok(plan);
    }

    let mut document =
        zorg_parse::parse_zettel_document_with_path(&source, &request.target.absolute_path)
            .map_err(|error| {
                operation_failed(format!(
                    "failed to parse todo source {}: {error}",
                    request.target.absolute_path.display()
                ))
            })?;
    zorg_parse::resolve_document(&mut document);

    let target = match find_target_zettel(&document, &request.target) {
        Ok(target) => target,
        Err(message) => {
            plan.rejections.push(message);
            return Ok(plan);
        }
    };

    match plan_action_edits(&source, target, &request.action) {
        Ok((edits, changes)) => {
            plan.changes = changes;
            if !edits.is_empty() {
                let mut file = RefactorFilePlan::new(
                    &request.target.absolute_path,
                    &request.target.root_relative_path,
                    request.target.source_guard.clone(),
                );
                file.edits = edits;
                validate_and_sort_file_edits(&source, &mut file.edits)?;
                refactor_plan.files.push(file);
                refactor_plan.sort_edits();
                validate_planned_corpus(
                    "todo",
                    &[loaded_source(&request.target, &source, &document)],
                    &refactor_plan,
                )?;
            }
            plan.refactor_plan = refactor_plan;
        }
        Err(message) => plan.rejections.push(message),
    }

    Ok(plan)
}

/// Applies a previously planned write-mode todo action.
pub fn apply_todo_action_plan(plan: &TodoActionPlan) -> ZorgResult<TodoActionApplyOutcome> {
    if !plan.rejections.is_empty() {
        return Err(operation_failed(
            "todo action plan has rejections and cannot be written",
        ));
    }
    apply_refactor_plan(&plan.refactor_plan)?;
    let changed_path = plan
        .refactor_plan
        .files
        .first()
        .map(|file| file.absolute_path.clone())
        .unwrap_or_default();
    Ok(TodoActionApplyOutcome {
        changed_path,
        zettel_store_id: plan.zettel_store_id,
        canonical_id: plan.refactor_plan.target_id.clone(),
        action: plan.action.clone(),
        changed_fields: plan
            .changes
            .iter()
            .map(|change| change.field.clone())
            .collect(),
    })
}

fn loaded_source(
    target: &TodoActionTarget,
    source: &str,
    document: &ZettelDocument,
) -> crate::LoadedSource {
    crate::LoadedSource {
        file: zorg_store::StoredFile {
            id: 0,
            absolute_path: target.absolute_path.clone(),
            relative_path: target.root_relative_path.clone(),
            mtime_unix_ms: target.source_guard.mtime_unix_ms,
            byte_len: i64::try_from(target.source_guard.byte_len).unwrap_or(i64::MAX),
            content_hash: target.source_guard.content_hash.clone(),
            indexed_at_unix_ms: None,
        },
        absolute_path: target.absolute_path.clone(),
        relative_path: target.root_relative_path.clone(),
        source: source.to_owned(),
        guard: target.source_guard.clone(),
        document: document.clone(),
    }
}

fn find_target_zettel<'a>(
    document: &'a ZettelDocument,
    target: &TodoActionTarget,
) -> Result<&'a Zettel, String> {
    let mut matches = Vec::new();
    collect_zettels_by_span(&document.root, target.zettel_span, &mut matches);
    if matches.len() == 1 {
        return Ok(matches[0]);
    }
    if matches.len() > 1 {
        return Err("target zettel span matched multiple reparsed zettels".to_owned());
    }

    let Some(canonical_id) = target.canonical_id.as_deref() else {
        return Err("target zettel span did not match reparsed source".to_owned());
    };
    let mut id_matches = Vec::new();
    collect_zettels_by_canonical_id(&document.root, canonical_id, &mut id_matches);
    match id_matches.as_slice() {
        [zettel] => Ok(*zettel),
        [] => Err(format!(
            "target zettel `@{canonical_id}` was not found after reparsing"
        )),
        _ => Err(format!(
            "target zettel `@{canonical_id}` matched multiple reparsed zettels"
        )),
    }
}

fn collect_zettels_by_span<'a>(
    zettel: &'a Zettel,
    span: SourceSpan,
    matches: &mut Vec<&'a Zettel>,
) {
    if zettel.span == Some(span) {
        matches.push(zettel);
    }
    for block in &zettel.body {
        if let BodyBlock::ChildZettel(child) = block {
            collect_zettels_by_span(child, span, matches);
        }
    }
}

fn collect_zettels_by_canonical_id<'a>(
    zettel: &'a Zettel,
    canonical_id: &str,
    matches: &mut Vec<&'a Zettel>,
) {
    if zettel
        .canonical_id
        .as_ref()
        .is_some_and(|id| id.as_str() == canonical_id)
    {
        matches.push(zettel);
    }
    for block in &zettel.body {
        if let BodyBlock::ChildZettel(child) = block {
            collect_zettels_by_canonical_id(child, canonical_id, matches);
        }
    }
}

fn plan_action_edits(
    source: &str,
    zettel: &Zettel,
    action: &TodoActionKind,
) -> Result<(Vec<RefactorEdit>, Vec<TodoChange>), String> {
    reject_duplicate_properties(zettel, &["did", "due", "do"])?;
    match action {
        TodoActionKind::MarkDone { did } => plan_mark_done(source, zettel, did),
        TodoActionKind::Postpone { field, date } => plan_postpone(zettel, *field, date),
        TodoActionKind::Schedule { date } => plan_schedule(zettel, date),
    }
}

fn plan_mark_done(
    source: &str,
    zettel: &Zettel,
    did: &TodoActionDate,
) -> Result<(Vec<RefactorEdit>, Vec<TodoChange>), String> {
    let (marker, marker_span) = todo_marker_and_span(zettel)?;
    if marker == TodoMarker::Unknown {
        return Err("todo marker `[?]` cannot be marked done safely".to_owned());
    }

    let mut edits = Vec::new();
    let mut changes = Vec::new();
    if marker != TodoMarker::Done {
        edits.push(RefactorEdit::new(
            marker_span,
            "[X]",
            Some("todo: mark done".to_owned()),
        ));
        changes.push(TodoChange {
            field: "todo".to_owned(),
            before: Some(marker.to_string()),
            after: Some("[X]".to_owned()),
        });
    }

    match properties_for(zettel, "did").as_slice() {
        [] => {
            edits.push(insert_after_todo_marker(
                marker_span,
                format!(" did::{}", did.iso),
                "did: add completion date",
            ));
            changes.push(TodoChange {
                field: "did".to_owned(),
                before: None,
                after: Some(did.iso.clone()),
            });
        }
        [property] if property.value == did.iso => {}
        [property] => {
            let span = property.value_span.ok_or_else(|| {
                "did property has no source span and cannot be updated safely".to_owned()
            })?;
            edits.push(RefactorEdit::new(
                span,
                did.iso.clone(),
                Some("did: update completion date".to_owned()),
            ));
            changes.push(TodoChange {
                field: "did".to_owned(),
                before: Some(property.value.clone()),
                after: Some(did.iso.clone()),
            });
        }
        _ => unreachable!("duplicates rejected before planning"),
    }

    validate_marker_slice(source, marker_span, marker)?;
    Ok((edits, changes))
}

fn plan_postpone(
    zettel: &Zettel,
    field: TodoDateField,
    date: &TodoActionDate,
) -> Result<(Vec<RefactorEdit>, Vec<TodoChange>), String> {
    if field == TodoDateField::Did {
        return Err("postpone only accepts due or do".to_owned());
    }
    let properties = properties_for(zettel, field.key());
    let [property] = properties.as_slice() else {
        return Err(format!(
            "postpone requires exactly one {} property on the selected zettel",
            field.key()
        ));
    };
    let span = property.value_span.ok_or_else(|| {
        format!(
            "{} property has no source span and cannot be updated safely",
            field.key()
        )
    })?;
    if property.value == date.iso {
        return Ok((Vec::new(), Vec::new()));
    }
    Ok((
        vec![RefactorEdit::new(
            span,
            date.iso.clone(),
            Some(format!("{}: postpone date", field.key())),
        )],
        vec![TodoChange {
            field: field.key().to_owned(),
            before: Some(property.value.clone()),
            after: Some(date.iso.clone()),
        }],
    ))
}

fn plan_schedule(
    zettel: &Zettel,
    date: &TodoActionDate,
) -> Result<(Vec<RefactorEdit>, Vec<TodoChange>), String> {
    let (marker, marker_span) = todo_marker_and_span(zettel)?;
    if !matches!(marker, TodoMarker::Open | TodoMarker::Next) {
        return Err("schedule requires an open or next todo marker".to_owned());
    }

    match properties_for(zettel, "do").as_slice() {
        [] => Ok((
            vec![insert_after_todo_marker(
                marker_span,
                format!(" do::{}", date.iso),
                "do: add scheduled date",
            )],
            vec![TodoChange {
                field: "do".to_owned(),
                before: None,
                after: Some(date.iso.clone()),
            }],
        )),
        [property] if property.value == date.iso => Ok((Vec::new(), Vec::new())),
        [property] => {
            let span = property.value_span.ok_or_else(|| {
                "do property has no source span and cannot be updated safely".to_owned()
            })?;
            Ok((
                vec![RefactorEdit::new(
                    span,
                    date.iso.clone(),
                    Some("do: update scheduled date".to_owned()),
                )],
                vec![TodoChange {
                    field: "do".to_owned(),
                    before: Some(property.value.clone()),
                    after: Some(date.iso.clone()),
                }],
            ))
        }
        _ => unreachable!("duplicates rejected before planning"),
    }
}

fn todo_marker_and_span(zettel: &Zettel) -> Result<(TodoMarker, SourceSpan), String> {
    let marker = zettel
        .todo
        .ok_or_else(|| "selected zettel has no todo marker".to_owned())?;
    let span = zettel
        .todo_span
        .ok_or_else(|| "selected zettel todo marker has no source span".to_owned())?;
    Ok((marker, span))
}

fn insert_after_todo_marker(
    marker_span: SourceSpan,
    replacement: String,
    label: &str,
) -> RefactorEdit {
    RefactorEdit::new(
        SourceSpan::bytes(marker_span.end_byte, marker_span.end_byte),
        replacement,
        Some(label.to_owned()),
    )
}

fn reject_duplicate_properties(zettel: &Zettel, keys: &[&str]) -> Result<(), String> {
    for key in keys {
        let count = zettel
            .properties
            .iter()
            .filter(|property| property.key == *key)
            .count();
        if count > 1 {
            return Err(format!(
                "selected zettel has duplicate {key} properties; refusing ambiguous todo write"
            ));
        }
    }
    Ok(())
}

fn properties_for<'a>(zettel: &'a Zettel, key: &str) -> Vec<&'a Property> {
    zettel
        .properties
        .iter()
        .filter(|property| property.key == key)
        .collect()
}

fn validate_marker_slice(source: &str, span: SourceSpan, marker: TodoMarker) -> Result<(), String> {
    let slice = source_slice(source, span).map_err(|error| error.to_string())?;
    if slice != marker.to_string() {
        return Err("todo marker span does not match the selected marker".to_owned());
    }
    Ok(())
}

fn target_summary(target: &TodoActionTarget) -> String {
    target
        .canonical_id
        .as_ref()
        .map(|id| format!("@{id}"))
        .unwrap_or_else(|| target.root_relative_path.display().to_string())
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: u16) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_source(source: &str) -> (tempfile::TempDir, PathBuf, TodoActionTarget) {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("tasks.z");
        fs::write(&path, source).expect("write source");
        let guard = source_guard(&path, source).expect("guard");
        let mut document =
            zorg_parse::parse_zettel_document_with_path(source, &path).expect("parse");
        zorg_parse::resolve_document(&mut document);
        let zettel = find_test_zettel(&document.root, "root/task").expect("target");
        let target = TodoActionTarget {
            zettel_store_id: Some(42),
            canonical_id: Some("root/task".to_owned()),
            absolute_path: path.clone(),
            root_relative_path: PathBuf::from("tasks.z"),
            zettel_span: zettel.span.expect("span"),
            source_guard: guard,
        };
        (temp, path, target)
    }

    #[test]
    fn mark_done_updates_marker_and_adds_did_without_touching_children() {
        let source = "\
%%% @root #z/ref
Root
%%%

- @root/task #z/todo [ ] due::2026-05-04 Task.
  Body.

  - ^child #z/ref Child.
";
        let (temp, path, target) = write_source(source);
        let request = TodoActionRequest {
            root: temp.path().to_path_buf(),
            target,
            mode: RefactorMode::Write,
            action: TodoActionKind::MarkDone {
                did: TodoActionDate::parse("2026-05-04").expect("date"),
            },
        };

        let plan = plan_todo_action(&request).expect("plan");
        assert!(plan.rejections.is_empty(), "{:?}", plan.rejections);
        assert_eq!(
            plan.changes,
            vec![
                TodoChange {
                    field: "todo".to_owned(),
                    before: Some("[ ]".to_owned()),
                    after: Some("[X]".to_owned())
                },
                TodoChange {
                    field: "did".to_owned(),
                    before: None,
                    after: Some("2026-05-04".to_owned())
                }
            ]
        );

        apply_todo_action_plan(&plan).expect("apply");
        assert_eq!(
            fs::read_to_string(path).expect("read"),
            "\
%%% @root #z/ref
Root
%%%

- @root/task #z/todo [X] did::2026-05-04 due::2026-05-04 Task.
  Body.

  - ^child #z/ref Child.
"
        );
    }

    #[test]
    fn mark_done_is_idempotent_when_did_today_already_exists() {
        let source = "\
%%% @root #z/ref
Root
%%%

- @root/task #z/todo [X] did::2026-05-04 Task.
";
        let (temp, _path, target) = write_source(source);
        let request = TodoActionRequest {
            root: temp.path().to_path_buf(),
            target,
            mode: RefactorMode::Preview,
            action: TodoActionKind::MarkDone {
                did: TodoActionDate::parse("2026-05-04").expect("date"),
            },
        };

        let plan = plan_todo_action(&request).expect("plan");
        assert!(plan.rejections.is_empty());
        assert!(plan.changes.is_empty());
        assert!(plan.refactor_plan.files.is_empty());
    }

    #[test]
    fn postpone_updates_only_requested_field() {
        let source = "\
%%% @root #z/ref
Root
%%%

- @root/task #z/todo [ ] due::2026-05-04 do::2026-05-03 Task.
";
        let (temp, _path, target) = write_source(source);
        let request = TodoActionRequest {
            root: temp.path().to_path_buf(),
            target,
            mode: RefactorMode::Preview,
            action: TodoActionKind::Postpone {
                field: TodoDateField::Due,
                date: TodoActionDate::parse("2026-05-09").expect("date"),
            },
        };

        let plan = plan_todo_action(&request).expect("plan");
        assert!(plan.rejections.is_empty(), "{:?}", plan.rejections);
        let next = crate::apply_edits_to_source(source, &plan.refactor_plan.files[0].edits)
            .expect("apply to source");
        assert!(next.contains("due::2026-05-09 do::2026-05-03"));
    }

    #[test]
    fn schedule_adds_do_to_open_todo() {
        let source = "\
%%% @root #z/ref
Root
%%%

- @root/task #z/todo [N] Task.
";
        let (temp, _path, target) = write_source(source);
        let request = TodoActionRequest {
            root: temp.path().to_path_buf(),
            target,
            mode: RefactorMode::Preview,
            action: TodoActionKind::Schedule {
                date: TodoActionDate::parse("2026-05-09").expect("date"),
            },
        };

        let plan = plan_todo_action(&request).expect("plan");
        assert!(plan.rejections.is_empty(), "{:?}", plan.rejections);
        let next = crate::apply_edits_to_source(source, &plan.refactor_plan.files[0].edits)
            .expect("apply to source");
        assert!(next.contains("[N] do::2026-05-09 Task."));
    }

    #[test]
    fn duplicate_lifecycle_properties_fail_closed() {
        let source = "\
%%% @root #z/ref
Root
%%%

- @root/task #z/todo [ ] did::2026-05-03 did::2026-05-04 Task.
";
        let (temp, _path, target) = write_source(source);
        let request = TodoActionRequest {
            root: temp.path().to_path_buf(),
            target,
            mode: RefactorMode::Preview,
            action: TodoActionKind::MarkDone {
                did: TodoActionDate::parse("2026-05-04").expect("date"),
            },
        };

        let plan = plan_todo_action(&request).expect("plan");
        assert!(plan.rejections[0].contains("duplicate did"));
    }

    #[test]
    fn stale_source_guard_fails_without_edits() {
        let source = "\
%%% @root #z/ref
Root
%%%

- @root/task #z/todo [ ] Task.
";
        let (temp, path, mut target) = write_source(source);
        target.source_guard.content_hash = "stale".to_owned();
        let request = TodoActionRequest {
            root: temp.path().to_path_buf(),
            target,
            mode: RefactorMode::Preview,
            action: TodoActionKind::Schedule {
                date: TodoActionDate::parse("2026-05-09").expect("date"),
            },
        };

        let plan = plan_todo_action(&request).expect("plan");
        assert!(plan.rejections[0].contains("source guard mismatch"));
        assert!(plan.refactor_plan.files.is_empty());
        assert_eq!(fs::read_to_string(path).expect("read"), source);
    }

    #[test]
    fn date_parser_rejects_malformed_dates() {
        assert!(TodoActionDate::parse("2026-5-04").is_err());
        assert!(TodoActionDate::parse("2026-02-30").is_err());
        assert!(TodoActionDate::parse("2024-02-29").is_ok());
    }

    fn find_test_zettel<'a>(zettel: &'a Zettel, canonical_id: &str) -> Option<&'a Zettel> {
        if zettel
            .canonical_id
            .as_ref()
            .is_some_and(|id| id.as_str() == canonical_id)
        {
            return Some(zettel);
        }
        for block in &zettel.body {
            if let BodyBlock::ChildZettel(child) = block {
                if let Some(found) = find_test_zettel(child, canonical_id) {
                    return Some(found);
                }
            }
        }
        None
    }
}
