//! A strict JSON reader that keeps every value's and member's byte range, for `package.json`
//! evidence.
//!
//! The proof of concept read package inputs twice (`JSON.parse` for values, TypeScript's JSON
//! parser for ranges) and required both to succeed, so only strict JSON (RFC 8259) is accepted:
//! no comments, trailing commas, single quotes, or control characters in strings. A leading
//! byte-order mark is skipped, and offsets still count it. A member's range runs from the opening
//! quote of its key to the end of its value, with no surrounding whitespace. Duplicate keys keep
//! the last value and that member's range, in the first key's place, as `JSON.parse` does. An
//! escaped lone surrogate (`"\ud800"`), which a Rust string can't hold, decodes to U+FFFD.

use rustc_hash::FxHashMap;
use unfold_core::ByteRange;

/// The deepest nesting accepted; deeper input is refused rather than risking the stack.
const MAX_DEPTH: usize = 256;

/// One JSON value and its range.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node {
    /// The value's byte range.
    pub(crate) range: ByteRange,
    /// The value.
    pub(crate) value: Value,
}

/// A JSON value. Numbers, booleans, and `null` keep only their kind.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    /// An object's members, in first-occurrence order with duplicates merged.
    Object(Vec<Member>),
    /// An array's elements.
    Array(Vec<Node>),
    /// A decoded string.
    String(Box<str>),
    /// Any other value: a number, `true`, `false`, or `null`.
    Scalar,
}

/// One member of an object.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Member {
    /// The decoded key.
    pub(crate) key: Box<str>,
    /// From the key's opening quote to the end of the value.
    pub(crate) range: ByteRange,
    /// The value.
    pub(crate) value: Node,
}

impl Value {
    /// The members of an object, if this is one.
    pub(crate) fn members(&self) -> Option<&[Member]> {
        match self {
            Self::Object(members) => Some(members),
            _ => None,
        }
    }

    /// The member named `key` of an object.
    pub(crate) fn member(&self, key: &str) -> Option<&Member> {
        self.members()?.iter().find(|member| &*member.key == key)
    }

    /// The string, if this is one.
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }
}

