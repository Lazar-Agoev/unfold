//! The store's error type.

use std::path::PathBuf;

use rusqlite::ErrorCode;

/// Why a store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite reported an error.
    #[error("SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A file next to the database could not be removed while recreating it.
    #[error("cannot remove {path}: {source}")]
    Remove {
        /// The file that could not be removed.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The database's file system refused write-ahead logging (for example a network volume),
    /// so readers could not see a consistent graph during writes.
    #[error("write-ahead logging is unavailable for {path} (journal mode is `{mode}`)")]
    WalUnavailable {
        /// The database file.
        path: PathBuf,
        /// The journal mode SQLite kept instead.
        mode: String,
    },
    /// A stored value could not be decoded into the graph model, which means the database was
    /// modified outside the store or is damaged.
    #[error("invalid stored {what}: `{value}`")]
    Invalid {
        /// What was being decoded, such as `block kind`.
        what: &'static str,
        /// The offending value.
        value: String,
    },
    /// A number in an update does not fit the column that stores it.
    #[error("{what} {value} is out of range")]
    OutOfRange {
        /// What the number is, such as `file size`.
        what: &'static str,
        /// The offending value.
        value: u64,
    },
    /// The caller cancelled a commit; nothing was written.
    #[error("the commit was cancelled")]
    Cancelled,
    /// A test-only fault injected into a commit.
    #[cfg(test)]
    #[error("injected fault at {0:?}")]
    Injected(crate::writer::FaultPoint),
}

impl StoreError {
    /// Whether the error means the database file is damaged or is not a database at all. The
    /// caller recovers by recreating the cache ([`crate::Store::recreate`]).
    pub fn is_corrupt(&self) -> bool {
        match self {
            Self::Sqlite(error) => is_corruption(error),
            Self::Invalid { .. } => true,
            _ => false,
        }
    }
}

/// Whether SQLite reported a damaged file or a file that is not a database.
pub(crate) fn is_corruption(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase)
    )
}

/// The store's result type.
pub type Result<T, E = StoreError> = std::result::Result<T, E>;
