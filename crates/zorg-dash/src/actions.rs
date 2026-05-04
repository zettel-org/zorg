use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use zorg_capture::{CaptureRequest, CaptureResult, CaptureTemplate};
use zorg_core::{BodyBlock, Severity, SourcePath, Zettel, ZettelDocument};
use zorg_fix::{
    CorpusView, DiagnosticFixSelector, FixPlan, FixPreviewSet, FixUnavailableReason,
    LineColumnSpan, apply_selected_fix_to_source, plan_fixes, preview_diagnostic_fix,
    validate_rewritten_documents,
};
use zorg_parse::parse_zettel_document_with_path;
use zorg_refactor::{
    RefactorMode, SourceGuard, TodoActionApplyOutcome as PlannerTodoApplyOutcome, TodoActionDate,
    TodoActionKind, TodoActionPlan, TodoActionRequest, TodoActionTarget, TodoDateField,
    apply_todo_action_plan, plan_todo_action,
};
use zorg_store::{IndexStatus, ReindexSummary, Store, StoreOptions};

use crate::data;
use crate::data::current_query_date;
use crate::model::{
    CaptureDraft, CaptureTemplateRow, DashboardSnapshot, DiagnosticPreviewContext, DiagnosticRow,
    FixPreviewOverlay, FixPreviewRow, SourceLocation, ZettelRow,
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ReindexOutcome {
    pub(crate) summary: ReindexSummary,
    pub(crate) snapshot: DashboardSnapshot,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CaptureOutcome {
    pub(crate) result: CaptureResult,
    pub(crate) snapshot: DashboardSnapshot,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct FixApplyOutcome {
    pub(crate) changed_path: PathBuf,
    pub(crate) applied_rule_codes: Vec<String>,
    pub(crate) applied_edits: usize,
    pub(crate) reindex_summary: ReindexSummary,
    pub(crate) snapshot: DashboardSnapshot,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct TodoApplyOutcome {
    pub(crate) planner: PlannerTodoApplyOutcome,
    pub(crate) reindex_summary: ReindexSummary,
    pub(crate) snapshot: DashboardSnapshot,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum ClipboardTransport {
    Osc52,
    PlatformCommand(&'static str),
}

impl ClipboardTransport {
    pub(crate) const fn label(&self) -> &'static str {
        match self {
            Self::Osc52 => "OSC 52",
            Self::PlatformCommand(label) => label,
        }
    }
}

pub(crate) fn refresh_snapshot(
    options: StoreOptions,
    query: Option<String>,
    dashboard_id: Option<String>,
) -> DashboardSnapshot {
    data::load_snapshot(options, query.as_deref(), dashboard_id.as_deref())
}

pub(crate) fn copy_to_clipboard(
    value: &str,
    stdout_is_terminal: bool,
) -> Result<ClipboardTransport, String> {
    if stdout_is_terminal {
        write_osc52(value)?;
        return Ok(ClipboardTransport::Osc52);
    }

    try_platform_clipboard_commands(value, platform_clipboard_commands())
}

pub(crate) fn reindex(
    options: StoreOptions,
    query: Option<String>,
    dashboard_id: Option<String>,
) -> Result<ReindexOutcome, String> {
    let mut store = Store::open_with_options(options.clone()).map_err(|error| error.to_string())?;
    let summary = store.reindex().map_err(|error| error.to_string())?;
    let snapshot = data::load_snapshot(options, query.as_deref(), dashboard_id.as_deref());
    Ok(ReindexOutcome { summary, snapshot })
}

pub(crate) fn capture_templates(root: &Path) -> Result<Vec<CaptureTemplateRow>, String> {
    let templates = zorg_capture::list_templates(root).map_err(|error| error.to_string())?;
    Ok(templates.iter().map(capture_template_row).collect())
}

pub(crate) fn capture(
    options: StoreOptions,
    query: Option<String>,
    dashboard_id: Option<String>,
    draft: CaptureDraft,
) -> Result<CaptureOutcome, String> {
    let request = CaptureRequest {
        root: options.corpus_root().to_path_buf(),
        template: required_field(&draft.template, "template")?,
        title: optional_field(draft.title),
        source: Some("zorg dash".to_owned()),
        body: optional_field(draft.body),
        dest: optional_path(draft.destination),
        id: None,
        allow_outside: false,
    };
    let result = zorg_capture::capture(&request).map_err(|error| error.to_string())?;
    let snapshot = data::load_snapshot(options, query.as_deref(), dashboard_id.as_deref());
    Ok(CaptureOutcome { result, snapshot })
}

pub(crate) fn todo_mark_done_preview(
    options: StoreOptions,
    row: &ZettelRow,
) -> Result<TodoActionPlan, String> {
    ensure_index_current(&options, "todo mark done")?;
    let target = todo_target_from_row(&options, row, "todo mark done")?;
    let today = current_query_date();
    let did = TodoActionDate::parse(&format!(
        "{:04}-{:02}-{:02}",
        today.year, today.month, today.day
    ))
    .map_err(|error| error.to_string())?;
    let request = TodoActionRequest {
        root: options.corpus_root().to_path_buf(),
        target,
        mode: RefactorMode::Write,
        action: TodoActionKind::MarkDone { did },
    };
    let plan = plan_todo_action(&request).map_err(|error| error.to_string())?;
    if !plan.rejections.is_empty() {
        return Err(format!(
            "todo mark done rejected:\n{}",
            plan.rejections.join("\n")
        ));
    }
    Ok(plan)
}

pub(crate) fn todo_postpone_preview(
    options: StoreOptions,
    row: &ZettelRow,
    field: TodoDateField,
    date: TodoActionDate,
) -> Result<TodoActionPlan, String> {
    ensure_index_current(&options, "todo postpone")?;
    let target = todo_target_from_row(&options, row, "todo postpone")?;
    let request = TodoActionRequest {
        root: options.corpus_root().to_path_buf(),
        target,
        mode: RefactorMode::Write,
        action: TodoActionKind::Postpone { field, date },
    };
    let plan = plan_todo_action(&request).map_err(|error| error.to_string())?;
    if !plan.rejections.is_empty() {
        return Err(format!(
            "todo postpone rejected:\n{}",
            plan.rejections.join("\n")
        ));
    }
    Ok(plan)
}

pub(crate) fn todo_schedule_preview(
    options: StoreOptions,
    row: &ZettelRow,
    date: TodoActionDate,
) -> Result<TodoActionPlan, String> {
    ensure_index_current(&options, "todo schedule")?;
    let target = todo_target_from_row(&options, row, "todo schedule")?;
    let request = TodoActionRequest {
        root: options.corpus_root().to_path_buf(),
        target,
        mode: RefactorMode::Write,
        action: TodoActionKind::Schedule { date },
    };
    let plan = plan_todo_action(&request).map_err(|error| error.to_string())?;
    if !plan.rejections.is_empty() {
        return Err(format!(
            "todo schedule rejected:\n{}",
            plan.rejections.join("\n")
        ));
    }
    Ok(plan)
}

pub(crate) fn parse_todo_prompt_date(input: &str) -> Result<TodoActionDate, String> {
    parse_todo_prompt_date_from(input, current_query_date())
}

pub(crate) fn parse_todo_prompt_date_from(
    input: &str,
    today: zorg_query::QueryDate,
) -> Result<TodoActionDate, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("date is required; use YYYY-MM-DD, +1d, or +1w".to_owned());
    }

    if let Some(date) = parse_relative_todo_date(trimmed, today)? {
        return Ok(date);
    }

    TodoActionDate::parse(trimmed).map_err(|error| error.to_string())
}

pub(crate) fn todo_apply(
    options: StoreOptions,
    query: Option<String>,
    dashboard_id: Option<String>,
    plan: TodoActionPlan,
) -> Result<TodoApplyOutcome, String> {
    ensure_index_current(&options, "todo apply")?;
    let planner = apply_todo_action_plan(&plan).map_err(|error| error.to_string())?;
    let mut store = Store::open_with_options(options.clone()).map_err(|error| error.to_string())?;
    let reindex_summary = store.reindex().map_err(|error| error.to_string())?;
    drop(store);
    let snapshot = data::load_snapshot(options, query.as_deref(), dashboard_id.as_deref());
    Ok(TodoApplyOutcome {
        planner,
        reindex_summary,
        snapshot,
    })
}

pub(crate) fn fix_preview(
    options: StoreOptions,
    diagnostic: DiagnosticRow,
) -> Result<FixPreviewOverlay, String> {
    let selector = selector_for_diagnostic(options.corpus_root(), &diagnostic);
    let Some(path) = diagnostic_source_path(options.corpus_root(), &diagnostic) else {
        let preview_set = preview_diagnostic_fix(&FixPlan::default(), &selector);
        return Ok(overlay_from_preview_set(&diagnostic, preview_set, selector));
    };

    let source = std::fs::read_to_string(&path).map_err(|error| {
        format!(
            "fix preview failed: could not read {}: {error}",
            display_path(&path)
        )
    })?;
    let document = parse_zettel_document_with_path(&source, &path).map_err(|error| {
        format!(
            "fix preview failed: could not parse {}: {error}",
            display_path(&path)
        )
    })?;
    let canonical_ids = indexed_canonical_ids(options)?;
    let corpus_view = CorpusView::from_canonical_ids(canonical_ids.iter().map(String::as_str));
    let plan = plan_fixes(&document, &corpus_view);
    let preview_set = preview_diagnostic_fix(&plan, &selector);
    Ok(overlay_from_preview_set(&diagnostic, preview_set, selector))
}

pub(crate) fn fix_apply(
    options: StoreOptions,
    query: Option<String>,
    dashboard_id: Option<String>,
    selector: DiagnosticFixSelector,
) -> Result<FixApplyOutcome, String> {
    ensure_index_current(&options, "fix apply")?;
    let path = selector
        .path
        .as_ref()
        .map(|path| path.as_path().to_path_buf())
        .ok_or_else(|| "fix apply failed: selected diagnostic has no source path".to_owned())?;

    let mut documents = parse_corpus_documents(&options)?;
    let target_index = documents
        .iter()
        .position(|document| {
            document
                .path
                .as_ref()
                .is_some_and(|document_path| document_path.as_path() == path.as_path())
        })
        .ok_or_else(|| {
            format!(
                "fix apply failed: selected source {} is not indexed",
                display_path(&path)
            )
        })?;

    let canonical_ids = collect_document_canonical_ids(&documents);
    let corpus_view = CorpusView::from_canonical_ids(canonical_ids.iter().map(String::as_str));
    let plan = plan_fixes(&documents[target_index], &corpus_view);
    let summary = apply_selected_fix_to_source(&documents[target_index].source, &plan, &selector)
        .map_err(|error| format!("fix apply failed: {error}"))?;
    let rewritten_document =
        parse_zettel_document_with_path(&summary.source, &path).map_err(|error| {
            format!(
                "fix apply failed: rewritten {} did not parse: {error}",
                display_path(&path)
            )
        })?;
    documents[target_index] = rewritten_document;

    let diagnostics = validate_rewritten_documents(&mut documents);
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(format_apply_validation_failure(&diagnostics));
    }

    if summary.changed {
        atomic_write(&path, &summary.source, 0)?;
    }

    let mut store = Store::open_with_options(options.clone()).map_err(|error| error.to_string())?;
    let reindex_summary = store.reindex().map_err(|error| error.to_string())?;
    drop(store);
    let snapshot = data::load_snapshot(options, query.as_deref(), dashboard_id.as_deref());
    Ok(FixApplyOutcome {
        changed_path: path,
        applied_rule_codes: summary
            .applied_rule_codes
            .into_iter()
            .map(str::to_owned)
            .collect(),
        applied_edits: summary.applied_edits,
        reindex_summary,
        snapshot,
    })
}

pub(crate) fn open_in_editor(location: &SourceLocation) -> Result<(), String> {
    let editor = editor_command_from_env()?;
    open_in_editor_with_command(location, &editor)
}

fn editor_command_from_env() -> Result<String, String> {
    env::var("EDITOR")
        .map_err(|_| "open failed: $EDITOR is not set".to_owned())
        .and_then(|value| validate_editor_command(&value).map(str::to_owned))
}

fn validate_editor_command(editor: &str) -> Result<&str, String> {
    if editor.trim().is_empty() {
        Err("open failed: $EDITOR is empty".to_owned())
    } else {
        Ok(editor)
    }
}

fn open_in_editor_with_command(location: &SourceLocation, editor: &str) -> Result<(), String> {
    let editor = validate_editor_command(editor)?;
    let mut parts = editor.split_whitespace();
    let Some(program) = parts.next() else {
        return Err("open failed: $EDITOR is empty".to_owned());
    };

    let mut command = Command::new(program);
    command.args(parts);
    if is_code_editor(program) {
        let line = location.line.unwrap_or(1);
        let column = location.column.unwrap_or(1);
        command
            .arg("--goto")
            .arg(format!("{}:{line}:{column}", display_path(&location.path)));
    } else {
        if let Some(line) = location.line {
            command.arg(format!("+{line}"));
        }
        command.arg(&location.path);
    }
    let status = command.status().map_err(|error| {
        format!(
            "open failed: could not launch `{}` for {}: {error}",
            editor,
            display_path(&location.path)
        )
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "open failed: editor exited with status {status} for {}",
            display_path(&location.path)
        ))
    }
}

pub(crate) fn reindex_summary_line(summary: ReindexSummary) -> String {
    format!(
        "reindex complete: discovered {} indexed {} unchanged {} new {} changed {} deleted {} diagnostics {}",
        summary.discovered_files,
        summary.indexed_files,
        summary.unchanged_files,
        summary.new_files,
        summary.changed_files,
        summary.deleted_files,
        summary.diagnostic_count
    )
}

fn todo_target_from_row(
    options: &StoreOptions,
    row: &ZettelRow,
    operation: &str,
) -> Result<TodoActionTarget, String> {
    let root = options.corpus_root();
    let root_relative_path = root_relative_row_path(root, &row.file_path);
    let store =
        Store::open_read_only_with_options(options.clone()).map_err(|error| error.to_string())?;
    let indexed_file = store
        .list_files()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|file| file.relative_path == root_relative_path)
        .ok_or_else(|| {
            format!(
                "{operation} failed: indexed file {} was not found",
                root_relative_path.display()
            )
        })?;
    let byte_len = u64::try_from(indexed_file.byte_len).map_err(|_| {
        format!(
            "{operation} failed: indexed byte length is invalid for {}",
            indexed_file.relative_path.display()
        )
    })?;

    Ok(TodoActionTarget {
        zettel_store_id: Some(row.store_id),
        canonical_id: row.canonical_id.clone(),
        absolute_path: indexed_file.absolute_path,
        root_relative_path: indexed_file.relative_path,
        zettel_span: row.source_span,
        source_guard: SourceGuard {
            content_hash: indexed_file.content_hash,
            mtime_unix_ms: indexed_file.mtime_unix_ms,
            byte_len,
        },
    })
}

