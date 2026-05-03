use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    fn new() -> Self {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("zorg-cli-mvp-e2e-{}-{counter}", std::process::id()));
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
fn mvp_e2e_cli_uses_isolated_root_and_explicit_database() {
    let temp = TempWorkspace::new();
    let fake_home = temp.path().join("real-home");
    let root = temp.path().join("workspace-home").join("zorg");
    let db = temp.path().join("db").join("zorg.sqlite3");
    std::fs::create_dir_all(&fake_home).expect("create fake home");
    std::fs::create_dir_all(&root).expect("create temp zorg root");
    copy_fixture_corpus(&root);

    let minimal = root.join("minimal.z");
    let parse_minimal = run_zorg(&fake_home, &["parse", path_str(&minimal)]);
    assert_success(&parse_minimal, "parse minimal fixture");
    let parsed: Value =
        serde_json::from_slice(&parse_minimal.stdout).expect("parse output should be json");
    assert_eq!(parsed["root"]["id"], "minimal");
    assert_eq!(parsed["diagnostics"], Value::Array(Vec::new()));

    let query_fixture = root.join("query_focus.z");
    let parse_query = run_zorg(&fake_home, &["parse", path_str(&query_fixture)]);
    assert_success(&parse_query, "parse query fixture");
    let stdout = String::from_utf8(parse_query.stdout).expect("parse output should be utf8");
    assert!(stdout.contains("\"tag\": \"z/query\""));

    let legacy_path = root.join("legacy.zo");
    std::fs::write(&legacy_path, "ID:: legacy\n").expect("write legacy source");
    let legacy_path_output = run_zorg(&fake_home, &["parse", path_str(&legacy_path)]);
    assert!(!legacy_path_output.status.success());
    let stderr =
        String::from_utf8(legacy_path_output.stderr).expect("legacy path error should be utf8");
    assert!(stderr.contains("unsupported source path"));
    assert!(stderr.contains("expected .z"));

    let strict_legacy = root.join("legacy_invalid.z");
    std::fs::write(&strict_legacy, "ID:: legacy\n").expect("write strict legacy source");
    let strict_output = run_zorg(&fake_home, &["check", path_str(&strict_legacy)]);
    assert!(!strict_output.status.success());
    let stderr =
        String::from_utf8(strict_output.stderr).expect("legacy check error should be utf8");
    assert!(stderr.contains("legacy.unsupported"));
    assert!(stderr.contains("ID::"));
    std::fs::remove_file(&strict_legacy).expect("remove negative fixture from valid root");

    let check = run_zorg(&fake_home, &["check", "--root", path_str(&root)]);
    assert_success(&check, "check valid copied corpus");

    let reindex = run_zorg(
        &fake_home,
        &[
            "db",
            "reindex",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&reindex, "reindex explicit database");
    assert!(db.exists(), "expected explicit database to be created");
    assert!(
        !root.join(".zorg").exists(),
        "explicit --db must not create a default database under the corpus root"
    );

    let path_json = run_zorg(
        &fake_home,
        &[
            "path",
            "@minimal",
            "--format",
            "json",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&path_json, "resolve zettel path as json");
    let path_value: Value =
        serde_json::from_slice(&path_json.stdout).expect("path output should be json");
    assert_eq!(path_value["schema_version"], 1);
    assert_eq!(path_value["command"], "path");
    assert_eq!(path_value["canonical_id"], "minimal");
    assert_eq!(path_value["root_relative_path"], "minimal.z");

    let refactor_source = root.join("refactor.z");
    std::fs::write(
        &refactor_source,
        "\
%%% @refactor #z/ref
Refactor root
%%%

- @refactor/promote #z/ref Promote target.
  Promote body.

Extract this paragraph.

Leave this paragraph.
",
    )
    .expect("write refactor fixture");
    let reindex_refactor = run_zorg(
        &fake_home,
        &[
            "db",
            "reindex",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&reindex_refactor, "reindex refactor fixture");

    let promote_preview = run_zorg(
        &fake_home,
        &[
            "promote",
            "@refactor/promote",
            "--json",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&promote_preview, "preview promote as json");
    let promote_value: Value =
        serde_json::from_slice(&promote_preview.stdout).expect("promote output should be json");
    assert_eq!(promote_value["schema_version"], 1);
    assert_eq!(promote_value["plan"]["operation"], "promote");
    assert_eq!(promote_value["plan"]["mode"], "preview");
    assert_eq!(promote_value["plan"]["target_id"], "refactor/promote");
    assert!(
        !root.join("refactor").join("promote.z").exists(),
        "preview mode must not write promoted files"
    );

    let promote_write = run_zorg(
        &fake_home,
        &[
            "promote",
            "@refactor/promote",
            "--write",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&promote_write, "write promote refactor");
    assert!(root.join("refactor").join("promote.z").exists());

    let reindex_after_promote = run_zorg(
        &fake_home,
        &[
            "db",
            "reindex",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&reindex_after_promote, "reindex after promote");

    let move_preview = run_zorg(
        &fake_home,
        &[
            "move",
            "@refactor/promote",
            "--to",
            "archive/promoted.z",
            "--json",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&move_preview, "preview move as json");
    let move_value: Value =
        serde_json::from_slice(&move_preview.stdout).expect("move output should be json");
    assert_eq!(move_value["schema_version"], 1);
    assert_eq!(move_value["plan"]["operation"], "move");
    assert_eq!(move_value["plan"]["mode"], "preview");
    assert_eq!(move_value["plan"]["target_id"], "refactor/promote");
    assert!(
        !root.join("archive").join("promoted.z").exists(),
        "preview mode must not write moved files"
    );

    let move_write = run_zorg(
        &fake_home,
        &[
            "move",
            "@refactor/promote",
            "--to",
            "archive/promoted.z",
            "--write",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&move_write, "write move refactor");
    assert!(!root.join("refactor").join("promote.z").exists());
    assert!(root.join("archive").join("promoted.z").exists());

    let reindex_after_move = run_zorg(
        &fake_home,
        &[
            "db",
            "reindex",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&reindex_after_move, "reindex after move");

    let source_before_extract =
        std::fs::read_to_string(&refactor_source).expect("read source before extract");
    let extract_start = source_before_extract
        .find("Extract this paragraph.")
        .expect("extract paragraph is present");
    let extract_end = extract_start + "Extract this paragraph.".len();
    let extract_range = format!("{extract_start}..{extract_end}");

    let extract_preview = run_zorg(
        &fake_home,
        &[
            "extract",
            "--file",
            "refactor.z",
            "--byte-range",
            &extract_range,
            "--id",
            "@refactor/extracted",
            "--json",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&extract_preview, "preview extract as json");
    let extract_value: Value =
        serde_json::from_slice(&extract_preview.stdout).expect("extract output should be json");
    assert_eq!(extract_value["schema_version"], 1);
    assert_eq!(extract_value["plan"]["operation"], "extract");
    assert_eq!(extract_value["plan"]["mode"], "preview");
    assert_eq!(extract_value["plan"]["target_id"], "refactor/extracted");
    assert!(
        !root.join("refactor").join("extracted.z").exists(),
        "preview mode must not write extracted files"
    );

    let extract_write = run_zorg(
        &fake_home,
        &[
            "extract",
            "--file",
            "refactor.z",
            "--byte-range",
            &extract_range,
            "--id",
            "@refactor/extracted",
            "--write",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&extract_write, "write extract refactor");
    assert!(root.join("refactor").join("extracted.z").exists());
    assert!(
        std::fs::read_to_string(&refactor_source)
            .expect("read source after extract")
            .contains("#refactor/extracted")
    );

    let check_after_refactors = run_zorg(&fake_home, &["check", "--root", path_str(&root)]);
    assert_success(&check_after_refactors, "check corpus after refactors");

    let reindex_after_refactors = run_zorg(
        &fake_home,
        &[
            "db",
            "reindex",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&reindex_after_refactors, "reindex after refactors");

    let refactor_query = run_zorg(
        &fake_home,
        &[
            "query",
            "#z/ref",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&refactor_query, "query after refactors");
    let refactor_stdout = String::from_utf8(refactor_query.stdout).expect("query output utf8");
    assert!(refactor_stdout.contains("@refactor/promote"));

    let extracted_path = run_zorg(
        &fake_home,
        &[
            "path",
            "@refactor/extracted",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&extracted_path, "path after extract refactor");

    let inline_query = run_zorg(
        &fake_home,
        &[
            "query",
            "#z/todo -did:*",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&inline_query, "run inline SWOG query");
    let inline_stdout = String::from_utf8(inline_query.stdout).expect("query output utf8");
    assert!(inline_stdout.contains("@query-fixture/inbox"));
    assert!(inline_stdout.contains("@project/plan"));
    assert!(!inline_stdout.contains("@query-fixture/done"));

    let stored_query = run_zorg(
        &fake_home,
        &[
            "query",
            "--id",
            "@query-fixture/queries/daily",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&stored_query, "run query zettel by id");
    let stored_stdout = String::from_utf8(stored_query.stdout).expect("query output utf8");
    assert!(stored_stdout.contains("@query-fixture/inbox"));
    assert!(!stored_stdout.contains("@query-fixture/done"));

    let capture = run_zorg(
        &fake_home,
        &[
            "capture",
            "--json",
            "--template",
            "@system/templates/todo",
            "--title",
            "MVP captured task",
            "--source",
            "mvp-e2e",
            "--body",
            "Validate capture from the end-to-end harness.",
            "--id",
            "@tasks/mvp-captured",
            "--root",
            path_str(&root),
        ],
    );
    assert_success(&capture, "capture zettel as json");
    let capture_json: Value =
        serde_json::from_slice(&capture.stdout).expect("capture output should be json");
    assert_eq!(capture_json["zettel_id"], "@tasks/mvp-captured");
    assert_eq!(
        capture_json["destination"],
        root.join("inbox.z").display().to_string()
    );

    let reindex_after_capture = run_zorg(
        &fake_home,
        &[
            "db",
            "reindex",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&reindex_after_capture, "reindex after capture");
    let captured_query = run_zorg(
        &fake_home,
        &[
            "query",
            "#z/todo source:mvp-e2e",
            "--root",
            path_str(&root),
            "--db",
            path_str(&db),
        ],
    );
    assert_success(&captured_query, "query captured zettel");
    let captured_stdout = String::from_utf8(captured_query.stdout).expect("query output utf8");
    assert!(captured_stdout.contains("@tasks/mvp-captured"));
    assert!(captured_stdout.contains("MVP captured task"));

    copy_fixture("autofix_unfixed.z", &root);
    let fix = run_zorg(&fake_home, &["fix", "--root", path_str(&root)]);
    assert_success(&fix, "apply fixes");
    let fixed_source =
        std::fs::read_to_string(root.join("autofix_unfixed.z")).expect("read fixed fixture");
    assert!(fixed_source.contains("area::work/research"));
    assert!(fixed_source.contains("#autofix/target"));

    let fix_check = run_zorg(&fake_home, &["fix", "--check", "--root", path_str(&root)]);
    assert_success(&fix_check, "verify fixed corpus");
    assert!(fix_check.stdout.is_empty());
    assert!(fix_check.stderr.is_empty());

    assert!(
        !fake_home.join("zorg").exists(),
        "explicit --root commands must not read or create the process HOME/zorg corpus"
    );
    assert!(
        !fake_home.join(".zorg").exists(),
        "explicit --db commands must not create state in process HOME"
    );
}

fn run_zorg(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zorg"))
        .args(args)
        .env("HOME", home)
        .output()
        .expect("run zorg")
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "expected {context} to succeed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn copy_fixture_corpus(root: &Path) {
    for relative in [
        "minimal.z",
        "nested.z",
        "query_and_template.z",
        "query_focus.z",
        "dir/init.z",
    ] {
        copy_fixture(relative, root);
    }
}

fn copy_fixture(relative: &str, root: &Path) {
    let source = fixture_root().join(relative);
    let target = root.join(relative);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).expect("create fixture parent");
    }
    std::fs::copy(source, target).expect("copy fixture");
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/corpus")
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("path should be utf8")
}
