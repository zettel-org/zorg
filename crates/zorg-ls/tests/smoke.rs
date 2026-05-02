use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};

#[test]
fn zorg_ls_version_works() {
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

    client.shutdown();
}

#[test]
fn opens_and_changes_z_document_over_stdio() {
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
                "text": "@note\n"
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
                    "text": "@note\nBody\n"
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
