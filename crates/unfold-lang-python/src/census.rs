//! The independent census validation compares Python files with: token boundaries and call
//! sites from a fresh run of the lexer (never the syntax tree or block selection), plus the
//! file's runtime module dependencies.
//!
//! The call scanner is the proof of concept's primary-chain scanner (`token-census.ts`) over
//! Ruff's tokens. It tracks, for every token that can end a primary expression, where that
//! expression starts (names, literals, closing brackets, `.member` steps, implicitly
//! concatenated strings, whole f-strings), and reports every `(` that follows one as a call
//! from the primary's start to the matching `)`. Parentheses that are not calls are excluded:
//! a `def` or `class` header's parameter or base list, a `match` subject in parentheses, and
//! everything in a `case` pattern. Starred and double-starred arguments are recorded as
//! positions that can never supply a named reference.

use ruff_python_ast::token::TokenKind;
use ruff_python_parser::{Mode, lexer};
use ruff_text_size::TextRange;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use unfold_core::text::ByteRange;

/// One call site the scanner found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSite {
    /// From the primary's first token to the closing parenthesis.
    pub range: ByteRange,
    /// The written positions of starred and double-starred arguments.
    pub ineligible: Vec<u32>,
}

/// What the lexer pass found in one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Scan {
    /// Sorted, distinct token starts (plus 0 and the text's length).
    pub(crate) starts: Vec<u32>,
    /// Sorted, distinct token ends (plus 0 and the text's length).
    pub(crate) ends: Vec<u32>,
    /// Call sites, sorted by range.
    pub(crate) calls: Vec<CallSite>,
}

/// Tokens that carry no syntax for the scanner. Logical newlines stay: they end statements.
fn ignored(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Comment
            | TokenKind::NonLogicalNewline
            | TokenKind::Indent
            | TokenKind::Dedent
            | TokenKind::EndOfFile
    )
}

fn opener(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace)
}

fn closer(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace)
}

/// Tokens that end a primary expression on their own.
fn atom(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::String
            | TokenKind::Int
            | TokenKind::Float
            | TokenKind::Complex
            | TokenKind::Ellipsis
            | TokenKind::True
            | TokenKind::False
            | TokenKind::None
            | TokenKind::Match
            | TokenKind::Case
            | TokenKind::Type
            | TokenKind::Lazy
    )
}

fn string_end(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::String | TokenKind::FStringEnd | TokenKind::TStringEnd
    )
}

/// Every token of `text` from a fresh lexer run (comments and layout included, the end of
/// file excluded). The lexer recovers from errors, so this never fails.
pub(crate) fn lex(text: &str) -> Vec<(TokenKind, TextRange)> {
    let mut tokens = Vec::new();
    let mut lexer = lexer::lex(text, Mode::Module);
    // Every token but a dedent is nonempty, so the count is bounded; the guard only protects
    // against a lexer that stops advancing.
    let limit = text.len().saturating_mul(2).saturating_add(64);
    loop {
        let kind = lexer.next_token();
        if kind == TokenKind::EndOfFile || tokens.len() > limit {
            break;
        }
        tokens.push((kind, lexer.current_range()));
    }
    tokens
}

/// Lexes `text` once and scans it.
pub(crate) fn scan(text: &str) -> Scan {
    let tokens: Vec<(TokenKind, TextRange)> = lex(text)
        .into_iter()
        .filter(|(kind, _)| !ignored(*kind))
        .collect();
    let len = u32::try_from(text.len()).unwrap_or(u32::MAX);
    let mut starts = vec![0, len];
    let mut ends = vec![0, len];
    for &(kind, range) in &tokens {
        if kind != TokenKind::Newline && kind != TokenKind::Unknown && !range.is_empty() {
            starts.push(range.start().to_u32());
            ends.push(range.end().to_u32());
        }
    }
    starts.sort_unstable();
    starts.dedup();
    ends.sort_unstable();
    ends.dedup();
    Scan {
        starts,
        ends,
        calls: calls(&tokens),
    }
}

