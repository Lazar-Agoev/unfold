//! Conversion between UTF-8 byte offsets (the engine's unit) and UTF-16 code-unit offsets (the
//! UI's unit) within one file.

use super::range::ByteRange;

/// An offset that cannot be converted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OffsetError {
    /// The offset is past the end of the text.
    #[error("offset {offset} is past the end of the text ({len})")]
    OutOfBounds {
        /// The rejected offset.
        offset: u32,
        /// The text's length in the offset's unit.
        len: u32,
    },
    /// The byte offset falls inside a multi-byte UTF-8 sequence.
    #[error("byte offset {offset} is not on a character boundary")]
    NotCharBoundary {
        /// The rejected byte offset.
        offset: u32,
    },
    /// The UTF-16 offset falls between the two halves of a surrogate pair.
    #[error("UTF-16 offset {offset} splits a surrogate pair")]
    InsideSurrogatePair {
        /// The rejected UTF-16 offset.
        offset: u32,
    },
    /// A range's start is after its end.
    #[error("range start {start} is after its end {end}")]
    Reversed {
        /// The range's start.
        start: u32,
        /// The range's end.
        end: u32,
    },
}

/// A half-open range of UTF-16 code-unit offsets, as the UI receives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Utf16Range {
    /// The first included code unit.
    pub start: u32,
    /// The code unit immediately after the last included one.
    pub end: u32,
}

/// A run of consecutive non-ASCII scalars that all have the same UTF-8 width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    byte_start: u32,
    utf16_start: u32,
    count: u32,
    /// UTF-8 bytes per scalar: 2, 3, or 4.
    width: u8,
}

impl Run {
    /// UTF-16 units per scalar: astral scalars (four UTF-8 bytes) need a surrogate pair.
    const fn units(self) -> u32 {
        if self.width == 4 { 2 } else { 1 }
    }

    const fn byte_end(self) -> u32 {
        self.byte_start + self.count * self.width as u32
    }

    const fn utf16_end(self) -> u32 {
        self.utf16_start + self.count * self.units()
    }
}

/// Converts offsets for one text in O(log n) per offset.
///
/// The index stores only runs of non-ASCII scalars (ASCII maps one to one), so an ASCII file
/// costs nothing and conversion there is a bounds check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utf16Index {
    runs: Vec<Run>,
    len_utf8: u32,
    len_utf16: u32,
}

impl Utf16Index {
    /// Indexes `text`. Texts are at most `u32::MAX` bytes (see [`Snapshot`](super::Snapshot)).
    pub fn new(text: &str) -> Self {
        let len_utf8 = u32::try_from(text.len()).unwrap_or(u32::MAX);
        if text.is_ascii() {
            return Self {
                runs: Vec::new(),
                len_utf8,
                len_utf16: len_utf8,
            };
        }
        let mut runs: Vec<Run> = Vec::new();
        let mut utf16 = 0u32;
        for (index, character) in text.char_indices() {
            let width = character.len_utf8();
            if width > 1 {
                let byte_start = u32::try_from(index).unwrap_or(u32::MAX);
                // `width` is 2, 3, or 4 here.
                let width = width as u8;
                match runs.last_mut() {
                    Some(run) if run.width == width && run.byte_end() == byte_start => run.count += 1,
                    _ => runs.push(Run {
                        byte_start,
                        utf16_start: utf16,
                        count: 1,
                        width,
                    }),
                }
            }
            utf16 = utf16.saturating_add(u32::try_from(character.len_utf16()).unwrap_or(2));
        }
        Self {
            runs,
            len_utf8,
            len_utf16: utf16,
        }
    }

    /// The text's length in bytes.
    pub fn len_utf8(&self) -> u32 {
        self.len_utf8
    }

    /// The text's length in UTF-16 code units.
    pub fn len_utf16(&self) -> u32 {
        self.len_utf16
    }

    /// Whether every scalar is ASCII, so offsets are equal in both units.
    pub fn is_ascii(&self) -> bool {
        self.runs.is_empty()
    }

    /// The UTF-16 offset of byte `offset`, which must be a character boundary.
    pub fn to_utf16(&self, offset: u32) -> Result<u32, OffsetError> {
        if offset > self.len_utf8 {
            return Err(OffsetError::OutOfBounds {
                offset,
                len: self.len_utf8,
            });
        }
        let Some(run) = self.run_before(|run| run.byte_start <= offset) else {
            return Ok(offset);
        };
        if offset < run.byte_end() {
            let delta = offset - run.byte_start;
            let width = u32::from(run.width);
            if !delta.is_multiple_of(width) {
                return Err(OffsetError::NotCharBoundary { offset });
            }
            Ok(run.utf16_start + delta / width * run.units())
        } else {
            Ok(run.utf16_end() + (offset - run.byte_end()))
        }
    }

