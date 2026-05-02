use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Mutex, MutexGuard};

use serde_json::{Value, json};
use zorg_store::Store;

static LSP_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn zorg_ls_version_works() {
    let _guard = lsp_test_lock();
    let output = Command::new(env!("CARGO_BIN_EXE_zorg-ls"))
        .arg("--version")
        .output()
        .expect("run zorg-ls --version");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("version should be utf8");
    assert!(stdout.starts_with("zorg-ls "));
}

#[test]
fn zorg_ls_help_works() {
    let _guard = lsp_test_lock();
    let output = Command::new(env!("CARGO_BIN_EXE_zorg-ls"))
        .arg("--help")
        .output()
        .expect("run zorg-ls --help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help should be utf8");
    assert!(stdout.contains("Language Server Protocol"));
}

#[test]
fn initializes_and_shuts_down_over_stdio() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let mut client = LspTestClient::start();

    client.send_request(
        1,
        "initialize",
        initialize_params(root.path().to_string_lossy().as_ref()),
    );
    let response = client.read_response(1);

    assert_eq!(response["result"]["serverInfo"]["name"], "zorg-ls");
    assert_eq!(
        response["result"]["capabilities"]["textDocumentSync"]["openClose"],
        true
    );
    assert_eq!(
        response["result"]["capabilities"]["textDocumentSync"]["change"],
        1
    );
    assert_eq!(
        response["result"]["capabilities"]["renameProvider"]["prepareProvider"],
        true
    );
    assert_eq!(
        response["result"]["capabilities"]["codeActionProvider"]["codeActionKinds"][0],
        "quickfix"
    );

    client.shutdown();
}

#[test]
fn opens_and_changes_z_document_over_stdio() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let mut client = LspTestClient::start();
    let root_path = root.path().to_string_lossy();
    let document_uri = file_uri(&root.path().join("note.z").to_string_lossy());

    client.send_request(1, "initialize", initialize_params(root_path.as_ref()));
    client.read_response(1);
    client.send_notification("initialized", json!({}));

    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": document_uri,
                "languageId": "zorg",
                "version": 1,
                "text": "%%% @note #z/ref\nNote\n%%%\n"
            }
        }),
    );
    let diagnostics = client.read_notification("textDocument/publishDiagnostics");
    assert_eq!(diagnostics["params"]["uri"], document_uri);
    assert_eq!(diagnostics["params"]["version"], 1);
    assert_eq!(diagnostics["params"]["diagnostics"], json!([]));

    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": {
                "uri": document_uri,
                "version": 2
            },
            "contentChanges": [
                {
                    "text": "%%% @note #z/ref\nNote\n%%%\n\nBody\n"
                }
            ]
        }),
    );
    let diagnostics = client.read_notification("textDocument/publishDiagnostics");
    assert_eq!(diagnostics["params"]["version"], 2);
    assert_eq!(diagnostics["params"]["diagnostics"], json!([]));

    client.send_notification(
        "textDocument/didClose",
        json!({
            "textDocument": {
                "uri": document_uri
            }
        }),
    );
    let diagnostics = client.read_notification("textDocument/publishDiagnostics");
    assert_eq!(diagnostics["params"]["uri"], document_uri);
    assert_eq!(diagnostics["params"]["diagnostics"], json!([]));

    client.shutdown();
}

#[test]
fn publishes_live_diagnostics_with_zero_based_ranges_and_clears_them() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let mut client = LspTestClient::start();
    let root_path = root.path().to_string_lossy();
    let document_uri = file_uri(&root.path().join("bad.z").to_string_lossy());

    client.send_request(1, "initialize", initialize_params(root_path.as_ref()));
    client.read_response(1);
    client.send_notification("initialized", json!({}));

    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": document_uri,
                "languageId": "zorg",
                "version": 1,
                "text": "%%% @bad #z/todo due::2026-02-30\nBad date\n%%%\n"
            }
        }),
    );
    let diagnostics = client.read_notification("textDocument/publishDiagnostics");
    let first = &diagnostics["params"]["diagnostics"][0];
    assert_eq!(diagnostics["params"]["uri"], document_uri);
    assert_eq!(diagnostics["params"]["version"], 1);
    assert_eq!(first["code"], "property.invalid_date");
    assert_eq!(first["range"]["start"]["line"], 0);
    assert_eq!(first["range"]["start"]["character"], 22);

    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": {
                "uri": document_uri,
                "version": 2
            },
            "contentChanges": [
                {
                    "text": "%%% @bad #z/todo due::2026-02-28\nGood date\n%%%\n"
                }
            ]
        }),
    );
    let diagnostics = client.read_notification("textDocument/publishDiagnostics");
    assert_eq!(diagnostics["params"]["version"], 2);
    assert_eq!(diagnostics["params"]["diagnostics"], json!([]));

    client.shutdown();
}

