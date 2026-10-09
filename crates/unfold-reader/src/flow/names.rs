//! Fallback names for Flow nodes: the dotted identifier paths written in a piece of verified
//! source, in order and without duplicates.
//!
//! This is a lexical reading aid, not a parser. It never derives structure, ranges, or
//! relationships (those come only from the stored blocks, slots, and edges); it only picks the
//! names a person would read in a condition, a loop header, or a statement, so an undescribed
//! node can show something better than its kind. The rules are the proof of concept's:
//!
//! - **TypeScript.** Line comments, block comments, string and template literals, and regular
//!   expression literals are blanked in that order; then every identifier path
//!   (`a.b`, `a?.b`) that does not follow a word character, `$`, or `.` is taken, with
//!   whitespace removed, a leading `this.` dropped, and reserved words skipped.
//! - **Python.** The text is split into tokens (comments, string literals with their prefixes,
//!   identifiers, keywords, numbers, punctuation); identifiers joined by `.` form one path, and
//!   keywords are skipped.
//!
//! A name that extends another one (`customerId.trim` after `customerId`) is dropped, and past
//! `limit` names the list ends with `…`.

use rustc_hash::FxHashSet;
use unfold_core::vocab::Language;

/// TypeScript words that never name a value worth showing.
const RESERVED: &[&str] = &[
    "typeof",
    "instanceof",
    "in",
    "of",
    "new",
    "void",
    "delete",
    "await",
    "yield",
    "null",
    "undefined",
    "true",
    "false",
    "this",
    "super",
    "as",
    "keyof",
    "const",
    "let",
    "var",
];

/// Python's keywords, including the soft keywords `match`, `case`, and `type`.
const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue", "def",
    "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with", "yield", "match",
    "case", "type",
];

/// The names written in `text`, at most `limit` of them followed by `…` when there are more.
pub(crate) fn written_names(language: Language, text: &str, limit: usize) -> Vec<String> {
    let names = match language {
        Language::TypeScript => typescript_names(text),
        Language::Python => python_names(text),
    };
    // A name is dropped when a shorter one is a prefix of it ending right before a `.`. The
    // names are distinct, so checking each of its `.` positions against a set is enough (and
    // linear, where comparing every pair was quadratic on long statements).
    let known: FxHashSet<&str> = names.iter().map(String::as_str).collect();
    let extends_another = |name: &str| {
        name.match_indices('.')
            .any(|(at, _)| at > 0 && known.contains(&name[..at]))
    };
    let mut distinct: Vec<String> = names
        .iter()
        .filter(|name| !extends_another(name))
        .take(limit.saturating_add(1))
        .cloned()
        .collect();
    if distinct.len() > limit {
        distinct.truncate(limit);
        distinct.push("…".to_owned());
    }
    distinct
}

/// Appends `name` to `names` unless it is already there.
fn push_new(names: &mut Vec<String>, seen: &mut FxHashSet<String>, name: &str) {
    if !seen.contains(name) {
        seen.insert(name.to_owned());
        names.push(name.to_owned());
    }
}

/// Collapses whitespace runs to one space, trims, and shortens to at most `length` UTF-16 code
/// units, ending with `…` when shortened.
pub(crate) fn clip(value: &str, length: usize) -> String {
    let flat = value
        .split(is_space)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if flat.encode_utf16().count() <= length {
        return flat;
    }
    let mut out = String::with_capacity(flat.len());
    let mut units = 0;
    for character in flat.chars() {
        units += character.len_utf16();
        if units > length - 1 {
            break;
        }
        out.push(character);
    }
    out.push('…');
    out
}

/// JavaScript's `\s`.
fn is_space(character: char) -> bool {
    character.is_whitespace() || character == '\u{feff}'
}

/// JavaScript's line terminators, which `.` does not match.
fn is_line_terminator(character: char) -> bool {
    matches!(character, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn is_word(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn is_ts_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_' || character == '$'
}

fn is_ts_continue(character: char) -> bool {
    is_word(character) || character == '$'
}

/// One blanking pass: wherever `matcher` matches at a position (returning the match's end), the
/// match becomes one space; elsewhere characters are kept.
fn blank(text: &[char], matcher: impl Fn(&[char], usize) -> Option<usize>) -> Vec<char> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        match matcher(text, at) {
            Some(end) if end > at => {
                out.push(' ');
                at = end;
            }
            _ => {
                out.push(text[at]);
                at += 1;
            }
        }
    }
    out
}