    /// The byte offset of UTF-16 `offset`, which must not split a surrogate pair.
    pub fn to_utf8(&self, offset: u32) -> Result<u32, OffsetError> {
        if offset > self.len_utf16 {
            return Err(OffsetError::OutOfBounds {
                offset,
                len: self.len_utf16,
            });
        }
        let Some(run) = self.run_before(|run| run.utf16_start <= offset) else {
            return Ok(offset);
        };
        if offset < run.utf16_end() {
            let delta = offset - run.utf16_start;
            if !delta.is_multiple_of(run.units()) {
                return Err(OffsetError::InsideSurrogatePair { offset });
            }
            Ok(run.byte_start + delta / run.units() * u32::from(run.width))
        } else {
            Ok(run.byte_end() + (offset - run.utf16_end()))
        }
    }

    /// Converts a byte range to UTF-16.
    pub fn range_to_utf16(&self, range: ByteRange) -> Result<Utf16Range, OffsetError> {
        if range.start > range.end {
            return Err(OffsetError::Reversed {
                start: range.start,
                end: range.end,
            });
        }
        Ok(Utf16Range {
            start: self.to_utf16(range.start)?,
            end: self.to_utf16(range.end)?,
        })
    }

    /// Converts a UTF-16 range to bytes.
    pub fn range_to_utf8(&self, range: Utf16Range) -> Result<ByteRange, OffsetError> {
        if range.start > range.end {
            return Err(OffsetError::Reversed {
                start: range.start,
                end: range.end,
            });
        }
        // Conversion is monotonic, so the converted range stays well ordered.
        Ok(ByteRange {
            start: self.to_utf8(range.start)?,
            end: self.to_utf8(range.end)?,
        })
    }

    /// The last run whose start satisfies `starts_at_or_before`, if any.
    fn run_before(&self, starts_at_or_before: impl Fn(&Run) -> bool) -> Option<Run> {
        let after = self.runs.partition_point(starts_at_or_before);
        after.checked_sub(1).map(|index| self.runs[index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn utf16_prefix(text: &str, byte: usize) -> u32 {
        u32::try_from(text[..byte].encode_utf16().count()).unwrap()
    }

    #[test]
    fn ascii_is_identity() {
        let index = Utf16Index::new("hello");
        assert!(index.is_ascii());
        assert_eq!(index.to_utf16(3), Ok(3));
        assert_eq!(index.to_utf8(5), Ok(5));
        assert_eq!(
            index.to_utf16(6),
            Err(OffsetError::OutOfBounds { offset: 6, len: 5 })
        );
    }

    #[test]
    fn bom_counts_as_one_unit() {
        let index = Utf16Index::new("\u{feff}x");
        assert_eq!(index.len_utf8(), 4);
        assert_eq!(index.len_utf16(), 2);
        assert_eq!(index.to_utf16(3), Ok(1));
        assert_eq!(index.to_utf16(1), Err(OffsetError::NotCharBoundary { offset: 1 }));
    }

    #[test]
    fn astral_characters_take_two_units() {
        let text = "a😀b𝄞c";
        let index = Utf16Index::new(text);
        assert_eq!(index.to_utf16(5), Ok(3));
        assert_eq!(index.to_utf8(3), Ok(5));
        assert_eq!(
            index.to_utf8(2),
            Err(OffsetError::InsideSurrogatePair { offset: 2 })
        );
        assert_eq!(index.len_utf16(), 7);
        let range = ByteRange::new(1, 6).unwrap();
        let converted = index.range_to_utf16(range).unwrap();
        assert_eq!(converted, Utf16Range { start: 1, end: 4 });
        assert_eq!(index.range_to_utf8(converted), Ok(range));
    }

    #[test]
    fn mixed_widths_and_line_separators() {
        let text = "é界😀\u{2028}x\r\n";
        let index = Utf16Index::new(text);
        for (byte, _) in text.char_indices() {
            assert_eq!(
                index.to_utf16(u32::try_from(byte).unwrap()),
                Ok(utf16_prefix(text, byte))
            );
        }
    }

    proptest! {
        #[test]
        fn agrees_with_encode_utf16(text in "\\PC{0,64}") {
            let index = Utf16Index::new(&text);
            prop_assert_eq!(index.len_utf16(), u32::try_from(text.encode_utf16().count()).unwrap());
            let mut boundaries = Vec::new();
            for byte in 0..=text.len() {
                let offset = u32::try_from(byte).unwrap();
                if text.is_char_boundary(byte) {
                    let utf16 = utf16_prefix(&text, byte);
                    prop_assert_eq!(index.to_utf16(offset), Ok(utf16));
                    prop_assert_eq!(index.to_utf8(utf16), Ok(offset));
                    boundaries.push(utf16);
                } else {
                    prop_assert_eq!(index.to_utf16(offset), Err(OffsetError::NotCharBoundary { offset }));
                }
            }
            for unit in 0..=index.len_utf16() {
                if !boundaries.contains(&unit) {
                    prop_assert_eq!(index.to_utf8(unit), Err(OffsetError::InsideSurrogatePair { offset: unit }));
                }
            }
        }
    }
}
