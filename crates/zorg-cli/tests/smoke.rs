use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    fn new() -> Self {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("zorg-cli-test-{}-{counter}", std::process::id()));
        if path.exists() {
            std::fs::remove_dir_all(&path).expect("clear stale temp workspace");
        }
        std::fs::create_dir_all(&path).expect("create temp workspace");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn zorg_help_works() {
    let output = Command::new(env!("CARGO_BIN_EXE_zorg"))
        .arg("--help")
        .output()
        .expect("run zorg --help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help should be utf8");
    assert!(stdout.contains("Usage: zorg"));
    assert!(stdout.contains("dash [--root PATH]"));
}

#[test]
fn zorg_dash_help_lists_no_color() {
    let output = run_zorg(&["dash", "--help"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("dash help should be utf8");
    assert!(stdout.contains("--mouse"));
    assert!(stdout.contains("--no-mouse"));
    assert!(stdout.contains("--no-color"));
    assert!(stdout.contains("y to yank a row ID"));
}

#[test]
fn zorg_dash_once_renders_stable_frame() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let db = temp.path().join("zorg.sqlite3");

    let output = run_zorg(&[
        "dash",
        "--once",
        "--panel",
        "index",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("Zorg Dash"));
    assert!(stdout.contains("Panels"));
    assert!(stdout.contains("> Index"));
    assert!(stdout.contains("Read-only index unavailable"));
    assert!(stdout.contains("Root:"));
    assert!(stdout.contains("Database:"));
    assert!(stdout.contains("zorg db reindex"));
    assert!(stdout.contains("read-only"));
}

#[test]
fn zorg_dash_mouse_flags_render_once() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let db = temp.path().join("zorg.sqlite3");

    let mouse_output = run_zorg(&[
        "dash",
        "--once",
        "--mouse",
        "--panel",
        "index",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert!(mouse_output.status.success());
    let stdout = String::from_utf8(mouse_output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("Read-only index unavailable"));

    let no_mouse_output = run_zorg(&[
        "dash",
        "--once",
        "--no-mouse",
        "--panel",
        "index",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert!(no_mouse_output.status.success());
    let stdout = String::from_utf8(no_mouse_output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("Read-only index unavailable"));
}

#[test]
fn zorg_dash_no_alt_screen_flag_renders_once() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let db = temp.path().join("zorg.sqlite3");

    let output = run_zorg(&[
        "dash",
        "--once",
        "--no-alt-screen",
        "--panel",
        "index",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("Zorg Dash"));
    assert!(stdout.contains("> Index"));
    assert!(stdout.contains("Read-only index unavailable"));
}

#[test]
fn zorg_dash_no_color_and_no_color_env_render_once() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let db = temp.path().join("zorg.sqlite3");

    let flag_output = run_zorg(&[
        "dash",
        "--once",
        "--no-color",
        "--panel",
        "index",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert!(flag_output.status.success());
    let stdout = String::from_utf8(flag_output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("Zorg Dash"));

    let env_output = run_zorg_with_env(
        &[
            "dash",
            "--once",
            "--panel",
            "index",
            "--root",
            root.to_str().expect("root utf8"),
            "--db",
            db.to_str().expect("db utf8"),
        ],
        &[("NO_COLOR", "1")],
    );
    assert!(env_output.status.success());
    let stdout = String::from_utf8(env_output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("Read-only index unavailable"));
}

#[test]
fn zorg_dash_once_empty_current_index_gives_next_step() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let db = temp.path().join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "dash",
        "--once",
        "--panel",
        "index",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("> Index"));
    assert!(stdout.contains("No .z files are indexed"));
    assert!(stdout.contains("zorg db reindex"));
}

#[test]
fn zorg_dash_once_index_panel_shows_real_counts() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref
Root
%%%

- @root/task #z/todo [ ] Task.
",
    )
    .expect("write source");
    let db = temp.path().join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "dash",
        "--once",
        "--panel",
        "index",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("> Index"));
    assert!(stdout.contains("Discovered files"));
    assert!(stdout.contains("Indexed files"));
    assert!(stdout.contains("Schema version"));
}

#[test]
fn zorg_dash_once_diagnostics_panel_shows_indexed_messages() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source_path = root.join("broken.z");
    let source = "\
%%% @root #z/ref
Root
%%%

See #missing.
";
    std::fs::write(&source_path, source).expect("write source");
    let db = temp.path().join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "dash",
        "--once",
        "--panel",
        "diagnostics",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("> Diagnostics"));
    assert!(stdout.contains("marked 0"));
    assert!(stdout.contains("broken.z"));
    assert!(stdout.contains("reference.unresolved_absolute"));
    assert_eq!(
        std::fs::read_to_string(source_path).expect("read source after dash once"),
        source
    );
}

#[test]
fn zorg_dash_once_today_panel_shows_due_do_todo_and_attention_rows() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let today = current_utc_date();
    std::fs::write(
        root.join("today.z"),
        format!(
            "\
%%% @root #z/ref
Root
%%%

- @root/due #z/todo [ ] due::{today}
  Due today.

- @root/do #z/todo [N] do::{today}
  Do today.

See #missing.
"
        ),
    )
    .expect("write source");
    let db = temp.path().join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "dash",
        "--once",
        "--panel",
        "today",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("> Today"));
    assert!(stdout.contains("@root/due"));
    assert!(stdout.contains("@root/do"));
    assert!(stdout.contains("reference.unresolved_absolute"));
    assert!(stdout.contains("y yank"));
}

#[test]
fn zorg_dash_once_inbox_and_search_panels_use_indexed_queries() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref
Root
%%%

- @tasks/inbox #z/inbox #z/todo [ ] Inbox task.
- @tasks/later #z/todo [N] Later task.
- @queries/inbox #z/query title::Inbox query query::#z/inbox
",
    )
    .expect("write source");
    let db = temp.path().join("zorg.sqlite3");
    reindex(&root, &db);

    let inbox_output = run_zorg(&[
        "dash",
        "--once",
        "--panel",
        "inbox",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert!(inbox_output.status.success());
    let stdout = String::from_utf8(inbox_output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("> Inbox"));
    assert!(stdout.contains("@tasks/inbox"));
    assert!(!stdout.contains("@tasks/later"));

    let search_output = run_zorg(&[
        "dash",
        "--once",
        "--panel",
        "search",
        "--query",
        "#z/inbox",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert!(search_output.status.success());
    let stdout = String::from_utf8(search_output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("> Search"));
    assert!(stdout.contains("Query: #z/inbox"));
    assert!(stdout.contains("@tasks/inbox"));
    assert!(!stdout.contains("@tasks/later"));

    let stored_output = run_zorg(&[
        "dash",
        "--once",
        "--panel",
        "search",
        "--query",
        "@queries/inbox",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert!(stored_output.status.success());
    let stdout = String::from_utf8(stored_output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("Query: @queries/inbox"));
    assert!(stdout.contains("Stored:"));
    assert!(stdout.contains("@queries/inbox"));
    assert!(stdout.contains("Source: query:: property"));
    assert!(stdout.contains("Definition: #z/inbox"));
    assert!(stdout.contains("@tasks/inbox"));
    assert!(!stdout.contains("@tasks/later"));

    let query_output = run_zorg(&[
        "query",
        "--id",
        "@queries/inbox",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert!(query_output.status.success());
    let query_stdout = String::from_utf8(query_output.stdout).expect("query output should be utf8");
    assert!(query_stdout.contains("@tasks/inbox"));
    assert_eq!(
        stdout.contains("@tasks/inbox"),
        query_stdout.contains("@tasks/inbox")
    );
    assert_eq!(
        stdout.contains("@tasks/later"),
        query_stdout.contains("@tasks/later")
    );
}

#[test]
fn zorg_dash_once_queries_panel_lists_saved_query_catalog() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref
Root
%%%

- @tasks/open #z/todo [ ] Open task.
- @tasks/inbox #z/inbox Inbox task.
- @queries/open #z/query title::Open query query::#z/todo
- @queries/inbox #z/query title::Inbox query
  ```swog
  #z/inbox
  ```
- @queries/bad #z/query title::Broken query query::todo:[A]
",
    )
    .expect("write source");
    let db = temp.path().join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "dash",
        "--once",
        "--panel",
        "queries",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("> Queries"));
    assert!(stdout.contains("@queries/open"));
    assert!(stdout.contains("@queries/inbox"));
    assert!(stdout.contains("error @queries/bad"));
    assert!(stdout.contains("Source: query:: property"));
    assert!(stdout.contains("Definition: #z/todo"));
}

#[test]
fn zorg_dash_once_invalid_search_query_renders_inline_error() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(root.join("main.z"), "%%% @root #z/ref\nRoot\n%%%\n").expect("write source");
    let db = temp.path().join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "dash",
        "--once",
        "--panel",
        "search",
        "--query",
        "OR",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("> Search"));
    assert!(stdout.contains("Query: OR"));
    assert!(stdout.contains("Error: query parse failed"));
}

#[test]
fn zorg_dash_exit_after_is_bounded() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let db = temp.path().join("zorg.sqlite3");

    let output = run_zorg(&[
        "dash",
        "--exit-after",
        "1",
        "--no-alt-screen",
        "--no-mouse",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("dash output should be utf8");
    assert!(stdout.contains("Zorg Dash"));
}

#[test]
fn zorg_parse_emits_pretty_model_json() {
    let fixture = format!(
        "{}/../../fixtures/corpus/minimal.z",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_zorg"))
        .args(["parse", fixture.as_str()])
        .output()
        .expect("run zorg parse");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("parse output should be utf8");
    assert!(stdout.contains("\"id\": \"minimal\""));
    assert!(stdout.contains("\"tag\": \"z/ref\""));
    assert!(stdout.contains("\"key\": \"area\""));
    assert!(stdout.contains("\"diagnostics\": []"));
}

#[test]
fn zorg_parse_reports_unreadable_file() {
    let fixture = format!(
        "{}/../../fixtures/corpus/missing.z",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_zorg"))
        .args(["parse", fixture.as_str()])
        .output()
        .expect("run zorg parse");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("error output should be utf8");
    assert!(stderr.contains("failed to read"));
}

#[test]
fn zorg_check_reports_strict_semantic_errors() {
    let fixture = format!(
        "{}/../../fixtures/corpus/legacy_invalid.z",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_zorg"))
        .args(["check", fixture.as_str()])
        .output()
        .expect("run zorg check");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("check output should be utf8");
    assert!(stderr.contains("legacy.unsupported"));
    assert!(stderr.contains("ID::"));
}

#[test]
fn zorg_check_root_validates_full_corpus_for_unresolved_links() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("alpha.z"),
        "%%% @alpha #z/ref\nAlpha\n%%%\n\nSee #beta.\n",
    )
    .expect("write alpha");
    std::fs::write(root.join("beta.z"), "%%% @beta #z/ref\nBeta\n%%%\n").expect("write beta");

    let success = run_zorg(&["check", "--root", root.to_str().expect("root utf8")]);
    assert!(
        success.status.success(),
        "expected success: stderr={}",
        String::from_utf8_lossy(&success.stderr)
    );

    std::fs::remove_file(root.join("beta.z")).expect("remove target");
    let failure = run_zorg(&["check", "--root", root.to_str().expect("root utf8")]);
    assert!(!failure.status.success());
    let stderr = String::from_utf8(failure.stderr).expect("check output should be utf8");
    assert!(
        stderr.contains("reference.unresolved_absolute"),
        "expected unresolved absolute diagnostic: {stderr}"
    );
    assert!(stderr.contains("alpha.z"));
}

#[test]
fn zorg_check_reports_query_definition_errors() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("query.z"),
        "\
%%% @root #z/ref
Root
%%%

- @queries/bad #z/query
  ```swog
  sum(#z/todo)
  ```
",
    )
    .expect("write invalid query");

    let output = run_zorg(&["check", "--root", root.to_str().expect("root utf8")]);
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("check output should be utf8");
    assert!(stderr.contains("query.definition"), "{stderr}");
    assert!(stderr.contains("invalid query definition"), "{stderr}");
    assert!(
        stderr.contains("aggregation functions other than count() are not supported"),
        "{stderr}"
    );
}

#[test]
fn zorg_check_reports_template_definition_errors() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("template.z"),
        "\
%%% @root #z/ref
Root
%%%

- @templates/bad #z/tmpl title::Bad dest::out.z
  ```zorg-template
  - @{{unknown}} #z/ref Bad
  ```
",
    )
    .expect("write invalid template");

    let output = run_zorg(&["check", "--root", root.to_str().expect("root utf8")]);
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("check output should be utf8");
    assert!(stderr.contains("template.definition"), "{stderr}");
    assert!(
        stderr.contains("template variable `unknown` is not defined"),
        "{stderr}"
    );
}

