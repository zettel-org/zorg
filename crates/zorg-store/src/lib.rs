//! Storage and indexing boundary for Zorg.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Deserialize;
use zorg_core::{
    BodyBlock, Diagnostic, DiagnosticCategory, Reference, ReferenceTarget, ResolvedReference,
    Severity, SourcePath, SourceSpan, TodoMarker, Zettel, ZettelDocument, ZettelKind, ZorgError,
    ZorgResult,
};

/// Current SQLite schema version created by this crate.
pub const SCHEMA_VERSION: i64 = 1;

const DEFAULT_ROOT_DIR: &str = "zorg";
const DEFAULT_DB_DIR: &str = ".zorg";
const DEFAULT_DB_FILE: &str = "zorg.sqlite3";
const DEFAULT_WATCHER_DEBOUNCE_MS: u64 = 250;
const CANONICAL_SOURCE_EXTENSION: &str = "z";
const UNSUPPORTED_LEGACY_EXTENSIONS: &[&str] = &["zo", "zoq", "zot", "zoc"];

const MIGRATIONS: &[Migration] = &[Migration {
    version: SCHEMA_VERSION,
    sql: r#"
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY,
    absolute_path TEXT NOT NULL UNIQUE,
    relative_path TEXT NOT NULL,
    mtime_unix_ms INTEGER,
    byte_len INTEGER,
    content_hash TEXT,
    indexed_at_unix_ms INTEGER
);

CREATE TABLE IF NOT EXISTS zettel (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    parent_id INTEGER REFERENCES zettel(id) ON DELETE CASCADE,
    source_order INTEGER NOT NULL,
    kind TEXT NOT NULL,
    parser_key TEXT NOT NULL,
    title TEXT,
    canonical_id TEXT,
    local_id TEXT,
    body_text TEXT,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL,
    start_line INTEGER,
    start_column INTEGER,
    end_line INTEGER,
    end_column INTEGER,
    UNIQUE(file_id, parser_key)
);

CREATE TABLE IF NOT EXISTS zettel_ids (
    canonical_id TEXT PRIMARY KEY,
    zettel_id INTEGER NOT NULL REFERENCES zettel(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS links (
    id INTEGER PRIMARY KEY,
    source_zettel_id INTEGER NOT NULL REFERENCES zettel(id) ON DELETE CASCADE,
    target_zettel_id INTEGER REFERENCES zettel(id) ON DELETE SET NULL,
    target_canonical_id TEXT,
    target_text TEXT NOT NULL,
    link_kind TEXT NOT NULL,
    resolved INTEGER NOT NULL DEFAULT 0,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tags (
    id INTEGER PRIMARY KEY,
    zettel_id INTEGER NOT NULL REFERENCES zettel(id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    tag_kind TEXT NOT NULL,
    start_byte INTEGER,
    end_byte INTEGER,
    UNIQUE(zettel_id, tag, tag_kind)
);

CREATE TABLE IF NOT EXISTS effective_tags (
    id INTEGER PRIMARY KEY,
    zettel_id INTEGER NOT NULL REFERENCES zettel(id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    source_zettel_id INTEGER REFERENCES zettel(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    UNIQUE(zettel_id, tag, source_zettel_id, source)
);

CREATE TABLE IF NOT EXISTS properties (
    id INTEGER PRIMARY KEY,
    zettel_id INTEGER NOT NULL REFERENCES zettel(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    start_byte INTEGER,
    end_byte INTEGER
);

CREATE TABLE IF NOT EXISTS todos (
    id INTEGER PRIMARY KEY,
    zettel_id INTEGER NOT NULL REFERENCES zettel(id) ON DELETE CASCADE,
    marker TEXT NOT NULL,
    start_byte INTEGER,
    end_byte INTEGER
);

CREATE TABLE IF NOT EXISTS text_index (
    zettel_id INTEGER PRIMARY KEY REFERENCES zettel(id) ON DELETE CASCADE,
    title_text TEXT,
    body_text TEXT NOT NULL,
    raw_text TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS diagnostics (
    id INTEGER PRIMARY KEY,
    file_id INTEGER REFERENCES files(id) ON DELETE CASCADE,
    zettel_id INTEGER REFERENCES zettel(id) ON DELETE CASCADE,
    severity TEXT NOT NULL,
    category TEXT NOT NULL,
    code TEXT,
    message TEXT NOT NULL,
    start_byte INTEGER,
    end_byte INTEGER,
    start_line INTEGER,
    start_column INTEGER,
    end_line INTEGER,
    end_column INTEGER
);

CREATE TABLE IF NOT EXISTS index_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_files_relative_path ON files(relative_path);
CREATE INDEX IF NOT EXISTS idx_zettel_file_order ON zettel(file_id, source_order);
CREATE INDEX IF NOT EXISTS idx_zettel_canonical_id ON zettel(canonical_id);
CREATE INDEX IF NOT EXISTS idx_links_source ON links(source_zettel_id);
CREATE INDEX IF NOT EXISTS idx_links_target ON links(target_zettel_id);
CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags(tag);
CREATE INDEX IF NOT EXISTS idx_effective_tags_tag ON effective_tags(tag);
CREATE INDEX IF NOT EXISTS idx_properties_key ON properties(key);
CREATE INDEX IF NOT EXISTS idx_diagnostics_file ON diagnostics(file_id);
"#,
}];

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct Migration {
    version: i64,
    sql: &'static str,
}

/// A canonical `.z` source discovered under a corpus root.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct CorpusSource {
    absolute_path: PathBuf,
    relative_path: PathBuf,
}

impl CorpusSource {
    fn new(absolute_path: PathBuf, relative_path: PathBuf) -> Self {
        Self {
            absolute_path,
            relative_path,
        }
    }

    /// Returns the full source path as discovered on disk.
    #[must_use]
    pub fn absolute_path(&self) -> &Path {
        &self.absolute_path
    }

    /// Returns the source path relative to the corpus root.
    #[must_use]
    pub fn relative_path(&self) -> &Path {
        &self.relative_path
    }
}

/// Summary returned after a reindex operation.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ReindexSummary {
    /// Number of canonical `.z` files discovered under the corpus root.
    pub discovered_files: usize,
    /// Number of discovered files parsed and written to the index.
    pub indexed_files: usize,
    /// Number of discovered files that already matched the indexed hash.
    pub unchanged_files: usize,
    /// Number of discovered files that had no previous indexed row.
    pub new_files: usize,
    /// Number of discovered files whose content hash changed.
    pub changed_files: usize,
    /// Number of indexed file rows no longer present on disk.
    pub deleted_files: usize,
    /// Number of zettel rows written.
    pub zettel_count: usize,
    /// Number of diagnostic rows written.
    pub diagnostic_count: usize,
    /// Number of materialized effective tag rows currently indexed.
    pub effective_tag_count: usize,
    /// Latest successful index timestamp in Unix milliseconds when available.
    pub last_indexed_at_unix_ms: Option<i64>,
}

/// Current SQLite index status for a corpus root.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct IndexStatus {
    /// Number of canonical `.z` files discovered under the corpus root.
    pub discovered_files: usize,
    /// Number of source files currently recorded in the index.
    pub indexed_files: usize,
    /// Number of discovered files that already match the indexed hash.
    pub unchanged_files: usize,
    /// Number of discovered files that have no previous indexed row.
    pub new_files: usize,
    /// Number of discovered files whose content hash differs from the index.
    pub changed_files: usize,
    /// Number of indexed file rows no longer present on disk.
    pub deleted_files: usize,
    /// Number of diagnostics currently recorded in the index.
    pub diagnostic_count: usize,
    /// Number of materialized effective tag rows currently indexed.
    pub effective_tag_count: usize,
    /// Latest successful index timestamp in Unix milliseconds when available.
    pub last_indexed_at_unix_ms: Option<i64>,
}

/// Query-facing file row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StoredFile {
    /// SQLite row ID.
    pub id: i64,
    /// Absolute path recorded during discovery.
    pub absolute_path: PathBuf,
    /// Path relative to the corpus root.
    pub relative_path: PathBuf,
    /// Source modified time in Unix milliseconds when available.
    pub mtime_unix_ms: Option<i64>,
    /// Source byte length.
    pub byte_len: i64,
    /// Stable content hash for the indexed source.
    pub content_hash: String,
    /// Snapshot write time in Unix milliseconds.
    pub indexed_at_unix_ms: Option<i64>,
}

/// Query-facing zettel row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StoredZettel {
    /// SQLite row ID.
    pub id: i64,
    /// Owning file row ID.
    pub file_id: i64,
    /// Parent zettel row ID when this zettel is nested.
    pub parent_id: Option<i64>,
    /// Preorder source position within the file.
    pub source_order: i64,
    /// Source hierarchy role.
    pub kind: String,
    /// Parser-local key.
    pub parser_key: String,
    /// Plain title text.
    pub title: Option<String>,
    /// Resolved canonical ID when available.
    pub canonical_id: Option<String>,
    /// Local ID declaration when available.
    pub local_id: Option<String>,
    /// Direct body text retained for MVP text filters.
    pub body_text: String,
    /// Source span start byte.
    pub start_byte: i64,
    /// Source span end byte.
    pub end_byte: i64,
}

/// Query-facing link row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StoredLink {
    /// SQLite row ID.
    pub id: i64,
    /// Source zettel row ID.
    pub source_zettel_id: i64,
    /// Target zettel row ID for resolved links that point into this snapshot.
    pub target_zettel_id: Option<i64>,
    /// Canonical target ID for resolved links.
    pub target_canonical_id: Option<String>,
    /// Link text as written.
    pub target_text: String,
    /// Link form.
    pub link_kind: String,
    /// True when semantic resolution succeeded.
    pub resolved: bool,
    /// Source span start byte.
    pub start_byte: i64,
    /// Source span end byte.
    pub end_byte: i64,
}

/// Query-facing tag row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StoredTag {
    /// SQLite row ID.
    pub id: i64,
    /// Owning zettel row ID.
    pub zettel_id: i64,
    /// Tag text without the leading `#`.
    pub tag: String,
    /// `explicit` or `type`.
    pub tag_kind: String,
}

/// Query-facing effective tag row with inheritance provenance.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StoredEffectiveTag {
    /// SQLite row ID.
    pub id: i64,
    /// Zettel row receiving the effective tag.
    pub zettel_id: i64,
    /// Tag text without the leading `#`.
    pub tag: String,
    /// Zettel row that contributed the tag.
    pub source_zettel_id: Option<i64>,
    /// Provenance class: `explicit`, `parent`, or `path`.
    pub source: String,
}

/// Query-facing property row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StoredProperty {
    /// SQLite row ID.
    pub id: i64,
    /// Owning zettel row ID.
    pub zettel_id: i64,
    /// Property key.
    pub key: String,
    /// Property value.
    pub value: String,
}

/// Query-facing todo row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StoredTodo {
    /// SQLite row ID.
    pub id: i64,
    /// Owning zettel row ID.
    pub zettel_id: i64,
    /// Todo marker text as written.
    pub marker: String,
}

/// Query-facing diagnostic row.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StoredDiagnostic {
    /// SQLite row ID.
    pub id: i64,
    /// Owning file row ID when known.
    pub file_id: Option<i64>,
    /// Absolute source file path when known.
    pub absolute_path: Option<PathBuf>,
    /// Source file path relative to the corpus root when known.
    pub relative_path: Option<PathBuf>,
    /// Owning zettel row ID when known.
    pub zettel_id: Option<i64>,
    /// Diagnostic severity.
    pub severity: String,
    /// Diagnostic category.
    pub category: String,
    /// Stable diagnostic code.
    pub code: Option<String>,
    /// Human-readable diagnostic message.
    pub message: String,
    /// Source span start byte when known.
    pub start_byte: Option<i64>,
    /// Source span end byte when known.
    pub end_byte: Option<i64>,
    /// One-based source span start line when known.
    pub start_line: Option<i64>,
    /// One-based source span start column when known.
    pub start_column: Option<i64>,
    /// One-based source span end line when known.
    pub end_line: Option<i64>,
    /// One-based source span end column when known.
    pub end_column: Option<i64>,
}

/// Paths used when opening a Zorg SQLite store.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StoreOptions {
    corpus_root: PathBuf,
    database_path: PathBuf,
}

impl StoreOptions {
    /// Builds options from explicit corpus root and database paths.
    pub fn new(
        corpus_root: impl Into<PathBuf>,
        database_path: impl Into<PathBuf>,
    ) -> ZorgResult<Self> {
        let corpus_root = corpus_root.into();
        let database_path = database_path.into();
        validate_non_empty_path(&corpus_root, "store root")?;
        validate_non_empty_path(&database_path, "database path")?;

        Ok(Self {
            corpus_root,
            database_path,
        })
    }

    /// Builds options for an explicit root using the default database path below that root.
    pub fn for_root(corpus_root: impl Into<PathBuf>) -> ZorgResult<Self> {
        let corpus_root = corpus_root.into();
        validate_non_empty_path(&corpus_root, "store root")?;
        let database_path = Self::default_database_path(&corpus_root);
        Self::new(corpus_root, database_path)
    }

    /// Builds options for the default `~/zorg` corpus root.
    pub fn default_paths() -> ZorgResult<Self> {
        Self::for_root(Self::default_root()?)
    }

    /// Returns the default corpus root, `~/zorg`.
    pub fn default_root() -> ZorgResult<PathBuf> {
        let home = env::var_os("HOME").ok_or_else(|| {
            operation_failed("could not determine home directory for default store root")
        })?;
        let home = PathBuf::from(home);
        validate_non_empty_path(&home, "home directory")?;
        Ok(home.join(DEFAULT_ROOT_DIR))
    }

    /// Returns the default SQLite path for a corpus root.
    #[must_use]
    pub fn default_database_path(corpus_root: impl AsRef<Path>) -> PathBuf {
        corpus_root
            .as_ref()
            .join(DEFAULT_DB_DIR)
            .join(DEFAULT_DB_FILE)
    }

    /// Returns the configured corpus root.
    #[must_use]
    pub fn corpus_root(&self) -> &Path {
        &self.corpus_root
    }

    /// Returns the configured database path.
    #[must_use]
    pub fn database_path(&self) -> &Path {
        &self.database_path
    }
}

/// CLI overrides that sit at the top of Zorg config precedence.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct ConfigOverrides {
    /// Explicit corpus root supplied by the caller.
    pub root: Option<PathBuf>,
    /// Explicit SQLite database path supplied by the caller.
    pub database_path: Option<PathBuf>,
}

/// Inputs used to resolve a deterministic Zorg config.
#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct ConfigResolutionInputs {
    /// CLI-level overrides.
    pub cli: ConfigOverrides,
    /// Environment-provided corpus root, normally `ZORG_ROOT`.
    pub env_root: Option<PathBuf>,
    /// Environment-provided database path, normally `ZORG_DATABASE_PATH` or `ZORG_DB`.
    pub env_database_path: Option<PathBuf>,
    /// Environment-provided watcher debounce milliseconds.
    pub env_watcher_debounce_ms: Option<String>,
    /// Environment-provided watcher log path.
    pub env_watcher_log_path: Option<PathBuf>,
    /// Home directory used for defaults and `~` expansion.
    pub home_dir: Option<PathBuf>,
    /// XDG config home used to find user config.
    pub xdg_config_home: Option<PathBuf>,
    /// Test hook for an exact user config path. Normal callers leave this unset.
    pub user_config_path: Option<PathBuf>,
}

