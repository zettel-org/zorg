use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
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
  TABLE #z/todo
  ```
",
    )
    .expect("write invalid query");

    let output = run_zorg(&["check", "--root", root.to_str().expect("root utf8")]);
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("check output should be utf8");
    assert!(stderr.contains("query.definition"), "{stderr}");
    assert!(stderr.contains("invalid query definition"), "{stderr}");
    assert!(stderr.contains("TABLE output is not supported"), "{stderr}");
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

    let output = Command::new(env!("CARGO_BIN_EXE_zorg"))
        .args([
            "db",
            "status",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--db",
            db.to_str().expect("db should be utf8"),
        ])
        .output()
        .expect("run zorg db status");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("status output should be utf8");
    assert!(stdout.contains(&format!("root: {}", root.display())));
    assert!(stdout.contains(&format!("database: {}", db.display())));
    assert!(stdout.contains("schema_version: 1"));
    assert!(stdout.contains("discovered_files: 2"));
    assert!(stdout.contains("indexed_files: 0"));
    assert!(stdout.contains("new_files: 2"));
    assert!(stdout.contains("changed_files: 0"));
    assert!(stdout.contains("deleted_files: 0"));
    assert!(stdout.contains("last_indexed_at_unix_ms: never"));
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

    let output = Command::new(env!("CARGO_BIN_EXE_zorg"))
        .args([
            "db",
            "reindex",
            "--root",
            root.to_str().expect("root should be utf8"),
            "--db",
            db.to_str().expect("db should be utf8"),
        ])
        .output()
        .expect("run zorg db reindex");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("reindex output should be utf8");
    assert!(stdout.contains("discovered_files: 1"));
    assert!(stdout.contains("indexed_files: 1"));
    assert!(stdout.contains("new_files: 1"));
    assert!(stdout.contains("indexed_zettel: 1"));
    assert!(stdout.contains("reindex: incremental complete"));
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
        "file:query_focus.z text:\"alpha text\"",
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

    assert_query_error(&root, &db, "TABLE #z/todo", "TABLE output is not supported");
    assert_query_error(
        &root,
        &db,
        "#z/todo OR #z/ref",
        "OR expressions are not supported",
    );
    assert_query_error(
        &root,
        &db,
        "count()",
        "count() aggregation is not supported",
    );
    assert_query_error(&root, &db, "(#z/todo)", "parenthesized groups");
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
    Command::new(env!("CARGO_BIN_EXE_zorg"))
        .args(args)
        .output()
        .expect("run zorg")
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