#[test]
fn zorg_fix_check_reports_pending_typo_rewrite() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(root.join("plan.z"), "%%% @project/plan #z/ref\nPlan\n%%%\n")
        .expect("write plan target");
    let typo_path = root.join("links.z");
    std::fs::write(
        &typo_path,
        "%%% @links #z/ref\nLinks\n%%%\n\nSee #poject/plan.\n",
    )
    .expect("write typo source");

    let output = run_zorg(&[
        "fix",
        "--check",
        "--root",
        root.to_str().expect("root utf8"),
    ]);
    assert!(
        !output.status.success(),
        "expected nonzero exit when fixes pending"
    );
    let stderr = String::from_utf8(output.stderr).expect("fix output should be utf8");
    assert!(
        stderr.contains("fix.unresolved_absolute_link_typo"),
        "expected typo rule code: {stderr}"
    );
    assert!(stderr.contains("Rewrite unresolved link to #project/plan"));
    assert!(stderr.contains("links.z"));
    let typo_line = stderr
        .lines()
        .find(|line| line.contains("fix.unresolved_absolute_link_typo"))
        .expect("typo line");
    let suffix = typo_line.split_once("links.z:").expect("path prefix").1;
    let position = suffix.split(':').next().expect("line number");
    assert!(
        position.parse::<usize>().is_ok(),
        "expected line number, got {position}"
    );
}

#[test]
fn zorg_fix_check_exits_zero_when_corpus_clean() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(root.join("clean.z"), "%%% @clean #z/ref\nClean\n%%%\n")
        .expect("write clean source");

    let output = run_zorg(&[
        "fix",
        "--check",
        "--root",
        root.to_str().expect("root utf8"),
    ]);
    assert!(
        output.status.success(),
        "expected zero exit on clean corpus: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn zorg_fix_applies_safe_autofixes_idempotently() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source_path = root.join("format.z");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus/autofix_unfixed.z"),
        &source_path,
    )
    .expect("copy malformed source");
    let date = current_utc_date();
    let expected_fixed = format!(
        "\
%%% @autofix modified::{date} #z/ref area::work/research
Autofix fixture
%%%

- @autofix/task #z/todo due::2026-05-15
  See #autofix/target.

- @autofix/target #z/ref Target.
"
    );

    let check = run_zorg(&[
        "fix",
        "--check",
        source_path.to_str().expect("source path utf8"),
    ]);
    assert!(
        !check.status.success(),
        "expected pending fixes before write"
    );
    let stderr = String::from_utf8(check.stderr).expect("fix check output should be utf8");
    assert!(stderr.contains("fix.bullet_symbol"));
    assert!(stderr.contains("fix.property_whitespace"));
    assert!(stderr.contains("fix.modified_stamp"));

    let first = run_zorg(&["fix", source_path.to_str().expect("source path utf8")]);
    assert!(
        first.status.success(),
        "expected fix write success: stderr={}",
        String::from_utf8_lossy(&first.stderr)
    );
    let fixed = std::fs::read_to_string(&source_path).expect("read fixed source");
    assert_eq!(fixed, expected_fixed);

    let second = run_zorg(&["fix", source_path.to_str().expect("source path utf8")]);
    assert!(
        second.status.success(),
        "expected idempotent second fix: stderr={}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&source_path).expect("read second fixed source"),
        fixed
    );

    let clean_check = run_zorg(&[
        "fix",
        "--check",
        source_path.to_str().expect("source path utf8"),
    ]);
    assert!(
        clean_check.status.success(),
        "expected fixed source to pass --check: stderr={}",
        String::from_utf8_lossy(&clean_check.stderr)
    );
}

#[test]
fn zorg_fix_applies_spec_backed_stamping_and_sorting_idempotently() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source_path = root.join("stamp-sort.z");
    std::fs::write(
        &source_path,
        "\
%%% #z/ref
Stamp sort fixture
%%%

zorg-sort:start
- beta
- alpha
zorg-sort:end
",
    )
    .expect("write stamp sort source");

    let check = run_zorg(&[
        "fix",
        "--check",
        source_path.to_str().expect("source path utf8"),
    ]);
    assert!(!check.status.success());
    let stderr = String::from_utf8(check.stderr).expect("fix check output should be utf8");
    assert!(stderr.contains("fix.id_stamp"));
    assert!(stderr.contains("fix.modified_stamp"));
    assert!(stderr.contains("fix.sort_pragma_region"));

    let first = run_zorg(&["fix", source_path.to_str().expect("source path utf8")]);
    assert!(
        first.status.success(),
        "expected fix write success: stderr={}",
        String::from_utf8_lossy(&first.stderr)
    );

    let date = current_utc_date();
    assert_eq!(
        std::fs::read_to_string(&source_path).expect("read rewritten source"),
        format!(
            "\
%%% @stamp-sort modified::{date} #z/ref
Stamp sort fixture
%%%

zorg-sort:start
- alpha
- beta
zorg-sort:end
"
        )
    );

    let second = run_zorg(&["fix", source_path.to_str().expect("source path utf8")]);
    assert!(
        second.status.success(),
        "expected idempotent second fix: stderr={}",
        String::from_utf8_lossy(&second.stderr)
    );

    let clean_check = run_zorg(&[
        "fix",
        "--check",
        source_path.to_str().expect("source path utf8"),
    ]);
    assert!(
        clean_check.status.success(),
        "expected fixed source to pass --check: stderr={}",
        String::from_utf8_lossy(&clean_check.stderr)
    );
}

#[test]
fn zorg_fix_applies_link_typo_rewrite_with_root_context() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(root.join("plan.z"), "%%% @project/plan #z/ref\nPlan\n%%%\n")
        .expect("write target");
    let links_path = root.join("links.z");
    std::fs::write(
        &links_path,
        "%%% @links #z/ref\nLinks\n%%%\n\nSee #poject/plan.\n",
    )
    .expect("write typo source");

    let output = run_zorg(&["fix", "--root", root.to_str().expect("root utf8")]);
    assert!(
        output.status.success(),
        "expected fix write success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&links_path).expect("read rewritten links"),
        format!(
            "%%% @links modified::{} #z/ref\nLinks\n%%%\n\nSee #project/plan.\n",
            current_utc_date()
        )
    );

    let clean_check = run_zorg(&[
        "fix",
        "--check",
        "--root",
        root.to_str().expect("root utf8"),
    ]);
    assert!(
        clean_check.status.success(),
        "expected clean corpus after link fix: stderr={}",
        String::from_utf8_lossy(&clean_check.stderr)
    );
}

#[test]
fn zorg_fix_json_reports_schema_versioned_summary() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source_path = root.join("format.z");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus/autofix_unfixed.z"),
        &source_path,
    )
    .expect("copy malformed source");

    let output = run_zorg(&[
        "fix",
        "--check",
        "--json",
        source_path.to_str().expect("source path utf8"),
    ]);

    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("fix json should parse");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["mode"], "check");
    assert_eq!(value["files"][0]["path"], source_path.display().to_string());
    assert!(
        value["files"][0]["planned_fixes"]
            .as_u64()
            .expect("planned fix count")
            >= 3
    );
    assert_eq!(value["files"][0]["applied_edits"], 0);
    assert!(
        value["files"][0]["fixes"]
            .as_array()
            .expect("fix array")
            .iter()
            .any(|fix| fix["code"] == "fix.bullet_symbol")
    );
}

