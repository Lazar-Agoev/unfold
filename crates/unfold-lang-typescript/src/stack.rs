//! A thread with a stack large enough for oxc's parser.
//!
//! oxc parses recursively and has no depth limit of its own, so deeply nested input could
//! overflow a thread's stack (an abort, not a recoverable panic). Each parse therefore runs on a
//! scoped thread whose stack is sized from the text before parsing:
//!
//! - Every level of parser recursion consumes at least one token, and measured oxc frames stay
//!   under [`STACK_PER_TOKEN`] per level, so a stack of [`BASE_STACK`] plus that much per
//!   possible token is always enough. Stack memory is reserved, not committed, until used.
//! - When that bound would exceed [`MAX_STACK`] (files of roughly a megabyte), the text's nesting
//!   is estimated lexically instead ([`estimate_nesting`]), and text nested deeper than
//!   [`MAX_NESTING`] fails without being parsed. The largest stack then covers that nesting many
//!   times over.

use crate::scan::{Lexed, scan};

/// The stack every parse thread starts with; it also holds the depth-guarded conversion.
const BASE_STACK: usize = 64 << 20;
/// An upper bound on the stack one level of oxc's recursion uses.
const STACK_PER_TOKEN: usize = 4 << 10;
/// The largest stack reserved for one parse.
const MAX_STACK: usize = 1 << 30;
/// The deepest estimated nesting parsed when the token bound is too large.
pub(crate) const MAX_NESTING: u32 = 4_000;

/// Why text was not parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StackError {
    /// The text nests more deeply than [`MAX_NESTING`].
    TooDeep,
    /// The parse thread could not start, or panicked.
    Thread,
}

/// Runs `work` (which parses `text`) on a thread with a large enough stack.
pub(crate) fn on_parse_stack<T: Send>(text: &str, work: impl FnOnce() -> T + Send) -> Result<T, StackError> {
    let stack = stack_for(text)?;
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("unfold-typescript-parse".into())
            .stack_size(stack)
            .spawn_scoped(scope, work)
            .map_err(|_| StackError::Thread)?
            .join()
            .map_err(|_| StackError::Thread)
    })
}

/// The stack to parse `text` with.
fn stack_for(text: &str) -> Result<usize, StackError> {
    let tokens = possible_tokens(text);
    let bound = BASE_STACK.saturating_add(tokens.saturating_mul(STACK_PER_TOKEN));
    if bound <= MAX_STACK {
        return Ok(bound);
    }
    if estimate_nesting(text) > MAX_NESTING {
        return Err(StackError::TooDeep);
    }
    Ok(MAX_STACK)
}

/// An upper bound on the number of tokens in `text`: every punctuation byte, plus every run of
/// word bytes. It ignores lexical context, so it can only overcount.
fn possible_tokens(text: &str) -> usize {
    let mut count = 0;
    let mut in_word = false;
    for byte in text.bytes() {
        let word = byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte >= 0x80;
        if word {
            count += usize::from(!in_word);
        } else if !byte.is_ascii_whitespace() {
            count += 1;
        }
        in_word = word;
    }
    count
}

/// Words that can open a level of parser recursion.
const NESTING_WORDS: &[&str] = &[
    "new", "typeof", "void", "delete", "await", "yield", "keyof", "readonly", "unique", "infer", "if", "for",
    "while", "do", "else", "with", "class", "extends", "function", "async",
];

/// Estimates how deeply `text` nests: brackets and template substitutions, plus, within each
/// bracket level, the run of operators and keywords that recurse (assignments, conditionals,
/// unary and arrow operators, statement keywords, ...) since the last `,` or `;`, which always
/// return the parser to that level.
pub(crate) fn estimate_nesting(text: &str) -> u32 {
    let mut levels: Vec<u32> = vec![0];
    let mut running: u32 = 0;
    let mut deepest: u32 = 0;
    for lexeme in scan(text) {
        let spelled = text
            .get(lexeme.start as usize..lexeme.end as usize)
            .unwrap_or_default();
        match lexeme.kind {
            Lexed::TemplateHead => levels.push(0),
            Lexed::TemplateTail => {
                if levels.len() > 1 {
                    running = running.saturating_sub(levels.pop().unwrap_or_default());
                }
            }
            Lexed::Punct => match spelled {
                "(" | "[" | "{" => levels.push(0),
                ")" | "]" | "}" => {
                    if levels.len() > 1 {
                        running = running.saturating_sub(levels.pop().unwrap_or_default());
                    }
                }
                "," | ";" => {
                    if let Some(top) = levels.last_mut() {
                        running = running.saturating_sub(*top);
                        *top = 0;
                    }
                }
                "=" | "?" | ":" | "!" | "~" | "-" | "+" | "*" | "<" | "@" => {
                    if let Some(top) = levels.last_mut() {
                        *top += 1;
                        running += 1;
                    }
                }
                _ => {}
            },
            Lexed::Word if NESTING_WORDS.contains(&spelled) => {
                if let Some(top) = levels.last_mut() {
                    *top += 1;
                    running += 1;
                }
            }
            _ => {}
        }
        deepest = deepest.max(levels.len() as u32 + running);
    }
    deepest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nesting_counts_brackets_and_recursive_chains_per_level() {
        assert_eq!(estimate_nesting("f(a, b);"), 2);
        assert_eq!(estimate_nesting("((((x))))"), 5);
        assert_eq!(estimate_nesting("a = b = c = d;"), 4);
        // Commas and semicolons return to the bracket level.
        assert_eq!(estimate_nesting("f(a = 1, b = 2, c = 3);"), 3);
        assert_eq!(estimate_nesting("x = `${`${y}`}`"), 4);
        // Brackets inside strings and comments don't count.
        assert_eq!(estimate_nesting("'(((' /* ((( */ // (((\nx"), 1);
        let deep = format!("{}0", "x=".repeat(10_000));
        assert!(estimate_nesting(&deep) > MAX_NESTING);
    }

    #[test]
    fn small_files_get_a_stack_for_every_possible_token() {
        assert_eq!(possible_tokens("let x = f(a, b);"), 10);
        let stack = stack_for("((((((((").unwrap_or_default();
        assert_eq!(stack, BASE_STACK + 8 * STACK_PER_TOKEN);
        let huge = "(".repeat(MAX_STACK / STACK_PER_TOKEN);
        assert_eq!(stack_for(&huge), Err(StackError::TooDeep));
    }
}
