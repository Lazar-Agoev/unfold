//! Byte ranges from oxc spans, and the tokens around them.

use oxc_parser::{Kind, Token};
use oxc_span::Span;
use unfold_core::text::ByteRange;

/// A span as a byte range.
pub(crate) const fn range(span: Span) -> ByteRange {
    ByteRange {
        start: span.start,
        end: span.end,
    }
}

/// One file's tokens, in source order.
#[derive(Clone, Copy)]
pub(crate) struct Tokens<'t>(pub(crate) &'t [Token]);

impl<'t> Tokens<'t> {
    /// The last token ending at or before `offset`.
    pub(crate) fn before(self, offset: u32) -> Option<&'t Token> {
        let index = self.0.partition_point(|token| token.end() <= offset);
        index.checked_sub(1).and_then(|index| self.0.get(index))
    }

    /// The first token starting at or after `offset`.
    pub(crate) fn from(self, offset: u32) -> Option<&'t Token> {
        let index = self.0.partition_point(|token| token.start() < offset);
        self.0.get(index).filter(|token| token.kind() != Kind::Eof)
    }

    /// The first token of `kind` starting at or after `offset`.
    pub(crate) fn find(self, offset: u32, kind: Kind) -> Option<&'t Token> {
        let index = self.0.partition_point(|token| token.start() < offset);
        self.0[index..].iter().find(|token| token.kind() == kind)
    }

    /// The last token of `kind` ending at or before `offset`, searching back no further than
    /// `floor`.
    pub(crate) fn find_back(self, offset: u32, floor: u32, kind: Kind) -> Option<&'t Token> {
        let index = self.0.partition_point(|token| token.end() <= offset);
        self.0[..index]
            .iter()
            .rev()
            .take_while(|token| token.start() >= floor)
            .find(|token| token.kind() == kind)
    }
}
