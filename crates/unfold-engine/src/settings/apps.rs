//! The `[[app]]` tables of `unfold.toml` (docs/designs/entrance-definitions.md, "The tables"):
//! apps and their entrances as written, each with its span and, when it is wrong, its problem.
//!
//! A problem marks only its own entry: an unknown field, a value of the wrong type, a missing
//! `name` (or, for an entrance, `code`), a name with no letters or digits, an empty `code` key,
//! or a key another entry has already. The entry is still listed, but it stays out of the graph; an app's problem
//! keeps its entrances out too. The rest of the file works.

use std::ops::Range;

use toml::Spanned;
use toml::de::{DeTable, DeValue};
use unfold_core::key::slug;
use unfold_core::{ByteRange, Key};

use super::SettingsError;
use super::parse::Source;

/// One `[[app]]` table as written: something a person runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    /// `entrance:<slug>` ([`Key::app`]), when the name makes one.
    pub key: Option<Key>,
    /// Its heading on Home.
    pub name: Option<Box<str>>,
    /// The command that starts it, as written; never run or parsed.
    pub run: Option<Box<str>>,
    /// Where that command starts, as written.
    pub code: Option<Key>,
    /// One sentence for its card.
    pub about: Option<Box<str>>,
    /// What it handles, in file order.
    pub entrances: Vec<EntranceEntry>,
    /// The table in the file.
    pub span: ByteRange,
    /// Why it stays out of the graph, with its entrances.
    pub problem: Option<EntryProblem>,
}

/// One entry of an app's `entrances` as written: something the app handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntranceEntry {
    /// `entrance:<app slug>/<entrance slug>` ([`Key::entrance`]), when both names make one.
    pub key: Option<Key>,
    /// What a person calls it.
    pub name: Option<Box<str>>,
    /// What starts it, as written; never parsed.
    pub on: Option<Box<str>>,
    /// The blocks it starts at, in the order they run, as written.
    pub code: Vec<Key>,
    /// One sentence for its card.
    pub about: Option<Box<str>>,
    /// The inline table in the file.
    pub span: ByteRange,
    /// Why it stays out of the graph.
    pub problem: Option<EntryProblem>,
}

impl AppEntry {
    /// Whether the app goes into the graph.
    pub fn is_valid(&self) -> bool {
        self.problem.is_none()
    }
}

impl EntranceEntry {
    /// Whether the entrance goes into the graph, given that its app does.
    pub fn is_valid(&self) -> bool {
        self.problem.is_none()
    }
}

/// What is wrong with one app or entrance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryProblem {
    /// What kind of problem.
    pub kind: EntryProblemKind,
    /// What to fix.
    pub message: Box<str>,
    /// The field at fault, or the whole entry.
    pub span: ByteRange,
}

/// The kinds of [`EntryProblem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntryProblemKind {
    /// A field an app or entrance doesn't have.
    UnknownField,
    /// A field whose value has the wrong type, or an entry that isn't a table.
    WrongType,
    /// No `name`.
    MissingName,
    /// An entrance with no `code`, or with an empty list.
    MissingCode,
    /// A name with no letters or digits, which makes no key, or an empty `code` key.
    EmptyKey,
    /// Another app, or another entrance in the same app, has the same key.
    DuplicateKey,
}

const APP_FIELDS: &str = "`name`, `run`, `code`, `about`, and `entrances`";
const ENTRANCE_FIELDS: &str = "`name`, `on`, `code`, and `about`";

