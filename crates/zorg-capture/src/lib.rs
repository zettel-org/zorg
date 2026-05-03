//! Capture and template boundary for Zorg.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use zorg_core::{
    BodyBlock, Severity, SourcePath, Zettel, ZettelDocument, ZettelId, ZettelKind, ZorgError,
    ZorgResult,
};
use zorg_fix::{CorpusView, apply_plan_to_source, plan_fixes};

/// Input for a noninteractive capture request.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CaptureRequest {
    /// Corpus root used for template discovery and destination resolution.
    pub root: PathBuf,
    /// Template selector: `@id` or exact `title::` value.
    pub template: String,
    /// Captured title variable.
    pub title: Option<String>,
    /// Captured source variable.
    pub source: Option<String>,
    /// Captured body variable.
    pub body: Option<String>,
    /// Destination override.
    pub dest: Option<PathBuf>,
    /// Captured zettel ID override.
    pub id: Option<String>,
    /// Allow destinations outside the configured root.
    pub allow_outside: bool,
}

/// Result of a successful capture write.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CaptureResult {
    /// Absolute path written or appended.
    pub destination: PathBuf,
    /// Canonical ID of the created zettel.
    pub zettel_id: ZettelId,
}

/// Template metadata exposed for interactive capture clients.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CaptureTemplate {
    /// Canonical template ID, when declared.
    pub id: Option<ZettelId>,
    /// Human-readable `title::` property, when declared.
    pub title: Option<String>,
    /// Source path containing the template.
    pub path: Option<PathBuf>,
    /// Case-sensitive variables referenced by the template body.
    pub variables: Vec<String>,
}

/// Runs a noninteractive capture request.
pub fn capture(request: &CaptureRequest) -> ZorgResult<CaptureResult> {
    let root = absolute_normalized(&request.root)?;
    let documents = load_documents(&root)?;
    let candidates = collect_template_candidates(&documents);
    let template = select_template(&request.template, &candidates)?;
    let existing_ids = collect_existing_ids(&documents);
    let zettel_id = capture_id(request, template, &existing_ids)?;
    let template_text = extract_template_text(template)?;
    let dest = request
        .dest
        .clone()
        .or_else(|| property_value(template.zettel, "dest").map(PathBuf::from))
        .ok_or_else(|| {
            operation_failed(
                "capture destination is missing; pass --dest or set dest:: on the template",
            )
        })?;
    let destination = resolve_destination(&root, &dest, request.allow_outside)?;
    let title = request
        .title
        .clone()
        .or_else(|| property_value(template.zettel, "title"))
        .unwrap_or_default();
    let source = request
        .source
        .clone()
        .or_else(|| property_value(template.zettel, "source"))
        .unwrap_or_default();
    let body = request.body.clone().unwrap_or_default();
    let rendered = expand_template(
        &template_text,
        &TemplateValues {
            id: zettel_id.as_str().to_owned(),
            title,
            date: current_utc_date(),
            source,
            body,
        },
    )?;
    let rendered = ensure_trailing_newline(&rendered);
    let write = build_write_plan(&destination, &rendered)?;
    let formatted = format_candidate_source(&write.path, &write.source)?;
    validate_candidate_source(&write.path, &formatted)?;
    atomic_write(&write.path, &formatted)?;

    Ok(CaptureResult {
        destination: write.path,
        zettel_id,
    })
}

/// Lists discovered `#z/tmpl` templates under a corpus root.
pub fn list_templates(root: &Path) -> ZorgResult<Vec<CaptureTemplate>> {
    let root = absolute_normalized(root)?;
    let documents = load_documents(&root)?;
    let mut templates = collect_template_candidates(&documents)
        .into_iter()
        .map(|template| template_summary(&template))
        .collect::<ZorgResult<Vec<_>>>()?;
    templates.sort_by_key(template_sort_key);
    Ok(templates)
}

/// Returns metadata for the template matched by a capture selector.
pub fn inspect_template(root: &Path, selector: &str) -> ZorgResult<CaptureTemplate> {
    let root = absolute_normalized(root)?;
    let documents = load_documents(&root)?;
    let candidates = collect_template_candidates(&documents);
    let template = select_template(selector, &candidates)?;
    template_summary(template)
}

#[derive(Debug)]
struct TemplateCandidate<'a> {
    zettel: &'a Zettel,
    document: &'a ZettelDocument,
}

#[derive(Debug)]
struct TemplateValues {
    id: String,
    title: String,
    date: String,
    source: String,
    body: String,
}

#[derive(Debug)]
struct WritePlan {
    path: PathBuf,
    source: String,
}

