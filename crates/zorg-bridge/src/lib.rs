//! Import and export bridge behavior for explicit conversion workflows.
//!
//! Legacy parsing belongs here, not in `zorg-parse`. This crate plans bridge
//! operations in memory and leaves all writing to later, explicit phases.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zorg_core::{LocalId, Severity, ZettelId};

const SCHEMA_VERSION: u32 = 1;

/// Options for deterministic legacy import planning.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct LegacyImportOptions {
    /// Existing corpus root used for overwrite checks.
    pub root: Option<PathBuf>,
    /// Destination prefix used when deriving root-relative output paths.
    pub dest: Option<PathBuf>,
}

impl LegacyImportOptions {
    /// Creates default read-only planning options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Versioned import plan envelope.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImportPlan {
    /// JSON schema version for bridge plan consumers.
    pub schema_version: u32,
    /// Stable command label.
    pub command: String,
    /// Planning mode. This phase only supports `plan`.
    pub mode: String,
    /// Inputs considered by the planner.
    pub inputs: Vec<ImportInput>,
    /// Writeable outputs after fatal diagnostics are excluded.
    pub outputs: Vec<ImportOutput>,
    /// Stable diagnostics emitted while planning.
    pub diagnostics: Vec<BridgeDiagnostic>,
    /// Collision records grouped by canonical ID and output path.
    pub collisions: Vec<CollisionRecord>,
    /// Counts derived from the plan.
    pub summary: ImportSummary,
}

/// A legacy input path and recognized kind.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImportInput {
    /// User-visible input path.
    pub path: String,
    /// Legacy input kind.
    pub kind: ImportInputKind,
}

/// Supported or explicitly rejected legacy input kinds.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportInputKind {
    /// Legacy note file (`.zo`).
    LegacyNote,
    /// Legacy saved query file (`.zoq`).
    LegacyQuery,
    /// Legacy template file (`.zot`).
    LegacyTemplate,
    /// Generated cache file (`.zoc`), always unsupported.
    LegacyCache,
    /// Unknown explicit input.
    Unknown,
}

/// A planned canonical output.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImportOutput {
    /// Input that produced this planned output.
    pub input_path: String,
    /// Destination path relative to the chosen root/destination.
    pub root_relative_path: String,
    /// Canonical Zorg ID without the leading `@`.
    pub canonical_id: String,
    /// Output status. This phase only emits `planned` writeable outputs.
    pub status: String,
    /// Lossy conversions applied to this output.
    pub lossiness: Vec<Lossiness>,
    /// Planned canonical `.z` source.
    #[serde(skip)]
    pub generated_content: String,
}

/// Diagnostic emitted by bridge planning.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct BridgeDiagnostic {
    /// Diagnostic severity.
    pub severity: BridgeSeverity,
    /// Diagnostic kind.
    pub kind: BridgeDiagnosticKind,
    /// Stable code.
    pub code: String,
    /// Source path.
    pub path: String,
    /// One-based source line when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// Human-readable message.
    pub message: String,
}

/// Bridge diagnostic severity.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeSeverity {
    /// Deterministic conversion note.
    Info,
    /// Lossy but writeable conversion.
    Warning,
    /// Fatal issue for affected output.
    Error,
}

/// Bridge diagnostic kind.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeDiagnosticKind {
    /// Deterministic conversion with reduced legacy semantics.
    Lossy,
    /// No deterministic canonical `.z` mapping.
    Unsupported,
    /// Duplicate ID/path or overwrite risk.
    Collision,
    /// Generated `.z` did not pass parser validation.
    InvalidOutput,
    /// Unreadable input or destination error.
    Io,
}

/// Lossy conversion markers.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lossiness {
    /// `tick::` history was collapsed into one `modified::` value.
    TickHistoryCollapsed,
    /// Legacy link text was preserved instead of rewritten.
    LinkPreserved,
}

/// Duplicate/collision group.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CollisionRecord {
    /// Colliding canonical ID.
    pub canonical_id: String,
    /// Colliding root-relative output path.
    pub root_relative_path: String,
    /// Inputs participating in the collision.
    pub input_paths: Vec<String>,
}

