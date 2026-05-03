use std::env;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use zorg_capture::{CaptureRequest, CaptureResult, CaptureTemplate};
use zorg_core::{
    BodyBlock, Diagnostic, DiagnosticCategory, Severity, SourcePath, SourceSpan, Zettel,
};
use zorg_fix::{ApplySummary, CorpusView, FixOp, FixPlan, apply_plan_to_source, plan_fixes};
use zorg_query::{QueryContext, QueryDate};
use zorg_store::{ConfigOverrides, ResolvedConfig, Store, StoreOptions, discover_corpus_sources};
use zorg_watch::{
    RunControl, WatchEventSink, WatchOptions, WatchRunResult, WatchState, WatchStateKind,
    run_watch_service,
};

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
        Some("watch") => run_watch_cli(args.collect()),
        Some("dash") => run_dash_cli(args.collect()),
        Some("query") => run_query(args.collect()),
        Some("path") => run_path_cli("path", args.collect()),
        Some("open") => run_path_cli("open", args.collect()),
        Some("promote") => run_promote_cli(args.collect()),
        Some("move") => run_move_cli(args.collect()),
        Some("extract") => run_extract_cli(args.collect()),
        Some("import") => run_import_cli(args.collect()),
        Some("export") => run_export_cli(args.collect()),
        Some("index") => {
            eprintln!(
                "`zorg index` is deferred; use `zorg db reindex` for the database command path"
            );
            std::process::exit(2);
        }
        Some("fix") => run_fix_cli(args.collect()),
        Some("capture") => run_capture_cli(args.collect()),
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
        if options.json {
            print_fix_json("check", &documents, &plans, None, &diagnostics);
        }

        if has_strict_errors {
            for diagnostic in &diagnostics {
                if !options.json {
                    print_diagnostic(diagnostic);
                }
            }
        }

        if pending_fixes > 0 && !options.json {
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
        if options.json {
            print_fix_json("write", &documents, &plans, None, &diagnostics);
        }
        if has_strict_errors {
            if !options.json {
                for diagnostic in &diagnostics {
                    print_diagnostic(diagnostic);
                }
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
        if options.json {
            print_fix_json(
                "write",
                &documents,
                &plans,
                Some(&rewritten),
                &rewritten_diagnostics,
            );
        } else {
            for diagnostic in &rewritten_diagnostics {
                print_diagnostic(diagnostic);
            }
            eprintln!(
                "zorg fix refused to write because rewritten sources failed strict validation"
            );
        }
        std::process::exit(1);
    }

    write_rewritten_documents(&documents, &rewritten);
    if options.json {
        print_fix_json(
            "write",
            &documents,
            &plans,
            Some(&rewritten),
            &rewritten_diagnostics,
        );
    }
}

fn run_capture_cli(args: Vec<String>) {
    let (request, output) = parse_capture_options(&args);
    match zorg_capture::capture(&request) {
        Ok(result) => print_capture_result(&result, output),
        Err(error) => exit_capture_error(error.to_string(), "capture.failed", output, 1),
    }
}

fn run_dash_cli(args: Vec<String>) {
    #[cfg(feature = "dash")]
    {
        std::process::exit(zorg_dash::run(args));
    }

    #[cfg(not(feature = "dash"))]
    {
        let _ = args;
        eprintln!("zorg dash: not built with the `dash` feature");
        std::process::exit(2);
    }
}

#[derive(Debug, Default)]
struct CaptureCliOptions {
    template: Option<String>,
    title: Option<String>,
    source: Option<String>,
    body: Option<String>,
    dest: Option<PathBuf>,
    id: Option<String>,
    allow_outside: bool,
    json: bool,
    store_args: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum CaptureOutput {
    Text,
    Json,
}

fn parse_capture_options(args: &[String]) -> (CaptureRequest, CaptureOutput) {
    let mut options = CaptureCliOptions::default();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                print_capture_help();
                std::process::exit(0);
            }
            "--template" | "--title" | "--source" | "--body" | "--dest" | "--id" | "--root"
            | "--db" | "--format" => {
                let flag = args[index].clone();
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for {flag}");
                    std::process::exit(2);
                };
                match flag.as_str() {
                    "--template" => set_once(&mut options.template, value.clone(), "--template"),
                    "--title" => set_once(&mut options.title, value.clone(), "--title"),
                    "--source" => set_once(&mut options.source, value.clone(), "--source"),
                    "--body" => set_once(&mut options.body, value.clone(), "--body"),
                    "--dest" => set_once(&mut options.dest, PathBuf::from(value), "--dest"),
                    "--id" => set_once(&mut options.id, value.clone(), "--id"),
                    "--root" | "--db" => {
                        options.store_args.push(flag);
                        options.store_args.push(value.clone());
                    }
                    "--format" if value == "json" => options.json = true,
                    "--format" => {
                        eprintln!("zorg capture only supports --format json");
                        std::process::exit(2);
                    }
                    _ => unreachable!("matched capture flag"),
                }
            }
            "--allow-outside" => options.allow_outside = true,
            "--json" => options.json = true,
            argument if argument.starts_with('-') => {
                eprintln!("unexpected argument for `zorg capture`: {argument}");
                std::process::exit(2);
            }
            argument => {
                eprintln!("unexpected positional argument for `zorg capture`: {argument}");
                std::process::exit(2);
            }
        }
        index += 1;
    }

    let output = if options.json {
        CaptureOutput::Json
    } else {
        CaptureOutput::Text
    };
    let store_options = parse_store_options(&options.store_args);
    let root = store_options.corpus_root().to_path_buf();
    if options.template.is_none() {
        if io::stdin().is_terminal() && io::stdout().is_terminal() {
            options.template = Some(prompt_template_selection(&root, output));
        } else {
            exit_capture_error(
                "missing inputs: --template (interactive template selection requires a TTY)",
                "capture.missing_inputs",
                output,
                2,
            );
        }
    }
    fill_interactive_capture_values(&root, &mut options, output);

    let request = CaptureRequest {
        root: store_options.corpus_root().to_path_buf(),
        template: options.template.expect("template is set"),
        title: options.title,
        source: options.source,
        body: options.body,
        dest: options.dest,
        id: options.id,
        allow_outside: options.allow_outside,
    };
    (request, output)
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) {
    if slot.replace(value).is_some() {
        eprintln!("zorg capture accepts at most one {flag} value");
        std::process::exit(2);
    }
}

fn prompt_template_selection(root: &Path, output: CaptureOutput) -> String {
    let templates = zorg_capture::list_templates(root).unwrap_or_else(|error| {
        exit_capture_error(
            error.to_string(),
            "capture.template_discovery_failed",
            output,
            1,
        );
    });
    if templates.is_empty() {
        exit_capture_error(
            "missing inputs: --template (no #z/tmpl templates were found)",
            "capture.missing_inputs",
            output,
            2,
        );
    }

    eprintln!("Select capture template:");
    for (index, template) in templates.iter().enumerate() {
        eprintln!("  {}. {}", index + 1, template_label(template));
    }
    eprint!("Template number: ");
    io::stderr().flush().expect("flush stderr");

    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).unwrap_or(0) == 0 {
        exit_capture_error(
            "capture aborted before template selection",
            "capture.aborted",
            output,
            1,
        );
    }
    let choice = answer.trim().parse::<usize>().ok();
    let Some(template) = choice
        .and_then(|choice| choice.checked_sub(1))
        .and_then(|index| templates.get(index))
    else {
        exit_capture_error(
            "capture template selection is invalid",
            "capture.invalid_selection",
            output,
            2,
        );
    };
    template_selector(template)
}

fn fill_interactive_capture_values(
    root: &Path,
    options: &mut CaptureCliOptions,
    output: CaptureOutput,
) {
    if !(io::stdin().is_terminal() && io::stdout().is_terminal()) {
        return;
    }
    let Some(template) = options.template.as_deref() else {
        return;
    };
    let metadata = zorg_capture::inspect_template(root, template).unwrap_or_else(|error| {
        exit_capture_error(
            error.to_string(),
            "capture.template_discovery_failed",
            output,
            1,
        );
    });

    if metadata.variables.iter().any(|name| name == "title") && options.title.is_none() {
        options.title = Some(prompt_capture_value("Title", output));
    }
    if metadata.variables.iter().any(|name| name == "source") && options.source.is_none() {
        options.source = Some(prompt_capture_value("Source", output));
    }
    if metadata.variables.iter().any(|name| name == "body") && options.body.is_none() {
        options.body = Some(prompt_capture_value("Body", output));
    }
}

fn prompt_capture_value(label: &str, output: CaptureOutput) -> String {
    eprint!("{label}: ");
    io::stderr().flush().expect("flush stderr");
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).unwrap_or(0) == 0 {
        exit_capture_error(
            "capture aborted while reading input",
            "capture.aborted",
            output,
            1,
        );
    }
    answer.trim_end_matches(['\r', '\n']).to_owned()
}

fn template_label(template: &CaptureTemplate) -> String {
    match (&template.id, &template.title) {
        (Some(id), Some(title)) => format!("{id} - {title}"),
        (Some(id), None) => id.to_string(),
        (None, Some(title)) => title.clone(),
        (None, None) => template
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<untitled template>".to_owned()),
    }
}

fn template_selector(template: &CaptureTemplate) -> String {
    template
        .id
        .as_ref()
        .map(zorg_core::ZettelId::declaration)
        .or_else(|| template.title.clone())
        .unwrap_or_else(|| {
            eprintln!("selected template has no ID or title and cannot be selected");
            std::process::exit(2);
        })
}

fn print_capture_result(result: &CaptureResult, output: CaptureOutput) {
    match output {
        CaptureOutput::Text => {
            println!("destination: {}", result.destination.display());
            println!("zettel_id: {}", result.zettel_id);
        }
        CaptureOutput::Json => println!(
            "{}",
            json!({
                "destination": result.destination.display().to_string(),
                "zettel_id": result.zettel_id.declaration(),
            })
        ),
    }
}

