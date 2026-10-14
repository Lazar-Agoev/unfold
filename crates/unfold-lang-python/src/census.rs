//! The independent census validation compares Python files with: call sites from a fresh run of
//! the lexer (never the syntax tree or lowering).
//!
//! The call scanner is the proof of concept's primary-chain scanner (`token-census.ts`) over
//! Ruff's tokens. It tracks, for every token that can end a primary expression, where that
//! expression starts (names, literals, closing brackets, `.member` steps, implicitly
//! concatenated strings, whole f-strings), and reports every `(` that follows one as a call
//! from the primary's start to the matching `)`. Parentheses that are not calls are excluded:
//! a `def` or `class` header's parameter or base list, a `match` subject in parentheses, and
//! everything in a `case` pattern. Every decorator is a call too: one written as a call is
//! already a site, and any other (`@property`, `@app.task`) is one from its first token to its
//! last, without the parentheses that enclose all of it.

use ruff_python_ast::token::TokenKind;
use ruff_python_parser::{Mode, lexer};
use ruff_text_size::TextRange;
use rustc_hash::FxHashSet;
use unfold_core::text::ByteRange;

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

/// Lexes `text` once and finds its call sites, sorted.
pub(crate) fn calls(text: &str) -> Vec<ByteRange> {
    let tokens: Vec<(TokenKind, TextRange)> = lex(text)
        .into_iter()
        .filter(|(kind, _)| !ignored(*kind))
        .collect();
    call_sites(&tokens)
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
}

fn call_sites(tokens: &[(TokenKind, TextRange)]) -> Vec<ByteRange> {
    let (headers, patterns) = exclusions(tokens);
    let mut starts: Vec<Option<u32>> = vec![None; tokens.len()];
    let mut stack: Vec<Open> = Vec::new();
    let mut formatted: Vec<u32> = Vec::new();
    let mut calls = Vec::new();
    for (index, &(kind, range)) in tokens.iter().enumerate() {
        let previous = index.checked_sub(1).map(|before| tokens[before].0);
        let prior = index.checked_sub(1).and_then(|before| starts[before]);
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
                });
            }
            _ if closer(kind) => {
                if let Some(open) = stack.pop() {
                    starts[index] = Some(open.start);
                    if open.invocation {
                        calls.push(ByteRange {
                            start: open.start,
                            end: range.end().to_u32(),
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
    calls.sort_unstable();
    let named: Vec<ByteRange> = decorators(tokens)
        .into_iter()
        .filter(|decorator| calls.binary_search(decorator).is_err())
        .collect();
    calls.extend(named);
    calls.sort_unstable();
    calls
}

/// The expression of every decorator: from the token after an `@` that starts a statement to
/// the last token before the line ends, without parentheses that enclose all of it.
fn decorators(tokens: &[(TokenKind, TextRange)]) -> Vec<ByteRange> {
    let mut found = Vec::new();
    let mut statement_start = true;
    for (index, &(kind, _)) in tokens.iter().enumerate() {
        if statement_start && kind == TokenKind::At {
            let end = tokens[index + 1..]
                .iter()
                .position(|&(next, _)| next == TokenKind::Newline)
                .map_or(tokens.len(), |offset| index + 1 + offset);
            let mut expression = &tokens[(index + 1).min(end)..end];
            while let [(TokenKind::Lpar, _), inner @ .., (TokenKind::Rpar, _)] = expression
                && encloses(expression)
            {
                expression = inner;
            }
            if let (Some(first), Some(last)) = (expression.first(), expression.last()) {
                found.push(ByteRange {
                    start: first.1.start().to_u32(),
                    end: last.1.end().to_u32(),
                });
            }
        }
        statement_start = matches!(kind, TokenKind::Newline | TokenKind::Semi);
    }
    found
}

/// Whether the opening parenthesis of `tokens` is closed by its last token.
fn encloses(tokens: &[(TokenKind, TextRange)]) -> bool {
    let mut depth = 0u32;
    for (index, &(kind, _)) in tokens.iter().enumerate() {
        if opener(kind) {
            depth += 1;
        } else if closer(kind) {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return index + 1 == tokens.len();
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(text: &str) -> Vec<&str> {
        calls(text)
            .into_iter()
            .map(|range| &text[range.as_usize()])
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
    fn every_decorator_is_a_call() {
        let text = "@property\n@app.task(queue=\"q\")\n@(handlers[0])\n@((f))(1)\ndef g(): pass\nx = a @ b\n";
        assert_eq!(
            texts(text),
            ["property", "app.task(queue=\"q\")", "handlers[0]", "((f))(1)"]
        );
    }
}