#[test]
fn publishes_legacy_live_diagnostics_without_transforming_source() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let mut client = LspTestClient::start();
    let root_path = root.path().to_string_lossy();
    let document_uri = file_uri(&root.path().join("legacy.z").to_string_lossy());

    client.send_request(1, "initialize", initialize_params(root_path.as_ref()));
    client.read_response(1);
    client.send_notification("initialized", json!({}));

    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": document_uri,
                "languageId": "zorg",
                "version": 1,
                "text": "ID:: legacy\n"
            }
        }),
    );
    let diagnostics = client.read_notification("textDocument/publishDiagnostics");
    let diagnostic = diagnostics["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .iter()
        .find(|diagnostic| diagnostic["code"] == "legacy.unsupported")
        .expect("legacy diagnostic");
    assert!(
        diagnostic["message"]
            .as_str()
            .expect("message")
            .contains("ID::")
    );
    assert_eq!(diagnostic["range"]["start"]["line"], 0);
    assert_eq!(diagnostic["range"]["start"]["character"], 0);

    client.shutdown();
}

#[test]
fn publishes_stored_cross_file_diagnostics_on_initialized() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    fs::write(
        root.path().join("first.z"),
        "%%% @same #z/ref\nFirst\n%%%\n",
    )
    .expect("write first source");
    fs::write(
        root.path().join("second.z"),
        "%%% @same #z/ref\nSecond\n%%%\n",
    )
    .expect("write second source");
    let mut store = Store::open(root.path()).expect("open store");
    let summary = store.reindex_full().expect("reindex store");
    assert_eq!(summary.diagnostic_count, 1);

    let mut client = LspTestClient::start();
    let root_path = root.path().to_string_lossy();
    let second_uri = file_uri(&root.path().join("second.z").to_string_lossy());

    client.send_request(1, "initialize", initialize_params(root_path.as_ref()));
    client.read_response(1);
    client.send_notification("initialized", json!({}));

    let diagnostics = client.read_diagnostics_for_uri(&second_uri);
    let diagnostic = &diagnostics["params"]["diagnostics"][0];
    assert_eq!(diagnostic["code"], "id.duplicate");
    assert!(
        diagnostic["message"]
            .as_str()
            .expect("message")
            .contains("across corpus")
    );
    assert_eq!(diagnostic["range"]["start"]["line"], 0);
    assert_eq!(diagnostic["range"]["start"]["character"], 0);

    client.shutdown();
}

#[test]
fn goes_to_definition_across_indexed_files() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let target = "%%% @project/plan #z/ref\nPlan\n%%%\n";
    let source = "%%% @links #z/ref\nLinks\n%%%\n\nSee #project/plan.\n";
    fs::write(root.path().join("plan.z"), target).expect("write target source");
    fs::write(root.path().join("links.z"), source).expect("write link source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let source_uri = file_uri(&root.path().join("links.z").to_string_lossy());
    let target_uri = file_uri(&root.path().join("plan.z").to_string_lossy());
    let position = position_for_token(source, "#project/plan");

    client.send_request(
        2,
        "textDocument/definition",
        json!({
            "textDocument": { "uri": source_uri },
            "position": position
        }),
    );
    let response = client.read_response(2);

    assert_eq!(response["result"]["uri"], target_uri);
    assert_eq!(response["result"]["range"]["start"]["line"], 0);
    assert_eq!(response["result"]["range"]["start"]["character"], 4);

    client.shutdown();
}