fn exit_capture_error(
    message: impl Into<String>,
    code: &'static str,
    output: CaptureOutput,
    status: i32,
) -> ! {
    let message = message.into();
    match output {
        CaptureOutput::Text => eprintln!("{message}"),
        CaptureOutput::Json => println!("{}", json!({ "error": message, "code": code })),
    }
    std::process::exit(status);
}

#[derive(Debug, Default)]
struct FixCliOptions {
    check: bool,
    json: bool,
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
            "--json" => options.json = true,
            "--format" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for --format");
                    std::process::exit(2);
                };
                if value == "json" {
                    options.json = true;
                } else {
                    eprintln!("zorg fix only supports --format json");
                    std::process::exit(2);
                }
            }
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
    diagnostics.extend(validate_definition_diagnostics(documents));
    diagnostics
}

fn validate_definition_diagnostics(documents: &[zorg_core::ZettelDocument]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for document in documents {
        validate_zettel_definitions(document, &document.root, &mut diagnostics);
    }
    diagnostics
}

fn validate_zettel_definitions(
    document: &zorg_core::ZettelDocument,
    zettel: &Zettel,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if has_type_tag(zettel, "z/query") {
        validate_query_definition(document, zettel, diagnostics);
    }
    if has_type_tag(zettel, "z/tmpl") {
        validate_template_definition(document, zettel, diagnostics);
    }

    for block in &zettel.body {
        if let BodyBlock::ChildZettel(child) = block {
            validate_zettel_definitions(document, child, diagnostics);
        }
    }
}

fn validate_query_definition(
    document: &zorg_core::ZettelDocument,
    zettel: &Zettel,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let query_properties = zettel
        .properties
        .iter()
        .filter(|property| property.key == "query")
        .collect::<Vec<_>>();
    let swog_blocks = direct_fenced_blocks(zettel, "swog");
    let id = zettel_label(zettel);

    match (query_properties.len(), swog_blocks.len()) {
        (0, 0) => diagnostics.push(definition_diagnostic(
            "query.definition",
            format!("{id} has no query:: property or fenced swog block"),
            zettel.span,
            document.path.as_ref(),
        )),
        (properties, _) if properties > 1 => diagnostics.push(definition_diagnostic(
            "query.definition",
            format!("{id} has multiple query:: properties"),
            query_properties
                .get(1)
                .and_then(|property| property.span)
                .or(zettel.span),
            document.path.as_ref(),
        )),
        (_, blocks) if blocks > 1 => diagnostics.push(definition_diagnostic(
            "query.definition",
            format!("{id} has multiple fenced swog blocks"),
            swog_blocks
                .get(1)
                .and_then(|block| block.span)
                .or(zettel.span),
            document.path.as_ref(),
        )),
        (1, 1) => diagnostics.push(definition_diagnostic(
            "query.definition",
            format!("{id} has both query:: and fenced swog definitions"),
            zettel.span,
            document.path.as_ref(),
        )),
        (1, 0) => {
            let property = query_properties[0];
            validate_swog_query_text(
                &property.value,
                property.value_span.or(property.span).or(zettel.span),
                document.path.as_ref(),
                diagnostics,
            );
        }
        (0, 1) => {
            let block = swog_blocks[0];
            validate_swog_query_text(
                &block.body,
                block.body_span.or(block.span).or(zettel.span),
                document.path.as_ref(),
                diagnostics,
            );
        }
        _ => {}
    }
}

fn validate_swog_query_text(
    query: &str,
    span: Option<SourceSpan>,
    path: Option<&SourcePath>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Err(error) = zorg_query::parse_output_query(query.trim()) {
        diagnostics.push(definition_diagnostic(
            "query.definition",
            format!("invalid query definition: {error}"),
            span,
            path,
        ));
    }
}

fn validate_template_definition(
    document: &zorg_core::ZettelDocument,
    zettel: &Zettel,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let template_blocks = direct_fenced_blocks(zettel, "zorg-template");
    if template_blocks.len() > 1 {
        diagnostics.push(definition_diagnostic(
            "template.definition",
            format!("{} has multiple zorg-template fences", zettel_label(zettel)),
            template_blocks
                .get(1)
                .and_then(|block| block.span)
                .or(zettel.span),
            document.path.as_ref(),
        ));
        return;
    }

    let (template_text, span) = if let Some(block) = template_blocks.first() {
        (
            block.body.clone(),
            block.body_span.or(block.span).or(zettel.span),
        )
    } else {
        (template_fallback_text(zettel), zettel.span)
    };
    validate_template_variables(&template_text, span, document.path.as_ref(), diagnostics);
}

fn template_fallback_text(zettel: &Zettel) -> String {
    zettel
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

fn validate_template_variables(
    text: &str,
    span: Option<SourceSpan>,
    path: Option<&SourcePath>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes.get(index..index + 4) == Some(b"{{{{")
            || bytes.get(index..index + 4) == Some(b"}}}}")
        {
            index += 4;
        } else if bytes.get(index..index + 2) == Some(b"{{") {
            let Some(end) = text[index + 2..].find("}}") else {
                diagnostics.push(definition_diagnostic(
                    "template.definition",
                    "template has an unclosed variable",
                    span,
                    path,
                ));
                return;
            };
            let name = &text[index + 2..index + 2 + end];
            if !matches!(name, "id" | "title" | "date" | "source" | "body") {
                diagnostics.push(definition_diagnostic(
                    "template.definition",
                    format!("template variable `{name}` is not defined"),
                    span,
                    path,
                ));
            }
            index += 2 + end + 2;
        } else {
            let character = text[index..].chars().next().expect("valid char boundary");
            index += character.len_utf8();
        }
    }
}

fn direct_fenced_blocks<'a>(zettel: &'a Zettel, info: &str) -> Vec<&'a zorg_core::FencedCodeBlock> {
    zettel
        .body
        .iter()
        .filter_map(|block| match block {
            BodyBlock::FencedCode(block) if block.info.as_deref() == Some(info) => Some(block),
            _ => None,
        })
        .collect()
}

fn has_type_tag(zettel: &Zettel, tag: &str) -> bool {
    zettel
        .type_tags
        .iter()
        .any(|candidate| candidate.tag.as_str() == tag)
}

fn zettel_label(zettel: &Zettel) -> String {
    zettel
        .canonical_id
        .as_ref()
        .or(zettel.id.as_ref())
        .map(zorg_core::ZettelId::declaration)
        .unwrap_or_else(|| "query zettel".to_owned())
}

fn definition_diagnostic(
    code: &str,
    message: impl Into<String>,
    span: Option<SourceSpan>,
    path: Option<&SourcePath>,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::semantic_validation(code, message, span);
    if let Some(path) = path {
        diagnostic.path = Some(path.clone());
    }
    diagnostic
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
) -> Vec<ApplySummary> {
    documents
        .iter()
        .zip(plans)
        .map(|(document, plan)| {
            apply_plan_to_source(&document.source, plan).unwrap_or_else(|error| {
                let path = document
                    .path
                    .as_ref()
                    .map(|path| path.as_path().display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_owned());
                eprintln!("failed to apply fixes for {path}: {error}");
                std::process::exit(1);
            })
        })
        .collect()
}

fn reparse_rewritten_documents(
    documents: &[zorg_core::ZettelDocument],
    rewritten: &[ApplySummary],
) -> Vec<zorg_core::ZettelDocument> {
    documents
        .iter()
        .zip(rewritten)
        .map(|(document, summary)| {
            let path = document.path.as_ref().unwrap_or_else(|| {
                eprintln!("cannot rewrite a document without a source path");
                std::process::exit(1);
            });
            zorg_parse::parse_zettel_document_with_path(&summary.source, path.as_path())
                .unwrap_or_else(|error| {
                    eprintln!(
                        "failed to parse rewritten {}: {error}",
                        path.as_path().display()
                    );
                    std::process::exit(1);
                })
        })
        .collect()
}

fn write_rewritten_documents(documents: &[zorg_core::ZettelDocument], rewritten: &[ApplySummary]) {
    for (index, (document, summary)) in documents.iter().zip(rewritten).enumerate() {
        if document.source == summary.source {
            continue;
        }
        let path = document.path.as_ref().unwrap_or_else(|| {
            eprintln!("cannot rewrite a document without a source path");
            std::process::exit(1);
        });
        atomic_write(path.as_path(), &summary.source, index);
    }
}

