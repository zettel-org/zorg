use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Mutex, MutexGuard};

use serde_json::{Value, json};
use zorg_store::Store;

static LSP_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn mvp_e2e_lsp_uses_indexed_temp_root_for_navigation_and_actions() {
    let _guard = lsp_test_lock();
    let fake_home = tempfile::tempdir().expect("fake home");
    let root = tempfile::tempdir().expect("workspace root");
    copy_fixture_corpus(root.path());
    let typo_source = "\
%%% @links #z/ref
Links
%%%

See #poject/plan and #project/plan.
";
    fs::write(root.path().join("typo.z"), typo_source).expect("write typo source");
    Store::open(root.path())
        .expect("open store")
        .reindex_full()
        .expect("reindex copied fixture corpus");

    let mut client = initialized_client(root.path().to_string_lossy().as_ref(), fake_home.path());
    let nested_path = root.path().join("nested.z");
    let nested_source = fs::read_to_string(&nested_path).expect("read nested fixture");
    let nested_uri = file_uri(&nested_path.to_string_lossy());
    let typo_uri = file_uri(&root.path().join("typo.z").to_string_lossy());

    let diagnostics = client.read_diagnostics_for_uri(&typo_uri);
    let typo_diagnostic = diagnostics["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array")
        .iter()
        .find(|diagnostic| diagnostic["code"] == "reference.unresolved_absolute")
        .expect("unresolved typo diagnostic")
        .clone();

    client.send_request(
        2,
        "textDocument/definition",
        json!({
            "textDocument": { "uri": nested_uri },
            "position": position_for_token(&nested_source, "#project/plan")
        }),
    );
    let definition = client.read_response(2);
    assert_eq!(definition["result"]["uri"], nested_uri);
    assert_eq!(
        definition["result"]["range"]["start"],
        position_for_token(&nested_source, "@project/plan")
    );

    client.send_request(
        3,
        "textDocument/references",
        json!({
            "textDocument": { "uri": nested_uri },
            "position": position_for_token(&nested_source, "@project/plan"),
            "context": { "includeDeclaration": true }
        }),
    );
    let references = client.read_response(3);
    assert!(
        references["result"]
            .as_array()
            .expect("references")
            .iter()
            .any(|location| location["uri"] == typo_uri)
    );

    client.send_request(
        4,
        "textDocument/documentSymbol",
        json!({ "textDocument": { "uri": nested_uri } }),
    );
    let document_symbols = client.read_response(4);
    assert_eq!(document_symbols["result"][0]["name"], "project");

    client.send_request(5, "workspace/symbol", json!({ "query": "review" }));
    let workspace_symbols = client.read_response(5);
    assert!(
        workspace_symbols["result"]
            .as_array()
            .expect("workspace symbols")
            .iter()
            .any(|symbol| symbol["name"] == "project/review")
    );

    let completion_source = nested_source.replace(
        "The plan links to its child",
        "The plan links # to its child",
    );
    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": nested_uri,
                "languageId": "zorg",
                "version": 1,
                "text": completion_source
            }
        }),
    );
    client.send_request(
        6,
        "textDocument/completion",
        json!({
            "textDocument": { "uri": nested_uri },
            "position": position_after_token(&completion_source, "links #"),
            "context": { "triggerKind": 2, "triggerCharacter": "#" }
        }),
    );
    let completions = client.read_response(6);
    assert!(completion_labels(&completions).contains(&"#project/plan"));

    client.send_request(
        7,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": typo_uri },
            "range": range_for_token(typo_source, "#poject/plan"),
            "context": {
                "diagnostics": [typo_diagnostic],
                "only": ["quickfix"]
            }
        }),
    );
    let code_action = client.read_response(7);
    assert_eq!(
        code_action["result"][0]["edit"]["changes"][&typo_uri][0]["newText"],
        "#project/plan"
    );

    client.shutdown();
    assert!(
        !fake_home.path().join("zorg").exists(),
        "LSP initialize rootPath must keep the test independent from HOME/zorg"
    );
}

struct LspTestClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl LspTestClient {
    fn start(home: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_zorg-ls"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("HOME", home)
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

fn initialized_client(root_path: &str, home: &Path) -> LspTestClient {
    let mut client = LspTestClient::start(home);
    client.send_request(1, "initialize", initialize_params(root_path));
    let response = client.read_response(1);
    assert!(
        response.get("error").is_none(),
        "initialize failed: {response}"
    );
    client.send_notification("initialized", json!({}));
    client
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

fn position_for_token(source: &str, token: &str) -> Value {
    let offset = source
        .find(token)
        .unwrap_or_else(|| panic!("missing token {token}"));
    position_for_offset(source, offset)
}

fn position_after_token(source: &str, token: &str) -> Value {
    let offset = source
        .find(token)
        .unwrap_or_else(|| panic!("missing token {token}"))
        + token.len();
    position_for_offset(source, offset)
}

fn position_for_offset(source: &str, offset: usize) -> Value {
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

fn copy_fixture_corpus(root: &Path) {
    let fixture_root = workspace_root().join("fixtures/corpus");
    for relative in [
        "minimal.z",
        "nested.z",
        "query_and_template.z",
        "query_focus.z",
        "dir/init.z",
    ] {
        let source = fixture_root.join(relative);
        let target = root.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("create copied fixture directory");
        }
        fs::copy(source, target).expect("copy fixture source");
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn lsp_test_lock() -> MutexGuard<'static, ()> {
    LSP_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
