use std::path::PathBuf;

use serde::Deserialize;
use serde::de::{self, Deserializer, Visitor};
use tower_lsp::lsp_types::{InitializeParams, Url};
use zorg_core::{ZorgError, ZorgResult};
use zorg_store::StoreOptions;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum RefreshOnSave {
    Reindex,
    Diagnostics,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ServerConfig {
    pub(crate) root_path: PathBuf,
    pub(crate) database_path: PathBuf,
    pub(crate) trace: Option<String>,
    pub(crate) multi_root_notice: Option<String>,
    pub(crate) refresh_on_save: RefreshOnSave,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializationOptions {
    root_path: Option<PathBuf>,
    root: Option<PathBuf>,
    database_path: Option<PathBuf>,
    db_path: Option<PathBuf>,
    trace: Option<String>,
    log_level: Option<String>,
    refresh_on_save: Option<RefreshOnSave>,
}

impl ServerConfig {
    pub(crate) fn from_initialize_params(params: &InitializeParams) -> ZorgResult<Self> {
        let options = params
            .initialization_options
            .as_ref()
            .map(|value| serde_json::from_value::<InitializationOptions>(value.clone()))
            .transpose()
            .map_err(|error| ZorgError::OperationFailed {
                message: format!("invalid zorg-ls initialization options: {error}"),
            })?
            .unwrap_or_default();

        let workspace_root = params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .and_then(|folder| path_from_url(&folder.uri).ok())
            .or_else(|| {
                params
                    .root_uri
                    .as_ref()
                    .and_then(|uri| path_from_url(uri).ok())
            })
            .or_else(|| deprecated_root_path(params));

        let root_path = options
            .root_path
            .or(options.root)
            .or(workspace_root)
            .unwrap_or(StoreOptions::default_root()?);

        let database_path = options
            .database_path
            .or(options.db_path)
            .unwrap_or_else(|| StoreOptions::default_database_path(&root_path));

        let multi_root_notice = params.workspace_folders.as_ref().and_then(|folders| {
            (folders.len() > 1).then(|| {
                format!(
                    "zorg-ls MVP uses a single workspace root; selected {} and ignored {} additional roots",
                    root_path.display(),
                    folders.len() - 1
                )
            })
        });

        let store_options = StoreOptions::new(&root_path, &database_path)?;

        Ok(Self {
            root_path: store_options.corpus_root().to_path_buf(),
            database_path: store_options.database_path().to_path_buf(),
            trace: options.trace.or(options.log_level),
            multi_root_notice,
            refresh_on_save: options.refresh_on_save.unwrap_or(RefreshOnSave::Reindex),
        })
    }
}

impl<'de> Deserialize<'de> for RefreshOnSave {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(RefreshOnSaveVisitor)
    }
}

struct RefreshOnSaveVisitor;

impl Visitor<'_> for RefreshOnSaveVisitor {
    type Value = RefreshOnSave;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("false, true, \"diagnostics\", or \"reindex\"")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(if value {
            RefreshOnSave::Reindex
        } else {
            RefreshOnSave::Diagnostics
        })
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match value {
            "diagnostics" => Ok(RefreshOnSave::Diagnostics),
            "reindex" => Ok(RefreshOnSave::Reindex),
            _ => Err(E::unknown_variant(value, &["diagnostics", "reindex"])),
        }
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_str(&value)
    }
}

fn path_from_url(uri: &Url) -> Result<PathBuf, ()> {
    uri.to_file_path()
}

#[allow(deprecated)]
fn deprecated_root_path(params: &InitializeParams) -> Option<PathBuf> {
    params.root_path.as_ref().map(PathBuf::from)
}