fn print_fix_json(
    mode: &str,
    documents: &[zorg_core::ZettelDocument],
    plans: &[FixPlan],
    rewritten: Option<&[ApplySummary]>,
    diagnostics: &[Diagnostic],
) {
    let files = documents
        .iter()
        .zip(plans)
        .enumerate()
        .map(|(index, (document, plan))| {
            let summary = rewritten.and_then(|items| items.get(index));
            json!({
                "path": document.path.as_ref().map(|path| path.as_path().display().to_string()).unwrap_or_else(|| "<unknown>".to_owned()),
                "planned_fixes": plan.ops.len(),
                "applied_edits": summary.map(|summary| summary.applied_edits).unwrap_or(0),
                "changed": summary.is_some_and(|summary| summary.source != document.source),
                "fixes": plan.ops.iter().map(json_fix_op).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();

    println!(
        "{}",
        json!({
            "schema_version": 1,
            "mode": mode,
            "files": files,
            "diagnostics": diagnostics.iter().map(json_diagnostic).collect::<Vec<_>>(),
        })
    );
}

fn json_fix_op(op: &FixOp) -> serde_json::Value {
    let (line, column) = primary_position(op.primary_span());
    json!({
        "code": op.rule_code,
        "message": op.message,
        "line": line,
        "column": column,
        "preferred": op.is_preferred,
    })
}

fn json_diagnostic(diagnostic: &Diagnostic) -> serde_json::Value {
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
    json!({
        "path": path,
        "line": line,
        "column": column,
        "code": diagnostic.code.as_deref().unwrap_or("diagnostic"),
        "message": diagnostic.message,
        "severity": format!("{:?}", diagnostic.severity),
    })
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

    ResolvedConfig::from_env(ConfigOverrides {
        root,
        database_path: db,
    })
    .map(ResolvedConfig::into_store_options)
    .unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    })
}

fn run_query(args: Vec<String>) {
    let (query, output, options) = parse_query_options(&args);
    let store = open_store(options);
    ensure_query_index_ready(&store);
    let context = query_context_for_store(&store);

    let output = match (query, output) {
        (CliQuery::Inline(query), QueryOutput::List) => {
            zorg_query::execute_and_render_query(&store, &context, &query).map(QueryCliOutput::Text)
        }
        (CliQuery::Id(query_id), QueryOutput::List) => {
            zorg_query::execute_and_render_query_by_id(&store, &context, &query_id)
                .map(QueryCliOutput::Text)
        }
        (CliQuery::Inline(query), QueryOutput::Json) => {
            zorg_query::execute_query(&store, &context, &query).map(|result| {
                QueryCliOutput::Json(query_json_envelope(
                    result.kind,
                    "inline",
                    Some(&query),
                    None,
                    &result.rows,
                    store.root(),
                ))
            })
        }
        (CliQuery::Id(query_id), QueryOutput::Json) => {
            let definition =
                zorg_query::query_definition_by_id(&store, &query_id).and_then(|definition| {
                    zorg_query::execute_query(&store, &context, &definition.query)
                        .map(|result| (definition, result))
                });
            definition.map(|(definition, result)| {
                QueryCliOutput::Json(query_json_envelope(
                    result.kind,
                    "zettel",
                    Some(&definition.query),
                    Some(&definition),
                    &result.rows,
                    store.root(),
                ))
            })
        }
    }
    .unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    match output {
        QueryCliOutput::Text(output) => {
            if !output.is_empty() {
                println!("{output}");
            }
        }
        QueryCliOutput::Json(output) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&output).expect("query JSON serializes")
            );
        }
    }
}

fn run_export_cli(args: Vec<String>) {
    let options = parse_export_markdown_options(&args);
    let store = open_store(options.store_options);
    ensure_query_index_ready(&store);
    let documents = load_export_documents(&store);
    let (target, render_options, selection) = export_markdown_target(&store, &options.selector);
    let plan = zorg_bridge::plan_markdown_export(&documents, target, &render_options);

    if plan.summary.fatal > 0 {
        if options.json {
            println!(
                "{}",
                export_markdown_json(&plan, &selection, &options.output, &[])
            );
        } else {
            print_export_markdown_diagnostics(&plan);
        }
        std::process::exit(1);
    }

    if plan.items.is_empty() {
        eprintln!("export markdown selected no zettels");
        std::process::exit(1);
    }

    let outputs = match &options.output {
        ExportMarkdownOutput::Stdout => {
            if options.json {
                Vec::new()
            } else {
                print_export_markdown_stdout(&plan);
                Vec::new()
            }
        }
        ExportMarkdownOutput::Directory(directory) => write_export_markdown_items(&plan, directory),
    };

    if options.json {
        println!(
            "{}",
            export_markdown_json(&plan, &selection, &options.output, &outputs)
        );
    } else {
        print_export_markdown_diagnostics(&plan);
        if matches!(options.output, ExportMarkdownOutput::Directory(_)) {
            for output in outputs {
                println!(
                    "export: @{} -> {}",
                    output.canonical_id,
                    output.path.display()
                );
            }
        }
    }
}

fn parse_export_markdown_options(args: &[String]) -> ExportMarkdownOptions {
    let Some(first) = args.first() else {
        print_export_help();
        std::process::exit(2);
    };
    if first == "-h" || first == "--help" {
        print_export_help();
        std::process::exit(0);
    }
    if first != "markdown" {
        eprintln!("unsupported export target `{first}`; expected `markdown`");
        std::process::exit(2);
    }

    let mut selector = None;
    let mut output = None;
    let mut json_output = false;
    let mut store_args = Vec::new();
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                print_export_markdown_help();
                std::process::exit(0);
            }
            "--id" | "--subtree" | "--query" | "--query-id" | "--root" | "--db" | "--out"
            | "--format" => {
                let flag = args[index].clone();
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for {flag}");
                    std::process::exit(2);
                };
                match flag.as_str() {
                    "--id" => set_export_selector(
                        &mut selector,
                        ExportMarkdownSelector::Id(value.clone()),
                        "--id",
                    ),
                    "--subtree" => set_export_selector(
                        &mut selector,
                        ExportMarkdownSelector::Subtree(value.clone()),
                        "--subtree",
                    ),
                    "--query" => set_export_selector(
                        &mut selector,
                        ExportMarkdownSelector::Query(value.clone()),
                        "--query",
                    ),
                    "--query-id" => set_export_selector(
                        &mut selector,
                        ExportMarkdownSelector::QueryId(value.clone()),
                        "--query-id",
                    ),
                    "--root" | "--db" => {
                        store_args.push(flag);
                        store_args.push(value.clone());
                    }
                    "--out" => set_once(&mut output, PathBuf::from(value), "--out"),
                    "--format" => match value.as_str() {
                        "json" => json_output = true,
                        "text" | "markdown" => {}
                        other => {
                            eprintln!(
                                "unsupported export markdown output format `{other}`; expected json or text"
                            );
                            std::process::exit(2);
                        }
                    },
                    _ => unreachable!("matched export flag"),
                }
            }
            "--stdout" => {
                if output.replace(PathBuf::new()).is_some() {
                    eprintln!("zorg export markdown accepts either --out or --stdout, not both");
                    std::process::exit(2);
                }
            }
            "--json" => json_output = true,
            argument if argument.starts_with('-') => {
                eprintln!("unexpected argument for `zorg export markdown`: {argument}");
                std::process::exit(2);
            }
            argument => {
                eprintln!("unexpected positional argument for `zorg export markdown`: {argument}");
                std::process::exit(2);
            }
        }
        index += 1;
    }

    let Some(selector) = selector else {
        print_export_markdown_usage();
        std::process::exit(2);
    };
    let output = match output {
        Some(path) if path.as_os_str().is_empty() => ExportMarkdownOutput::Stdout,
        Some(path) => ExportMarkdownOutput::Directory(path),
        None => ExportMarkdownOutput::Stdout,
    };

    ExportMarkdownOptions {
        selector,
        output,
        json: json_output,
        store_options: parse_store_options(&store_args),
    }
}

fn set_export_selector(
    slot: &mut Option<ExportMarkdownSelector>,
    value: ExportMarkdownSelector,
    flag: &str,
) {
    if slot.replace(value).is_some() {
        eprintln!("zorg export markdown accepts exactly one selector; duplicate {flag}");
        std::process::exit(2);
    }
}

fn load_export_documents(store: &Store) -> Vec<zorg_core::ZettelDocument> {
    let sources = discover_corpus_sources(store.root()).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });
    let mut documents = sources
        .into_iter()
        .map(|source| {
            let path = source.absolute_path().to_path_buf();
            let text = fs::read_to_string(&path).unwrap_or_else(|error| {
                eprintln!("failed to read {}: {error}", path.display());
                std::process::exit(1);
            });
            zorg_parse::parse_zettel_document_with_path(&text, path.clone()).unwrap_or_else(
                |error| {
                    eprintln!("failed to parse {}: {error}", path.display());
                    std::process::exit(1);
                },
            )
        })
        .collect::<Vec<_>>();
    let _ = zorg_parse::validate_corpus(&documents);
    let _ = zorg_parse::resolve_corpus(&mut documents);
    documents
}

fn export_markdown_target(
    store: &Store,
    selector: &ExportMarkdownSelector,
) -> (
    zorg_bridge::ExportTarget,
    zorg_bridge::MarkdownRenderOptions,
    serde_json::Value,
) {
    let mut render_options = zorg_bridge::MarkdownRenderOptions::new();
    match selector {
        ExportMarkdownSelector::Id(id) => {
            render_options.render_children = false;
            let canonical_id = normalize_export_id(id, "--id");
            (
                zorg_bridge::ExportTarget::Single {
                    canonical_id: canonical_id.clone(),
                },
                render_options,
                json!({"kind": "id", "canonical_id": canonical_id}),
            )
        }
        ExportMarkdownSelector::Subtree(id) => {
            let canonical_id = normalize_export_id(id, "--subtree");
            (
                zorg_bridge::ExportTarget::Subtree {
                    canonical_id: canonical_id.clone(),
                },
                render_options,
                json!({"kind": "subtree", "canonical_id": canonical_id}),
            )
        }
        ExportMarkdownSelector::Query(query) => {
            render_options.render_children = false;
            let rows = execute_export_query(store, query);
            let canonical_ids = export_query_ids(&rows);
            (
                zorg_bridge::ExportTarget::Query {
                    label: query.clone(),
                    canonical_ids: canonical_ids.clone(),
                },
                render_options,
                json!({"kind": "query", "query": query, "canonical_ids": canonical_ids}),
            )
        }
        ExportMarkdownSelector::QueryId(query_id) => {
            render_options.render_children = false;
            let canonical_query_id = normalize_export_id(query_id, "--query-id");
            let context = query_context_for_store(store);
            let definition =
                zorg_query::query_definition_by_id(store, query_id).unwrap_or_else(|error| {
                    eprintln!("{error}");
                    std::process::exit(1);
                });
            let result = zorg_query::execute_query(store, &context, &definition.query)
                .unwrap_or_else(|error| {
                    eprintln!("{error}");
                    std::process::exit(1);
                });
            reject_aggregate_export(result.kind);
            let canonical_ids = export_query_ids(&result.rows);
            (
                zorg_bridge::ExportTarget::Query {
                    label: format!("@{canonical_query_id}"),
                    canonical_ids: canonical_ids.clone(),
                },
                render_options,
                json!({
                    "kind": "query_id",
                    "canonical_id": canonical_query_id,
                    "query": definition.query,
                    "canonical_ids": canonical_ids,
                }),
            )
        }
    }
}

