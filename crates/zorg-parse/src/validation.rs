use std::collections::{BTreeMap, BTreeSet};

use zorg_core::{
    BodyBlock, Diagnostic, LocalId, Property, ReferenceTarget, RelativeId, Severity, SourcePath,
    SourceSpan, Tag, Zettel, ZettelDocument, ZettelId,
};

/// Result of strict semantic validation.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ValidationReport {
    /// Parser, legacy, and semantic diagnostics considered by validation.
    pub diagnostics: Vec<Diagnostic>,
}

impl ValidationReport {
    /// Returns true when validation produced no error diagnostics.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.has_errors()
    }

    /// Returns true when validation produced at least one error diagnostic.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }
}

/// Validates a single parsed document.
#[must_use]
pub fn validate_document(document: &ZettelDocument) -> ValidationReport {
    let mut diagnostics = document.diagnostics.clone();
    append_document_semantics(document, &mut diagnostics);
    ValidationReport { diagnostics }
}

/// Validates a small in-memory parsed corpus.
#[must_use]
pub fn validate_corpus(documents: &[ZettelDocument]) -> ValidationReport {
    let mut diagnostics = Vec::new();

    for document in documents {
        diagnostics.extend(validate_document(document).diagnostics);
    }

    append_cross_document_duplicates(documents, &mut diagnostics);
    ValidationReport { diagnostics }
}

/// Returns failure when single-document strict validation has error diagnostics.
pub fn check_document(document: &ZettelDocument) -> Result<(), Vec<Diagnostic>> {
    let report = validate_document(document);
    if report.has_errors() {
        Err(report.diagnostics)
    } else {
        Ok(())
    }
}