fn parse_relative_todo_date(
    value: &str,
    today: zorg_query::QueryDate,
) -> Result<Option<TodoActionDate>, String> {
    let Some(rest) = value.strip_prefix('+') else {
        return Ok(None);
    };
    let Some(unit) = rest.chars().last() else {
        return Ok(None);
    };
    if !matches!(unit, 'd' | 'w') {
        return Ok(None);
    }
    let amount_text = &rest[..rest.len().saturating_sub(unit.len_utf8())];
    if amount_text.is_empty() || !amount_text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("relative dates must look like +1d or +1w".to_owned());
    }
    let amount = amount_text
        .parse::<i64>()
        .map_err(|_| "relative date amount is too large".to_owned())?;
    if amount == 0 {
        return Err("relative date amount must be greater than zero".to_owned());
    }
    let days = amount
        .checked_mul(if unit == 'w' { 7 } else { 1 })
        .ok_or_else(|| "relative date amount is too large".to_owned())?;
    let date = add_days(today, days)?;
    TodoActionDate::parse(&format_query_date(date))
        .map(Some)
        .map_err(|error| error.to_string())
}

fn format_query_date(date: zorg_query::QueryDate) -> String {
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
}

fn add_days(date: zorg_query::QueryDate, days: i64) -> Result<zorg_query::QueryDate, String> {
    let base = days_from_civil(date.year, date.month, date.day);
    let shifted = base
        .checked_add(i128::from(days))
        .ok_or_else(|| "relative date is outside supported range".to_owned())?;
    let (year, month, day) = civil_from_days(shifted);
    zorg_query::QueryDate::new(year, month, day)
        .ok_or_else(|| "relative date is outside supported range".to_owned())
}