impl ConfigResolutionInputs {
    /// Builds resolution inputs from process environment plus explicit CLI overrides.
    #[must_use]
    pub fn from_env(cli: ConfigOverrides) -> Self {
        Self {
            cli,
            env_root: env::var_os("ZORG_ROOT").map(PathBuf::from),
            env_database_path: env::var_os("ZORG_DATABASE_PATH")
                .or_else(|| env::var_os("ZORG_DB"))
                .map(PathBuf::from),
            env_watcher_debounce_ms: env::var("ZORG_WATCHER_DEBOUNCE_MS").ok(),
            env_watcher_log_path: env::var_os("ZORG_WATCHER_LOG_PATH").map(PathBuf::from),
            home_dir: env::var_os("HOME").map(PathBuf::from),
            xdg_config_home: env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            user_config_path: None,
        }
    }
}

/// Fully resolved Zorg configuration.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ResolvedConfig {
    store_options: StoreOptions,
    watcher_debounce_ms: u64,
    watcher_log_path: Option<PathBuf>,
    named_roots: BTreeMap<String, PathBuf>,
}

impl ResolvedConfig {
    /// Resolves config with precedence:
    /// CLI flags > environment variables > root-local `.zorg/config.toml` > user config > defaults.
    pub fn resolve(inputs: ConfigResolutionInputs) -> ZorgResult<Self> {
        let home_dir = inputs
            .home_dir
            .as_ref()
            .filter(|path| !path.as_os_str().is_empty())
            .map(|path| expand_home(path, None));
        let default_root = home_dir.as_ref().map(|home| home.join(DEFAULT_ROOT_DIR));
        let user_config_path = user_config_path(&inputs, home_dir.as_ref());
        let user_config = load_config_file(&user_config_path)?;

        let env_root = inputs
            .env_root
            .as_ref()
            .map(|path| expand_home(path, home_dir.as_ref()));
        let env_database_path = inputs
            .env_database_path
            .as_ref()
            .map(|path| expand_home(path, home_dir.as_ref()));
        let env_watcher_log_path = inputs
            .env_watcher_log_path
            .as_ref()
            .map(|path| expand_home(path, home_dir.as_ref()));
        let env_watcher_debounce_ms = parse_env_debounce(&inputs.env_watcher_debounce_ms)?;
        let cli_root = inputs
            .cli
            .root
            .as_ref()
            .map(|path| expand_home(path, home_dir.as_ref()));
        let cli_database_path = inputs
            .cli
            .database_path
            .as_ref()
            .map(|path| expand_home(path, home_dir.as_ref()));

        let user_root = user_config
            .root
            .as_ref()
            .map(|path| expand_home(path, home_dir.as_ref()));
        let provisional_root = cli_root
            .clone()
            .or_else(|| env_root.clone())
            .or_else(|| user_root.clone())
            .or_else(|| default_root.clone())
            .ok_or_else(|| {
                operation_failed("could not determine home directory for default store root")
            })?;
        let root_config_path = provisional_root.join(DEFAULT_DB_DIR).join("config.toml");
        let root_config = load_config_file(&root_config_path)?;

        let root = cli_root
            .or(env_root)
            .or_else(|| {
                root_config
                    .root
                    .as_ref()
                    .map(|path| expand_home(path, home_dir.as_ref()))
            })
            .or(user_root)
            .or(default_root)
            .ok_or_else(|| {
                operation_failed("could not determine home directory for default store root")
            })?;
        let database_path = cli_database_path
            .or(env_database_path)
            .or_else(|| {
                root_config
                    .database_path
                    .as_ref()
                    .map(|path| expand_home(path, home_dir.as_ref()))
            })
            .or_else(|| {
                user_config
                    .database_path
                    .as_ref()
                    .map(|path| expand_home(path, home_dir.as_ref()))
            })
            .unwrap_or_else(|| StoreOptions::default_database_path(&root));
        let watcher_debounce_ms = env_watcher_debounce_ms
            .or(root_config.watcher_debounce_ms)
            .or(user_config.watcher_debounce_ms)
            .unwrap_or(DEFAULT_WATCHER_DEBOUNCE_MS);
        let watcher_log_path = env_watcher_log_path
            .or_else(|| {
                root_config
                    .watcher_log_path
                    .as_ref()
                    .map(|path| expand_home(path, home_dir.as_ref()))
            })
            .or_else(|| {
                user_config
                    .watcher_log_path
                    .as_ref()
                    .map(|path| expand_home(path, home_dir.as_ref()))
            });
        let named_roots = merged_named_roots(&user_config, &root_config, home_dir.as_ref())?;

        Ok(Self {
            store_options: StoreOptions::new(root, database_path)?,
            watcher_debounce_ms,
            watcher_log_path,
            named_roots,
        })
    }

    /// Resolves config from process environment plus explicit CLI overrides.
    pub fn from_env(cli: ConfigOverrides) -> ZorgResult<Self> {
        Self::resolve(ConfigResolutionInputs::from_env(cli))
    }

    /// Returns the canonical store paths consumed by store APIs.
    #[must_use]
    pub fn store_options(&self) -> &StoreOptions {
        &self.store_options
    }

    /// Consumes this config and returns its canonical store paths.
    #[must_use]
    pub fn into_store_options(self) -> StoreOptions {
        self.store_options
    }

    /// Returns watcher debounce in milliseconds for later watcher integrations.
    #[must_use]
    pub const fn watcher_debounce_ms(&self) -> u64 {
        self.watcher_debounce_ms
    }

    /// Returns the optional watcher log path.
    #[must_use]
    pub fn watcher_log_path(&self) -> Option<&Path> {
        self.watcher_log_path.as_deref()
    }

    /// Returns configured named roots.
    #[must_use]
    pub fn named_roots(&self) -> &BTreeMap<String, PathBuf> {
        &self.named_roots
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct ConfigFile {
    root: Option<PathBuf>,
    database_path: Option<PathBuf>,
    watcher_debounce_ms: Option<u64>,
    watcher_log_path: Option<PathBuf>,
    named_roots: Option<BTreeMap<String, PathBuf>>,
}

fn load_config_file(path: &Path) -> ZorgResult<ConfigFile> {
    if !path.exists() {
        return Ok(ConfigFile::default());
    }

    let source = std::fs::read_to_string(path).map_err(|error| {
        operation_failed(format!(
            "failed to read Zorg config {}: {error}",
            path.display()
        ))
    })?;
    toml::from_str(&source).map_err(|error| {
        operation_failed(format!(
            "failed to parse Zorg config {}: {error}",
            path.display()
        ))
    })
}

fn user_config_path(inputs: &ConfigResolutionInputs, home_dir: Option<&PathBuf>) -> PathBuf {
    if let Some(path) = &inputs.user_config_path {
        return path.clone();
    }

    inputs
        .xdg_config_home
        .as_ref()
        .map(|path| expand_home(path, home_dir))
        .or_else(|| home_dir.map(|home| home.join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("zorg")
        .join("config.toml")
}

fn expand_home(path: &Path, home_dir: Option<&PathBuf>) -> PathBuf {
    let text = path.to_string_lossy();
    if text == "~" {
        return home_dir.cloned().unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = text.strip_prefix("~/") {
        return home_dir
            .map(|home| home.join(rest))
            .unwrap_or_else(|| path.to_path_buf());
    }
    path.to_path_buf()
}

fn parse_env_debounce(value: &Option<String>) -> ZorgResult<Option<u64>> {
    value
        .as_ref()
        .map(|value| {
            value.parse::<u64>().map_err(|error| {
                operation_failed(format!(
                    "invalid ZORG_WATCHER_DEBOUNCE_MS value {value:?}: {error}"
                ))
            })
        })
        .transpose()
}

fn merged_named_roots(
    user_config: &ConfigFile,
    root_config: &ConfigFile,
    home_dir: Option<&PathBuf>,
) -> ZorgResult<BTreeMap<String, PathBuf>> {
    let mut named_roots = BTreeMap::new();
    merge_named_roots(&mut named_roots, user_config, home_dir)?;
    merge_named_roots(&mut named_roots, root_config, home_dir)?;
    Ok(named_roots)
}

fn merge_named_roots(
    target: &mut BTreeMap<String, PathBuf>,
    config: &ConfigFile,
    home_dir: Option<&PathBuf>,
) -> ZorgResult<()> {
    let Some(named_roots) = &config.named_roots else {
        return Ok(());
    };

    for (name, path) in named_roots {
        validate_named_root(name)?;
        if target
            .insert(name.clone(), expand_home(path, home_dir))
            .is_some()
        {
            return Err(operation_failed(format!("duplicate named root {name:?}")));
        }
    }

    Ok(())
}

fn validate_named_root(name: &str) -> ZorgResult<()> {
    if name.is_empty() {
        return Err(operation_failed("named root names must not be empty"));
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return Err(operation_failed(format!(
            "invalid named root {name:?}; use only ASCII letters, numbers, '-' and '_'"
        )));
    }
    Ok(())
}

/// SQLite-backed Zorg store handle.
#[derive(Debug)]
pub struct Store {
    options: StoreOptions,
    connection: Connection,
}

impl Store {
    /// Opens a store rooted at the supplied corpus path with the default database path.
    pub fn open(root: impl Into<PathBuf>) -> ZorgResult<Self> {
        Self::open_with_options(StoreOptions::for_root(root)?)
    }

    /// Opens the default store rooted at `~/zorg`.
    pub fn open_default() -> ZorgResult<Self> {
        Self::open_with_options(StoreOptions::default_paths()?)
    }

    /// Opens a SQLite store and runs deterministic embedded migrations.
    pub fn open_with_options(options: StoreOptions) -> ZorgResult<Self> {
        if let Some(parent) = options.database_path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|error| {
                operation_failed(format!(
                    "failed to create database directory {}: {error}",
                    parent.display()
                ))
            })?;
        }

        let mut connection = Connection::open(&options.database_path).map_err(|error| {
            operation_failed(format!(
                "failed to open SQLite database {}: {error}",
                options.database_path.display()
            ))
        })?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|error| {
                operation_failed(format!("failed to enable SQLite foreign keys: {error}"))
            })?;

        run_migrations(&mut connection)?;

        Ok(Self {
            options,
            connection,
        })
    }

    /// Returns the configured corpus root.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.options.corpus_root()
    }

    /// Returns the configured database path.
    #[must_use]
    pub fn database_path(&self) -> &Path {
        self.options.database_path()
    }

    /// Returns the schema version recorded in the database.
    pub fn schema_version(&self) -> ZorgResult<i64> {
        schema_version(&self.connection)
    }

    /// Discovers canonical `.z` source files under the configured corpus root.
    pub fn discover_sources(&self) -> ZorgResult<Vec<CorpusSource>> {
        discover_corpus_sources(self.root())
    }

    /// Returns index status without mutating indexed rows.
    pub fn index_status(&self) -> ZorgResult<IndexStatus> {
        let sources = self.discover_sources()?;
        let snapshots = read_source_snapshots(sources)?;
        let indexed_files = indexed_file_states(&self.connection)?;
        let changes = classify_file_changes(&snapshots, &indexed_files);

        Ok(IndexStatus {
            discovered_files: snapshots.len(),
            indexed_files: indexed_files.len(),
            unchanged_files: changes.unchanged_files,
            new_files: changes.new_files,
            changed_files: changes.changed_files,
            deleted_files: changes.deleted_files,
            diagnostic_count: diagnostic_count(&self.connection)?,
            effective_tag_count: effective_tag_count(&self.connection)?,
            last_indexed_at_unix_ms: last_indexed_at_unix_ms(&self.connection)?,
        })
    }

    /// Incrementally refreshes the indexed snapshot for this store's corpus root.
    pub fn reindex(&mut self) -> ZorgResult<ReindexSummary> {
        let sources = self.discover_sources()?;
        let snapshots = read_source_snapshots(sources)?;
        let indexed_files = indexed_file_states(&self.connection)?;
        let changes = classify_file_changes(&snapshots, &indexed_files);

        if changes.new_files == 0 && changes.changed_files == 0 && changes.deleted_files == 0 {
            refresh_unchanged_file_metadata(&self.connection, &snapshots)?;
            return Ok(ReindexSummary {
                discovered_files: snapshots.len(),
                indexed_files: 0,
                unchanged_files: changes.unchanged_files,
                new_files: 0,
                changed_files: 0,
                deleted_files: 0,
                zettel_count: 0,
                diagnostic_count: diagnostic_count(&self.connection)?,
                effective_tag_count: effective_tag_count(&self.connection)?,
                last_indexed_at_unix_ms: last_indexed_at_unix_ms(&self.connection)?,
            });
        }

        let mut documents = snapshots
            .iter()
            .map(|snapshot| {
                zorg_parse::parse_zettel_document_with_path(
                    &snapshot.source_text,
                    snapshot.source.absolute_path(),
                )
                .map_err(|error| {
                    operation_failed(format!(
                        "failed to parse source {}: {error}",
                        snapshot.source.absolute_path().display()
                    ))
                })
            })
            .collect::<ZorgResult<Vec<_>>>()?;

        let validation = zorg_parse::validate_corpus(&documents);
        let validation_diagnostics = validation.diagnostics;
        let resolution = zorg_parse::resolve_corpus(&mut documents);
        let resolution_diagnostics = resolution.diagnostics;
        let indexed_at_unix_ms = now_unix_ms();

        let transaction = self.connection.transaction().map_err(|error| {
            operation_failed(format!(
                "failed to begin incremental reindex transaction: {error}"
            ))
        })?;
        apply_incremental_snapshot(
            &transaction,
            &snapshots,
            &documents,
            &indexed_files,
            &validation_diagnostics,
            &resolution_diagnostics,
            indexed_at_unix_ms,
        )
        .and_then(|summary| {
            transaction.commit().map_err(|error| {
                operation_failed(format!(
                    "failed to commit incremental reindex transaction: {error}"
                ))
            })?;
            Ok(summary)
        })
    }

    /// Rebuilds the indexed snapshot for this store's corpus root in one transaction.
    pub fn reindex_full(&mut self) -> ZorgResult<ReindexSummary> {
        let sources = self.discover_sources()?;
        let files = read_source_snapshots(sources)?;
        let mut documents = Vec::with_capacity(files.len());

        for file in &files {
            let document = zorg_parse::parse_zettel_document_with_path(
                &file.source_text,
                file.source.absolute_path(),
            )
            .map_err(|error| {
                operation_failed(format!(
                    "failed to parse source {}: {error}",
                    file.source.absolute_path().display()
                ))
            })?;

            documents.push(document);
        }

        let validation = zorg_parse::validate_corpus(&documents);
        let validation_diagnostics = validation.diagnostics;
        let resolution = zorg_parse::resolve_corpus(&mut documents);
        let resolution_diagnostics = resolution.diagnostics;
        let indexed_at_unix_ms = now_unix_ms();

        let transaction = self.connection.transaction().map_err(|error| {
            operation_failed(format!(
                "failed to begin snapshot reindex transaction: {error}"
            ))
        })?;
        replace_snapshot(
            &transaction,
            &files,
            &documents,
            &validation_diagnostics,
            &resolution_diagnostics,
            indexed_at_unix_ms,
        )
        .and_then(|summary| {
            transaction.commit().map_err(|error| {
                operation_failed(format!(
                    "failed to commit snapshot reindex transaction: {error}"
                ))
            })?;
            Ok(summary)
        })
    }

    /// Returns indexed source files in deterministic relative-path order.
    pub fn list_files(&self) -> ZorgResult<Vec<StoredFile>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, absolute_path, relative_path, mtime_unix_ms, byte_len, content_hash,
                        indexed_at_unix_ms
                 FROM files
                 ORDER BY relative_path",
            )
            .map_err(|error| operation_failed(format!("failed to prepare file query: {error}")))?;
        let rows = statement
            .query_map([], |row| {
                Ok(StoredFile {
                    id: row.get(0)?,
                    absolute_path: PathBuf::from(row.get::<_, String>(1)?),
                    relative_path: PathBuf::from(row.get::<_, String>(2)?),
                    mtime_unix_ms: row.get(3)?,
                    byte_len: row.get(4)?,
                    content_hash: row.get(5)?,
                    indexed_at_unix_ms: row.get(6)?,
                })
            })
            .map_err(|error| operation_failed(format!("failed to list files: {error}")))?;

