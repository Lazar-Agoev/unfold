//! Opening a folder's database: version checks, rebuilding stale or damaged caches, and
//! handing out the writer and the reader pool.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, ErrorCode, OpenFlags, OptionalExtension};

use crate::error::{Result, StoreError};
use crate::read::ReadPool;
use crate::schema::{self, SCHEMA_VERSION, STATEMENT_CACHE_CAPACITY, WRITER_PRAGMAS, meta_key};
use crate::writer::Writer;

/// How to open a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreConfig {
    /// The running engine's version. A database written by another version is rebuilt.
    pub engine_version: String,
    /// How many idle reader connections the pool keeps open.
    pub idle_readers: usize,
}

impl StoreConfig {
    /// A configuration for `engine_version` with the default pool size.
    pub fn new(engine_version: impl Into<String>) -> Self {
        Self {
            engine_version: engine_version.into(),
            idle_readers: 4,
        }
    }
}

/// What opening found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    /// No database existed (or the file was empty); a new one was created.
    Created,
    /// The existing database matches this schema and engine; its graph is usable at once.
    Reused,
    /// The existing database was discarded and a new one created.
    Recreated(RecreateReason),
}

/// Why an existing database was discarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecreateReason {
    /// It has another schema version.
    SchemaChanged {
        /// The version it recorded, if any.
        found: Option<String>,
    },
    /// It was written by another engine version.
    EngineChanged {
        /// The version it recorded, if any.
        found: Option<String>,
    },
    /// It is damaged, not a database, or not an Unfold database.
    Corrupt,
}

/// An open database: its writer, its readers, and what opening found.
#[derive(Debug)]
pub struct Store {
    /// The only writer; the folder's engine thread owns it.
    pub writer: Writer,
    /// Read-only connections for every other thread.
    pub readers: ReadPool,
    /// Whether the graph was reused or must be compiled again.
    pub opened: Opened,
}

impl Store {
    /// Opens the database at `path`, creating it if needed. A database with another schema
    /// or engine version, or one that is damaged, is discarded and recreated: it is a cache.
    ///
    /// The directory must exist. Fails with [`StoreError::WalUnavailable`] when the file
    /// system does not support write-ahead logging.
    pub fn open(path: &Path, config: &StoreConfig) -> Result<Self> {
        match open_existing(path, &config.engine_version) {
            Ok((conn, Found::Empty)) => {
                schema::create(&conn, &config.engine_version)?;
                Ok(Self::assemble(conn, path, config, Opened::Created))
            }
            Ok((conn, Found::Current)) => Ok(Self::assemble(conn, path, config, Opened::Reused)),
            Ok((conn, Found::Stale(reason))) => {
                drop(conn);
                Self::create_new(path, config, reason)
            }
            Err(error) if error.is_corrupt() => Self::create_new(path, config, RecreateReason::Corrupt),
            Err(error) => Err(error),
        }
    }

    fn create_new(path: &Path, config: &StoreConfig, reason: RecreateReason) -> Result<Self> {
        remove_database_files(path)?;
        let conn = connect_writer(path)?;
        schema::create(&conn, &config.engine_version)?;
        Ok(Self::assemble(conn, path, config, Opened::Recreated(reason)))
    }

    fn assemble(conn: Connection, path: &Path, config: &StoreConfig, opened: Opened) -> Self {
        Self {
            writer: Writer::new(conn, path.to_path_buf(), &config.engine_version),
            readers: ReadPool::new(path.to_path_buf(), config.idle_readers),
            opened,
        }
    }
}

/// What an existing file holds.
pub(crate) enum Found {
    /// No tables: a new file.
    Empty,
    /// This schema and engine.
    Current,
    /// Something to discard.
    Stale(RecreateReason),
}

fn open_existing(path: &Path, engine_version: &str) -> Result<(Connection, Found)> {
    let conn = connect_writer(path)?;
    let found = inspect(&conn, engine_version)?;
    Ok((conn, found))
}

/// Reads the versions an existing database records. Only reads, so it also serves read-only
/// connections ([`ReadPool::open_existing`]).
pub(crate) fn inspect(conn: &Connection, engine_version: &str) -> Result<Found> {
    let objects: i64 = conn.query_row("SELECT count(*) FROM sqlite_schema", [], |row| row.get(0))?;
    if objects == 0 {
        return Ok(Found::Empty);
    }
    let has_meta: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'meta')",
        [],
        |row| row.get(0),
    )?;
    if !has_meta {
        return Ok(Found::Stale(RecreateReason::Corrupt));
    }
    let value = |key: &str| {
        conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
            row.get::<_, String>(0)
        })
        .optional()
    };
    let versions =
        value(meta_key::SCHEMA_VERSION).and_then(|schema| Ok((schema, value(meta_key::ENGINE_VERSION)?)));
    let (schema, engine) = match versions {
        Ok(versions) => versions,
        // A `meta` table of another shape (no `value` column, values that are not text): not
        // an Unfold database.
        Err(error) if is_foreign_meta(&error) => return Ok(Found::Stale(RecreateReason::Corrupt)),
        Err(error) => return Err(error.into()),
    };
    if schema.as_deref() != Some(SCHEMA_VERSION.to_string().as_str()) {
        return Ok(Found::Stale(RecreateReason::SchemaChanged { found: schema }));
    }
    if engine.as_deref() != Some(engine_version) {
        return Ok(Found::Stale(RecreateReason::EngineChanged { found: engine }));
    }
    Ok(Found::Current)
}

/// Whether reading the versions failed because `meta` is not the store's table, as opposed to
/// an I/O or locking failure the caller should see.
fn is_foreign_meta(error: &rusqlite::Error) -> bool {
    match error {
        rusqlite::Error::SqliteFailure(failure, _) => failure.code == ErrorCode::Unknown,
        rusqlite::Error::SqlInputError { error, .. } => error.code == ErrorCode::Unknown,
        rusqlite::Error::InvalidColumnType(..) | rusqlite::Error::FromSqlConversionFailure(..) => true,
        _ => false,
    }
}

/// Opens the read-write connection, switches it to write-ahead logging, and applies the
/// writer's settings.
fn connect_writer(path: &Path) -> Result<Connection> {
    let flags =
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags)?;
    conn.set_prepared_statement_cache_capacity(STATEMENT_CACHE_CAPACITY);
    let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(StoreError::WalUnavailable {
            path: path.to_path_buf(),
            mode,
        });
    }
    conn.execute_batch(WRITER_PRAGMAS)?;
    Ok(conn)
}

/// Removes the database and its write-ahead log, shared-memory, and rollback-journal files.
fn remove_database_files(path: &Path) -> Result<()> {
    let mut candidates = vec![path.to_path_buf()];
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        candidates.push(PathBuf::from(name));
    }
    for candidate in candidates {
        match std::fs::remove_file(&candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(StoreError::Remove {
                    path: candidate,
                    source,
                });
            }
        }
    }
    Ok(())
}