fn template_summary(template: &TemplateCandidate<'_>) -> ZorgResult<CaptureTemplate> {
    let template_text = extract_template_text(template)?;
    Ok(CaptureTemplate {
        id: template
            .zettel
            .canonical_id
            .as_ref()
            .or(template.zettel.id.as_ref())
            .cloned(),
        title: property_value(template.zettel, "title"),
        path: template
            .document
            .path
            .as_ref()
            .map(|path| path.as_path().to_path_buf()),
        variables: template_variables(&template_text)?,
    })
}

fn template_sort_key(template: &CaptureTemplate) -> (String, String, String) {
    (
        template
            .id
            .as_ref()
            .map(ZettelId::as_str)
            .unwrap_or("")
            .to_owned(),
        template.title.clone().unwrap_or_default(),
        template
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
    )
}

fn load_documents(root: &Path) -> ZorgResult<Vec<ZettelDocument>> {
    let sources = zorg_store::discover_corpus_sources(root)?;
    sources
        .into_iter()
        .map(|source| {
            let text = fs::read_to_string(source.absolute_path()).map_err(|error| {
                operation_failed(format!(
                    "failed to read template source {}: {error}",
                    source.absolute_path().display()
                ))
            })?;
            zorg_parse::parse_zettel_document_with_path(&text, source.absolute_path()).map_err(
                |error| {
                    operation_failed(format!(
                        "failed to parse template source {}: {error}",
                        source.absolute_path().display()
                    ))
                },
            )
        })
        .collect()
}

fn collect_template_candidates(documents: &[ZettelDocument]) -> Vec<TemplateCandidate<'_>> {
    let mut candidates = Vec::new();
    for document in documents {
        collect_template_candidates_for_zettel(document, &document.root, &mut candidates);
    }
    candidates
}

fn collect_template_candidates_for_zettel<'a>(
    document: &'a ZettelDocument,
    zettel: &'a Zettel,
    candidates: &mut Vec<TemplateCandidate<'a>>,
) {
    if is_template(zettel) {
        candidates.push(TemplateCandidate { zettel, document });
    }

    for block in &zettel.body {
        if let BodyBlock::ChildZettel(child) = block {
            collect_template_candidates_for_zettel(document, child, candidates);
        }
    }
}

fn select_template<'a>(
    selector: &str,
    templates: &'a [TemplateCandidate<'a>],
) -> ZorgResult<&'a TemplateCandidate<'a>> {
    if selector.trim().is_empty() {
        return Err(operation_failed("capture template selector is empty"));
    }

    let mut matches = templates
        .iter()
        .filter(|candidate| template_matches(selector, candidate.zettel));
    let first = matches.next().ok_or_else(|| {
        operation_failed(format!(
            "capture template `{selector}` was not found or is not tagged #z/tmpl"
        ))
    })?;
    if matches.next().is_some() {
        return Err(operation_failed(format!(
            "capture template `{selector}` is ambiguous"
        )));
    }
    Ok(first)
}

fn template_matches(selector: &str, zettel: &Zettel) -> bool {
    if let Some(id) = selector.strip_prefix('@') {
        return zettel
            .canonical_id
            .as_ref()
            .or(zettel.id.as_ref())
            .is_some_and(|candidate| candidate.as_str() == id);
    }

    property_value(zettel, "title").is_some_and(|title| title == selector)
}

fn is_template(zettel: &Zettel) -> bool {
    zettel
        .type_tags
        .iter()
        .any(|tag| tag.tag.as_str() == "z/tmpl")
}

fn property_value(zettel: &Zettel, key: &str) -> Option<String> {
    zettel
        .properties
        .iter()
        .find(|property| property.key == key)
        .map(|property| property.value.clone())
}

fn collect_existing_ids(documents: &[ZettelDocument]) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for document in documents {
        collect_existing_ids_for_zettel(&document.root, &mut ids);
    }
    ids
}

fn collect_existing_ids_for_zettel(zettel: &Zettel, ids: &mut BTreeSet<String>) {
    if let Some(id) = zettel.canonical_id.as_ref().or(zettel.id.as_ref()) {
        ids.insert(id.as_str().to_owned());
    }
    for block in &zettel.body {
        if let BodyBlock::ChildZettel(child) = block {
            collect_existing_ids_for_zettel(child, ids);
        }
    }
}

fn capture_id(
    request: &CaptureRequest,
    template: &TemplateCandidate<'_>,
    existing_ids: &BTreeSet<String>,
) -> ZorgResult<ZettelId> {
    if let Some(id) = &request.id {
        let parsed = ZettelId::parse(id)
            .or_else(|_| ZettelId::parse_canonical(id))
            .map_err(|error| operation_failed(format!("capture ID `{id}` is invalid: {error}")))?;
        if existing_ids.contains(parsed.as_str()) {
            return Err(operation_failed(format!(
                "capture ID `{}` already exists",
                parsed.declaration()
            )));
        }
        return Ok(parsed);
    }

    let template_title = property_value(template.zettel, "title");
    let title = request
        .title
        .as_deref()
        .or(template_title.as_deref())
        .unwrap_or("capture");
    let base = slugify_id(title);
    let mut candidate = base.clone();
    let mut suffix = 2;
    while existing_ids.contains(&candidate) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    ZettelId::parse_canonical(&candidate).map_err(|error| {
        operation_failed(format!(
            "generated capture ID `{candidate}` is invalid: {error}"
        ))
    })
}

