//! A forgiving lexical scanner that never fails, for text oxc cannot parse (highlighting) and
//! for estimating how deeply text nests before handing it to oxc ([`crate::stack`]).
//!
//! It knows comments, strings, template literals and their substitutions, numbers, words, and
//! punctuation, and tells a regular expression from a division by the previous lexeme, as
//! TypeScript's own scanner does for highlighting.

/// What a lexeme is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lexed {
    Comment,
    /// A string, a template without substitutions, or a regular expression.
    String,
    TemplateHead,
    TemplateMiddle,
    TemplateTail,
    Number,
    /// An identifier, private name, or keyword.
    Word,
    Punct,
}

/// One lexeme.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lexeme {
    pub(crate) kind: Lexed,
    pub(crate) start: u32,
    pub(crate) end: u32,
    /// Whether the innermost enclosing bracket is a `{` block (set by highlighting).
    pub(crate) in_brace: bool,
}

/// Reserved words (TypeScript's `FirstReservedWord..=LastReservedWord`).
pub(crate) const RESERVED: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
];

/// The lexemes of `text`, in order and non-overlapping.
pub(crate) fn scan(text: &str) -> Vec<Lexeme> {
    Scanner {
        text,
        bytes: text.as_bytes(),
        at: 0,
        out: Vec::new(),
        templates: Vec::new(),
    }
    .run()
}

struct Scanner<'t> {
    text: &'t str,
    bytes: &'t [u8],
    at: usize,
    out: Vec<Lexeme>,
    /// For each open `{`, whether it opened a template substitution.
    templates: Vec<bool>,
}

