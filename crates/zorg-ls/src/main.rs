use std::env;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::{
    Diagnostic as LspDiagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, InitializeResult, InitializedParams, MessageType,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions,
};
use tower_lsp::{Client, LanguageServer, LspService, Server, async_trait};

use crate::config::ServerConfig;
use crate::diagnostics::live_diagnostics;
use crate::state::{ServerState, StoreLoadStatus};

mod config;
mod diagnostics;
mod state;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("-V" | "--version") => {
            println!("zorg-ls {VERSION}");
        }
        Some("-h" | "--help") => {
            print_help();
        }
        Some(argument) => {
            eprintln!("unsupported zorg-ls argument: {argument}");
            eprintln!("run `zorg-ls --help` for usage");
            std::process::exit(2);
        }
        None => run_stdio_server().await,
    }
}

async fn run_stdio_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(ZorgLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[derive(Debug)]
struct ZorgLanguageServer {
    client: Client,
    state: Arc<RwLock<Option<ServerState>>>,
}

impl ZorgLanguageServer {
    fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(RwLock::new(None)),
        }
    }

    async fn publish_document_diagnostics(
        &self,
        uri: tower_lsp::lsp_types::Url,
        version: Option<i32>,
        live_text: Option<String>,
    ) {
        let indexed = self
            .state
            .read()
            .await
            .as_ref()
            .map(|state| state.indexed_diagnostics_for(&uri))
            .unwrap_or_default();

        let diagnostics = match live_text {
            Some(text) => merge_diagnostics(live_diagnostics(&uri, &text), indexed),
            None => indexed,
        };

        self.client
            .publish_diagnostics(uri, diagnostics, version)
            .await;
    }

    async fn publish_indexed_diagnostics(&self) {
        let diagnostics_by_uri = self
            .state
            .read()
            .await
            .as_ref()
            .and_then(|state| match &state.store_status {
                StoreLoadStatus::Ready(snapshot) => Some(snapshot.indexed_diagnostics.clone()),
                StoreLoadStatus::NotLoaded | StoreLoadStatus::Degraded(_) => None,
            })
            .unwrap_or_default();

        for (uri, diagnostics) in diagnostics_by_uri {
            self.client
                .publish_diagnostics(uri, diagnostics, None)
                .await;
        }
    }
}

#[async_trait]
impl LanguageServer for ZorgLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let config = ServerConfig::from_initialize_params(&params)
            .map_err(|error| Error::invalid_params(error.to_string()))?;
        let mut state = ServerState::new(config);
        state.load_store_snapshot();

        *self.state.write().await = Some(state);

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        ..TextDocumentSyncOptions::default()
                    },
                )),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "zorg-ls".to_owned(),
                version: Some(VERSION.to_owned()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let state = self.state.read().await;
        let Some(state) = state.as_ref() else {
            return;
        };

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "zorg-ls initialized with root {} and database {}",
                    state.config.root_path.display(),
                    state.config.database_path.display()
                ),
            )
            .await;

        if let Some(trace) = &state.config.trace {
            self.client
                .log_message(MessageType::INFO, format!("zorg-ls trace level: {trace}"))
                .await;
        }

        if let Some(notice) = &state.config.multi_root_notice {
            self.client
                .log_message(MessageType::WARNING, notice.clone())
                .await;
        }

        match &state.store_status {
            StoreLoadStatus::Ready(snapshot) => {
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!(
                            "zorg-ls loaded store schema {} with {} indexed files",
                            snapshot.schema_version, snapshot.index_status.indexed_files
                        ),
                    )
                    .await;
            }
            StoreLoadStatus::Degraded(detail) => {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("zorg-ls running with degraded store status: {detail}"),
                    )
                    .await;
            }
            StoreLoadStatus::NotLoaded => {}
        }

        self.publish_indexed_diagnostics().await;
    }

    async fn shutdown(&self) -> Result<()> {
        if let Some(state) = self.state.write().await.as_mut() {
            state.shutdown_requested = true;
        }
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let text = params.text_document.text;

        if let Some(state) = self.state.write().await.as_mut() {
            state.open_document(
                uri.clone(),
                version,
                params.text_document.language_id,
                text.clone(),
            );
        }

        self.publish_document_diagnostics(uri, Some(version), Some(text))
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let version = params.text_document.version;

        let live_text = params
            .content_changes
            .into_iter()
            .last()
            .map(|change| change.text);

        if let Some(text) = live_text.as_ref()
            && let Some(state) = self.state.write().await.as_mut()
        {
            state.change_document(&params.text_document, text.clone());
        }

        self.publish_document_diagnostics(uri, Some(version), live_text)
            .await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;

        if let Some(state) = self.state.write().await.as_mut() {
            state.close_document(&uri);
        }

        self.publish_document_diagnostics(uri, None, None).await;
    }
}

fn merge_diagnostics(
    mut live: Vec<LspDiagnostic>,
    indexed: Vec<LspDiagnostic>,
) -> Vec<LspDiagnostic> {
    for diagnostic in indexed {
        if !live
            .iter()
            .any(|candidate| diagnostic_identity(candidate) == diagnostic_identity(&diagnostic))
        {
            live.push(diagnostic);
        }
    }
    live
}

fn diagnostic_identity(diagnostic: &LspDiagnostic) -> String {
    format!(
        "{}:{}:{}:{}:{:?}:{}",
        diagnostic.range.start.line,
        diagnostic.range.start.character,
        diagnostic.range.end.line,
        diagnostic.range.end.character,
        diagnostic.code,
        diagnostic.message
    )
}

fn print_help() {
    println!(
        "\
zorg-ls {VERSION}

Usage: zorg-ls [OPTIONS]

Options:
  -h, --help     Print help
  -V, --version  Print version

Without options, zorg-ls starts a Language Server Protocol session over stdio."
    );
}