        collect_rows(rows, "failed to read indexed file")
    }

    /// Returns indexed zettel in deterministic file/source order.
    pub fn list_zettel(&self) -> ZorgResult<Vec<StoredZettel>> {
        query_zettel(
            &self.connection,
            "SELECT z.id, z.file_id, z.parent_id, z.source_order, z.kind, z.parser_key, z.title,
                    z.canonical_id, z.local_id, z.body_text, z.start_byte, z.end_byte
             FROM zettel z
             JOIN files f ON f.id = z.file_id
             ORDER BY f.relative_path, z.source_order",
            [],
        )
    }

    /// Looks up one indexed zettel by canonical ID.
    pub fn lookup_zettel_by_canonical_id(
        &self,
        canonical_id: &str,
    ) -> ZorgResult<Option<StoredZettel>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT z.id, z.file_id, z.parent_id, z.source_order, z.kind, z.parser_key, z.title,
                        z.canonical_id, z.local_id, z.body_text, z.start_byte, z.end_byte
                 FROM zettel_ids zi
                 JOIN zettel z ON z.id = zi.zettel_id
                 WHERE zi.canonical_id = ?1",
            )
            .map_err(|error| operation_failed(format!("failed to prepare zettel lookup: {error}")))?;
        statement
            .query_row([canonical_id], stored_zettel_from_row)
            .optional()
            .map_err(|error| operation_failed(format!("failed to lookup zettel: {error}")))
    }

    /// Returns indexed links in deterministic source order.
    pub fn list_links(&self) -> ZorgResult<Vec<StoredLink>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, source_zettel_id, target_zettel_id, target_canonical_id, target_text,
                        link_kind, resolved, start_byte, end_byte
                 FROM links
                 ORDER BY source_zettel_id, start_byte, id",
            )
            .map_err(|error| operation_failed(format!("failed to prepare link query: {error}")))?;
        let rows = statement
            .query_map([], |row| {
                Ok(StoredLink {
                    id: row.get(0)?,
                    source_zettel_id: row.get(1)?,
                    target_zettel_id: row.get(2)?,
                    target_canonical_id: row.get(3)?,
                    target_text: row.get(4)?,
                    link_kind: row.get(5)?,
                    resolved: row.get::<_, i64>(6)? != 0,
                    start_byte: row.get(7)?,
                    end_byte: row.get(8)?,
                })
            })
            .map_err(|error| operation_failed(format!("failed to list links: {error}")))?;

        collect_rows(rows, "failed to read indexed link")
    }

    /// Returns indexed explicit and type tags.
    pub fn list_tags(&self) -> ZorgResult<Vec<StoredTag>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, zettel_id, tag, tag_kind FROM tags ORDER BY zettel_id, tag")
            .map_err(|error| operation_failed(format!("failed to prepare tag query: {error}")))?;
        let rows = statement
            .query_map([], |row| {
                Ok(StoredTag {
                    id: row.get(0)?,
                    zettel_id: row.get(1)?,
                    tag: row.get(2)?,
                    tag_kind: row.get(3)?,
                })
            })
            .map_err(|error| operation_failed(format!("failed to list tags: {error}")))?;

        collect_rows(rows, "failed to read indexed tag")
    }

    /// Returns indexed explicit and type tags for one zettel.
    pub fn list_tags_for_zettel(&self, zettel_id: i64) -> ZorgResult<Vec<StoredTag>> {
        query_tags(
            &self.connection,
            "SELECT id, zettel_id, tag, tag_kind
             FROM tags
             WHERE zettel_id = ?1
             ORDER BY tag, tag_kind, id",
            [zettel_id],
        )
    }

    /// Returns materialized effective tags in deterministic order.
    pub fn list_effective_tags(&self) -> ZorgResult<Vec<StoredEffectiveTag>> {
        query_effective_tags(
            &self.connection,
            "SELECT id, zettel_id, tag, source_zettel_id, source
             FROM effective_tags
             ORDER BY zettel_id, tag, source, source_zettel_id",
            [],
        )
    }

    /// Returns materialized effective tags for one zettel.
    pub fn list_effective_tags_for_zettel(
        &self,
        zettel_id: i64,
    ) -> ZorgResult<Vec<StoredEffectiveTag>> {
        query_effective_tags(
            &self.connection,
            "SELECT id, zettel_id, tag, source_zettel_id, source
             FROM effective_tags
             WHERE zettel_id = ?1
             ORDER BY tag, source, source_zettel_id",
            [zettel_id],
        )
    }

    /// Returns ancestors of a zettel from root-most parent to direct parent.
    pub fn list_zettel_ancestors(&self, zettel_id: i64) -> ZorgResult<Vec<StoredZettel>> {
        query_zettel(
            &self.connection,
            "WITH RECURSIVE ancestors(depth, id) AS (
                SELECT 1, parent_id
                FROM zettel
                WHERE id = ?1 AND parent_id IS NOT NULL
                UNION ALL
                SELECT ancestors.depth + 1, z.parent_id
                FROM ancestors
                JOIN zettel z ON z.id = ancestors.id
                WHERE z.parent_id IS NOT NULL
             )
             SELECT z.id, z.file_id, z.parent_id, z.source_order, z.kind, z.parser_key, z.title,
                    z.canonical_id, z.local_id, z.body_text, z.start_byte, z.end_byte
             FROM ancestors
             JOIN zettel z ON z.id = ancestors.id
             ORDER BY ancestors.depth DESC",
            [zettel_id],
        )
    }

    /// Returns descendants of a zettel in file/source order.
    pub fn list_zettel_descendants(&self, zettel_id: i64) -> ZorgResult<Vec<StoredZettel>> {
        query_zettel(
            &self.connection,
            "WITH RECURSIVE descendants(id) AS (
                SELECT id
                FROM zettel
                WHERE parent_id = ?1
                UNION ALL
                SELECT z.id
                FROM zettel z
                JOIN descendants d ON z.parent_id = d.id
             )
             SELECT z.id, z.file_id, z.parent_id, z.source_order, z.kind, z.parser_key, z.title,
                    z.canonical_id, z.local_id, z.body_text, z.start_byte, z.end_byte
             FROM descendants
             JOIN zettel z ON z.id = descendants.id
             JOIN files f ON f.id = z.file_id
             ORDER BY f.relative_path, z.source_order",
            [zettel_id],
        )
    }

    /// Returns outgoing links for one source zettel.
    pub fn list_outgoing_links(&self, zettel_id: i64) -> ZorgResult<Vec<StoredLink>> {
        query_links(
            &self.connection,
            "SELECT id, source_zettel_id, target_zettel_id, target_canonical_id, target_text,
                    link_kind, resolved, start_byte, end_byte
             FROM links
             WHERE source_zettel_id = ?1
             ORDER BY start_byte, id",
            [zettel_id],
        )
    }

    /// Returns incoming resolved links for one target zettel.
    pub fn list_incoming_links(&self, zettel_id: i64) -> ZorgResult<Vec<StoredLink>> {
        query_links(
            &self.connection,
            "SELECT id, source_zettel_id, target_zettel_id, target_canonical_id, target_text,
                    link_kind, resolved, start_byte, end_byte
             FROM links
             WHERE target_zettel_id = ?1
             ORDER BY source_zettel_id, start_byte, id",
            [zettel_id],
        )
    }

    /// Returns indexed properties.
    pub fn list_properties(&self) -> ZorgResult<Vec<StoredProperty>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, zettel_id, key, value FROM properties ORDER BY zettel_id, key, id")
            .map_err(|error| {
                operation_failed(format!("failed to prepare property query: {error}"))
            })?;
        let rows = statement
            .query_map([], |row| {
                Ok(StoredProperty {
                    id: row.get(0)?,
                    zettel_id: row.get(1)?,
                    key: row.get(2)?,
                    value: row.get(3)?,
                })
            })
            .map_err(|error| operation_failed(format!("failed to list properties: {error}")))?;

        collect_rows(rows, "failed to read indexed property")
    }

    /// Returns indexed todos.
    pub fn list_todos(&self) -> ZorgResult<Vec<StoredTodo>> {
        let mut statement = self
            .connection
            .prepare("SELECT id, zettel_id, marker FROM todos ORDER BY zettel_id")
            .map_err(|error| operation_failed(format!("failed to prepare todo query: {error}")))?;
        let rows = statement
            .query_map([], |row| {
                Ok(StoredTodo {
                    id: row.get(0)?,
                    zettel_id: row.get(1)?,
                    marker: row.get(2)?,
                })
            })
            .map_err(|error| operation_failed(format!("failed to list todos: {error}")))?;

        collect_rows(rows, "failed to read indexed todo")
    }

    /// Returns indexed diagnostics.
    pub fn list_diagnostics(&self) -> ZorgResult<Vec<StoredDiagnostic>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT d.id, d.file_id, f.absolute_path, f.relative_path, d.zettel_id,
                        d.severity, d.category, d.code, d.message, d.start_byte, d.end_byte,
                        d.start_line, d.start_column, d.end_line, d.end_column
                 FROM diagnostics d
                 LEFT JOIN files f ON f.id = d.file_id
                 ORDER BY d.id",
            )
            .map_err(|error| {
                operation_failed(format!("failed to prepare diagnostic query: {error}"))
            })?;
        let rows = statement
            .query_map([], |row| {
                Ok(StoredDiagnostic {
                    id: row.get(0)?,
                    file_id: row.get(1)?,
                    absolute_path: row.get::<_, Option<String>>(2)?.map(PathBuf::from),
                    relative_path: row.get::<_, Option<String>>(3)?.map(PathBuf::from),
                    zettel_id: row.get(4)?,
                    severity: row.get(5)?,
                    category: row.get(6)?,
                    code: row.get(7)?,
                    message: row.get(8)?,
                    start_byte: row.get(9)?,
                    end_byte: row.get(10)?,
                    start_line: row.get(11)?,
                    start_column: row.get(12)?,
                    end_line: row.get(13)?,
                    end_column: row.get(14)?,
                })
            })
            .map_err(|error| operation_failed(format!("failed to list diagnostics: {error}")))?;

        collect_rows(rows, "failed to read indexed diagnostic")
    }
}

/// Returns true when `path` has the canonical Zorg source extension, `.z`.
#[must_use]
pub fn is_canonical_source_path(path: impl AsRef<Path>) -> bool {
    path.as_ref().extension().is_some_and(|extension| {
        extension
            .to_str()
            .is_some_and(|extension| extension == CANONICAL_SOURCE_EXTENSION)
    })
}

/// Validates an explicit source path passed by a caller.
///
/// Corpus discovery ignores non-`.z` files, but explicit source arguments should
/// fail clearly so callers do not accidentally rely on legacy file formats.
pub fn validate_explicit_source_path(path: impl AsRef<Path>) -> ZorgResult<()> {
    let path = path.as_ref();
    validate_non_empty_path(path, "source path")?;

    if is_canonical_source_path(path) {
        return Ok(());
    }

    let extension = path.extension().and_then(|extension| extension.to_str());
    let detail = match extension {
        Some(extension) if UNSUPPORTED_LEGACY_EXTENSIONS.contains(&extension) => {
            format!(".{extension} is not a canonical Zorg source extension")
        }
        Some(extension) => format!(".{extension} is not a supported Zorg source extension"),
        None => "missing .z source extension".to_owned(),
    };

    Err(operation_failed(format!(
        "unsupported source path {}: {detail}; expected .z",
        path.display()
    )))
}

/// Recursively discovers canonical `.z` source files below a corpus root.
///
/// Non-`.z` files are ignored during discovery. The returned list is sorted by
/// relative path for deterministic indexing and CLI output.
pub fn discover_corpus_sources(root: impl AsRef<Path>) -> ZorgResult<Vec<CorpusSource>> {
    let root = root.as_ref();
    validate_non_empty_path(root, "store root")?;
    let root = root.canonicalize().map_err(|error| {
        operation_failed(format!(
            "failed to resolve corpus root {}: {error}",
            root.display()
        ))
    })?;

    let mut sources = Vec::new();
    discover_corpus_sources_in(&root, &root, &mut sources)?;
    sources.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(sources)
}

fn discover_corpus_sources_in(
    root: &Path,
    directory: &Path,
    sources: &mut Vec<CorpusSource>,
) -> ZorgResult<()> {
    let entries = std::fs::read_dir(directory).map_err(|error| {
        operation_failed(format!(
            "failed to read corpus directory {}: {error}",
            directory.display()
        ))
    })?;
    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| operation_failed(format!("failed to read corpus entry: {error}")))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let file_type = entry.file_type().map_err(|error| {
            operation_failed(format!(
                "failed to inspect corpus entry {}: {error}",
                entry.path().display()
            ))
        })?;
        let path = entry.path();

        if file_type.is_dir() {
            discover_corpus_sources_in(root, &path, sources)?;
        } else if file_type.is_file() && is_canonical_source_path(&path) {
            let relative_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            sources.push(CorpusSource::new(path, relative_path));
        }
    }

    Ok(())
}

fn read_source_snapshots(sources: Vec<CorpusSource>) -> ZorgResult<Vec<SourceSnapshot>> {
    sources
        .into_iter()
        .map(read_source_snapshot)
        .collect::<ZorgResult<Vec<_>>>()
}

fn read_source_snapshot(source: CorpusSource) -> ZorgResult<SourceSnapshot> {
    let source_text = std::fs::read_to_string(source.absolute_path()).map_err(|error| {
        operation_failed(format!(
            "failed to read source {}: {error}",
            source.absolute_path().display()
        ))
    })?;
    let metadata = std::fs::metadata(source.absolute_path()).map_err(|error| {
        operation_failed(format!(
            "failed to inspect source {}: {error}",
            source.absolute_path().display()
        ))
    })?;

    Ok(SourceSnapshot {
        mtime_unix_ms: modified_unix_ms(&metadata),
        byte_len: i64::try_from(source_text.len()).unwrap_or(i64::MAX),
        content_hash: content_hash(source_text.as_bytes()),
        source_text,
        source,
    })
}

fn indexed_file_states(connection: &Connection) -> ZorgResult<BTreeMap<PathBuf, IndexedFileState>> {
    let mut statement = connection
        .prepare(
            "SELECT relative_path, content_hash
             FROM files
             ORDER BY relative_path",
        )
        .map_err(|error| {
            operation_failed(format!("failed to prepare indexed file query: {error}"))
        })?;
    let rows = statement
        .query_map([], |row| {
            let relative_path = PathBuf::from(row.get::<_, String>(0)?);
            Ok((
                relative_path.clone(),
                IndexedFileState {
                    relative_path,
                    content_hash: row.get(1)?,
                },
            ))
        })
        .map_err(|error| operation_failed(format!("failed to query indexed files: {error}")))?;

    rows.collect::<rusqlite::Result<BTreeMap<_, _>>>()
        .map_err(|error| operation_failed(format!("failed to read indexed files: {error}")))
}