#[test]
fn navigates_nested_child_and_sibling_links() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let source = nested_navigation_source();
    fs::write(root.path().join("nested.z"), source).expect("write nested source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let uri = file_uri(&root.path().join("nested.z").to_string_lossy());

    client.send_request(
        2,
        "textDocument/definition",
        json!({
            "textDocument": { "uri": uri },
            "position": position_for_token(source, "+task")
        }),
    );
    let child_response = client.read_response(2);
    assert_eq!(child_response["result"]["uri"], uri);
    assert_eq!(
        child_response["result"]["range"]["start"],
        position_for_token(source, "^task")
    );

    client.send_request(
        3,
        "textDocument/definition",
        json!({
            "textDocument": { "uri": uri },
            "position": position_for_token(source, "~review")
        }),
    );
    let sibling_response = client.read_response(3);
    assert_eq!(sibling_response["result"]["uri"], uri);
    assert_eq!(
        sibling_response["result"]["range"]["start"],
        position_for_token(source, "@project/review")
    );

    client.shutdown();
}

#[test]
fn finds_references_from_multiple_files_and_nested_zettel() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let nested = nested_navigation_source();
    let other = "%%% @other #z/ref\nOther\n%%%\n\nAnother link to #project/plan.\n";
    fs::write(root.path().join("nested.z"), nested).expect("write nested source");
    fs::write(root.path().join("other.z"), other).expect("write other source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let nested_uri = file_uri(&root.path().join("nested.z").to_string_lossy());
    let other_uri = file_uri(&root.path().join("other.z").to_string_lossy());

    client.send_request(
        2,
        "textDocument/references",
        json!({
            "textDocument": { "uri": nested_uri },
            "position": position_for_token(nested, "@project/plan"),
            "context": { "includeDeclaration": true }
        }),
    );
    let response = client.read_response(2);
    let references = response["result"].as_array().expect("references result");

    assert!(references.len() >= 3, "references: {references:#?}");
    assert!(
        references
            .iter()
            .any(|location| location["uri"] == nested_uri)
    );
    assert!(
        references
            .iter()
            .any(|location| location["uri"] == other_uri)
    );
    assert!(references.iter().any(|location| {
        location["range"]["start"] == position_for_token(nested, "@project/plan")
    }));

    client.shutdown();
}

#[test]
fn document_symbols_preserve_nested_zettel_hierarchy() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let source = nested_navigation_source();
    fs::write(root.path().join("nested.z"), source).expect("write nested source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let uri = file_uri(&root.path().join("nested.z").to_string_lossy());

    client.send_request(
        2,
        "textDocument/documentSymbol",
        json!({
            "textDocument": { "uri": uri }
        }),
    );
    let response = client.read_response(2);
    let root_symbol = &response["result"][0];
    let plan_symbol = root_symbol["children"]
        .as_array()
        .expect("root children")
        .iter()
        .find(|symbol| symbol["name"] == "project/plan")
        .expect("plan symbol");

    assert_eq!(root_symbol["name"], "project");
    assert!(
        plan_symbol["children"]
            .as_array()
            .expect("plan children")
            .iter()
            .any(|symbol| symbol["name"] == "project/plan/task")
    );

    client.shutdown();
}

#[test]
fn workspace_symbols_find_canonical_ids() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let source = nested_navigation_source();
    fs::write(root.path().join("nested.z"), source).expect("write nested source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let uri = file_uri(&root.path().join("nested.z").to_string_lossy());

    client.send_request(2, "workspace/symbol", json!({ "query": "plan" }));
    let response = client.read_response(2);
    let symbols = response["result"].as_array().expect("workspace symbols");

    assert!(
        symbols
            .iter()
            .any(|symbol| { symbol["name"] == "project/plan" && symbol["location"]["uri"] == uri })
    );

    client.shutdown();
}