/// `//.*$` with the multiline flag.
fn line_comment(text: &[char], at: usize) -> Option<usize> {
    if text.get(at) != Some(&'/') || text.get(at + 1) != Some(&'/') {
        return None;
    }
    let mut end = at + 2;
    while end < text.len() && !is_line_terminator(text[end]) {
        end += 1;
    }
    Some(end)
}

/// `/\*[\s\S]*?\*/`.
fn block_comment(text: &[char], at: usize) -> Option<usize> {
    if text.get(at) != Some(&'/') || text.get(at + 1) != Some(&'*') {
        return None;
    }
    let mut end = at + 2;
    while end + 1 < text.len() {
        if text[end] == '*' && text[end + 1] == '/' {
            return Some(end + 2);
        }
        end += 1;
    }
    None
}

/// `(["'`])(?:\\.|(?!\1)[^\\])*\1`.
fn string_literal(text: &[char], at: usize) -> Option<usize> {
    let quote = *text.get(at)?;
    if !matches!(quote, '"' | '\'' | '`') {
        return None;
    }
    let mut end = at + 1;
    while end < text.len() {
        match text[end] {
            '\\' => match text.get(end + 1) {
                Some(&next) if !is_line_terminator(next) => end += 2,
                _ => return None,
            },
            character if character == quote => return Some(end + 1),
            _ => end += 1,
        }
    }
    None
}

/// `\/(?![*/])(?:\\.|[^/\n\\])+\/[a-z]*`.
fn regex_literal(text: &[char], at: usize) -> Option<usize> {
    if text.get(at) != Some(&'/') || matches!(text.get(at + 1), Some('*' | '/') | None) {
        return None;
    }
    let mut end = at + 1;
    let mut length = 0;
    loop {
        match text.get(end) {
            Some('\\') => match text.get(end + 1) {
                Some(&next) if !is_line_terminator(next) => end += 2,
                _ => return None,
            },
            Some('/') => break,
            Some('\n') | None => return None,
            Some(_) => end += 1,
        }
        length += 1;
    }
    if length == 0 {
        return None;
    }
    end += 1;
    while text.get(end).is_some_and(char::is_ascii_lowercase) {
        end += 1;
    }
    Some(end)
}

fn typescript_names(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let chars = blank(&chars, line_comment);
    let chars = blank(&chars, block_comment);
    let chars = blank(&chars, string_literal);
    let text = blank(&chars, regex_literal);
    let mut names: Vec<String> = Vec::new();
    let mut seen = FxHashSet::default();
    let mut at = 0;
    while at < text.len() {
        let follows_word = at > 0 && (is_ts_continue(text[at - 1]) || text[at - 1] == '.');
        if !is_ts_start(text[at]) || follows_word {
            at += 1;
            continue;
        }
        let mut end = at + 1;
        while end < text.len() && is_ts_continue(text[end]) {
            end += 1;
        }
        // Repeat `\s*\??\.\s*identifier` while it matches completely.
        loop {
            let mut probe = end;
            while probe < text.len() && is_space(text[probe]) {
                probe += 1;
            }
            if text.get(probe) == Some(&'?') {
                probe += 1;
            }
            if text.get(probe) != Some(&'.') {
                break;
            }
            probe += 1;
            while probe < text.len() && is_space(text[probe]) {
                probe += 1;
            }
            if !text.get(probe).copied().is_some_and(is_ts_start) {
                break;
            }
            probe += 1;
            while probe < text.len() && is_ts_continue(text[probe]) {
                probe += 1;
            }
            end = probe;
        }
        let written: String = text[at..end]
            .iter()
            .filter(|&&character| !is_space(character))
            .collect();
        let name = written
            .strip_prefix("this.")
            .map_or(written.as_str(), |rest| rest);
        if !RESERVED.contains(&name) {
            push_new(&mut names, &mut seen, name);
        }
        at = end;
    }
    names
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Identifier,
    Other,
}

fn is_python_start(character: char) -> bool {
    character == '_' || (character != '$' && unicode_id_start::is_id_start(character))
}

fn is_python_continue(character: char) -> bool {
    character == '_' || (character != '$' && unicode_id_start::is_id_continue(character))
}