/// Summary counts for an import plan.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImportSummary {
    /// Number of writeable planned outputs.
    pub planned: usize,
    /// Number of lossy diagnostics.
    pub lossy: usize,
    /// Number of unsupported diagnostics.
    pub unsupported: usize,
    /// Number of fatal diagnostics.
    pub fatal: usize,
}

/// Plans a read-only legacy import over explicit files or directories.
#[must_use]
pub fn plan_legacy_import(paths: &[PathBuf], options: &LegacyImportOptions) -> ImportPlan {
    let discovered = discover_inputs(paths);
    let mut inputs = Vec::new();
    let mut diagnostics = Vec::new();
    let mut candidates = Vec::new();

    for input_path in discovered {
        let display_path = input_path.display_path;
        let kind = input_kind(&input_path.fs_path);
        inputs.push(ImportInput {
            path: display_path.clone(),
            kind,
        });

        match kind {
            ImportInputKind::LegacyCache => diagnostics.push(BridgeDiagnostic::error(
                BridgeDiagnosticKind::Unsupported,
                "legacy.generated_cache",
                display_path,
                Some(1),
                "generated .zoc cache files are not import sources",
            )),
            ImportInputKind::LegacyNote
            | ImportInputKind::LegacyQuery
            | ImportInputKind::LegacyTemplate => match fs::read_to_string(&input_path.fs_path) {
                Ok(source) => {
                    if let Some(candidate) =
                        plan_legacy_source(&display_path, kind, &source, options, &mut diagnostics)
                    {
                        candidates.push(candidate);
                    }
                }
                Err(error) => {
                    diagnostics.push(BridgeDiagnostic::error(
                        BridgeDiagnosticKind::Io,
                        "legacy.read_failed",
                        display_path,
                        None,
                        format!("failed to read legacy input: {error}"),
                    ));
                }
            },
            ImportInputKind::Unknown => {
                if input_path.fs_path.is_dir() {
                    diagnostics.push(BridgeDiagnostic::error(
                        BridgeDiagnosticKind::Io,
                        "legacy.read_dir_failed",
                        display_path,
                        None,
                        "failed to read legacy input directory",
                    ));
                } else {
                    diagnostics.push(BridgeDiagnostic::error(
                        BridgeDiagnosticKind::Unsupported,
                        "legacy.unsupported_extension",
                        display_path,
                        None,
                        "unsupported legacy import extension",
                    ));
                }
            }
        }
    }

    let collisions = append_collision_diagnostics(&candidates, &mut diagnostics);
    let collided_paths = collided_input_paths(&collisions);
    let outputs = candidates
        .into_iter()
        .filter(|candidate| !collided_paths.contains(&candidate.output.input_path))
        .filter(|candidate| !has_fatal_for_path(&diagnostics, &candidate.output.input_path))
        .map(|candidate| candidate.output)
        .collect::<Vec<_>>();

    let summary = summarize(&outputs, &diagnostics);

    ImportPlan {
        schema_version: SCHEMA_VERSION,
        command: "import legacy plan".to_owned(),
        mode: "plan".to_owned(),
        inputs,
        outputs,
        diagnostics,
        collisions,
        summary,
    }
}

#[derive(Debug, Clone)]
struct DiscoveredInput {
    fs_path: PathBuf,
    display_path: String,
}

#[derive(Debug, Clone)]
struct OutputCandidate {
    output: ImportOutput,
    id_line: usize,
}

#[derive(Debug, Default)]
struct LegacyFields {
    id: Option<(String, usize)>,
    title: Option<String>,
    tags: Vec<String>,
    properties: Vec<(String, String)>,
    tick: Option<(String, usize)>,
    body_start: usize,
    template_body: Option<String>,
}

fn discover_inputs(paths: &[PathBuf]) -> Vec<DiscoveredInput> {
    let mut discovered = Vec::new();
    for path in paths {
        collect_path(path, &display_path(path), &mut discovered);
    }
    discovered
}