/// The `(` tokens that are not calls: `def`/`class` headers and parenthesized `match`
/// subjects; and the tokens of `case` patterns.
fn exclusions(tokens: &[(TokenKind, TextRange)]) -> (FxHashSet<usize>, FxHashSet<usize>) {
    let kind = |index: usize| tokens.get(index).map(|token| token.0);
    let mut headers = FxHashSet::default();
    let mut patterns = FxHashSet::default();
    let mut statement_start = true;
    for (index, &(token, _)) in tokens.iter().enumerate() {
        if matches!(token, TokenKind::Newline | TokenKind::Semi) {
            statement_start = true;
            continue;
        }
        if matches!(token, TokenKind::Def | TokenKind::Class) {
            let mut cursor = index + 2;
            if kind(cursor) == Some(TokenKind::Lsqb) {
                let mut depth = 1i32;
                while depth > 0 && cursor + 1 < tokens.len() {
                    cursor += 1;
                    match tokens[cursor].0 {
                        TokenKind::Lsqb => depth += 1,
                        TokenKind::Rsqb => depth -= 1,
                        _ => {}
                    }
                }
                cursor += 1;
            }
            if kind(cursor) == Some(TokenKind::Lpar) {
                headers.insert(cursor);
            }
        }
        if statement_start && matches!(token, TokenKind::Match | TokenKind::Case) {
            let mut depth = 0i32;
            let mut stop = None;
            let mut guard = None;
            for (cursor, &(next, _)) in tokens.iter().enumerate().skip(index + 1) {
                if next == TokenKind::Newline {
                    break;
                }
                if depth == 0 && next == TokenKind::Colon {
                    stop = Some(cursor);
                    break;
                }
                if depth == 0 && next == TokenKind::If && guard.is_none() {
                    guard = Some(cursor);
                }
                if opener(next) {
                    depth += 1;
                } else if closer(next) {
                    depth -= 1;
                }
            }
            if let Some(stop) = stop {
                if token == TokenKind::Case {
                    patterns.extend(index + 1..guard.unwrap_or(stop));
                } else if kind(index + 1) == Some(TokenKind::Lpar) {
                    headers.insert(index + 1);
                }
            }
        }
        statement_start = false;
    }
    (headers, patterns)
}

/// An open bracket during the scan.
struct Open {
    start: u32,
    invocation: bool,
    /// The written position of the argument being read.
    position: u32,
    /// Whether no token of the current argument has been read yet.
    at_argument_start: bool,
    /// Unclosed `lambda` parameter lists directly inside, whose commas separate parameters.
    lambdas: u32,
    ineligible: Vec<u32>,
}

fn calls(tokens: &[(TokenKind, TextRange)]) -> Vec<CallSite> {
    let (headers, patterns) = exclusions(tokens);
    let mut starts: Vec<Option<u32>> = vec![None; tokens.len()];
    let mut stack: Vec<Open> = Vec::new();
    let mut formatted: Vec<u32> = Vec::new();
    let mut calls = Vec::new();
    for (index, &(kind, range)) in tokens.iter().enumerate() {
        let previous = index.checked_sub(1).map(|before| tokens[before].0);
        let prior = index.checked_sub(1).and_then(|before| starts[before]);
        if let Some(open) = stack.last_mut().filter(|open| open.invocation) {
            match kind {
                TokenKind::Comma if open.lambdas == 0 => {
                    open.position += 1;
                    open.at_argument_start = true;
                }
                TokenKind::Lambda => {
                    open.lambdas += 1;
                    open.at_argument_start = false;
                }
                TokenKind::Colon if open.lambdas > 0 => open.lambdas -= 1,
                TokenKind::Star | TokenKind::DoubleStar if open.at_argument_start => {
                    open.ineligible.push(open.position);
                    open.at_argument_start = false;
                }
                _ if closer(kind) => {}
                _ => open.at_argument_start = false,
            }
        }
        let start = range.start().to_u32();
        match kind {
            TokenKind::FStringStart | TokenKind::TStringStart => {
                let concatenated = previous.is_some_and(string_end);
                formatted.push(prior.filter(|_| concatenated).unwrap_or(start));
            }
            TokenKind::FStringEnd | TokenKind::TStringEnd => starts[index] = formatted.pop(),
            _ if opener(kind) => {
                let excluded = headers.contains(&index);
                let invocation =
                    kind == TokenKind::Lpar && prior.is_some() && !excluded && !patterns.contains(&index);
                let chained = matches!(kind, TokenKind::Lpar | TokenKind::Lsqb) && !excluded;
                stack.push(Open {
                    start: prior.filter(|_| chained).unwrap_or(start),
                    invocation,
                    position: 0,
                    at_argument_start: true,
                    lambdas: 0,
                    ineligible: Vec::new(),
                });
            }
            _ if closer(kind) => {
                if let Some(open) = stack.pop() {
                    starts[index] = Some(open.start);
                    if open.invocation {
                        calls.push(CallSite {
                            range: ByteRange {
                                start: open.start,
                                end: range.end().to_u32(),
                            },
                            ineligible: open.ineligible,
                        });
                    }
                }
            }
            _ if atom(kind) => {
                let mut begin = start;
                if previous == Some(TokenKind::Dot) && index > 1 {
                    begin = starts[index - 2].unwrap_or(begin);
                } else if kind == TokenKind::String && previous.is_some_and(string_end) {
                    begin = prior.unwrap_or(begin);
                }
                starts[index] = Some(begin);
            }
            _ => {}
        }
    }
    calls.sort_by_key(|call| (call.range.start, call.range.end));
    calls
}

