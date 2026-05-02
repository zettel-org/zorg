use std::collections::BTreeMap;
use std::fs;
use tower_lsp::lsp_types::{
    CodeActionParams, CodeActionResponse, CompletionItem, Diagnostic as LspDiagnostic, Position,
    Url, VersionedTextDocumentIdentifier,
};
use zorg_store::{IndexStatus, Store, StoreOptions};

use crate::actions::code_actions;
use crate::completion::completion_items;
use crate::config::ServerConfig;
use crate::diagnostics::{file_uri, stored_diagnostic_to_lsp};
use crate::navigation::LspIndex;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct OpenDocument {
    pub(crate) version: i32,
    pub(crate) language_id: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone)]
pub(crate) enum StoreLoadStatus {
    NotLoaded,
    Ready(Box<StoreSnapshot>),
    Degraded(String),
}

#[derive(Debug, Clone)]
pub(crate) struct StoreSnapshot {
    pub(crate) schema_version: i64,
    pub(crate) index_status: IndexStatus,
    pub(crate) indexed_diagnostics: BTreeMap<Url, Vec<LspDiagnostic>>,
    pub(crate) lsp_index: Option<LspIndex>,
    pub(crate) lsp_index_error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ServerState {
    pub(crate) config: ServerConfig,
    pub(crate) open_documents: BTreeMap<Url, OpenDocument>,
    pub(crate) store_status: StoreLoadStatus,
    pub(crate) shutdown_requested: bool,
}

impl ServerState {
    pub(crate) fn new(config: ServerConfig) -> Self {
        Self {
            config,
            open_documents: BTreeMap::new(),
            store_status: StoreLoadStatus::NotLoaded,
            shutdown_requested: false,
        }
    }

    pub(crate) fn load_store_snapshot(&mut self) {
        let options = StoreOptions::new(&self.config.root_path, &self.config.database_path);
        self.store_status = match options.and_then(Store::open_with_options) {
            Ok(store) => {
                match (
                    store.schema_version(),
                    store.index_status(),
                    indexed_diagnostics(&store),
                ) {
                    (Ok(schema_version), Ok(index_status), Ok(indexed_diagnostics)) => {
                        let (lsp_index, lsp_index_error) = match LspIndex::from_store(&store) {
                            Ok(index) => (Some(index), None),
                            Err(error) => (None, Some(error.to_string())),
                        };
                        StoreLoadStatus::Ready(Box::new(StoreSnapshot {
                            schema_version,
                            index_status,
                            indexed_diagnostics,
                            lsp_index,
                            lsp_index_error,
                        }))
                    }
                    (schema_result, status_result, diagnostics_result) => {
                        let detail = schema_result
                            .err()
                            .or_else(|| status_result.err())
                            .or_else(|| diagnostics_result.err())
                            .map(|error| error.to_string())
                            .unwrap_or_else(|| "failed to read store status".to_owned());
                        StoreLoadStatus::Degraded(detail)
                    }
                }
            }
            Err(error) => StoreLoadStatus::Degraded(error.to_string()),
        };
    }

    pub(crate) fn open_document(
        &mut self,
        uri: Url,
        version: i32,
        language_id: String,
        text: String,
    ) {
        self.open_documents.insert(
            uri,
            OpenDocument {
                version,
                language_id,
                text,
            },
        );
    }

    pub(crate) fn change_document(
        &mut self,
        identifier: &VersionedTextDocumentIdentifier,
        text: String,
    ) {
        if let Some(document) = self.open_documents.get_mut(&identifier.uri) {
            document.version = identifier.version;
            document.text = text;
        }
    }

    pub(crate) fn close_document(&mut self, uri: &Url) {
        self.open_documents.remove(uri);
    }

    pub(crate) fn indexed_diagnostics_for(&self, uri: &Url) -> Vec<LspDiagnostic> {
        match &self.store_status {
            StoreLoadStatus::Ready(snapshot) => snapshot
                .indexed_diagnostics
                .get(uri)
                .cloned()
                .unwrap_or_default(),
            StoreLoadStatus::NotLoaded | StoreLoadStatus::Degraded(_) => Vec::new(),
        }
    }

    pub(crate) fn completion_items(
        &self,
        uri: &Url,
        position: Position,
        trigger_character: Option<&str>,
    ) -> Vec<CompletionItem> {
        let Some(index) = self.lsp_index() else {
            return Vec::new();
        };
        let live_text = self
            .open_documents
            .get(uri)
            .map(|document| document.text.as_str());
        completion_items(index, uri, position, live_text, trigger_character)
    }

    pub(crate) fn code_actions(&self, params: &CodeActionParams) -> CodeActionResponse {
        let uri = &params.text_document.uri;
        let text = self
            .open_documents
            .get(uri)
            .map(|document| document.text.clone())
            .or_else(|| {
                uri.to_file_path()
                    .ok()
                    .and_then(|path| fs::read_to_string(path).ok())
            });

        code_actions(self.lsp_index(), uri, text.as_deref(), params)
    }

    fn lsp_index(&self) -> Option<&LspIndex> {
        match &self.store_status {
            StoreLoadStatus::Ready(snapshot) => snapshot.lsp_index.as_ref(),
            StoreLoadStatus::NotLoaded | StoreLoadStatus::Degraded(_) => None,
        }
    }
}

fn indexed_diagnostics(store: &Store) -> zorg_core::ZorgResult<BTreeMap<Url, Vec<LspDiagnostic>>> {
    let mut diagnostics_by_uri = BTreeMap::<Url, Vec<LspDiagnostic>>::new();

    for diagnostic in store.list_diagnostics()? {
        let Some(path) = diagnostic.absolute_path.as_deref() else {
            continue;
        };
        let Some(uri) = file_uri(path) else {
            continue;
        };
        diagnostics_by_uri
            .entry(uri)
            .or_default()
            .push(stored_diagnostic_to_lsp(&diagnostic));
    }

    Ok(diagnostics_by_uri)
}