fn collect_path(path: &Path, display: &str, discovered: &mut Vec<DiscoveredInput>) {
    let fs_path = resolve_input_path(path);
    if fs_path.is_dir() {
        let Ok(entries) = fs::read_dir(&fs_path) else {
            discovered.push(DiscoveredInput {
                fs_path,
                display_path: display.to_owned(),
            });
            return;
        };
        let mut children = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        children.sort_by_key(|child| display_path(child));
        for child in children {
            let child_display = display_for_resolved_child(&child);
            collect_path(&child, &child_display, discovered);
        }
    } else {
        discovered.push(DiscoveredInput {
            fs_path,
            display_path: display.to_owned(),
        });
    }
}

fn resolve_input_path(path: &Path) -> PathBuf {
    if path.exists() || path.is_absolute() {
        return path.to_path_buf();
    }
    let workspace_path = workspace_root().join(path);
    if workspace_path.exists() {
        workspace_path
    } else {
        path.to_path_buf()
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn display_for_resolved_child(path: &Path) -> String {
    let workspace = workspace_root();
    path.strip_prefix(&workspace)
        .map_or_else(|_| display_path(path), display_path)
}

fn input_kind(path: &Path) -> ImportInputKind {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("zo") => ImportInputKind::LegacyNote,
        Some("zoq") => ImportInputKind::LegacyQuery,
        Some("zot") => ImportInputKind::LegacyTemplate,
        Some("zoc") => ImportInputKind::LegacyCache,
        _ => ImportInputKind::Unknown,
    }
}

fn plan_legacy_source(
    display_path: &str,
    kind: ImportInputKind,
    source: &str,
    options: &LegacyImportOptions,
    diagnostics: &mut Vec<BridgeDiagnostic>,
) -> Option<OutputCandidate> {
    if let Some(line) = find_custom_fence_line(source) {
        diagnostics.push(BridgeDiagnostic::error(
            BridgeDiagnosticKind::Unsupported,
            "legacy.custom_fence",
            display_path,
            Some(line),
            "custom @@@ fences are not converted unless they have a deterministic Markdown fence mapping",
        ));
        return None;
    }

    let fields = parse_legacy_fields(source);
    let Some((raw_id, id_line)) = fields.id.clone() else {
        diagnostics.push(BridgeDiagnostic::error(
            BridgeDiagnosticKind::Unsupported,
            "legacy.missing_id",
            display_path,
            None,
            "legacy input is missing an ID:: marker",
        ));
        return None;
    };
    let Ok(id) = normalize_zettel_id(&raw_id) else {
        diagnostics.push(BridgeDiagnostic::error(
            BridgeDiagnosticKind::Unsupported,
            "legacy.invalid_id",
            display_path,
            Some(id_line),
            format!("legacy ID `{raw_id}` cannot be represented as a canonical Zorg ID"),
        ));
        return None;
    };

    let mut lossiness = Vec::new();
    let content = match kind {
        ImportInputKind::LegacyNote => {
            render_note(&fields, source, diagnostics, display_path, &mut lossiness)
        }
        ImportInputKind::LegacyQuery => render_query(&fields),
        ImportInputKind::LegacyTemplate => render_template(&fields),
        ImportInputKind::LegacyCache | ImportInputKind::Unknown => {
            unreachable!("unsupported kind handled before render")
        }
    };

    if let Some((_, line)) = fields.tick {
        lossiness.push(Lossiness::TickHistoryCollapsed);
        diagnostics.push(BridgeDiagnostic::warning(
            BridgeDiagnosticKind::Lossy,
            "legacy.tick_history_collapsed",
            display_path.to_owned(),
            Some(line),
            "tick:: was converted to modified::; historical tick state is not preserved",
        ));
    }

    let root_relative_path = root_relative_path(id.as_str(), options);
    if let Some(root) = &options.root {
        let target = root.join(&root_relative_path);
        if target.exists() {
            diagnostics.push(BridgeDiagnostic::error(
                BridgeDiagnosticKind::Collision,
                "legacy.output_exists",
                display_path.to_owned(),
                Some(id_line),
                format!(
                    "planned output path {} already exists",
                    path_to_string(&root_relative_path)
                ),
            ));
        }
    }

    append_invalid_output_diagnostics(&content, &root_relative_path, display_path, diagnostics);

    Some(OutputCandidate {
        output: ImportOutput {
            input_path: display_path.to_owned(),
            root_relative_path: path_to_string(&root_relative_path),
            canonical_id: id.as_str().to_owned(),
            status: "planned".to_owned(),
            lossiness,
            generated_content: content,
        },
        id_line,
    })
}

fn normalize_zettel_id(raw_id: &str) -> Result<ZettelId, zorg_core::ZorgError> {
    let canonical = raw_id.trim().trim_start_matches('@');
    ZettelId::parse_canonical(canonical)
}

fn normalize_local_id(raw_id: &str) -> Result<LocalId, zorg_core::ZorgError> {
    let canonical = raw_id.trim().trim_start_matches('^');
    LocalId::parse(&format!("^{canonical}"))
}

fn parse_legacy_fields(source: &str) -> LegacyFields {
    let mut fields = LegacyFields::default();
    let mut body_start = 0;
    let lines = source.lines().collect::<Vec<_>>();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        body_start = index;
        if line.trim().is_empty() {
            body_start = index + 1;
            break;
        }

        let Some((key, value)) = split_marker(line) else {
            break;
        };
        match key {
            "ID" => fields.id = Some((value.to_owned(), index + 1)),
            "title" => fields.title = Some(value.to_owned()),
            "tags" => {
                fields.tags.extend(
                    value
                        .split_whitespace()
                        .map(|tag| tag.trim_start_matches('#').to_owned()),
                );
            }
            "tick" => fields.tick = Some((value.to_owned(), index + 1)),
            "template" => {
                let body = lines
                    .iter()
                    .skip(index + 1)
                    .copied()
                    .collect::<Vec<_>>()
                    .join("\n");
                fields.template_body = Some(body);
                body_start = lines.len();
                break;
            }
            _ => fields.properties.push((key.to_owned(), value.to_owned())),
        }
        index += 1;
    }

    fields.body_start = body_start;
    fields
}

