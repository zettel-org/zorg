use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use zorg_core::{
    BodyBlock, FencedCodeBlock, Paragraph, Reference, ReferenceTarget, SourcePath, Zettel,
    ZettelDocument, ZettelId,
};

use crate::BridgeSeverity;

const EXPORT_SCHEMA_VERSION: u32 = 1;

/// Options for deterministic canonical `.z` to Markdown rendering.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct MarkdownRenderOptions {
    /// Include YAML-like front matter for root export items.
    pub front_matter: bool,
    /// Render nested child zettels as nested Markdown headings.
    pub render_children: bool,
}

impl Default for MarkdownRenderOptions {
    fn default() -> Self {
        Self {
            front_matter: true,
            render_children: true,
        }
    }
}

impl MarkdownRenderOptions {
    /// Creates default Markdown rendering options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Versioned Markdown export plan envelope.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportPlan {
    /// JSON schema version for bridge export consumers.
    pub schema_version: u32,
    /// Stable command label.
    pub command: String,
    /// Export target selector used to build this plan.
    pub target: ExportTarget,
    /// Rendered Markdown items in deterministic order.
    pub items: Vec<ExportItem>,
    /// Stable diagnostics emitted while rendering.
    pub diagnostics: Vec<ExportDiagnostic>,
    /// Counts derived from the plan.
    pub summary: ExportSummary,
}

/// A selected export target.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExportTarget {
    /// Export explicit canonical IDs in the given order.
    Explicit {
        /// Canonical IDs without leading `@`.
        canonical_ids: Vec<String>,
    },
    /// Export one zettel by canonical ID.
    Single {
        /// Canonical ID without leading `@`.
        canonical_id: String,
    },
    /// Export the selected zettel and its descendants.
    Subtree {
        /// Canonical ID without leading `@`.
        canonical_id: String,
    },
    /// Export precomputed query result IDs in query order.
    Query {
        /// Stable query label or source text.
        label: String,
        /// Canonical IDs without leading `@`, already in query order.
        canonical_ids: Vec<String>,
    },
}

/// A rendered Markdown export item.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportItem {
    /// Canonical ID without leading `@`.
    pub canonical_id: String,
    /// Source path when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Plain title used for the top-level heading.
    pub title: String,
    /// Deterministic Markdown body.
    pub markdown: String,
}

/// Diagnostic emitted by Markdown export.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportDiagnostic {
    /// Diagnostic severity.
    pub severity: BridgeSeverity,
    /// Diagnostic kind.
    pub kind: ExportDiagnosticKind,
    /// Stable code.
    pub code: String,
    /// Source path when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Canonical source zettel ID when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_id: Option<String>,
    /// One-based source line when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    /// Human-readable message.
    pub message: String,
}

/// Markdown export diagnostic kind.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportDiagnosticKind {
    /// Requested target was not present in parsed data.
    MissingTarget,
    /// The selected zettel has no canonical export ID.
    MissingId,
    /// A Zorg link could not be rendered as a Markdown link.
    LossyLink,
}

/// Summary counts for a Markdown export plan.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExportSummary {
    /// Number of requested root items selected.
    pub planned: usize,
    /// Number of Markdown items rendered.
    pub rendered: usize,
    /// Number of lossy diagnostics.
    pub lossy: usize,
    /// Number of fatal diagnostics.
    pub fatal: usize,
}

/// Plans a deterministic Markdown export from parsed and resolved zettel data.
///
/// Callers should run `zorg_parse::resolve_document` or
/// `zorg_parse::resolve_corpus` first when they need relative links to resolve.
#[must_use]
pub fn plan_markdown_export(
    documents: &[ZettelDocument],
    target: ExportTarget,
    options: &MarkdownRenderOptions,
) -> ExportPlan {
    let zettels = collect_exportable_zettels(documents);
    let by_id = zettels
        .iter()
        .filter_map(|zettel| {
            zettel
                .canonical_id
                .as_ref()
                .map(|id| (id.as_str().to_owned(), *zettel))
        })
        .collect::<BTreeMap<_, _>>();

    let mut diagnostics = Vec::new();
    let selected = select_zettels(&target, &by_id, &mut diagnostics);
    let link_targets = collect_link_targets(&selected, options);
    let items = selected
        .iter()
        .map(|zettel| render_export_item(zettel, &link_targets, options, &mut diagnostics))
        .collect::<Vec<_>>();
    let summary = summarize_export(selected.len(), items.len(), &diagnostics);

    ExportPlan {
        schema_version: EXPORT_SCHEMA_VERSION,
        command: "export markdown".to_owned(),
        target,
        items,
        diagnostics,
        summary,
    }
}