#[test]
fn zorg_db_status_reports_discovered_canonical_sources() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    let nested = root.join("dir");
    std::fs::create_dir_all(&nested).expect("create corpus");
    std::fs::write(root.join("alpha.z"), "").expect("write root source");
    std::fs::write(root.join("alpha.zo"), "").expect("write legacy source");
    std::fs::write(root.join("notes.txt"), "").expect("write unrelated source");
    std::fs::write(nested.join("init.z"), "").expect("write directory source");
    std::fs::write(nested.join("template.zot"), "").expect("write legacy template");
    let db = temp.path().join("db").join("zorg.sqlite3");

    let output = run_zorg(&[
        "db",
        "status",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("status output should be utf8");
    assert!(stdout.contains(&format!("root: {}", root.display())));
    assert!(stdout.contains(&format!("database: {}", db.display())));
    assert!(stdout.contains("schema_version: 2"));
    assert!(stdout.contains("discovered_files: 2"));
    assert!(stdout.contains("indexed_files: 0"));
    assert!(stdout.contains("new_files: 2"));
    assert!(stdout.contains("changed_files: 0"));
    assert!(stdout.contains("deleted_files: 0"));
    assert!(stdout.contains("last_indexed_at_unix_ms: never"));
}

#[test]
fn zorg_db_status_resolves_store_config_precedence() {
    let temp = TempWorkspace::new();
    let home = temp.path().join("home");
    let xdg = temp.path().join("xdg");
    let user_root = temp.path().join("user-root");
    let env_root = temp.path().join("env-root");
    let root_config_db = temp.path().join("root-config.sqlite3");
    let cli_db = temp.path().join("cli.sqlite3");
    std::fs::create_dir_all(&user_root).expect("create user root");
    std::fs::create_dir_all(xdg.join("zorg")).expect("create user config dir");
    std::fs::write(
        xdg.join("zorg/config.toml"),
        format!(
            "root = \"{}\"\ndatabase_path = \"{}\"\nwatcher_debounce_ms = 50\n",
            user_root.display(),
            temp.path().join("user.sqlite3").display()
        ),
    )
    .expect("write user config");
    std::fs::create_dir_all(env_root.join(".zorg")).expect("create root config dir");
    std::fs::write(
        env_root.join(".zorg/config.toml"),
        format!("database_path = \"{}\"\n", root_config_db.display()),
    )
    .expect("write root config");

    let output = run_zorg_with_env(
        &["db", "status"],
        &[
            ("HOME", home.to_str().expect("home should be utf8")),
            ("XDG_CONFIG_HOME", xdg.to_str().expect("xdg should be utf8")),
            ("ZORG_ROOT", env_root.to_str().expect("root should be utf8")),
        ],
    );
    assert!(
        output.status.success(),
        "expected config status success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("status output should be utf8");
    assert!(stdout.contains(&format!("root: {}", env_root.display())));
    assert!(stdout.contains(&format!("database: {}", root_config_db.display())));

    let output = run_zorg_with_env(
        &[
            "db",
            "status",
            "--root",
            user_root.to_str().expect("root should be utf8"),
            "--db",
            cli_db.to_str().expect("db should be utf8"),
        ],
        &[
            ("HOME", home.to_str().expect("home should be utf8")),
            ("XDG_CONFIG_HOME", xdg.to_str().expect("xdg should be utf8")),
            ("ZORG_ROOT", env_root.to_str().expect("root should be utf8")),
        ],
    );
    assert!(
        output.status.success(),
        "expected CLI override success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("status output should be utf8");
    assert!(stdout.contains(&format!("root: {}", user_root.display())));
    assert!(stdout.contains(&format!("database: {}", cli_db.display())));
}

#[test]
fn zorg_db_status_reports_invalid_config() {
    let temp = TempWorkspace::new();
    let home = temp.path().join("home");
    let xdg = temp.path().join("xdg");
    std::fs::create_dir_all(xdg.join("zorg")).expect("create user config dir");
    std::fs::write(
        xdg.join("zorg/config.toml"),
        "watcher_debounce_ms = \"slow\"\n",
    )
    .expect("write invalid user config");

    let output = run_zorg_with_env(
        &["db", "status"],
        &[
            ("HOME", home.to_str().expect("home should be utf8")),
            ("XDG_CONFIG_HOME", xdg.to_str().expect("xdg should be utf8")),
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("error output should be utf8");
    assert!(stderr.contains("failed to parse Zorg config"), "{stderr}");
    assert!(stderr.contains("config.toml"), "{stderr}");
}

#[test]
fn zorg_db_status_reports_duplicate_named_roots() {
    let temp = TempWorkspace::new();
    let home = temp.path().join("home");
    let xdg = temp.path().join("xdg");
    let root = home.join("root");
    std::fs::create_dir_all(xdg.join("zorg")).expect("create user config dir");
    std::fs::write(
        xdg.join("zorg/config.toml"),
        "root = \"~/root\"\n[named_roots]\nwork = \"~/work\"\n",
    )
    .expect("write user config");
    std::fs::create_dir_all(root.join(".zorg")).expect("create root config dir");
    std::fs::write(
        root.join(".zorg/config.toml"),
        "[named_roots]\nwork = \"~/other-work\"\n",
    )
    .expect("write root config");

    let output = run_zorg_with_env(
        &["db", "status"],
        &[
            ("HOME", home.to_str().expect("home should be utf8")),
            ("XDG_CONFIG_HOME", xdg.to_str().expect("xdg should be utf8")),
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("error output should be utf8");
    assert!(stderr.contains("duplicate named root"), "{stderr}");
    assert!(stderr.contains("work"), "{stderr}");
}

#[test]
fn zorg_db_reindex_builds_full_snapshot() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("minimal.z"),
        "%%% @minimal #z/ref area::work/research\nMinimal fixture\n%%%\n",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");

    let output = run_zorg(&[
        "db",
        "reindex",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("reindex output should be utf8");
    assert!(stdout.contains("discovered_files: 1"));
    assert!(stdout.contains("indexed_files: 1"));
    assert!(stdout.contains("new_files: 1"));
    assert!(stdout.contains("indexed_zettel: 1"));
    assert!(stdout.contains("reindex: incremental complete"));
}

#[test]
fn zorg_path_prints_text_location_for_indexed_zettel() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source = root.join("main.z");
    std::fs::write(
        &source,
        "\
%%% @root #z/ref
Root fixture
%%%

- @root/child #z/ref Child fixture.
",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "path",
        "@root/child",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected path success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("path output should be utf8");
    assert_eq!(
        stdout,
        format!("{}:5:1 @root/child Child fixture.\n", source.display())
    );
}

#[test]
fn zorg_path_json_is_editor_jump_contract() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source = root.join("main.z");
    std::fs::write(&source, "%%% @root #z/ref\nRoot fixture\n%%%\n").expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let path_output = run_zorg(&[
        "path",
        "@root",
        "--format",
        "json",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(path_output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&path_output.stdout).expect("path json should parse");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["command"], "path");
    assert_eq!(value["canonical_id"], "root");
    assert_eq!(value["absolute_path"], source.display().to_string());
    assert_eq!(value["root_relative_path"], "main.z");
    assert_eq!(value["source_span"]["start_line"], 1);
    assert_eq!(value["source_span"]["start_column"], 1);
    assert_eq!(value["title"], "Root fixture");
    assert_eq!(value["kind"], "file");

    let open_output = run_zorg(&[
        "open",
        "@root",
        "--json",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(open_output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&open_output.stdout).expect("open json should parse");
    assert_eq!(value["command"], "open");
    assert_eq!(value["canonical_id"], "root");
}

#[test]
fn zorg_path_reports_invalid_missing_and_ambiguous_ids() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(root.join("one.z"), "%%% @dup #z/ref\nOne\n%%%\n").expect("write first source");
    std::fs::write(root.join("two.z"), "%%% @dup #z/ref\nTwo\n%%%\n").expect("write second source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let invalid = run_zorg(&[
        "path",
        "dup",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!invalid.status.success());
    let stderr = String::from_utf8(invalid.stderr).expect("invalid error should be utf8");
    assert!(stderr.contains("must start with '@'"), "{stderr}");

    let missing = run_zorg(&[
        "path",
        "@missing",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!missing.status.success());
    let stderr = String::from_utf8(missing.stderr).expect("missing error should be utf8");
    assert!(
        stderr.contains("no indexed zettel found for `@missing`"),
        "{stderr}"
    );

    let ambiguous = run_zorg(&[
        "path",
        "@dup",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!ambiguous.status.success());
    let stderr = String::from_utf8(ambiguous.stderr).expect("ambiguous error should be utf8");
    assert!(
        stderr.contains("multiple indexed zettels found for `@dup`"),
        "{stderr}"
    );
}

#[test]
fn zorg_path_reports_missing_or_stale_index() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source = root.join("main.z");
    std::fs::write(&source, "%%% @root #z/ref\nRoot\n%%%\n").expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");

    let missing_index = run_zorg(&[
        "path",
        "@root",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!missing_index.status.success());
    let stderr = String::from_utf8(missing_index.stderr).expect("index error should be utf8");
    assert!(stderr.contains("query index is missing"), "{stderr}");

    reindex(&root, &db);
    std::fs::write(
        &source,
        "%%% @root #z/ref\nRoot\n%%%\n\n- @root/new #z/ref New.\n",
    )
    .expect("write changed source");

    let stale_index = run_zorg(&[
        "path",
        "@root",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!stale_index.status.success());
    let stderr = String::from_utf8(stale_index.stderr).expect("stale error should be utf8");
    assert!(stderr.contains("query index is stale"), "{stderr}");
}

#[test]
fn zorg_promote_json_preview_does_not_write() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source = root.join("main.z");
    let original = "\
%%% @root #z/ref
Root
%%%

- @root/child #z/todo [ ] due::2026-05-15 Child title.
  Child paragraph.
";
    std::fs::write(&source, original).expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "promote",
        "@root/child",
        "--json",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected promote preview success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&source).expect("read source"),
        original
    );
    assert!(!root.join("root").join("child.z").exists());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("promote json should parse");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["plan"]["operation"], "promote");
    assert_eq!(value["plan"]["mode"], "preview");
    assert_eq!(value["plan"]["target_id"], "root/child");
    assert_eq!(value["plan"]["files"].as_array().expect("files").len(), 2);
    assert!(
        value["plan"]["files"]
            .as_array()
            .expect("files")
            .iter()
            .any(|file| file["root_relative_path"] == "root/child.z")
    );
}

#[test]
fn zorg_promote_write_creates_file_and_reindexes() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source = root.join("main.z");
    std::fs::write(
        &source,
        "\
%%% @root #z/ref
Root
%%%

- @root/child #z/todo [ ] due::2026-05-15 Child title.
  Child paragraph.

  - ^task #z/ref Task title.
    Task body.
",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "promote",
        "@root/child",
        "--write",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected promote write success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("root").join("child.z")).expect("read promoted source"),
        "\
%%% @root/child #z/todo [ ] due::2026-05-15
Child title.
%%%

Child paragraph.

- ^task #z/ref Task title.
  Task body.
"
    );
    assert!(
        !std::fs::read_to_string(&source)
            .expect("read source")
            .contains("@root/child")
    );

    reindex(&root, &db);
    let path = run_zorg(&[
        "path",
        "@root/child",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(
        path.status.success(),
        "expected promoted path success: stderr={}",
        String::from_utf8_lossy(&path.stderr)
    );
    let stdout = String::from_utf8(path.stdout).expect("path output should be utf8");
    assert!(stdout.contains("root/child.z:1:1 @root/child Child title."));
}

#[test]
fn zorg_promote_supports_explicit_destination() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref
Root
%%%

- @root/child #z/ref Child.
",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "promote",
        "@root/child",
        "--write",
        "--to",
        "notes/child.z",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected explicit destination success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(root.join("notes").join("child.z").exists());
}

#[test]
fn zorg_promote_refuses_collisions_outside_root_and_non_nested_targets() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(root.join("root")).expect("create corpus");
    std::fs::write(
        root.join("root").join("child.z"),
        "%%% @other #z/ref\nOther\n%%%\n",
    )
    .expect("write collision");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref
Root
%%%

- @root/child #z/ref Child.
",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let collision = run_zorg(&[
        "promote",
        "@root/child",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!collision.status.success());
    let stderr = String::from_utf8(collision.stderr).expect("collision error should be utf8");
    assert!(stderr.contains("destination"), "{stderr}");
    assert!(stderr.contains("already exists"), "{stderr}");

    let outside = run_zorg(&[
        "promote",
        "@root/child",
        "--to",
        "../child.z",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!outside.status.success());
    let stderr = String::from_utf8(outside.stderr).expect("outside error should be utf8");
    assert!(stderr.contains("outside corpus root"), "{stderr}");

    let non_nested = run_zorg(&[
        "promote",
        "@root",
        "--to",
        "root-promoted.z",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!non_nested.status.success());
    let stderr = String::from_utf8(non_nested.stderr).expect("non-nested error should be utf8");
    assert!(
        stderr.contains("only nested zettels can be promoted"),
        "{stderr}"
    );
}

#[test]
fn zorg_promote_refuses_unsafe_relative_reference_context() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref
Root
%%%

- @child #z/ref Child.
  See ~sibling.

- @root/sibling #z/ref Sibling.
",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "promote",
        "@child",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("relative error should be utf8");
    assert!(
        stderr.contains("promote plan would produce invalid source"),
        "{stderr}"
    );
    assert!(stderr.contains("requires the parent zettel"), "{stderr}");
}

#[test]
fn zorg_move_file_to_file_writes_and_reindexes() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("alpha.z"),
        "\
%%% @alpha #z/ref
Alpha
%%%

Alpha body.
",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "move",
        "@alpha",
        "--to",
        "moved/alpha.z",
        "--write",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected move write success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!root.join("alpha.z").exists());
    assert_eq!(
        std::fs::read_to_string(root.join("moved").join("alpha.z")).expect("read moved source"),
        "\
%%% @alpha #z/ref
Alpha
%%%

Alpha body.
"
    );

    reindex(&root, &db);
    let path = run_zorg(&[
        "path",
        "@alpha",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(
        path.status.success(),
        "expected moved path success: stderr={}",
        String::from_utf8_lossy(&path.stderr)
    );
    let stdout = String::from_utf8(path.stdout).expect("path output should be utf8");
    assert!(stdout.contains("moved/alpha.z:1:1 @alpha Alpha"));
}

#[test]
fn zorg_move_nested_to_file_writes_and_reindexes() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref
Root
%%%

- @root/child #z/ref Child.
  Child body.
",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "move",
        "@root/child",
        "--to",
        "child.z",
        "--write",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected nested move success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("child.z")).expect("read moved source"),
        "\
%%% @root/child #z/ref
Child.
%%%

Child body.
"
    );
    assert!(
        !std::fs::read_to_string(root.join("main.z"))
            .expect("read source")
            .contains("@root/child")
    );

    reindex(&root, &db);
    let path = run_zorg(&[
        "path",
        "@root/child",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(
        path.status.success(),
        "expected nested moved path success: stderr={}",
        String::from_utf8_lossy(&path.stderr)
    );
}

#[test]
fn zorg_move_nested_parent_change_json_preview_does_not_write() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source = root.join("main.z");
    let original = "\
%%% @root #z/ref
Root
%%%

- @root/source #z/ref Source.
  Source body.

- @root/dest #z/ref Dest.
";
    std::fs::write(&source, original).expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "move",
        "@root/source",
        "--to",
        "@root/dest",
        "--json",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected parent move preview success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&source).expect("read source"),
        original
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("move json should parse");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["plan"]["operation"], "move");
    assert_eq!(value["plan"]["mode"], "preview");
    assert_eq!(value["plan"]["target_id"], "root/source");
    assert_eq!(value["plan"]["files"].as_array().expect("files").len(), 1);
}

#[test]
fn zorg_move_nested_parent_change_writes_reindented_child() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source = root.join("main.z");
    std::fs::write(
        &source,
        "\
%%% @root #z/ref
Root
%%%

- @root/source #z/ref Source.
  Source body.

- @root/dest #z/ref Dest.
",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "move",
        "@root/source",
        "--to",
        "@root/dest",
        "--write",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected parent move write success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let moved = std::fs::read_to_string(&source).expect("read moved source");
    assert!(moved.contains("- @root/dest #z/ref Dest.\n  - @root/source #z/ref Source."));
    assert!(moved.contains("    Source body."));

    reindex(&root, &db);
    let check = run_zorg(&[
        "check",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(
        check.status.success(),
        "expected moved corpus check success: stderr={}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn zorg_move_refuses_collisions_outside_root_cycles_and_reports_noop() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(root.join("collision.z"), "%%% @other #z/ref\nOther\n%%%\n")
        .expect("write collision");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref
Root
%%%

- @root/parent #z/ref Parent.
  Parent body.

  - ^child #z/ref Child.
",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let collision = run_zorg(&[
        "move",
        "@root/parent",
        "--to",
        "collision.z",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!collision.status.success());
    let stderr = String::from_utf8(collision.stderr).expect("collision error should be utf8");
    assert!(stderr.contains("already exists"), "{stderr}");

    let outside = run_zorg(&[
        "move",
        "@root/parent",
        "--to",
        "../parent.z",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!outside.status.success());
    let stderr = String::from_utf8(outside.stderr).expect("outside error should be utf8");
    assert!(stderr.contains("outside corpus root"), "{stderr}");

    let cycle = run_zorg(&[
        "move",
        "@root/parent",
        "--to",
        "@root/parent/child",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!cycle.status.success());
    let stderr = String::from_utf8(cycle.stderr).expect("cycle error should be utf8");
    assert!(stderr.contains("descendant"), "{stderr}");

    let noop = run_zorg(&[
        "move",
        "@root/parent",
        "--to",
        "@root",
        "--json",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(noop.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&noop.stdout).expect("noop json should parse");
    assert_eq!(value["plan"]["files"].as_array().expect("files").len(), 0);
    assert_eq!(
        value["plan"]["warnings"]
            .as_array()
            .expect("warnings")
            .len(),
        1
    );
}

#[test]
fn zorg_extract_line_column_json_preview_does_not_write() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source = root.join("main.z");
    let original = "\
%%% @root #z/ref
Root
%%%

Alpha paragraph.

Beta paragraph.
";
    std::fs::write(&source, original).expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "extract",
        "--file",
        "main.z",
        "--range",
        "5:1-5:17",
        "--id",
        "@root/extracted",
        "--json",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected extract preview success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&source).expect("read source"),
        original
    );
    assert!(!root.join("root").join("extracted.z").exists());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("extract json should parse");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["plan"]["operation"], "extract");
    assert_eq!(value["plan"]["mode"], "preview");
    assert_eq!(value["plan"]["target_id"], "root/extracted");
    assert!(
        value["plan"]["files"]
            .as_array()
            .expect("files")
            .iter()
            .any(|file| file["root_relative_path"] == "root/extracted.z")
    );
}

#[test]
fn zorg_extract_byte_range_write_creates_file_and_reindexes() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source = root.join("main.z");
    let original = "\
%%% @root #z/ref
Root
%%%

Alpha paragraph.

Beta paragraph.
";
    std::fs::write(&source, original).expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);
    let start = original.find("Alpha").expect("alpha start");
    let end = start + "Alpha paragraph.".len();

    let output = run_zorg(&[
        "extract",
        "--file",
        source.to_str().expect("source should be utf8"),
        "--byte-range",
        &format!("{start}..{end}"),
        "--id",
        "@root/extracted",
        "--write",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected extract write success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&source).expect("read source"),
        "\
%%% @root #z/ref
Root
%%%

#root/extracted

Beta paragraph.
"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("root").join("extracted.z"))
            .expect("read extracted source"),
        "\
%%% @root/extracted
extracted
%%%

Alpha paragraph.
"
    );

    reindex(&root, &db);
    let path = run_zorg(&[
        "path",
        "@root/extracted",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(
        path.status.success(),
        "expected extracted path success: stderr={}",
        String::from_utf8_lossy(&path.stderr)
    );
}

#[test]
fn zorg_extract_refuses_invalid_utf8_collisions_and_crossing_boundaries() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(root.join("root")).expect("create corpus");
    std::fs::write(
        root.join("root").join("collision.z"),
        "%%% @other #z/ref\nOther\n%%%\n",
    )
    .expect("write collision");
    let source = root.join("main.z");
    let original = "\
%%% @root #z/ref
Root
%%%

Café paragraph.

- @root/child #z/ref Child.
  Child paragraph.
";
    std::fs::write(&source, original).expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);
    let cafe = original.find("Café").expect("cafe start");

    let invalid_utf8 = run_zorg(&[
        "extract",
        "--file",
        "main.z",
        "--byte-range",
        &format!("{cafe}..{}", cafe + 4),
        "--id",
        "@root/new",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!invalid_utf8.status.success());
    let stderr = String::from_utf8(invalid_utf8.stderr).expect("utf8 error should be utf8");
    assert!(stderr.contains("UTF-8 boundaries"), "{stderr}");

    let id_collision = run_zorg(&[
        "extract",
        "--file",
        "main.z",
        "--range",
        "5:1-5:16",
        "--id",
        "@root",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!id_collision.status.success());
    let stderr = String::from_utf8(id_collision.stderr).expect("id error should be utf8");
    assert!(stderr.contains("already exists"), "{stderr}");

    let destination_collision = run_zorg(&[
        "extract",
        "--file",
        "main.z",
        "--range",
        "5:1-5:16",
        "--id",
        "@root/collision",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!destination_collision.status.success());
    let stderr =
        String::from_utf8(destination_collision.stderr).expect("destination error should be utf8");
    assert!(stderr.contains("destination"), "{stderr}");
    assert!(stderr.contains("already exists"), "{stderr}");

    let crossing = run_zorg(&[
        "extract",
        "--file",
        "main.z",
        "--range",
        "5:1-8:19",
        "--id",
        "@root/crossing",
        "--replace-with-link",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!crossing.status.success());
    let stderr = String::from_utf8(crossing.stderr).expect("crossing error should be utf8");
    assert!(
        stderr.contains("must stay inside one paragraph"),
        "{stderr}"
    );
}

#[test]
fn zorg_watch_help_lists_stable_flags() {
    let output = run_zorg(&["watch", "--help"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("watch help should be utf8");
    assert!(stdout.contains("Usage: zorg watch"));
    assert!(stdout.contains("--root PATH"));
    assert!(stdout.contains("--db PATH"));
    assert!(stdout.contains("--debounce MS"));
    assert!(stdout.contains("--format text|json"));
    assert!(stdout.contains("--exit-after-ready"));
    assert!(stdout.contains("--once"));
}

#[test]
fn zorg_watch_rejects_invalid_args() {
    let output = run_zorg(&["watch", "--format", "xml", "--exit-after-ready"]);

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("watch error should be utf8");
    assert!(stderr.contains("--format must be text or json"), "{stderr}");
}

#[test]
fn zorg_watch_exit_after_ready_prints_text_states() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let db = temp.path().join("db").join("zorg.sqlite3");

    let output = run_zorg(&[
        "watch",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
        "--exit-after-ready",
    ]);

    assert!(
        output.status.success(),
        "expected watch success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("watch output should be utf8");
    assert!(stdout.contains("watch: starting"));
    assert!(stdout.contains("watch: ready"));
    assert!(stdout.contains("watch: stopped"));
    assert!(stdout.contains(&format!("root={}", root.display())));
    assert!(stdout.contains(&format!("database={}", db.display())));
}

#[test]
fn zorg_watch_once_emits_json_indexed_event() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(root.join("main.z"), "%%% @root #z/ref\nRoot\n%%%\n").expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");

    let output = run_zorg(&[
        "watch",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
        "--once",
        "--json",
    ]);

    assert!(
        output.status.success(),
        "expected watch success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("watch json should be utf8");
    let events = stdout
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("watch JSON line"))
        .collect::<Vec<_>>();
    assert!(events.iter().any(|event| event["state"] == "starting"));
    let indexed = events
        .iter()
        .find(|event| event["state"] == "indexed")
        .expect("indexed event");
    assert_eq!(indexed["schema_version"], 1);
    assert_eq!(indexed["root"], root.display().to_string());
    assert_eq!(indexed["database"], db.display().to_string());
    assert_eq!(indexed["summary"]["discovered_files"], 1);
    assert_eq!(indexed["summary"]["indexed_files"], 1);
    assert_eq!(indexed["summary"]["new_files"], 1);
    assert!(events.iter().any(|event| event["state"] == "stopped"));
}

#[test]
fn zorg_watch_reindexes_changed_file_during_bounded_run() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    let source = root.join("main.z");
    std::fs::write(&source, "%%% @root #z/ref\nRoot\n%%%\n").expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let mut child = spawn_zorg(&[
        "watch",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
        "--debounce",
        "25",
        "--json",
        "--exit-after-events",
        "1",
    ]);
    let mut stdout = BufReader::new(child.stdout.take().expect("watch stdout"));
    let mut output = wait_for_watch_state(&mut child, &mut stdout, "ready");

    std::fs::write(
        &source,
        "%%% @root #z/ref\nRoot\n%%%\n\n- @root/live #z/todo [ ] Added live.\n",
    )
    .expect("write live change");

    output.push_str(&read_remaining_watch_output(&mut child, &mut stdout));
    let events = output
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("watch JSON line"))
        .collect::<Vec<_>>();
    let indexed = events
        .iter()
        .find(|event| event["state"] == "indexed")
        .expect("indexed event after filesystem change");
    assert_eq!(indexed["summary"]["changed_files"], 1);
    assert!(events.iter().any(|event| event["state"] == "stopped"));

    let query = run_zorg(&[
        "query",
        "#z/todo",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(
        query.status.success(),
        "expected query success: stderr={}",
        String::from_utf8_lossy(&query.stderr)
    );
    let stdout = String::from_utf8(query.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@root/live"));
}

#[test]
fn zorg_query_runs_inline_swog_against_existing_index() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref area::work/zorg
Root fixture
%%%

Root body.

- @root/plan #z/todo [N] due::2026-05-15 area::work/zorg Plan next milestone.

  - ^task #z/todo [ ] do::2026-05-02 area::work/research Write implementation notes.

- @root/archive #z/ref [X] did::2026-05-01 area::archive Archive completed work.
",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");

    reindex(&root, &db);

    let tag_output = run_zorg(&[
        "query",
        "#z/todo",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(tag_output.status.success());
    let stdout = String::from_utf8(tag_output.stdout).expect("query output should be utf8");
    assert_eq!(
        stdout,
        "\
[ ] @root/plan/task  main.z  Write implementation notes.
[N] @root/plan       main.z  Plan next milestone.
"
    );

    let property_output = run_zorg(&[
        "query",
        "area:archive",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(property_output.status.success());
    let stdout = String::from_utf8(property_output.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@root/archive"));
    assert!(stdout.contains("Archive completed work."));

    let todo_output = run_zorg(&[
        "query",
        "todo:[ ]",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(todo_output.status.success());
    let stdout = String::from_utf8(todo_output.stdout).expect("query output should be utf8");
    assert!(stdout.contains("[ ] @root/plan/task"));
    assert!(!stdout.contains("@root/plan       "));

    let title_text_output = run_zorg(&[
        "query",
        "text:\"Root fixture\"",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(title_text_output.status.success());
    let stdout = String::from_utf8(title_text_output.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@root"));

    let body_text_output = run_zorg(&[
        "query",
        "text:\"Root body\"",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(body_text_output.status.success());
    let stdout = String::from_utf8(body_text_output.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@root"));

    let raw_text_output = run_zorg(&[
        "query",
        "text:\"Root fixture Root body\"",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(raw_text_output.status.success());
    let stdout = String::from_utf8(raw_text_output.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@root"));

    let empty_output = run_zorg(&[
        "query",
        "#area/missing",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(empty_output.status.success());
    assert!(empty_output.stdout.is_empty());

    let json_output = run_zorg(&[
        "query",
        "#z/todo",
        "--format",
        "json",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(json_output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&json_output.stdout).expect("query json should parse");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["kind"], "list");
    assert_eq!(value["query_source"], "inline");
    assert_eq!(value["query"], "#z/todo");
    assert_eq!(value["rows"].as_array().expect("rows array").len(), 2);
    assert_eq!(value["rows"][0]["canonical_id"], "root/plan/task");
    assert_eq!(value["rows"][0]["path"], "main.z");
    assert_eq!(value["rows"][0]["todo_marker"], "[ ]");
    assert_eq!(value["rows"][0]["source_order"], 2);
    assert!(value["rows"][0]["span"]["start_line"].is_number());
    assert!(
        value["rows"][0]["tags"]
            .as_array()
            .expect("tags array")
            .contains(&serde_json::Value::String("z/todo".to_owned()))
    );
    assert!(
        value["rows"][0]["properties"]
            .as_array()
            .expect("properties array")
            .iter()
            .any(|property| property["key"] == "area" && property["value"] == "work/research")
    );

    let empty_json_output = run_zorg(&[
        "query",
        "#area/missing",
        "--json",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(empty_json_output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&empty_json_output.stdout).expect("empty query json should parse");
    assert_eq!(value["rows"].as_array().expect("rows array").len(), 0);
}

#[test]
fn zorg_query_runs_query_zettel_by_id() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref
Root
%%%

- @queries/all #z/query title::All todos query::#z/todo
  Finds every todo.

- @queries/next #z/query title::Next todos
  ```swog
  #z/todo todo:[N]
  ```

- @tasks/open #z/todo [ ] Open task.
- @tasks/next #z/todo [N] Next task.
",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let property_output = run_zorg(&[
        "query",
        "--id",
        "@queries/all",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(property_output.status.success());
    let stdout = String::from_utf8(property_output.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@tasks/open"));
    assert!(stdout.contains("@tasks/next"));

    let fenced_output = run_zorg(&[
        "query",
        "--id",
        "@queries/next",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(fenced_output.status.success());
    let stdout = String::from_utf8(fenced_output.stdout).expect("query output should be utf8");
    assert!(stdout.contains("[N] @tasks/next"));
    assert!(!stdout.contains("@tasks/open"));

    let json_output = run_zorg(&[
        "query",
        "--id",
        "@queries/next",
        "--json",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(json_output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&json_output.stdout).expect("query id json should parse");
    assert_eq!(value["query_source"], "zettel");
    assert_eq!(value["query_zettel"]["id"], "queries/next");
    assert_eq!(value["query_zettel"]["path"], "main.z");
    assert_eq!(value["query_zettel"]["query"], "#z/todo todo:[N]");
    assert_eq!(value["rows"].as_array().expect("rows array").len(), 1);
    assert_eq!(value["rows"][0]["canonical_id"], "tasks/next");
}

#[test]
fn zorg_query_renders_table_output() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref
Root
%%%

- @queries/table #z/query title::Table query
  ```swog
  TABLE (#z/todo OR #z/query)
  ```

- @tasks/open #z/todo [ ] Open task.
- @tasks/next #z/todo [N] Next task.
",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let text_output = run_zorg(&[
        "query",
        "TABLE #z/todo",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(text_output.status.success());
    let stdout = String::from_utf8(text_output.stdout).expect("table output should be utf8");
    assert!(stdout.starts_with("Todo  ID"));
    assert!(stdout.contains("@tasks/open"));
    assert!(stdout.contains("@tasks/next"));

    let json_output = run_zorg(&[
        "query",
        "TABLE (#z/todo OR #z/query)",
        "--format",
        "json",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(json_output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&json_output.stdout).expect("table json should parse");
    assert_eq!(value["kind"], "table");
    assert_eq!(value["columns"][0]["key"], "todo");
    assert_eq!(value["columns"][1]["key"], "id");
    assert!(
        value["rows"]
            .as_array()
            .expect("table rows")
            .iter()
            .any(|row| row["id"] == "@tasks/open" && row["todo"] == "[ ]")
    );
    assert!(
        value["rows"]
            .as_array()
            .expect("table rows")
            .iter()
            .any(|row| row["id"] == "@queries/table")
    );

    let stored_output = run_zorg(&[
        "query",
        "--id",
        "@queries/table",
        "--json",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(stored_output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&stored_output.stdout).expect("stored table json should parse");
    assert_eq!(value["kind"], "table");
    assert_eq!(value["query_source"], "zettel");
    assert_eq!(
        value["query_zettel"]["query"],
        "TABLE (#z/todo OR #z/query)"
    );
}

#[test]
fn zorg_query_executes_fixture_query_zettel_by_id() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/corpus/query_and_template.z"),
        root.join("query_and_template.z"),
    )
    .expect("copy fixture");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "query",
        "--id",
        "@system/queries/today",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(output.status.success());
}

#[test]
fn zorg_query_runs_fixture_backed_end_to_end_sequence() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    copy_fixture("query_focus.z", &root);
    let db = temp.path().join("db").join("zorg.sqlite3");

    let parse_output = run_zorg(&[
        "parse",
        root.join("query_focus.z")
            .to_str()
            .expect("fixture path should be utf8"),
    ]);
    assert!(parse_output.status.success());
    let stdout = String::from_utf8(parse_output.stdout).expect("parse output should be utf8");
    assert!(stdout.contains("\"id\": \"query-fixture\""));
    assert!(stdout.contains("\"tag\": \"z/query\""));

    reindex(&root, &db);

    let inline_output = run_zorg(&[
        "query",
        "#z/todo -did:*",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(inline_output.status.success());
    let inline_stdout =
        String::from_utf8(inline_output.stdout).expect("query output should be utf8");
    assert_eq!(
        inline_stdout,
        "\
[ ] @query-fixture/inbox  query_focus.z  Inbox task with alpha text.
[N] @query-fixture/later  query_focus.z  Later task links to #query-fixture/reference.
"
    );

    let stored_output = run_zorg(&[
        "query",
        "--id",
        "@query-fixture/queries/daily",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(stored_output.status.success());
    let stored_stdout =
        String::from_utf8(stored_output.stdout).expect("query output should be utf8");
    assert_eq!(stored_stdout, inline_stdout);

    let link_output = run_zorg(&[
        "query",
        "links:#query-fixture/reference",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(link_output.status.success());
    let stdout = String::from_utf8(link_output.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@query-fixture"));
    assert!(stdout.contains("@query-fixture/later"));

    let text_output = run_zorg(&[
        "query",
        "file:query_focus.z text:\"alpha text\" #z/todo",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(text_output.status.success());
    let stdout = String::from_utf8(text_output.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@query-fixture/inbox"));
    assert!(!stdout.contains("@query-fixture/later"));

    let modified_output = run_zorg(&[
        "query",
        "modified:<7d",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(modified_output.status.success());
    let stdout = String::from_utf8(modified_output.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@query-fixture/queries/daily"));
}

#[test]
fn zorg_query_rejects_deferred_syntax_through_cli() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    copy_fixture("query_focus.z", &root);
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    assert_query_error(
        &root,
        &db,
        "TABLE todo,id #z/todo",
        "TABLE custom columns are not supported",
    );
    assert_query_error(
        &root,
        &db,
        "sum(#z/todo)",
        "aggregation functions other than count() are not supported",
    );
}

#[test]
fn zorg_query_renders_count_output() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref
Root
%%%

- @queries/count #z/query title::Count query
  ```swog
  count(#z/todo OR #z/query)
  ```
- @tasks/open #z/todo [ ] Open task.
- @tasks/next #z/todo [N] Next task.
",
    )
    .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let text_output = run_zorg(&[
        "query",
        "count(#z/todo)",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(text_output.status.success());
    let stdout = String::from_utf8(text_output.stdout).expect("count output should be utf8");
    assert_eq!(stdout, "count 2\n");

    let json_output = run_zorg(&[
        "query",
        "count(#z/todo OR #z/query)",
        "--format",
        "json",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(json_output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&json_output.stdout).expect("count json should parse");
    assert_eq!(value["kind"], "aggregate");
    assert_eq!(value["values"]["count"], 3);
    assert!(value.get("rows").is_none());

    let stored_output = run_zorg(&[
        "query",
        "--id",
        "@queries/count",
        "--json",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(stored_output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&stored_output.stdout).expect("stored count json should parse");
    assert_eq!(value["kind"], "aggregate");
    assert_eq!(value["query_source"], "zettel");
    assert_eq!(value["query_zettel"]["query"], "count(#z/todo OR #z/query)");
    assert_eq!(value["values"]["count"], 3);
}

#[test]
fn zorg_query_supports_boolean_expressions_through_cli() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref
Root
%%%

- @tasks/inbox #z/inbox #z/todo [ ] due::2026-05-02 Inbox task.
- @tasks/later #z/todo [N] due::2026-05-15 Later task.
- @tasks/done #z/todo [X] did::2026-05-01 Done task.
- @refs/note #z/ref Reference note.

- @queries/boolean #z/query title::Boolean query
  ```swog
  #z/inbox OR todo:[N]
  ```

- @queries/grouped #z/query title::Grouped query
  ```swog
  (#z/todo OR #z/ref) -did:*
  ```
",
    )
    .expect("write corpus");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let inline_output = run_zorg(&[
        "query",
        "#z/inbox OR todo:[N]",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(inline_output.status.success());
    let stdout = String::from_utf8(inline_output.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@tasks/inbox"));
    assert!(stdout.contains("@tasks/later"));
    assert!(!stdout.contains("@tasks/done"));

    let stored_output = run_zorg(&[
        "query",
        "--id",
        "@queries/boolean",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(stored_output.status.success());
    let stdout = String::from_utf8(stored_output.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@tasks/inbox"));
    assert!(stdout.contains("@tasks/later"));

    let grouped_output = run_zorg(&[
        "query",
        "--id",
        "@queries/grouped",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(grouped_output.status.success());
    let stdout = String::from_utf8(grouped_output.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@root"));
    assert!(stdout.contains("@tasks/inbox"));
    assert!(stdout.contains("@tasks/later"));
    assert!(stdout.contains("@refs/note"));
    assert!(!stdout.contains("@tasks/done"));
}

#[test]
fn zorg_query_reports_query_zettel_definition_errors() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref
Root
%%%

- @queries/none #z/query Missing definition.
- @queries/multi-prop #z/query query::#z/todo query::todo:[N]
- @queries/both #z/query query::#z/todo
  ```swog
  todo:[N]
  ```
- @queries/bad #z/query query::todo:[A]
- @ordinary #z/ref Not a query.
- @tasks/next #z/todo [N] Next task.
",
    )
    .expect("write source");
    std::fs::write(
        root.join("multi_swog.z"),
        "\
%%% @queries/multi-swog #z/query
Multiple swog definitions
%%%

```swog
#z/todo
```

```swog
todo:[N]
```
",
    )
    .expect("write multi-swog source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    assert_query_id_error(&root, &db, "@queries/missing", "not found");
    assert_query_id_error(&root, &db, "@ordinary", "not explicitly tagged #z/query");
    assert_query_id_error(&root, &db, "@queries/none", "has no query:: property");
    assert_query_id_error(
        &root,
        &db,
        "@queries/multi-prop",
        "multiple query:: properties",
    );
    assert_query_id_error(
        &root,
        &db,
        "@queries/multi-swog",
        "multiple fenced swog blocks",
    );
    assert_query_id_error(&root, &db, "@queries/both", "both query:: and fenced swog");
    assert_query_id_error(&root, &db, "@queries/bad", "invalid query definition");
    assert_query_id_error(&root, &db, "queries/bad", "invalid query zettel ID");
}

#[test]
fn zorg_query_reports_parse_errors_with_position() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(root.join("main.z"), "%%% @root #z/ref\nRoot fixture\n%%%\n")
        .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");
    reindex(&root, &db);

    let output = run_zorg(&[
        "query",
        "todo:[A]",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("error output should be utf8");
    assert!(stderr.contains("query parse failed"));
    assert!(stderr.contains("byte 0"));
    assert!(stderr.contains("todo filters"));
}

#[test]
fn zorg_query_requires_existing_current_index() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(root.join("main.z"), "%%% @root #z/ref\nRoot fixture\n%%%\n")
        .expect("write source");
    let db = temp.path().join("db").join("zorg.sqlite3");

    let missing_output = run_zorg(&[
        "query",
        "#z/ref",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!missing_output.status.success());
    let stderr = String::from_utf8(missing_output.stderr).expect("error output should be utf8");
    assert!(stderr.contains("query index is missing"));
    assert!(stderr.contains("zorg db reindex"));

    reindex(&root, &db);
    std::fs::write(
        root.join("added.z"),
        "%%% @added #z/ref\nAdded fixture\n%%%\n",
    )
    .expect("write added source");

    let stale_output = run_zorg(&[
        "query",
        "#z/ref",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!stale_output.status.success());
    let stderr = String::from_utf8(stale_output.stderr).expect("error output should be utf8");
    assert!(stderr.contains("query index is stale"));
    assert!(stderr.contains("zorg db reindex"));
}

#[test]
fn zorg_capture_creates_new_file_from_fenced_template() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    copy_fixture("query_and_template.z", &root);

    let output = run_zorg(&[
        "capture",
        "--template",
        "@system/templates/todo",
        "--title",
        "Write capture test",
        "--source",
        "cli",
        "--body",
        "Draft the smoke test.",
        "--id",
        "@tasks/capture-test",
        "--root",
        root.to_str().expect("root should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected capture success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("capture output should be utf8");
    let destination = root.join("inbox.z");
    assert!(stdout.contains(&format!("destination: {}", destination.display())));
    assert!(stdout.contains("zettel_id: @tasks/capture-test"));
    assert_eq!(
        std::fs::read_to_string(destination).expect("read capture destination"),
        format!(
            "\
%%% @inbox #z/ref
Captured zettel
%%%

- @tasks/capture-test #z/todo [ ] do::{} source::cli Write capture test
  Draft the smoke test.
",
            current_utc_date()
        )
    );
}

#[test]
fn zorg_capture_appends_child_to_existing_directory_zettel() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    let templates = root.join("system");
    let projects = root.join("projects");
    std::fs::create_dir_all(&templates).expect("create templates");
    std::fs::create_dir_all(&projects).expect("create projects");
    std::fs::write(
        templates.join("capture.z"),
        "\
%%% @templates #z/ref
Templates
%%%

- @templates/project #z/tmpl title::Project note dest::projects
  ```zorg-template
  - @{{id}} #z/ref {{title}}
  ```
",
    )
    .expect("write template");
    std::fs::write(
        projects.join("init.z"),
        "\
%%% @projects #z/ref
Projects
%%%
",
    )
    .expect("write directory zettel");

    let output = run_zorg(&[
        "capture",
        "--template",
        "Project",
        "--title",
        "Alpha Plan",
        "--root",
        root.to_str().expect("root should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected capture success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(projects.join("init.z")).expect("read appended zettel"),
        "\
%%% @projects #z/ref
Projects
%%%

- @alpha-plan #z/ref Alpha Plan
"
    );
}

#[test]
fn zorg_capture_refuses_existing_file_overwrite() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("template.z"),
        "\
%%% @templates #z/ref
Templates
%%%

- @templates/note #z/tmpl title::Note dest::note.z
  ```zorg-template
  - @{{id}} #z/ref {{title}}
  ```
",
    )
    .expect("write template");
    std::fs::write(root.join("note.z"), "%%% @note #z/ref\nExisting\n%%%\n")
        .expect("write existing destination");

    let output = run_zorg(&[
        "capture",
        "--template",
        "@templates/note",
        "--title",
        "New Note",
        "--root",
        root.to_str().expect("root should be utf8"),
    ]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("capture error should be utf8");
    assert!(stderr.contains("refusing to overwrite"));
}

#[test]
fn zorg_capture_rejects_missing_template_tag() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("template.z"),
        "\
%%% @templates #z/ref
Templates
%%%

- @templates/not-template #z/ref title::Not template dest::note.z
  ```zorg-template
  - @{{id}} #z/ref {{title}}
  ```
",
    )
    .expect("write non-template");

    let output = run_zorg(&[
        "capture",
        "--template",
        "@templates/not-template",
        "--title",
        "Nope",
        "--root",
        root.to_str().expect("root should be utf8"),
    ]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("capture error should be utf8");
    assert!(stderr.contains("not found or is not tagged #z/tmpl"));
}

#[test]
fn zorg_capture_rejects_destination_outside_root() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("template.z"),
        "\
%%% @templates #z/ref
Templates
%%%

- @templates/outside #z/tmpl title::Outside dest::../outside.z
  ```zorg-template
  - @{{id}} #z/ref {{title}}
  ```
",
    )
    .expect("write template");

    let output = run_zorg(&[
        "capture",
        "--template",
        "Outside",
        "--title",
        "Outside Note",
        "--root",
        root.to_str().expect("root should be utf8"),
    ]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("capture error should be utf8");
    assert!(stderr.contains("outside root"));
}

#[test]
fn zorg_capture_uses_body_template_content() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    std::fs::write(
        root.join("template.z"),
        "\
%%% @templates #z/ref
Templates
%%%

- @templates/body #z/tmpl title::Body template dest::body.z
  - @{{id}} #z/ref {{title}}
",
    )
    .expect("write body template");

    let output = run_zorg(&[
        "capture",
        "--template",
        "Body",
        "--title",
        "Body Capture",
        "--root",
        root.to_str().expect("root should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected capture success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("body.z")).expect("read body capture"),
        "\
%%% @body #z/ref
Captured zettel
%%%

- @body-capture #z/ref Body Capture
"
    );
}

#[test]
fn zorg_capture_json_success_and_missing_input_error_are_stable() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    copy_fixture("query_and_template.z", &root);

    let output = run_zorg(&[
        "capture",
        "--json",
        "--template",
        "@system/templates/todo",
        "--title",
        "JSON capture",
        "--source",
        "cli",
        "--body",
        "Exercise JSON output.",
        "--id",
        "@tasks/json-capture",
        "--root",
        root.to_str().expect("root should be utf8"),
    ]);

    assert!(
        output.status.success(),
        "expected capture success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("capture json should parse");
    assert_eq!(
        value["destination"],
        root.join("inbox.z").display().to_string()
    );
    assert_eq!(value["zettel_id"], "@tasks/json-capture");

    let missing = run_zorg(&[
        "capture",
        "--json",
        "--root",
        root.to_str().expect("root should be utf8"),
    ]);
    assert!(!missing.status.success());
    assert!(missing.stderr.is_empty());
    let error: serde_json::Value =
        serde_json::from_slice(&missing.stdout).expect("capture error json should parse");
    assert_eq!(error["code"], "capture.missing_inputs");
    assert!(
        error["error"]
            .as_str()
            .expect("error string")
            .contains("--template")
    );
}

#[test]
fn zorg_capture_query_fix_end_to_end_loop_is_healthy() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create corpus");
    copy_fixture("query_and_template.z", &root);
    let db = temp.path().join("db").join("zorg.sqlite3");

    reindex(&root, &db);

    let capture = run_zorg(&[
        "capture",
        "--json",
        "--template",
        "@system/templates/todo",
        "--title",
        "End to end capture",
        "--source",
        "smoke",
        "--body",
        "Verify the loop.",
        "--id",
        "@tasks/e2e-capture",
        "--root",
        root.to_str().expect("root should be utf8"),
    ]);
    assert!(
        capture.status.success(),
        "expected capture success: stderr={}",
        String::from_utf8_lossy(&capture.stderr)
    );

    reindex(&root, &db);

    let query = run_zorg(&[
        "query",
        "#z/todo",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(query.status.success());
    let stdout = String::from_utf8(query.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@tasks/e2e-capture"));
    assert!(stdout.contains("End to end capture"));

    let fix = run_zorg(&["fix", "--root", root.to_str().expect("root should be utf8")]);
    assert!(
        fix.status.success(),
        "expected fix success: stderr={}",
        String::from_utf8_lossy(&fix.stderr)
    );

    let check = run_zorg(&[
        "fix",
        "--check",
        "--root",
        root.to_str().expect("root should be utf8"),
    ]);
    assert!(
        check.status.success(),
        "expected fixed corpus to pass --check: stderr={}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn import_legacy_plan_human_output_is_read_only_and_deterministic_for_multiple_files() {
    let output = run_zorg(&[
        "import",
        "legacy",
        "plan",
        "fixtures/import_export/legacy/notes/project.zo",
        "fixtures/import_export/legacy/queries/open.zoq",
        "fixtures/import_export/legacy/templates/todo.zot",
        "--dest",
        "imported",
    ]);

    assert!(
        output.status.success(),
        "expected import plan success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("import plan output should be utf8");
    assert!(stdout.contains("import legacy plan: planned=3 lossy=1 unsupported=0 fatal=0"));
    assert!(stdout.contains(
        "fixtures/import_export/legacy/notes/project.zo -> imported/legacy/project.z planned"
    ));
    assert!(stdout.contains(
        "fixtures/import_export/legacy/queries/open.zoq -> imported/legacy/query/open.z planned"
    ));
    assert!(stdout.contains(
        "fixtures/import_export/legacy/templates/todo.zot -> imported/legacy/templates/todo.z planned"
    ));
    assert!(stdout.contains(
        "warning: fixtures/import_export/legacy/notes/project.zo:5: legacy.tick_history_collapsed"
    ));
}

#[test]
fn import_legacy_plan_json_output_is_valid_for_lossy_success() {
    let output = run_zorg(&[
        "import",
        "legacy",
        "plan",
        "fixtures/import_export/legacy/notes/project.zo",
        "--format",
        "json",
    ]);

    assert!(
        output.status.success(),
        "expected import plan JSON success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("import plan JSON should parse");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["command"], "import legacy plan");
    assert_eq!(value["mode"], "plan");
    assert_eq!(value["summary"]["planned"], 1);
    assert_eq!(value["summary"]["lossy"], 1);
    assert_eq!(value["summary"]["fatal"], 0);
    assert_eq!(
        value["outputs"][0]["root_relative_path"],
        "legacy/project.z"
    );
    assert!(value["outputs"][0].get("generated_content").is_none());
}

#[test]
fn import_legacy_plan_json_output_exits_one_for_fatal_plan() {
    let output = run_zorg(&[
        "import",
        "legacy",
        "plan",
        "fixtures/import_export/legacy/collisions/alpha.zo",
        "fixtures/import_export/legacy/collisions/beta.zo",
        "--json",
    ]);

    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("fatal import plan JSON should parse");
    assert_eq!(value["summary"]["fatal"], 2);
    assert_eq!(value["outputs"].as_array().expect("outputs array").len(), 0);
    assert_eq!(
        value["collisions"]
            .as_array()
            .expect("collisions array")
            .len(),
        1
    );
}

#[test]
fn import_legacy_plan_directory_inputs_are_sorted() {
    let output = run_zorg(&[
        "import",
        "legacy",
        "plan",
        "fixtures/import_export/legacy",
        "--format",
        "json",
    ]);

    assert_eq!(output.status.code(), Some(1));
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("directory import plan JSON should parse");
    let input_paths = value["inputs"]
        .as_array()
        .expect("inputs array")
        .iter()
        .map(|input| input["path"].as_str().expect("input path"))
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
fn import_legacy_plan_root_checks_existing_output_without_writing() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(root.join("legacy")).expect("create collision dir");
    let existing = root.join("legacy/project.z");
    std::fs::write(&existing, "existing").expect("write existing output");

    let output = run_zorg(&[
        "import",
        "legacy",
        "plan",
        "fixtures/import_export/legacy/notes/project.zo",
        "--root",
        root.to_str().expect("root utf8"),
        "--format",
        "json",
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        std::fs::read_to_string(&existing).expect("existing output should remain"),
        "existing"
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("collision import plan JSON should parse");
    assert_eq!(value["summary"]["fatal"], 1);
    assert!(
        value["diagnostics"]
            .as_array()
            .expect("diagnostics array")
            .iter()
            .any(|diagnostic| diagnostic["code"] == "legacy.output_exists")
    );
}

#[test]
fn import_legacy_plan_usage_errors_exit_two() {
    let output = run_zorg(&["import", "legacy", "plan", "--format", "yaml"]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("usage error should be utf8");
    assert!(stderr.contains("unsupported import output format"));
}

#[test]
fn export_markdown_id_and_subtree_stdout_use_current_index() {
    let temp = TempWorkspace::new();
    let (root, db) = write_export_markdown_fixture(&temp);
    reindex(&root, &db);

    let single = run_zorg(&[
        "export",
        "markdown",
        "--id",
        "@root",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert!(
        single.status.success(),
        "expected single export: stderr={}",
        String::from_utf8_lossy(&single.stderr)
    );
    let stdout = String::from_utf8(single.stdout).expect("markdown stdout should be utf8");
    assert!(stdout.contains("# Root title"));
    assert!(stdout.contains("Root body links to #tasks/open."));
    assert!(!stdout.contains("## @tasks/open"));

    let subtree = run_zorg(&[
        "export",
        "markdown",
        "--subtree",
        "@root",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert!(
        subtree.status.success(),
        "expected subtree export: stderr={}",
        String::from_utf8_lossy(&subtree.stderr)
    );
    let stdout = String::from_utf8(subtree.stdout).expect("markdown stdout should be utf8");
    assert!(stdout.contains("# Root title"));
    assert!(stdout.contains("## @tasks/open [ ]"));
    assert!(stdout.contains("zorg:#tasks/open"));
}

#[test]
fn export_markdown_query_selectors_json_and_out_dir_are_stable() {
    let temp = TempWorkspace::new();
    let (root, db) = write_export_markdown_fixture(&temp);
    reindex(&root, &db);

    let inline = run_zorg(&[
        "export",
        "markdown",
        "--query",
        "#z/todo",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert!(
        inline.status.success(),
        "expected inline query export: stderr={}",
        String::from_utf8_lossy(&inline.stderr)
    );
    let stdout = String::from_utf8(inline.stdout).expect("markdown stdout should be utf8");
    let open_index = stdout.find("# Open task").expect("open task heading");
    let next_index = stdout.find("# Next task").expect("next task heading");
    assert!(open_index < next_index, "query order should be preserved");
    assert!(stdout.contains("\n---\n"));

    let out_dir = temp.path().join("markdown");
    let stored = run_zorg(&[
        "export",
        "markdown",
        "--query-id",
        "@queries/open",
        "--out",
        out_dir.to_str().expect("out dir utf8"),
        "--json",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert!(
        stored.status.success(),
        "expected query-id export: stderr={}",
        String::from_utf8_lossy(&stored.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&stored.stdout).expect("export json should parse");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["command"], "export markdown");
    assert_eq!(value["selection"]["kind"], "query_id");
    assert_eq!(value["selection"]["canonical_id"], "queries/open");
    assert_eq!(value["summary"]["rendered"], 2);
    assert!(value["items"][0].get("markdown").is_none());
    assert_eq!(
        collect_relative_files(&out_dir),
        vec!["tasks/next.md", "tasks/open.md"]
    );
}

#[test]
fn export_markdown_json_reports_lossy_links_without_bodies() {
    let temp = TempWorkspace::new();
    let (root, db) = write_export_markdown_fixture(&temp);
    reindex(&root, &db);

    let output = run_zorg(&[
        "export",
        "markdown",
        "--id",
        "@tasks/open",
        "--json",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert!(
        output.status.success(),
        "expected lossy export JSON success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("export JSON should parse");
    assert_eq!(value["output"]["mode"], "stdout");
    assert_eq!(value["summary"]["lossy"], 1);
    assert_eq!(value["diagnostics"][0]["code"], "markdown.link_unresolved");
    assert!(value["items"][0].get("markdown").is_none());
}

#[test]
fn export_markdown_reports_empty_selection_and_index_problems() {
    let temp = TempWorkspace::new();
    let (root, db) = write_export_markdown_fixture(&temp);

    let missing_index = run_zorg(&[
        "export",
        "markdown",
        "--id",
        "@root",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert_eq!(missing_index.status.code(), Some(1));
    let stderr = String::from_utf8(missing_index.stderr).expect("stderr utf8");
    assert!(stderr.contains("query index is missing"));

    reindex(&root, &db);
    let empty = run_zorg(&[
        "export",
        "markdown",
        "--query",
        "#area/missing",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert_eq!(empty.status.code(), Some(1));
    let stderr = String::from_utf8(empty.stderr).expect("stderr utf8");
    assert!(stderr.contains("selected no zettels"));

    std::fs::write(root.join("extra.z"), "%%% @extra #z/ref\nExtra\n%%%\n")
        .expect("write stale source");
    let stale = run_zorg(&[
        "export",
        "markdown",
        "--id",
        "@root",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert_eq!(stale.status.code(), Some(1));
    let stderr = String::from_utf8(stale.stderr).expect("stderr utf8");
    assert!(stderr.contains("query index is stale"));
}

#[test]
fn import_legacy_apply_writes_expected_z_outputs_and_temp_root_is_queryable() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    let db = temp.path().join("db").join("zorg.sqlite3");

    let output = run_zorg(&[
        "import",
        "legacy",
        "apply",
        "fixtures/import_export/legacy/notes/project.zo",
        "fixtures/import_export/legacy/queries/open.zoq",
        "fixtures/import_export/legacy/templates/todo.zot",
        "--root",
        root.to_str().expect("root utf8"),
        "--dest",
        "imported",
        "--json",
    ]);

    assert!(
        output.status.success(),
        "expected import apply success: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("import apply JSON should parse");
    assert_eq!(value["command"], "import legacy apply");
    assert_eq!(value["mode"], "apply");
    assert_eq!(value["summary"]["fatal"], 0);
    assert_eq!(
        value["write_results"]
            .as_array()
            .expect("write results")
            .len(),
        3
    );
    assert!(
        value["write_results"]
            .as_array()
            .expect("write results")
            .iter()
            .all(|result| result["status"] == "written")
    );

    assert_eq!(
        collect_relative_files(&root),
        vec![
            "imported/legacy/project.z",
            "imported/legacy/query/open.z",
            "imported/legacy/templates/todo.z",
        ]
    );

    let check = run_zorg(&["check", "--root", root.to_str().expect("root utf8")]);
    assert!(
        check.status.success(),
        "expected written import output to pass check: stderr={}",
        String::from_utf8_lossy(&check.stderr)
    );

    reindex(&root, &db);
    let query = run_zorg(&[
        "query",
        "#z/todo",
        "--root",
        root.to_str().expect("root utf8"),
        "--db",
        db.to_str().expect("db utf8"),
    ]);
    assert!(
        query.status.success(),
        "expected query over imported output: stderr={}",
        String::from_utf8_lossy(&query.stderr)
    );
    let stdout = String::from_utf8(query.stdout).expect("query output should be utf8");
    assert!(stdout.contains("@legacy/project/follow-up"));
}

#[test]
fn import_legacy_apply_refuses_overwrites_without_writing() {
    let temp = TempWorkspace::new();
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(root.join("legacy")).expect("create collision dir");
    let existing = root.join("legacy/project.z");
    std::fs::write(&existing, "existing").expect("write existing output");

    let output = run_zorg(&[
        "import",
        "legacy",
        "apply",
        "fixtures/import_export/legacy/notes/project.zo",
        "--root",
        root.to_str().expect("root utf8"),
        "--json",
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        std::fs::read_to_string(&existing).expect("existing output should remain"),
        "existing"
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("collision apply JSON should parse");
    assert_eq!(value["command"], "import legacy apply");
    assert_eq!(value["mode"], "apply");
    assert_eq!(value["summary"]["fatal"], 1);
    assert_eq!(
        value["write_results"]
            .as_array()
            .expect("write results")
            .len(),
        0
    );
}

#[test]
fn zorg_parse_rejects_explicit_unsupported_source_path() {
    let fixture = format!(
        "{}/../../fixtures/corpus/legacy.zo",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = Command::new(env!("CARGO_BIN_EXE_zorg"))
        .args(["parse", fixture.as_str()])
        .output()
        .expect("run zorg parse");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("error output should be utf8");
    assert!(stderr.contains("unsupported source path"));
    assert!(stderr.contains(".zo is not a canonical"));
    assert!(stderr.contains("expected .z"));
}

fn run_zorg(args: &[&str]) -> std::process::Output {
    zorg_command().args(args).output().expect("run zorg")
}

fn run_zorg_with_env(args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut command = zorg_command();
    for (key, value) in envs {
        command.env(key, value);
    }
    command.args(args).output().expect("run zorg")
}

fn spawn_zorg(args: &[&str]) -> Child {
    let mut command = zorg_command();
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn zorg")
}

fn zorg_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zorg"));
    command
        .env_remove("ZORG_ROOT")
        .env_remove("ZORG_DATABASE_PATH")
        .env_remove("ZORG_DB")
        .env_remove("ZORG_WATCHER_DEBOUNCE_MS")
        .env_remove("ZORG_WATCHER_LOG_PATH")
        .env_remove("NO_COLOR");
    command
}

fn wait_for_watch_state(
    child: &mut Child,
    stdout: &mut BufReader<std::process::ChildStdout>,
    state: &str,
) -> String {
    let mut output = String::new();
    for _ in 0..16 {
        let mut line = String::new();
        let read = stdout.read_line(&mut line).expect("read watch line");
        if read == 0 {
            break;
        }
        output.push_str(&line);
        if serde_json::from_str::<serde_json::Value>(&line)
            .ok()
            .is_some_and(|event| event["state"] == state)
        {
            return output;
        }
    }

    kill_child(child);
    panic!("watch process did not emit {state:?}; output was {output:?}");
}

fn read_remaining_watch_output(
    child: &mut Child,
    stdout: &mut BufReader<std::process::ChildStdout>,
) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().expect("poll watch process") {
            let mut output = String::new();
            stdout
                .read_to_string(&mut output)
                .expect("read remaining watch output");
            assert!(status.success(), "watch exited with {status}: {output}");
            return output;
        }
        if std::time::Instant::now() >= deadline {
            kill_child(child);
            panic!("watch process did not exit after filesystem event");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn kill_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn copy_fixture(name: &str, root: &Path) {
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/corpus")
            .join(name),
        root.join(name),
    )
    .expect("copy fixture");
}

fn write_export_markdown_fixture(temp: &TempWorkspace) -> (PathBuf, PathBuf) {
    let root = temp.path().join("corpus");
    std::fs::create_dir_all(&root).expect("create export corpus");
    std::fs::write(
        root.join("main.z"),
        "\
%%% @root #z/ref area::work
Root title
%%%

Root body links to #tasks/open.

- @queries/open #z/query title::Open tasks query::#z/todo

- @tasks/open #z/todo [ ] Open task.
  Open task body links to +missing.

- @tasks/next #z/todo [N] Next task.
",
    )
    .expect("write export fixture");
    let db = temp.path().join("db").join("zorg.sqlite3");
    (root, db)
}

fn collect_relative_files(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_relative_files_inner(root, root, &mut files);
    files.sort();
    files
}

fn collect_relative_files_inner(root: &Path, path: &Path, files: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries {
        let entry = entry.expect("read dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_relative_files_inner(root, &path, files);
        } else {
            files.push(
                path.strip_prefix(root)
                    .expect("file under root")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
}

fn reindex(root: &Path, db: &Path) {
    let output = run_zorg(&[
        "db",
        "reindex",
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(output.status.success());
}

fn assert_query_error(root: &Path, db: &Path, query: &str, expected: &str) {
    let output = run_zorg(&[
        "query",
        query,
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!output.status.success(), "{query} should fail");
    let stderr = String::from_utf8(output.stderr).expect("error output should be utf8");
    assert!(
        stderr.contains(expected),
        "expected stderr for {query} to contain {expected:?}, got {stderr:?}"
    );
}

fn assert_query_id_error(root: &Path, db: &Path, query_id: &str, expected: &str) {
    let output = run_zorg(&[
        "query",
        "--id",
        query_id,
        "--root",
        root.to_str().expect("root should be utf8"),
        "--db",
        db.to_str().expect("db should be utf8"),
    ]);
    assert!(!output.status.success(), "{query_id} should fail");
    let stderr = String::from_utf8(output.stderr).expect("error output should be utf8");
    assert!(
        stderr.contains(expected),
        "expected stderr for {query_id} to contain {expected:?}, got {stderr:?}"
    );
    if query_id.starts_with('@') && !query_id.contains("missing") {
        assert!(stderr.contains(query_id));
        assert!(stderr.contains(".z"));
    }
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