fn days_from_civil(year: i32, month: u8, day: u8) -> i128 {
    let mut year = i128::from(year);
    let month = i128::from(month);
    let day = i128::from(day);
    year -= if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days_since_epoch: i128) -> (i32, u8, u8) {
    let days = days_since_epoch + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (
        year.try_into().unwrap_or(if year.is_negative() {
            i32::MIN
        } else {
            i32::MAX
        }),
        month.try_into().unwrap_or(1),
        day.try_into().unwrap_or(1),
    )
}

fn root_relative_row_path(root: &Path, row_path: &Path) -> PathBuf {
    row_path
        .strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| row_path.to_path_buf())
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn indexed_canonical_ids(options: StoreOptions) -> Result<Vec<String>, String> {
    let store = Store::open_read_only_with_options(options).map_err(|error| error.to_string())?;
    store
        .list_zettel()
        .map_err(|error| error.to_string())
        .map(|rows| {
            rows.into_iter()
                .filter_map(|row| row.canonical_id)
                .collect()
        })
}

fn ensure_index_current(options: &StoreOptions, operation: &str) -> Result<(), String> {
    let store =
        Store::open_read_only_with_options(options.clone()).map_err(|error| error.to_string())?;
    let status = store.index_status().map_err(|error| error.to_string())?;
    if index_has_source_changes(&status) {
        return Err(format!(
            "{operation} refused: index is stale relative to source (new {} changed {} deleted {}); reindex before applying",
            status.new_files, status.changed_files, status.deleted_files
        ));
    }
    Ok(())
}