fn normalize_export_id(input: &str, flag: &str) -> String {
    let id = input.trim().trim_start_matches('@');
    if id.is_empty() || id.contains(char::is_whitespace) {
        eprintln!("zorg export markdown {flag} expects a canonical zettel ID such as @foo/bar");
        std::process::exit(2);
    }
    id.to_owned()
}

fn execute_export_query(store: &Store, query: &str) -> Vec<zorg_query::QueryResultRow> {
    let context = query_context_for_store(store);
    let result = zorg_query::execute_query(store, &context, query).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });
    reject_aggregate_export(result.kind);
    result.rows
}

fn reject_aggregate_export(kind: zorg_query::QueryResultKind) {
    if kind == zorg_query::QueryResultKind::Aggregate {
        eprintln!("zorg export markdown requires a LIST or TABLE query, not count()");
        std::process::exit(1);
    }
}

fn export_query_ids(rows: &[zorg_query::QueryResultRow]) -> Vec<String> {
    let mut ids = Vec::new();
    for row in rows {
        let Some(id) = &row.canonical_id else {
            eprintln!(
                "query result row at {} has no canonical ID",
                row.file_path.display()
            );
            std::process::exit(1);
        };
        ids.push(id.clone());
    }
    ids
}

fn print_export_markdown_stdout(plan: &zorg_bridge::ExportPlan) {
    for (index, item) in plan.items.iter().enumerate() {
        if index > 0 {
            println!("\n---\n");
        }
        print!("{}", item.markdown);
        if !item.markdown.ends_with('\n') {
            println!();
        }
    }
}

#[derive(Debug)]
struct ExportWrite {
    canonical_id: String,
    path: PathBuf,
}

fn write_export_markdown_items(
    plan: &zorg_bridge::ExportPlan,
    directory: &Path,
) -> Vec<ExportWrite> {
    let mut seen = std::collections::BTreeSet::new();
    let mut writes = Vec::new();
    for item in &plan.items {
        let relative_path = markdown_output_relative_path(&item.canonical_id);
        if !seen.insert(relative_path.clone()) {
            eprintln!(
                "export markdown output path collision for {}",
                relative_path.display()
            );
            std::process::exit(1);
        }
        let path = directory.join(&relative_path);
        if path.exists() {
            eprintln!("export markdown refuses to overwrite {}", path.display());
            std::process::exit(1);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| {
                eprintln!("failed to create {}: {error}", parent.display());
                std::process::exit(1);
            });
        }
        fs::write(&path, ensure_trailing_newline(&item.markdown)).unwrap_or_else(|error| {
            eprintln!("failed to write {}: {error}", path.display());
            std::process::exit(1);
        });
        writes.push(ExportWrite {
            canonical_id: item.canonical_id.clone(),
            path,
        });
    }
    writes
}

fn markdown_output_relative_path(canonical_id: &str) -> PathBuf {
    let mut path = PathBuf::new();
    for segment in canonical_id.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." || segment.contains('\\') {
            eprintln!("cannot derive Markdown output path from @{canonical_id}");
            std::process::exit(1);
        }
        path.push(segment);
    }
    path.set_extension("md");
    path
}

fn ensure_trailing_newline(markdown: &str) -> String {
    if markdown.ends_with('\n') {
        markdown.to_owned()
    } else {
        format!("{markdown}\n")
    }
}

fn export_markdown_json(
    plan: &zorg_bridge::ExportPlan,
    selection: &serde_json::Value,
    output: &ExportMarkdownOutput,
    writes: &[ExportWrite],
) -> String {
    let output = match output {
        ExportMarkdownOutput::Stdout => json!({
            "mode": "stdout",
            "items": plan.items.len(),
        }),
        ExportMarkdownOutput::Directory(directory) => json!({
            "mode": "directory",
            "directory": path_json(directory),
            "written": writes.iter().map(|write| {
                json!({
                    "canonical_id": &write.canonical_id,
                    "path": path_json(&write.path),
                })
            }).collect::<Vec<_>>(),
        }),
    };
    let value = json!({
        "schema_version": 1,
        "command": "export markdown",
        "selection": selection,
        "output": output,
        "items": plan.items.iter().map(|item| {
            json!({
                "canonical_id": &item.canonical_id,
                "source_path": &item.source_path,
                "title": &item.title,
            })
        }).collect::<Vec<_>>(),
        "diagnostics": &plan.diagnostics,
        "summary": &plan.summary,
    });
    serde_json::to_string_pretty(&value).expect("export markdown JSON serializes")
}

fn print_export_markdown_diagnostics(plan: &zorg_bridge::ExportPlan) {
    for diagnostic in &plan.diagnostics {
        let label = import_severity_label(diagnostic.severity);
        eprintln!("{label}: {diagnostic}");
    }
}

enum CliQuery {
    Inline(String),
    Id(String),
}

enum QueryOutput {
    List,
    Json,
}

#[derive(Debug)]
enum ExportMarkdownSelector {
    Id(String),
    Subtree(String),
    Query(String),
    QueryId(String),
}

#[derive(Debug)]
struct ExportMarkdownOptions {
    selector: ExportMarkdownSelector,
    output: ExportMarkdownOutput,
    json: bool,
    store_options: StoreOptions,
}

#[derive(Debug)]
enum ExportMarkdownOutput {
    Stdout,
    Directory(PathBuf),
}

enum QueryCliOutput {
    Text(String),
    Json(serde_json::Value),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum PathOutput {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum RefactorOutput {
    Text,
    Json,
}

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
enum ImportOutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Default)]
struct ImportLegacyPlanOptions {
    paths: Vec<PathBuf>,
    root: Option<PathBuf>,
    dest: Option<PathBuf>,
    output: ImportOutputFormat,
    mode: ImportLegacyMode,
}

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq)]
enum ImportLegacyMode {
    #[default]
    Plan,
    Apply,
}

fn run_import_cli(args: Vec<String>) {
    let (options, output) = parse_import_legacy_plan_options(&args);
    let root = match options.mode {
        ImportLegacyMode::Plan => options.root.clone(),
        ImportLegacyMode::Apply => Some(options.root.clone().unwrap_or_else(apply_default_root)),
    };
    let bridge_options = zorg_bridge::LegacyImportOptions {
        root,
        dest: options.dest,
    };
    let plan = zorg_bridge::plan_legacy_import(&options.paths, &bridge_options);

    match options.mode {
        ImportLegacyMode::Plan => {
            match output {
                ImportOutputFormat::Text => print_import_legacy_plan_text(&plan),
                ImportOutputFormat::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&plan).expect("import plan JSON serializes")
                ),
            }

            if plan.summary.fatal > 0 {
                std::process::exit(1);
            }
        }
        ImportLegacyMode::Apply => {
            let root = bridge_options
                .root
                .clone()
                .expect("apply always sets an explicit root");
            let report =
                zorg_bridge::apply_legacy_import(&plan, &zorg_bridge::LegacyApplyOptions { root });
            match output {
                ImportOutputFormat::Text => print_import_legacy_apply_text(&report),
                ImportOutputFormat::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&report).expect("import apply JSON serializes")
                ),
            }

            if report.plan.summary.fatal > 0
                || report
                    .write_results
                    .iter()
                    .any(|result| result.status == zorg_bridge::ImportWriteStatus::Failed)
            {
                std::process::exit(1);
            }
        }
    }
}

fn apply_default_root() -> PathBuf {
    env::current_dir().unwrap_or_else(|error| {
        eprintln!("failed to resolve current directory for import apply: {error}");
        std::process::exit(2);
    })
}

fn parse_import_legacy_plan_options(
    args: &[String],
) -> (ImportLegacyPlanOptions, ImportOutputFormat) {
    let mut options = ImportLegacyPlanOptions {
        output: ImportOutputFormat::Text,
        ..ImportLegacyPlanOptions::default()
    };

    let Some(first) = args.first() else {
        print_import_legacy_plan_usage();
        std::process::exit(2);
    };
    if first == "-h" || first == "--help" {
        print_import_help();
        std::process::exit(0);
    }
    if first != "legacy" {
        eprintln!("unsupported import target `{first}`; expected `legacy`");
        std::process::exit(2);
    }

    let Some(second) = args.get(1) else {
        print_import_legacy_plan_usage();
        std::process::exit(2);
    };
    if second == "-h" || second == "--help" {
        print_import_legacy_help();
        std::process::exit(0);
    }
    match second.as_str() {
        "plan" => options.mode = ImportLegacyMode::Plan,
        "apply" => options.mode = ImportLegacyMode::Apply,
        _ => {
            eprintln!(
                "unsupported import legacy subcommand `{second}`; expected `plan` or `apply`"
            );
            std::process::exit(2);
        }
    }

    if options.mode == ImportLegacyMode::Apply {
        options.output = ImportOutputFormat::Text;
    }

    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                if options.mode == ImportLegacyMode::Apply {
                    print_import_legacy_apply_help();
                } else {
                    print_import_legacy_plan_help();
                }
                std::process::exit(0);
            }
            "--root" | "--dest" => {
                let flag = args[index].clone();
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for {flag}");
                    std::process::exit(2);
                };
                match flag.as_str() {
                    "--root" => set_once(&mut options.root, PathBuf::from(value), "--root"),
                    "--dest" => set_once(&mut options.dest, PathBuf::from(value), "--dest"),
                    _ => unreachable!("flag handled above"),
                }
            }
            "--json" => options.output = ImportOutputFormat::Json,
            "--format" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for --format");
                    std::process::exit(2);
                };
                match value.as_str() {
                    "json" => options.output = ImportOutputFormat::Json,
                    "text" => options.output = ImportOutputFormat::Text,
                    other => {
                        eprintln!(
                            "unsupported import output format `{other}`; expected json or text"
                        );
                        std::process::exit(2);
                    }
                }
            }
            argument if argument.starts_with('-') => {
                eprintln!("unexpected argument for `zorg import legacy {second}`: {argument}");
                std::process::exit(2);
            }
            argument => options.paths.push(PathBuf::from(argument)),
        }
        index += 1;
    }

    if options.paths.is_empty() {
        print_import_legacy_plan_usage();
        std::process::exit(2);
    }

    let output = options.output;
    (options, output)
}

