use std::collections::BTreeMap;
use std::path::PathBuf;

use tower_lsp::lsp_types::{Url, VersionedTextDocumentIdentifier};
use zorg_store::{IndexStatus, Store, StoreOptions};

use crate::config::ServerConfig;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct OpenDocument {
    pub(crate) version: i32,
    pub(crate) language_id: String,
    pub(crate) text: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum StoreLoadStatus {
    NotLoaded,
    Ready(StoreSnapshot),
    Degraded(String),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct StoreSnapshot {
    pub(crate) root_path: PathBuf,
    pub(crate) database_path: PathBuf,
    pub(crate) schema_version: i64,
    pub(crate) index_status: IndexStatus,
}

#[derive(Debug, Clone, Eq, PartialEq)]
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
            Ok(store) => match (store.schema_version(), store.index_status()) {
                (Ok(schema_version), Ok(index_status)) => StoreLoadStatus::Ready(StoreSnapshot {
                    root_path: store.root().to_path_buf(),
                    database_path: store.database_path().to_path_buf(),
                    schema_version,
                    index_status,
                }),
                (schema_result, status_result) => {
                    let detail = schema_result
                        .err()
                        .or_else(|| status_result.err())
                        .map(|error| error.to_string())
                        .unwrap_or_else(|| "failed to read store status".to_owned());
                    StoreLoadStatus::Degraded(detail)
                }
            },
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
}
