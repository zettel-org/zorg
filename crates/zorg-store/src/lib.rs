//! Storage and indexing boundary for Zorg.

use std::env;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};
use zorg_core::{ZorgError, ZorgResult};

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
}
