//! Reading `[project.scripts]` from a `pyproject.toml`, with the exact byte range and RFC 6901
//! pointer of every assignment (the proof of concept's TOML input ranges).
//!
//! The `toml` crate validates the whole document (duplicate keys, invalid syntax) and supplies
//! the values; a second pass over `toml_parser`'s events finds each assignment's range: from the
//! first key of the assignment as written (`project.scripts."a" = …`, `a = …` under a
//! `[project.scripts]` header, or `"a" = …` inside an inline table) to the end of its value. No
//! trailing comment or whitespace is included.

use toml_parser::decoder::Encoding;
use toml_parser::parser::{EventReceiver, parse_document};
use toml_parser::{ErrorSink, ParseError, Raw, Source, Span};
use unfold_core::ByteRange;

/// One `[project.scripts]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Script {
    /// The script's name (the entrance trigger).
    pub(crate) name: Box<str>,
    /// The declared value: a string, or the written text of anything else.
    pub(crate) value: ScriptValue,
    /// The assignment's byte range in the file.
    pub(crate) range: ByteRange,
    /// `/project/scripts/<name>`, escaped.
    pub(crate) pointer: Box<str>,
}

/// A script's declared value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScriptValue {
    /// A TOML string, decoded.
    Text(Box<str>),
    /// Any other value, as written after the `=`.
    Other(Box<str>),
}

/// The scripts of a `pyproject.toml`, in written order, or why the file can't supply them.
pub(crate) fn scripts(text: &str) -> Result<Vec<Script>, String> {
    // Offsets stay file offsets: a byte-order mark is skipped, not removed.
    let (body, base) = match text.strip_prefix('\u{feff}') {
        Some(body) => (body, 3),
        None => (text, 0),
    };
    let document: toml::Table =
        toml::from_str(body).map_err(|error| format!("invalid TOML: {}", error.message().trim_end()))?;
    let Some(project) = document.get("project") else {
        return Ok(Vec::new());
    };
    let toml::Value::Table(project) = project else {
        return Err("project must be a TOML table".to_owned());
    };
    let Some(declared) = project.get("scripts") else {
        return Ok(Vec::new());
    };
    let toml::Value::Table(declared) = declared else {
        return Err("project.scripts must be a TOML table".to_owned());
    };
    let spans = assignments(body);
    let mut scripts = Vec::with_capacity(declared.len());
    for (name, value) in declared {
        let pointer = format!("/project/scripts/{}", pointer_segment(name));
        let Some(&(start, end)) = spans
            .iter()
            .find(|(path, _)| path.len() == 3 && path[2] == *name)
            .map(|(_, span)| span)
        else {
            return Err(format!("missing console-script assignment evidence: {pointer}"));
        };
        let written = &body[start..end];
        let value = match value {
            toml::Value::String(text) => ScriptValue::Text(text.as_str().into()),
            _ => ScriptValue::Other(written_value(written).into()),
        };
        let range = file_range(base + start, base + end)
            .ok_or_else(|| "the file is too large for byte offsets".to_owned())?;
        scripts.push(Script {
            name: name.as_str().into(),
            value,
            range,
            pointer: pointer.into(),
        });
    }
    scripts.sort_by_key(|script| script.range.start);
    Ok(scripts)
}

fn file_range(start: usize, end: usize) -> Option<ByteRange> {
    ByteRange::new(u32::try_from(start).ok()?, u32::try_from(end).ok()?).ok()
}

/// One RFC 6901 reference token: `~` as `~0`, `/` as `~1`.
pub(crate) fn pointer_segment(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}

/// The value part of an assignment's text: everything after the first `=` outside quotes.
fn written_value(assignment: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in assignment.char_indices() {
        match quote {
            Some(open) => {
                if escaped {
                    escaped = false;
                } else if open == '"' && character == '\\' {
                    escaped = true;
                } else if character == open {
                    quote = None;
                }
            }
            None if character == '"' || character == '\'' => quote = Some(character),
            None if character == '=' => return assignment[index + 1..].trim(),
            None => {}
        }
    }
    assignment
}

