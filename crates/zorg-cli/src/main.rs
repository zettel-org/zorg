use std::env;
use std::fs;
use std::path::PathBuf;

use zorg_core::{Diagnostic, DiagnosticCategory, Severity};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        None | Some("-h" | "--help") => print_help(),
        Some("-V" | "--version") => println!("zorg {VERSION}"),
        Some("parse") => {
            let Some(path) = args.next() else {
                eprintln!("usage: zorg parse FILE");
                std::process::exit(2);
            };
            if let Some(extra) = args.next() {
                eprintln!("unexpected argument for `zorg parse`: {extra}");
                std::process::exit(2);
            }
            run_parse(PathBuf::from(path));
        }
        Some("check") => {
            let paths = args.map(PathBuf::from).collect::<Vec<_>>();
            if paths.is_empty() {
                eprintln!("usage: zorg check FILE...");
                std::process::exit(2);
            }
            run_check(paths);
        }
        Some("fix" | "query" | "capture" | "index") => {
            eprintln!("zorg command behavior is pending; this is an Epic 1 workspace stub");
            std::process::exit(2);
        }
        Some(command) => {
            eprintln!("unknown zorg command: {command}");
            eprintln!("run `zorg --help` for usage");
            std::process::exit(2);
        }
    }
}

fn run_parse(path: PathBuf) {
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("failed to read {}: {error}", path.display());
            std::process::exit(1);
        }
    };

    let mut document = match zorg_parse::parse_zettel_document_with_path(&source, path.clone()) {
        Ok(document) => document,
        Err(error) => {
            eprintln!("failed to parse {}: {error}", path.display());
            std::process::exit(1);
        }
    };
    let validation = zorg_parse::validate_document(&document);
    document.diagnostics = validation.diagnostics;
    document.root.diagnostics = document.diagnostics.clone();
    zorg_parse::resolve_document(&mut document);

    if document.diagnostics.iter().any(|diagnostic| {
        diagnostic.category == DiagnosticCategory::Syntax && diagnostic.severity == Severity::Error
    }) {
        for diagnostic in &document.diagnostics {
            eprintln!("{}", diagnostic.message);
        }
        std::process::exit(1);
    }

    match serde_json::to_string_pretty(&document) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("failed to serialize parse output: {error}");
            std::process::exit(1);
        }
    }
}

fn run_check(paths: Vec<PathBuf>) {
    let mut documents = Vec::new();

    for path in paths {
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("failed to read {}: {error}", path.display());
                std::process::exit(1);
            }
        };

        match zorg_parse::parse_zettel_document_with_path(&source, path.clone()) {
            Ok(document) => documents.push(document),
            Err(error) => {
                eprintln!("failed to parse {}: {error}", path.display());
                std::process::exit(1);
            }
        }
    }

    let validation = zorg_parse::validate_corpus(&documents);
    let resolution = zorg_parse::resolve_corpus(&mut documents);
    let mut diagnostics = validation.diagnostics;
    diagnostics.extend(resolution.diagnostics);

    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        for diagnostic in &diagnostics {
            print_diagnostic(diagnostic);
        }
        std::process::exit(1);
    }
}

fn print_diagnostic(diagnostic: &Diagnostic) {
    let path = diagnostic
        .path
        .as_ref()
        .map(|path| path.as_path().display().to_string())
        .unwrap_or_else(|| "<unknown>".to_owned());
    let line = diagnostic
        .span
        .and_then(|span| span.start_line)
        .unwrap_or(1);
    let column = diagnostic
        .span
        .and_then(|span| span.start_column)
        .unwrap_or(1);
    let code = diagnostic.code.as_deref().unwrap_or("diagnostic");

    eprintln!("{path}:{line}:{column}: {code}: {}", diagnostic.message);
}

fn print_help() {
    println!(
        "\
zorg {VERSION}

Usage: zorg [OPTIONS] [COMMAND]

Commands:
  parse FILE Emit a JSON semantic model for a .z file
  check FILE... Run strict syntax and semantic validation
  index     Placeholder for corpus indexing
  query     Placeholder for SWOG LIST queries
  fix       Placeholder for strict checks and autofixes
  capture   Placeholder for template capture
  check     Placeholder alias for strict checking

Options:
  -h, --help     Print help
  -V, --version  Print version

Epic 1 provides the executable workspace skeleton only. Parser, store, query,
capture, and fix behavior are intentionally pending."
    );
}