fn split_marker(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once("::")?;
    Some((key.trim(), value.trim()))
}

fn render_note(
    fields: &LegacyFields,
    source: &str,
    diagnostics: &mut Vec<BridgeDiagnostic>,
    display_path: &str,
    lossiness: &mut Vec<Lossiness>,
) -> String {
    let id = normalize_zettel_id(fields.id.as_ref().map(|(id, _)| id.as_str()).unwrap_or(""))
        .expect("ID already validated");
    let title = fields.title.as_deref().unwrap_or(id.as_str());
    let mut header_parts = vec![id.declaration()];
    header_parts.extend(fields.tags.iter().map(|tag| format!("#{tag}")));
    for (key, value) in &fields.properties {
        if key != "title" {
            header_parts.push(format!("{key}::{value}"));
        }
    }
    if let Some((tick, _)) = &fields.tick {
        header_parts.push(format!("modified::{tick}"));
    }

    let body = render_note_body(
        source,
        fields.body_start,
        diagnostics,
        display_path,
        lossiness,
    );
    render_zettel(&header_parts, title, body.as_deref())
}

fn render_query(fields: &LegacyFields) -> String {
    let id = normalize_zettel_id(fields.id.as_ref().map(|(id, _)| id.as_str()).unwrap_or(""))
        .expect("ID already validated");
    let title = fields.title.as_deref().unwrap_or(id.as_str());
    let mut header_parts = vec![id.declaration(), "#z/query".to_owned()];
    if let Some(title) = &fields.title {
        header_parts.push(format!("title::{title}"));
    }
    for (key, value) in &fields.properties {
        if key != "title" {
            header_parts.push(format!("{key}::{value}"));
        }
    }
    render_zettel(&header_parts, title, None)
}

