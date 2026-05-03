use std::collections::{BTreeMap, BTreeSet};
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
    pub(crate) indexed_uris: BTreeSet<Url>,
    pub(crate) indexed_diagnostics: BTreeMap<Url, Vec<LspDiagnostic>>,
    pub(crate) lsp_index: Option<LspIndex>,
    pub(crate) lsp_index_error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct StoreRefreshOutcome {
    pub(crate) previous_status: StoreLoadStatus,
    pub(crate) current_status: StoreLoadStatus,
    pub(crate) diagnostic_uris: BTreeSet<Url>,
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
        self.store_status = load_snapshot(&self.config, SnapshotLoadMode::ExistingDatabase);
    }

    pub(crate) fn refresh_store_snapshot(&mut self) -> StoreRefreshOutcome {
        let previous_status = self.store_status.clone();
        let mut diagnostic_uris = diagnostic_uris_for_status(&previous_status);

        self.store_status = load_snapshot(&self.config, SnapshotLoadMode::Reindex);
        diagnostic_uris.extend(diagnostic_uris_for_status(&self.store_status));
        diagnostic_uris.extend(self.open_documents.keys().cloned());

        StoreRefreshOutcome {
            previous_status,
            current_status: self.store_status.clone(),
            diagnostic_uris,
        }
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

    pub(crate) fn save_document_text(&mut self, uri: &Url, text: String) {
        if let Some(document) = self.open_documents.get_mut(uri) {
            document.text = text;
        }
    }

    pub(crate) fn close_document(&mut self, uri: &Url) {
        self.open_documents.remove(uri);
    }

    pub(crate) fn open_document_diagnostics_input(
        &self,
        uri: &Url,
    ) -> Option<(Option<i32>, String)> {
        self.open_documents
            .get(uri)
            .map(|document| (Some(document.version), document.text.clone()))
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

#[derive(Debug, Clone, Copy)]
enum SnapshotLoadMode {
    ExistingDatabase,
    Reindex,
}

fn load_snapshot(config: &ServerConfig, mode: SnapshotLoadMode) -> StoreLoadStatus {
    if !config.root_path.is_dir() {
        return StoreLoadStatus::Degraded(format!(
            "zorg root {} is not a readable directory",
            config.root_path.display()
        ));
    }

    if matches!(mode, SnapshotLoadMode::ExistingDatabase) && !config.database_path.exists() {
        return StoreLoadStatus::Degraded(format!(
            "zorg store database {} does not exist; save a Zorg document or run `zorg db reindex --root {}`",
            config.database_path.display(),
            config.root_path.display()
        ));
    }

    let options = StoreOptions::new(&config.root_path, &config.database_path);
    match options.and_then(Store::open_with_options) {
        Ok(mut store) => {
            if matches!(mode, SnapshotLoadMode::Reindex)
                && let Err(error) = store.reindex()
            {
                return StoreLoadStatus::Degraded(format!(
                    "failed to refresh zorg store index: {error}"
                ));
            }
            snapshot_from_store(&store)
        }
        Err(error) => StoreLoadStatus::Degraded(error.to_string()),
    }
}

fn snapshot_from_store(store: &Store) -> StoreLoadStatus {
    match (
        store.schema_version(),
        store.index_status(),
        indexed_diagnostics(store),
    ) {
        (Ok(schema_version), Ok(index_status), Ok(indexed_diagnostics)) => {
            if let Some(detail) = stale_index_detail(&index_status) {
                StoreLoadStatus::Degraded(detail)
            } else {
                let (lsp_index, lsp_index_error) = match LspIndex::from_store(store) {
                    Ok(index) => (Some(index), None),
                    Err(error) => (None, Some(error.to_string())),
                };
                let indexed_uris = indexed_source_uris(store).unwrap_or_default();
                StoreLoadStatus::Ready(Box::new(StoreSnapshot {
                    schema_version,
                    index_status,
                    indexed_uris,
                    indexed_diagnostics,
                    lsp_index,
                    lsp_index_error,
                }))
            }
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

fn diagnostic_uris_for_status(status: &StoreLoadStatus) -> BTreeSet<Url> {
    match status {
        StoreLoadStatus::Ready(snapshot) => snapshot
            .indexed_uris
            .iter()
            .chain(snapshot.indexed_diagnostics.keys())
            .cloned()
            .collect(),
        StoreLoadStatus::NotLoaded | StoreLoadStatus::Degraded(_) => BTreeSet::new(),
    }
}

fn indexed_source_uris(store: &Store) -> zorg_core::ZorgResult<BTreeSet<Url>> {
    Ok(store
        .list_files()?
        .into_iter()
        .filter_map(|file| file_uri(&file.absolute_path))
        .collect())
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

fn stale_index_detail(status: &IndexStatus) -> Option<String> {
    (status.new_files > 0 || status.changed_files > 0 || status.deleted_files > 0).then(|| {
        format!(
            "zorg store index is stale: {} new, {} changed, {} deleted source files; run `zorg db reindex`",
            status.new_files, status.changed_files, status.deleted_files
        )
    })
}