fn index_has_source_changes(status: &IndexStatus) -> bool {
    status.new_files > 0 || status.changed_files > 0 || status.deleted_files > 0
}

fn parse_corpus_documents(options: &StoreOptions) -> Result<Vec<ZettelDocument>, String> {
    let store =
        Store::open_read_only_with_options(options.clone()).map_err(|error| error.to_string())?;
    store
        .discover_sources()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|source| {
            let path = source.absolute_path().to_path_buf();
            let source_text = fs::read_to_string(&path).map_err(|error| {
                format!(
                    "fix apply failed: could not read {}: {error}",
                    display_path(&path)
                )
            })?;
            parse_zettel_document_with_path(&source_text, &path).map_err(|error| {
                format!(
                    "fix apply failed: could not parse {}: {error}",
                    display_path(&path)
                )
            })
        })
        .collect()
}

fn collect_document_canonical_ids(documents: &[ZettelDocument]) -> Vec<String> {
    let mut ids = Vec::new();
    for document in documents {
        collect_zettel_canonical_ids(&document.root, &mut ids);
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn collect_zettel_canonical_ids(zettel: &Zettel, ids: &mut Vec<String>) {
    if let Some(canonical) = zettel
        .canonical_id
        .as_ref()
        .or(zettel.id.as_ref())
        .map(zorg_core::ZettelId::as_str)
    {
        ids.push(canonical.to_owned());
    }

    for block in &zettel.body {
        if let BodyBlock::ChildZettel(child) = block {
            collect_zettel_canonical_ids(child, ids);
        }
    }
}

fn format_apply_validation_failure(diagnostics: &[zorg_core::Diagnostic]) -> String {
    let mut lines =
        vec!["fix apply refused: rewritten sources failed strict validation".to_owned()];
    lines.extend(diagnostics.iter().take(6).map(|diagnostic| {
        let path = diagnostic
            .path
            .as_ref()
            .map(|path| display_path(path.as_path()))
            .unwrap_or_else(|| "<unknown>".to_owned());
        let code = diagnostic.code.as_deref().unwrap_or("diagnostic");
        format!("{path}: {code}: {}", diagnostic.message)
    }));
    lines.join("\n")
}

fn atomic_write(path: &Path, source: &str, index: usize) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("source.z");
    let temp_path = parent.join(format!(
        ".{file_name}.zorg-dash-fix-{}-{index}.tmp",
        std::process::id()
    ));

    fs::write(&temp_path, source).map_err(|error| {
        format!(
            "fix apply failed: could not write temporary {}: {error}",
            display_path(&temp_path)
        )
    })?;
    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        format!(
            "fix apply failed: could not replace {}: {error}",
            display_path(path)
        )
    })
}