fn render_template(fields: &LegacyFields) -> String {
    let id = normalize_zettel_id(fields.id.as_ref().map(|(id, _)| id.as_str()).unwrap_or(""))
        .expect("ID already validated");
    let title = fields.title.as_deref().unwrap_or(id.as_str());
    let mut header_parts = vec![id.declaration(), "#z/tmpl".to_owned()];
    if let Some(title) = &fields.title {
        header_parts.push(format!("title::{title}"));
    }
    for (key, value) in &fields.properties {
        if key != "title" {
            header_parts.push(format!("{key}::{value}"));
        }
    }

    let template = fields.template_body.as_deref().unwrap_or("").trim_end();
    let body = format!("```zorg-template\n{template}\n```");
    render_zettel(&header_parts, title, Some(&body))
}

fn render_zettel(header_parts: &[String], title: &str, body: Option<&str>) -> String {
    let mut output = String::new();
    output.push_str("%%% ");
    output.push_str(&header_parts.join(" "));
    output.push('\n');
    output.push_str(title);
    output.push_str("\n%%%\n");
    if let Some(body) = body.filter(|body| !body.is_empty()) {
        output.push('\n');
        output.push_str(body.trim_end());
        output.push('\n');
    }
    output
}

fn render_note_body(
    source: &str,
    body_start: usize,
    diagnostics: &mut Vec<BridgeDiagnostic>,
    display_path: &str,
    lossiness: &mut Vec<Lossiness>,
) -> Option<String> {
    let lines = source.lines().collect::<Vec<_>>();
    if body_start >= lines.len() {
        return None;
    }

    let mut output = Vec::new();
    let mut index = body_start;
    while index < lines.len() {
        let line = lines[index];
        if let Some((_, local_id)) = split_marker(line).filter(|(key, _)| *key == "LID") {
            let (nested, next) = render_nested_zettel(
                &lines,
                index,
                local_id,
                diagnostics,
                display_path,
                lossiness,
            );
            output.push(nested);
            index = next;
            continue;
        }
        output.push(convert_links(
            line,
            diagnostics,
            display_path,
            index + 1,
            lossiness,
        ));
        index += 1;
    }

    let rendered = output.join("\n").trim_end().to_owned();
    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
}

fn render_nested_zettel(
    lines: &[&str],
    start: usize,
    local_id: &str,
    diagnostics: &mut Vec<BridgeDiagnostic>,
    display_path: &str,
    lossiness: &mut Vec<Lossiness>,
) -> (String, usize) {
    let local = normalize_local_id(local_id).unwrap_or_else(|_| LocalId::unchecked(local_id));
    let mut index = start + 1;
    let mut todo = None;
    let mut properties = Vec::new();

    while index < lines.len() {
        let Some((key, value)) = split_marker(lines[index]) else {
            break;
        };
        match key {
            "todo" => todo = Some(todo_marker(value)),
            _ => properties.push(format!("{key}::{value}")),
        }
        index += 1;
    }

    let mut body = Vec::new();
    while index < lines.len() {
        if split_marker(lines[index]).is_some() {
            break;
        }
        if lines[index].trim().is_empty() {
            index += 1;
            break;
        }
        body.push(convert_links(
            lines[index],
            diagnostics,
            display_path,
            index + 1,
            lossiness,
        ));
        index += 1;
    }

    let mut parts = vec![format!("- {}", local.declaration())];
    if todo.is_some() {
        parts.push("#z/todo".to_owned());
    }
    if let Some(todo) = todo {
        parts.push(todo.to_owned());
    }
    parts.extend(properties);
    if !body.is_empty() {
        parts.push(body.join(" "));
    }
    (parts.join(" "), index)
}

fn todo_marker(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "open" | "todo" | " " => "[ ]",
        "next" | "n" => "[N]",
        "done" | "x" => "[X]",
        _ => "[?]",
    }
}

