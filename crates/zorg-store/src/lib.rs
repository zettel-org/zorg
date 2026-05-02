//! Storage and indexing boundary for Zorg.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use zorg_core::{
    BodyBlock, Diagnostic, DiagnosticCategory, Reference, ReferenceTarget, ResolvedReference,
    Severity, SourceSpan, TodoMarker, Zettel, ZettelDocument, ZettelKind, ZorgError, ZorgResult,
};

/// Current SQLite schema version created by this crate.
pub const SCHEMA_VERSION: i64 = 1;

const DEFAULT_ROOT_DIR: &str = "zorg";
const DEFAULT_DB_DIR: &str = ".zorg";
const DEFAULT_DB_FILE: &str = "zorg.sqlite3";
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

/// Summary returned after a full snapshot reindex.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ReindexSummary {
    /// Number of canonical `.z` files discovered under the corpus root.
    pub discovered_files: usize,
    /// Number of discovered files parsed and written to the index.
    pub indexed_files: usize,
    /// Number of zettel rows written.
    pub zettel_count: usize,
    /// Number of diagnostic rows written.
    pub diagnostic_count: usize,
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

    /// Rebuilds the indexed snapshot for this store's corpus root in one transaction.
    pub fn reindex_full(&mut self) -> ZorgResult<ReindexSummary> {
        let sources = self.discover_sources()?;
        let mut files = Vec::with_capacity(sources.len());
        let mut documents = Vec::with_capacity(sources.len());

        for source in sources {
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
            let document =
                zorg_parse::parse_zettel_document_with_path(&source_text, source.absolute_path())
                    .map_err(|error| {
                    operation_failed(format!(
                        "failed to parse source {}: {error}",
                        source.absolute_path().display()
                    ))
                })?;

            files.push(SourceSnapshot {
                source,
                mtime_unix_ms: modified_unix_ms(&metadata),
                byte_len: i64::try_from(source_text.len()).unwrap_or(i64::MAX),
                content_hash: content_hash(source_text.as_bytes()),
            });
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
                "SELECT id, file_id, zettel_id, severity, category, code, message
                 FROM diagnostics
                 ORDER BY id",
            )
            .map_err(|error| {
                operation_failed(format!("failed to prepare diagnostic query: {error}"))
            })?;
        let rows = statement
            .query_map([], |row| {
                Ok(StoredDiagnostic {
                    id: row.get(0)?,
                    file_id: row.get(1)?,
                    zettel_id: row.get(2)?,
                    severity: row.get(3)?,
                    category: row.get(4)?,
                    code: row.get(5)?,
                    message: row.get(6)?,
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

#[derive(Debug, Clone)]
struct SourceSnapshot {
    source: CorpusSource,
    mtime_unix_ms: Option<i64>,
    byte_len: i64,
    content_hash: String,
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

    Ok(ReindexSummary {
        discovered_files: files.len(),
        indexed_files: files.len(),
        zettel_count: state.zettel_count,
        diagnostic_count: state.diagnostic_count,
    })
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
        let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN (
                    'schema_metadata',
                    'files',
                    'zettel',
                    'zettel_ids',
                    'links',
                    'tags',
                    'effective_tags',
                    'properties',
                    'todos',
                    'text_index',
                    'diagnostics',
                    'index_metadata'
                )",
                [],
                |row| row.get(0),
            )
            .expect("table count");
        assert_eq!(table_count, 12);
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
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("reference.unresolved_absolute")
                && diagnostic.message.contains("@missing")
        }));
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
}