fn collect_exportable_zettels(documents: &[ZettelDocument]) -> Vec<&Zettel> {
    let mut zettels = Vec::new();
    for document in documents {
        collect_zettel_tree(&document.root, &mut zettels);
    }
    zettels
}

fn collect_zettel_tree<'a>(zettel: &'a Zettel, zettels: &mut Vec<&'a Zettel>) {
    zettels.push(zettel);
    for child in child_zettels(zettel) {
        collect_zettel_tree(child, zettels);
    }
}

fn select_zettels<'a>(
    target: &ExportTarget,
    by_id: &BTreeMap<String, &'a Zettel>,
    diagnostics: &mut Vec<ExportDiagnostic>,
) -> Vec<&'a Zettel> {
    match target {
        ExportTarget::Explicit { canonical_ids } | ExportTarget::Query { canonical_ids, .. } => {
            canonical_ids
                .iter()
                .filter_map(|id| select_one(id, by_id, diagnostics))
                .collect()
        }
        ExportTarget::Single { canonical_id } => select_one(canonical_id, by_id, diagnostics)
            .into_iter()
            .collect(),
        ExportTarget::Subtree { canonical_id } => {
            let Some(root) = select_one(canonical_id, by_id, diagnostics) else {
                return Vec::new();
            };
            let mut selected = Vec::new();
            collect_zettel_tree(root, &mut selected);
            selected
        }
    }
}

fn select_one<'a>(
    canonical_id: &str,
    by_id: &BTreeMap<String, &'a Zettel>,
    diagnostics: &mut Vec<ExportDiagnostic>,
) -> Option<&'a Zettel> {
    let id = canonical_id.trim().trim_start_matches('@');
    match by_id.get(id).copied() {
        Some(zettel) => Some(zettel),
        None => {
            diagnostics.push(ExportDiagnostic::error(
                ExportDiagnosticKind::MissingTarget,
                "markdown.missing_target",
                None,
                Some(id.to_owned()),
                None,
                format!("export target `@{id}` was not found in parsed data"),
            ));
            None
        }
    }
}

fn collect_link_targets(zettels: &[&Zettel], options: &MarkdownRenderOptions) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    for zettel in zettels {
        collect_rendered_ids(zettel, options, &mut targets);
    }
    targets
}

fn collect_rendered_ids(
    zettel: &Zettel,
    options: &MarkdownRenderOptions,
    targets: &mut BTreeSet<String>,
) {
    if let Some(id) = &zettel.canonical_id {
        targets.insert(id.as_str().to_owned());
    }
    if options.render_children {
        for child in child_zettels(zettel) {
            collect_rendered_ids(child, options, targets);
        }
    }
}

fn render_export_item(
    zettel: &Zettel,
    link_targets: &BTreeSet<String>,
    options: &MarkdownRenderOptions,
    diagnostics: &mut Vec<ExportDiagnostic>,
) -> ExportItem {
    let canonical_id = zettel
        .canonical_id
        .as_ref()
        .map(|id| id.as_str().to_owned())
        .unwrap_or_else(|| {
            diagnostics.push(ExportDiagnostic::error(
                ExportDiagnosticKind::MissingId,
                "markdown.missing_id",
                source_path_string(zettel.path.as_ref()),
                None,
                zettel.span.and_then(|span| span.start_line),
                "selected zettel has no canonical ID",
            ));
            zettel.key.as_str().to_owned()
        });
    let title = zettel.plain_title().unwrap_or_else(|| canonical_id.clone());
    let markdown = render_zettel_markdown(zettel, 1, true, link_targets, options, diagnostics);

    ExportItem {
        canonical_id,
        source_path: source_path_string(zettel.path.as_ref()),
        title,
        markdown,
    }
}