fn classify_file_changes(
    snapshots: &[SourceSnapshot],
    indexed_files: &BTreeMap<PathBuf, IndexedFileState>,
) -> FileChangeCounts {
    let mut unchanged_files = 0;
    let mut new_files = 0;
    let mut changed_files = 0;
    let discovered_paths = snapshots
        .iter()
        .map(|snapshot| snapshot.source.relative_path().to_path_buf())
        .collect::<BTreeSet<_>>();

    for snapshot in snapshots {
        match indexed_files.get(snapshot.source.relative_path()) {
            None => new_files += 1,
            Some(indexed) if indexed.content_hash == snapshot.content_hash => unchanged_files += 1,
            Some(_) => changed_files += 1,
        }
    }

    let deleted_files = indexed_files
        .keys()
        .filter(|path| !discovered_paths.contains(*path))
        .count();

    FileChangeCounts {
        unchanged_files,
        new_files,
        changed_files,
        deleted_files,
    }
}

fn file_change_kind(
    snapshot: &SourceSnapshot,
    indexed_files: &BTreeMap<PathBuf, IndexedFileState>,
) -> FileChangeKind {
    match indexed_files.get(snapshot.source.relative_path()) {
        None => FileChangeKind::New,
        Some(indexed) if indexed.content_hash == snapshot.content_hash => FileChangeKind::Unchanged,
        Some(_) => FileChangeKind::Changed,
    }
}

fn diagnostic_count(connection: &Connection) -> ZorgResult<usize> {
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM diagnostics", [], |row| row.get(0))
        .map_err(|error| operation_failed(format!("failed to count diagnostics: {error}")))?;
    Ok(usize::try_from(count).unwrap_or(usize::MAX))
}

fn effective_tag_count(connection: &Connection) -> ZorgResult<usize> {
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM effective_tags", [], |row| row.get(0))
        .map_err(|error| operation_failed(format!("failed to count effective tags: {error}")))?;
    Ok(usize::try_from(count).unwrap_or(usize::MAX))
}

fn last_indexed_at_unix_ms(connection: &Connection) -> ZorgResult<Option<i64>> {
    connection
        .query_row("SELECT MAX(indexed_at_unix_ms) FROM files", [], |row| {
            row.get(0)
        })
        .map_err(|error| {
            operation_failed(format!("failed to read last indexed timestamp: {error}"))
        })
}

fn refresh_unchanged_file_metadata(
    connection: &Connection,
    snapshots: &[SourceSnapshot],
) -> ZorgResult<()> {
    for snapshot in snapshots {
        connection
            .execute(
                "UPDATE files
                 SET absolute_path = ?1, mtime_unix_ms = ?2, byte_len = ?3, content_hash = ?4
                 WHERE relative_path = ?5",
                params![
                    path_to_string(snapshot.source.absolute_path()),
                    snapshot.mtime_unix_ms,
                    snapshot.byte_len,
                    snapshot.content_hash.as_str(),
                    path_to_string(snapshot.source.relative_path()),
                ],
            )
            .map_err(|error| {
                operation_failed(format!(
                    "failed to refresh indexed file metadata for {}: {error}",
                    snapshot.source.relative_path().display()
                ))
            })?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct SourceSnapshot {
    source: CorpusSource,
    mtime_unix_ms: Option<i64>,
    byte_len: i64,
    content_hash: String,
    source_text: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct IndexedFileState {
    relative_path: PathBuf,
    content_hash: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum FileChangeKind {
    New,
    Changed,
    Unchanged,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct FileChangeCounts {
    unchanged_files: usize,
    new_files: usize,
    changed_files: usize,
    deleted_files: usize,
}

#[derive(Debug, Clone)]
struct PendingLink {
    source_zettel_id: i64,
    reference: Reference,
    target_canonical_id: Option<String>,
}

#[derive(Debug)]
struct SnapshotState {
    zettel_count: usize,
    diagnostic_count: usize,
    canonical_ids: BTreeMap<String, i64>,
    file_ids_by_path: BTreeMap<PathBuf, i64>,
    diagnostics_seen: BTreeSet<String>,
    pending_links: Vec<PendingLink>,
}

impl SnapshotState {
    fn new() -> Self {
        Self {
            zettel_count: 0,
            diagnostic_count: 0,
            canonical_ids: BTreeMap::new(),
            file_ids_by_path: BTreeMap::new(),
            diagnostics_seen: BTreeSet::new(),
            pending_links: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct GraphZettel {
    id: i64,
    parent_id: Option<i64>,
    kind: String,
    relative_path: PathBuf,
}

#[derive(Debug, Clone)]
struct DirectoryTagSource {
    zettel_id: i64,
    directory_path: PathBuf,
}

fn replace_snapshot(
    transaction: &Transaction<'_>,
    files: &[SourceSnapshot],
    documents: &[ZettelDocument],
    validation_diagnostics: &[Diagnostic],
    resolution_diagnostics: &[Diagnostic],
    indexed_at_unix_ms: i64,
) -> ZorgResult<ReindexSummary> {
    transaction
        .execute("DELETE FROM files", [])
        .map_err(|error| operation_failed(format!("failed to clear indexed files: {error}")))?;
    transaction
        .execute(
            "INSERT INTO index_metadata (key, value) VALUES ('last_full_reindex_unix_ms', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [indexed_at_unix_ms.to_string()],
        )
        .map_err(|error| operation_failed(format!("failed to update index metadata: {error}")))?;
    transaction
        .execute(
            "INSERT INTO index_metadata (key, value) VALUES ('last_reindex_unix_ms', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [indexed_at_unix_ms.to_string()],
        )
        .map_err(|error| operation_failed(format!("failed to update index metadata: {error}")))?;

    let mut state = SnapshotState::new();

    for (file, document) in files.iter().zip(documents) {
        let file_id = insert_file(transaction, file, indexed_at_unix_ms)?;
        state
            .file_ids_by_path
            .insert(file.source.absolute_path().to_path_buf(), file_id);

        let mut source_order = 0;
        insert_zettel_tree(
            transaction,
            &document.root,
            file_id,
            None,
            &mut source_order,
            &mut state,
        )?;

        for diagnostic in &document.diagnostics {
            insert_diagnostic_once(transaction, Some(file_id), None, diagnostic, &mut state)?;
        }
    }

    for diagnostic in validation_diagnostics
        .iter()
        .chain(resolution_diagnostics.iter())
    {
        let file_id = diagnostic
            .path
            .as_ref()
            .and_then(|path| state.file_ids_by_path.get(path.as_path()).copied());
        insert_diagnostic_once(transaction, file_id, None, diagnostic, &mut state)?;
    }

    for link in &state.pending_links {
        let target_zettel_id = link
            .target_canonical_id
            .as_deref()
            .and_then(|target| state.canonical_ids.get(target).copied());
        insert_link(transaction, link, target_zettel_id)?;
    }
    let effective_tag_count = materialize_effective_tags(transaction)?;

    Ok(ReindexSummary {
        discovered_files: files.len(),
        indexed_files: files.len(),
        unchanged_files: 0,
        new_files: files.len(),
        changed_files: 0,
        deleted_files: 0,
        zettel_count: state.zettel_count,
        diagnostic_count: state.diagnostic_count,
        effective_tag_count,
        last_indexed_at_unix_ms: Some(indexed_at_unix_ms),
    })
}

fn apply_incremental_snapshot(
    transaction: &Transaction<'_>,
    files: &[SourceSnapshot],
    documents: &[ZettelDocument],
    indexed_files: &BTreeMap<PathBuf, IndexedFileState>,
    validation_diagnostics: &[Diagnostic],
    resolution_diagnostics: &[Diagnostic],
    indexed_at_unix_ms: i64,
) -> ZorgResult<ReindexSummary> {
    let changes = classify_file_changes(files, indexed_files);
    let discovered_paths = files
        .iter()
        .map(|file| file.source.relative_path().to_path_buf())
        .collect::<BTreeSet<_>>();

    transaction
        .execute("DELETE FROM links", [])
        .map_err(|error| operation_failed(format!("failed to clear indexed links: {error}")))?;
    transaction
        .execute("DELETE FROM diagnostics", [])
        .map_err(|error| {
            operation_failed(format!("failed to clear indexed diagnostics: {error}"))
        })?;

    for indexed in indexed_files.values() {
        if !discovered_paths.contains(&indexed.relative_path) {
            delete_file_by_relative_path(transaction, &indexed.relative_path)?;
        }
    }

    for (file, document) in files.iter().zip(documents) {
        match file_change_kind(file, indexed_files) {
            FileChangeKind::New => {}
            FileChangeKind::Changed => {
                delete_file_by_relative_path(transaction, file.source.relative_path())?
            }
            FileChangeKind::Unchanged => {
                refresh_unchanged_file_metadata_in_transaction(transaction, file)?;
                continue;
            }
        }

        let file_id = insert_file(transaction, file, indexed_at_unix_ms)?;
        let mut source_order = 0;
        let mut state = SnapshotState::new();
        insert_zettel_tree(
            transaction,
            &document.root,
            file_id,
            None,
            &mut source_order,
            &mut state,
        )?;
    }

    transaction
        .execute("DELETE FROM diagnostics", [])
        .map_err(|error| {
            operation_failed(format!("failed to clear transient diagnostics: {error}"))
        })?;

    let file_ids_by_path = query_file_ids_by_path(transaction)?;
    let zettel_ids_by_path_and_key = query_zettel_ids_by_path_and_key(transaction)?;
    let canonical_ids = query_canonical_zettel_ids(transaction)?;
    let mut diagnostics_seen = BTreeSet::new();
    let mut diagnostic_total = 0;

    for document in documents {
        let Some(document_path) = document.path.as_ref().map(SourcePath::as_path) else {
            continue;
        };
        let Some(file_id) = file_ids_by_path.get(document_path).copied() else {
            continue;
        };
        for diagnostic in &document.diagnostics {
            insert_global_diagnostic_once(
                transaction,
                Some(file_id),
                None,
                diagnostic,
                &mut diagnostics_seen,
                &mut diagnostic_total,
            )?;
        }
        insert_zettel_diagnostics(
            transaction,
            document_path,
            &document.root,
            file_id,
            &zettel_ids_by_path_and_key,
            &mut diagnostics_seen,
            &mut diagnostic_total,
        )?;
    }

    for diagnostic in validation_diagnostics
        .iter()
        .chain(resolution_diagnostics.iter())
    {
        let file_id = diagnostic
            .path
            .as_ref()
            .and_then(|path| file_ids_by_path.get(path.as_path()).copied());
        insert_global_diagnostic_once(
            transaction,
            file_id,
            None,
            diagnostic,
            &mut diagnostics_seen,
            &mut diagnostic_total,
        )?;
    }

    for document in documents {
        let Some(document_path) = document.path.as_ref().map(SourcePath::as_path) else {
            continue;
        };
        insert_document_links(
            transaction,
            document_path,
            &document.root,
            &zettel_ids_by_path_and_key,
            &canonical_ids,
        )?;
    }
    let effective_tag_count = materialize_effective_tags(transaction)?;

    transaction
        .execute(
            "INSERT INTO index_metadata (key, value) VALUES ('last_reindex_unix_ms', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [indexed_at_unix_ms.to_string()],
        )
        .map_err(|error| operation_failed(format!("failed to update index metadata: {error}")))?;

    Ok(ReindexSummary {
        discovered_files: files.len(),
        indexed_files: changes.new_files + changes.changed_files,
        unchanged_files: changes.unchanged_files,
        new_files: changes.new_files,
        changed_files: changes.changed_files,
        deleted_files: changes.deleted_files,
        zettel_count: count_indexed_zettel_for_changed_files(files, documents, indexed_files),
        diagnostic_count: diagnostic_total,
        effective_tag_count,
        last_indexed_at_unix_ms: Some(indexed_at_unix_ms),
    })
}

fn delete_file_by_relative_path(
    transaction: &Transaction<'_>,
    relative_path: &Path,
) -> ZorgResult<()> {
    transaction
        .execute(
            "DELETE FROM files WHERE relative_path = ?1",
            [path_to_string(relative_path)],
        )
        .map_err(|error| {
            operation_failed(format!(
                "failed to delete indexed file {}: {error}",
                relative_path.display()
            ))
        })?;
    Ok(())
}

fn refresh_unchanged_file_metadata_in_transaction(
    transaction: &Transaction<'_>,
    file: &SourceSnapshot,
) -> ZorgResult<()> {
    transaction
        .execute(
            "UPDATE files
             SET absolute_path = ?1, mtime_unix_ms = ?2, byte_len = ?3, content_hash = ?4
             WHERE relative_path = ?5",
            params![
                path_to_string(file.source.absolute_path()),
                file.mtime_unix_ms,
                file.byte_len,
                file.content_hash.as_str(),
                path_to_string(file.source.relative_path()),
            ],
        )
        .map_err(|error| {
            operation_failed(format!(
                "failed to refresh indexed file metadata for {}: {error}",
                file.source.relative_path().display()
            ))
        })?;
    Ok(())
}

fn query_file_ids_by_path(transaction: &Transaction<'_>) -> ZorgResult<BTreeMap<PathBuf, i64>> {
    let mut statement = transaction
        .prepare("SELECT absolute_path, id FROM files")
        .map_err(|error| operation_failed(format!("failed to prepare file ID query: {error}")))?;
    let rows = statement
        .query_map([], |row| {
            Ok((PathBuf::from(row.get::<_, String>(0)?), row.get(1)?))
        })
        .map_err(|error| operation_failed(format!("failed to query file IDs: {error}")))?;
    rows.collect::<rusqlite::Result<BTreeMap<_, _>>>()
        .map_err(|error| operation_failed(format!("failed to read file IDs: {error}")))
}

fn query_zettel_ids_by_path_and_key(
    transaction: &Transaction<'_>,
) -> ZorgResult<BTreeMap<(PathBuf, String), i64>> {
    let mut statement = transaction
        .prepare(
            "SELECT f.absolute_path, z.parser_key, z.id
             FROM zettel z
             JOIN files f ON f.id = z.file_id",
        )
        .map_err(|error| operation_failed(format!("failed to prepare zettel ID query: {error}")))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                (
                    PathBuf::from(row.get::<_, String>(0)?),
                    row.get::<_, String>(1)?,
                ),
                row.get(2)?,
            ))
        })
        .map_err(|error| operation_failed(format!("failed to query zettel IDs: {error}")))?;
    rows.collect::<rusqlite::Result<BTreeMap<_, _>>>()
        .map_err(|error| operation_failed(format!("failed to read zettel IDs: {error}")))
}

fn query_canonical_zettel_ids(transaction: &Transaction<'_>) -> ZorgResult<BTreeMap<String, i64>> {
    let mut statement = transaction
        .prepare("SELECT canonical_id, zettel_id FROM zettel_ids")
        .map_err(|error| {
            operation_failed(format!("failed to prepare canonical ID query: {error}"))
        })?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))
        .map_err(|error| operation_failed(format!("failed to query canonical IDs: {error}")))?;
    rows.collect::<rusqlite::Result<BTreeMap<_, _>>>()
        .map_err(|error| operation_failed(format!("failed to read canonical IDs: {error}")))
}

fn insert_global_diagnostic_once(
    transaction: &Transaction<'_>,
    file_id: Option<i64>,
    zettel_id: Option<i64>,
    diagnostic: &Diagnostic,
    diagnostics_seen: &mut BTreeSet<String>,
    diagnostic_total: &mut usize,
) -> ZorgResult<()> {
    let key = diagnostic_key(diagnostic);
    if !diagnostics_seen.insert(key) {
        return Ok(());
    }

    let span = diagnostic.span;
    transaction
        .execute(
            "INSERT INTO diagnostics (
                file_id, zettel_id, severity, category, code, message, start_byte, end_byte,
                start_line, start_column, end_line, end_column
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                file_id,
                zettel_id,
                severity(diagnostic.severity),
                diagnostic_category(&diagnostic.category),
                diagnostic.code.as_deref(),
                diagnostic.message.as_str(),
                span.map(|span| usize_to_i64(span.start_byte)),
                span.map(|span| usize_to_i64(span.end_byte)),
                span.and_then(|span| optional_usize_to_i64(span.start_line)),
                span.and_then(|span| optional_usize_to_i64(span.start_column)),
                span.and_then(|span| optional_usize_to_i64(span.end_line)),
                span.and_then(|span| optional_usize_to_i64(span.end_column)),
            ],
        )
        .map_err(|error| operation_failed(format!("failed to insert diagnostic: {error}")))?;
    *diagnostic_total += 1;
    Ok(())
}

fn insert_zettel_diagnostics(
    transaction: &Transaction<'_>,
    path: &Path,
    zettel: &Zettel,
    file_id: i64,
    zettel_ids_by_path_and_key: &BTreeMap<(PathBuf, String), i64>,
    diagnostics_seen: &mut BTreeSet<String>,
    diagnostic_total: &mut usize,
) -> ZorgResult<()> {
    let zettel_id = zettel_ids_by_path_and_key
        .get(&(path.to_path_buf(), zettel.key.as_str().to_owned()))
        .copied();
    for diagnostic in &zettel.diagnostics {
        insert_global_diagnostic_once(
            transaction,
            Some(file_id),
            zettel_id,
            diagnostic,
            diagnostics_seen,
            diagnostic_total,
        )?;
    }
    for child in child_zettels(zettel) {
        insert_zettel_diagnostics(
            transaction,
            path,
            child,
            file_id,
            zettel_ids_by_path_and_key,
            diagnostics_seen,
            diagnostic_total,
        )?;
    }
    Ok(())
}

fn insert_document_links(
    transaction: &Transaction<'_>,
    path: &Path,
    zettel: &Zettel,
    zettel_ids_by_path_and_key: &BTreeMap<(PathBuf, String), i64>,
    canonical_ids: &BTreeMap<String, i64>,
) -> ZorgResult<()> {
    let Some(source_zettel_id) = zettel_ids_by_path_and_key
        .get(&(path.to_path_buf(), zettel.key.as_str().to_owned()))
        .copied()
    else {
        return Ok(());
    };

    for reference in &zettel.links {
        let target_canonical_id = resolved_target_for(reference, &zettel.resolved_links);
        let target_zettel_id = target_canonical_id
            .as_deref()
            .and_then(|target| canonical_ids.get(target).copied());
        let pending = PendingLink {
            source_zettel_id,
            reference: reference.clone(),
            target_canonical_id,
        };
        insert_link(transaction, &pending, target_zettel_id)?;
    }

    for child in child_zettels(zettel) {
        insert_document_links(
            transaction,
            path,
            child,
            zettel_ids_by_path_and_key,
            canonical_ids,
        )?;
    }

    Ok(())
}

fn count_indexed_zettel_for_changed_files(
    files: &[SourceSnapshot],
    documents: &[ZettelDocument],
    indexed_files: &BTreeMap<PathBuf, IndexedFileState>,
) -> usize {
    files
        .iter()
        .zip(documents)
        .filter(|(file, _)| file_change_kind(file, indexed_files) != FileChangeKind::Unchanged)
        .map(|(_, document)| count_zettel_tree(&document.root))
        .sum()
}

fn count_zettel_tree(zettel: &Zettel) -> usize {
    1 + child_zettels(zettel).map(count_zettel_tree).sum::<usize>()
}

fn insert_file(
    transaction: &Transaction<'_>,
    file: &SourceSnapshot,
    indexed_at_unix_ms: i64,
) -> ZorgResult<i64> {
    transaction
        .execute(
            "INSERT INTO files (
                absolute_path, relative_path, mtime_unix_ms, byte_len, content_hash,
                indexed_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                path_to_string(file.source.absolute_path()),
                path_to_string(file.source.relative_path()),
                file.mtime_unix_ms,
                file.byte_len,
                file.content_hash.as_str(),
                indexed_at_unix_ms,
            ],
        )
        .map_err(|error| {
            operation_failed(format!(
                "failed to insert indexed file {}: {error}",
                file.source.relative_path().display()
            ))
        })?;
    Ok(transaction.last_insert_rowid())
}

