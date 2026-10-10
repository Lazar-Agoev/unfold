//! The lines of a shown slice.
//!
//! Lines end at CR, LF, CRLF, U+2028, and U+2029, exactly as the stored line numbers count them
//! (core's `LineIndex`), so line `n` of the view is line `n` of the file. A terminator at the
//! very end of the slice (a whole file ending in a newline) does not start an extra empty line.
//!
//! A block's slice starts at the block, not at the start of its first line, so the view shows the
//! first line from there and may hide indentation from the others. It hides only what every line
//! shares, the first counted at its line's indentation in the file: a method loses its class's
//! indentation, and a top-level function's body keeps its own.

use unfold_core::Utf16Index;

use super::CodeLine;
use crate::index::to_u32;

/// The lines of `slice`, numbered from `first`, and the indentation common to every nonblank
/// line, the first counted as `first_indent` (its indentation in the file, [`indent_at`]).
pub(super) fn lines(slice: &str, first: u32, first_indent: u32, utf16: &Utf16Index) -> (Vec<CodeLine>, u32) {
    let bytes = slice.as_bytes();
    let mut lines = Vec::new();
    let mut dedent = first_indent;
    let mut push = |start: usize, end: usize| {
        let indent = indentation(&bytes[start..end]);
        let blank = slice.get(start..end).is_none_or(|text| text.trim().is_empty());
        if !lines.is_empty() && !blank {
            dedent = dedent.min(indent);
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
    (lines, dedent)
}

/// The indentation of the line of `text` that byte `offset` (a character boundary) lies on: the
/// leading spaces and tabs of the whole line, wherever on it `offset` is.
pub(super) fn indent_at(text: &str, offset: usize) -> u32 {
    const TERMINATORS: [char; 4] = ['\n', '\r', '\u{2028}', '\u{2029}'];
    let start = text[..offset]
        .char_indices()
        .rev()
        .find(|(_, character)| TERMINATORS.contains(character))
        .map_or(0, |(index, character)| index + character.len_utf8());
    indentation(&text.as_bytes()[start..])
}

/// The number of leading spaces and tabs.
fn indentation(bytes: &[u8]) -> u32 {
    to_u32(
        bytes
            .iter()
            .take_while(|&&byte| byte == b' ' || byte == b'\t')
            .count(),
    )
}

/// The UTF-16 offset of a byte offset that lies on a character boundary.
fn unit(utf16: &Utf16Index, byte: usize) -> u32 {
    utf16
        .to_utf16(to_u32(byte))
        .expect("line boundaries fall next to ASCII or line-separator characters")
}