/// Encodes sorted offsets as gaps, which postcard stores in a byte or two each.
pub(crate) fn delta_encode(sorted: &[u32]) -> Vec<u32> {
    let mut previous = 0;
    sorted
        .iter()
        .map(|&offset| {
            let gap = offset.wrapping_sub(previous);
            previous = offset;
            gap
        })
        .collect()
}

/// Decodes [`delta_encode`]'s output.
pub(crate) fn delta_decode(gaps: &[u32]) -> Vec<u32> {
    let mut offset = 0u32;
    gaps.iter()
        .map(|&gap| {
            offset = offset.wrapping_add(gap);
            offset
        })
        .collect()
}

/// The byte range of a census call, for comparisons in tests.
#[cfg(test)]
pub(crate) fn call_ranges(text: &str) -> Vec<(u32, u32)> {
    scan(text)
        .calls
        .iter()
        .map(|call| (call.range.start, call.range.end))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(text: &str) -> Vec<&str> {
        call_ranges(text)
            .into_iter()
            .map(|(start, end)| &text[start as usize..end as usize])
            .collect()
    }

    #[test]
    fn finds_primary_chain_calls() {
        assert_eq!(
            texts("a.b(c)(d)\n(f)(x)\nx[0](1)\n\"s\".join(y)\nf\"{g(1)}\".upper()\n"),
            [
                "a.b(c)",
                "a.b(c)(d)",
                "(f)(x)",
                "x[0](1)",
                "\"s\".join(y)",
                "f\"{g(1)}\".upper()",
                "g(1)"
            ]
        );
    }

    #[test]
    fn headers_and_patterns_are_not_calls() {
        let text = "@d(1)\ndef f[T](x=g()): pass\nclass C(B, m=h()): pass\nmatch (v):\n    case P(x=1) if ok(): pass\nmatch(v)\n";
        assert_eq!(texts(text), ["d(1)", "g()", "h()", "ok()", "match(v)"]);
    }

    #[test]
    fn expansions_are_ineligible_by_written_position() {
        let scanned = scan("f(a, *b, k=lambda x, y: x, **c, d=(1, 2))\n");
        assert_eq!(scanned.calls.len(), 1);
        assert_eq!(scanned.calls[0].ineligible, [1, 3]);
    }

    #[test]
    fn boundaries_skip_layout_and_comments() {
        let scanned = scan("x = 1  # note\n");
        assert_eq!(scanned.starts, [0, 2, 4, 14]);
        assert_eq!(scanned.ends, [0, 1, 3, 5, 14]);
        assert_eq!(delta_decode(&delta_encode(&scanned.ends)), scanned.ends);
    }
}
