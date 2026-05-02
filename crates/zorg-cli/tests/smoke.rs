use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

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
