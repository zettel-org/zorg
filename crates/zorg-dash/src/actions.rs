use std::env;
use std::path::Path;
use std::process::Command;

use zorg_store::{ReindexSummary, Store, StoreOptions};

use crate::data;
use crate::model::{DashboardSnapshot, SourceLocation};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ReindexOutcome {
    pub(crate) summary: ReindexSummary,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
}