impl Scanner<'_> {
    fn run(mut self) -> Vec<Lexeme> {
        while self.at < self.bytes.len() {
            let start = self.at;
            let byte = self.bytes[start];
            let kind = match byte {
                b' ' | b'\t' | b'\n' | b'\r' => {
                    self.at += 1;
                    continue;
                }
                b'/' if self.peek(1) == Some(b'/') => {
                    self.until_line_end();
                    Lexed::Comment
                }
                b'/' if self.peek(1) == Some(b'*') => {
                    self.at = self.text[start + 2..]
                        .find("*/")
                        .map_or(self.bytes.len(), |end| start + end + 4);
                    Lexed::Comment
                }
                b'\'' | b'"' => {
                    self.string(byte);
                    Lexed::String
                }
                b'`' => {
                    self.at += 1;
                    self.template_rest(Lexed::String, Lexed::TemplateHead)
                }
                b'}' if self.templates.last() == Some(&true) => {
                    self.templates.pop();
                    self.at += 1;
                    self.template_rest(Lexed::TemplateTail, Lexed::TemplateMiddle)
                }
                b'{' => {
                    self.templates.push(false);
                    self.at += 1;
                    Lexed::Punct
                }
                b'}' => {
                    self.templates.pop();
                    self.at += 1;
                    Lexed::Punct
                }
                b'0'..=b'9' => {
                    self.number(start);
                    Lexed::Number
                }
                b'.' if self.peek(1).is_some_and(|next| next.is_ascii_digit()) => {
                    self.number(start);
                    Lexed::Number
                }
                b'/' if !self.after_operand() && self.regex() => Lexed::String,
                b'?' if self.peek(1) == Some(b'.')
                    && !self.peek(2).is_some_and(|next| next.is_ascii_digit()) =>
                {
                    self.at += 2;
                    Lexed::Punct
                }
                b'#' if self.text[start + 1..].chars().next().is_some_and(is_word_char) => {
                    self.at += 1;
                    self.word();
                    Lexed::Word
                }
                _ if self.text[start..].chars().next().is_some_and(is_word_start) => {
                    self.word();
                    Lexed::Word
                }
                _ => {
                    self.at += self.text[start..].chars().next().map_or(1, char::len_utf8);
                    Lexed::Punct
                }
            };
            self.out.push(Lexeme {
                kind,
                start: start as u32,
                end: self.at as u32,
                in_brace: false,
            });
        }
        self.out
    }

    fn peek(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.at + offset).copied()
    }

    fn until_line_end(&mut self) {
        while self.at < self.bytes.len() && !matches!(self.bytes[self.at], b'\n' | b'\r') {
            self.at += 1;
        }
        self.at = self.char_boundary(self.at);
    }

    fn char_boundary(&self, mut at: usize) -> usize {
        while at < self.bytes.len() && !self.text.is_char_boundary(at) {
            at += 1;
        }
        at
    }

    fn string(&mut self, quote: u8) {
        self.at += 1;
        while let Some(&byte) = self.bytes.get(self.at) {
            self.at += 1;
            match byte {
                b'\\' => self.at += 1,
                b'\n' | b'\r' => break,
                _ if byte == quote => break,
                _ => {}
            }
        }
        self.at = self.char_boundary(self.at.min(self.bytes.len()));
    }

    /// Scans template text after `` ` `` or a substitution's `}`, to the closing backtick
    /// (`done`) or a `${` (`open`).
    fn template_rest(&mut self, done: Lexed, open: Lexed) -> Lexed {
        while let Some(&byte) = self.bytes.get(self.at) {
            match byte {
                b'\\' => self.at += 2,
                b'`' => {
                    self.at += 1;
                    return done;
                }
                b'$' if self.peek(1) == Some(b'{') => {
                    self.at += 2;
                    self.templates.push(true);
                    return open;
                }
                _ => self.at += 1,
            }
        }
        self.at = self.bytes.len();
        done
    }

    fn number(&mut self, start: usize) {
        let hex = matches!(self.bytes.get(start + 1), Some(b'x' | b'X'));
        while let Some(&byte) = self.bytes.get(self.at) {
            let exponent_sign = !hex
                && matches!(byte, b'+' | b'-')
                && self.at > start
                && matches!(self.bytes[self.at - 1], b'e' | b'E');
            if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' || exponent_sign {
                self.at += 1;
            } else {
                break;
            }
        }
    }

    fn word(&mut self) {
        let rest = &self.text[self.at..];
        let len = rest
            .char_indices()
            .find(|&(_, c)| !is_word_char(c))
            .map_or(rest.len(), |(index, _)| index);
        self.at += len.max(1);
        self.at = self.char_boundary(self.at);
    }

    /// Whether the previous lexeme ends an operand, so a `/` divides.
    fn after_operand(&self) -> bool {
        let Some(previous) = self.out.iter().rev().find(|lexeme| lexeme.kind != Lexed::Comment) else {
            return false;
        };
        let spelled = &self.text[previous.start as usize..previous.end as usize];
        match previous.kind {
            Lexed::Number | Lexed::String | Lexed::TemplateTail => true,
            Lexed::Word => matches!(spelled, "this" | "super") || !RESERVED.contains(&spelled),
            Lexed::Punct => matches!(spelled, ")" | "]" | "}"),
            _ => false,
        }
    }

    /// Scans a regular expression literal at `/`; `false` (consuming nothing) if it does not
    /// close on this line.
    fn regex(&mut self) -> bool {
        let mut at = self.at + 1;
        let mut class = false;
        while let Some(&byte) = self.bytes.get(at) {
            match byte {
                b'\\' => at += 1,
                b'[' => class = true,
                b']' => class = false,
                b'/' if !class => {
                    at += 1;
                    while self.bytes.get(at).is_some_and(|flag| flag.is_ascii_alphabetic()) {
                        at += 1;
                    }
                    self.at = self.char_boundary(at.min(self.bytes.len()));
                    return true;
                }
                b'\n' | b'\r' => return false,
                _ => {}
            }
            at += 1;
        }
        false
    }
}

fn is_word_start(c: char) -> bool {
    c == '$' || c == '_' || c.is_alphabetic()
}

fn is_word_char(c: char) -> bool {
    c == '$' || c == '_' || c.is_alphanumeric() || c == '\u{200c}' || c == '\u{200d}'
}
