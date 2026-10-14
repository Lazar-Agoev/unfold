//! Type text: a written type with one span per type name inside it.
//!
//! The stored text is the written source of the type's range (cut to 120 UTF-16 units), so a
//! `types` or `extends` link's byte offset relative to the range start is its byte offset in the
//! text, and its UTF-16 offset is the UTF-16 length of the text before it. No source is read. A
//! span the cut text doesn't hold whole (or that doesn't fall on character boundaries, which
//! only a damaged cache could produce) is left out.

use unfold_core::text::utf16_len;
use unfold_core::{Link, LinkTarget, WrittenType};

use super::{TypeSpan, TypeText};
use crate::notes::LinkEnd;
use crate::tree::to_u32;

/// A span from `start` to `end` for a name that reaches `target`.
fn span(start: u32, end: u32, target: &LinkTarget) -> TypeSpan {
    TypeSpan {
        start,
        end,
        to: LinkEnd::of(target),
    }
}

/// `written` with a span for each of `links` (the type links inside its range, in order).
pub(super) fn type_text<'a>(written: &WrittenType, links: impl Iterator<Item = &'a Link>) -> TypeText {
    let text = &*written.text;
    let mut spans = Vec::new();
    let mut previous_end = 0usize;
    for link in links {
        let (Some(start), Some(end)) = (
            link.range.start.checked_sub(written.range.start),
            link.range.end.checked_sub(written.range.start),
        ) else {
            continue;
        };
        let (start, end) = (start as usize, end as usize);
        let whole = start >= previous_end
            && start < end
            && end <= text.len()
            && text.is_char_boundary(start)
            && text.is_char_boundary(end);
        if !whole {
            continue;
        }
        let units = to_u32(utf16_len(&text[..start]));
        let length = to_u32(utf16_len(&text[start..end]));
        spans.push(span(units, units + length, &link.to));
        previous_end = end;
    }
    TypeText {
        text: text.to_owned(),
        spans,
    }
}

/// `text` as one name with one span over all of it, reaching `target`: a constructed or raised
/// class, or a base.
pub(super) fn whole_text(text: &str, target: &LinkTarget) -> TypeText {
    let end = to_u32(utf16_len(text));
    TypeText {
        text: text.to_owned(),
        spans: (end > 0).then(|| span(0, end, target)).into_iter().collect(),
    }
}