fn insert_zettel_tree(
    transaction: &Transaction<'_>,
    zettel: &Zettel,
    file_id: i64,
    parent_id: Option<i64>,
    source_order: &mut i64,
    state: &mut SnapshotState,
) -> ZorgResult<i64> {
    let zettel_id = insert_zettel(transaction, zettel, file_id, parent_id, *source_order)?;
    *source_order += 1;
    state.zettel_count += 1;

    if let Some(canonical_id) = &zettel.canonical_id {
        let canonical_id = canonical_id.as_str().to_owned();
        state
            .canonical_ids
            .entry(canonical_id.clone())
            .or_insert(zettel_id);
        transaction
            .execute(
                "INSERT OR IGNORE INTO zettel_ids (canonical_id, zettel_id) VALUES (?1, ?2)",
                params![canonical_id, zettel_id],
            )
            .map_err(|error| operation_failed(format!("failed to insert zettel ID: {error}")))?;
    }

    insert_zettel_semantics(transaction, zettel_id, zettel)?;
    for diagnostic in &zettel.diagnostics {
        insert_diagnostic_once(
            transaction,
            Some(file_id),
            Some(zettel_id),
            diagnostic,
            state,
        )?;
    }
    for reference in &zettel.links {
        let target_canonical_id = resolved_target_for(reference, &zettel.resolved_links);
        state.pending_links.push(PendingLink {
            source_zettel_id: zettel_id,
            reference: reference.clone(),
            target_canonical_id,
        });
    }

    for child in child_zettels(zettel) {
        insert_zettel_tree(
            transaction,
            child,
            file_id,
            Some(zettel_id),
            source_order,
            state,
        )?;
    }

    Ok(zettel_id)
}

fn insert_zettel(
    transaction: &Transaction<'_>,
    zettel: &Zettel,
    file_id: i64,
    parent_id: Option<i64>,
    source_order: i64,
) -> ZorgResult<i64> {
    let span = zettel.span.unwrap_or(SourceSpan::bytes(0, 0));
    let title = zettel.plain_title();
    let canonical_id = zettel
        .canonical_id
        .as_ref()
        .map(|canonical_id| canonical_id.as_str().to_owned());
    let local_id = zettel
        .local_id
        .as_ref()
        .map(|local_id| local_id.as_str().to_owned());
    let body_text = zettel_body_text(zettel);

    transaction
        .execute(
            "INSERT INTO zettel (
                file_id, parent_id, source_order, kind, parser_key, title, canonical_id, local_id,
                body_text, start_byte, end_byte, start_line, start_column, end_line, end_column
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                file_id,
                parent_id,
                source_order,
                zettel_kind(zettel.kind),
                zettel.key.as_str(),
                title,
                canonical_id,
                local_id,
                body_text,
                usize_to_i64(span.start_byte),
                usize_to_i64(span.end_byte),
                optional_usize_to_i64(span.start_line),
                optional_usize_to_i64(span.start_column),
                optional_usize_to_i64(span.end_line),
                optional_usize_to_i64(span.end_column),
            ],
        )
        .map_err(|error| operation_failed(format!("failed to insert zettel row: {error}")))?;
    Ok(transaction.last_insert_rowid())
}

fn insert_zettel_semantics(
    transaction: &Transaction<'_>,
    zettel_id: i64,
    zettel: &Zettel,
) -> ZorgResult<()> {
    for tagged in &zettel.tags {
        insert_tag(
            transaction,
            zettel_id,
            tagged.tag.as_str(),
            "explicit",
            tagged.span,
        )?;
    }
    for tagged in &zettel.type_tags {
        insert_tag(
            transaction,
            zettel_id,
            tagged.tag.as_str(),
            "type",
            tagged.span,
        )?;
    }
    for property in &zettel.properties {
        let span = property.span;
        transaction
            .execute(
                "INSERT INTO properties (zettel_id, key, value, start_byte, end_byte)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    zettel_id,
                    property.key.as_str(),
                    property.value.as_str(),
                    span.map(|span| usize_to_i64(span.start_byte)),
                    span.map(|span| usize_to_i64(span.end_byte)),
                ],
            )
            .map_err(|error| operation_failed(format!("failed to insert property: {error}")))?;
    }
    if let Some(todo) = zettel.todo {
        let span = zettel.span;
        transaction
            .execute(
                "INSERT INTO todos (zettel_id, marker, start_byte, end_byte)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    zettel_id,
                    todo_marker(todo),
                    span.map(|span| usize_to_i64(span.start_byte)),
                    span.map(|span| usize_to_i64(span.end_byte)),
                ],
            )
            .map_err(|error| operation_failed(format!("failed to insert todo: {error}")))?;
    }

    let title_text = zettel.plain_title();
    let body_text = zettel_body_text(zettel);
    let raw_text = zettel_raw_text(title_text.as_deref(), &body_text);
    transaction
        .execute(
            "INSERT INTO text_index (zettel_id, title_text, body_text, raw_text)
             VALUES (?1, ?2, ?3, ?4)",
            params![zettel_id, title_text, body_text, raw_text],
        )
        .map_err(|error| operation_failed(format!("failed to insert text index: {error}")))?;

    Ok(())
}

fn insert_tag(
    transaction: &Transaction<'_>,
    zettel_id: i64,
    tag: &str,
    tag_kind: &str,
    span: Option<SourceSpan>,
) -> ZorgResult<()> {
    transaction
        .execute(
            "INSERT OR IGNORE INTO tags (zettel_id, tag, tag_kind, start_byte, end_byte)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                zettel_id,
                tag,
                tag_kind,
                span.map(|span| usize_to_i64(span.start_byte)),
                span.map(|span| usize_to_i64(span.end_byte)),
            ],
        )
        .map_err(|error| operation_failed(format!("failed to insert tag: {error}")))?;
    Ok(())
}

fn insert_link(
    transaction: &Transaction<'_>,
    link: &PendingLink,
    target_zettel_id: Option<i64>,
) -> ZorgResult<()> {
    let span = link.reference.span.unwrap_or(SourceSpan::bytes(0, 0));
    transaction
        .execute(
            "INSERT INTO links (
                source_zettel_id, target_zettel_id, target_canonical_id, target_text, link_kind,
                resolved, start_byte, end_byte
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                link.source_zettel_id,
                target_zettel_id,
                link.target_canonical_id.as_deref(),
                link.reference.raw.as_str(),
                reference_kind(&link.reference.target),
                link.target_canonical_id.is_some() as i64,
                usize_to_i64(span.start_byte),
                usize_to_i64(span.end_byte),
            ],
        )
        .map_err(|error| operation_failed(format!("failed to insert link: {error}")))?;
    Ok(())
}

fn materialize_effective_tags(transaction: &Transaction<'_>) -> ZorgResult<usize> {
    transaction
        .execute("DELETE FROM effective_tags", [])
        .map_err(|error| operation_failed(format!("failed to clear effective tags: {error}")))?;

    let zettels = query_graph_zettels(transaction)?;
    let parent_by_zettel = zettels
        .iter()
        .map(|zettel| (zettel.id, zettel.parent_id))
        .collect::<BTreeMap<_, _>>();
    let explicit_tags_by_zettel = query_explicit_tags_by_zettel(transaction)?;
    let directory_sources = zettels
        .iter()
        .filter(|zettel| {
            zettel.kind == "directory"
                && zettel
                    .relative_path
                    .file_name()
                    .is_some_and(|file_name| file_name == "init.z")
        })
        .map(|zettel| DirectoryTagSource {
            zettel_id: zettel.id,
            directory_path: zettel
                .relative_path
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .to_path_buf(),
        })
        .collect::<Vec<_>>();

    let mut inserted = 0;
    for zettel in &zettels {
        if let Some(tags) = explicit_tags_by_zettel.get(&zettel.id) {
            for tag in tags {
                inserted +=
                    insert_effective_tag(transaction, zettel.id, tag, Some(zettel.id), "explicit")?;
            }
        }

        let ancestors = ancestor_ids(&parent_by_zettel, zettel.id);
        let ancestor_set = ancestors.iter().copied().collect::<BTreeSet<_>>();
        for ancestor_id in &ancestors {
            if let Some(tags) = explicit_tags_by_zettel.get(ancestor_id) {
                for tag in tags {
                    inserted += insert_effective_tag(
                        transaction,
                        zettel.id,
                        tag,
                        Some(*ancestor_id),
                        "parent",
                    )?;
                }
            }
        }

        let zettel_directory = zettel
            .relative_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        for directory in &directory_sources {
            if directory.zettel_id == zettel.id || ancestor_set.contains(&directory.zettel_id) {
                continue;
            }
            if !path_is_ancestor_or_same(&directory.directory_path, &zettel_directory) {
                continue;
            }
            if let Some(tags) = explicit_tags_by_zettel.get(&directory.zettel_id) {
                for tag in tags {
                    inserted += insert_effective_tag(
                        transaction,
                        zettel.id,
                        tag,
                        Some(directory.zettel_id),
                        "path",
                    )?;
                }
            }
        }
    }

    Ok(inserted)
}

fn query_graph_zettels(transaction: &Transaction<'_>) -> ZorgResult<Vec<GraphZettel>> {
    let mut statement = transaction
        .prepare(
            "SELECT z.id, z.parent_id, z.kind, f.relative_path
             FROM zettel z
             JOIN files f ON f.id = z.file_id
             ORDER BY f.relative_path, z.source_order",
        )
        .map_err(|error| {
            operation_failed(format!("failed to prepare graph zettel query: {error}"))
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok(GraphZettel {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                kind: row.get(2)?,
                relative_path: PathBuf::from(row.get::<_, String>(3)?),
            })
        })
        .map_err(|error| operation_failed(format!("failed to query graph zettels: {error}")))?;

    collect_rows(rows, "failed to read graph zettel")
}

fn query_explicit_tags_by_zettel(
    transaction: &Transaction<'_>,
) -> ZorgResult<BTreeMap<i64, BTreeSet<String>>> {
    let mut statement = transaction
        .prepare(
            "SELECT zettel_id, tag
             FROM tags
             WHERE tag_kind = 'explicit'
             ORDER BY zettel_id, tag",
        )
        .map_err(|error| {
            operation_failed(format!("failed to prepare explicit tag query: {error}"))
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| operation_failed(format!("failed to query explicit tags: {error}")))?;

    let mut tags_by_zettel: BTreeMap<i64, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        let (zettel_id, tag) =
            row.map_err(|error| operation_failed(format!("failed to read explicit tag: {error}")))?;
        tags_by_zettel.entry(zettel_id).or_default().insert(tag);
    }
    Ok(tags_by_zettel)
}

fn ancestor_ids(parent_by_zettel: &BTreeMap<i64, Option<i64>>, zettel_id: i64) -> Vec<i64> {
    let mut ancestors = Vec::new();
    let mut current = parent_by_zettel.get(&zettel_id).copied().flatten();
    while let Some(ancestor_id) = current {
        ancestors.push(ancestor_id);
        current = parent_by_zettel.get(&ancestor_id).copied().flatten();
    }
    ancestors
}

fn path_is_ancestor_or_same(candidate_ancestor: &Path, path: &Path) -> bool {
    candidate_ancestor.as_os_str().is_empty()
        || path == candidate_ancestor
        || path.starts_with(candidate_ancestor)
}

fn insert_effective_tag(
    transaction: &Transaction<'_>,
    zettel_id: i64,
    tag: &str,
    source_zettel_id: Option<i64>,
    source: &str,
) -> ZorgResult<usize> {
    transaction
        .execute(
            "INSERT OR IGNORE INTO effective_tags (zettel_id, tag, source_zettel_id, source)
             VALUES (?1, ?2, ?3, ?4)",
            params![zettel_id, tag, source_zettel_id, source],
        )
        .map_err(|error| operation_failed(format!("failed to insert effective tag: {error}")))
}

