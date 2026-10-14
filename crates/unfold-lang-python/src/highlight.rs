//! Syntax colors for the code panel, from Ruff's lexer (the parser's token stream). Highlighting
//! never parses: it colors text that does not parse as well as it can.
//!
//! The classes are the proof of concept reader's (`reader/core/python-syntax.ts`):
//!
//! | Token | Class |
//! | --- | --- |
//! | Keywords, including `True`, `False`, `None` | `keyword` |
//! | Comments | `comment` |
//! | Strings, bytes, and the literal parts of f-strings and t-strings | `string` |
//! | Numbers | `number` |
//! | A name after `class`, or starting with an uppercase letter | `type` |
//! | A name after `def` or `@`, or before `(` | `function` |
//! | A name after `.` | `property` |
//!
//! Other names, operators, and punctuation stay plain. Two refinements over the proof of
//! concept, which read text with a regular expression: soft keywords (`match`, `case`, `type`,
//! `lazy`) are keywords only where they start such a statement, and the expressions interpolated
//! into an f-string are colored as code.

use ruff_python_ast::token::TokenKind;
use ruff_text_size::TextRange;
use unfold_core::highlight::{HighlightSpan, Highlighter, TokenClass};
use unfold_core::vocab::Language;

use crate::ast::bytes;
use crate::census::lex;

/// The Python highlighter.
#[derive(Debug, Clone, Copy, Default)]
pub struct PythonHighlighter;

/// Tokens that only shape lines.
fn layout(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Newline
            | TokenKind::NonLogicalNewline
            | TokenKind::Indent
            | TokenKind::Dedent
            | TokenKind::Comment
    )
}

fn literal(kind: TokenKind) -> Option<TokenClass> {
    Some(match kind {
        TokenKind::Comment => TokenClass::Comment,
        TokenKind::String
        | TokenKind::FStringStart
        | TokenKind::FStringMiddle
        | TokenKind::FStringEnd
        | TokenKind::TStringStart
        | TokenKind::TStringMiddle
        | TokenKind::TStringEnd => TokenClass::String,
        TokenKind::Int | TokenKind::Float | TokenKind::Complex => TokenClass::Number,
        _ if kind.is_non_soft_keyword() => TokenClass::Keyword,
        _ => return None,
    })
}

impl Highlighter for PythonHighlighter {
    fn language(&self) -> Language {
        Language::Python
    }

    fn highlight(&self, text: &str) -> Vec<HighlightSpan> {
        let tokens: Vec<(TokenKind, TextRange)> = lex(text)
            .into_iter()
            .filter(|(kind, range)| *kind != TokenKind::Unknown && !range.is_empty())
            .collect();
        // Neighbors are the nearest tokens that are not layout or comments.
        let code: Vec<usize> = (0..tokens.len())
            .filter(|&index| !layout(tokens[index].0))
            .collect();
        let mut spans = Vec::new();
        let mut position = 0usize;
        for (index, &(kind, range)) in tokens.iter().enumerate() {
            let class = match literal(kind) {
                Some(class) => Some(class),
                None if kind == TokenKind::Identifier || kind.is_soft_keyword() => {
                    while position < code.len() && code[position] < index {
                        position += 1;
                    }
                    let at = |offset: isize| -> Option<TokenKind> {
                        let slot = position as isize + offset;
                        usize::try_from(slot)
                            .ok()
                            .and_then(|slot| code.get(slot))
                            .map(|&i| tokens[i].0)
                    };
                    let line_start = position == 0 || {
                        let previous = code[position - 1];
                        tokens[previous + 1..index]
                            .iter()
                            .any(|(kind, _)| *kind == TokenKind::Newline)
                            || tokens[previous].0 == TokenKind::Semi
                    };
                    if kind.is_soft_keyword() && soft_keyword(kind, line_start, &tokens, &code, position) {
                        Some(TokenClass::Keyword)
                    } else {
                        name(text, range, at(-1), at(1))
                    }
                }
                None => None,
            };
            if let Some(class) = class {
                spans.push(HighlightSpan {
                    range: bytes(range),
                    class,
                });
            }
        }
        spans
    }
}