/// Reads the value of the document's `app` key: an array of tables. Anything else fails the
/// file, since there are no entries to mark.
pub(super) fn parse(
    value: Spanned<DeValue<'_>>,
    source: &Source<'_>,
) -> Result<Vec<AppEntry>, SettingsError> {
    let span = value.span();
    let DeValue::Array(tables) = value.into_inner() else {
        return Err(source.error(span, "`app` must be a list of `[[app]]` tables"));
    };
    let mut apps: Vec<AppEntry> = tables.into_iter().map(|table| app(table, source)).collect();
    // A key already taken marks every later entry with it; the first keeps working.
    let mut seen: Vec<Key> = Vec::new();
    for app in &mut apps {
        if let Some(key) = &app.key {
            if seen.contains(key) {
                let problem = duplicate("app", key, app.span);
                app.problem.get_or_insert(problem);
            } else {
                seen.push(key.clone());
            }
        }
        let mut seen: Vec<Key> = Vec::new();
        for entrance in &mut app.entrances {
            if let Some(key) = &entrance.key {
                if seen.contains(key) {
                    let problem = duplicate("entrance in this app", key, entrance.span);
                    entrance.problem.get_or_insert(problem);
                } else {
                    seen.push(key.clone());
                }
            }
        }
    }
    Ok(apps)
}

fn duplicate(what: &str, key: &Key, span: ByteRange) -> EntryProblem {
    EntryProblem {
        kind: EntryProblemKind::DuplicateKey,
        message: format!("another {what} has the key `{key}`").into(),
        span,
    }
}

fn app(value: Spanned<DeValue<'_>>, source: &Source<'_>) -> AppEntry {
    let mut fields = Fields::new(value, source, "an `[[app]]` entry must be a table");
    let name = fields.text("name");
    let run = fields.text("run");
    let code = fields.key("code");
    let about = fields.text("about");
    let entrances = fields.take("entrances");
    fields.unknown(APP_FIELDS);
    let key = fields.name("app", name.as_deref(), Key::app);
    let entrances = match entrances {
        None => Vec::new(),
        Some(value) => {
            let span = value.span();
            match value.into_inner() {
                DeValue::Array(items) => items
                    .into_iter()
                    .map(|item| entrance(item, name.as_deref().filter(|_| key.is_some()), source))
                    .collect(),
                _ => {
                    fields.fail(
                        EntryProblemKind::WrongType,
                        span,
                        "`entrances` must be a list of inline tables",
                    );
                    Vec::new()
                }
            }
        }
    };
    let (span, problem) = fields.finish();
    AppEntry {
        key,
        name,
        run,
        code,
        about,
        entrances,
        span,
        problem,
    }
}

/// One entrance; `app` is its app's name when that makes a key.
fn entrance(value: Spanned<DeValue<'_>>, app: Option<&str>, source: &Source<'_>) -> EntranceEntry {
    let mut fields = Fields::new(value, source, "an entrance must be an inline table");
    let name = fields.text("name");
    let on = fields.text("on");
    let code = fields.keys("code");
    let about = fields.text("about");
    fields.unknown(ENTRANCE_FIELDS);
    let key = fields.name("entrance", name.as_deref(), |name| match app {
        Some(app) => Key::entrance(app, name).map(Some),
        None => slug(name).map(|_| None),
    });
    if code.is_none() {
        fields.missing(EntryProblemKind::MissingCode, "an entrance needs a `code`");
    }
    let (span, problem) = fields.finish();
    EntranceEntry {
        key: key.flatten(),
        name,
        on,
        code: code.unwrap_or_default(),
        about,
        span,
        problem,
    }
}

/// The fields of one entry, read one by one, keeping the entry's first problem.
struct Fields<'s, 'i> {
    source: &'s Source<'s>,
    table: DeTable<'i>,
    span: Range<usize>,
    problem: Option<EntryProblem>,
}

impl<'s, 'i> Fields<'s, 'i> {
    fn new(value: Spanned<DeValue<'i>>, source: &'s Source<'s>, not_a_table: &str) -> Self {
        let span = value.span();
        let mut fields = Self {
            source,
            table: DeTable::default(),
            span: span.clone(),
            problem: None,
        };
        match value.into_inner() {
            DeValue::Table(table) => {
                // A `[[app]]` table's own span is its header: it reaches to its last value.
                let end = table.iter().map(|(_, value)| value.span().end).max();
                fields.span.end = fields.span.end.max(end.unwrap_or(0));
                fields.table = table;
            }
            _ => fields.fail(EntryProblemKind::WrongType, span, not_a_table),
        }
        fields
    }