fn convert_links(
    line: &str,
    diagnostics: &mut Vec<BridgeDiagnostic>,
    display_path: &str,
    line_number: usize,
    lossiness: &mut Vec<Lossiness>,
) -> String {
    let mut rendered = String::new();
    let mut rest = line;

    while let Some(start) = rest.find("[[") {
        let before = &rest[..start];
        rendered.push_str(before);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("]]") else {
            rendered.push_str(&rest[start..]);
            diagnostics.push(BridgeDiagnostic::warning(
                BridgeDiagnosticKind::Lossy,
                "legacy.link_preserved",
                display_path.to_owned(),
                Some(line_number),
                "malformed legacy link was preserved as body text",
            ));
            lossiness.push(Lossiness::LinkPreserved);
            return rendered;
        };
        let target = after_start[..end].trim();
        if ZettelId::parse_canonical(target).is_ok() {
            rendered.push('#');
            rendered.push_str(target);
        } else {
            rendered.push_str("[[");
            rendered.push_str(&after_start[..end]);
            rendered.push_str("]]");
            diagnostics.push(BridgeDiagnostic::warning(
                BridgeDiagnosticKind::Lossy,
                "legacy.link_preserved",
                display_path.to_owned(),
                Some(line_number),
                "legacy link target could not be normalized and was preserved",
            ));
            lossiness.push(Lossiness::LinkPreserved);
        }
        rest = &after_start[end + 2..];
    }

    rendered.push_str(rest);
    rendered
}

fn root_relative_path(id: &str, options: &LegacyImportOptions) -> PathBuf {
    let id_path = PathBuf::from(format!("{id}.z"));
    match &options.dest {
        Some(dest) => dest.join(id_path),
        None => id_path,
    }
}

fn append_invalid_output_diagnostics(
    content: &str,
    root_relative_path: &Path,
    display_path: &str,
    diagnostics: &mut Vec<BridgeDiagnostic>,
) {
    let document = match zorg_parse::parse_zettel_document_with_path(content, root_relative_path) {
        Ok(document) => document,
        Err(error) => {
            diagnostics.push(BridgeDiagnostic::error(
                BridgeDiagnosticKind::InvalidOutput,
                "legacy.invalid_output",
                display_path.to_owned(),
                None,
                format!("generated .z failed to parse: {error}"),
            ));
            return;
        }
    };
    let report = zorg_parse::validate_document(&document);
    for diagnostic in report
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
    {
        diagnostics.push(BridgeDiagnostic::error(
            BridgeDiagnosticKind::InvalidOutput,
            diagnostic
                .code
                .as_deref()
                .unwrap_or("legacy.invalid_output"),
            display_path.to_owned(),
            diagnostic.span.and_then(|span| span.start_line),
            format!("generated .z failed validation: {}", diagnostic.message),
        ));
    }
}

fn append_collision_diagnostics(
    candidates: &[OutputCandidate],
    diagnostics: &mut Vec<BridgeDiagnostic>,
) -> Vec<CollisionRecord> {
    let mut by_id: BTreeMap<&str, Vec<&OutputCandidate>> = BTreeMap::new();
    let mut by_path: BTreeMap<&str, Vec<&OutputCandidate>> = BTreeMap::new();
    for candidate in candidates {
        by_id
            .entry(&candidate.output.canonical_id)
            .or_default()
            .push(candidate);
        by_path
            .entry(&candidate.output.root_relative_path)
            .or_default()
            .push(candidate);
    }

    let mut collisions = Vec::new();
    for group in by_id.values().filter(|group| group.len() > 1) {
        let first = group[0];
        for duplicate in group.iter().skip(1) {
            diagnostics.push(BridgeDiagnostic::error(
                BridgeDiagnosticKind::Collision,
                "legacy.duplicate_id",
                duplicate.output.input_path.clone(),
                Some(duplicate.id_line),
                format!(
                    "duplicate canonical ID @{} also claimed by {}",
                    duplicate.output.canonical_id, first.output.input_path
                ),
            ));
        }
        collisions.push(CollisionRecord {
            canonical_id: first.output.canonical_id.clone(),
            root_relative_path: first.output.root_relative_path.clone(),
            input_paths: group
                .iter()
                .map(|candidate| candidate.output.input_path.clone())
                .collect(),
        });
    }

    for group in by_path.values().filter(|group| group.len() > 1) {
        for duplicate in group.iter().skip(1) {
            diagnostics.push(BridgeDiagnostic::error(
                BridgeDiagnosticKind::Collision,
                "legacy.duplicate_output_path",
                duplicate.output.input_path.clone(),
                Some(duplicate.id_line),
                format!(
                    "duplicate planned output path {}",
                    duplicate.output.root_relative_path
                ),
            ));
        }
    }

    collisions
}