/// The class of a name from its neighbors.
fn name(
    text: &str,
    range: TextRange,
    before: Option<TokenKind>,
    after: Option<TokenKind>,
) -> Option<TokenClass> {
    match (before, after) {
        (Some(TokenKind::Class), _) => Some(TokenClass::Type),
        (Some(TokenKind::Def | TokenKind::At), _) | (_, Some(TokenKind::Lpar)) => Some(TokenClass::Function),
        (Some(TokenKind::Dot), _) => Some(TokenClass::Property),
        _ => text
            .get(range.start().to_usize()..range.end().to_usize())
            .and_then(|word| word.chars().next())
            .filter(|first| first.is_uppercase())
            .map(|_| TokenClass::Type),
    }
}

/// Whether a soft keyword at code position `position` starts its statement: `match`/`case`
/// headers end with a colon at bracket depth zero, `type X = …` / `type X[…]` aliases, and
/// `lazy import` / `lazy from`.
fn soft_keyword(
    kind: TokenKind,
    line_start: bool,
    tokens: &[(TokenKind, TextRange)],
    code: &[usize],
    position: usize,
) -> bool {
    if !line_start {
        return false;
    }
    let next = |offset: usize| code.get(position + offset).map(|&index| tokens[index].0);
    match kind {
        TokenKind::Match | TokenKind::Case => {
            let follows_as_name = matches!(
                next(1),
                None | Some(
                    TokenKind::Equal
                        | TokenKind::Dot
                        | TokenKind::Colon
                        | TokenKind::Comma
                        | TokenKind::Rpar
                        | TokenKind::Newline
                        | TokenKind::PlusEqual
                        | TokenKind::MinusEqual
                )
            );
            !follows_as_name && header_ends_with_colon(tokens, code, position)
        }
        TokenKind::Type => {
            matches!(next(1), Some(TokenKind::Identifier))
                && matches!(next(2), Some(TokenKind::Equal | TokenKind::Lsqb))
        }
        TokenKind::Lazy => matches!(next(1), Some(TokenKind::Import | TokenKind::From)),
        _ => false,
    }
}

/// Whether the logical line from code position `position` has a colon at bracket depth zero.
fn header_ends_with_colon(tokens: &[(TokenKind, TextRange)], code: &[usize], position: usize) -> bool {
    let mut depth = 0i32;
    let start = code.get(position).copied().unwrap_or(tokens.len());
    for &(kind, _) in &tokens[start..] {
        match kind {
            TokenKind::Newline => return false,
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => depth += 1,
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => depth -= 1,
            TokenKind::Colon if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use unfold_core::highlight::spans_are_well_formed;

    use super::*;

    fn classes(text: &str) -> Vec<(&str, TokenClass)> {
        let spans = PythonHighlighter.highlight(text);
        assert!(spans_are_well_formed(text, &spans));
        spans
            .iter()
            .map(|span| {
                (
                    &text[span.range.start as usize..span.range.end as usize],
                    span.class,
                )
            })
            .collect()
    }

    #[test]
    fn colors_like_the_proof_of_concept() {
        use TokenClass::*;
        assert_eq!(
            classes("@app.get('/x')\ndef run(n=1):  # go\n    return Order(n).total\n"),
            [
                ("app", Function),
                ("get", Function),
                ("'/x'", String),
                ("def", Keyword),
                ("run", Function),
                ("1", Number),
                ("# go", Comment),
                ("return", Keyword),
                ("Order", Function),
                ("total", Property),
            ]
        );
        assert_eq!(
            classes("class Box: pass"),
            [("class", Keyword), ("Box", Type), ("pass", Keyword)]
        );
        assert_eq!(classes("x = None"), [("None", Keyword)]);
    }

    #[test]
    fn soft_keywords_are_keywords_only_in_their_statements() {
        use TokenClass::*;
        assert_eq!(
            classes("match = 1\nmatch x:\n    case 1: pass\n"),
            [
                ("1", Number),
                ("match", Keyword),
                ("case", Keyword),
                ("1", Number),
                ("pass", Keyword)
            ]
        );
        assert_eq!(classes("type X = int"), [("type", Keyword), ("X", Type)]);
    }

    #[test]
    fn broken_text_still_highlights() {
        let text = "def broken(:\n  'unterminated\n\u{1F600} = 1\n";
        let spans = PythonHighlighter.highlight(text);
        assert!(spans_are_well_formed(text, &spans));
        assert!(!spans.is_empty());
    }
}
