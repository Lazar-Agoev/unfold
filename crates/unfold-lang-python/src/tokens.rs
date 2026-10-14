//! Token lookups over the parser's token stream, for the ranges the syntax tree doesn't record:
//! where a header ends (the token before the colon that opens a suite), the keyword that opens
//! an `else` or `finally` suite, where a decorated definition's own keyword starts, the `else`
//! of a conditional expression, and the written extent of a `from` import's module path.

use ruff_python_ast::token::{Token, TokenKind};
use ruff_text_size::{Ranged, TextRange, TextSize};

/// The parser's tokens with the lookups lowering needs. Every lookup is a binary search.
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

    /// The nearest token that is not layout and starts before `offset`.
    pub(crate) fn before(&self, offset: TextSize) -> Option<&'a Token> {
        self.tokens[..self.first_at(offset)]
            .iter()
            .rev()
            .find(|token| !is_layout(token.kind()))
    }

    /// The nearest token that is not layout and starts at or after `offset`.
    pub(crate) fn after(&self, offset: TextSize) -> Option<&'a Token> {
        self.significant_from(offset).next()
    }

    /// The last token of `kind` that starts before `offset`.
    pub(crate) fn last_before(&self, kind: TokenKind, offset: TextSize) -> Option<&'a Token> {
        self.tokens[..self.first_at(offset)]
            .iter()
            .rev()
            .find(|token| token.kind() == kind)
    }

    /// The end of a compound statement's header or clause whose suite starts at `suite`: the end
    /// of the last token before the suite's colon.
    pub(crate) fn header_end(&self, suite: TextSize) -> Option<TextSize> {
        let colon = self
            .before(suite)
            .filter(|token| token.kind() == TokenKind::Colon)?;
        Some(self.before(colon.start())?.end())
    }

    /// The start of the keyword (`else`, `finally`) that opens the suite starting at `suite`.
    pub(crate) fn clause_start(&self, suite: TextSize) -> Option<TextSize> {
        let colon = self
            .before(suite)
            .filter(|token| token.kind() == TokenKind::Colon)?;
        Some(self.before(colon.start())?.start())
    }

    /// The tokens from the first one starting at or after `start`, skipping layout.
    fn significant_from(&self, start: TextSize) -> impl Iterator<Item = &'a Token> {
        self.tokens[self.first_at(start)..]
            .iter()
            .filter(|token| !is_layout(token.kind()))
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