/// Where a Python string literal opening at `quote` ends (or the end of the text).
fn python_string_end(text: &[char], quote: usize, formatted: bool) -> usize {
    let mark = text[quote];
    let triple = text.get(quote + 1) == Some(&mark) && text.get(quote + 2) == Some(&mark);
    let delimiter = if triple { 3 } else { 1 };
    let closes = |at: usize| (0..delimiter).all(|offset| text.get(at + offset) == Some(&mark));
    let mut end = quote + delimiter;
    let mut braces = 0u32;
    while end < text.len() {
        let character = text[end];
        if character == '\\' {
            end = (end + 2).min(text.len());
            continue;
        }
        if formatted && braces > 0 && matches!(character, '\'' | '"') {
            end = python_string_end(text, end, false);
            continue;
        }
        if formatted && character == '{' {
            if braces == 0 && text.get(end + 1) == Some(&'{') {
                end += 2;
                continue;
            }
            braces += 1;
            end += 1;
            continue;
        }
        if formatted && braces > 0 && character == '}' {
            braces -= 1;
            end += 1;
            continue;
        }
        if braces == 0 && closes(end) {
            return end + delimiter;
        }
        if braces == 0 && delimiter == 1 && matches!(character, '\r' | '\n') {
            break;
        }
        end += 1;
    }
    end
}

/// The length of a string prefix at `at` that is directly followed by a quote, if any:
/// `(?:[rRuUbBfFtT]|[rR][bBfFtT]|[bBfFtT][rR])?(['"])`.
fn python_prefix(text: &[char], at: usize) -> Option<usize> {
    let is_quote = |offset: usize| matches!(text.get(at + offset), Some('\'' | '"'));
    let lower = |offset: usize| text.get(at + offset).map(char::to_ascii_lowercase);
    if is_quote(0) {
        return Some(0);
    }
    if matches!(lower(0), Some('r' | 'u' | 'b' | 'f' | 't')) && is_quote(1) {
        return Some(1);
    }
    let two = matches!(
        (lower(0), lower(1)),
        (Some('r'), Some('b' | 'f' | 't')) | (Some('b' | 'f' | 't'), Some('r'))
    );
    (two && is_quote(2)).then_some(2)
}

/// The end of a Python number literal starting at `at`, if one starts there.
fn python_number(text: &[char], at: usize) -> Option<usize> {
    let digit_or_underscore = |character: char, radix: u32| character == '_' || character.is_digit(radix);
    let run = |mut end: usize, radix: u32| {
        while text
            .get(end)
            .is_some_and(|&character| digit_or_underscore(character, radix))
        {
            end += 1;
        }
        end
    };
    let first = *text.get(at)?;
    if first == '0' {
        let radix = match text.get(at + 1) {
            Some('x' | 'X') => Some(16),
            Some('b' | 'B') => Some(2),
            Some('o' | 'O') => Some(8),
            _ => None,
        };
        if let Some(radix) = radix
            && text
                .get(at + 2)
                .is_some_and(|&character| digit_or_underscore(character, radix))
        {
            let end = run(at + 2, radix);
            return Some(if matches!(text.get(end), Some('j' | 'J')) {
                end + 1
            } else {
                end
            });
        }
    }
    let mut end = if first.is_ascii_digit() {
        let end = run(at + 1, 10);
        if text.get(end) == Some(&'.') {
            run(end + 1, 10)
        } else {
            end
        }
    } else if first == '.'
        && text
            .get(at + 1)
            .is_some_and(|&character| digit_or_underscore(character, 10))
    {
        run(at + 1, 10)
    } else {
        return None;
    };
    if matches!(text.get(end), Some('e' | 'E')) {
        let sign = usize::from(matches!(text.get(end + 1), Some('+' | '-')));
        if text
            .get(end + 1 + sign)
            .is_some_and(|&character| digit_or_underscore(character, 10))
        {
            end = run(end + 1 + sign, 10);
        }
    }
    if matches!(text.get(end), Some('j' | 'J')) {
        end += 1;
    }
    Some(end)
}

