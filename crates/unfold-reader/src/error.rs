//! Errors from the reader.

use unfold_store::StoreError;

/// Why a reader request failed.
#[derive(Debug, thiserror::Error)]
pub enum ReaderError {
    /// A store query failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A stored range does not fit the verified source: the store and the bytes it hashed
    /// disagree, so the cache is damaged.
    #[error("{path}: stored range {start}..{end} does not fit the verified source")]
    InvalidRange {
        /// The file.
        path: Box<str>,
        /// The range's start byte.
        start: u32,
        /// The range's end byte.
        end: u32,
    },
}

/// A reader result.
pub type Result<T, E = ReaderError> = std::result::Result<T, E>;