fn print_import_legacy_plan_text(plan: &zorg_bridge::ImportPlan) {
    println!(
        "{}: planned={} lossy={} unsupported={} fatal={}",
        plan.command,
        plan.summary.planned,
        plan.summary.lossy,
        plan.summary.unsupported,
        plan.summary.fatal
    );

    for input in &plan.inputs {
        if let Some(output) = plan
            .outputs
            .iter()
            .find(|output| output.input_path == input.path)
        {
            println!(
                "{} -> {} {}",
                input.path, output.root_relative_path, output.status
            );
        } else {
            let status = if plan
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.path == input.path)
            {
                "diagnostic"
            } else {
                "skipped"
            };
            println!("{} -> - {status}", input.path);
        }
    }

    for diagnostic in &plan.diagnostics {
        println!(
            "{}: {}",
            import_severity_label(diagnostic.severity),
            diagnostic
        );
    }
}

fn print_import_legacy_apply_text(report: &zorg_bridge::ImportApplyReport) {
    print_import_legacy_plan_text(&report.plan);
    for result in &report.write_results {
        match result.status {
            zorg_bridge::ImportWriteStatus::Written => {
                println!("write: {} written", result.path);
            }
            zorg_bridge::ImportWriteStatus::Failed => {
                println!(
                    "write: {} failed: {}",
                    result.path,
                    result.error.as_deref().unwrap_or("write failed")
                );
            }
        }
    }
}

fn import_severity_label(severity: zorg_bridge::BridgeSeverity) -> &'static str {
    match severity {
        zorg_bridge::BridgeSeverity::Info => "info",
        zorg_bridge::BridgeSeverity::Warning => "warning",
        zorg_bridge::BridgeSeverity::Error => "error",
    }
}

fn run_promote_cli(args: Vec<String>) {
    let (request, output) = parse_promote_options(&args);
    let store = open_store(request.store_options.clone());
    ensure_query_index_ready(&store);
    let plan = zorg_refactor::plan_promote(&request).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    if request.mode == zorg_refactor::RefactorMode::Write {
        zorg_refactor::apply_refactor_plan(&plan).unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(1);
        });
    }

    match output {
        RefactorOutput::Text => print_refactor_plan_text(&plan),
        RefactorOutput::Json => println!(
            "{}",
            serde_json::to_string_pretty(&zorg_refactor::RefactorPreview::new(plan))
                .expect("refactor JSON serializes")
        ),
    }
}

fn run_move_cli(args: Vec<String>) {
    let (request, output) = parse_move_options(&args);
    let store = open_store(request.store_options.clone());
    ensure_query_index_ready(&store);
    let plan = zorg_refactor::plan_move(&request).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    if request.mode == zorg_refactor::RefactorMode::Write {
        zorg_refactor::apply_refactor_plan(&plan).unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(1);
        });
    }

    match output {
        RefactorOutput::Text => print_refactor_plan_text(&plan),
        RefactorOutput::Json => println!(
            "{}",
            serde_json::to_string_pretty(&zorg_refactor::RefactorPreview::new(plan))
                .expect("refactor JSON serializes")
        ),
    }
}

fn run_extract_cli(args: Vec<String>) {
    let (request, output) = parse_extract_options(&args);
    let store = open_store(request.store_options.clone());
    ensure_query_index_ready(&store);
    let plan = zorg_refactor::plan_extract(&request).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    if request.mode == zorg_refactor::RefactorMode::Write {
        zorg_refactor::apply_refactor_plan(&plan).unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(1);
        });
    }

    match output {
        RefactorOutput::Text => print_refactor_plan_text(&plan),
        RefactorOutput::Json => println!(
            "{}",
            serde_json::to_string_pretty(&zorg_refactor::RefactorPreview::new(plan))
                .expect("refactor JSON serializes")
        ),
    }
}

fn parse_promote_options(args: &[String]) -> (zorg_refactor::PromoteRequest, RefactorOutput) {
    let mut id = None;
    let mut mode = None;
    let mut destination = None;
    let mut output = RefactorOutput::Text;
    let mut store_args = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                print_promote_help();
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
            "--to" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for --to");
                    std::process::exit(2);
                };
                if destination.replace(PathBuf::from(value)).is_some() {
                    eprintln!("zorg promote accepts at most one --to value");
                    std::process::exit(2);
                }
            }
            "--check" => set_refactor_mode(&mut mode, zorg_refactor::RefactorMode::Check),
            "--write" => set_refactor_mode(&mut mode, zorg_refactor::RefactorMode::Write),
            "--json" => output = RefactorOutput::Json,
            "--format" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for --format");
                    std::process::exit(2);
                };
                match value.as_str() {
                    "json" => output = RefactorOutput::Json,
                    "text" => output = RefactorOutput::Text,
                    other => {
                        eprintln!(
                            "unsupported promote output format `{other}`; expected json or text"
                        );
                        std::process::exit(2);
                    }
                }
            }
            argument if argument.starts_with('-') => {
                eprintln!("unexpected argument for `zorg promote`: {argument}");
                std::process::exit(2);
            }
            argument => {
                if id.replace(argument.to_owned()).is_some() {
                    eprintln!("zorg promote accepts exactly one zettel ID");
                    std::process::exit(2);
                }
            }
        }
        index += 1;
    }

    let Some(id) = id else {
        eprintln!(
            "usage: zorg promote @id [--to PATH] [--check|--write] [--root PATH] [--db PATH] [--json|--format json]"
        );
        std::process::exit(2);
    };

    (
        zorg_refactor::PromoteRequest {
            store_options: parse_store_options(&store_args),
            id,
            mode: mode.unwrap_or(zorg_refactor::RefactorMode::Preview),
            destination,
        },
        output,
    )
}

fn parse_move_options(args: &[String]) -> (zorg_refactor::MoveRequest, RefactorOutput) {
    let mut id = None;
    let mut mode = None;
    let mut destination = None;
    let mut output = RefactorOutput::Text;
    let mut store_args = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                print_move_help();
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
            "--to" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for --to");
                    std::process::exit(2);
                };
                let parsed_destination = if value.starts_with('@') {
                    zorg_refactor::MoveDestination::ParentId(value.clone())
                } else {
                    zorg_refactor::MoveDestination::Path(PathBuf::from(value))
                };
                if destination.replace(parsed_destination).is_some() {
                    eprintln!("zorg move accepts exactly one --to value");
                    std::process::exit(2);
                }
            }
            "--check" => set_refactor_mode(&mut mode, zorg_refactor::RefactorMode::Check),
            "--write" => set_refactor_mode(&mut mode, zorg_refactor::RefactorMode::Write),
            "--json" => output = RefactorOutput::Json,
            "--format" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for --format");
                    std::process::exit(2);
                };
                match value.as_str() {
                    "json" => output = RefactorOutput::Json,
                    "text" => output = RefactorOutput::Text,
                    other => {
                        eprintln!(
                            "unsupported move output format `{other}`; expected json or text"
                        );
                        std::process::exit(2);
                    }
                }
            }
            argument if argument.starts_with('-') => {
                eprintln!("unexpected argument for `zorg move`: {argument}");
                std::process::exit(2);
            }
            argument => {
                if id.replace(argument.to_owned()).is_some() {
                    eprintln!("zorg move accepts exactly one zettel ID");
                    std::process::exit(2);
                }
            }
        }
        index += 1;
    }

    let Some(id) = id else {
        eprintln!(
            "usage: zorg move @id --to PATH_OR_PARENT [--check|--write] [--root PATH] [--db PATH] [--json|--format json]"
        );
        std::process::exit(2);
    };
    let Some(destination) = destination else {
        eprintln!("zorg move requires --to PATH_OR_PARENT");
        std::process::exit(2);
    };

    (
        zorg_refactor::MoveRequest {
            store_options: parse_store_options(&store_args),
            id,
            mode: mode.unwrap_or(zorg_refactor::RefactorMode::Preview),
            destination,
        },
        output,
    )
}

fn parse_extract_options(args: &[String]) -> (zorg_refactor::ExtractRequest, RefactorOutput) {
    let mut file = None;
    let mut range = None;
    let mut id = None;
    let mut mode = None;
    let mut destination = None;
    let mut replace_with_link = false;
    let mut output = RefactorOutput::Text;
    let mut store_args = Vec::new();
    let mut positional_range = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                print_extract_help();
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
            "--file" | "--id" | "--to" | "--range" | "--byte-range" => {
                let flag = args[index].clone();
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for {flag}");
                    std::process::exit(2);
                };
                match flag.as_str() {
                    "--file" => set_once(&mut file, PathBuf::from(value), "--file"),
                    "--id" => set_once(&mut id, value.clone(), "--id"),
                    "--to" => set_once(&mut destination, PathBuf::from(value), "--to"),
                    "--range" => set_once(
                        &mut range,
                        parse_extract_line_column_range(value),
                        "--range",
                    ),
                    "--byte-range" => {
                        set_once(&mut range, parse_extract_byte_range(value), "--byte-range")
                    }
                    _ => unreachable!("flag handled above"),
                }
            }
            "--replace-with-link" => replace_with_link = true,
            "--check" => set_refactor_mode(&mut mode, zorg_refactor::RefactorMode::Check),
            "--write" => set_refactor_mode(&mut mode, zorg_refactor::RefactorMode::Write),
            "--json" => output = RefactorOutput::Json,
            "--format" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for --format");
                    std::process::exit(2);
                };
                match value.as_str() {
                    "json" => output = RefactorOutput::Json,
                    "text" => output = RefactorOutput::Text,
                    other => {
                        eprintln!(
                            "unsupported extract output format `{other}`; expected json or text"
                        );
                        std::process::exit(2);
                    }
                }
            }
            argument if argument.starts_with('-') => {
                eprintln!("unexpected argument for `zorg extract`: {argument}");
                std::process::exit(2);
            }
            argument => {
                if positional_range.replace(argument.to_owned()).is_some() {
                    eprintln!("zorg extract accepts at most one positional RANGE");
                    std::process::exit(2);
                }
            }
        }
        index += 1;
    }

    if let Some(positional) = positional_range {
        if range
            .replace(parse_extract_line_column_range(&positional))
            .is_some()
        {
            eprintln!("choose only one extract range form");
            std::process::exit(2);
        }
    }

    let Some(file) = file else {
        print_extract_usage_and_exit();
    };
    let Some(range) = range else {
        print_extract_usage_and_exit();
    };
    let Some(id) = id else {
        print_extract_usage_and_exit();
    };

    (
        zorg_refactor::ExtractRequest {
            store_options: parse_store_options(&store_args),
            file,
            range,
            id,
            mode: mode.unwrap_or(zorg_refactor::RefactorMode::Preview),
            destination,
            replace_with_link,
        },
        output,
    )
}