#[test]
fn completes_absolute_links_and_tags_with_deterministic_order() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let indexed_source = completion_source();
    let open_source = indexed_source.replace("Complete absolute", "Complete absolute #");
    fs::write(root.path().join("complete.z"), indexed_source).expect("write completion source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let uri = file_uri(&root.path().join("complete.z").to_string_lossy());
    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "zorg",
                "version": 1,
                "text": open_source
            }
        }),
    );

    client.send_request(
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": position_after_token(&open_source, "absolute #"),
            "context": { "triggerKind": 2, "triggerCharacter": "#" }
        }),
    );
    let response = client.read_response(2);
    let labels = completion_labels(&response);

    assert_eq!(
        labels,
        vec![
            "#project",
            "#project/plan",
            "#project/plan/task",
            "#project/review",
            "#area/task",
            "#area/work",
            "#z/query",
            "#z/ref",
            "#z/tmpl",
            "#z/todo",
        ]
    );
    assert_eq!(completion_detail(&response, "#project"), "zettel: Project");
    assert_eq!(completion_detail(&response, "#area/work"), "tag");
    assert_eq!(completion_detail(&response, "#z/todo"), "type tag");

    client.shutdown();
}

#[test]
fn completes_child_and_sibling_links_from_containing_zettel() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let indexed_source = completion_source();
    let open_source = indexed_source
        .replace("Complete child", "Complete child +")
        .replace("Complete sibling", "Complete sibling ~");
    fs::write(root.path().join("complete.z"), indexed_source).expect("write completion source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let uri = file_uri(&root.path().join("complete.z").to_string_lossy());
    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "zorg",
                "version": 1,
                "text": open_source
            }
        }),
    );

    client.send_request(
        2,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": position_after_token(&open_source, "child +"),
            "context": { "triggerKind": 2, "triggerCharacter": "+" }
        }),
    );
    let child_response = client.read_response(2);
    assert_eq!(completion_labels(&child_response), vec!["+task"]);

    client.send_request(
        3,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": uri },
            "position": position_after_token(&open_source, "sibling ~"),
            "context": { "triggerKind": 2, "triggerCharacter": "~" }
        }),
    );
    let sibling_response = client.read_response(3);
    assert_eq!(completion_labels(&sibling_response), vec!["~review"]);

    client.shutdown();
}

#[test]
fn code_action_rewrites_unresolved_absolute_link_with_single_typo_candidate() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let target = "%%% @project/plan #z/ref\nPlan\n%%%\n";
    let source = "%%% @links #z/ref\nLinks\n%%%\n\nSee #poject/plan.\n";
    fs::write(root.path().join("plan.z"), target).expect("write target source");
    fs::write(root.path().join("links.z"), source).expect("write link source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let source_uri = file_uri(&root.path().join("links.z").to_string_lossy());
    let diagnostics = client.read_diagnostics_for_uri(&source_uri);
    let diagnostic = diagnostics["params"]["diagnostics"][0].clone();
    assert_eq!(diagnostic["code"], "reference.unresolved_absolute");

    client.send_request(
        2,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": source_uri },
            "range": range_for_token(source, "#poject/plan"),
            "context": {
                "diagnostics": [diagnostic],
                "only": ["quickfix"]
            }
        }),
    );
    let response = client.read_response(2);
    let action = &response["result"][0];
    assert_eq!(action["title"], "Rewrite unresolved link to #project/plan");
    assert_eq!(action["kind"], "quickfix");
    assert_eq!(
        action["edit"]["changes"][&source_uri][0]["newText"],
        "#project/plan"
    );
    assert_eq!(
        action["edit"]["changes"][&source_uri][0]["range"],
        range_for_token(source, "#poject/plan")
    );

    client.shutdown();
}

#[test]
fn code_action_declines_ambiguous_unresolved_link_candidates() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let source = "\
%%% @alpha #z/ref
Alpha
%%%

- @alphi #z/ref Similar.

See #alphx.
";
    fs::write(root.path().join("ambiguous.z"), source).expect("write source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let uri = file_uri(&root.path().join("ambiguous.z").to_string_lossy());
    let diagnostics = client.read_diagnostics_for_uri(&uri);
    let diagnostic = diagnostics["params"]["diagnostics"][0].clone();
    assert_eq!(diagnostic["code"], "reference.unresolved_absolute");

    client.send_request(
        2,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": uri },
            "range": range_for_token(source, "#alphx"),
            "context": { "diagnostics": [diagnostic] }
        }),
    );
    let response = client.read_response(2);
    assert_eq!(response["result"], json!([]));

    client.shutdown();
}