fn insert_diagnostic_once(
    transaction: &Transaction<'_>,
    file_id: Option<i64>,
    zettel_id: Option<i64>,
    diagnostic: &Diagnostic,
    state: &mut SnapshotState,
) -> ZorgResult<()> {
    let key = diagnostic_key(diagnostic);
    if !state.diagnostics_seen.insert(key) {
        return Ok(());
    }

    let span = diagnostic.span;
    transaction
        .execute(
            "INSERT INTO diagnostics (
                file_id, zettel_id, severity, category, code, message, start_byte, end_byte,
                start_line, start_column, end_line, end_column
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                file_id,
                zettel_id,
                severity(diagnostic.severity),
                diagnostic_category(&diagnostic.category),
                diagnostic.code.as_deref(),
                diagnostic.message.as_str(),
                span.map(|span| usize_to_i64(span.start_byte)),
                span.map(|span| usize_to_i64(span.end_byte)),
                span.and_then(|span| optional_usize_to_i64(span.start_line)),
                span.and_then(|span| optional_usize_to_i64(span.start_column)),
                span.and_then(|span| optional_usize_to_i64(span.end_line)),
                span.and_then(|span| optional_usize_to_i64(span.end_column)),
            ],
        )
        .map_err(|error| operation_failed(format!("failed to insert diagnostic: {error}")))?;
    state.diagnostic_count += 1;
    Ok(())
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
    context: &str,
) -> ZorgResult<Vec<T>> {
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| operation_failed(format!("{context}: {error}")))
}

fn query_zettel<P>(connection: &Connection, sql: &str, params: P) -> ZorgResult<Vec<StoredZettel>>
where
    P: rusqlite::Params,
{
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| operation_failed(format!("failed to prepare zettel query: {error}")))?;
    let rows = statement
        .query_map(params, stored_zettel_from_row)
        .map_err(|error| operation_failed(format!("failed to list zettel: {error}")))?;

    collect_rows(rows, "failed to read indexed zettel")
}

fn query_links<P>(connection: &Connection, sql: &str, params: P) -> ZorgResult<Vec<StoredLink>>
where
    P: rusqlite::Params,
{
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| operation_failed(format!("failed to prepare link query: {error}")))?;
    let rows = statement
        .query_map(params, |row| {
            Ok(StoredLink {
                id: row.get(0)?,
                source_zettel_id: row.get(1)?,
                target_zettel_id: row.get(2)?,
                target_canonical_id: row.get(3)?,
                target_text: row.get(4)?,
                link_kind: row.get(5)?,
                resolved: row.get::<_, i64>(6)? != 0,
                start_byte: row.get(7)?,
                end_byte: row.get(8)?,
            })
        })
        .map_err(|error| operation_failed(format!("failed to list links: {error}")))?;

    collect_rows(rows, "failed to read indexed link")
}

fn query_tags<P>(connection: &Connection, sql: &str, params: P) -> ZorgResult<Vec<StoredTag>>
where
    P: rusqlite::Params,
{
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| operation_failed(format!("failed to prepare tag query: {error}")))?;
    let rows = statement
        .query_map(params, |row| {
            Ok(StoredTag {
                id: row.get(0)?,
                zettel_id: row.get(1)?,
                tag: row.get(2)?,
                tag_kind: row.get(3)?,
            })
        })
        .map_err(|error| operation_failed(format!("failed to list tags: {error}")))?;

    collect_rows(rows, "failed to read indexed tag")
}

fn query_effective_tags<P>(
    connection: &Connection,
    sql: &str,
    params: P,
) -> ZorgResult<Vec<StoredEffectiveTag>>
where
    P: rusqlite::Params,
{
    let mut statement = connection.prepare(sql).map_err(|error| {
        operation_failed(format!("failed to prepare effective tag query: {error}"))
    })?;
    let rows = statement
        .query_map(params, |row| {
            Ok(StoredEffectiveTag {
                id: row.get(0)?,
                zettel_id: row.get(1)?,
                tag: row.get(2)?,
                source_zettel_id: row.get(3)?,
                source: row.get(4)?,
            })
        })
        .map_err(|error| operation_failed(format!("failed to list effective tags: {error}")))?;

    collect_rows(rows, "failed to read indexed effective tag")
}

fn stored_zettel_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredZettel> {
    Ok(StoredZettel {
        id: row.get(0)?,
        file_id: row.get(1)?,
        parent_id: row.get(2)?,
        source_order: row.get(3)?,
        kind: row.get(4)?,
        parser_key: row.get(5)?,
        title: row.get(6)?,
        canonical_id: row.get(7)?,
        local_id: row.get(8)?,
        body_text: row.get(9)?,
        start_byte: row.get(10)?,
        end_byte: row.get(11)?,
    })
}

fn child_zettels(zettel: &Zettel) -> impl Iterator<Item = &Zettel> {
    zettel.body.iter().filter_map(|block| match block {
        BodyBlock::ChildZettel(child) => Some(child.as_ref()),
        _ => None,
    })
}

fn zettel_body_text(zettel: &Zettel) -> String {
    let mut parts = Vec::new();
    for block in &zettel.body {
        match block {
            BodyBlock::Paragraph(paragraph) => parts.push(paragraph.text.clone()),
            BodyBlock::FencedCode(fenced) => parts.push(fenced.body.clone()),
            BodyBlock::ChildZettel(_) => {}
        }
    }
    parts.join("\n\n")
}

fn zettel_raw_text(title: Option<&str>, body_text: &str) -> String {
    match (title, body_text.is_empty()) {
        (Some(title), false) => format!("{title}\n\n{body_text}"),
        (Some(title), true) => title.to_owned(),
        (None, false) => body_text.to_owned(),
        (None, true) => String::new(),
    }
}

fn resolved_target_for(
    reference: &Reference,
    resolved_links: &[ResolvedReference],
) -> Option<String> {
    resolved_links
        .iter()
        .find(|resolved| references_match(&resolved.reference, reference))
        .map(|resolved| resolved.target_id.as_str().to_owned())
}

fn references_match(left: &Reference, right: &Reference) -> bool {
    left.raw == right.raw
        && left.span.map(|span| (span.start_byte, span.end_byte))
            == right.span.map(|span| (span.start_byte, span.end_byte))
}

fn diagnostic_key(diagnostic: &Diagnostic) -> String {
    let path = diagnostic
        .path
        .as_ref()
        .map(|path| path.as_path().display().to_string())
        .unwrap_or_default();
    let span = diagnostic
        .span
        .map(|span| {
            format!(
                "{}:{}:{}:{}",
                span.start_byte,
                span.end_byte,
                span.start_line.unwrap_or(0),
                span.end_line.unwrap_or(0)
            )
        })
        .unwrap_or_default();
    format!(
        "{path}|{}|{}|{}|{}|{span}",
        severity(diagnostic.severity),
        diagnostic_category(&diagnostic.category),
        diagnostic.code.as_deref().unwrap_or_default(),
        diagnostic.message
    )
}

fn content_hash(bytes: &[u8]) -> String {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    format!("{hash:016x}")
}

fn modified_unix_ms(metadata: &std::fs::Metadata) -> Option<i64> {
    metadata.modified().ok().and_then(system_time_unix_ms)
}

fn now_unix_ms() -> i64 {
    system_time_unix_ms(SystemTime::now()).unwrap_or(0)
}

fn system_time_unix_ms(time: SystemTime) -> Option<i64> {
    let duration = time.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn usize_to_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn optional_usize_to_i64(value: Option<usize>) -> Option<i64> {
    value.map(usize_to_i64)
}

fn zettel_kind(kind: ZettelKind) -> &'static str {
    match kind {
        ZettelKind::File => "file",
        ZettelKind::Directory => "directory",
        ZettelKind::Nested => "nested",
    }
}

fn todo_marker(marker: TodoMarker) -> &'static str {
    match marker {
        TodoMarker::Open => "[ ]",
        TodoMarker::Next => "[N]",
        TodoMarker::Done => "[X]",
        TodoMarker::Unknown => "[?]",
    }
}

fn reference_kind(target: &ReferenceTarget) -> &'static str {
    match target {
        ReferenceTarget::Absolute(_) => "absolute",
        ReferenceTarget::Child(_) => "child",
        ReferenceTarget::Sibling(_) => "sibling",
        ReferenceTarget::LocalDeclaration(_) => "local",
    }
}

fn severity(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

fn diagnostic_category(category: &DiagnosticCategory) -> &'static str {
    match category {
        DiagnosticCategory::Syntax => "syntax",
        DiagnosticCategory::Semantic => "semantic",
        DiagnosticCategory::Legacy => "legacy",
        DiagnosticCategory::Unsupported => "unsupported",
    }
}

fn run_migrations(connection: &mut Connection) -> ZorgResult<()> {
    connection
        .execute_batch(
            r#"
CREATE TABLE IF NOT EXISTS schema_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#,
        )
        .map_err(|error| {
            operation_failed(format!("failed to initialize schema metadata: {error}"))
        })?;

    let current_version = schema_version(connection)?;
    if current_version > SCHEMA_VERSION {
        return Err(operation_failed(format!(
            "database schema version {current_version} is newer than supported version {SCHEMA_VERSION}"
        )));
    }
    let sqlite_user_version = sqlite_user_version(connection)?;
    if sqlite_user_version > SCHEMA_VERSION {
        return Err(operation_failed(format!(
            "SQLite user_version {sqlite_user_version} is newer than supported schema version {SCHEMA_VERSION}"
        )));
    }

    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version > current_version)
    {
        let transaction = connection.transaction().map_err(|error| {
            operation_failed(format!(
                "failed to begin schema migration {}: {error}",
                migration.version
            ))
        })?;
        transaction.execute_batch(migration.sql).map_err(|error| {
            operation_failed(format!(
                "failed to apply schema migration {}: {error}",
                migration.version
            ))
        })?;
        transaction
            .execute(
                "INSERT INTO schema_metadata (key, value) VALUES ('schema_version', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [migration.version.to_string()],
            )
            .map_err(|error| {
                operation_failed(format!(
                    "failed to record schema migration {}: {error}",
                    migration.version
                ))
            })?;
        transaction
            .pragma_update(None, "user_version", migration.version)
            .map_err(|error| {
                operation_failed(format!(
                    "failed to record SQLite user_version {}: {error}",
                    migration.version
                ))
            })?;
        transaction.commit().map_err(|error| {
            operation_failed(format!(
                "failed to commit schema migration {}: {error}",
                migration.version
            ))
        })?;
    }

    Ok(())
}

fn schema_version(connection: &Connection) -> ZorgResult<i64> {
    let stored = connection
        .query_row(
            "SELECT value FROM schema_metadata WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| operation_failed(format!("failed to read schema version: {error}")))?;

    stored.map_or(Ok(0), |value| {
        value.parse::<i64>().map_err(|error| {
            operation_failed(format!("stored schema version is not an integer: {error}"))
        })
    })
}

fn sqlite_user_version(connection: &Connection) -> ZorgResult<i64> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(|error| operation_failed(format!("failed to read SQLite user_version: {error}")))
}

fn validate_non_empty_path(path: &Path, label: &str) -> ZorgResult<()> {
    if path.as_os_str().is_empty() {
        return Err(operation_failed(format!("{label} must not be empty")));
    }

    Ok(())
}

