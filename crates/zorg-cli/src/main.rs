use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use zorg_core::{Diagnostic, DiagnosticCategory, Severity, SourcePath, SourceSpan};
use zorg_fix::{CorpusView, FixOp, FixPlan, apply_plan_to_source, plan_fixes};
use zorg_query::{QueryContext, QueryDate};
use zorg_store::{Store, StoreOptions, discover_corpus_sources};

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
        Some("check") => run_check_cli(args.collect()),
        Some("db") => run_db(args.collect()),
        Some("query") => run_query(args.collect()),
        Some("index") => {
            eprintln!(
                "`zorg index` is deferred; use `zorg db reindex` for the database command path"
            );
            std::process::exit(2);
        }
        Some("fix") => run_fix_cli(args.collect()),
        Some("capture") => {
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

fn run_check_cli(args: Vec<String>) {
    let inputs = parse_strict_inputs("check", &args);
    let documents = collect_documents_for_inputs(inputs);
    let diagnostics = run_strict_validation(&documents);

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

fn run_fix_cli(args: Vec<String>) {
    let (inputs, options) = parse_fix_options(&args);

    let mut documents = collect_documents_for_inputs(inputs);
    let diagnostics = validate_documents(&mut documents);
    let plans = plan_documents(&documents);
    let pending_fixes = plans.iter().map(FixPlan::len).sum::<usize>();
    let has_strict_errors = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error);

    if options.check {
        if has_strict_errors {
            for diagnostic in &diagnostics {
                print_diagnostic(diagnostic);
            }
        }

        if pending_fixes > 0 {
            for plan in &plans {
                for op in &plan.ops {
                    print_pending_fix(plan.path.as_ref(), op);
                }
            }
        }

        if has_strict_errors || pending_fixes > 0 {
            std::process::exit(1);
        }
        return;
    }

    if pending_fixes == 0 {
        if has_strict_errors {
            for diagnostic in &diagnostics {
                print_diagnostic(diagnostic);
            }
            std::process::exit(1);
        }
        return;
    }

    let rewritten = apply_plans_to_documents(&documents, &plans);
    let mut reparsed = reparse_rewritten_documents(&documents, &rewritten);
    let rewritten_diagnostics = validate_documents(&mut reparsed);
    if rewritten_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        for diagnostic in &rewritten_diagnostics {
            print_diagnostic(diagnostic);
        }
        eprintln!("zorg fix refused to write because rewritten sources failed strict validation");
        std::process::exit(1);
    }

    write_rewritten_documents(&documents, &rewritten);
}

#[derive(Debug, Default)]
struct FixCliOptions {
    check: bool,
}

fn parse_fix_options(args: &[String]) -> (StrictInputs, FixCliOptions) {
    let mut options = FixCliOptions::default();
    let mut remaining = Vec::with_capacity(args.len());
    let mut index = 0;

    while index < args.len() {
        let argument = args[index].as_str();
        match argument {
            "-h" | "--help" => {
                print_fix_help();
                std::process::exit(0);
            }
            "--check" => options.check = true,
            other => remaining.push(other.to_owned()),
        }
        index += 1;
    }

    let inputs = parse_strict_inputs("fix", &remaining);
    (inputs, options)
}

#[derive(Debug)]
enum StrictInputs {
    Files(Vec<PathBuf>),
    Root(StoreOptions),
}

fn parse_strict_inputs(command: &str, args: &[String]) -> StrictInputs {
    let mut files = Vec::new();
    let mut store_args = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                if command == "check" {
                    print_check_help();
                } else {
                    print_fix_help();
                }
                std::process::exit(0);
            }
            "--root" | "--db" => {
                let flag = args[index].clone();
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for {flag}");
                    std::process::exit(2);
                };
                store_args.push(flag);
                store_args.push(value.clone());
            }
            argument if argument.starts_with('-') => {
                eprintln!("unexpected argument for `zorg {command}`: {argument}");
                std::process::exit(2);
            }
            argument => files.push(PathBuf::from(argument)),
        }
        index += 1;
    }

    if !files.is_empty() && !store_args.is_empty() {
        eprintln!("`zorg {command}` accepts either FILE arguments or --root/--db, not both");
        std::process::exit(2);
    }

    if !store_args.is_empty() {
        return StrictInputs::Root(parse_store_options(&store_args));
    }

    if files.is_empty() {
        eprintln!("usage: zorg {command} [--root PATH] [--db PATH] FILE...");
        std::process::exit(2);
    }

    StrictInputs::Files(files)
}

