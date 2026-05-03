use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use zorg_capture::{CaptureRequest, CaptureResult, CaptureTemplate};
use zorg_store::{ReindexSummary, Store, StoreOptions};

use crate::data;
use crate::model::{CaptureDraft, DashboardSnapshot, SourceLocation};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ReindexOutcome {
    pub(crate) summary: ReindexSummary,
    pub(crate) snapshot: DashboardSnapshot,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CaptureDefaults {
    pub(crate) template: String,
    pub(crate) destination: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CaptureOutcome {
    pub(crate) result: CaptureResult,
    pub(crate) snapshot: DashboardSnapshot,
}

pub(crate) fn refresh_snapshot(options: StoreOptions, query: Option<String>) -> DashboardSnapshot {
    data::load_snapshot(options, query.as_deref())
}

pub(crate) fn reindex(
    options: StoreOptions,
    query: Option<String>,
) -> Result<ReindexOutcome, String> {
    let mut store = Store::open_with_options(options.clone()).map_err(|error| error.to_string())?;
    let summary = store.reindex().map_err(|error| error.to_string())?;
    let snapshot = data::load_snapshot(options, query.as_deref());
    Ok(ReindexOutcome { summary, snapshot })
}

pub(crate) fn capture_defaults(root: &Path) -> Result<CaptureDefaults, String> {
    let templates = zorg_capture::list_templates(root).map_err(|error| error.to_string())?;
    let template = templates
        .first()
        .ok_or_else(|| "capture failed: no #z/tmpl templates were found".to_owned())?;
    let selector = template_selector(template)
        .ok_or_else(|| "capture failed: first template has no selectable ID or title".to_owned())?;
    Ok(CaptureDefaults {
        template: selector,
        destination: default_destination(template),
    })
}

pub(crate) fn capture(
    options: StoreOptions,
    query: Option<String>,
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
    let snapshot = data::load_snapshot(options, query.as_deref());
    Ok(CaptureOutcome { result, snapshot })
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

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
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

fn default_destination(template: &CaptureTemplate) -> Option<String> {
    let _ = template;
    None
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
        let defaults = capture_defaults(&root).expect("capture defaults");
        assert_eq!(defaults.template, "@system/templates/todo");

        let outcome = capture(
            options,
            None,
            CaptureDraft {
                template: defaults.template,
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

    fn temp_path(label: &str) -> PathBuf {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "zorg-dash-actions-test-{}-{label}-{counter}",
            std::process::id()
        ))
    }
}
