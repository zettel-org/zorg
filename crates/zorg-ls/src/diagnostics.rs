use std::path::Path;

use tower_lsp::lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticSeverity, NumberOrString, Position, Range, Url,
};
use zorg_core::{
    Diagnostic as CoreDiagnostic, DiagnosticCategory, Severity, SourceSpan, position_for_offset,
};
use zorg_store::StoredDiagnostic;

pub(crate) fn live_diagnostics(uri: &Url, text: &str) -> Vec<LspDiagnostic> {
    let path = uri.to_file_path().ok();
    let parsed = match path {
        Some(path) => zorg_parse::parse_zettel_document_with_path(text, path),
        None => zorg_parse::parse_zettel_document(text),
    };

    match parsed {
        Ok(document) => zorg_parse::validate_document(&document)
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic_to_lsp(&diagnostic, Some(text)))
            .collect(),
        Err(error) => vec![LspDiagnostic {
            range: zero_range(),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("zorg".to_owned()),
            message: error.to_string(),
            ..LspDiagnostic::default()
        }],
    }
}

pub(crate) fn stored_diagnostic_to_lsp(diagnostic: &StoredDiagnostic) -> LspDiagnostic {
    LspDiagnostic {
        range: stored_range(diagnostic),
        severity: Some(severity_from_str(&diagnostic.severity)),
        code: diagnostic
            .code
            .clone()
            .map(NumberOrString::String)
            .or_else(|| Some(NumberOrString::String(diagnostic.category.clone()))),
        source: Some(format!("zorg.{}", diagnostic.category)),
        message: diagnostic.message.clone(),
        ..LspDiagnostic::default()
    }
}

pub(crate) fn file_uri(path: &Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

fn diagnostic_to_lsp(diagnostic: &CoreDiagnostic, source: Option<&str>) -> LspDiagnostic {
    LspDiagnostic {
        range: diagnostic_range(diagnostic.span, source),
        severity: Some(severity(diagnostic.severity)),
        code: diagnostic
            .code
            .clone()
            .map(NumberOrString::String)
            .or_else(|| {
                Some(NumberOrString::String(
                    category(&diagnostic.category).to_owned(),
                ))
            }),
        source: Some(format!("zorg.{}", category(&diagnostic.category))),
        message: diagnostic.message.clone(),
        ..LspDiagnostic::default()
    }
}

fn diagnostic_range(span: Option<SourceSpan>, source: Option<&str>) -> Range {
    let Some(span) = span else {
        return zero_range();
    };

    match (
        span.start_line,
        span.start_column,
        span.end_line,
        span.end_column,
    ) {
        (Some(start_line), Some(start_column), Some(end_line), Some(end_column)) => Range {
            start: position(start_line, start_column),
            end: position(end_line, end_column),
        },
        _ => source.map_or_else(zero_range, |source| {
            let start = position_for_offset(source, span.start_byte);
            let end = position_for_offset(source, span.end_byte);
            Range {
                start: position(start.line, start.column),
                end: position(end.line, end.column),
            }
        }),
    }
}

fn stored_range(diagnostic: &StoredDiagnostic) -> Range {
    match (
        diagnostic.start_line,
        diagnostic.start_column,
        diagnostic.end_line,
        diagnostic.end_column,
    ) {
        (Some(start_line), Some(start_column), Some(end_line), Some(end_column)) => Range {
            start: stored_position(start_line, start_column),
            end: stored_position(end_line, end_column),
        },
        _ => zero_range(),
    }
}

fn zero_range() -> Range {
    Range {
        start: Position::new(0, 0),
        end: Position::new(0, 0),
    }
}

fn position(line: usize, column: usize) -> Position {
    Position::new(
        one_based_to_zero_based(line),
        one_based_to_zero_based(column),
    )
}

fn stored_position(line: i64, column: i64) -> Position {
    Position::new(
        i64_one_based_to_zero_based(line),
        i64_one_based_to_zero_based(column),
    )
}

fn one_based_to_zero_based(value: usize) -> u32 {
    u32::try_from(value.saturating_sub(1)).unwrap_or(u32::MAX)
}

fn i64_one_based_to_zero_based(value: i64) -> u32 {
    u32::try_from(value.saturating_sub(1)).unwrap_or(0)
}

fn severity(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    }
}

fn severity_from_str(severity: &str) -> DiagnosticSeverity {
    match severity {
        "warning" => DiagnosticSeverity::WARNING,
        "info" => DiagnosticSeverity::INFORMATION,
        _ => DiagnosticSeverity::ERROR,
    }
}

fn category(category: &DiagnosticCategory) -> &'static str {
    match category {
        DiagnosticCategory::Syntax => "syntax",
        DiagnosticCategory::Semantic => "semantic",
        DiagnosticCategory::Legacy => "legacy",
        DiagnosticCategory::Unsupported => "unsupported",
    }
}