fn collect_documents_for_inputs(inputs: StrictInputs) -> Vec<zorg_core::ZettelDocument> {
    match inputs {
        StrictInputs::Files(paths) => paths
            .into_iter()
            .map(|path| {
                validate_cli_source_path(&path);
                let source = match fs::read_to_string(&path) {
                    Ok(source) => source,
                    Err(error) => {
                        eprintln!("failed to read {}: {error}", path.display());
                        std::process::exit(1);
                    }
                };
                match zorg_parse::parse_zettel_document_with_path(&source, path.clone()) {
                    Ok(document) => document,
                    Err(error) => {
                        eprintln!("failed to parse {}: {error}", path.display());
                        std::process::exit(1);
                    }
                }
            })
            .collect(),
        StrictInputs::Root(options) => {
            let sources = discover_corpus_sources(options.corpus_root()).unwrap_or_else(|error| {
                eprintln!("{error}");
                std::process::exit(1);
            });
            sources
                .into_iter()
                .map(|source| {
                    let path = source.absolute_path().to_path_buf();
                    let text = match fs::read_to_string(&path) {
                        Ok(text) => text,
                        Err(error) => {
                            eprintln!("failed to read {}: {error}", path.display());
                            std::process::exit(1);
                        }
                    };
                    match zorg_parse::parse_zettel_document_with_path(&text, path.clone()) {
                        Ok(document) => document,
                        Err(error) => {
                            eprintln!("failed to parse {}: {error}", path.display());
                            std::process::exit(1);
                        }
                    }
                })
                .collect()
        }
    }
}

fn run_strict_validation(documents: &[zorg_core::ZettelDocument]) -> Vec<Diagnostic> {
    let mut documents = documents.to_vec();
    validate_documents(&mut documents)
}

fn validate_documents(documents: &mut [zorg_core::ZettelDocument]) -> Vec<Diagnostic> {
    let validation = zorg_parse::validate_corpus(documents);
    let resolution = zorg_parse::resolve_corpus(documents);
    let mut diagnostics = validation.diagnostics;
    diagnostics.extend(resolution.diagnostics);
    diagnostics
}

fn plan_documents(documents: &[zorg_core::ZettelDocument]) -> Vec<FixPlan> {
    let canonical_ids = collect_corpus_canonical_ids(documents);
    let view = CorpusView::from_canonical_ids(canonical_ids.iter().map(String::as_str));
    documents
        .iter()
        .map(|document| plan_fixes(document, &view))
        .collect()
}