#[test]
fn code_action_declines_legacy_migration_and_missing_graph_snapshot() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let legacy_source = "ID:: legacy\n";
    let uri = file_uri(&root.path().join("legacy.z").to_string_lossy());

    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "zorg",
                "version": 1,
                "text": legacy_source
            }
        }),
    );
    let diagnostics = client.read_notification("textDocument/publishDiagnostics");
    let diagnostic = diagnostics["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .iter()
        .find(|diagnostic| diagnostic["code"] == "legacy.unsupported")
        .expect("legacy diagnostic")
        .clone();
    assert_eq!(diagnostic["code"], "legacy.unsupported");

    client.send_request(
        2,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": uri },
            "range": range_for_token(legacy_source, "ID::"),
            "context": { "diagnostics": [diagnostic] }
        }),
    );
    let response = client.read_response(2);
    assert_eq!(response["result"], json!([]));

    let unresolved_source = "See #poject/plan.\n";
    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": {
                "uri": uri,
                "version": 2
            },
            "contentChanges": [
                {
                    "text": unresolved_source
                }
            ]
        }),
    );
    let _ = client.read_notification("textDocument/publishDiagnostics");
    client.send_request(
        3,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": uri },
            "range": range_for_token(unresolved_source, "#poject/plan"),
            "context": {
                "diagnostics": [{
                    "range": range_for_token(unresolved_source, "#poject/plan"),
                    "severity": 1,
                    "code": "reference.unresolved_absolute",
                    "source": "zorg.semantic",
                    "message": "unresolved absolute reference"
                }]
            }
        }),
    );
    let response = client.read_response(3);
    assert_eq!(response["result"], json!([]));

    client.shutdown();
}

#[test]
fn prepare_rename_succeeds_only_on_source_backed_occurrences() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let source = "%%% @project/plan #z/ref\nPlan\n%%%\n\nSee #project/plan.\n";
    fs::write(root.path().join("plan.z"), source).expect("write source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let uri = file_uri(&root.path().join("plan.z").to_string_lossy());

    client.send_request(
        2,
        "textDocument/prepareRename",
        json!({
            "textDocument": { "uri": uri },
            "position": position_for_token(source, "#project/plan")
        }),
    );
    let response = client.read_response(2);
    assert_eq!(response["result"]["placeholder"], "project/plan");
    assert_eq!(
        response["result"]["range"]["start"],
        position_for_token(source, "#project/plan")
    );

    client.send_request(
        3,
        "textDocument/prepareRename",
        json!({
            "textDocument": { "uri": uri },
            "position": position_for_token(source, "See")
        }),
    );
    let response = client.read_response(3);
    assert_eq!(response["result"], json!(null));

    client.shutdown();
}

#[test]
fn rename_rewrites_absolute_declaration_and_links_atomically() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let target = "%%% @project/plan #z/ref\nPlan\n%%%\n";
    let source = "%%% @links #z/ref\nLinks\n%%%\n\nSee #project/plan.\n";
    fs::write(root.path().join("plan.z"), target).expect("write target source");
    fs::write(root.path().join("links.z"), source).expect("write link source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let target_uri = file_uri(&root.path().join("plan.z").to_string_lossy());
    let source_uri = file_uri(&root.path().join("links.z").to_string_lossy());

    client.send_request(
        2,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": target_uri },
            "position": position_for_token(target, "@project/plan"),
            "newName": "project/roadmap"
        }),
    );
    let response = client.read_response(2);
    assert!(response.get("error").is_none(), "rename failed: {response}");
    assert_edit(
        &response,
        &target_uri,
        position_for_token(target, "@project/plan"),
        "@project/roadmap",
    );
    assert_edit(
        &response,
        &source_uri,
        position_for_token(source, "#project/plan"),
        "#project/roadmap",
    );

    client.shutdown();
}

#[test]
fn rename_rejects_duplicate_target_ids() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let source = "\
%%% @alpha #z/ref
Alpha
%%%