    fn fail(&mut self, kind: EntryProblemKind, span: Range<usize>, message: impl Into<Box<str>>) {
        if self.problem.is_none() {
            self.problem = Some(EntryProblem {
                kind,
                message: message.into(),
                span: self.source.range(span),
            });
        }
    }

    /// A problem with the entry as a whole.
    fn missing(&mut self, kind: EntryProblemKind, message: &str) {
        self.fail(kind, self.span.clone(), message);
    }

    fn take(&mut self, field: &str) -> Option<Spanned<DeValue<'i>>> {
        self.table.remove(field)
    }

    /// A string field.
    fn text(&mut self, field: &str) -> Option<Box<str>> {
        let value = self.take(field)?;
        match value.get_ref() {
            DeValue::String(text) => Some(text.as_ref().into()),
            _ => {
                self.fail(
                    EntryProblemKind::WrongType,
                    value.span(),
                    format!("`{field}` must be a string"),
                );
                None
            }
        }
    }

    /// A key: a string that isn't empty.
    fn key(&mut self, field: &str) -> Option<Key> {
        let span = self.table.get(field)?.span();
        let text = self.text(field)?;
        if text.is_empty() {
            self.empty_key(field, span);
        }
        Some(Key::from_text(&text))
    }

    /// A key, or a nonempty list of keys, none of them empty.
    fn keys(&mut self, field: &str) -> Option<Vec<Key>> {
        let value = self.take(field)?;
        let span = value.span();
        let items = match value.get_ref() {
            DeValue::String(_) => Some(std::slice::from_ref(&value)),
            DeValue::Array(items) => Some(&items[..]),
            _ => None,
        };
        let keys: Option<Vec<_>> = items.and_then(|items| {
            items
                .iter()
                .map(|item| match item.get_ref() {
                    DeValue::String(text) => Some((Key::from_text(text), item.span())),
                    _ => None,
                })
                .collect()
        });
        match keys {
            Some(keys) if keys.is_empty() => {
                self.fail(
                    EntryProblemKind::MissingCode,
                    span,
                    format!("`{field}` must name at least one key"),
                );
                None
            }
            Some(keys) => {
                if let Some((_, span)) = keys.iter().find(|(key, _)| key.as_str().is_empty()) {
                    self.empty_key(field, span.clone());
                }
                Some(keys.into_iter().map(|(key, _)| key).collect())
            }
            None => {
                self.fail(
                    EntryProblemKind::WrongType,
                    span,
                    format!("`{field}` must be a key or a list of keys"),
                );
                None
            }
        }
    }

    fn empty_key(&mut self, field: &str, span: Range<usize>) {
        self.fail(
            EntryProblemKind::EmptyKey,
            span,
            format!("`{field}` can't be an empty key"),
        );
    }

    /// Marks the first field left over as unknown.
    fn unknown(&mut self, known: &str) {
        let first = self
            .table
            .iter()
            .min_by_key(|(key, _)| key.span().start)
            .map(|(key, _)| (key.get_ref().to_string(), key.span()));
        if let Some((field, span)) = first {
            self.fail(
                EntryProblemKind::UnknownField,
                span,
                format!("unknown field `{field}`; use {known}"),
            );
        }
    }

    /// The entry's key from its name: missing and empty names are problems.
    fn name<K, E: std::fmt::Display>(
        &mut self,
        what: &str,
        name: Option<&str>,
        key: impl FnOnce(&str) -> Result<K, E>,
    ) -> Option<K> {
        let Some(name) = name else {
            self.missing(
                EntryProblemKind::MissingName,
                &format!("an {what} needs a `name`"),
            );
            return None;
        };
        match key(name) {
            Ok(key) => Some(key),
            Err(error) => {
                self.missing(EntryProblemKind::EmptyKey, &error.to_string());
                None
            }
        }
    }

    fn finish(self) -> (ByteRange, Option<EntryProblem>) {
        (self.source.range(self.span), self.problem)
    }
}