fn python_tokens(text: &[char]) -> Vec<(usize, usize, TokenKind)> {
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let character = text[at];
        if is_space(character) {
            at += 1;
            continue;
        }
        if character == '#' {
            let mut end = at + 1;
            while end < text.len() && !matches!(text[end], '\r' | '\n') {
                end += 1;
            }
            tokens.push((at, end, TokenKind::Other));
            at = end;
            continue;
        }
        if let Some(prefix) = python_prefix(text, at) {
            let formatted = text[at..at + prefix]
                .iter()
                .any(|character| matches!(character, 'f' | 'F' | 't' | 'T'));
            let end = python_string_end(text, at + prefix, formatted);
            tokens.push((at, end, TokenKind::Other));
            at = end;
            continue;
        }
        if is_python_start(character) {
            let mut end = at + 1;
            while end < text.len() && is_python_continue(text[end]) {
                end += 1;
            }
            let word: String = text[at..end].iter().collect();
            let kind = if PYTHON_KEYWORDS.contains(&word.as_str()) {
                TokenKind::Other
            } else {
                TokenKind::Identifier
            };
            tokens.push((at, end, kind));
            at = end;
            continue;
        }
        if let Some(end) = python_number(text, at) {
            tokens.push((at, end, TokenKind::Other));
            at = end;
            continue;
        }
        tokens.push((at, at + 1, TokenKind::Other));
        at += 1;
    }
    tokens
}

fn python_names(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let tokens = python_tokens(&chars);
    let spelling = |token: &(usize, usize, TokenKind)| chars[token.0..token.1].iter().collect::<String>();
    let is_dot = |token: &(usize, usize, TokenKind)| token.1 == token.0 + 1 && chars[token.0] == '.';
    let mut names: Vec<String> = Vec::new();
    let mut seen = FxHashSet::default();
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        if token.2 != TokenKind::Identifier {
            index += 1;
            continue;
        }
        let mut name = spelling(&token);
        while tokens.get(index + 1).is_some_and(is_dot)
            && tokens
                .get(index + 2)
                .is_some_and(|member| member.2 == TokenKind::Identifier)
        {
            name.push('.');
            name.extend(&chars[tokens[index + 2].0..tokens[index + 2].1]);
            index += 2;
        }
        push_new(&mut names, &mut seen, &name);
        index += 1;
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(text: &str) -> String {
        written_names(Language::TypeScript, text, 4).join(", ")
    }

    fn py(text: &str) -> String {
        written_names(Language::Python, text, 4).join(", ")
    }

    #[test]
    fn typescript_names_follow_the_written_paths() {
        assert_eq!(
            ts(r#"typeof customerId !== "string" || customerId.trim() === """#),
            "customerId"
        );
        assert_eq!(ts(r#"req.header("prefer") === "respond-async""#), "req.header");
        assert_eq!(ts("this.orders.get(id) ?? fallback"), "orders.get, id, fallback");
        assert_eq!(ts("a?.b // trailing comment"), "a?.b");
        assert_eq!(ts("/* x */ value / 2 > limit"), "value, limit");
        assert_eq!(ts("/^[^\\s@]+@[^\\s@]+$/.test(email)"), "email");
        assert_eq!(ts("`${prefix}-${id}` === key"), "key");
        assert_eq!(ts("res.status(404).json({ error })"), "res.status, error");
        assert_eq!(ts("a || b || c || d || e"), "a, b, c, d, …");
        assert_eq!(ts("1e5 + x.y"), "x.y");
    }

    #[test]
    fn python_names_skip_keywords_strings_and_numbers() {
        assert_eq!(py("not isinstance(value, dict)"), "isinstance, value, dict");
        assert_eq!(py("value == \"STOP\""), "value");
        assert_eq!(py("f\"{name!r}\" in self.orders # note"), "self.orders");
        assert_eq!(py("x = 0x1f + 1.5e3j"), "x");
        assert_eq!(py("rb'raw' + café.naïve"), "café.naïve");
        assert_eq!(py("foo().bar"), "foo, bar");
        assert_eq!(py("a.b + a + a.b.c + d"), "a, d");
    }

    #[test]
    fn long_texts_are_read_in_linear_time() {
        // Forty thousand distinct names: pairwise checks made this take minutes.
        let text: String = (0..20_000)
            .map(|index| format!("name{index}.member + name{index} + "))
            .collect();
        assert_eq!(
            written_names(Language::TypeScript, &text, 2),
            ["name0", "name1", "…"]
        );
        assert_eq!(written_names(Language::Python, &text, 2), ["name0", "name1", "…"]);
    }

    #[test]
    fn clipping_counts_utf16_units() {
        assert_eq!(clip("  a \n  b ", 48), "a b");
        assert_eq!(clip(&"x".repeat(60), 48), format!("{}…", "x".repeat(47)));
        assert_eq!(clip(&"😀".repeat(30), 10).chars().count(), 5);
    }
}