/// Returns failure when corpus strict validation has error diagnostics.
pub fn check_corpus(documents: &[ZettelDocument]) -> Result<(), Vec<Diagnostic>> {
    let report = validate_corpus(documents);
    if report.has_errors() {
        Err(report.diagnostics)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct IdOccurrence {
    path: Option<SourcePath>,
    span: Option<SourceSpan>,
}

#[derive(Debug, Default)]
struct DocumentValidator {
    absolute_ids: BTreeMap<String, IdOccurrence>,
    canonical_local_ids: BTreeMap<String, IdOccurrence>,
    malformed_spans: BTreeSet<(usize, usize, &'static str)>,
}

impl DocumentValidator {
    fn validate_document(&mut self, document: &ZettelDocument, diagnostics: &mut Vec<Diagnostic>) {
        self.collect_existing_absolute_ids(&document.root);
        self.validate_zettel(&document.root, None, diagnostics);
        self.validate_malformed_id_like_text(document, diagnostics);
    }

    fn collect_existing_absolute_ids(&mut self, zettel: &Zettel) {
        if let Some(id) = &zettel.id {
            self.canonical_local_ids
                .entry(id.as_str().to_owned())
                .or_insert(IdOccurrence {
                    path: zettel.path.clone(),
                    span: zettel.span,
                });
        }

        for child in child_zettels(zettel) {
            self.collect_existing_absolute_ids(child);
        }
    }

    fn validate_zettel(
        &mut self,
        zettel: &Zettel,
        ancestor_id: Option<&ZettelId>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if let Some(id) = &zettel.id {
            let occurrence = IdOccurrence {
                path: zettel.path.clone(),
                span: zettel.span,
            };
            if self
                .absolute_ids
                .insert(id.as_str().to_owned(), occurrence.clone())
                .is_some()
            {
                diagnostics.push(semantic(
                    "id.duplicate",
                    format!("duplicate zettel ID `{id}`"),
                    occurrence.span,
                    occurrence.path.as_ref(),
                ));
            }
        }

        if let Some(local_id) = &zettel.local_id {
            self.validate_local_id(local_id, ancestor_id, zettel, diagnostics);
        }

        for property in &zettel.properties {
            validate_property(property, zettel.path.as_ref(), diagnostics);
        }

        let child_ancestor = zettel.id.as_ref().or(ancestor_id);
        for child in child_zettels(zettel) {
            self.validate_zettel(child, child_ancestor, diagnostics);
        }
    }

    fn validate_local_id(
        &mut self,
        local_id: &LocalId,
        ancestor_id: Option<&ZettelId>,
        zettel: &Zettel,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let occurrence = IdOccurrence {
            path: zettel.path.clone(),
            span: zettel.span,
        };

        let Some(ancestor_id) = ancestor_id else {
            diagnostics.push(semantic(
                "local.missing_ancestor",
                format!("local ID `{local_id}` requires a nearest ancestor with an absolute ID"),
                occurrence.span,
                occurrence.path.as_ref(),
            ));
            return;
        };

        let canonical = format!("{}/{}", ancestor_id.as_str(), local_id.as_str());
        if self
            .canonical_local_ids
            .insert(canonical.clone(), occurrence.clone())
            .is_some()
        {
            diagnostics.push(semantic(
                "local.duplicate_canonical",
                format!("duplicate local ID canonicalizes to `@{canonical}`"),
                occurrence.span,
                occurrence.path.as_ref(),
            ));
        }
    }

    fn validate_malformed_id_like_text(
        &mut self,
        document: &ZettelDocument,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut line_start = 0;
        let mut in_code_fence = false;

        for line in document.source.split_inclusive('\n') {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") {
                in_code_fence = !in_code_fence;
                line_start += line.len();
                continue;
            }

            if !in_code_fence {
                self.scan_line_for_malformed_tokens(document, line, line_start, diagnostics);
            }

            line_start += line.len();
        }
    }

    fn scan_line_for_malformed_tokens(
        &mut self,
        document: &ZettelDocument,
        line: &str,
        line_start: usize,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut cursor = 0;
        for raw_token in line.split_whitespace() {
            let Some(relative_start) = line[cursor..].find(raw_token) else {
                continue;
            };
            let token_start = cursor + relative_start;
            cursor = token_start + raw_token.len();

            let token = raw_token.trim_matches(|character: char| {
                matches!(
                    character,
                    '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ',' | ';' | '.'
                )
            });
            if token.is_empty() || token.contains("{{") {
                continue;
            }

            let leading_trim = raw_token.find(token).unwrap_or(0);
            let start = line_start + token_start + leading_trim;
            let end = start + token.len();
            self.validate_possible_token(document, token, start, end, diagnostics);
        }
    }

    fn validate_possible_token(
        &mut self,
        document: &ZettelDocument,
        token: &str,
        start: usize,
        end: usize,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let candidate = match token.as_bytes().first().copied() {
            Some(b'@') if token.starts_with("@@@") => None,
            Some(b'@') if ZettelId::parse(token).is_err() => {
                Some(("id.malformed", format!("malformed ID-like text `{token}`")))
            }
            Some(b'^') if LocalId::parse(token).is_err() => Some((
                "local.malformed",
                format!("malformed local ID-like text `{token}`"),
            )),
            Some(b'+') if RelativeId::parse(&token[1..]).is_err() => Some((
                "reference.malformed",
                format!("malformed child-relative reference `{token}`"),
            )),
            Some(b'~') if RelativeId::parse(&token[1..]).is_err() => Some((
                "reference.malformed",
                format!("malformed sibling-relative reference `{token}`"),
            )),
            Some(b'#') if Tag::parse(token).is_err() && ReferenceTarget::parse(token).is_err() => {
                Some((
                    "reference.malformed",
                    format!("malformed tag or link `{token}`"),
                ))
            }
            _ => None,
        };

        if let Some((code, message)) = candidate {
            if self.malformed_spans.insert((start, end, code)) {
                diagnostics.push(semantic(
                    code,
                    message,
                    Some(SourceSpan::from_offsets(&document.source, start, end)),
                    document.path.as_ref(),
                ));
            }
        }
    }
}

fn append_document_semantics(document: &ZettelDocument, diagnostics: &mut Vec<Diagnostic>) {
    let mut validator = DocumentValidator::default();
    validator.validate_document(document, diagnostics);
}

fn append_cross_document_duplicates(
    documents: &[ZettelDocument],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen = BTreeMap::<String, IdOccurrence>::new();

    for document in documents {
        for (id, occurrence) in document_absolute_ids(document) {
            if let Some(first) = seen.get(&id) {
                if first.path != occurrence.path {
                    diagnostics.push(semantic(
                        "id.duplicate",
                        format!("duplicate zettel ID `@{id}` across corpus"),
                        occurrence.span,
                        occurrence.path.as_ref(),
                    ));
                }
            } else {
                seen.insert(id, occurrence);
            }
        }
    }
}

fn document_absolute_ids(document: &ZettelDocument) -> Vec<(String, IdOccurrence)> {
    let mut ids = Vec::new();
    collect_absolute_ids(&document.root, &mut ids);
    ids
}

fn collect_absolute_ids(zettel: &Zettel, ids: &mut Vec<(String, IdOccurrence)>) {
    if let Some(id) = &zettel.id {
        ids.push((
            id.as_str().to_owned(),
            IdOccurrence {
                path: zettel.path.clone(),
                span: zettel.span,
            },
        ));
    }

    for child in child_zettels(zettel) {
        collect_absolute_ids(child, ids);
    }
}

fn validate_property(
    property: &Property,
    path: Option<&SourcePath>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Err(error) = Property::new(property.key.clone(), property.value.clone()) {
        diagnostics.push(semantic(
            "property.invalid_key",
            error.to_string(),
            property.key_span.or(property.span),
            path,
        ));
    }

    match property.key.as_str() {
        "do" | "due" | "did" if !is_valid_iso_date(&property.value) => {
            diagnostics.push(semantic(
                "property.invalid_date",
                format!(
                    "property `{}` requires an ISO date value like 2026-05-15",
                    property.key
                ),
                property.value_span.or(property.span),
                path,
            ));
        }
        "start" | "end" if !is_valid_hhmm_time(&property.value) => {
            diagnostics.push(semantic(
                "property.invalid_time",
                format!("property `{}` requires a 24-hour HHMM value", property.key),
                property.value_span.or(property.span),
                path,
            ));
        }
        "tick" => diagnostics.push(legacy(
            "legacy tick:: properties are not Zorg v1 syntax",
            property.span,
            path,
        )),
        _ => {}
    }
}

fn is_valid_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }

    let Ok(year) = value[..4].parse::<u16>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u8>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u8>() else {
        return false;
    };

    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };

    day >= 1 && day <= max_day
}