fn parse_extract_line_column_range(value: &str) -> zorg_refactor::ExtractRange {
    let Some((start, end)) = value.split_once('-') else {
        eprintln!(
            "invalid extract range `{value}`; expected START_LINE:START_COL-END_LINE:END_COL"
        );
        std::process::exit(2);
    };
    let (start_line, start_column) = parse_line_column(start, value);
    let (end_line, end_column) = parse_line_column(end, value);
    zorg_refactor::ExtractRange::LineColumn {
        start_line,
        start_column,
        end_line,
        end_column,
    }
}

fn parse_line_column(value: &str, original: &str) -> (usize, usize) {
    let Some((line, column)) = value.split_once(':') else {
        eprintln!("invalid extract range `{original}`; expected line:column endpoints");
        std::process::exit(2);
    };
    let line = parse_positive_usize(line, "line", original);
    let column = parse_positive_usize(column, "column", original);
    (line, column)
}

fn parse_extract_byte_range(value: &str) -> zorg_refactor::ExtractRange {
    let Some((start, end)) = value.split_once("..") else {
        eprintln!("invalid byte range `{value}`; expected START..END");
        std::process::exit(2);
    };
    zorg_refactor::ExtractRange::Bytes {
        start: parse_usize(start, "start byte", value),
        end: parse_usize(end, "end byte", value),
    }
}

fn parse_positive_usize(value: &str, label: &str, original: &str) -> usize {
    let parsed = parse_usize(value, label, original);
    if parsed == 0 {
        eprintln!("invalid {label} in `{original}`; positions are one-based");
        std::process::exit(2);
    }
    parsed
}

fn parse_usize(value: &str, label: &str, original: &str) -> usize {
    value.parse::<usize>().unwrap_or_else(|_| {
        eprintln!("invalid {label} `{value}` in `{original}`");
        std::process::exit(2);
    })
}

fn print_extract_usage_and_exit() -> ! {
    eprintln!(
        "usage: zorg extract --file PATH --range START_LINE:START_COL-END_LINE:END_COL --id @new/id [--byte-range START..END] [--replace-with-link] [--check|--write] [--root PATH] [--db PATH] [--json|--format json]"
    );
    std::process::exit(2);
}

fn set_refactor_mode(
    mode: &mut Option<zorg_refactor::RefactorMode>,
    next: zorg_refactor::RefactorMode,
) {
    if mode.replace(next).is_some() {
        eprintln!("choose only one of --check or --write");
        std::process::exit(2);
    }
}

fn print_refactor_plan_text(plan: &zorg_refactor::RefactorPlan) {
    println!("{}: {:?}", plan.operation, plan.mode);
    for warning in &plan.warnings {
        println!("warning: {warning}");
    }
    for rejection in &plan.rejections {
        println!("rejected: {rejection}");
    }
    for file in &plan.files {
        println!("{}", file.absolute_path.display());
        for edit in &file.edits {
            let label = edit.label.as_deref().unwrap_or("edit");
            println!(
                "  {}:{} {}..{}",
                edit.span.start_line.unwrap_or(1),
                edit.span.start_column.unwrap_or(1),
                edit.span.start_byte,
                edit.span.end_byte
            );
            println!("  {label}");
        }
    }
}

fn run_path_cli(command: &'static str, args: Vec<String>) {
    let (id, output, options) = parse_path_options(command, &args);
    let store = open_store(options);
    ensure_query_index_ready(&store);
    let location = zorg_refactor::locate_zettel(&store, &id).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    match output {
        PathOutput::Text => println!("{}", path_location_text(&location)),
        PathOutput::Json => println!(
            "{}",
            serde_json::to_string_pretty(&path_location_json(command, &location))
                .expect("path JSON serializes")
        ),
    }
}

fn parse_path_options(
    command: &'static str,
    args: &[String],
) -> (String, PathOutput, StoreOptions) {
    let mut id = None;
    let mut output = PathOutput::Text;
    let mut store_args = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                print_path_help(command);
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
            "--json" => output = PathOutput::Json,
            "--format" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for --format");
                    std::process::exit(2);
                };
                match value.as_str() {
                    "json" => output = PathOutput::Json,
                    "text" | "list" => output = PathOutput::Text,
                    other => {
                        eprintln!(
                            "unsupported {command} output format `{other}`; expected json or text"
                        );
                        std::process::exit(2);
                    }
                }
            }
            argument if argument.starts_with('-') => {
                eprintln!("unexpected argument for `zorg {command}`: {argument}");
                std::process::exit(2);
            }
            argument => {
                if id.replace(argument.to_owned()).is_some() {
                    eprintln!("zorg {command} accepts exactly one zettel ID");
                    std::process::exit(2);
                }
            }
        }
        index += 1;
    }

    let Some(id) = id else {
        eprintln!("usage: zorg {command} @id [--root PATH] [--db PATH] [--json|--format json]");
        std::process::exit(2);
    };

    (id, output, parse_store_options(&store_args))
}

fn path_location_text(location: &zorg_refactor::ZettelLocation) -> String {
    let line = location.source_span.start_line.unwrap_or(1);
    let column = location.source_span.start_column.unwrap_or(1);
    let mut text = format!(
        "{}:{line}:{column} @{}",
        location.absolute_path.display(),
        location.canonical_id
    );
    if let Some(title) = &location.title {
        if !title.is_empty() {
            text.push(' ');
            text.push_str(title);
        }
    }
    text
}

fn path_location_json(
    command: &'static str,
    location: &zorg_refactor::ZettelLocation,
) -> serde_json::Value {
    json!({
        "schema_version": 1,
        "command": command,
        "canonical_id": location.canonical_id,
        "absolute_path": path_json(&location.absolute_path),
        "root_relative_path": path_json(&location.root_relative_path),
        "source_span": source_span_json(&location.source_span),
        "title": location.title,
        "kind": location.kind,
    })
}

fn parse_query_options(args: &[String]) -> (CliQuery, QueryOutput, StoreOptions) {
    let mut inline_query = None;
    let mut query_id = None;
    let mut output = QueryOutput::List;
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
            "--json" => {
                output = QueryOutput::Json;
            }
            "--format" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for --format");
                    std::process::exit(2);
                };
                match value.as_str() {
                    "json" => output = QueryOutput::Json,
                    "list" | "text" => output = QueryOutput::List,
                    other => {
                        eprintln!(
                            "unsupported query output format `{other}`; expected json or list"
                        );
                        std::process::exit(2);
                    }
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

    (query, output, parse_store_options(&store_args))
}

fn query_json_envelope(
    kind: zorg_query::QueryResultKind,
    query_source: &str,
    query: Option<&str>,
    definition: Option<&zorg_query::QueryDefinition>,
    rows: &[zorg_query::QueryResultRow],
    root: &Path,
) -> serde_json::Value {
    let rows_json = match kind {
        zorg_query::QueryResultKind::List => rows.iter().map(query_row_json).collect::<Vec<_>>(),
        zorg_query::QueryResultKind::Table => {
            rows.iter().map(query_table_row_json).collect::<Vec<_>>()
        }
        zorg_query::QueryResultKind::Aggregate => Vec::new(),
    };
    let columns = match kind {
        zorg_query::QueryResultKind::List => None,
        zorg_query::QueryResultKind::Table => Some(
            zorg_query::table_columns()
                .into_iter()
                .map(|column| {
                    json!({
                        "key": column.key,
                        "label": column.label,
                    })
                })
                .collect::<Vec<_>>(),
        ),
        zorg_query::QueryResultKind::Aggregate => None,
    };
    let mut envelope = json!({
        "schema_version": 1,
        "kind": query_kind_json(kind),
        "query_source": query_source,
        "query": query,
        "query_zettel": definition.map(|definition| query_definition_json(definition, root)),
        "rows": rows_json,
        "diagnostics": [],
    });
    if let Some(columns) = columns {
        envelope["columns"] = json!(columns);
    }
    if kind == zorg_query::QueryResultKind::Aggregate {
        envelope["values"] = json!({
            "count": rows.len(),
        });
        envelope
            .as_object_mut()
            .expect("query envelope should be an object")
            .remove("rows");
    }
    envelope
}

fn query_kind_json(kind: zorg_query::QueryResultKind) -> &'static str {
    match kind {
        zorg_query::QueryResultKind::List => "list",
        zorg_query::QueryResultKind::Table => "table",
        zorg_query::QueryResultKind::Aggregate => "aggregate",
    }
}

fn query_table_row_json(row: &zorg_query::QueryResultRow) -> serde_json::Value {
    let row = zorg_query::TableRow::from_query_result(row);
    json!({
        "todo": row.todo,
        "id": row.id,
        "file": row.file,
        "title": row.title,
    })
}

