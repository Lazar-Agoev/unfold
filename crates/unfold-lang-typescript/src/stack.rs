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

use crate::scan::{Lexed, RESERVED, scan};

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

/// Words that go on with the code before them instead of beginning a statement: an `if`'s
/// `else`, a `try`'s `catch` and `finally`, a `do`'s `while`, and the operators and clauses
/// written as words.
const CONTINUING: &[&str] = &[
    "else",
    "catch",
    "finally",
    "while",
    "in",
    "instanceof",
    "as",
    "satisfies",
    "extends",
    "implements",
    "is",
    "of",
];

/// Words whose parenthesized header a statement follows (`for await (…)` included).
const HEADED: &[&str] = &["if", "for", "while", "with", "await"];

/// Words that end an expression although they are keywords.
const VALUES: &[&str] = &["this", "super", "null", "true", "false"];

/// One bracket level: the run of recursing operators and keywords written in it, and whether
/// it is a statement's parenthesized header (`if (…)`), which a statement follows.
#[derive(Debug, Default)]
struct Level {
    run: u32,
    header: bool,
}

/// The open bracket levels, the outermost always open, and the sum of their runs.
struct Levels {
    open: Vec<Level>,
    running: u32,
}

impl Levels {
    fn new() -> Self {
        Self {
            open: vec![Level::default()],
            running: 0,
        }
    }

    /// The estimated depth: one per open level, plus every open run.
    fn depth(&self) -> u32 {
        self.open.len() as u32 + self.running
    }

    fn open(&mut self, header: bool) {
        self.open.push(Level { run: 0, header });
    }

    /// Closes the innermost level, if it isn't the outermost; whether it was a header.
    fn close(&mut self) -> bool {
        if self.open.len() == 1 {
            return false;
        }
        self.open.pop().is_some_and(|level| {
            self.running -= level.run;
            level.header
        })
    }

    /// One more recursing operator or keyword at the innermost level.
    fn grow(&mut self) {
        if let Some(level) = self.open.last_mut() {
            level.run += 1;
            self.running += 1;
        }
    }

    /// The parser is back at the innermost level: its run starts again.
    fn restart(&mut self) {
        if let Some(level) = self.open.last_mut() {
            self.running -= level.run;
            level.run = 0;
        }
    }
}

/// What the previous lexeme lets a following word begin.
#[derive(Debug, Clone, Copy)]
enum Ended {
    /// Nothing: an operator, a keyword, or a statement's header came last.
    Nothing,
    /// A `}`, after which a word begins a statement.
    Block,
    /// A complete expression ending at this offset, after which a word on a later line begins
    /// a statement (automatic semicolon insertion).
    Expression(usize),
}

/// Estimates how deeply `text` nests: brackets and template substitutions, plus, within each
/// bracket level, the run of operators and keywords that recurse (assignments, conditionals,
/// unary and arrow operators, statement keywords, ...) in the statement or list item being
/// written. The run starts again where the parser returns to the level: at `,` and `;`, and at
/// a word (or decorator) that begins a new statement, after a `}` or after a line break that
/// ends a complete expression. A word that goes on with the code before it ([`CONTINUING`])
/// begins nothing.
pub(crate) fn estimate_nesting(text: &str) -> u32 {
    let mut levels = Levels::new();
    let mut deepest: u32 = 0;
    let mut ended = Ended::Nothing;
    let mut previous_word: Option<&str> = None;
    for lexeme in scan(text) {
        if lexeme.kind == Lexed::Comment {
            continue;
        }
        let (start, end) = (lexeme.start as usize, lexeme.end as usize);
        let spelled = text.get(start..end).unwrap_or_default();
        let may_begin = match lexeme.kind {
            Lexed::Word => !CONTINUING.contains(&spelled),
            Lexed::Punct => spelled == "@",
            _ => false,
        };
        let begins = may_begin
            && match ended {
                Ended::Nothing => false,
                Ended::Block => true,
                Ended::Expression(at) => text
                    .get(at..start)
                    .is_some_and(|between| between.contains(['\n', '\r', '\u{2028}', '\u{2029}'])),
            };
        if begins {
            levels.restart();
        }
        ended = Ended::Nothing;
        match lexeme.kind {
            Lexed::TemplateHead => levels.open(false),
            Lexed::TemplateTail => {
                levels.close();
                ended = Ended::Expression(end);
            }
            Lexed::Punct => match spelled {
                "(" => levels.open(previous_word.is_some_and(|word| HEADED.contains(&word))),
                "[" | "{" => levels.open(false),
                ")" | "]" | "}" => {
                    let header = levels.close();
                    ended = match spelled {
                        "}" => Ended::Block,
                        _ if header => Ended::Nothing,
                        _ => Ended::Expression(end),
                    };
                }
                "," | ";" => levels.restart(),
                "=" | "?" | ":" | "!" | "~" | "-" | "+" | "*" | "<" | "@" => levels.grow(),
                _ => {}
            },
            Lexed::Word if NESTING_WORDS.contains(&spelled) => levels.grow(),
            Lexed::Word => {
                let keyword = RESERVED.contains(&spelled) || CONTINUING.contains(&spelled);
                if VALUES.contains(&spelled) || !keyword {
                    ended = Ended::Expression(end);
                }
            }
            Lexed::Number | Lexed::String => ended = Ended::Expression(end),
            Lexed::Comment | Lexed::TemplateMiddle => {}
        }
        previous_word = (lexeme.kind == Lexed::Word).then_some(spelled);
        deepest = deepest.max(levels.depth());
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
    fn a_new_statement_returns_to_its_level() {
        // A word after a `}`, or after a line break that ends an expression, begins a statement.
        assert_eq!(
            estimate_nesting("if (a) { f(); } if (b) { g(); }"),
            estimate_nesting("if (a) { f(); }")
        );
        assert_eq!(estimate_nesting("x = a\ny = b"), 2);
        let siblings = [
            "function f(a: T): T { return a; }\n",
            "if (a) { f(); }\n",
            "const x: T = f(1)\n",
            "@d\nm(): T {}\n",
        ];
        for sibling in siblings {
            assert!(estimate_nesting(&sibling.repeat(10_000)) < 10, "{sibling}");
        }
        // What goes on after them still nests: `else`, an operator word, an unfinished
        // expression, or a statement's header.
        let nested = [
            "if (a) {} else ",
            "x = {} in ",
            "x = () => {} as ",
            "x = a +\n",
            "if (a)\n",
        ];
        for nested in nested {
            let text = format!("{}0", nested.repeat(10_000));
            assert!(estimate_nesting(&text) > MAX_NESTING, "{nested}");
        }
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