fn selector_for_diagnostic(root: &Path, diagnostic: &DiagnosticRow) -> DiagnosticFixSelector {
    let path = diagnostic_source_path(root, diagnostic).map(SourcePath::new);
    let code = diagnostic.code.clone();
    let rule_code = code
        .as_deref()
        .filter(|code| code.starts_with("fix."))
        .map(str::to_owned);
    let diagnostic_code = code.filter(|code| !code.starts_with("fix."));

    DiagnosticFixSelector {
        path,
        diagnostic_code,
        rule_code,
        severity: severity_from_label(&diagnostic.severity),
        message: Some(diagnostic.message.clone()),
        byte_span: diagnostic.start_byte.zip(diagnostic.end_byte),
        line_column_span: line_column_span_for_diagnostic(diagnostic),
    }
}

fn diagnostic_source_path(root: &Path, diagnostic: &DiagnosticRow) -> Option<PathBuf> {
    diagnostic.absolute_path.clone().or_else(|| {
        diagnostic
            .relative_path
            .as_ref()
            .map(|path| root.join(path))
    })
}

fn severity_from_label(label: &str) -> Option<Severity> {
    match label {
        "error" => Some(Severity::Error),
        "warning" => Some(Severity::Warning),
        "info" => Some(Severity::Info),
        _ => None,
    }
}

fn line_column_span_for_diagnostic(diagnostic: &DiagnosticRow) -> Option<LineColumnSpan> {
    Some(LineColumnSpan::new(
        diagnostic.start_line?,
        diagnostic.start_column?,
        diagnostic.end_line?,
        diagnostic.end_column?,
    ))
}

fn overlay_from_preview_set(
    diagnostic: &DiagnosticRow,
    preview_set: FixPreviewSet,
    selector: DiagnosticFixSelector,
) -> FixPreviewOverlay {
    FixPreviewOverlay {
        diagnostic: DiagnosticPreviewContext {
            severity: diagnostic.severity.clone(),
            code: diagnostic
                .code
                .clone()
                .unwrap_or_else(|| diagnostic.category.clone()),
            message: diagnostic.message.clone(),
            path: diagnostic
                .absolute_path
                .as_ref()
                .or(diagnostic.relative_path.as_ref())
                .map(|path| display_path(path))
                .unwrap_or_else(|| "-".to_owned()),
            position: preview_position_text(
                diagnostic.start_line,
                diagnostic.start_column,
                diagnostic.end_line,
                diagnostic.end_column,
            ),
        },
        previews: preview_set
            .previews
            .into_iter()
            .map(|preview| FixPreviewRow {
                rule_code: preview.rule_code.to_owned(),
                severity: severity_label(preview.severity).to_owned(),
                path: preview.path.into_path_buf(),
                primary_line: preview.primary_line,
                primary_column: preview.primary_column,
                replacement_preview: preview.replacement_preview,
                replacement_truncated: preview.replacement_truncated,
                is_preferred: preview.is_preferred,
                is_safe: preview.is_safe,
                explanation: preview.explanation,
            })
            .collect(),
        unavailable_reason: preview_set.unavailable_reason.map(unavailable_reason_text),
        selector,
        marked_summary: None,
    }
}

fn preview_position_text(
    start_line: Option<usize>,
    start_column: Option<usize>,
    end_line: Option<usize>,
    end_column: Option<usize>,
) -> String {
    let start = location_text(start_line, start_column);
    let end = location_text(end_line, end_column);
    if start == "-" || end == "-" || start == end {
        start
    } else {
        format!("{start}-{end}")
    }
}

fn location_text(line: Option<usize>, column: Option<usize>) -> String {
    match (line, column) {
        (Some(line), Some(column)) => format!("{line}:{column}"),
        (Some(line), None) => line.to_string(),
        _ => "-".to_owned(),
    }
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

fn unavailable_reason_text(reason: FixUnavailableReason) -> String {
    match reason {
        FixUnavailableReason::SourcePathUnavailable => {
            "Source path is unavailable for this diagnostic.".to_owned()
        }
        FixUnavailableReason::DiagnosticHasNoSourceSpan => {
            "Diagnostic has no source span to match against a safe fix.".to_owned()
        }
        FixUnavailableReason::KnownUnavailable { explanation, .. } => explanation,
        FixUnavailableReason::NoMatchingFix => {
            "No safe matching fix was found for this diagnostic.".to_owned()
        }
        FixUnavailableReason::AmbiguousMatchingFixes { count } => {
            format!("{count} matching fixes were found; choosing one would be ambiguous.")
        }
    }
}

fn is_code_editor(program: &str) -> bool {
    Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, "code" | "code-insiders" | "codium" | "cursor"))
}

fn template_selector(template: &CaptureTemplate) -> Option<String> {
    template
        .id
        .as_ref()
        .map(|id| id.declaration())
        .or_else(|| template.title.clone())
}