/// Every `project.scripts.<name>` assignment's key path and byte span (in `body`).
fn assignments(body: &str) -> Vec<(Vec<String>, (usize, usize))> {
    let mut spans = Spans {
        body,
        header: None,
        frames: vec![Frame::Keys(Keys::new(Some(Vec::new())))],
        found: Vec::new(),
    };
    let tokens = Source::new(body).lex().into_vec();
    let mut errors: Vec<ParseError> = Vec::new();
    parse_document(&tokens, &mut spans, &mut errors);
    spans.found
}

/// The keys of the assignment being read at one table level.
struct Keys {
    /// The table's key path, or `None` inside an array, whose members have no pointer here.
    base: Option<Vec<String>>,
    keys: Vec<String>,
    start: usize,
    in_value: bool,
}

impl Keys {
    fn new(base: Option<Vec<String>>) -> Self {
        Self {
            base,
            keys: Vec::new(),
            start: 0,
            in_value: false,
        }
    }

    /// The path of the assignment being read, if it has one.
    fn path(&self) -> Option<Vec<String>> {
        let mut path = self.base.clone()?;
        path.extend(self.keys.iter().cloned());
        Some(path)
    }
}

enum Frame {
    Keys(Keys),
    Array,
}

struct Spans<'t> {
    body: &'t str,
    /// The keys of the table header being read.
    header: Option<Vec<String>>,
    frames: Vec<Frame>,
    found: Vec<(Vec<String>, (usize, usize))>,
}

impl Spans<'_> {
    fn decode(&self, span: Span, encoding: Option<Encoding>) -> String {
        let text = self.body.get(span.start()..span.end()).unwrap_or_default();
        let mut decoded = String::new();
        let mut errors: Vec<ParseError> = Vec::new();
        Raw::new_unchecked(text, encoding, span).decode_key(&mut decoded, &mut errors);
        decoded
    }

    /// The value of the assignment at the top level ends at `end`.
    fn value_end(&mut self, end: usize) {
        if let Some(Frame::Keys(keys)) = self.frames.last_mut()
            && keys.in_value
        {
            if let Some(path) = keys.path()
                && path.len() == 3
                && path[0] == "project"
                && path[1] == "scripts"
            {
                self.found.push((path, (keys.start, end)));
            }
            keys.keys.clear();
            keys.in_value = false;
        }
    }

    fn open_header(&mut self) {
        self.header = Some(Vec::new());
    }

    /// Ends a table header: assignments now belong to `base` (none for an array table).
    fn close_header(&mut self, tracked: bool) {
        let header = self.header.take().unwrap_or_default();
        self.frames.truncate(1);
        if let Some(Frame::Keys(root)) = self.frames.first_mut() {
            *root = Keys::new(tracked.then_some(header));
        }
    }
}