- @beta #z/ref Beta.
";
    fs::write(root.path().join("ids.z"), source).expect("write source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let uri = file_uri(&root.path().join("ids.z").to_string_lossy());

    client.send_request(
        2,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": position_for_token(source, "@alpha"),
            "newName": "beta"
        }),
    );
    let response = client.read_response(2);
    assert!(
        response["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("already exists")
    );

    client.shutdown();
}

#[test]
fn rename_handles_nested_local_ids_when_relative_rewrites_are_deterministic() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let source = "\
%%% @project #z/ref
Project
%%%

- @project/plan #z/ref Plan.
  See child +task.

  - ^task #z/todo Task.
";
    fs::write(root.path().join("nested.z"), source).expect("write source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let uri = file_uri(&root.path().join("nested.z").to_string_lossy());

    client.send_request(
        2,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": position_for_token(source, "^task"),
            "newName": "done"
        }),
    );
    let response = client.read_response(2);
    assert!(response.get("error").is_none(), "rename failed: {response}");
    assert_edit(
        &response,
        &uri,
        position_for_token(source, "^task"),
        "^done",
    );
    assert_edit(
        &response,
        &uri,
        position_for_token(source, "+task"),
        "+done",
    );

    client.shutdown();
}

#[test]
fn rename_rejects_relative_rewrites_that_are_not_deterministic() {
    let _guard = lsp_test_lock();
    let root = tempfile::tempdir().expect("workspace root");
    let source = "\
%%% @project #z/ref
Project
%%%

- @project/plan #z/ref Plan.
  See child +task.

  - ^task #z/todo Task.
";
    fs::write(root.path().join("nested.z"), source).expect("write source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex store");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref());
    let uri = file_uri(&root.path().join("nested.z").to_string_lossy());

    client.send_request(
        2,
        "textDocument/rename",
        json!({
            "textDocument": { "uri": uri },
            "position": position_for_token(source, "^task"),
            "newName": "elsewhere/task"
        }),
    );
    let response = client.read_response(2);
    assert!(
        response["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("local zettel ID rename must stay")
    );

    client.shutdown();
}

struct LspTestClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl LspTestClient {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_zorg-ls"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start zorg-ls");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));

        Self {
            child,
            stdin: Some(stdin),
            stdout,
        }
    }

    fn send_request(&mut self, id: i64, method: &str, params: Value) {
        self.write_message(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }));
    }

    fn send_notification(&mut self, method: &str, params: Value) {
        self.write_message(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        }));
    }

    fn send_request_without_params(&mut self, id: i64, method: &str) {
        self.write_message(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method
        }));
    }

    fn send_notification_without_params(&mut self, method: &str) {
        self.write_message(json!({
            "jsonrpc": "2.0",
            "method": method
        }));
    }

    fn read_response(&mut self, id: i64) -> Value {
        loop {
            let message = self.read_message();
            if message.get("id").and_then(Value::as_i64) == Some(id) {
                return message;
            }
        }
    }

    fn read_notification(&mut self, method: &str) -> Value {
        loop {
            let message = self.read_message();
            if message.get("method").and_then(Value::as_str) == Some(method) {
                return message;
            }
        }
    }

    fn read_diagnostics_for_uri(&mut self, uri: &str) -> Value {
        loop {
            let message = self.read_notification("textDocument/publishDiagnostics");
            if message["params"]["uri"] == uri {
                return message;
            }
        }
    }

    fn shutdown(&mut self) {
        self.send_request_without_params(99, "shutdown");
        let response = self.read_response(99);
        assert!(
            response.get("error").is_none(),
            "shutdown failed: {response}"
        );
        assert_eq!(response["result"], json!(null));
        self.send_notification_without_params("exit");
        drop(self.stdin.take());
        let status = self.child.wait().expect("wait for zorg-ls exit");
        assert!(status.success());
    }

    fn write_message(&mut self, message: Value) {
        let payload = serde_json::to_vec(&message).expect("serialize json-rpc message");
        let stdin = self.stdin.as_mut().expect("child stdin should be open");
        write!(stdin, "Content-Length: {}\r\n\r\n", payload.len()).expect("write message header");
        stdin.write_all(&payload).expect("write message body");
        stdin.flush().expect("flush message");
    }

    fn read_message(&mut self) -> Value {
        let mut content_length = None;

        loop {
            let mut line = String::new();
            let bytes = self.stdout.read_line(&mut line).expect("read header");
            assert_ne!(bytes, 0, "server stdout closed before message");

            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break;
            }

            if let Some(length) = trimmed.strip_prefix("Content-Length:") {
                content_length = Some(
                    length
                        .trim()
                        .parse::<usize>()
                        .expect("content length should be numeric"),
                );
            }
        }

        let length = content_length.expect("message should include content length");
        let mut payload = vec![0; length];
        self.stdout
            .read_exact(&mut payload)
            .expect("read message body");
        serde_json::from_slice(&payload).expect("message body should be json")
    }
}