fn operation_failed(message: impl Into<String>) -> ZorgError {
    ZorgError::OperationFailed {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);
    const SCHEMA_TABLES: &[&str] = &[
        "schema_metadata",
        "files",
        "zettel",
        "zettel_ids",
        "links",
        "tags",
        "effective_tags",
        "properties",
        "todos",
        "text_index",
        "diagnostics",
        "index_metadata",
    ];
    const SCHEMA_INDEXES: &[&str] = &[
        "idx_files_relative_path",
        "idx_zettel_file_order",
        "idx_zettel_canonical_id",
        "idx_links_source",
        "idx_links_target",
        "idx_tags_tag",
        "idx_effective_tags_tag",
        "idx_properties_key",
        "idx_diagnostics_file",
    ];

    #[derive(Debug)]
    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                env::temp_dir().join(format!("zorg-store-test-{}-{counter}", std::process::id()));
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
    fn open_creates_database_with_expected_schema_version() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        let db = temp.path().join("db").join("zorg.sqlite3");
        let options = StoreOptions::new(&root, &db).expect("store options");

        let store = Store::open_with_options(options).expect("open store");

        assert_eq!(store.root(), root.as_path());
        assert_eq!(store.database_path(), db.as_path());
        assert_eq!(
            store.schema_version().expect("schema version"),
            SCHEMA_VERSION
        );
        assert!(db.exists());

        let connection = Connection::open(db).expect("open created db");
        assert_schema_objects(&connection);
    }

    #[test]
    fn reopening_existing_database_is_idempotent() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        let db = temp.path().join("zorg.sqlite3");
        let options = StoreOptions::new(&root, &db).expect("store options");

        let first = Store::open_with_options(options.clone()).expect("first open");
        let second = Store::open_with_options(options).expect("second open");

        assert_eq!(
            first.schema_version().expect("first version"),
            SCHEMA_VERSION
        );
        assert_eq!(
            second.schema_version().expect("second version"),
            SCHEMA_VERSION
        );
    }

    #[test]
    fn opening_schema_v1_fixture_preserves_indexed_rows() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        let db = temp.path().join("fixtures").join("schema-v1.sqlite3");
        create_schema_v1_fixture(&db);
        let options = StoreOptions::new(&root, &db).expect("store options");

        let store = Store::open_with_options(options).expect("open v1 fixture");

        assert_eq!(
            store.schema_version().expect("schema version"),
            SCHEMA_VERSION
        );
        let connection = Connection::open(&db).expect("open migrated fixture");
        assert_schema_objects(&connection);
        assert_eq!(
            sqlite_user_version(&connection).expect("sqlite user version"),
            SCHEMA_VERSION
        );

        let files = store.list_files().expect("files");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, PathBuf::from("fixture.z"));
        assert_eq!(files[0].content_hash, "fixture-hash");

        let zettel = store.list_zettel().expect("zettel");
        assert_eq!(zettel.len(), 1);
        assert_eq!(zettel[0].canonical_id.as_deref(), Some("fixture"));
        assert_eq!(zettel[0].body_text, "Persisted fixture body.");
        assert_eq!(
            store
                .lookup_zettel_by_canonical_id("fixture")
                .expect("lookup")
                .expect("fixture zettel")
                .id,
            zettel[0].id
        );
        assert_eq!(store.list_tags().expect("tags")[0].tag, "area/test");
        assert_eq!(
            store.list_effective_tags().expect("effective tags")[0].source,
            "explicit"
        );
        assert_eq!(
            store.list_properties().expect("properties")[0].value,
            "2026-05-03"
        );
        assert_eq!(store.list_todos().expect("todos")[0].marker, "[N]");
        assert_eq!(
            store.list_links().expect("links")[0]
                .target_canonical_id
                .as_deref(),
            Some("fixture")
        );
        assert_eq!(
            store.list_diagnostics().expect("diagnostics")[0]
                .code
                .as_deref(),
            Some("fixture.notice")
        );
    }

    #[test]
    fn opening_database_with_non_integer_schema_metadata_fails() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        let db = temp.path().join("corrupt.sqlite3");
        let connection = Connection::open(&db).expect("open corrupt fixture");
        connection
            .execute_batch(
                r#"
CREATE TABLE schema_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT INTO schema_metadata (key, value) VALUES ('schema_version', 'not-an-integer');
"#,
            )
            .expect("write corrupt metadata");
        let options = StoreOptions::new(&root, &db).expect("store options");

        let error =
            Store::open_with_options(options).expect_err("non-integer metadata should fail");

        assert!(
            error
                .to_string()
                .contains("stored schema version is not an integer")
        );
    }

    #[test]
    fn opening_database_with_future_schema_metadata_fails() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        let db = temp.path().join("future-metadata.sqlite3");
        let connection = Connection::open(&db).expect("open future fixture");
        connection
            .execute_batch(
                r#"
CREATE TABLE schema_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT INTO schema_metadata (key, value) VALUES ('schema_version', '999');
"#,
            )
            .expect("write future metadata");
        let options = StoreOptions::new(&root, &db).expect("store options");

        let error = Store::open_with_options(options).expect_err("future metadata should fail");

        assert!(
            error
                .to_string()
                .contains("database schema version 999 is newer")
        );
    }

    #[test]
    fn opening_database_with_future_sqlite_user_version_fails() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        let db = temp.path().join("future-user-version.sqlite3");
        let connection = Connection::open(&db).expect("open future fixture");
        connection
            .pragma_update(None, "user_version", 999)
            .expect("write future user_version");
        let options = StoreOptions::new(&root, &db).expect("store options");

        let error = Store::open_with_options(options).expect_err("future user_version should fail");

        assert!(
            error
                .to_string()
                .contains("SQLite user_version 999 is newer")
        );
    }

    #[test]
    fn open_uses_default_database_path_under_root() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");

        let store = Store::open(&root).expect("open store");

        assert_eq!(store.root(), root.as_path());
        assert_eq!(
            store.database_path(),
            root.join(DEFAULT_DB_DIR).join(DEFAULT_DB_FILE).as_path()
        );
        assert!(store.database_path().exists());
    }

    #[test]
    fn default_paths_use_home_zorg_root() {
        let default_root = StoreOptions::default_root().expect("default root");
        let home = env::var_os("HOME").expect("HOME should be set in tests");

        assert_eq!(default_root, PathBuf::from(home).join(DEFAULT_ROOT_DIR));
    }

    #[test]
    fn config_resolution_uses_defaults_without_real_home() {
        let temp = TempWorkspace::new();
        let home = temp.path().join("home");
        let config = ResolvedConfig::resolve(ConfigResolutionInputs {
            home_dir: Some(home.clone()),
            ..ConfigResolutionInputs::default()
        })
        .expect("resolved config");

        assert_eq!(config.store_options().corpus_root(), home.join("zorg"));
        assert_eq!(
            config.store_options().database_path(),
            home.join("zorg").join(".zorg").join("zorg.sqlite3")
        );
        assert_eq!(config.watcher_debounce_ms(), DEFAULT_WATCHER_DEBOUNCE_MS);
        assert!(config.watcher_log_path().is_none());
        assert!(config.named_roots().is_empty());
    }

    #[test]
    fn config_resolution_applies_precedence_and_expands_home() {
        let temp = TempWorkspace::new();
        let home = temp.path().join("home");
        let xdg = temp.path().join("xdg");
        let user_config = xdg.join("zorg").join("config.toml");
        std::fs::create_dir_all(user_config.parent().expect("user config parent"))
            .expect("create user config parent");
        std::fs::write(
            &user_config,
            "\
root = \"~/user-root\"
database_path = \"~/user.sqlite3\"
watcher_debounce_ms = 100
watcher_log_path = \"~/user-watch.log\"

[named_roots]
work = \"~/work-zorg\"
",
        )
        .expect("write user config");

        let root = home.join("root-local");
        std::fs::create_dir_all(root.join(".zorg")).expect("create root config dir");
        std::fs::write(
            root.join(".zorg/config.toml"),
            "\
root = \"~/root-config-root\"
database_path = \"~/root.sqlite3\"
watcher_debounce_ms = 200
watcher_log_path = \"~/root-watch.log\"

[named_roots]
team = \"~/team-zorg\"
",
        )
        .expect("write root config");

        let cli_db = temp.path().join("cli.sqlite3");
        let config = ResolvedConfig::resolve(ConfigResolutionInputs {
            cli: ConfigOverrides {
                root: None,
                database_path: Some(cli_db.clone()),
            },
            env_root: Some(root.clone()),
            env_database_path: None,
            env_watcher_debounce_ms: Some("300".to_owned()),
            env_watcher_log_path: Some(PathBuf::from("~/env-watch.log")),
            home_dir: Some(home.clone()),
            xdg_config_home: Some(xdg),
            user_config_path: None,
        })
        .expect("resolved config");

        assert_eq!(config.store_options().corpus_root(), root);
        assert_eq!(config.store_options().database_path(), cli_db);
        assert_eq!(config.watcher_debounce_ms(), 300);
        assert_eq!(
            config.watcher_log_path(),
            Some(home.join("env-watch.log").as_path())
        );
        assert_eq!(
            config.named_roots().get("work"),
            Some(&home.join("work-zorg"))
        );
        assert_eq!(
            config.named_roots().get("team"),
            Some(&home.join("team-zorg"))
        );
    }

    #[test]
    fn config_resolution_reports_invalid_config() {
        let temp = TempWorkspace::new();
        let home = temp.path().join("home");
        let config_path = temp.path().join("config.toml");
        std::fs::write(&config_path, "watcher_debounce_ms = \"slow\"\n")
            .expect("write invalid config");

        let error = ResolvedConfig::resolve(ConfigResolutionInputs {
            home_dir: Some(home),
            user_config_path: Some(config_path.clone()),
            ..ConfigResolutionInputs::default()
        })
        .expect_err("invalid config should fail");

        assert!(error.to_string().contains("failed to parse Zorg config"));
        assert!(
            error
                .to_string()
                .contains(&config_path.display().to_string())
        );
    }

    #[test]
    fn config_resolution_rejects_malformed_named_root_names() {
        let temp = TempWorkspace::new();
        let home = temp.path().join("home");
        let config_path = temp.path().join("config.toml");
        std::fs::write(&config_path, "[named_roots]\n\"bad/name\" = \"~/zorg\"\n")
            .expect("write invalid config");

        let error = ResolvedConfig::resolve(ConfigResolutionInputs {
            home_dir: Some(home),
            user_config_path: Some(config_path),
            ..ConfigResolutionInputs::default()
        })
        .expect_err("invalid named root should fail");

        assert!(error.to_string().contains("invalid named root"));
    }

    #[test]
    fn config_resolution_rejects_duplicate_named_roots() {
        let temp = TempWorkspace::new();
        let home = temp.path().join("home");
        let xdg = temp.path().join("xdg");
        let root = home.join("root");
        let user_config = xdg.join("zorg").join("config.toml");
        std::fs::create_dir_all(user_config.parent().expect("user config parent"))
            .expect("create user config parent");
        std::fs::write(
            &user_config,
            "root = \"~/root\"\n[named_roots]\nwork = \"~/work\"\n",
        )
        .expect("write user config");
        std::fs::create_dir_all(root.join(".zorg")).expect("create root config dir");
        std::fs::write(
            root.join(".zorg/config.toml"),
            "[named_roots]\nwork = \"~/other-work\"\n",
        )
        .expect("write root config");

        let error = ResolvedConfig::resolve(ConfigResolutionInputs {
            home_dir: Some(home),
            xdg_config_home: Some(xdg),
            ..ConfigResolutionInputs::default()
        })
        .expect_err("duplicate named roots should fail");

        assert!(error.to_string().contains("duplicate named root"));
        assert!(error.to_string().contains("work"));
    }

    #[test]
    fn empty_root_and_database_path_errors_are_clear() {
        let root_error = StoreOptions::for_root("").expect_err("empty root should fail");
        let db_error = StoreOptions::new("root", "").expect_err("empty db should fail");

        assert_eq!(root_error.to_string(), "store root must not be empty");
        assert_eq!(db_error.to_string(), "database path must not be empty");
    }

    #[test]
    fn discovery_returns_only_canonical_sources_in_stable_order() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        let nested = root.join("dir").join("nested");
        std::fs::create_dir_all(&nested).expect("create nested corpus");
        std::fs::write(root.join("root.z"), "").expect("write root source");
        std::fs::write(root.join("ignore.zo"), "").expect("write legacy source");
        std::fs::write(root.join("ignore.txt"), "").expect("write ignored source");
        std::fs::write(root.join("dir").join("init.z"), "").expect("write dir source");
        std::fs::write(nested.join("child.z"), "").expect("write child source");
        std::fs::write(nested.join("child.zot"), "").expect("write legacy child source");

        let sources = discover_corpus_sources(&root).expect("discover sources");
        let relative_paths = sources
            .iter()
            .map(|source| source.relative_path())
            .collect::<Vec<_>>();

        assert_eq!(
            relative_paths,
            vec![
                Path::new("dir/init.z"),
                Path::new("dir/nested/child.z"),
                Path::new("root.z"),
            ]
        );
        assert!(
            sources
                .iter()
                .all(|source| source.absolute_path().is_absolute())
        );
    }

    #[test]
    fn store_discovers_sources_from_configured_root() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        std::fs::create_dir_all(&root).expect("create corpus");
        std::fs::write(root.join("minimal.z"), "").expect("write source");
        let db = temp.path().join("zorg.sqlite3");
        let store = Store::open_with_options(StoreOptions::new(&root, db).expect("options"))
            .expect("open store");

        let sources = store.discover_sources().expect("discover sources");

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].relative_path(), Path::new("minimal.z"));
    }

    #[test]
    fn explicit_source_validation_rejects_unsupported_paths() {
        validate_explicit_source_path("ok.z").expect("canonical source should pass");

        let legacy = validate_explicit_source_path("legacy.zo").expect_err("legacy should fail");
        let unrelated =
            validate_explicit_source_path("notes.txt").expect_err("unrelated should fail");
        let missing =
            validate_explicit_source_path("notes").expect_err("missing extension should fail");

        assert!(legacy.to_string().contains(".zo is not a canonical"));
        assert!(unrelated.to_string().contains(".txt is not a supported"));
        assert!(missing.to_string().contains("missing .z source extension"));
    }

    #[test]
    fn full_reindex_populates_query_facing_rows() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        std::fs::create_dir_all(&root).expect("create corpus");
        write_fixture(&root, "minimal.z");
        write_fixture(&root, "nested.z");
        std::fs::write(
            root.join("unresolved.z"),
            "\
%%% @broken #z/ref #area/test
Broken fixture
%%%

This link points to #missing.
",
        )
        .expect("write unresolved fixture");
        let db = temp.path().join("zorg.sqlite3");
        let mut store = Store::open_with_options(StoreOptions::new(&root, db).expect("options"))
            .expect("open store");

        let summary = store.reindex_full().expect("reindex");

        assert_eq!(summary.discovered_files, 3);
        assert_eq!(summary.indexed_files, 3);
        assert_eq!(summary.zettel_count, 7);
        assert!(summary.diagnostic_count >= 1);

        let files = store.list_files().expect("files");
        assert_eq!(files.len(), 3);
        assert!(files.iter().all(|file| file.byte_len > 0));
        assert!(files.iter().all(|file| !file.content_hash.is_empty()));

        let zettels = store.list_zettel().expect("zettel");
        assert_eq!(zettels.len(), 7);
        let task = store
            .lookup_zettel_by_canonical_id("project/plan/task")
            .expect("lookup")
            .expect("task zettel");
        assert_eq!(task.local_id.as_deref(), Some("task"));
        assert_eq!(task.kind, "nested");

        let tags = store.list_tags().expect("tags");
        assert!(
            tags.iter()
                .any(|tag| tag.tag == "z/ref" && tag.tag_kind == "type")
        );
        assert!(
            tags.iter()
                .any(|tag| tag.tag == "area/test" && tag.tag_kind == "explicit")
        );

        let properties = store.list_properties().expect("properties");
        assert!(
            properties
                .iter()
                .any(|property| property.key == "due" && property.value == "2026-05-15")
        );
        assert!(
            properties
                .iter()
                .any(|property| property.key == "area" && property.value == "work/research")
        );

        let todos = store.list_todos().expect("todos");
        assert!(todos.iter().any(|todo| todo.marker == "[N]"));
        assert!(todos.iter().any(|todo| todo.marker == "[ ]"));
        assert!(todos.iter().any(|todo| todo.marker == "[?]"));
        assert!(todos.iter().any(|todo| todo.marker == "[X]"));

        let links = store.list_links().expect("links");
        assert!(links.iter().any(|link| {
            link.target_text == "+task"
                && link.resolved
                && link.target_canonical_id.as_deref() == Some("project/plan/task")
                && link.target_zettel_id.is_some()
        }));
        assert!(links.iter().any(|link| {
            link.target_text == "#missing" && !link.resolved && link.target_zettel_id.is_none()
        }));

        let diagnostics = store.list_diagnostics().expect("diagnostics");
        let unresolved = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code.as_deref() == Some("reference.unresolved_absolute")
                    && diagnostic.message.contains("@missing")
            })
            .expect("unresolved diagnostic");
        assert_eq!(
            unresolved.absolute_path.as_deref(),
            Some(root.join("unresolved.z").as_path())
        );
        assert_eq!(
            unresolved.relative_path.as_deref(),
            Some(Path::new("unresolved.z"))
        );
        assert_eq!(unresolved.start_line, Some(5));
        assert_eq!(unresolved.start_column, Some(21));
    }

    #[test]
    fn incremental_reindex_skips_unchanged_files() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        std::fs::create_dir_all(&root).expect("create corpus");
        write_source(
            &root,
            "minimal.z",
            "\
%%% @minimal #z/ref area::work/research
Minimal fixture
%%%

Stable body.
",
        );
        let db = temp.path().join("zorg.sqlite3");
        let mut store = Store::open_with_options(StoreOptions::new(&root, db).expect("options"))
            .expect("open store");

        let first = store.reindex().expect("initial reindex");
        let indexed_before = store.list_files().expect("files before");
        let status = store.index_status().expect("status");
        let second = store.reindex().expect("second reindex");
        let indexed_after = store.list_files().expect("files after");

        assert_eq!(first.indexed_files, 1);
        assert_eq!(first.new_files, 1);
        assert_eq!(first.zettel_count, 1);
        assert_eq!(status.indexed_files, 1);
        assert_eq!(status.unchanged_files, 1);
        assert_eq!(status.new_files, 0);
        assert_eq!(status.changed_files, 0);
        assert_eq!(status.deleted_files, 0);
        assert_eq!(second.indexed_files, 0);
        assert_eq!(second.unchanged_files, 1);
        assert_eq!(second.zettel_count, 0);
        assert_eq!(indexed_after[0].id, indexed_before[0].id);
        assert_eq!(
            indexed_after[0].indexed_at_unix_ms,
            indexed_before[0].indexed_at_unix_ms
        );
        assert_eq!(
            indexed_after[0].content_hash,
            indexed_before[0].content_hash
        );
    }

    #[test]
    fn incremental_reindex_refreshes_changed_files() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        std::fs::create_dir_all(&root).expect("create corpus");
        write_source(
            &root,
            "main.z",
            "\
%%% @main #area/one field::old
Main
%%%

This points to #missing.
",
        );
        let db = temp.path().join("zorg.sqlite3");
        let mut store = Store::open_with_options(StoreOptions::new(&root, db).expect("options"))
            .expect("open store");
        store.reindex().expect("initial reindex");
        assert!(
            store
                .list_diagnostics()
                .expect("initial diagnostics")
                .iter()
                .any(|diagnostic| diagnostic.code.as_deref()
                    == Some("reference.unresolved_absolute"))
        );

        write_source(
            &root,
            "main.z",
            "\
%%% @main #area/two field::new
Main updated
%%%

This points to #target.
",
        );
        write_source(
            &root,
            "target.z",
            "\
%%% @target #z/ref
Target
%%%
",
        );

        let summary = store.reindex().expect("changed reindex");

        assert_eq!(summary.indexed_files, 2);
        assert_eq!(summary.changed_files, 1);
        assert_eq!(summary.new_files, 1);
        assert_eq!(summary.deleted_files, 0);
        let tags = store.list_tags().expect("tags");
        assert!(tags.iter().any(|tag| tag.tag == "area/two"));
        assert!(!tags.iter().any(|tag| tag.tag == "area/one"));
        let properties = store.list_properties().expect("properties");
        assert!(
            properties
                .iter()
                .any(|property| property.key == "field" && property.value == "new")
        );
        assert!(
            !properties
                .iter()
                .any(|property| property.key == "field" && property.value == "old")
        );
        let main = store
            .lookup_zettel_by_canonical_id("main")
            .expect("lookup")
            .expect("main zettel");
        assert!(main.body_text.contains("This points to #target."));
        let links = store.list_links().expect("links");
        assert!(links.iter().any(|link| {
            link.target_text == "#target"
                && link.resolved
                && link.target_canonical_id.as_deref() == Some("target")
        }));
        assert!(!links.iter().any(|link| link.target_text == "#missing"));
        assert!(
            !store
                .list_diagnostics()
                .expect("diagnostics")
                .iter()
                .any(|diagnostic| diagnostic.code.as_deref()
                    == Some("reference.unresolved_absolute"))
        );
    }

    #[test]
    fn incremental_reindex_removes_deleted_file_rows() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        std::fs::create_dir_all(&root).expect("create corpus");
        write_source(&root, "keep.z", "%%% @keep #z/ref\nKeep\n%%%\n");
        write_source(
            &root,
            "delete.z",
            "\
%%% @delete #area/delete gone::soon
Delete
%%%

This links to #keep.
",
        );
        let db = temp.path().join("zorg.sqlite3");
        let mut store = Store::open_with_options(StoreOptions::new(&root, db).expect("options"))
            .expect("open store");
        store.reindex().expect("initial reindex");

        std::fs::remove_file(root.join("delete.z")).expect("delete source");
        let summary = store.reindex().expect("delete reindex");

        assert_eq!(summary.deleted_files, 1);
        assert_eq!(store.list_files().expect("files").len(), 1);
        assert!(
            store
                .lookup_zettel_by_canonical_id("delete")
                .expect("lookup")
                .is_none()
        );
        assert!(
            !store
                .list_tags()
                .expect("tags")
                .iter()
                .any(|tag| tag.tag == "area/delete")
        );
        assert!(
            !store
                .list_properties()
                .expect("properties")
                .iter()
                .any(|property| property.key == "gone")
        );
        assert!(store.list_links().expect("links").is_empty());
    }

    #[test]
    fn incremental_reindex_treats_renames_as_delete_plus_add() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        std::fs::create_dir_all(&root).expect("create corpus");
        write_source(&root, "old.z", "%%% @old #z/ref\nOld\n%%%\n");
        let db = temp.path().join("zorg.sqlite3");
        let mut store = Store::open_with_options(StoreOptions::new(&root, db).expect("options"))
            .expect("open store");
        store.reindex().expect("initial reindex");

        std::fs::rename(root.join("old.z"), root.join("new.z")).expect("rename source");
        write_source(&root, "new.z", "%%% @new #z/ref\nNew\n%%%\n");
        let summary = store.reindex().expect("rename reindex");

        assert_eq!(summary.new_files, 1);
        assert_eq!(summary.deleted_files, 1);
        assert_eq!(
            store
                .list_files()
                .expect("files")
                .iter()
                .map(|file| file.relative_path.as_path())
                .collect::<Vec<_>>(),
            vec![Path::new("new.z")]
        );
        assert!(
            store
                .lookup_zettel_by_canonical_id("old")
                .expect("old lookup")
                .is_none()
        );
        assert!(
            store
                .lookup_zettel_by_canonical_id("new")
                .expect("new lookup")
                .is_some()
        );
    }

    #[test]
    fn materializes_effective_tags_from_explicit_parent_file_and_directory_sources() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        std::fs::create_dir_all(root.join("dir")).expect("create corpus");
        write_source(
            &root,
            "dir/init.z",
            "\
%%% @dir #area/dir
Directory
%%%

- @dir/local #area/local Local child.
",
        );
        write_source(
            &root,
            "dir/work.z",
            "\
%%% @work #area/file
Work
%%%

- @work/parent #area/parent Parent.

  - @work/parent/child #area/child Child.
",
        );
        let db = temp.path().join("zorg.sqlite3");
        let mut store = Store::open_with_options(StoreOptions::new(&root, db).expect("options"))
            .expect("open store");

        let summary = store.reindex().expect("reindex");

        assert_eq!(summary.discovered_files, 2);
        assert_eq!(summary.effective_tag_count, 12);

        let child = lookup(&store, "work/parent/child");
        let explicit_tags = store.list_tags_for_zettel(child.id).expect("explicit tags");
        assert_eq!(
            explicit_tags
                .iter()
                .map(|tag| (tag.tag.as_str(), tag.tag_kind.as_str()))
                .collect::<Vec<_>>(),
            vec![("area/child", "explicit")]
        );

        let effective = store
            .list_effective_tags_for_zettel(child.id)
            .expect("effective tags");
        assert_effective_tag(&effective, "area/child", child.id, "explicit");
        assert_effective_tag(
            &effective,
            "area/parent",
            lookup(&store, "work/parent").id,
            "parent",
        );
        assert_effective_tag(&effective, "area/file", lookup(&store, "work").id, "parent");
        assert_effective_tag(&effective, "area/dir", lookup(&store, "dir").id, "path");

        let dir_child = lookup(&store, "dir/local");
        let dir_child_effective = store
            .list_effective_tags_for_zettel(dir_child.id)
            .expect("directory child effective tags");
        assert_effective_tag(&dir_child_effective, "area/local", dir_child.id, "explicit");
        assert_effective_tag(
            &dir_child_effective,
            "area/dir",
            lookup(&store, "dir").id,
            "parent",
        );
        assert!(
            !dir_child_effective
                .iter()
                .any(|tag| tag.tag == "area/dir" && tag.source == "path")
        );
    }

    #[test]
    fn graph_apis_expose_ancestry_link_directions_and_incremental_link_survival() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        std::fs::create_dir_all(&root).expect("create corpus");
        write_source(
            &root,
            "a.z",
            "\
%%% @a #area/root
A
%%%

- @a/section #area/section Section.
  This links to #target and #missing.

  - @a/section/leaf #area/leaf Leaf.
    Leaf links #target.
",
        );
        write_source(&root, "target.z", "%%% @target #area/target\nTarget\n%%%\n");
        let db = temp.path().join("zorg.sqlite3");
        let mut store = Store::open_with_options(StoreOptions::new(&root, db).expect("options"))
            .expect("open store");
        store.reindex().expect("initial reindex");

        let root_zettel = lookup(&store, "a");
        let section = lookup(&store, "a/section");
        let leaf = lookup(&store, "a/section/leaf");
        let target = lookup(&store, "target");

        assert_eq!(
            store
                .list_zettel_ancestors(leaf.id)
                .expect("ancestors")
                .iter()
                .map(|zettel| zettel.canonical_id.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("a"), Some("a/section")]
        );
        assert_eq!(
            store
                .list_zettel_descendants(root_zettel.id)
                .expect("descendants")
                .iter()
                .map(|zettel| zettel.canonical_id.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("a/section"), Some("a/section/leaf")]
        );

        let outgoing = store.list_outgoing_links(section.id).expect("outgoing");
        assert!(outgoing.iter().any(|link| {
            link.target_text == "#target"
                && link.resolved
                && link.target_zettel_id == Some(target.id)
        }));
        assert!(
            outgoing
                .iter()
                .any(|link| link.target_text == "#missing" && !link.resolved)
        );
        assert_eq!(
            store
                .list_incoming_links(target.id)
                .expect("incoming")
                .len(),
            2
        );

        write_source(&root, "new.z", "%%% @new #area/new\nNew\n%%%\n");
        store.reindex().expect("incremental reindex");

        let section = lookup(&store, "a/section");
        let target = lookup(&store, "target");
        let outgoing = store
            .list_outgoing_links(section.id)
            .expect("outgoing after incremental");
        assert!(outgoing.iter().any(|link| {
            link.target_text == "#target"
                && link.resolved
                && link.target_zettel_id == Some(target.id)
        }));
        assert!(
            outgoing
                .iter()
                .any(|link| link.target_text == "#missing" && !link.resolved)
        );
        assert_eq!(
            store
                .list_incoming_links(target.id)
                .expect("incoming after incremental")
                .len(),
            2
        );
    }

    #[test]
    fn failed_read_does_not_replace_existing_snapshot() {
        let temp = TempWorkspace::new();
        let root = temp.path().join("corpus");
        std::fs::create_dir_all(&root).expect("create corpus");
        write_fixture(&root, "minimal.z");
        let db = temp.path().join("zorg.sqlite3");
        let mut store = Store::open_with_options(StoreOptions::new(&root, db).expect("options"))
            .expect("open store");
        store.reindex_full().expect("initial reindex");
        let before_files = store.list_files().expect("files before");
        let before_zettel = store.list_zettel().expect("zettel before");

        std::fs::write(root.join("bad.z"), [0xff]).expect("write invalid utf8 source");

        let error = store.reindex_full().expect_err("invalid UTF-8 should fail");
        assert!(error.to_string().contains("failed to read source"));
        assert_eq!(store.list_files().expect("files after"), before_files);
        assert_eq!(store.list_zettel().expect("zettel after"), before_zettel);
    }

    fn write_fixture(root: &Path, name: &str) {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("fixtures/corpus")
            .join(name);
        let source = std::fs::read_to_string(fixture).expect("fixture source");
        std::fs::write(root.join(name), source).expect("write fixture");
    }

    fn write_source(root: &Path, name: &str, source: &str) {
        let path = root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create source parent");
        }
        std::fs::write(path, source).expect("write source");
    }

    fn create_schema_v1_fixture(db: &Path) {
        if let Some(parent) = db.parent() {
            std::fs::create_dir_all(parent).expect("create fixture db parent");
        }
        let mut connection = Connection::open(db).expect("open fixture db");
        let migration = MIGRATIONS
            .iter()
            .find(|migration| migration.version == 1)
            .expect("schema v1 migration");
        let transaction = connection.transaction().expect("begin fixture transaction");
        transaction
            .execute_batch(
                r#"
CREATE TABLE schema_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#,
            )
            .expect("create schema metadata table");
        transaction
            .execute_batch(migration.sql)
            .expect("create schema v1 fixture");
        transaction
            .execute(
                "INSERT INTO files (
                    id, absolute_path, relative_path, mtime_unix_ms, byte_len, content_hash,
                    indexed_at_unix_ms
                 ) VALUES (1, '/tmp/zorg-fixture/fixture.z', 'fixture.z', 1, 42,
                    'fixture-hash', 2)",
                [],
            )
            .expect("insert fixture file");
        transaction
            .execute(
                "INSERT INTO zettel (
                    id, file_id, parent_id, source_order, kind, parser_key, title, canonical_id,
                    local_id, body_text, start_byte, end_byte, start_line, start_column, end_line,
                    end_column
                 ) VALUES (1, 1, NULL, 0, 'file', '0', 'Fixture', 'fixture', NULL,
                    'Persisted fixture body.', 0, 42, 1, 1, 3, 4)",
                [],
            )
            .expect("insert fixture zettel");
        transaction
            .execute(
                "INSERT INTO zettel_ids (canonical_id, zettel_id) VALUES ('fixture', 1)",
                [],
            )
            .expect("insert fixture zettel id");
        transaction
            .execute(
                "INSERT INTO links (
                    id, source_zettel_id, target_zettel_id, target_canonical_id, target_text,
                    link_kind, resolved, start_byte, end_byte
                 ) VALUES (1, 1, 1, 'fixture', '#fixture', 'absolute', 1, 10, 18)",
                [],
            )
            .expect("insert fixture link");
        transaction
            .execute(
                "INSERT INTO tags (id, zettel_id, tag, tag_kind, start_byte, end_byte)
                 VALUES (1, 1, 'area/test', 'explicit', 0, 10)",
                [],
            )
            .expect("insert fixture tag");
        transaction
            .execute(
                "INSERT INTO effective_tags (id, zettel_id, tag, source_zettel_id, source)
                 VALUES (1, 1, 'area/test', 1, 'explicit')",
                [],
            )
            .expect("insert fixture effective tag");
        transaction
            .execute(
                "INSERT INTO properties (id, zettel_id, key, value, start_byte, end_byte)
                 VALUES (1, 1, 'due', '2026-05-03', 20, 35)",
                [],
            )
            .expect("insert fixture property");
        transaction
            .execute(
                "INSERT INTO todos (id, zettel_id, marker, start_byte, end_byte)
                 VALUES (1, 1, '[N]', 36, 39)",
                [],
            )
            .expect("insert fixture todo");
        transaction
            .execute(
                "INSERT INTO text_index (zettel_id, title_text, body_text, raw_text)
                 VALUES (1, 'Fixture', 'Persisted fixture body.', 'Fixture raw text')",
                [],
            )
            .expect("insert fixture text index");
        transaction
            .execute(
                "INSERT INTO diagnostics (
                    id, file_id, zettel_id, severity, category, code, message, start_byte,
                    end_byte, start_line, start_column, end_line, end_column
                 ) VALUES (1, 1, 1, 'info', 'semantic', 'fixture.notice',
                    'Fixture diagnostic persisted.', 0, 1, 1, 1, 1, 2)",
                [],
            )
            .expect("insert fixture diagnostic");
        transaction
            .execute(
                "INSERT INTO index_metadata (key, value) VALUES ('last_indexed_at_unix_ms', '2')",
                [],
            )
            .expect("insert fixture index metadata");
        transaction
            .execute(
                "INSERT INTO schema_metadata (key, value) VALUES ('schema_version', '1')",
                [],
            )
            .expect("insert fixture schema metadata");
        transaction
            .pragma_update(None, "user_version", 1)
            .expect("record fixture user_version");
        transaction.commit().expect("commit fixture db");
    }

    fn assert_schema_objects(connection: &Connection) {
        for table in SCHEMA_TABLES {
            assert!(
                schema_object_exists(connection, "table", table),
                "missing schema table {table}"
            );
        }
        for index in SCHEMA_INDEXES {
            assert!(
                schema_object_exists(connection, "index", index),
                "missing schema index {index}"
            );
        }
    }

    fn schema_object_exists(connection: &Connection, object_type: &str, name: &str) -> bool {
        connection
            .query_row(
                "SELECT EXISTS (
                    SELECT 1
                    FROM sqlite_master
                    WHERE type = ?1 AND name = ?2
                )",
                (object_type, name),
                |row| row.get::<_, bool>(0),
            )
            .expect("query schema object")
    }

    fn lookup(store: &Store, canonical_id: &str) -> StoredZettel {
        store
            .lookup_zettel_by_canonical_id(canonical_id)
            .expect("lookup")
            .unwrap_or_else(|| panic!("missing zettel @{canonical_id}"))
    }

    fn assert_effective_tag(
        effective_tags: &[StoredEffectiveTag],
        expected_tag: &str,
        expected_source_zettel_id: i64,
        expected_source: &str,
    ) {
        assert!(
            effective_tags.iter().any(|tag| {
                tag.tag == expected_tag
                    && tag.source_zettel_id == Some(expected_source_zettel_id)
                    && tag.source == expected_source
            }),
            "missing effective tag {expected_tag} from {expected_source}: {effective_tags:#?}"
        );
    }
}
