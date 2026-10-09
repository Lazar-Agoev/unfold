//! Highlighted spans for a shown slice.
//!
//! The whole file is highlighted (a slice taken alone can begin inside a string or template and
//! lex wrongly), then the spans are clipped to the slice and converted to UTF-16. A highlighter
//! must return well-formed spans; any that are not (empty, overlapping, out of range, or inside a
//! UTF-8 sequence) are dropped rather than trusted.

use unfold_core::ByteRange;
use unfold_core::highlight::HighlightSpan;

use super::{CodeSpan, Offsets};

/// The spans of `highlighted` (over the whole `text`) that fall in `range`, clipped and
/// converted.
pub(super) fn spans(
    text: &str,
    range: ByteRange,
    highlighted: &[HighlightSpan],
    offsets: &Offsets<'_>,
) -> Vec<CodeSpan> {
    let mut previous_end = 0u32;
    let mut spans = Vec::new();
    for span in highlighted {
        let ByteRange { start, end } = span.range;
        let well_formed = start >= previous_end
            && start < end
            && (end as usize) <= text.len()
            && text.is_char_boundary(start as usize)
            && text.is_char_boundary(end as usize);
        if !well_formed {
            continue;
        }
        previous_end = end;
        let (start, end) = (start.max(range.start), end.min(range.end));
        if start >= end {
            continue;
        }
        if let (Ok(start), Ok(end)) = (offsets.at(start), offsets.at(end)) {
            spans.push(CodeSpan(start, end, span.class));
        }
    }
    spans
}