fn render_zettel_markdown(
    zettel: &Zettel,
    heading_level: usize,
    is_root_item: bool,
    link_targets: &BTreeSet<String>,
    options: &MarkdownRenderOptions,
    diagnostics: &mut Vec<ExportDiagnostic>,
) -> String {
    let mut output = String::new();
    if is_root_item && options.front_matter {
        append_front_matter(&mut output, zettel);
    }

    output.push_str(&heading_marker(heading_level));
    output.push(' ');
    output.push_str(&heading_text(zettel, is_root_item));
    output.push_str("\n\n");

    if !is_root_item {
        append_nested_metadata(&mut output, zettel);
    }

    if !is_root_item && heading_uses_identity(zettel) {
        if let Some(title) = zettel.plain_title() {
            output.push_str(&render_inline_links(
                &title,
                &[],
                zettel,
                link_targets,
                diagnostics,
            ));
            output.push_str("\n\n");
        }
    }

    for block in &zettel.body {
        match block {
            BodyBlock::Paragraph(paragraph) => {
                let rendered = render_paragraph(paragraph, zettel, link_targets, diagnostics);
                if !rendered.trim().is_empty() {
                    output.push_str(rendered.trim_end());
                    output.push_str("\n\n");
                }
            }
            BodyBlock::FencedCode(block) => {
                output.push_str(&render_fenced_code(block));
                output.push_str("\n\n");
            }
            BodyBlock::ChildZettel(child) if options.render_children => {
                output.push_str(&render_zettel_markdown(
                    child,
                    heading_level + 1,
                    false,
                    link_targets,
                    options,
                    diagnostics,
                ));
            }
            BodyBlock::ChildZettel(_) => {}
        }
    }

    trim_markdown(output)
}

fn append_front_matter(output: &mut String, zettel: &Zettel) {
    output.push_str("---\n");
    if let Some(id) = &zettel.canonical_id {
        output.push_str("id: ");
        output.push_str(&yaml_scalar(id.as_str()));
        output.push('\n');
    }

    let tags = zettel_tags(zettel);
    if !tags.is_empty() {
        output.push_str("tags:\n");
        for tag in tags {
            output.push_str("  - ");
            output.push_str(&yaml_scalar(&tag));
            output.push('\n');
        }
    }

    if !zettel.properties.is_empty() {
        output.push_str("properties:\n");
        for property in &zettel.properties {
            output.push_str("  ");
            output.push_str(&property.key);
            output.push_str(": ");
            output.push_str(&yaml_scalar(&property.value));
            output.push('\n');
        }
    }
    output.push_str("---\n\n");
}

fn append_nested_metadata(output: &mut String, zettel: &Zettel) {
    let tags = nested_zettel_tags(zettel);
    if !tags.is_empty() {
        output.push_str("Tags:\n\n");
        for tag in tags {
            output.push_str("- #");
            output.push_str(&tag);
            output.push('\n');
        }
        output.push('\n');
    }

    if !zettel.properties.is_empty() {
        output.push_str("Properties:\n\n");
        for property in &zettel.properties {
            output.push_str("- ");
            output.push_str(&property.key);
            output.push_str(": ");
            output.push_str(&property.value);
            output.push('\n');
        }
        output.push('\n');
    }
}

fn zettel_tags(zettel: &Zettel) -> Vec<String> {
    zettel
        .type_tags
        .iter()
        .chain(zettel.tags.iter())
        .map(|tagged| tagged.tag.as_str().to_owned())
        .collect()
}

fn nested_zettel_tags(zettel: &Zettel) -> Vec<String> {
    zettel_tags(zettel)
        .into_iter()
        .filter(|tag| !(zettel.todo.is_some() && tag == "z/todo"))
        .collect()
}

fn heading_text(zettel: &Zettel, is_root_item: bool) -> String {
    let mut text = if is_root_item {
        zettel
            .plain_title()
            .or_else(|| {
                zettel
                    .canonical_id
                    .as_ref()
                    .map(|id| id.as_str().to_owned())
            })
            .unwrap_or_else(|| zettel.key.as_str().to_owned())
    } else if let Some(local_id) = &zettel.local_id {
        local_id.declaration()
    } else if let Some(id) = &zettel.id {
        id.declaration()
    } else {
        zettel
            .plain_title()
            .or_else(|| {
                zettel
                    .canonical_id
                    .as_ref()
                    .map(|id| format!("@{}", id.as_str()))
            })
            .unwrap_or_else(|| zettel.key.as_str().to_owned())
    };

    if let Some(todo) = zettel.todo {
        text.push(' ');
        text.push_str(&todo.to_string());
    }
    text
}

fn heading_uses_identity(zettel: &Zettel) -> bool {
    zettel.local_id.is_some() || zettel.id.is_some()
}

fn heading_marker(level: usize) -> String {
    "#".repeat(level.clamp(1, 6))
}

