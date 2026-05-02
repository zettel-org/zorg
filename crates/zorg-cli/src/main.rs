use std::env;
use std::fs;
use std::path::PathBuf;

use zorg_core::{DiagnosticCategory, Severity};

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
        Some("check" | "fix" | "query" | "capture" | "index") => {
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

    let document = match zorg_parse::parse_zettel_document_with_path(&source, path.clone()) {
        Ok(document) => document,
        Err(error) => {
            eprintln!("failed to parse {}: {error}", path.display());
            std::process::exit(1);
        }
    };

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

fn print_help() {
    println!(
        "\
zorg {VERSION}

Usage: zorg [OPTIONS] [COMMAND]

Commands:
  parse FILE Emit a JSON semantic model for a .z file
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
