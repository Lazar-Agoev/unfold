//! Conversions between the graph model and column values: vocabularies as their strings,
//! hashes as 32-byte blobs, offsets as integers, node flags as bit sets, and paths as text.
//!
//! Decoding never panics: a value the model rejects becomes [`StoreError::Invalid`].

use std::str::FromStr;

use rusqlite::Row;
use rusqlite::types::ValueRef;
use unfold_core::vocab::{Flag, Flags};
use unfold_core::{ByteRange, ContentHash, Key, LineSpan, ModuleLabel, RepoPath};

use crate::error::{Result, StoreError};

/// A stored value the model rejects.
pub(crate) fn invalid(what: &'static str, value: impl Into<String>) -> StoreError {
    StoreError::Invalid {
        what,
        value: value.into(),
    }
}

/// Text in column `index`, borrowed from the row.
pub(crate) fn text<'r>(row: &'r Row<'_>, index: usize) -> Result<&'r str> {
    match row.get_ref(index)? {
        ValueRef::Text(bytes) => std::str::from_utf8(bytes).map_err(|_| invalid("text", "invalid UTF-8")),
        other => Err(invalid("text", format!("{:?}", other.data_type()))),
    }
}

/// Optional text in column `index`.
pub(crate) fn opt_text<'r>(row: &'r Row<'_>, index: usize) -> Result<Option<&'r str>> {
    match row.get_ref(index)? {
        ValueRef::Null => Ok(None),
        _ => text(row, index).map(Some),
    }
}

/// Optional boxed text in column `index`.
pub(crate) fn opt_boxed(row: &Row<'_>, index: usize) -> Result<Option<Box<str>>> {
    Ok(opt_text(row, index)?.map(Into::into))
}

/// A vocabulary value stored as its string.
pub(crate) fn vocab<T: FromStr>(row: &Row<'_>, index: usize, what: &'static str) -> Result<T> {
    let value = text(row, index)?;
    value.parse().map_err(|_| invalid(what, value))
}

/// An optional vocabulary value.
pub(crate) fn opt_vocab<T: FromStr>(row: &Row<'_>, index: usize, what: &'static str) -> Result<Option<T>> {
    match opt_text(row, index)? {
        None => Ok(None),
        Some(value) => value.parse().map(Some).map_err(|_| invalid(what, value)),
    }
}

/// A folder-relative path.
pub(crate) fn path(row: &Row<'_>, index: usize) -> Result<RepoPath> {
    let value = text(row, index)?;
    RepoPath::new(value).map_err(|_| invalid("path", value))
}

/// A structural key.
pub(crate) fn key(row: &Row<'_>, index: usize) -> Result<Key> {
    Ok(Key::from_text(text(row, index)?))
}

/// An optional structural key (a `LEFT JOIN` on `keys`).
pub(crate) fn opt_key(row: &Row<'_>, index: usize) -> Result<Option<Key>> {
    Ok(opt_text(row, index)?.map(Key::from_text))
}

/// An optional module label.
pub(crate) fn opt_module(row: &Row<'_>, index: usize) -> Result<Option<ModuleLabel>> {
    Ok(opt_text(row, index)?.map(ModuleLabel::from_text))
}

/// An optional SHA-256 stored as a 32-byte blob.
pub(crate) fn opt_hash(row: &Row<'_>, index: usize) -> Result<Option<ContentHash>> {
    match row.get_ref(index)? {
        ValueRef::Null => Ok(None),
        ValueRef::Blob(bytes) => {
            let digest: [u8; 32] = bytes
                .try_into()
                .map_err(|_| invalid("hash", format!("{} bytes", bytes.len())))?;
            Ok(Some(ContentHash::from_bytes(digest)))
        }
        other => Err(invalid("hash", format!("{:?}", other.data_type()))),
    }
}

/// A SHA-256 stored as a 32-byte blob.
pub(crate) fn hash(row: &Row<'_>, index: usize) -> Result<ContentHash> {
    opt_hash(row, index)?.ok_or_else(|| invalid("hash", "NULL"))
}

/// A `u32` column (offsets, lines, local IDs, positions). A value out of range means the
/// database is damaged, so it is [`StoreError::Invalid`] like any other undecodable value.
pub(crate) fn u32_at(row: &Row<'_>, index: usize) -> Result<u32> {
    opt_u32(row, index)?.ok_or_else(|| invalid("integer", "NULL"))
}

/// An optional `u32` column.
pub(crate) fn opt_u32(row: &Row<'_>, index: usize) -> Result<Option<u32>> {
    let Some(value) = row.get::<_, Option<i64>>(index)? else {
        return Ok(None);
    };
    u32::try_from(value)
        .map(Some)
        .map_err(|_| invalid("integer", value.to_string()))
}

/// A byte range from two columns.
pub(crate) fn range(row: &Row<'_>, start: usize) -> Result<ByteRange> {
    let (from, to) = (u32_at(row, start)?, u32_at(row, start + 1)?);
    ByteRange::new(from, to).map_err(|error| invalid("range", error.to_string()))
}

/// An optional byte range from two nullable columns: absent when the start is.
pub(crate) fn opt_range(row: &Row<'_>, start: usize) -> Result<Option<ByteRange>> {
    match row.get_ref(start)? {
        ValueRef::Null => Ok(None),
        _ => range(row, start).map(Some),
    }
}

/// The start and end columns of an optional byte range.
pub(crate) fn range_columns(range: Option<ByteRange>) -> (Option<u32>, Option<u32>) {
    (range.map(|range| range.start), range.map(|range| range.end))
}

/// A node's flags as a bit set: bit *i* for the *i*-th [`Flag`].
pub(crate) fn flag_bits(flags: Flags) -> u32 {
    Flag::ALL
        .iter()
        .enumerate()
        .filter(|(_, flag)| flags.contains(**flag))
        .fold(0, |bits, (bit, _)| bits | 1 << bit)
}

/// Node flags from the bit set in column `index` ([`flag_bits`]).
pub(crate) fn flags(row: &Row<'_>, index: usize) -> Result<Flags> {
    let bits = u32_at(row, index)?;
    if bits >> Flag::ALL.len() != 0 {
        return Err(invalid("node flags", bits.to_string()));
    }
    Ok(Flag::ALL
        .iter()
        .enumerate()
        .filter(|(bit, _)| bits & 1 << bit != 0)
        .map(|(_, flag)| *flag)
        .collect())
}

/// A line span from two columns.
pub(crate) fn lines(row: &Row<'_>, start: usize) -> Result<LineSpan> {
    Ok(LineSpan {
        start: u32_at(row, start)?,
        end: u32_at(row, start + 1)?,
    })
}

/// A count that SQLite returned as an integer.
pub(crate) fn count(row: &Row<'_>, index: usize) -> Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| invalid("count", value.to_string()))
}

/// A `u64` that must fit SQLite's signed 64-bit integers.
pub(crate) fn to_i64(what: &'static str, value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| StoreError::OutOfRange { what, value })
}

/// A position in a list (a local ID, an app's or target's position) as an integer column.
pub(crate) fn position(what: &'static str, index: usize) -> Result<i64> {
    i64::try_from(index).map_err(|_| StoreError::OutOfRange {
        what,
        value: index as u64,
    })
}