fn render_paragraph(
    paragraph: &Paragraph,
    zettel: &Zettel,
    link_targets: &BTreeSet<String>,
    diagnostics: &mut Vec<ExportDiagnostic>,
) -> String {
    render_inline_links(
        &paragraph.text,
        &paragraph.links,
        zettel,
        link_targets,
        diagnostics,
    )
}

fn render_inline_links(
    text: &str,
    links: &[Reference],
    zettel: &Zettel,
    link_targets: &BTreeSet<String>,
    diagnostics: &mut Vec<ExportDiagnostic>,
) -> String {
    if links.is_empty() {
        return text.to_owned();
    }

    let paragraph_start = links
        .iter()
        .filter_map(|link| link.span.map(|span| span.start_byte))
        .min()
        .and_then(|first_link| find_reference_base_offset(text, links, first_link));
    let Some(base_offset) = paragraph_start else {
        return render_inline_links_by_search(text, links, zettel, link_targets, diagnostics);
    };

    let mut rendered = String::new();
    let mut cursor = 0;
    let mut sorted = links.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|link| link.span.map(|span| span.start_byte).unwrap_or(usize::MAX));

    for link in sorted {
        let Some(span) = link.span else {
            continue;
        };
        if span.start_byte < base_offset || span.end_byte < span.start_byte {
            continue;
        }
        let start = span.start_byte - base_offset;
        let end = span.end_byte - base_offset;
        if start < cursor
            || end > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            continue;
        }
        rendered.push_str(&text[cursor..start]);
        rendered.push_str(&render_reference(link, zettel, link_targets, diagnostics));
        cursor = end;
    }
    rendered.push_str(&text[cursor..]);
    rendered
}

fn find_reference_base_offset(
    text: &str,
    links: &[Reference],
    first_link_start: usize,
) -> Option<usize> {
    links
        .iter()
        .find_map(|link| {
            let span = link.span?;
            let raw_start = text.find(&link.raw)?;
            (span.start_byte >= raw_start).then_some(span.start_byte - raw_start)
        })
        .or(Some(first_link_start.saturating_sub(text.len())))
}

fn render_inline_links_by_search(
    text: &str,
    links: &[Reference],
    zettel: &Zettel,
    link_targets: &BTreeSet<String>,
    diagnostics: &mut Vec<ExportDiagnostic>,
) -> String {
    let mut rendered = text.to_owned();
    for link in links {
        let replacement = render_reference(link, zettel, link_targets, diagnostics);
        rendered = rendered.replacen(&link.raw, &replacement, 1);
    }
    rendered
}

fn render_reference(
    reference: &Reference,
    zettel: &Zettel,
    link_targets: &BTreeSet<String>,
    diagnostics: &mut Vec<ExportDiagnostic>,
) -> String {
    let target_id = resolved_target(reference, zettel).or_else(|| match &reference.target {
        ReferenceTarget::Absolute(id) => Some(id.clone()),
        ReferenceTarget::Child(_)
        | ReferenceTarget::Sibling(_)
        | ReferenceTarget::LocalDeclaration(_) => None,
    });

    match target_id {
        Some(id) if link_targets.contains(id.as_str()) => {
            format!(
                "[{}](zorg:#{})",
                escape_markdown_link_text(&reference.raw),
                id.as_str()
            )
        }
        Some(id) => {
            diagnostics.push(link_diagnostic(
                reference,
                zettel,
                "markdown.link_outside_export",
                format!(
                    "link `{}` to `@{}` was preserved because the target is not part of the export set",
                    reference.raw,
                    id.as_str()
                ),
            ));
            reference.raw.clone()
        }
        None => {
            diagnostics.push(link_diagnostic(
                reference,
                zettel,
                "markdown.link_unresolved",
                format!(
                    "link `{}` was preserved because it could not be resolved",
                    reference.raw
                ),
            ));
            reference.raw.clone()
        }
    }
}

fn resolved_target(reference: &Reference, zettel: &Zettel) -> Option<ZettelId> {
    zettel
        .resolved_links
        .iter()
        .find(|resolved| {
            resolved.reference.raw == reference.raw
                && resolved.reference.span == reference.span
                && resolved.reference.target == reference.target
        })
        .map(|resolved| resolved.target_id.clone())
}