fn capture_template_row(template: &CaptureTemplate) -> CaptureTemplateRow {
    CaptureTemplateRow {
        selector: template_selector(template),
        id: template.id.as_ref().map(|id| id.declaration()),
        title: template.title.clone(),
        destination: template
            .destination
            .as_ref()
            .map(|destination| destination.display().to_string()),
        path: template.path.clone(),
        variables: template.variables.clone(),
    }
}

fn required_field(value: &str, label: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("capture failed: {label} is required"))
    } else {
        Ok(trimmed.to_owned())
    }
}

fn optional_field(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn optional_path(value: String) -> Option<PathBuf> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

#[derive(Debug, Clone, Copy)]
struct PlatformClipboardCommand {
    label: &'static str,
    program: &'static str,
    args: &'static [&'static str],
}

fn write_osc52(value: &str) -> Result<(), String> {
    let mut stdout = std::io::stdout();
    write!(stdout, "\x1b]52;c;{}\x07", base64_encode(value.as_bytes()))
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("OSC 52 clipboard write failed: {error}"))
}

fn try_platform_clipboard_commands(
    value: &str,
    commands: &[PlatformClipboardCommand],
) -> Result<ClipboardTransport, String> {
    let mut failures = Vec::new();
    for command in commands {
        match run_platform_clipboard_command(value, command) {
            Ok(()) => return Ok(ClipboardTransport::PlatformCommand(command.label)),
            Err(message) => failures.push(message),
        }
    }

    let mut message = "clipboard transport unavailable: stdout is not a terminal".to_owned();
    if commands.is_empty() {
        message.push_str(" and no local clipboard command is configured for this platform");
    } else {
        message.push_str(" and local clipboard commands failed");
        if !failures.is_empty() {
            message.push_str(":\n");
            message.push_str(&failures.join("\n"));
        }
    }
    Err(message)
}

fn run_platform_clipboard_command(
    value: &str,
    command: &PlatformClipboardCommand,
) -> Result<(), String> {
    let mut child = Command::new(command.program)
        .args(command.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("{}: {error}", command.label))?;

    if let Some(stdin) = &mut child.stdin {
        stdin
            .write_all(value.as_bytes())
            .map_err(|error| format!("{} stdin: {error}", command.label))?;
    }

    let status = child
        .wait()
        .map_err(|error| format!("{} wait: {error}", command.label))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} exited with {status}", command.label))
    }
}

