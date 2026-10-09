//! One-based line numbers for byte offsets.

use super::range::{ByteRange, LineSpan};

/// The byte offset at which each line starts.
///
/// CR, LF, CRLF, U+2028, and U+2029 end a line, and CRLF counts once. Line `n` (one-based)
/// starts at `starts[n - 1]`; the first line always starts at 0, so an empty text has one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIndex {
    starts: Vec<u32>,
    len: u32,
}

impl LineIndex {
    /// Indexes `text`. Texts longer than `u32::MAX` bytes are rejected earlier, by
    /// [`Snapshot`](super::Snapshot); offsets past `u32::MAX` saturate here.
    pub fn new(text: &str) -> Self {
        let bytes = text.as_bytes();
        let mut starts = vec![0];
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\n' => starts.push(to_u32(index + 1)),
                b'\r' => {
                    if bytes.get(index + 1) == Some(&b'\n') {
                        index += 1;
                    }
                    starts.push(to_u32(index + 1));
                }
                // U+2028 and U+2029 encode as E2 80 A8 and E2 80 A9.
                0xE2 if bytes.get(index + 1) == Some(&0x80)
                    && matches!(bytes.get(index + 2), Some(0xA8 | 0xA9)) =>
                {
                    index += 2;
                    starts.push(to_u32(index + 1));
                }
                _ => {}
            }
            index += 1;
        }
        Self {
            starts,
            len: to_u32(bytes.len()),
        }
    }

    /// The number of lines (at least one).
    pub fn line_count(&self) -> u32 {
        to_u32(self.starts.len())
    }

    /// The text's length in bytes.
    pub fn len(&self) -> u32 {
        self.len
    }

    /// Whether the text is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The one-based line containing byte `offset`. Offsets at or past the end belong to the
    /// last line; a line terminator belongs to the line it ends.
    pub fn line_of(&self, offset: u32) -> u32 {
        // The number of line starts at or before `offset` is the line number.
        to_u32(self.starts.partition_point(|&start| start <= offset))
    }

    /// The byte offset where one-based `line` starts, if the line exists.
    pub fn line_start(&self, line: u32) -> Option<u32> {
        let index = usize::try_from(line.checked_sub(1)?).ok()?;
        self.starts.get(index).copied()
    }

    /// The lines `range` starts and ends on: the start's line, and the line of the last byte
    /// (the start's line for an empty range).
    pub fn span(&self, range: ByteRange) -> LineSpan {
        let start = self.line_of(range.start);
        let end = if range.end > range.start {
            self.line_of(range.end - 1)
        } else {
            start
        };
        LineSpan { start, end }
    }
}

fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// A deliberately simple reference: one plus the number of terminators that end at or
    /// before `offset`, where a CRLF is one two-byte terminator.
    fn naive_line_of(text: &str, offset: usize) -> u32 {
        let mut ends = Vec::new();
        let mut characters = text.char_indices().peekable();
        while let Some((index, character)) = characters.next() {
            match character {
                '\r' if characters.peek().map(|&(_, next)| next) == Some('\n') => {
                    characters.next();
                    ends.push(index + 2);
                }
                '\r' | '\n' | '\u{2028}' | '\u{2029}' => ends.push(index + character.len_utf8()),
                _ => {}
            }
        }
        1 + u32::try_from(ends.iter().filter(|&&end| end <= offset).count()).unwrap()
    }

    #[test]
    fn handles_every_terminator() {
        let text = "a\nb\r\nc\rd\u{2028}e\u{2029}f";
        let index = LineIndex::new(text);
        assert_eq!(index.line_count(), 6);
        let line_of = |needle: char| index.line_of(u32::try_from(text.find(needle).unwrap()).unwrap());
        assert_eq!(
            [
                line_of('a'),
                line_of('b'),
                line_of('c'),
                line_of('d'),
                line_of('e'),
                line_of('f')
            ],
            [1, 2, 3, 4, 5, 6]
        );
        // The LF of a CRLF belongs to the line the CR ends.
        assert_eq!(index.line_of(4), 2);
        assert_eq!(index.line_start(3), Some(5));
        assert_eq!(index.line_start(7), None);
        assert_eq!(index.line_start(0), None);
    }

    #[test]
    fn empty_and_trailing_newline() {
        let empty = LineIndex::new("");
        assert_eq!(empty.line_count(), 1);
        assert_eq!(
            empty.span(ByteRange::new(0, 0).unwrap()),
            LineSpan { start: 1, end: 1 }
        );
        let trailing = LineIndex::new("x\n");
        assert_eq!(trailing.line_count(), 2);
        // A whole-file range ending in a newline ends on the newline's line.
        assert_eq!(
            trailing.span(ByteRange::new(0, 2).unwrap()),
            LineSpan { start: 1, end: 1 }
        );
    }

    #[test]
    fn bom_and_astral_characters() {
        let text = "\u{feff}a\n😀b\nc";
        let index = LineIndex::new(text);
        assert_eq!(index.line_of(0), 1);
        assert_eq!(index.line_of(3), 1);
        assert_eq!(index.line_of(5), 2);
        assert_eq!(index.line_of(10), 2);
        assert_eq!(index.line_of(11), 3);
    }

    proptest! {
        #[test]
        fn agrees_with_a_naive_splitter(
            text in proptest::string::string_regex("([a-c\r\n\u{2028}\u{2029}é😀]){0,40}").unwrap()
        ) {
            let index = LineIndex::new(&text);
            for (offset, _) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
                prop_assert_eq!(index.line_of(u32::try_from(offset).unwrap()), naive_line_of(&text, offset));
            }
        }
    }
}