fn link_diagnostic(
    reference: &Reference,
    zettel: &Zettel,
    code: impl Into<String>,
    message: impl Into<String>,
) -> ExportDiagnostic {
    ExportDiagnostic::warning(
        ExportDiagnosticKind::LossyLink,
        code,
        source_path_string(zettel.path.as_ref()),
        zettel
            .canonical_id
            .as_ref()
            .map(|id| id.as_str().to_owned()),
        reference.span.and_then(|span| span.start_line),
        message,
    )
}

fn render_fenced_code(block: &FencedCodeBlock) -> String {
    let fence = safe_fence(&block.body);
    let mut output = String::new();
    output.push_str(&fence);
    if let Some(info) = &block.info {
        output.push_str(info);
    }
    output.push('\n');
    output.push_str(block.body.trim_end());
    output.push('\n');
    output.push_str(&fence);
    output
}

fn safe_fence(body: &str) -> String {
    let longest = body
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("```").then(|| {
                trimmed
                    .chars()
                    .take_while(|character| *character == '`')
                    .count()
            })
        })
        .max()
        .unwrap_or(2);
    "`".repeat((longest + 1).max(3))
}

fn yaml_scalar(value: &str) -> String {
    if value.is_empty()
        || value.starts_with(['-', '@', '#', '[', '{', '&', '*', '!', '|', '>', '\'', '"'])
        || value.contains([':', '\n'])
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_owned()
    }
}

fn escape_markdown_link_text(value: &str) -> String {
    value.replace('[', "\\[").replace(']', "\\]")
}

fn trim_markdown(mut output: String) -> String {
    while output.ends_with("\n\n") {
        output.pop();
    }
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn source_path_string(path: Option<&SourcePath>) -> Option<String> {
    path.map(|path| path_to_string(path.as_path()))
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn child_zettels(zettel: &Zettel) -> impl Iterator<Item = &Zettel> {
    zettel.body.iter().filter_map(|block| match block {
        BodyBlock::ChildZettel(child) => Some(child.as_ref()),
        _ => None,
    })
}

fn summarize_export(
    planned: usize,
    rendered: usize,
    diagnostics: &[ExportDiagnostic],
) -> ExportSummary {
    ExportSummary {
        planned,
        rendered,
        lossy: diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.kind == ExportDiagnosticKind::LossyLink)
            .count(),
        fatal: diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == BridgeSeverity::Error)
            .count(),
    }
}