impl Drop for LspTestClient {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn initialize_params(root_path: &str) -> Value {
    json!({
        "processId": null,
        "rootUri": file_uri(root_path),
        "capabilities": {},
        "initializationOptions": {
            "rootPath": root_path,
            "trace": "off"
        }
    })
}

fn file_uri(path: &str) -> String {
    format!("file://{path}")
}

fn initialized_client(root_path: &str) -> LspTestClient {
    let mut client = LspTestClient::start();
    client.send_request(1, "initialize", initialize_params(root_path));
    let response = client.read_response(1);
    assert!(
        response.get("error").is_none(),
        "initialize failed: {response}"
    );
    client.send_notification("initialized", json!({}));
    client
}

fn nested_navigation_source() -> &'static str {
    "\
%%% @project #z/ref
Project
%%%

The root links to #project/plan.

- @project/plan #z/todo Plan.
  The plan links to child +task and sibling ~review.

  - ^task #z/todo Write the task.

  - @project/review #z/ref Review.
    This review links back to ~plan.
"
}

fn completion_source() -> &'static str {
    "\
%%% @project #area/work
Project
%%%

Complete absolute

- @project/plan #z/todo Plan.
  Complete child
  Complete sibling

  - ^task #area/task Write the task.

  - @project/review #z/ref Review.
"
}

fn position_for_token(source: &str, token: &str) -> Value {
    let offset = source
        .find(token)
        .unwrap_or_else(|| panic!("missing token {token}"));
    let mut line = 0_u32;
    let mut character = 0_u32;
    for character_value in source[..offset].chars() {
        if character_value == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }
    json!({ "line": line, "character": character })
}

fn position_after_token(source: &str, token: &str) -> Value {
    let offset = source
        .find(token)
        .unwrap_or_else(|| panic!("missing token {token}"))
        + token.len();
    let mut line = 0_u32;
    let mut character = 0_u32;
    for character_value in source[..offset].chars() {
        if character_value == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }
    json!({ "line": line, "character": character })
}

fn range_for_token(source: &str, token: &str) -> Value {
    let start = position_for_token(source, token);
    let end = position_after_token(source, token);
    json!({ "start": start, "end": end })
}

fn completion_labels(response: &Value) -> Vec<&str> {
    response["result"]
        .as_array()
        .expect("completion result")
        .iter()
        .map(|item| item["label"].as_str().expect("completion label"))
        .collect()
}

fn completion_detail<'a>(response: &'a Value, label: &str) -> &'a str {
    response["result"]
        .as_array()
        .expect("completion result")
        .iter()
        .find(|item| item["label"] == label)
        .unwrap_or_else(|| panic!("missing completion {label}"))["detail"]
        .as_str()
        .expect("completion detail")
}

fn assert_edit(response: &Value, uri: &str, start: Value, new_text: &str) {
    let edits = response["result"]["changes"][uri]
        .as_array()
        .unwrap_or_else(|| panic!("missing edits for {uri}: {response}"));
    assert!(
        edits
            .iter()
            .any(|edit| edit["range"]["start"] == start && edit["newText"] == new_text),
        "missing edit {uri} {start} -> {new_text}: {edits:#?}"
    );
}

fn lsp_test_lock() -> MutexGuard<'static, ()> {
    LSP_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
