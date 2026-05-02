use std::env;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic as LspDiagnostic,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
    InitializeParams, InitializeResult, InitializedParams, Location, MessageType, OneOf,
    PrepareRenameResponse, ReferenceParams, RenameOptions, RenameParams, ServerCapabilities,
    ServerInfo, SymbolInformation, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, WorkspaceEdit, WorkspaceSymbolParams,
};
use tower_lsp::{Client, LanguageServer, LspService, Server, async_trait};

use crate::config::ServerConfig;
use crate::diagnostics::live_diagnostics;
use crate::state::{ServerState, StoreLoadStatus};

mod completion;
mod config;
mod diagnostics;
mod navigation;
mod rename;
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
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "#".to_owned(),
                        "+".to_owned(),
                        "~".to_owned(),
                        "/".to_owned(),
                    ]),
                    ..CompletionOptions::default()
                }),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
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
                if let Some(detail) = &snapshot.lsp_index_error {
                    self.client
                        .log_message(
                            MessageType::WARNING,
                            format!("zorg-ls graph snapshot unavailable: {detail}"),
                        )
                        .await;
                }
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

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let trigger_character = params
            .context
            .as_ref()
            .and_then(|context| context.trigger_character.as_deref());

        let items = self
            .state
            .read()
            .await
            .as_ref()
            .map(|state| state.completion_items(uri, position, trigger_character))
            .unwrap_or_default();

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let location =
            self.state
                .read()
                .await
                .as_ref()
                .and_then(|state| match &state.store_status {
                    StoreLoadStatus::Ready(snapshot) => snapshot
                        .lsp_index
                        .as_ref()
                        .and_then(|index| index.goto_definition(&uri, position)),
                    StoreLoadStatus::NotLoaded | StoreLoadStatus::Degraded(_) => None,
                });

        Ok(location.map(GotoDefinitionResponse::Scalar))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;
        let locations =
            self.state
                .read()
                .await
                .as_ref()
                .and_then(|state| match &state.store_status {
                    StoreLoadStatus::Ready(snapshot) => snapshot
                        .lsp_index
                        .as_ref()
                        .and_then(|index| index.references(&uri, position, include_declaration)),
                    StoreLoadStatus::NotLoaded | StoreLoadStatus::Degraded(_) => None,
                });

        Ok(locations)
    }

    async fn prepare_rename(
        &self,
        params: tower_lsp::lsp_types::TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let position = params.position;
        let response =
            self.state
                .read()
                .await
                .as_ref()
                .and_then(|state| match &state.store_status {
                    StoreLoadStatus::Ready(snapshot) => snapshot
                        .lsp_index
                        .as_ref()
                        .and_then(|index| rename::prepare_rename(index, &uri, position)),
                    StoreLoadStatus::NotLoaded | StoreLoadStatus::Degraded(_) => None,
                });

        Ok(response)
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;
        let edit = self
            .state
            .read()
            .await
            .as_ref()
            .and_then(|state| match &state.store_status {
                StoreLoadStatus::Ready(snapshot) => snapshot.lsp_index.as_ref(),
                StoreLoadStatus::NotLoaded | StoreLoadStatus::Degraded(_) => None,
            })
            .ok_or_else(|| Error::invalid_params("zorg graph snapshot is unavailable"))
            .and_then(|index| {
                rename::plan_rename(index, &uri, position, &new_name).map_err(Error::invalid_params)
            })?;

        Ok(Some(edit))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let symbols =
            self.state
                .read()
                .await
                .as_ref()
                .and_then(|state| match &state.store_status {
                    StoreLoadStatus::Ready(snapshot) => snapshot
                        .lsp_index
                        .as_ref()
                        .map(|index| index.document_symbols(&uri)),
                    StoreLoadStatus::NotLoaded | StoreLoadStatus::Degraded(_) => None,
                });

        Ok(symbols.map(DocumentSymbolResponse::Nested))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let symbols =
            self.state
                .read()
                .await
                .as_ref()
                .and_then(|state| match &state.store_status {
                    StoreLoadStatus::Ready(snapshot) => snapshot
                        .lsp_index
                        .as_ref()
                        .map(|index| index.workspace_symbols(&params.query)),
                    StoreLoadStatus::NotLoaded | StoreLoadStatus::Degraded(_) => None,
                });

        Ok(symbols)
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