fn is_leap_year(year: u16) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn is_valid_hhmm_time(value: &str) -> bool {
    if value.len() != 4 || !value.as_bytes().iter().all(u8::is_ascii_digit) {
        return false;
    }

    let Ok(hour) = value[..2].parse::<u8>() else {
        return false;
    };
    let Ok(minute) = value[2..].parse::<u8>() else {
        return false;
    };

    hour <= 23 && minute <= 59
}

fn child_zettels(zettel: &Zettel) -> impl Iterator<Item = &Zettel> {
    zettel.body.iter().filter_map(|block| match block {
        BodyBlock::ChildZettel(child) => Some(child.as_ref()),
        _ => None,
    })
}

fn semantic(
    code: impl Into<String>,
    message: impl Into<String>,
    span: Option<SourceSpan>,
    path: Option<&SourcePath>,
) -> Diagnostic {
    with_optional_path(Diagnostic::semantic_validation(code, message, span), path)
}

fn legacy(
    message: impl Into<String>,
    span: Option<SourceSpan>,
    path: Option<&SourcePath>,
) -> Diagnostic {
    with_optional_path(Diagnostic::unsupported_legacy(message, span), path)
}

fn with_optional_path(mut diagnostic: Diagnostic, path: Option<&SourcePath>) -> Diagnostic {
    if let Some(path) = path {
        diagnostic.path = Some(path.clone());
    }

    diagnostic
}

#[cfg(test)]
mod tests {
    use super::{check_document, validate_corpus, validate_document};
    use std::fs;
    use std::path::{Path, PathBuf};
    use zorg_core::{
        BodyBlock, DiagnosticCategory, LocalId, Severity, Zettel, ZettelDocument, ZettelId,
        ZettelKey, ZettelKind,
    };

    use crate::parse_zettel_document_with_path;

    #[test]
    fn valid_fixtures_pass_document_validation() {
        for fixture in [
            "minimal.z",
            "nested.z",
            "query_and_template.z",
            "dir/init.z",
        ] {
            let document = parse_fixture(fixture);
            let report = validate_document(&document);
            assert!(
                report.is_valid(),
                "unexpected validation diagnostics for {fixture}: {:#?}",
                report.diagnostics
            );
        }
    }