fn query_definition_json(
    definition: &zorg_query::QueryDefinition,
    root: &Path,
) -> serde_json::Value {
    json!({
        "id": definition.zettel_id,
        "path": path_json(definition.source_path.strip_prefix(root).unwrap_or(&definition.source_path)),
        "query": definition.query,
        "span": source_span_json(&definition.span),
    })
}

fn query_row_json(row: &zorg_query::QueryResultRow) -> serde_json::Value {
    json!({
        "store_row_id": row.zettel_store_id,
        "canonical_id": row.canonical_id,
        "path": path_json(&row.file_path),
        "title": row.title,
        "todo_marker": row.todo_marker,
        "source_order": row.source_order,
        "span": source_span_json(&row.source_span),
        "tags": row.tags,
        "properties": row.properties.iter().map(|property| {
            json!({
                "key": property.key,
                "value": property.value,
            })
        }).collect::<Vec<_>>(),
    })
}

fn source_span_json(span: &SourceSpan) -> serde_json::Value {
    json!({
        "start_byte": span.start_byte,
        "end_byte": span.end_byte,
        "start_line": span.start_line,
        "start_column": span.start_column,
        "end_line": span.end_line,
        "end_column": span.end_column,
    })
}

fn path_json(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum WatchOutputFormat {
    Text,
    Json,
}

#[derive(Debug)]
struct WatchCliOptions {
    root: Option<PathBuf>,
    db: Option<PathBuf>,
    debounce_ms: Option<u64>,
    output: WatchOutputFormat,
    run_control: RunControl,
}

impl Default for WatchCliOptions {
    fn default() -> Self {
        Self {
            root: None,
            db: None,
            debounce_ms: None,
            output: WatchOutputFormat::Text,
            run_control: RunControl::Unbounded,
        }
    }
}

fn run_watch_cli(args: Vec<String>) {
    let cli = parse_watch_options(&args);
    let resolved = ResolvedConfig::from_env(ConfigOverrides {
        root: cli.root,
        database_path: cli.db,
    })
    .unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    let debounce_ms = cli.debounce_ms.unwrap_or(resolved.watcher_debounce_ms());
    let options = WatchOptions::from_store_options(resolved.into_store_options())
        .with_debounce(Duration::from_millis(debounce_ms))
        .with_run_control(cli.run_control);
    let sink = CliWatchSink::new(cli.output);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap_or_else(|error| {
            eprintln!("failed to initialize watcher runtime: {error}");
            std::process::exit(1);
        });

    match runtime.block_on(run_watch_with_signal(options, sink)) {
        Ok(_) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

async fn run_watch_with_signal(
    options: WatchOptions,
    sink: CliWatchSink,
) -> zorg_core::ZorgResult<WatchRunResult> {
    let signal_options = options.clone();
    let signal_sink = sink.clone();
    tokio::select! {
        result = run_watch_service(options, sink) => result,
        signal = tokio::signal::ctrl_c() => {
            if let Err(error) = signal {
                return Err(zorg_core::ZorgError::OperationFailed {
                    message: format!("failed to listen for shutdown signal: {error}"),
                });
            }
            signal_sink.emit(WatchState {
                root: signal_options.root().to_path_buf(),
                database_path: signal_options.database_path().to_path_buf(),
                kind: WatchStateKind::Stopping,
            });
            signal_sink.emit(WatchState {
                root: signal_options.root().to_path_buf(),
                database_path: signal_options.database_path().to_path_buf(),
                kind: WatchStateKind::Stopped,
            });
            Ok(WatchRunResult { indexed_passes: 0 })
        }
    }
}

fn parse_watch_options(args: &[String]) -> WatchCliOptions {
    let mut options = WatchCliOptions::default();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => {
                print_watch_help();
                std::process::exit(0);
            }
            "--root" | "--db" | "--debounce" | "--format" | "--exit-after-events" => {
                let flag = args[index].clone();
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("missing value for {flag}");
                    std::process::exit(2);
                };
                match flag.as_str() {
                    "--root" => set_watch_path(&mut options.root, value, "--root"),
                    "--db" => set_watch_path(&mut options.db, value, "--db"),
                    "--debounce" => {
                        options.debounce_ms = Some(parse_watch_u64(value, "--debounce"));
                    }
                    "--format" => match value.as_str() {
                        "text" => options.output = WatchOutputFormat::Text,
                        "json" => options.output = WatchOutputFormat::Json,
                        _ => {
                            eprintln!("zorg watch --format must be text or json");
                            std::process::exit(2);
                        }
                    },
                    "--exit-after-events" => {
                        let limit = parse_watch_u64(value, "--exit-after-events");
                        if limit == 0 {
                            eprintln!("zorg watch --exit-after-events must be greater than 0");
                            std::process::exit(2);
                        }
                        set_watch_run_control(
                            &mut options.run_control,
                            RunControl::StopAfterEvents(limit),
                        );
                    }
                    _ => unreachable!("matched watch flag"),
                }
            }
            "--json" => options.output = WatchOutputFormat::Json,
            "--exit-after-ready" => {
                set_watch_run_control(&mut options.run_control, RunControl::StopAfterReady)
            }
            "--once" => {
                set_watch_run_control(&mut options.run_control, RunControl::InitialReindexOnly)
            }
            argument if argument.starts_with('-') => {
                eprintln!("unexpected argument for `zorg watch`: {argument}");
                std::process::exit(2);
            }
            argument => {
                eprintln!("unexpected positional argument for `zorg watch`: {argument}");
                std::process::exit(2);
            }
        }
        index += 1;
    }

    options
}

fn set_watch_path(slot: &mut Option<PathBuf>, value: &str, flag: &str) {
    if slot.replace(PathBuf::from(value)).is_some() {
        eprintln!("zorg watch accepts at most one {flag} value");
        std::process::exit(2);
    }
}

fn set_watch_run_control(slot: &mut RunControl, value: RunControl) {
    if *slot != RunControl::Unbounded {
        eprintln!("zorg watch accepts only one bounded run flag");
        std::process::exit(2);
    }
    *slot = value;
}

fn parse_watch_u64(value: &str, flag: &str) -> u64 {
    value.parse::<u64>().unwrap_or_else(|_| {
        eprintln!("zorg watch {flag} must be an unsigned integer");
        std::process::exit(2);
    })
}

#[derive(Clone)]
struct CliWatchSink {
    output: WatchOutputFormat,
    writer: Arc<Mutex<io::Stdout>>,
}

impl CliWatchSink {
    fn new(output: WatchOutputFormat) -> Self {
        Self {
            output,
            writer: Arc::new(Mutex::new(io::stdout())),
        }
    }
}

impl WatchEventSink for CliWatchSink {
    fn emit(&self, state: WatchState) {
        let mut writer = self.writer.lock().expect("watch output lock");
        let result = match self.output {
            WatchOutputFormat::Text => writeln!(writer, "{}", format_watch_text(&state)),
            WatchOutputFormat::Json => writeln!(writer, "{}", format_watch_json(&state)),
        }
        .and_then(|_| writer.flush());

        if result.is_err() {
            std::process::exit(1);
        }
    }
}

fn format_watch_text(state: &WatchState) -> String {
    match &state.kind {
        WatchStateKind::Starting => format!(
            "watch: starting root={} database={}",
            state.root.display(),
            state.database_path.display()
        ),
        WatchStateKind::Ready => format!(
            "watch: ready root={} database={}",
            state.root.display(),
            state.database_path.display()
        ),
        WatchStateKind::Indexing => "watch: indexing".to_owned(),
        WatchStateKind::Indexed { summary } => format!(
            "watch: indexed discovered_files={} indexed_files={} unchanged_files={} new_files={} changed_files={} deleted_files={} indexed_zettel={} diagnostics={} effective_tags={}",
            summary.discovered_files,
            summary.indexed_files,
            summary.unchanged_files,
            summary.new_files,
            summary.changed_files,
            summary.deleted_files,
            summary.zettel_count,
            summary.diagnostic_count,
            summary.effective_tag_count
        ),
        WatchStateKind::Degraded { message } => format!("watch: degraded {message}"),
        WatchStateKind::Error { message } => format!("watch: error {message}"),
        WatchStateKind::Stopping => "watch: stopping".to_owned(),
        WatchStateKind::Stopped => "watch: stopped".to_owned(),
    }
}