/// An RFC 6901 pointer segment: `~` becomes `~0` and `/` becomes `~1`.
pub(crate) fn pointer_segment(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// Parses `text` as one strict JSON value.
pub(crate) fn parse(text: &str) -> Result<Node, String> {
    let start = if text.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let mut parser = Parser {
        bytes: text.as_bytes(),
        text,
        at: start,
    };
    parser.whitespace();
    let node = parser.value(0)?;
    parser.whitespace();
    if parser.at < parser.bytes.len() {
        return Err(parser.error("unexpected text after the value"));
    }
    Ok(node)
}

struct Parser<'t> {
    bytes: &'t [u8],
    text: &'t str,
    at: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> String {
        format!("{message} at byte {}", self.at)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn range(&self, start: usize) -> ByteRange {
        ByteRange {
            start: start as u32,
            end: self.at as u32,
        }
    }

    fn value(&mut self, depth: usize) -> Result<Node, String> {
        if depth > MAX_DEPTH {
            return Err(self.error("JSON is nested too deeply"));
        }
        let start = self.at;
        let value = match self.peek() {
            Some(b'{') => self.object(depth)?,
            Some(b'[') => self.array(depth)?,
            Some(b'"') => Value::String(self.string()?.into()),
            Some(b't') => self.word("true")?,
            Some(b'f') => self.word("false")?,
            Some(b'n') => self.word("null")?,
            Some(b'-' | b'0'..=b'9') => self.number()?,
            _ => return Err(self.error("expected a JSON value")),
        };
        Ok(Node {
            range: self.range(start),
            value,
        })
    }

    fn object(&mut self, depth: usize) -> Result<Value, String> {
        self.at += 1;
        let mut members: Vec<Member> = Vec::new();
        let mut positions: FxHashMap<Box<str>, usize> = FxHashMap::default();
        self.whitespace();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Value::Object(members));
        }
        loop {
            self.whitespace();
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a member name"));
            }
            let start = self.at;
            let key = self.string()?;
            self.whitespace();
            if self.peek() != Some(b':') {
                return Err(self.error("expected `:`"));
            }
            self.at += 1;
            self.whitespace();
            let value = self.value(depth + 1)?;
            let member = Member {
                key: key.into(),
                range: self.range(start),
                value,
            };
            match positions.get(&member.key) {
                Some(&position) => members[position] = member,
                None => {
                    positions.insert(member.key.clone(), members.len());
                    members.push(member);
                }
            }
            self.whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Value::Object(members));
                }
                _ => return Err(self.error("expected `,` or `}`")),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, String> {
        self.at += 1;
        let mut elements = Vec::new();
        self.whitespace();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Value::Array(elements));
        }
        loop {
            self.whitespace();
            elements.push(self.value(depth + 1)?);
            self.whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Value::Array(elements));
                }
                _ => return Err(self.error("expected `,` or `]`")),
            }
        }
    }

    fn word(&mut self, word: &str) -> Result<Value, String> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(Value::Scalar)
        } else {
            Err(self.error("expected a JSON value"))
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.at;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        self.at - start
    }

    fn number(&mut self) -> Result<Value, String> {
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(self.error("invalid number")),
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            if self.digits() == 0 {
                return Err(self.error("invalid number"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if self.digits() == 0 {
                return Err(self.error("invalid number"));
            }
        }
        Ok(Value::Scalar)
    }

    /// A string starting at the opening quote, decoded.
    fn string(&mut self) -> Result<String, String> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let run = self.at;
            while matches!(self.peek(), Some(byte) if byte != b'"' && byte != b'\\' && byte >= 0x20) {
                self.at += 1;
            }
            // The run ends at an ASCII byte (or the end), so it is on a character boundary.
            out.push_str(&self.text[run..self.at]);
            match self.peek() {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escaped = self.peek().ok_or_else(|| self.error("unterminated string"))?;
                    self.at += 1;
                    match escaped {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.unicode()?),
                        _ => return Err(self.error("invalid escape")),
                    }
                }
                Some(_) => return Err(self.error("control character in a string")),
                None => return Err(self.error("unterminated string")),
            }
        }
    }

    /// The character of a `\u` escape (the `\u` already read), joining a surrogate pair.
    fn unicode(&mut self) -> Result<char, String> {
        let first = self.hex4()?;
        if (0xD800..0xDC00).contains(&first)
            && self.bytes[self.at..].starts_with(b"\\u")
            && let Some(second) = self.peek_hex4(self.at + 2)
            && (0xDC00..0xE000).contains(&second)
        {
            self.at += 6;
            let code = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
            return Ok(char::from_u32(code).unwrap_or('\u{fffd}'));
        }
        Ok(char::from_u32(first).unwrap_or('\u{fffd}'))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let value = self
            .peek_hex4(self.at)
            .ok_or_else(|| self.error("invalid unicode escape"))?;
        self.at += 4;
        Ok(value)
    }

    fn peek_hex4(&self, at: usize) -> Option<u32> {
        let digits = self.bytes.get(at..at + 4)?;
        let text = std::str::from_utf8(digits).ok()?;
        if !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(text, 16).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice(text: &str, range: ByteRange) -> &str {
        &text[range.start as usize..range.end as usize]
    }

    #[test]
    fn keeps_member_ranges_and_the_last_duplicate() {
        let text =
            "\u{feff}{ \"a\" : [1, {\"b\": null}],\n\"c\":\"x\", \"c\" : \"y\\n\\u00e9\\ud83d\\ude00\" }";
        let root = parse(text).expect("parses");
        let members = root.value.members().expect("an object");
        assert_eq!(members.len(), 2);
        assert_eq!(slice(text, members[0].range), "\"a\" : [1, {\"b\": null}]");
        assert_eq!(
            slice(text, members[1].range),
            "\"c\" : \"y\\n\\u00e9\\ud83d\\ude00\""
        );
        assert_eq!(members[1].value.value.as_str(), Some("y\né😀"));
        assert_eq!(slice(text, root.range), &text[3..]);
    }

    #[test]
    fn rejects_what_is_not_strict_json() {
        for text in [
            "",
            "{",
            "{\"a\": 1,}",
            "[1,]",
            "// x\n{}",
            "{'a': 1}",
            "{\"a\": 01}",
            "{\"a\": 1.}",
            "{\"a\": \"\t\"}",
            "{\"a\": \"\\x\"}",
            "{} {}",
            "{\"a\" 1}",
            "nul",
        ] {
            assert!(parse(text).is_err(), "{text:?}");
        }
        assert!(parse(&"[".repeat(300)).is_err());
    }

    #[test]
    fn decodes_lone_surrogates_and_escapes_pointer_segments() {
        let root = parse("\"\\ud800x\"").expect("parses");
        assert_eq!(root.value.as_str(), Some("\u{fffd}x"));
        assert_eq!(pointer_segment("a/b~c"), "a~1b~0c");
    }
}