fn collect_corpus_canonical_ids(documents: &[zorg_core::ZettelDocument]) -> Vec<String> {
    let mut ids = Vec::new();
    for document in documents {
        collect_canonical_ids(&document.root, &mut ids);
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn collect_canonical_ids(zettel: &zorg_core::Zettel, ids: &mut Vec<String>) {
    if let Some(canonical) = zettel
        .canonical_id
        .as_ref()
        .or(zettel.id.as_ref())
        .map(zorg_core::ZettelId::as_str)
    {
        ids.push(canonical.to_owned());
    }

    for block in &zettel.body {
        if let zorg_core::BodyBlock::ChildZettel(child) = block {
            collect_canonical_ids(child, ids);
        }
    }
}

fn apply_plans_to_documents(
    documents: &[zorg_core::ZettelDocument],
    plans: &[FixPlan],
) -> Vec<String> {
    documents
        .iter()
        .zip(plans)
        .map(|(document, plan)| {
            apply_plan_to_source(&document.source, plan)
                .unwrap_or_else(|error| {
                    let path = document
                        .path
                        .as_ref()
                        .map(|path| path.as_path().display().to_string())
                        .unwrap_or_else(|| "<unknown>".to_owned());
                    eprintln!("failed to apply fixes for {path}: {error}");
                    std::process::exit(1);
                })
                .source
        })
        .collect()
}

fn reparse_rewritten_documents(
    documents: &[zorg_core::ZettelDocument],
    rewritten: &[String],
) -> Vec<zorg_core::ZettelDocument> {
    documents
        .iter()
        .zip(rewritten)
        .map(|(document, source)| {
            let path = document.path.as_ref().unwrap_or_else(|| {
                eprintln!("cannot rewrite a document without a source path");
                std::process::exit(1);
            });
            zorg_parse::parse_zettel_document_with_path(source, path.as_path()).unwrap_or_else(
                |error| {
                    eprintln!(
                        "failed to parse rewritten {}: {error}",
                        path.as_path().display()
                    );
                    std::process::exit(1);
                },
            )
        })
        .collect()
}

fn write_rewritten_documents(documents: &[zorg_core::ZettelDocument], rewritten: &[String]) {
    for (index, (document, source)) in documents.iter().zip(rewritten).enumerate() {
        if document.source == *source {
            continue;
        }
        let path = document.path.as_ref().unwrap_or_else(|| {
            eprintln!("cannot rewrite a document without a source path");
            std::process::exit(1);
        });
        atomic_write(path.as_path(), source, index);
    }
}

fn atomic_write(path: &Path, source: &str, index: usize) {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("source.z");
    let temp_path = parent.join(format!(
        ".{file_name}.zorg-fix-{}-{index}.tmp",
        std::process::id()
    ));

    if let Err(error) = fs::write(&temp_path, source) {
        eprintln!("failed to write temporary {}: {error}", temp_path.display());
        std::process::exit(1);
    }
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        eprintln!("failed to replace {}: {error}", path.display());
        std::process::exit(1);
    }
}

fn print_pending_fix(path: Option<&SourcePath>, op: &FixOp) {
    let path_text = path
        .map(|path| path.as_path().display().to_string())
        .unwrap_or_else(|| "<unknown>".to_owned());
    let (line, column) = primary_position(op.primary_span());
    eprintln!(
        "{path_text}:{line}:{column}: {code}: {message}",
        code = op.rule_code,
        message = op.message,
    );
}

fn primary_position(span: Option<SourceSpan>) -> (usize, usize) {
    let Some(span) = span else {
        return (1, 1);
    };
    (span.start_line.unwrap_or(1), span.start_column.unwrap_or(1))
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

fn run_query(args: Vec<String>) {
    let (query, options) = parse_query_options(&args);
    let store = open_store(options);
    ensure_query_index_ready(&store);
    let context = query_context_for_store(&store);

    let output = match query {
        CliQuery::Inline(query) => {
            zorg_query::execute_and_render_list_query(&store, &context, &query)
        }
        CliQuery::Id(query_id) => {
            zorg_query::execute_and_render_list_query_by_id(&store, &context, &query_id)
        }
    }
    .unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    if !output.is_empty() {
        println!("{output}");
    }
}

enum CliQuery {
    Inline(String),
    Id(String),
}

fn parse_query_options(args: &[String]) -> (CliQuery, StoreOptions) {
    let mut inline_query = None;
    let mut query_id = None;
    let mut store_args = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                print_query_help();
                std::process::exit(0);
            }
            "--root" | "--db" => {
                let flag = args[index].clone();
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for {flag}");
                    std::process::exit(2);
                };
                store_args.push(flag);
                store_args.push(value.clone());
            }
            "--id" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for --id");
                    std::process::exit(2);
                };
                if query_id.replace(value.clone()).is_some() {
                    eprintln!("zorg query accepts at most one --id value");
                    std::process::exit(2);
                }
            }
            argument if argument.starts_with('-') && inline_query.is_some() => {
                eprintln!("unexpected argument for `zorg query`: {argument}");
                std::process::exit(2);
            }
            argument => {
                if inline_query.replace(argument.to_owned()).is_some() {
                    eprintln!("zorg query accepts exactly one query string argument");
                    std::process::exit(2);
                }
            }
        }
        index += 1;
    }

    let query = match (inline_query, query_id) {
        (Some(_), Some(_)) => {
            eprintln!("zorg query accepts either an inline query or --id, not both");
            std::process::exit(2);
        }
        (Some(query), None) => CliQuery::Inline(query),
        (None, Some(query_id)) => CliQuery::Id(query_id),
        (None, None) => {
            eprintln!("usage: zorg query '<swog>' [--root PATH] [--db PATH]");
            eprintln!("   or: zorg query --id @some/query [--root PATH] [--db PATH]");
            std::process::exit(2);
        }
    };

    (query, parse_store_options(&store_args))
}