fn format_watch_json(state: &WatchState) -> serde_json::Value {
    let base = |state_name: &str| {
        json!({
            "schema_version": 1,
            "state": state_name,
            "root": state.root.display().to_string(),
            "database": state.database_path.display().to_string(),
        })
    };

    match &state.kind {
        WatchStateKind::Starting => base("starting"),
        WatchStateKind::Ready => base("ready"),
        WatchStateKind::Indexing => base("indexing"),
        WatchStateKind::Indexed { summary } => {
            let mut value = base("indexed");
            value["summary"] = json!({
                "discovered_files": summary.discovered_files,
                "indexed_files": summary.indexed_files,
                "unchanged_files": summary.unchanged_files,
                "new_files": summary.new_files,
                "changed_files": summary.changed_files,
                "deleted_files": summary.deleted_files,
                "zettel_count": summary.zettel_count,
                "diagnostic_count": summary.diagnostic_count,
                "effective_tag_count": summary.effective_tag_count,
                "last_indexed_at_unix_ms": summary.last_indexed_at_unix_ms,
            });
            value
        }
        WatchStateKind::Degraded { message } => {
            let mut value = base("degraded");
            value["message"] = json!(message);
            value
        }
        WatchStateKind::Error { message } => {
            let mut value = base("error");
            value["message"] = json!(message);
            value
        }
        WatchStateKind::Stopping => base("stopping"),
        WatchStateKind::Stopped => base("stopped"),
    }
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
  watch [--root PATH] [--db PATH] [--debounce MS] [--format text|json]
            Keep the SQLite store current while source files change
  dash [--root PATH] [--db PATH] [--panel today|inbox|search|diagnostics|index]
            Launch the terminal dashboard for an indexed corpus
  index     Deferred alias notice for corpus indexing
  query '<swog>' [--root PATH] [--db PATH]
  query --id @some/query [--root PATH] [--db PATH]
            Run an inline or stored SWOG query against an existing index
  path @id [--root PATH] [--db PATH] [--json|--format json]
            Print the indexed source location for a canonical zettel ID
  open @id [--root PATH] [--db PATH] [--json|--format json]
            Alias of path for editor jump integrations
  promote @id [--to PATH] [--check|--write] [--root PATH] [--db PATH]
            Promote a nested zettel into its own .z file
  move @id --to PATH_OR_PARENT [--check|--write] [--root PATH] [--db PATH]
            Move a zettel to a .z path or move a nested zettel under @parent
  extract --file PATH --range START_LINE:START_COL-END_LINE:END_COL --id @new/id
            Extract a body range into a new .z file and replace it with a link
  import legacy plan PATH... [--root ROOT] [--dest DEST] [--json|--format json]
            Preview deterministic legacy import output without writing files
  import legacy apply PATH... [--root ROOT] [--dest DEST] [--json|--format json]
            Write planned legacy import output as canonical .z files
  export markdown (--id @id|--subtree @id|--query '<swog>'|--query-id @id)
            Render indexed canonical .z zettels to Markdown
  fix [--check] [--json] [--root PATH] FILE...
            Apply safe autofixes or report pending autofixes with --check
  capture [--template @id|TITLE] [--json] [--title TEXT] [--dest PATH] [--root PATH]
            Create a zettel from a #z/tmpl template

Options:
  -h, --help     Print help
  -V, --version  Print version

Parser, store, inline query, location lookup, safe fix, and capture foundations are available."
    );
}

fn print_import_help() {
    println!(
        "\
Usage: zorg import legacy <plan|apply> PATH... [--root ROOT] [--dest DEST] [--json|--format json]

Commands:
  legacy plan   Read legacy .zo/.zoq/.zot inputs and preview canonical .z output
  legacy apply  Write canonical .z files after fatal-free planning"
    );
}

fn print_export_help() {
    println!(
        "\
Usage: zorg export markdown (--id @id|--subtree @id|--query '<swog>'|--query-id @id)
                            [--root ROOT] [--db DB] [--out DIR|--stdout]
                            [--json|--format json]

Commands:
  markdown  Render canonical .z zettels to Markdown from a current index"
    );
}

fn print_export_markdown_help() {
    println!(
        "\
Usage: zorg export markdown --id @id [--root ROOT] [--db DB] [--out DIR|--stdout] [--json|--format json]
       zorg export markdown --subtree @id [--root ROOT] [--db DB] [--out DIR|--stdout] [--json|--format json]
       zorg export markdown --query '<swog>' [--root ROOT] [--db DB] [--out DIR|--stdout] [--json|--format json]
       zorg export markdown --query-id @queries/foo [--root ROOT] [--db DB] [--out DIR|--stdout] [--json|--format json]

Uses the same current-index guard as `zorg query`, then reparses canonical .z
sources from the selected root and renders Markdown without mutating source
files. Stdout is the default. --out writes one .md file per exported canonical
ID using the ID path under DIR and refuses overwrites. JSON output reports
selection metadata, item metadata, written paths or stdout counts, diagnostics,
and summary counts without embedding Markdown bodies."
    );
}

fn print_export_markdown_usage() {
    eprintln!(
        "usage: zorg export markdown (--id @id|--subtree @id|--query '<swog>'|--query-id @id) [--root ROOT] [--db DB] [--out DIR|--stdout] [--json|--format json]"
    );
}

fn print_import_legacy_help() {
    println!(
        "\
Usage: zorg import legacy <plan|apply> PATH... [--root ROOT] [--dest DEST] [--json|--format json]

Commands:
  plan   Safe read-only import preview. This command does not write files.
  apply  Explicit write mode. Writes only canonical .z files and refuses overwrites."
    );
}

fn print_import_legacy_plan_help() {
    println!(
        "\
Usage: zorg import legacy plan PATH... [--root ROOT] [--dest DEST] [--json|--format json]

Plans conversion from explicit legacy .zo, .zoq, and .zot paths to canonical .z
source. PATH may name files or directories. Directory traversal and output are
deterministic. --root enables existing-output collision checks, and --dest
prefixes planned output paths under that root. The command is read-only: it
does not write source files, reindex, or mutate hidden state.

Exit codes:
  0  Plan completed without fatal diagnostics
  1  Inputs were readable but the plan contains fatal diagnostics
  2  CLI usage error"
    );
}

fn print_import_legacy_apply_help() {
    println!(
        "\
Usage: zorg import legacy apply PATH... [--root ROOT] [--dest DEST] [--json|--format json]

Plans conversion from explicit legacy .zo, .zoq, and .zot paths, refuses fatal
diagnostics and existing destination files, then writes only canonical .z
outputs. --root selects the destination corpus root and defaults to the current
directory. --dest prefixes output paths under that root.

Exit codes:
  0  Apply completed and all planned files were written
  1  Planning or writing reported fatal diagnostics
  2  CLI usage error"
    );
}

fn print_import_legacy_plan_usage() {
    eprintln!(
        "usage: zorg import legacy <plan|apply> PATH... [--root ROOT] [--dest DEST] [--json|--format json]"
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
Usage: zorg fix [--check] [--json|--format json] [--root PATH] [--db PATH] FILE...

Without --check, applies safe autofixes in place after validating the rewritten
sources. With --check, reports strict diagnostics and pending autofixes without
writing. Exits zero when there are no strict diagnostics and no pending fixes;
otherwise exits nonzero with one parseable line per finding. JSON output is
schema-versioned for editor integrations."
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

fn print_watch_help() {
    println!(
        "\
Usage: zorg watch [--root PATH] [--db PATH] [--debounce MS]
                  [--format text|json] [--json]
                  [--once|--exit-after-ready|--exit-after-events N]

Runs the live indexing watcher for a corpus root. Filesystem events are
debounced into incremental store reindex passes; `zorg db reindex` remains the
batch and CI command path. Text output is human-readable. JSON output is
line-delimited event objects with stable state names: starting, ready, indexing,
indexed, degraded, error, stopping, and stopped.

Bounded flags are intended for smoke tests and editor health checks:
  --exit-after-ready    Emit readiness and stop
  --once                Run one initial reindex pass and stop
  --exit-after-events N Stop after N accepted source events"
    );
}

fn print_query_help() {
    println!(
        "\
Usage: zorg query '<swog>' [--root PATH] [--db PATH] [--json|--format json]
       zorg query --id @some/query [--root PATH] [--db PATH] [--json|--format json]

Runs an inline SWOG LIST/TABLE/count query, or a query::/swog definition stored in an
ordinary #z/query zettel, against an existing, current SQLite index.
Run `zorg db reindex` first after adding or changing source files. LIST is the
default human output; leading TABLE selects the minimal table renderer, and
count(<query expression>) selects the aggregate renderer. JSON is the versioned
machine-readable contract."
    );
}

fn print_path_help(command: &str) {
    println!(
        "\
Usage: zorg {command} @id [--root PATH] [--db PATH] [--json|--format json]

Resolves a canonical zettel ID against an existing, current SQLite index and
prints the source file plus one-based line and column for editor jumps. `zorg
open` is an alias with the same behavior; JSON output records the requested
command as either `path` or `open`."
    );
}

fn print_promote_help() {
    println!(
        "\
Usage: zorg promote @id [--to PATH] [--check|--write]
                   [--root PATH] [--db PATH] [--json|--format json]

Plans promotion of a nested zettel into a file zettel. The default mode is a
dry-run preview. --check validates the plan without writing, and --write
applies it after reparsing planned sources. Without --to, the destination is
derived from the canonical ID under the corpus root, such as foo/bar.z for
@foo/bar. Destinations must stay under --root and use the .z extension."
    );
}

fn print_move_help() {
    println!(
        "\
Usage: zorg move @id --to PATH_OR_PARENT [--check|--write]
                [--root PATH] [--db PATH] [--json|--format json]

Plans a conservative structural move. File zettels can move to a new .z path.
Nested zettels can move to a new .z path or under a destination parent written
as @parent/id. The default mode is a dry-run preview. --check validates the plan
without writing, and --write applies it after reparsing planned sources.
Destinations must stay under --root and path destinations must use .z."
    );
}

fn print_extract_help() {
    println!(
        "\
Usage: zorg extract --file PATH --range START_LINE:START_COL-END_LINE:END_COL --id @new/id
                    [--byte-range START..END] [--to PATH] [--replace-with-link]
                    [--check|--write] [--root PATH] [--db PATH] [--json|--format json]

Plans extraction of a selected paragraph or fenced-code body range into a new
file zettel. The default mode is a dry-run preview. --check validates the plan
without writing, and --write applies it after reparsing planned sources. Line
and column positions are one-based. Byte ranges are zero-based and must align
with UTF-8 character boundaries.

Without --to, the destination is derived from the new canonical ID under the
corpus root, such as foo/bar.z for @foo/bar. The selected range is replaced
with #foo/bar when it covers a paragraph-like block; pass --replace-with-link
to allow replacement of a smaller valid body selection."
    );
}

fn print_capture_help() {
    println!(
        "\
Usage: zorg capture [--template @id|TITLE] [--title TEXT] [--source TEXT] [--body TEXT]
                    [--dest PATH] [--id @new-id] [--root PATH] [--db PATH]
                    [--allow-outside] [--json|--format json]

Creates a zettel from an ordinary #z/tmpl template. Template selection is by
canonical ID when --template starts with @, otherwise by exact title:: value.
When --template is omitted in a TTY, prompts for a template and missing values.
Destination defaults to template dest:: and must stay under --root unless
--allow-outside is supplied. On success, prints destination and zettel_id."
    );
}