impl ExportDiagnostic {
    fn warning(
        kind: ExportDiagnosticKind,
        code: impl Into<String>,
        path: Option<String>,
        canonical_id: Option<String>,
        line: Option<usize>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: BridgeSeverity::Warning,
            kind,
            code: code.into(),
            path,
            canonical_id,
            line,
            message: message.into(),
        }
    }

    fn error(
        kind: ExportDiagnosticKind,
        code: impl Into<String>,
        path: Option<String>,
        canonical_id: Option<String>,
        line: Option<usize>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: BridgeSeverity::Error,
            kind,
            code: code.into(),
            path,
            canonical_id,
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for ExportDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.path, self.line.as_ref()) {
            (Some(path), Some(line)) => {
                write!(formatter, "{path}:{line}: {}: {}", self.code, self.message)
            }
            (Some(path), None) => write!(formatter, "{path}: {}: {}", self.code, self.message),
            (None, _) => write!(formatter, "{}: {}", self.code, self.message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn fixture_path(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    }

    fn read_fixture(path: &str) -> String {
        fs::read_to_string(fixture_path(path)).expect("read fixture")
    }

    fn parse_source(source: &str, path: &str) -> ZettelDocument {
        zorg_parse::parse_zettel_document_with_path(source, path).expect("parse zettel")
    }

    fn parse_and_resolve(source: &str, path: &str) -> ZettelDocument {
        let mut document = parse_source(source, path);
        zorg_parse::resolve_document(&mut document);
        document
    }

    fn plan_for_sources(sources: &[(&str, &str)], target: ExportTarget) -> ExportPlan {
        let mut documents = sources
            .iter()
            .map(|(source, path)| parse_source(source, path))
            .collect::<Vec<_>>();
        zorg_parse::resolve_corpus(&mut documents);
        plan_markdown_export(&documents, target, &MarkdownRenderOptions::new())
    }

    #[test]
    fn markdown_golden_matches_import_export_fixture() {
        let paths = [
            "fixtures/import_export/expected_z/legacy_project.z",
            "fixtures/import_export/expected_z/open_query.z",
            "fixtures/import_export/expected_z/todo_template.z",
        ];
        let mut documents = paths
            .iter()
            .map(|path| {
                let source = read_fixture(path);
                zorg_parse::parse_zettel_document_with_path(&source, path).expect("parse fixture")
            })
            .collect::<Vec<_>>();
        zorg_parse::resolve_corpus(&mut documents);

        let plan = plan_markdown_export(
            &documents,
            ExportTarget::Explicit {
                canonical_ids: vec![
                    "legacy/project".to_owned(),
                    "legacy/query/open".to_owned(),
                    "legacy/templates/todo".to_owned(),
                ],
            },
            &MarkdownRenderOptions::new(),
        );

        assert_eq!(plan.summary.rendered, 3);
        assert_eq!(plan.summary.fatal, 0);
        assert_eq!(
            plan.items[0].markdown,
            read_fixture("fixtures/import_export/expected_markdown/legacy_project.md")
        );
    }

    #[test]
    fn renders_properties_tags_todos_children_and_code_fences() {
        let source = "\
%%% @root #z/ref area::work
Root Title
%%%

Body before code.

```rust
let item = 1;
```

- ^task #z/todo [X] due::2026-05-04 Task title.
";
        let document = parse_and_resolve(source, "root.z");
        let plan = plan_markdown_export(
            &[document],
            ExportTarget::Single {
                canonical_id: "root".to_owned(),
            },
            &MarkdownRenderOptions::new(),
        );

        let markdown = &plan.items[0].markdown;
        assert!(markdown.contains("tags:\n  - z/ref\n"));
        assert!(markdown.contains("properties:\n  area: work\n"));
        assert!(markdown.contains("```rust\nlet item = 1;\n```"));
        assert!(
            markdown.contains("## ^task [X]\n\nProperties:\n\n- due: 2026-05-04\n\nTask title.\n")
        );
    }

    #[test]
    fn resolved_absolute_child_sibling_and_local_links_render_when_targets_are_exported() {
        let source = read_fixture("fixtures/corpus/nested.z");
        let document = parse_and_resolve(&source, "fixtures/corpus/nested.z");
        let plan = plan_markdown_export(
            &[document],
            ExportTarget::Single {
                canonical_id: "project".to_owned(),
            },
            &MarkdownRenderOptions::new(),
        );

        let markdown = &plan.items[0].markdown;
        assert!(
            markdown.contains("[+task](zorg:#project/plan/task)"),
            "{markdown}"
        );
        assert!(markdown.contains("[~review](zorg:#project/review)"));
        assert!(markdown.contains("[~plan](zorg:#project/plan)"));
        assert!(markdown.contains("[#project/plan/task](zorg:#project/plan/task)"));
        assert_eq!(plan.summary.lossy, 0);
    }

    #[test]
    fn unresolved_and_outside_links_are_preserved_with_lossy_diagnostics() {
        let source = "\
%%% @root
Root
%%%

Links to #outside and +missing.
";
        let document = parse_and_resolve(source, "unresolved.z");
        let plan = plan_markdown_export(
            &[document],
            ExportTarget::Single {
                canonical_id: "root".to_owned(),
            },
            &MarkdownRenderOptions::new(),
        );

        assert!(
            plan.items[0]
                .markdown
                .contains("Links to #outside and +missing.")
        );
        assert!(
            plan.diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "markdown.link_outside_export" })
        );
        assert!(
            plan.diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "markdown.link_unresolved" })
        );
        assert_eq!(plan.summary.lossy, 2);
    }

    #[test]
    fn query_target_preserves_requested_order() {
        let plan = plan_for_sources(
            &[
                ("%%% @alpha\nAlpha\n%%%\n", "alpha.z"),
                ("%%% @beta\nBeta\n%%%\n", "beta.z"),
            ],
            ExportTarget::Query {
                label: "manual".to_owned(),
                canonical_ids: vec!["beta".to_owned(), "alpha".to_owned()],
            },
        );

        let ids = plan
            .items
            .iter()
            .map(|item| item.canonical_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["beta", "alpha"]);
    }

    #[test]
    fn missing_targets_are_fatal_and_deterministic() {
        let plan = plan_for_sources(
            &[("%%% @alpha\nAlpha\n%%%\n", "alpha.z")],
            ExportTarget::Explicit {
                canonical_ids: vec!["missing".to_owned()],
            },
        );

        assert!(plan.items.is_empty());
        assert_eq!(plan.summary.fatal, 1);
        assert_eq!(plan.diagnostics[0].code, "markdown.missing_target");
    }
}