    #[test]
    fn legacy_fixture_reports_strict_legacy_diagnostics() {
        let document = parse_fixture("legacy_invalid.z");
        let report = validate_document(&document);

        assert!(report.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.category == DiagnosticCategory::Legacy)
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("ID::"))
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("folgezettel"))
        );
    }

    #[test]
    fn duplicate_ids_are_reported_with_path_and_span() {
        let source = "\
%%% @dupe #z/ref
Duplicate fixture
%%%

- @dupe #z/ref Nested duplicate.
";
        let path = fixture_path("semantic_duplicate.z");
        let document =
            parse_zettel_document_with_path(source, path.clone()).expect("parse document");
        let report = validate_document(&document);
        let duplicate = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("id.duplicate"))
            .expect("duplicate diagnostic");

        assert_eq!(duplicate.severity, Severity::Error);
        assert_eq!(duplicate.path.as_ref().expect("path").as_path(), path);
        assert_eq!(duplicate.span.expect("span").start_line, Some(5));
    }

    #[test]
    fn corpus_duplicate_ids_are_reported_across_files() {
        let first = parse_source("%%% @same #z/ref\nFirst\n%%%\n", "first.z");
        let second = parse_source("%%% @same #z/ref\nSecond\n%%%\n", "second.z");
        let report = validate_corpus(&[first, second]);

        assert!(
            report
                .diagnostics
                .iter()
                .any(
                    |diagnostic| diagnostic.code.as_deref() == Some("id.duplicate")
                        && diagnostic.message.contains("across corpus")
                )
        );
    }

    #[test]
    fn local_ids_require_absolute_ancestor_and_unique_canonical_form() {
        let missing = parse_source("%%% ^orphan #z/ref\nOrphan\n%%%\n", "missing-local.z");
        let missing_report = validate_document(&missing);
        assert!(
            missing_report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.as_deref() == Some("local.missing_ancestor"))
        );

        let duplicate = duplicate_local_document();
        let duplicate_report = validate_document(&duplicate);
        assert!(duplicate_report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("local.duplicate_canonical")
                && diagnostic.message.contains("`@root/parent/child`")
        }));
    }

    #[test]
    fn invalid_known_property_values_are_reported() {
        let document = parse_source(
            "\
%%% @bad-props #z/todo due::2026-02-30 start::2460
Bad properties
%%%
",
            "bad-props.z",
        );
        let report = validate_document(&document);

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.as_deref() == Some("property.invalid_date"))
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.as_deref() == Some("property.invalid_time"))
        );
    }

    #[test]
    fn malformed_id_like_text_is_reported_when_recovered_as_text() {
        let document = parse_source(
            "\
%%% @malformed #z/ref
Malformed
%%%

This paragraph mentions #bad.link and @bad.id.
",
            "malformed-text.z",
        );
        let report = validate_document(&document);

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.as_deref() == Some("reference.malformed"))
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.as_deref() == Some("id.malformed"))
        );
    }

    #[test]
    fn strict_document_check_returns_failure_status() {
        let document = parse_source(
            "\
%%% @bad-check #z/todo due::not-a-date
Bad check
%%%
",
            "bad-check.z",
        );

        assert!(check_document(&document).is_err());
    }

    fn parse_fixture(path: impl AsRef<Path>) -> zorg_core::ZettelDocument {
        let path = fixture_path(path);
        let source = fs::read_to_string(&path).expect("fixture");
        parse_zettel_document_with_path(&source, path).expect("parse document")
    }

    fn parse_source(source: &str, path: &str) -> zorg_core::ZettelDocument {
        parse_zettel_document_with_path(source, fixture_path(path)).expect("parse document")
    }

    fn duplicate_local_document() -> ZettelDocument {
        let path = fixture_path("duplicate-local.z");
        let mut root = Zettel::new(ZettelKey::new("root"), ZettelKind::File);
        root.path = Some(path.clone().into());
        root.id = Some(ZettelId::parse("@root").expect("valid ID"));

        let mut parent = Zettel::new(ZettelKey::new("parent"), ZettelKind::Nested);
        parent.path = Some(path.clone().into());
        parent.id = Some(ZettelId::parse("@root/parent").expect("valid ID"));

        for key in ["first-child", "second-child"] {
            let mut child = Zettel::new(ZettelKey::new(key), ZettelKind::Nested);
            child.path = Some(path.clone().into());
            child.parent = Some(parent.key.clone());
            child.local_id = Some(LocalId::parse("^child").expect("valid local ID"));
            parent.children.push(child.key.clone());
            parent.body.push(BodyBlock::ChildZettel(Box::new(child)));
        }

        root.children.push(parent.key.clone());
        root.body.push(BodyBlock::ChildZettel(Box::new(parent)));

        ZettelDocument {
            path: Some(path.into()),
            source: String::new(),
            root,
            diagnostics: Vec::new(),
        }
    }

    fn fixture_path(path: impl AsRef<Path>) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("fixtures/corpus")
            .join(path)
    }
}