fn ensure_query_index_ready(store: &Store) {
    let status = store.index_status().unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    if status.last_indexed_at_unix_ms.is_none() {
        eprintln!(
            "query index is missing for root {}; run `zorg db reindex --root {} --db {}` first",
            store.root().display(),
            store.root().display(),
            store.database_path().display()
        );
        std::process::exit(1);
    }

    if status.new_files > 0 || status.changed_files > 0 || status.deleted_files > 0 {
        eprintln!(
            "query index is stale for root {}; run `zorg db reindex --root {} --db {}` first",
            store.root().display(),
            store.root().display(),
            store.database_path().display()
        );
        std::process::exit(1);
    }
}

fn query_context_for_store(store: &Store) -> QueryContext {
    let now_unix_ms = current_unix_ms();
    QueryContext::new(store.root(), current_query_date(now_unix_ms), now_unix_ms)
}

fn current_unix_ms() -> i64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|error| {
            eprintln!("system clock is before the Unix epoch: {error}");
            std::process::exit(1);
        });

    i64::try_from(duration.as_millis()).unwrap_or_else(|_| {
        eprintln!("system clock value is too large for query timestamps");
        std::process::exit(1);
    })
}

fn current_query_date(now_unix_ms: i64) -> QueryDate {
    let days = now_unix_ms.div_euclid(86_400_000);
    civil_date_from_unix_days(days)
}

fn civil_date_from_unix_days(days_since_epoch: i64) -> QueryDate {
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

    QueryDate::new(year as i32, month as u8, day as u8).expect("civil date should be valid")
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
    println!("effective_tags: {}", status.effective_tag_count);
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
    println!("effective_tags: {}", summary.effective_tag_count);
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
  check [--root PATH] FILE...
            Run strict syntax and semantic validation across files or a corpus
  db status [--root PATH] [--db PATH]
            Show SQLite store status and pending source changes
  db reindex [--root PATH] [--db PATH]
            Incrementally refresh the SQLite store from discovered .z sources
  index     Deferred alias notice for corpus indexing
  query '<swog>' [--root PATH] [--db PATH]
  query --id @some/query [--root PATH] [--db PATH]
            Run an inline or stored SWOG LIST query against an existing index
  fix [--check] [--root PATH] FILE...
            Apply safe autofixes or report pending autofixes with --check
  capture   Placeholder for template capture

Options:
  -h, --help     Print help
  -V, --version  Print version

Parser, store, inline query, and safe fix foundations are available. Capture
behavior is intentionally pending."
    );
}

fn print_check_help() {
    println!(
        "\
Usage: zorg check [--root PATH] [--db PATH] FILE...

Runs strict syntax, semantic, and resolution validation across the supplied
files or, with --root, the corpus discovered under PATH. Exits nonzero on any
strict diagnostic listed in docs/fix.md."
    );
}

fn print_fix_help() {
    println!(
        "\
Usage: zorg fix [--check] [--root PATH] [--db PATH] FILE...

Without --check, applies safe autofixes in place after validating the rewritten
sources. With --check, reports strict diagnostics and pending autofixes without
writing. Exits zero when there are no strict diagnostics and no pending fixes;
otherwise exits nonzero with one parseable line per finding."
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

fn print_query_help() {
    println!(
        "\
Usage: zorg query '<swog>' [--root PATH] [--db PATH]
       zorg query --id @some/query [--root PATH] [--db PATH]

Runs an inline SWOG LIST query, or a query::/swog definition stored in an
ordinary #z/query zettel, against an existing, current SQLite index.
Run `zorg db reindex` first after adding or changing source files."
    );
}
