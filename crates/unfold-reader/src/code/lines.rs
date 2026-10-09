//! The lines of a shown slice.
//!
//! Lines end at CR, LF, CRLF, U+2028, and U+2029, exactly as the stored line numbers count them
//! (core's `LineIndex`), so line `n` of the view is line `n` of the file. A terminator at the
//! very end of the slice (a whole file ending in a newline) does not start an extra empty line.

use unfold_core::Utf16Index;

use super::CodeLine;
use crate::index::to_u32;

/// The lines of `slice`, numbered from `first`, and the indentation common to every nonblank
/// line after the first.
pub(super) fn lines(slice: &str, first: u32, utf16: &Utf16Index) -> (Vec<CodeLine>, u32) {
    let bytes = slice.as_bytes();
    let mut lines = Vec::new();
    let mut dedent: Option<u32> = None;
    let mut push = |start: usize, end: usize| {
        let indent = to_u32(
            bytes[start..end]
                .iter()
                .take_while(|&&byte| byte == b' ' || byte == b'\t')
                .count(),
        );
        let blank = slice.get(start..end).is_none_or(|text| text.trim().is_empty());
        if !lines.is_empty() && !blank {
            dedent = Some(dedent.map_or(indent, |common| common.min(indent)));
        }
        lines.push(CodeLine {
            number: first + to_u32(lines.len()),
            start: unit(utf16, start),
            end: unit(utf16, end),
            indent,
        });
    };
    let mut start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        let terminator = match bytes[index] {
            b'\n' => 1,
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => 2,
            b'\r' => 1,
            // U+2028 and U+2029 encode as E2 80 A8 and E2 80 A9.
            0xE2 if bytes.get(index + 1) == Some(&0x80)
                && matches!(bytes.get(index + 2), Some(0xA8 | 0xA9)) =>
            {
                3
            }
            _ => 0,
        };
        if terminator == 0 {
            index += 1;
            continue;
        }
        push(start, index);
        index += terminator;
        start = index;
    }
    if start < bytes.len() || start == 0 {
        push(start, bytes.len());
    }
    (lines, dedent.unwrap_or(0))
}

/// The UTF-16 offset of a byte offset that lies on a character boundary.
fn unit(utf16: &Utf16Index, byte: usize) -> u32 {
    utf16
        .to_utf16(to_u32(byte))
        .expect("line boundaries fall next to ASCII or line-separator characters")
}
