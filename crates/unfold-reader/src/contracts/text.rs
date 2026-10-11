//! Type text: a written type with one span per type name inside it.
//!
//! The stored text is the written source of the type's range (cut to 120 UTF-16 units), so a
//! link's byte offset relative to the range start is its byte offset in the text, and its
//! UTF-16 offset is the UTF-16 length of the text before it. No source is read. A span the cut
//! text doesn't hold whole (or that doesn't fall on character boundaries, which only a damaged
//! cache could produce) is left out.

use unfold_core::text::utf16_len;
use unfold_core::vocab::{Reason, Resolution};
use unfold_core::{Key, ModuleLabel, TypeLink, WrittenType};

use super::{TypeSpan, TypeStyle, TypeText};
use crate::index::to_u32;
use crate::notes::note;

/// The external module labels that name the languages' built-in types.
const BUILTIN_MODULES: [&str; 3] = ["global", "python:builtins", "python:typing"];

/// A span from `start` to `end` for an outcome: `declared` with its target when resolved,
/// `builtin` or `external` with the module when external, `unresolved` with the reason in words.
/// `None` for a resolved outcome without a target (a damaged cache).
pub(super) fn outcome_span(
    start: u32,
    end: u32,
    resolution: Resolution,
    target: Option<&Key>,
    reason: Option<Reason>,
    module: Option<&ModuleLabel>,
) -> Option<TypeSpan> {
    let style = match resolution {
        Resolution::Resolved if target.is_none() => return None,
        Resolution::Resolved => TypeStyle::Declared,
        Resolution::External if module.is_some_and(|module| BUILTIN_MODULES.contains(&module.as_str())) => {
            TypeStyle::Builtin
        }
        Resolution::External => TypeStyle::External,
        Resolution::Unresolved => TypeStyle::Unresolved,
    };
    Some(TypeSpan {
        start,
        end,
        style,
        target: target.filter(|_| style == TypeStyle::Declared).cloned(),
        module: module
            .filter(|_| matches!(style, TypeStyle::External | TypeStyle::Builtin))
            .cloned(),
        note: match style {
            TypeStyle::External | TypeStyle::Unresolved => note(resolution, reason, module),
            TypeStyle::Declared | TypeStyle::Builtin => None,
        },
    })
}

/// `written` with a span for each of `links` (the type links inside its range, in order).
pub(super) fn type_text<'a>(written: &WrittenType, links: impl Iterator<Item = &'a TypeLink>) -> TypeText {
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
        if let Some(span) = outcome_span(
            units,
            units + length,
            link.resolution,
            link.target.as_ref(),
            link.reason,
            link.module.as_ref(),
        ) {
            spans.push(span);
            previous_end = end;
        }
    }
    TypeText {
        text: text.to_owned(),
        spans,
    }
}

/// `text` as one name with one span over all of it (a constructed or thrown class's name).
pub(super) fn whole_text(text: &str, span: impl FnOnce(u32) -> Option<TypeSpan>) -> TypeText {
    TypeText {
        text: text.to_owned(),
        spans: span(to_u32(utf16_len(text)))
            .filter(|span| span.end > 0)
            .into_iter()
            .collect(),
    }
}