fn slugify_id(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() || character == '_' {
            slug.push(character);
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "capture".to_owned()
    } else {
        slug
    }
}

fn extract_template_text(template: &TemplateCandidate<'_>) -> ZorgResult<String> {
    let fences = template
        .zettel
        .body
        .iter()
        .filter_map(|block| match block {
            BodyBlock::FencedCode(fence) if fence.info.as_deref() == Some("zorg-template") => {
                Some(fence.body.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    match fences.len() {
        0 => Ok(dedent(&fallback_body_text(template))),
        1 => Ok(dedent(fences[0])),
        _ => Err(operation_failed(
            "capture template has multiple zorg-template fences",
        )),
    }
}

fn fallback_body_text(template: &TemplateCandidate<'_>) -> String {
    if !matches!(
        template.zettel.kind,
        ZettelKind::File | ZettelKind::Directory
    ) && let Some(span) = template.zettel.span
    {
        let source = &template.document.source;
        let start = source[span.start_byte..span.end_byte]
            .find('\n')
            .map(|offset| span.start_byte + offset + 1)
            .unwrap_or(span.end_byte);
        return source[start..span.end_byte].trim_matches('\n').to_owned();
    }

    template
        .zettel
        .body
        .iter()
        .filter_map(|block| match block {
            BodyBlock::Paragraph(paragraph) => Some(paragraph.text.as_str()),
            BodyBlock::FencedCode(fence) => Some(fence.body.as_str()),
            BodyBlock::ChildZettel(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn dedent(text: &str) -> String {
    let min_indent = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            line.char_indices()
                .find_map(|(index, character)| (!matches!(character, ' ' | '\t')).then_some(index))
        })
        .min()
        .unwrap_or(0);

    text.lines()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn expand_template(text: &str, values: &TemplateValues) -> ZorgResult<String> {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes.get(index..index + 4) == Some(b"{{{{") {
            output.push_str("{{");
            index += 4;
        } else if bytes.get(index..index + 4) == Some(b"}}}}") {
            output.push_str("}}");
            index += 4;
        } else if bytes.get(index..index + 2) == Some(b"{{") {
            let Some(end) = text[index + 2..].find("}}") else {
                return Err(operation_failed(
                    "capture template has an unclosed variable",
                ));
            };
            let name = &text[index + 2..index + 2 + end];
            output.push_str(variable_value(name, values)?);
            index += 2 + end + 2;
        } else {
            let character = text[index..].chars().next().expect("valid char boundary");
            output.push(character);
            index += character.len_utf8();
        }
    }
    Ok(output)
}

fn template_variables(text: &str) -> ZorgResult<Vec<String>> {
    let bytes = text.as_bytes();
    let mut names = BTreeSet::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes.get(index..index + 4) == Some(b"{{{{")
            || bytes.get(index..index + 4) == Some(b"}}}}")
        {
            index += 4;
        } else if bytes.get(index..index + 2) == Some(b"{{") {
            let Some(end) = text[index + 2..].find("}}") else {
                return Err(operation_failed(
                    "capture template has an unclosed variable",
                ));
            };
            let name = &text[index + 2..index + 2 + end];
            validate_variable_name(name)?;
            names.insert(name.to_owned());
            index += 2 + end + 2;
        } else {
            let character = text[index..].chars().next().expect("valid char boundary");
            index += character.len_utf8();
        }
    }
    Ok(names.into_iter().collect())
}

fn variable_value<'a>(name: &str, values: &'a TemplateValues) -> ZorgResult<&'a str> {
    validate_variable_name(name)?;
    match name {
        "id" => Ok(&values.id),
        "title" => Ok(&values.title),
        "date" => Ok(&values.date),
        "source" => Ok(&values.source),
        "body" => Ok(&values.body),
        _ => Err(operation_failed(format!(
            "capture template variable `{name}` is not defined"
        ))),
    }
}

fn validate_variable_name(name: &str) -> ZorgResult<()> {
    match name {
        "id" | "title" | "date" | "source" | "body" => Ok(()),
        _ => Err(operation_failed(format!(
            "capture template variable `{name}` is not defined"
        ))),
    }
}

fn resolve_destination(root: &Path, dest: &Path, allow_outside: bool) -> ZorgResult<PathBuf> {
    let path = if dest.is_absolute() {
        normalize_path(dest)
    } else {
        normalize_path(root.join(dest))
    };
    if !allow_outside && !path.starts_with(root) {
        return Err(operation_failed(format!(
            "capture destination {} is outside root {}",
            path.display(),
            root.display()
        )));
    }
    Ok(path)
}

fn build_write_plan(destination: &Path, rendered: &str) -> ZorgResult<WritePlan> {
    let is_z_file = destination
        .extension()
        .is_some_and(|extension| extension == "z");
    let path = if is_z_file {
        destination.to_path_buf()
    } else {
        destination.join("init.z")
    };

    if path.exists() {
        if path.file_name().is_some_and(|name| name == "init.z") {
            let mut source = fs::read_to_string(&path).map_err(|error| {
                operation_failed(format!("failed to read {}: {error}", path.display()))
            })?;
            if !source.ends_with('\n') {
                source.push('\n');
            }
            if !source.ends_with("\n\n") {
                source.push('\n');
            }
            source.push_str(rendered);
            return Ok(WritePlan { path, source });
        }

        return Err(operation_failed(format!(
            "capture destination {} already exists; refusing to overwrite",
            path.display()
        )));
    }

    let source = source_for_new_file(&path, rendered);
    Ok(WritePlan { path, source })
}

fn source_for_new_file(path: &Path, rendered: &str) -> String {
    if rendered.trim_start().starts_with("%%%") {
        return rendered.to_owned();
    }

    let id = path_derived_id(path).unwrap_or_else(|| "capture".to_owned());
    format!("%%% @{id} #z/ref\nCaptured zettel\n%%%\n\n{rendered}")
}

fn path_derived_id(path: &Path) -> Option<String> {
    let raw = if path.file_name().is_some_and(|name| name == "init.z") {
        path.parent()?.file_name()?.to_str()?
    } else {
        path.file_stem()?.to_str()?
    };
    ZettelId::parse_canonical(raw).ok()?;
    Some(raw.to_owned())
}

fn format_candidate_source(path: &Path, source: &str) -> ZorgResult<String> {
    let document = zorg_parse::parse_zettel_document_with_path(source, path)
        .map_err(|error| operation_failed(format!("failed to parse rendered capture: {error}")))?;
    let plan = plan_fixes(&document, &CorpusView::empty());
    apply_plan_to_source(source, &plan).map(|summary| summary.source)
}

fn validate_candidate_source(path: &Path, source: &str) -> ZorgResult<()> {
    let document = zorg_parse::parse_zettel_document_with_path(source, path)
        .map_err(|error| operation_failed(format!("failed to parse rendered capture: {error}")))?;
    let validation = zorg_parse::validate_document(&document);
    if let Some(diagnostic) = validation
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(operation_failed(format!(
            "{}:{}:{}: {}: {}",
            diagnostic
                .path
                .as_ref()
                .unwrap_or(&SourcePath::new(path.to_path_buf()))
                .as_path()
                .display(),
            diagnostic
                .span
                .and_then(|span| span.start_line)
                .unwrap_or(1),
            diagnostic
                .span
                .and_then(|span| span.start_column)
                .unwrap_or(1),
            diagnostic.code.as_deref().unwrap_or("diagnostic"),
            diagnostic.message
        )));
    }
    Ok(())
}

fn atomic_write(path: &Path, source: &str) -> ZorgResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            operation_failed(format!(
                "failed to create capture destination directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("capture.z");
    let temp_path = path.with_file_name(format!(
        ".{file_name}.zorg-capture-{}.tmp",
        std::process::id()
    ));
    fs::write(&temp_path, source).map_err(|error| {
        operation_failed(format!(
            "failed to write temporary {}: {error}",
            temp_path.display()
        ))
    })?;
    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        operation_failed(format!("failed to replace {}: {error}", path.display()))
    })
}

fn ensure_trailing_newline(source: &str) -> String {
    if source.ends_with('\n') {
        source.to_owned()
    } else {
        format!("{source}\n")
    }
}

fn absolute_normalized(path: &Path) -> ZorgResult<PathBuf> {
    let base = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                operation_failed(format!("failed to read current directory: {error}"))
            })?
            .join(path)
    };
    Ok(normalize_path(base))
}

fn normalize_path(path: impl AsRef<Path>) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn current_utc_date() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() / 86_400)
        .unwrap_or(0);
    let (year, month, day) = civil_date_from_unix_days(days as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_date_from_unix_days(days_since_epoch: i64) -> (i32, u8, u8) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }

    (year as i32, month as u8, day as u8)
}

fn operation_failed(message: impl Into<String>) -> ZorgError {
    ZorgError::OperationFailed {
        message: message.into(),
    }
}