fn collided_input_paths(collisions: &[CollisionRecord]) -> BTreeSet<String> {
    collisions
        .iter()
        .flat_map(|collision| collision.input_paths.iter().cloned())
        .collect()
}

fn has_fatal_for_path(diagnostics: &[BridgeDiagnostic], path: &str) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.path == path && diagnostic.severity == BridgeSeverity::Error)
}

fn summarize(outputs: &[ImportOutput], diagnostics: &[BridgeDiagnostic]) -> ImportSummary {
    ImportSummary {
        planned: outputs.len(),
        lossy: diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.kind == BridgeDiagnosticKind::Lossy)
            .count(),
        unsupported: diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.kind == BridgeDiagnosticKind::Unsupported)
            .count(),
        fatal: diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == BridgeSeverity::Error)
            .count(),
    }
}

fn find_custom_fence_line(source: &str) -> Option<usize> {
    source
        .lines()
        .position(|line| line.trim_start().starts_with("@@@"))
        .map(|line| line + 1)
}

fn display_path(path: &Path) -> String {
    path_to_string(path)
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

impl BridgeDiagnostic {
    fn warning(
        kind: BridgeDiagnosticKind,
        code: impl Into<String>,
        path: impl Into<String>,
        line: Option<usize>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: BridgeSeverity::Warning,
            kind,
            code: code.into(),
            path: path.into(),
            line,
            message: message.into(),
        }
    }

    fn error(
        kind: BridgeDiagnosticKind,
        code: impl Into<String>,
        path: impl Into<String>,
        line: Option<usize>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: BridgeSeverity::Error,
            kind,
            code: code.into(),
            path: path.into(),
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for BridgeDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(
                formatter,
                "{}:{}: {}: {}",
                self.path, line, self.code, self.message
            ),
            None => write!(formatter, "{}: {}: {}", self.path, self.code, self.message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture_path(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    }

    fn rel(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    fn read_fixture(path: &str) -> String {
        fs::read_to_string(fixture_path(path)).expect("read fixture")
    }

    #[test]
    fn successful_legacy_inputs_match_expected_outputs_and_plan() {
        let paths = vec![
            rel("fixtures/import_export/legacy/notes/project.zo"),
            rel("fixtures/import_export/legacy/queries/open.zoq"),
            rel("fixtures/import_export/legacy/templates/todo.zot"),
        ];

        let plan = plan_legacy_import(&paths, &LegacyImportOptions::new());

        assert_eq!(plan.summary.planned, 3);
        assert_eq!(plan.summary.fatal, 0);
        assert_output(
            &plan,
            "legacy/project",
            "fixtures/import_export/expected_z/legacy_project.z",
        );
        assert_output(
            &plan,
            "legacy/query/open",
            "fixtures/import_export/expected_z/open_query.z",
        );
        assert_output(
            &plan,
            "legacy/templates/todo",
            "fixtures/import_export/expected_z/todo_template.z",
        );
        assert_json_matches_expected(
            &plan,
            "fixtures/import_export/expected_plans/legacy_success.plan.json",
        );
    }

    #[test]
    fn directory_inputs_are_sorted_deterministically() {
        let paths = vec![rel("fixtures/import_export/legacy")];

        let plan = plan_legacy_import(&paths, &LegacyImportOptions::new());

        let input_paths = plan
            .inputs
            .iter()
            .map(|input| input.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            input_paths,
            vec![
                "fixtures/import_export/legacy/collisions/alpha.zo",
                "fixtures/import_export/legacy/collisions/beta.zo",
                "fixtures/import_export/legacy/notes/project.zo",
                "fixtures/import_export/legacy/queries/open.zoq",
                "fixtures/import_export/legacy/templates/todo.zot",
                "fixtures/import_export/legacy/unsupported/custom_fence.zo",
                "fixtures/import_export/legacy/unsupported/generated.zoc",
            ]
        );
    }

    #[test]
    fn duplicate_ids_and_output_paths_are_fatal_collisions() {
        let paths = vec![
            rel("fixtures/import_export/legacy/collisions/alpha.zo"),
            rel("fixtures/import_export/legacy/collisions/beta.zo"),
        ];

        let plan = plan_legacy_import(&paths, &LegacyImportOptions::new());

        assert!(plan.outputs.is_empty());
        assert_eq!(plan.collisions.len(), 1);
        assert_json_matches_expected(
            &plan,
            "fixtures/import_export/expected_plans/collision.plan.json",
        );
    }

    #[test]
    fn unsupported_cache_and_custom_fence_are_reported() {
        let paths = vec![
            rel("fixtures/import_export/legacy/unsupported/generated.zoc"),
            rel("fixtures/import_export/legacy/unsupported/custom_fence.zo"),
        ];

        let plan = plan_legacy_import(&paths, &LegacyImportOptions::new());

        assert!(plan.outputs.is_empty());
        assert_json_matches_expected(
            &plan,
            "fixtures/import_export/expected_plans/unsupported.plan.json",
        );
    }

    #[test]
    fn root_existing_output_is_reported_as_collision() {
        let temp =
            std::env::temp_dir().join(format!("zorg-bridge-test-{}-existing", std::process::id()));
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(temp.join("legacy")).expect("create temp output dir");
        fs::write(temp.join("legacy/project.z"), "existing").expect("write existing output");

        let paths = vec![rel("fixtures/import_export/legacy/notes/project.zo")];
        let options = LegacyImportOptions {
            root: Some(temp.clone()),
            dest: None,
        };
        let plan = plan_legacy_import(&paths, &options);
        let _ = fs::remove_dir_all(&temp);

        assert!(plan.outputs.is_empty());
        assert!(plan.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == BridgeDiagnosticKind::Collision
                && diagnostic.code == "legacy.output_exists"
        }));
    }

    #[test]
    fn generated_outputs_parse_and_validate() {
        let paths = vec![
            rel("fixtures/import_export/legacy/notes/project.zo"),
            rel("fixtures/import_export/legacy/queries/open.zoq"),
            rel("fixtures/import_export/legacy/templates/todo.zot"),
        ];
        let plan = plan_legacy_import(&paths, &LegacyImportOptions::new());

        for output in &plan.outputs {
            let document = zorg_parse::parse_zettel_document_with_path(
                &output.generated_content,
                &output.root_relative_path,
            )
            .expect("generated output parses");
            let report = zorg_parse::validate_document(&document);
            assert!(
                !report.has_errors(),
                "expected generated output to validate: {:?}",
                report.diagnostics
            );
        }
    }

    fn assert_output(plan: &ImportPlan, canonical_id: &str, expected_path: &str) {
        let output = plan
            .outputs
            .iter()
            .find(|output| output.canonical_id == canonical_id)
            .expect("output present");
        assert_eq!(output.generated_content, read_fixture(expected_path));
    }

    fn assert_json_matches_expected(plan: &ImportPlan, expected_path: &str) {
        let mut actual = serde_json::to_value(plan).expect("serialize plan");
        let mut expected: Value =
            serde_json::from_str(&read_fixture(expected_path)).expect("parse expected plan");
        strip_generated_content(&mut actual);
        strip_generated_content(&mut expected);
        assert_eq!(actual, expected);
    }

    fn strip_generated_content(value: &mut Value) {
        if let Some(outputs) = value.get_mut("outputs").and_then(Value::as_array_mut) {
            for output in outputs {
                if let Some(object) = output.as_object_mut() {
                    object.remove("generated_content");
                    object.remove("expected_output");
                }
            }
        }
    }
}