impl EventReceiver for Spans<'_> {
    fn std_table_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.open_header();
    }

    fn std_table_close(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.close_header(true);
    }

    fn array_table_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.open_header();
    }

    fn array_table_close(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        self.close_header(false);
    }

    fn inline_table_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) -> bool {
        let base = match self.frames.last() {
            Some(Frame::Keys(keys)) if keys.in_value => keys.path(),
            _ => None,
        };
        self.frames.push(Frame::Keys(Keys::new(base)));
        true
    }

    fn inline_table_close(&mut self, span: Span, _error: &mut dyn ErrorSink) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
        self.value_end(span.end());
    }

    fn array_open(&mut self, _span: Span, _error: &mut dyn ErrorSink) -> bool {
        self.frames.push(Frame::Array);
        true
    }

    fn array_close(&mut self, span: Span, _error: &mut dyn ErrorSink) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
        self.value_end(span.end());
    }

    fn simple_key(&mut self, span: Span, encoding: Option<Encoding>, _error: &mut dyn ErrorSink) {
        let key = self.decode(span, encoding);
        if let Some(header) = &mut self.header {
            header.push(key);
            return;
        }
        if let Some(Frame::Keys(keys)) = self.frames.last_mut()
            && !keys.in_value
        {
            if keys.keys.is_empty() {
                keys.start = span.start();
            }
            keys.keys.push(key);
        }
    }

    fn key_val_sep(&mut self, _span: Span, _error: &mut dyn ErrorSink) {
        if let Some(Frame::Keys(keys)) = self.frames.last_mut() {
            keys.in_value = true;
        }
    }

    fn scalar(&mut self, span: Span, _encoding: Option<Encoding>, _error: &mut dyn ErrorSink) {
        self.value_end(span.end());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slices(text: &str) -> Vec<(String, String, String)> {
        scripts(text)
            .unwrap()
            .into_iter()
            .map(|script| {
                let range = &text[script.range.start as usize..script.range.end as usize];
                (script.name.into(), script.pointer.into(), range.to_owned())
            })
            .collect()
    }

    #[test]
    fn header_assignments_keep_exact_ranges() {
        let text = "[project]\nname = \"x\"\n\n[project.scripts]\nrun = \"app:main\"  # comment\nother='app:other'\n";
        assert_eq!(
            slices(text),
            [
                (
                    "run".into(),
                    "/project/scripts/run".into(),
                    "run = \"app:main\"".into()
                ),
                (
                    "other".into(),
                    "/project/scripts/other".into(),
                    "other='app:other'".into()
                ),
            ]
        );
    }

    #[test]
    fn dotted_quoted_and_inline_keys_escape_their_pointers() {
        let text = "\u{feff}project.scripts.\"run/tilde~😀=key\" = \"app:main\" # c\r\nproject.scripts.'second.name' = 'app:main'\r\nproject.scripts.number = 1_000\r\n";
        let found = slices(text);
        assert_eq!(found[0].1, "/project/scripts/run~1tilde~0😀=key");
        assert_eq!(found[0].2, "project.scripts.\"run/tilde~😀=key\" = \"app:main\"");
        assert_eq!(found[1].1, "/project/scripts/second.name");
        assert_eq!(found[1].2, "project.scripts.'second.name' = 'app:main'");
        assert_eq!(found[2].2, "project.scripts.number = 1_000");
        let number = &scripts(text).unwrap()[2];
        assert_eq!(number.value, ScriptValue::Other("1_000".into()));
        let inline = "project.scripts = { \"inside/key\" = \"app:main\", b = [1, {c = 2}] }\n";
        let found = slices(inline);
        assert_eq!(found[0].1, "/project/scripts/inside~1key");
        assert_eq!(found[0].2, "\"inside/key\" = \"app:main\"");
        assert_eq!(found[1].2, "b = [1, {c = 2}]");
        let nested = "[project]\nscripts = { a = \"m:f\" }\n[[tool.x]]\nscripts.a = 1\n";
        assert_eq!(slices(nested)[0].2, "a = \"m:f\"");
    }

    #[test]
    fn invalid_documents_and_containers_are_reported() {
        assert!(scripts("[broken TOML").unwrap_err().starts_with("invalid TOML"));
        assert!(
            scripts("[project.scripts]\nrun=\"a:b\"\nrun=\"a:b\"\n")
                .unwrap_err()
                .starts_with("invalid TOML")
        );
        assert_eq!(
            scripts("project.scripts = \"app:main\"\n").unwrap_err(),
            "project.scripts must be a TOML table"
        );
        assert_eq!(
            scripts("project = 1\n").unwrap_err(),
            "project must be a TOML table"
        );
        assert_eq!(scripts("[project]\nname = \"x\"\n").unwrap(), []);
        assert_eq!(scripts("").unwrap(), []);
    }
}