fn platform_clipboard_commands() -> &'static [PlatformClipboardCommand] {
    #[cfg(target_os = "macos")]
    {
        &[PlatformClipboardCommand {
            label: "pbcopy",
            program: "pbcopy",
            args: &[],
        }]
    }
    #[cfg(target_os = "linux")]
    {
        &[
            PlatformClipboardCommand {
                label: "wl-copy",
                program: "wl-copy",
                args: &[],
            },
            PlatformClipboardCommand {
                label: "xclip",
                program: "xclip",
                args: &["-selection", "clipboard"],
            },
        ]
    }
    #[cfg(target_os = "windows")]
    {
        &[PlatformClipboardCommand {
            label: "clip",
            program: "clip",
            args: &[],
        }]
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        &[]
    }
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);

        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[(((first & 0b0000_0011) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((second & 0b0000_1111) << 2) | (third >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(third & 0b0011_1111) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn open_reports_empty_editor_explicitly() {
        let error = open_in_editor_with_command(
            &SourceLocation {
                path: PathBuf::from("note.z"),
                line: Some(2),
                column: Some(1),
            },
            "",
        )
        .expect_err("empty editor should fail");

        assert!(error.contains("$EDITOR is empty"));
    }

    #[test]
    fn detects_common_goto_style_editors() {
        assert!(is_code_editor("code"));
        assert!(is_code_editor("/usr/bin/cursor"));
        assert!(!is_code_editor("vim"));
    }

    #[test]
    fn base64_encoder_matches_osc52_examples() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"@task"), "QHRhc2s=");
        assert_eq!(base64_encode(b"notes/a.z:1:1"), "bm90ZXMvYS56OjE6MQ==");
    }

    #[test]
    fn clipboard_transport_reports_explicit_unavailable_fallback() {
        let error = try_platform_clipboard_commands("@task", &[])
            .expect_err("no commands should be unavailable");

        assert!(error.contains("clipboard transport unavailable"));
        assert!(error.contains("stdout is not a terminal"));
    }

    #[test]
    fn todo_prompt_dates_accept_strict_and_relative_values() {
        let today = zorg_query::QueryDate::new(2026, 5, 4).expect("date");

        assert_eq!(
            parse_todo_prompt_date_from("2026-05-09", today)
                .expect("absolute date")
                .iso,
            "2026-05-09"
        );
        assert_eq!(
            parse_todo_prompt_date_from("+1d", today)
                .expect("relative day")
                .iso,
            "2026-05-05"
        );
        assert_eq!(
            parse_todo_prompt_date_from("+1w", today)
                .expect("relative week")
                .iso,
            "2026-05-11"
        );
        assert!(parse_todo_prompt_date_from("2026-02-30", today).is_err());
        assert!(parse_todo_prompt_date_from("+0d", today).is_err());
        assert!(parse_todo_prompt_date_from("+1m", today).is_err());
    }

    #[test]
    fn capture_delegates_to_zorg_capture_and_refreshes() {
        let temp = temp_path("capture");
        let root = temp.join("corpus");
        let db = temp.join("zorg.sqlite3");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(
            root.join("templates.z"),
            "\
%%% @system #z/ref
System
%%%

- @system/templates/todo #z/tmpl title::Todo capture dest::inbox.z
  ```zorg-template
  - @{{id}} #z/todo [ ] source::{{source}} {{title}}
    {{body}}
  ```
",
        )
        .expect("write template");
        let options = StoreOptions::new(&root, &db).expect("store options");
        let mut store = Store::open_with_options(options.clone()).expect("open store");
        store.reindex().expect("initial reindex");
        let templates = capture_templates(&root).expect("capture templates");
        assert_eq!(templates.len(), 1);
        assert_eq!(
            templates[0].selector.as_deref(),
            Some("@system/templates/todo")
        );
        assert_eq!(templates[0].destination.as_deref(), Some("inbox.z"));

        let outcome = capture(
            options,
            None,
            None,
            CaptureDraft {
                template: templates[0]
                    .selector
                    .clone()
                    .expect("template selector should be present"),
                title: "Dashboard capture".to_owned(),
                body: "Created from the dashboard.".to_owned(),
                destination: String::new(),
                active: crate::model::CaptureField::Template,
            },
        )
        .expect("capture should succeed");

        assert_eq!(outcome.result.zettel_id.declaration(), "@dashboard-capture");
        assert!(outcome.result.destination.ends_with("inbox.z"));
        assert!(matches!(outcome.snapshot, DashboardSnapshot::Ready { .. }));

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn capture_templates_reports_empty_corpus_without_defaults() {
        let temp = temp_path("capture-empty");
        let root = temp.join("corpus");
        std::fs::create_dir_all(&root).expect("create root");

        let templates = capture_templates(&root).expect("capture templates");

        assert!(templates.is_empty());
        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn capture_templates_lists_metadata_for_picker() {
        let temp = temp_path("capture-picker");
        let root = temp.join("corpus");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(
            root.join("templates.z"),
            "\
%%% @system #z/ref
System
%%%

- @system/templates/project #z/tmpl title::Project note dest::projects
  ```zorg-template
  - @{{id}} #z/ref source::{{source}} {{title}}
    {{body}}
  ```

- @system/templates/todo #z/tmpl title::Todo capture dest::inbox.z
  ```zorg-template
  - @{{id}} #z/todo [ ] {{title}}
  ```
",
        )
        .expect("write templates");

        let templates = capture_templates(&root).expect("capture templates");

        assert_eq!(templates.len(), 2);
        assert_eq!(
            templates[0].selector.as_deref(),
            Some("@system/templates/project")
        );
        assert_eq!(templates[0].title.as_deref(), Some("Project"));
        assert_eq!(templates[0].destination.as_deref(), Some("projects"));
        assert_eq!(
            templates[0].variables,
            vec![
                "body".to_owned(),
                "id".to_owned(),
                "source".to_owned(),
                "title".to_owned()
            ]
        );
        assert!(
            templates[0]
                .path
                .as_ref()
                .is_some_and(|path| path.ends_with("templates.z"))
        );
        assert_eq!(
            templates[1].selector.as_deref(),
            Some("@system/templates/todo")
        );

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn capture_templates_keeps_unselectable_template_metadata() {
        let temp = temp_path("capture-unselectable");
        let root = temp.join("corpus");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(
            root.join("templates.z"),
            "\
%%% @system #z/ref
System
%%%

- #z/tmpl dest::misc.z
  ```zorg-template
  - @{{id}} #z/ref {{title}}
  ```
",
        )
        .expect("write template");

        let templates = capture_templates(&root).expect("capture templates");

        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0].selector, None);
        assert_eq!(templates[0].destination.as_deref(), Some("misc.z"));
        assert_eq!(
            templates[0].variables,
            vec!["id".to_owned(), "title".to_owned()]
        );
        assert!(templates[0].draft().is_err());

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn fix_preview_uses_indexed_diagnostic_and_shared_planner() {
        let temp = temp_path("fix-preview");
        let root = temp.join("corpus");
        let db = temp.join("zorg.sqlite3");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(
            root.join("target.z"),
            "\
%%% @project/plan #z/ref
Plan
%%%
",
        )
        .expect("write target");
        std::fs::write(
            root.join("links.z"),
            "\
%%% @links #z/ref
Links
%%%

See #poject/plan.
",
        )
        .expect("write links");
        let options = StoreOptions::new(&root, &db).expect("store options");
        let mut store = Store::open_with_options(options.clone()).expect("open store");
        store.reindex().expect("reindex");
        drop(store);

        let diagnostics = match data::load_snapshot(options.clone(), None, None) {
            DashboardSnapshot::Ready { diagnostics, .. } => diagnostics,
            DashboardSnapshot::Degraded { message } => panic!("snapshot degraded: {message}"),
            DashboardSnapshot::Loading => panic!("snapshot unexpectedly loading"),
        };
        let diagnostic = diagnostics
            .into_iter()
            .find(|row| row.code.as_deref() == Some("reference.unresolved_absolute"))
            .expect("unresolved diagnostic");

        let overlay = fix_preview(options, diagnostic).expect("fix preview");

        assert_eq!(overlay.previews.len(), 1);
        assert_eq!(
            overlay.previews[0].rule_code,
            "fix.unresolved_absolute_link_typo"
        );
        assert_eq!(overlay.previews[0].replacement_preview, "#project/plan");
        assert!(overlay.previews[0].is_safe);
        assert_eq!(overlay.unavailable_reason, None);

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn fix_apply_writes_selected_fix_and_refreshes_snapshot() {
        let temp = temp_path("fix-apply");
        let root = temp.join("corpus");
        let db = temp.join("zorg.sqlite3");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(
            root.join("target.z"),
            "\
%%% @project/plan #z/ref
Plan
%%%
",
        )
        .expect("write target");
        std::fs::write(
            root.join("links.z"),
            "\
%%% @links #z/ref
Links
%%%

See #poject/plan.
",
        )
        .expect("write links");
        let options = StoreOptions::new(&root, &db).expect("store options");
        let mut store = Store::open_with_options(options.clone()).expect("open store");
        store.reindex().expect("reindex");
        drop(store);
        let diagnostic = unresolved_absolute_diagnostic(options.clone());
        let overlay = fix_preview(options.clone(), diagnostic).expect("fix preview");

        let outcome = fix_apply(options, None, None, overlay.selector).expect("fix apply");

        let rewritten = std::fs::read_to_string(root.join("links.z")).expect("read rewritten");
        assert!(rewritten.contains("#project/plan"));
        assert_eq!(
            outcome.applied_rule_codes,
            vec!["fix.unresolved_absolute_link_typo"]
        );
        assert_eq!(outcome.applied_edits, 1);
        assert!(matches!(
            outcome.snapshot,
            DashboardSnapshot::Ready { diagnostics, .. } if diagnostics.is_empty()
        ));

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn fix_apply_refuses_stale_index_without_writing() {
        let temp = temp_path("fix-apply-stale");
        let root = temp.join("corpus");
        let db = temp.join("zorg.sqlite3");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(
            root.join("target.z"),
            "%%% @project/plan #z/ref\nPlan\n%%%\n",
        )
        .expect("write target");
        let original = "%%% @links #z/ref\nLinks\n%%%\n\nSee #poject/plan.\n";
        std::fs::write(root.join("links.z"), original).expect("write links");
        let options = StoreOptions::new(&root, &db).expect("store options");
        let mut store = Store::open_with_options(options.clone()).expect("open store");
        store.reindex().expect("reindex");
        drop(store);
        let diagnostic = unresolved_absolute_diagnostic(options.clone());
        let overlay = fix_preview(options.clone(), diagnostic).expect("fix preview");
        std::fs::write(root.join("links.z"), format!("{original}\nexternal edit\n"))
            .expect("make stale");

        let error =
            fix_apply(options, None, None, overlay.selector).expect_err("stale source refuses");

        assert!(error.contains("index is stale relative to source"));
        let current = std::fs::read_to_string(root.join("links.z")).expect("read current");
        assert!(current.contains("#poject/plan"));
        assert!(!current.contains("#project/plan"));

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn fix_apply_failure_leaves_source_unchanged() {
        let temp = temp_path("fix-apply-failure");
        let root = temp.join("corpus");
        let db = temp.join("zorg.sqlite3");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(
            root.join("target.z"),
            "%%% @project/plan #z/ref\nPlan\n%%%\n",
        )
        .expect("write target");
        let original = "%%% @links #z/ref\nLinks\n%%%\n\nSee #poject/plan.\n";
        std::fs::write(root.join("links.z"), original).expect("write links");
        let options = StoreOptions::new(&root, &db).expect("store options");
        let mut store = Store::open_with_options(options.clone()).expect("open store");
        store.reindex().expect("reindex");
        drop(store);
        let diagnostic = unresolved_absolute_diagnostic(options.clone());
        let mut overlay = fix_preview(options.clone(), diagnostic).expect("fix preview");
        overlay.selector.diagnostic_code = Some("reference.unresolved_child".to_owned());

        let error =
            fix_apply(options, None, None, overlay.selector).expect_err("invalid selector fails");

        assert!(error.contains("no matching safe fix op"));
        let current = std::fs::read_to_string(root.join("links.z")).expect("read current");
        assert_eq!(current, original);

        let _ = std::fs::remove_dir_all(temp);
    }

    fn temp_path(label: &str) -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "zorg-dash-actions-test-{}-{label}-{counter}",
            std::process::id()
        ))
    }

    fn unresolved_absolute_diagnostic(options: StoreOptions) -> DiagnosticRow {
        match data::load_snapshot(options, None, None) {
            DashboardSnapshot::Ready { diagnostics, .. } => diagnostics
                .into_iter()
                .find(|row| row.code.as_deref() == Some("reference.unresolved_absolute"))
                .expect("unresolved diagnostic"),
            DashboardSnapshot::Degraded { message } => panic!("snapshot degraded: {message}"),
            DashboardSnapshot::Loading => panic!("snapshot unexpectedly loading"),
        }
    }
}
