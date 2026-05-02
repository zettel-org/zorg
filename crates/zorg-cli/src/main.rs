use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use zorg_core::{Diagnostic, DiagnosticCategory, Severity};
use zorg_store::{Store, StoreOptions};

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
        Some("db") => run_db(args.collect()),
        Some("index") => {
            eprintln!(
                "`zorg index` is deferred; use `zorg db reindex` for the database command path"
            );
            std::process::exit(2);
        }
        Some("fix" | "query" | "capture") => {
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
    validate_cli_source_path(&path);

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
        validate_cli_source_path(&path);

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

fn validate_cli_source_path(path: &Path) {
    if let Err(error) = zorg_store::validate_explicit_source_path(path) {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run_db(args: Vec<String>) {
    let Some(subcommand) = args.first() else {
        eprintln!("usage: zorg db <status|reindex> [--root PATH] [--db PATH]");
        std::process::exit(2);
    };

    match subcommand.as_str() {
        "status" => run_db_status(parse_store_options(&args[1..])),
        "reindex" => run_db_reindex(parse_store_options(&args[1..])),
        "-h" | "--help" => print_db_help(),
        other => {
            eprintln!("unknown zorg db command: {other}");
            eprintln!("usage: zorg db <status|reindex> [--root PATH] [--db PATH]");
            std::process::exit(2);
        }
    }
}

fn parse_store_options(args: &[String]) -> StoreOptions {
    let mut root = None;
    let mut db = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for --root");
                    std::process::exit(2);
                };
                root = Some(PathBuf::from(value));
            }
            "--db" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for --db");
                    std::process::exit(2);
                };
                db = Some(PathBuf::from(value));
            }
            argument => {
                eprintln!("unexpected argument for `zorg db`: {argument}");
                std::process::exit(2);
            }
        }
        index += 1;
    }

    let result = match (root, db) {
        (Some(root), Some(db)) => StoreOptions::new(root, db),
        (Some(root), None) => StoreOptions::for_root(root),
        (None, Some(db)) => StoreOptions::new(
            StoreOptions::default_root().unwrap_or_else(|error| {
                eprintln!("{error}");
                std::process::exit(1);
            }),
            db,
        ),
        (None, None) => StoreOptions::default_paths(),
    };

    result.unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    })
}

fn run_db_status(options: StoreOptions) {
    let store = open_store(options);
    let schema_version = store.schema_version().unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });
    let sources = store.discover_sources().unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });
    let status = store.index_status().unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    println!("root: {}", store.root().display());
    println!("database: {}", store.database_path().display());
    println!("schema_version: {schema_version}");
    println!("discovered_files: {}", sources.len());
    println!("indexed_files: {}", status.indexed_files);
    println!("unchanged_files: {}", status.unchanged_files);
    println!("new_files: {}", status.new_files);
    println!("changed_files: {}", status.changed_files);
    println!("deleted_files: {}", status.deleted_files);
    println!("diagnostics: {}", status.diagnostic_count);
    println!(
        "last_indexed_at_unix_ms: {}",
        status
            .last_indexed_at_unix_ms
            .map(|timestamp| timestamp.to_string())
            .unwrap_or_else(|| "never".to_owned())
    );
}

fn run_db_reindex(options: StoreOptions) {
    let mut store = open_store(options);
    let summary = store.reindex().unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    println!("root: {}", store.root().display());
    println!("database: {}", store.database_path().display());
    println!("discovered_files: {}", summary.discovered_files);
    println!("indexed_files: {}", summary.indexed_files);
    println!("unchanged_files: {}", summary.unchanged_files);
    println!("new_files: {}", summary.new_files);
    println!("changed_files: {}", summary.changed_files);
    println!("deleted_files: {}", summary.deleted_files);
    println!("indexed_zettel: {}", summary.zettel_count);
    println!("diagnostics: {}", summary.diagnostic_count);
    println!(
        "last_indexed_at_unix_ms: {}",
        summary
            .last_indexed_at_unix_ms
            .map(|timestamp| timestamp.to_string())
            .unwrap_or_else(|| "never".to_owned())
    );
    println!("reindex: incremental complete");
}

fn open_store(options: StoreOptions) -> Store {
    Store::open_with_options(options).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    })
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
  db status [--root PATH] [--db PATH]
            Show SQLite store status and pending source changes
  db reindex [--root PATH] [--db PATH]
            Incrementally refresh the SQLite store from discovered .z sources
  index     Deferred alias notice for corpus indexing
  query     Placeholder for SWOG LIST queries
  fix       Placeholder for strict checks and autofixes
  capture   Placeholder for template capture

Options:
  -h, --help     Print help
  -V, --version  Print version

Parser and store foundations are available. Query, capture, and fix behavior
are intentionally pending."
    );
}

fn print_db_help() {
    println!(
        "\
Usage: zorg db <status|reindex> [--root PATH] [--db PATH]

Commands:
  status   Show SQLite store status and pending source changes
  reindex  Incrementally refresh the SQLite store from discovered .z sources"
    );
}
