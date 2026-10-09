//! Token lookups over the parser's token stream: the parentheses Pyright-style ranges include,
//! the colon that opens a suite, and the written extent of an import's module path.
//!
//! Ruff's expression ranges exclude the parentheses around an expression, while the proof of
//! concept's grammar (Pyright) widened a parenthesized expression's range over them. Unfold keeps
//! the proof of concept's ranges, so slots, names, and labels widen over enclosing parentheses
//! with [`Tokens::widen`].

use ruff_python_ast::token::{Token, TokenKind};
use ruff_text_size::{Ranged, TextRange, TextSize};

/// The parser's tokens with the lookups structure needs. Every lookup is a binary search.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Tokens<'a> {
    tokens: &'a [Token],
}

/// Whether a token carries no syntax: comments, line structure, and indentation.
fn is_layout(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Comment
            | TokenKind::NonLogicalNewline
            | TokenKind::Newline
            | TokenKind::Indent
            | TokenKind::Dedent
            | TokenKind::EndOfFile
    )
}

impl<'a> Tokens<'a> {
    pub(crate) fn new(tokens: &'a [Token]) -> Self {
        Self { tokens }
    }

    /// The index of the first token starting at or after `offset`.
    fn first_at(&self, offset: TextSize) -> usize {
        self.tokens.partition_point(|token| token.start() < offset)
    }

    /// The nearest token before index `index` that is not layout.
    fn previous(&self, index: usize) -> Option<&'a Token> {
        self.tokens[..index]
            .iter()
            .rev()
            .find(|token| !is_layout(token.kind()))
    }

    /// The nearest token at or after index `index` that is not layout.
    fn next(&self, index: usize) -> Option<&'a Token> {
        self.tokens[index..].iter().find(|token| !is_layout(token.kind()))
    }

    /// `range` widened over every pair of parentheses that encloses exactly it and lies inside
    /// `bound`. The bound keeps the parentheses that belong to the parent's own syntax (a call's
    /// argument list, a parenthesized `with` list) from being taken for the expression's.
    pub(crate) fn widen(&self, range: TextRange, bound: TextRange) -> TextRange {
        let mut range = range;
        loop {
            let open = self.previous(self.first_at(range.start()));
            let close = self.next(self.first_at(range.end()));
            match (open, close) {
                (Some(open), Some(close))
                    if open.kind() == TokenKind::Lpar
                        && close.kind() == TokenKind::Rpar
                        && open.start() >= bound.start()
                        && close.end() <= bound.end() =>
                {
                    range = TextRange::new(open.start(), close.end());
                }
                _ => return range,
            }
        }
    }

    /// The written range of a suite: from the colon that opens it to the end of its last
    /// statement (`None` for an empty suite, which valid code never has).
    pub(crate) fn suite(&self, first: TextSize, last: TextSize) -> Option<TextRange> {
        let colon = self.previous(self.first_at(first))?;
        (colon.kind() == TokenKind::Colon).then(|| TextRange::new(colon.start(), last))
    }

    /// The tokens from the first one starting at or after `start`, skipping layout.
    pub(crate) fn significant_from(&self, start: TextSize) -> impl Iterator<Item = &'a Token> {
        self.tokens[self.first_at(start)..]
            .iter()
            .filter(|token| !is_layout(token.kind()))
    }

    /// The range of the parentheses of a parenthesized `with` item list, if `with_start` (the
    /// statement's first token) opens one: `with (a, b as c):`. Like Pyright, a parenthesis is the
    /// list's when its match is directly followed by the colon.
    pub(crate) fn with_list_parens(&self, keyword: TextSize) -> Option<TextRange> {
        let mut tokens = self.significant_from(keyword);
        let mut first = tokens.next()?;
        if first.kind() == TokenKind::Async {
            first = tokens.next()?;
        }
        if first.kind() != TokenKind::With {
            return None;
        }
        let open = tokens.next().filter(|token| token.kind() == TokenKind::Lpar)?;
        let mut depth = 0u32;
        for token in tokens.by_ref() {
            match token.kind() {
                TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => depth += 1,
                TokenKind::Rsqb | TokenKind::Rbrace => depth = depth.saturating_sub(1),
                TokenKind::Rpar if depth > 0 => depth -= 1,
                TokenKind::Rpar => {
                    let colon = tokens.next()?;
                    return (colon.kind() == TokenKind::Colon)
                        .then(|| TextRange::new(open.start(), token.end()));
                }
                _ => {}
            }
        }
        None
    }

    /// The written module path of `from <dots><module> import …`: from the first dot (or the
    /// module) to the module's end, or to the last dot when there is no module.
    pub(crate) fn import_module_path(
        &self,
        statement: TextSize,
        module_end: Option<TextSize>,
    ) -> Option<TextRange> {
        let mut tokens = self.significant_from(statement);
        let from = tokens.next().filter(|token| token.kind() == TokenKind::From)?;
        let first = tokens.next()?;
        if let Some(end) = module_end {
            return Some(TextRange::new(first.start(), end));
        }
        let mut end = first.end();
        for token in std::iter::once(first).chain(tokens) {
            match token.kind() {
                TokenKind::Dot | TokenKind::Ellipsis => end = token.end(),
                _ => break,
            }
        }
        (end > from.end()).then(|| TextRange::new(first.start(), end))
    }
}
