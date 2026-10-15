//! Source text: snapshots and their hashes, byte ranges, the line index, and the mapping between
//! the engine's UTF-8 byte offsets and the UTF-16 offsets the UI uses.

mod hash;
mod label;
mod lines;
mod range;
mod snapshot;
mod utf16;

pub use hash::{ContentHash, HashParseError};
pub use label::{LABEL_MAX_UTF16, short_label, utf16_len};
pub use lines::{LineBreaks, LineIndex};
pub use range::{ByteRange, LineSpan, RangeError};
pub use snapshot::{Snapshot, SnapshotError};
pub use utf16::{OffsetError, Utf16Index, Utf16Range};
