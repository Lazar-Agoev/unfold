//! Validated edits to `unfold.toml` that keep its comments and formatting
//! (docs/engine.md#settings-edits).
//!
//! Each [`SettingsEdit`] changes the TOML document through `toml_edit`, so everything the edit
//! doesn't touch (comments, blank lines, key order, quoting) stays byte for byte, and the file
//! keeps its byte order mark and its line endings (those of its first line, CRLF or LF). The
//! result is then parsed with the strict settings parser; an edit that would make the file
//! invalid, or would leave the app or entrance it sets with an empty key, is refused with the
//! parser's message and position. [`apply_edits`] writes nothing: it returns the new text, what
//! it holds, and the diff from the old text.
//!
//! Apps and entrances are addressed by name or key ([`names_app`], [`names_entrance`]). A name
//! stands for the entry whose key it makes ([`Key::app`], [`Key::entrance`]), so `"shop api"`
//! addresses the app named `"Shop API"`, and so does `entrance:shop-api`. An edit never renames:
//! an entry it changes keeps its name, and a new one takes the name given (a key adds nothing).
//! An app's or entrance's field left out of an edit keeps what the file says; null removes it.
//! Each app and entrance edit reports what it did ([`EntryEdit`]). Keys are written as TOML
//! strings, in single quotes when they contain `"`.
//!
//! | Edit | Effect |
//! | --- | --- |
//! | Exclude from graph | Removes the path from `include` if listed there (and others remain), otherwise adds it to `exclude` |
//! | Include in graph | Removes the path from `exclude` if listed there; adds it to a nonempty `include` when that list doesn't cover it. A broader `exclude` pattern is a conflict, reported, never silently rewritten |
//! | Graph only this folder | Sets `include` to the one folder |
//! | Remove a scope pattern | Removes one pattern, exactly as written, from `include` or `exclude`; an emptied list's key goes |
//! | Python project | Adds or replaces the `[[python.project]]` table with that root, or removes it |
//! | Evidence files | Adds a pattern to `[evidence] files` (unless it is listed), or removes one exactly as written; an emptied list's key goes, and so does the emptied table |
//! | Set app | Adds an `[[app]]` table, or sets or removes the given ones of an existing app's `run`, `code`, and `about`; its entrances stay |
//! | Set entrance | Adds one line to an app's `entrances`, creating the app when it is new, or sets or removes the given fields of the entrance addressed (its line is rewritten when one changes) |
//! | Add entrance | Adds one line, as Set entrance does, and refuses when the app has an entrance with that key |
//! | Remove entrance | Removes one entrance's line (an emptied list's key goes), or a whole app |
//! | Replace code | Rewrites one `code` key of an app or entrance in place (a fix, or Choose code…), keeping the rest of its line; refused when the file no longer has the key it replaces at that place |

use serde::{Deserialize, Deserializer, Serialize};
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};
use unfold_core::Key;
use unfold_core::key::{ParsedKey, SlugError};

use super::diff::unified;
use super::parse::entry_error;
use super::{EntryProblem, EntryProblemKind, FolderPath, ScopeStatus, SettingsError, SettingsFile};

/// One change to the settings file. See the [module documentation](self).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "op", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum SettingsEdit {
    /// Exclude a file or folder from the graph.
    ExcludeFromGraph {
        /// The folder-relative path.
        path: String,
    },
    /// Include a file or folder in the graph.
    IncludeInGraph {
        /// The folder-relative path.
        path: String,
    },
    /// Graph only this folder.
    GraphOnly {
        /// The folder-relative path.
        path: String,
    },
    /// Remove one `[scope]` pattern exactly as written.
    RemoveScopePattern {
        /// Which list: `include` or `exclude`.
        list: ScopeList,
        /// The pattern, as the settings list it.
        pattern: String,
    },
    /// Add or replace the Python project with this root.
    SetPythonProject {
        /// The project.
        project: PythonProjectSpec,
    },
    /// Remove the Python project with this root.
    RemovePythonProject {
        /// The project's root.
        root: String,
    },
    /// Add a pattern to `[evidence] files`, unless it is listed already.
    AddEvidencePattern {
        /// The pattern: a file, a folder, or a glob, relative to the project's folder.
        pattern: String,
    },
    /// Remove one `[evidence] files` pattern exactly as written.
    RemoveEvidencePattern {
        /// The pattern, as the settings list it.
        pattern: String,
    },
    /// Add an app, or set or remove an existing app's `run`, `code`, and `about`.
    SetApp {
        /// The app.
        app: AppSpec,
    },
    /// Add one entrance to an app, creating the app when it is new, or change the entrance
    /// addressed.
    SetEntrance {
        /// The app's name or key.
        app: String,
        /// The entrance.
        entrance: EntranceSpec,
    },
    /// Add one entrance to an app, creating the app when it is new; refused when the app has
    /// an entrance with that key.
    AddEntrance {
        /// The app's name or key.
        app: String,
        /// The entrance.
        entrance: EntranceSpec,
    },
    /// Remove one entrance of an app, or the whole app.
    RemoveEntrance {
        /// The app's name or key.
        app: String,
        /// The entrance's name or key; `None` removes the app.
        entrance: Option<String>,
    },
    /// Rewrite one `code` key of an app or entrance, the one at `index`, when it is still `old`.
    ReplaceCode {
        /// The app's name or key.
        app: String,
        /// The entrance's name or key; `None` for the app's own `code`.
        entrance: Option<String>,
        /// The key's place in the `code` list; 0 for an app's `code` or a single key.
        index: u32,
        /// The key the file has there, as it was shown.
        old: String,
        /// The key to write in its place.
        code: String,
    },
}

/// One of the `[scope]` pattern lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ScopeList {
    /// `[scope] include`.
    Include,
    /// `[scope] exclude`.
    Exclude,
}

impl ScopeList {
    fn key(self) -> &'static str {
        match self {
            Self::Include => "include",
            Self::Exclude => "exclude",
        }
    }
}

/// A `[[python.project]]` table as the editor submits it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default)]
pub struct PythonProjectSpec {
    /// The project folder, relative to the open folder.
    pub root: String,
    /// Import roots relative to `root`; `None` keeps the front end's default.
    pub source_roots: Option<Vec<String>>,
    /// The grammar version, such as `3.12`; `None` means the default.
    pub version: Option<String>,
}

/// A field that is absent (`None`: keep it), null (`Some(None)`: remove it), or a value.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> std::result::Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

/// An `[[app]]` table as an edit submits it: everything but its entrances. A field left out
/// keeps what the table says; null removes it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default)]
pub struct AppSpec {
    /// The app's name or key, which addresses it. A new app is written with this name; an
    /// existing one keeps its own.
    pub name: String,
    /// The command that starts it.
    #[serde(deserialize_with = "present")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub run: Option<Option<String>>,
    /// The block key where that command starts.
    #[serde(deserialize_with = "present")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub code: Option<Option<String>>,
    /// One sentence for its card.
    #[serde(deserialize_with = "present")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub about: Option<Option<String>>,
}

/// One entrance as an edit submits it. A field left out keeps what the entrance says; null
/// removes `on` or `about`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default)]
pub struct EntranceSpec {
    /// The entrance's name or key, which addresses it within its app. A new entrance is
    /// written with this name; an existing one keeps its own.
    pub name: String,
    /// What starts it.
    #[serde(deserialize_with = "present")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub on: Option<Option<String>>,
    /// The block keys it starts at, in order; at least one, and a new entrance needs them. One
    /// key is written as a string, several as a list.
    #[cfg_attr(feature = "ts", ts(optional))]
    pub code: Option<Vec<String>>,
    /// One sentence for its card.
    #[serde(deserialize_with = "present")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub about: Option<Option<String>>,
}

/// What an app or entrance edit did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum EntryChange {
    /// It added the entry.
    Added,
    /// It changed the entry the file has (or left it as it was).
    Changed,
    /// It removed the entry.
    Removed,
}

/// What one app or entrance edit did, named as the file writes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct EntryEdit {
    /// What it did to the entry.
    pub change: EntryChange,
    /// The app's name.
    pub app: String,
    /// The entrance's name; absent for an edit of the app itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub entrance: Option<String>,
    /// Whether the edit added the app (with its first entrance, for an entrance edit).
    pub new_app: bool,
}

/// Whether `given`, an app's name or key, addresses the app whose key is `key`: it is that key,
/// or a name that makes it.
pub fn names_app(key: &Key, given: &str) -> bool {
    key.as_str() == given || Key::app(given).is_ok_and(|made| made == *key)
}

/// Whether `given`, an entrance's name or key, addresses the entrance of the app named `app`
/// whose key is `key`.
pub fn names_entrance(key: &Key, app: &str, given: &str) -> bool {
    key.as_str() == given || Key::entrance(app, given).is_ok_and(|made| made == *key)
}

/// Whether `given` is written as an app's or entrance's key, which only ever addresses an
/// entry the file has.
fn is_key(given: &str) -> bool {
    matches!(
        Key::from_text(given).parse(),
        Ok(ParsedKey::App { .. } | ParsedKey::Entrance { .. })
    )
}

/// Why an edit was refused. Nothing is written.
#[derive(Debug, thiserror::Error)]
pub enum EditError {
    /// The file as it is doesn't parse, so it can't be edited safely.
    #[error("fix unfold.toml first: {0}")]
    Unparsable(SettingsError),
    /// The edit would make the file invalid.
    #[error(transparent)]
    Invalid(SettingsError),
    /// The edit doesn't fit the file: what it names isn't there, or the file is written in a
    /// shape the editor doesn't change.
    #[error("{0}")]
    Conflict(Box<str>),
}

type Result<T, E = EditError> = std::result::Result<T, E>;

fn conflict(message: impl Into<Box<str>>) -> EditError {
    EditError::Conflict(message.into())
}

/// The settings file after a set of edits.
#[derive(Debug, Clone)]
pub struct EditedSettings {
    /// The new file text.
    pub text: String,
    /// What it holds.
    pub file: SettingsFile,
    /// The unified diff from the old text (empty for an absent file) to the new; empty when
    /// the edits changed nothing.
    pub diff: String,
    /// What each app and entrance edit did, in order.
    pub entries: Vec<EntryEdit>,
}

/// Applies `edits` to the settings text `current` (`None` when the file is absent) and
/// validates the result. Writes nothing.
pub fn apply_edits(current: Option<&str>, edits: &[SettingsEdit]) -> Result<EditedSettings> {
    let source = current.unwrap_or("");
    if current.is_some() {
        SettingsFile::parse(source).map_err(EditError::Unparsable)?;
    }
    let (bom, body) = match source.strip_prefix('\u{FEFF}') {
        Some(body) => ("\u{FEFF}", body),
        None => ("", source),
    };
    let ending = line_ending(body);
    let mut doc: DocumentMut = body
        .parse()
        .map_err(|error: toml_edit::TomlError| conflict(format!("fix unfold.toml first: {error}")))?;
    let mut entries = Vec::new();
    for edit in edits {
        entries.extend(apply(&mut doc, edit)?);
    }
    let text = format!("{bom}{}", with_line_endings(doc.to_string(), ending));
    let file = SettingsFile::parse(&text).map_err(EditError::Invalid)?;
    if let Some(problem) = entries.iter().find_map(|entry| empty_key_written(&file, entry)) {
        return Err(EditError::Invalid(entry_error(&text, problem)));
    }
    let diff = unified(source, &text);
    Ok(EditedSettings {
        text,
        file,
        diff,
        entries,
    })
}

/// The line ending `text` uses: its first line's, and `\n` for a single line.
fn line_ending(text: &str) -> &'static str {
    match text.find('\n') {
        Some(at) if text[..at].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

/// `text` (which `toml_edit` writes with `\n`) with every line ending written as `ending`.
fn with_line_endings(text: String, ending: &str) -> String {
    if ending == "\n" {
        return text;
    }
    text.replace("\r\n", "\n").replace('\n', ending)
}

/// The empty-key problem of the app or entrance an edit set, as `file` now holds it. Only that
/// entry is checked: a problem elsewhere was there before the edit.
fn empty_key_written<'f>(file: &'f SettingsFile, entry: &EntryEdit) -> Option<&'f EntryProblem> {
    if entry.change == EntryChange::Removed {
        return None;
    }
    let app_key = Key::app(&entry.app).ok()?;
    let written = file.apps.iter().find(|app| app.key.as_ref() == Some(&app_key))?;
    let problem = match &entry.entrance {
        None => written.problem.as_ref(),
        Some(entrance) => {
            let key = Key::entrance(&entry.app, entrance).ok()?;
            written
                .entrances
                .iter()
                .find(|entry| entry.key.as_ref() == Some(&key))?
                .problem
                .as_ref()
        }
    };
    problem.filter(|problem| problem.kind == EntryProblemKind::EmptyKey)
}

/// Applies one edit; an app or entrance edit says what it did.
fn apply(doc: &mut DocumentMut, edit: &SettingsEdit) -> Result<Option<EntryEdit>> {
    match edit {
        SettingsEdit::ExcludeFromGraph { path } => exclude(doc, path)?,
        SettingsEdit::IncludeInGraph { path } => include(doc, path)?,
        SettingsEdit::GraphOnly { path } => {
            let mut include = Array::new();
            include.push(escape_glob(path));
            table(doc, "scope")?.insert("include", Item::Value(Value::Array(include)));
        }
        SettingsEdit::RemoveScopePattern { list, pattern } => remove_pattern(doc, *list, pattern)?,
        SettingsEdit::SetPythonProject { project } => set_python_project(doc, project)?,
        SettingsEdit::RemovePythonProject { root } => {
            let root = folder(root)?;
            let projects = python_projects(doc)?;
            let position = (0..projects.len())
                .find(|&index| {
                    projects
                        .get(index)
                        .is_some_and(|table| table_root(table) == Some(root.clone()))
                })
                .ok_or_else(|| conflict(format!("no Python project has root `{root}`")))?;
            projects.remove(position);
        }
        SettingsEdit::AddEvidencePattern { pattern } => {
            let files = array(table(doc, "evidence")?, "files")?;
            if position(files, pattern).is_none() {
                files.push(pattern.as_str());
            }
        }
        SettingsEdit::RemoveEvidencePattern { pattern } => remove_evidence(doc, pattern)?,
        SettingsEdit::SetApp { app } => return set_app(doc, app).map(Some),
        SettingsEdit::SetEntrance { app, entrance } => {
            return set_entrance(doc, app, entrance, true).map(Some);
        }
        SettingsEdit::AddEntrance { app, entrance } => {
            return set_entrance(doc, app, entrance, false).map(Some);
        }
        SettingsEdit::RemoveEntrance { app, entrance } => {
            return remove_entrance(doc, app, entrance.as_deref()).map(Some);
        }
        SettingsEdit::ReplaceCode {
            app,
            entrance,
            index,
            old,
            code,
        } => {
            return replace_code(doc, app, entrance.as_deref(), *index, old, code).map(Some);
        }
    }
    Ok(None)
}
/// Removes `pattern`, exactly as written, from `[evidence] files`; an emptied list's key goes,
/// and so does the `[evidence]` table it emptied.
fn remove_evidence(doc: &mut DocumentMut, pattern: &str) -> Result<()> {
    let missing = || conflict(format!("`{pattern}` is not in `[evidence] files`"));
    let evidence = doc
        .get_mut("evidence")
        .and_then(Item::as_table_like_mut)
        .ok_or_else(missing)?;
    let files = evidence
        .get_mut("files")
        .and_then(Item::as_array_mut)
        .ok_or_else(missing)?;
    let index = position(files, pattern).ok_or_else(missing)?;
    files.remove(index);
    if files.is_empty() {
        evidence.remove("files");
    }
    if evidence.is_empty() {
        doc.remove("evidence");
    }
    Ok(())
}

/// The scope as the document says now.
fn scope_of(doc: &DocumentMut) -> Result<super::Scope> {
    Ok(SettingsFile::parse(&doc.to_string())
        .map_err(EditError::Invalid)?
        .settings
        .scope)
}

fn exclude(doc: &mut DocumentMut, path: &str) -> Result<()> {
    let pattern = escape_glob(path);
    if let Some(include) = existing_array(doc, "include")
        && include.len() > 1
        && let Some(index) = position(include, &pattern)
    {
        include.remove(index);
        return Ok(());
    }
    if scope_of(doc)?.status_of(path) != ScopeStatus::InScope {
        return Ok(());
    }
    array(table(doc, "scope")?, "exclude")?.push(pattern);
    Ok(())
}

fn include(doc: &mut DocumentMut, path: &str) -> Result<()> {
    let pattern = escape_glob(path);
    if let Some(exclude) = existing_array(doc, "exclude")
        && let Some(index) = position(exclude, &pattern)
    {
        exclude.remove(index);
    }
    let scope = scope_of(doc)?;
    match scope.status_of(path) {
        ScopeStatus::InScope => Ok(()),
        ScopeStatus::NotIncluded => {
            array(table(doc, "scope")?, "include")?.push(pattern);
            Ok(())
        }
        ScopeStatus::Excluded => {
            let patterns = scope.exclude().join("`, `");
            Err(conflict(format!(
                "`{path}` is excluded by a broader pattern (`{patterns}`); edit unfold.toml to include it"
            )))
        }
    }
}

fn remove_pattern(doc: &mut DocumentMut, list: ScopeList, pattern: &str) -> Result<()> {
    let key = list.key();
    let missing = || conflict(format!("`{pattern}` is not in `[scope] {key}`"));
    let patterns = existing_array(doc, key).ok_or_else(missing)?;
    let index = position(patterns, pattern).ok_or_else(missing)?;
    patterns.remove(index);
    if patterns.is_empty()
        && let Some(scope) = doc.get_mut("scope").and_then(Item::as_table_like_mut)
    {
        scope.remove(key);
    }
    Ok(())
}

/// `[scope] <key>`, if the document has it.
fn existing_array<'d>(doc: &'d mut DocumentMut, key: &str) -> Option<&'d mut Array> {
    doc.get_mut("scope")?.get_mut(key)?.as_array_mut()
}

fn position(array: &Array, text: &str) -> Option<usize> {
    array.iter().position(|value| value.as_str() == Some(text))
}

/// A path as a scope pattern that matches exactly it: glob metacharacters escaped.
fn escape_glob(path: &str) -> String {
    let mut pattern = String::with_capacity(path.len());
    for character in path.chars() {
        if matches!(character, '*' | '?' | '[' | ']' | '{' | '}' | '\\') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern
}

/// The table at `key`, created (as a standard table) if absent.
fn table<'d>(doc: &'d mut DocumentMut, key: &str) -> Result<&'d mut Table> {
    let item = doc.entry(key).or_insert_with(|| Item::Table(Table::new()));
    item.as_table_mut()
        .ok_or_else(|| conflict(format!("`{key}` in unfold.toml is not a table")))
}

/// The array at `key` in `table`, created if absent.
fn array<'t>(table: &'t mut Table, key: &str) -> Result<&'t mut Array> {
    let item = table
        .entry(key)
        .or_insert_with(|| Item::Value(Value::Array(Array::new())));
    item.as_array_mut()
        .ok_or_else(|| conflict(format!("`{key}` in unfold.toml is not an array")))
}

fn folder(text: &str) -> Result<FolderPath> {
    FolderPath::parse(text).map_err(|error| conflict(format!("`{text}`: {error}")))
}

fn table_root(table: &Table) -> Option<FolderPath> {
    let root = table.get("root").and_then(Item::as_str).unwrap_or(".");
    FolderPath::parse(root).ok()
}

fn python_projects(doc: &mut DocumentMut) -> Result<&mut ArrayOfTables> {
    let python = table(doc, "python")?;
    // `[python]` holds only `[[python.project]]`; keep it implicit so no empty header appears.
    python.set_implicit(true);
    let item = python
        .entry("project")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
    item.as_array_of_tables_mut()
        .ok_or_else(|| conflict("`python.project` is not a list of tables"))
}

fn set_python_project(doc: &mut DocumentMut, spec: &PythonProjectSpec) -> Result<()> {
    let root = folder(&spec.root)?;
    let mut fresh = Table::new();
    fresh.insert("root", toml_edit::value(root.as_str()));
    if let Some(source_roots) = &spec.source_roots {
        fresh.insert(
            "source_roots",
            Item::Value(Value::Array(source_roots.iter().map(String::as_str).collect())),
        );
    }
    if let Some(version) = &spec.version {
        fresh.insert("version", toml_edit::value(version.as_str()));
    }
    let projects = python_projects(doc)?;
    let existing = (0..projects.len()).find(|&index| {
        projects
            .get(index)
            .is_some_and(|table| table_root(table) == Some(root.clone()))
    });
    match existing.and_then(|index| projects.get_mut(index)) {
        Some(table) => {
            table.clear();
            for (key, value) in fresh {
                table.insert(&key, value);
            }
        }
        None => projects.push(fresh),
    }
    Ok(())
}

/// The conflict for a name that makes no key; `whose` is "an app's" or "an entrance's".
fn no_key(whose: &str) -> impl FnOnce(SlugError) -> EditError + '_ {
    move |error| conflict(format!("{whose} {error}"))
}

/// The `[[app]]` tables, if the document has any.
fn apps(doc: &mut DocumentMut) -> Result<Option<&mut ArrayOfTables>> {
    match doc.get_mut("app") {
        None => Ok(None),
        Some(item) => item
            .as_array_of_tables_mut()
            .map(Some)
            .ok_or_else(|| conflict("`app` in unfold.toml is not written as `[[app]]` tables")),
    }
}

/// An app table's `name`, when it is a string.
fn name_of(table: &Table) -> Option<&str> {
    table.get("name").and_then(Item::as_str)
}

/// An entrance line's `name`, when it is a string.
fn line_name(line: &Value) -> Option<&str> {
    line.as_inline_table()?.get("name")?.as_str()
}

/// The position of the first app that `given` (its name or key) addresses.
fn app_position(apps: &ArrayOfTables, given: &str) -> Option<usize> {
    apps.iter().position(|table| {
        name_of(table)
            .and_then(|name| Key::app(name).ok())
            .is_some_and(|key| names_app(&key, given))
    })
}

/// The position of the first entrance of the app named `app` that `given` (its name or key)
/// addresses.
fn entrance_position(lines: &Array, app: &str, given: &str) -> Option<usize> {
    lines.iter().position(|line| {
        line_name(line)
            .and_then(|name| Key::entrance(app, name).ok())
            .is_some_and(|key| names_entrance(&key, app, given))
    })
}

/// Names for a refusal: `A, B`, or `none`.
fn listed<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let names: Vec<&str> = names.collect();
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join(", ")
    }
}

/// The refusal for an app the file doesn't have, listing those it has.
fn no_app(apps: Option<&ArrayOfTables>, given: &str) -> EditError {
    let names = listed(apps.into_iter().flat_map(ArrayOfTables::iter).filter_map(name_of));
    conflict(format!("no app “{given}” is in unfold.toml. Its apps: {names}"))
}

/// The refusal for an entrance the app named `app` doesn't have, listing those it has.
fn no_entrance(lines: Option<&Array>, app: &str, given: &str) -> EditError {
    let names = listed(lines.into_iter().flat_map(Array::iter).filter_map(line_name));
    conflict(format!(
        "the app {app} has no entrance “{given}”. Its entrances: {names}"
    ))
}

/// The app `given` (its name or key) addresses, and whether the edit adds it: a name no app's
/// key matches adds an app with just that name.
fn app_table<'d>(doc: &'d mut DocumentMut, given: &str) -> Result<(&'d mut Table, bool)> {
    let written = doc.iter().next().is_some();
    let item = doc
        .entry("app")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
    let apps = item
        .as_array_of_tables_mut()
        .ok_or_else(|| conflict("`app` in unfold.toml is not written as `[[app]]` tables"))?;
    let (index, added) = match app_position(apps, given) {
        Some(index) => (index, false),
        None if is_key(given) => return Err(no_app(Some(apps), given)),
        None => {
            Key::app(given).map_err(no_key("an app's"))?;
            let mut table = Table::new();
            if written {
                table.decor_mut().set_prefix("\n");
            }
            table.insert("name", toml_edit::value(given));
            apps.push(table);
            (apps.len() - 1, true)
        }
    };
    let table = apps.get_mut(index).ok_or_else(|| no_app(None, given))?;
    Ok((table, added))
}

fn set_app(doc: &mut DocumentMut, spec: &AppSpec) -> Result<EntryEdit> {
    let (table, added) = app_table(doc, &spec.name)?;
    for (field, value) in [("run", &spec.run), ("code", &spec.code), ("about", &spec.about)] {
        if let Some(value) = value {
            set_field(table, field, value.as_deref());
        }
    }
    Ok(EntryEdit {
        change: if added {
            EntryChange::Added
        } else {
            EntryChange::Changed
        },
        app: name_of(table).unwrap_or(&spec.name).to_owned(),
        entrance: None,
        new_app: added,
    })
}

/// Sets `field` of an app table to `value`, keeping the value's place and spacing, or removes
/// it. A new field goes before a trailing `entrances` list.
fn set_field(table: &mut Table, field: &str, value: Option<&str>) {
    let Some(value) = value else {
        table.remove(field);
        return;
    };
    if let Some(item) = table.get_mut(field) {
        if item.as_str() != Some(value) {
            let mut fresh = Value::from(value);
            if let Some(old) = item.as_value() {
                *fresh.decor_mut() = old.decor().clone();
            }
            *item = Item::Value(fresh);
        }
        return;
    }
    let last = table.iter().last().map(|(key, _)| key == "entrances");
    let entrances = if last == Some(true) {
        table.remove_entry("entrances")
    } else {
        None
    };
    table.insert(field, toml_edit::value(value));
    if let Some((key, item)) = entrances {
        table.insert_formatted(&key, item);
    }
}

/// Adds one entrance to the app `app` (its name or key) addresses, creating the app when it is
/// new; when `change` allows it, an entrance `spec` addresses is changed instead of refused.
fn set_entrance(doc: &mut DocumentMut, app: &str, spec: &EntranceSpec, change: bool) -> Result<EntryEdit> {
    if spec.code.as_ref().is_some_and(Vec::is_empty) {
        return Err(conflict(format!(
            "entrance “{}” needs at least one code key",
            spec.name
        )));
    }
    let (table, new_app) = app_table(doc, app)?;
    let app_name = name_of(table).unwrap_or(app).to_owned();
    let lines = entrances(table)?;
    let done = |change, entrance: &str| EntryEdit {
        change,
        app: app_name.clone(),
        entrance: Some(entrance.to_owned()),
        new_app,
    };
    if let Some(index) = entrance_position(lines, &app_name, &spec.name) {
        let line = lines.get(index).and_then(Value::as_inline_table);
        let name = line
            .and_then(|line| line.get("name"))
            .and_then(Value::as_str)
            .unwrap_or(&spec.name)
            .to_owned();
        if !change {
            return Err(conflict(format!(
                "{app_name} already has the entrance “{name}”; give this one another name"
            )));
        }
        if let Some(fresh) = line.and_then(|line| changed_line(line, spec)) {
            lines.replace(index, fresh);
        }
        return Ok(done(EntryChange::Changed, &name));
    }
    if is_key(&spec.name) {
        return Err(no_entrance(Some(lines), &app_name, &spec.name));
    }
    Key::entrance(&app_name, &spec.name).map_err(no_key("an entrance's"))?;
    let Some(code) = &spec.code else {
        return Err(conflict(format!(
            "“{}” is a new entrance of {app_name}, and a new entrance needs code: the block it starts at",
            spec.name
        )));
    };
    // A new line is indented like the last one (without its comments), or goes on its own line.
    let mut line = Value::InlineTable(entrance_line(spec, code));
    let prefix = match lines.iter().last() {
        Some(last) => {
            let written = last
                .decor()
                .prefix()
                .and_then(|prefix| prefix.as_str())
                .unwrap_or(" ");
            match written.rfind('\n') {
                Some(at) => format!("\n{}", &written[at + 1..]),
                None => " ".to_owned(),
            }
        }
        None => "\n  ".to_owned(),
    };
    line.decor_mut().set_prefix(prefix);
    if lines.is_empty() {
        lines.set_trailing("\n");
        lines.set_trailing_comma(true);
    }
    lines.push_formatted(line);
    Ok(done(EntryChange::Added, &spec.name))
}

/// An app's `entrances` list, created empty if absent.
fn entrances(table: &mut Table) -> Result<&mut Array> {
    let item = table
        .entry("entrances")
        .or_insert_with(|| Item::Value(Value::Array(Array::new())));
    item.as_array_mut()
        .ok_or_else(|| conflict("this app's `entrances` is not a list of inline tables"))
}

/// `code` as a line writes it: one key as a string, several as a list.
fn code_value(keys: &[String]) -> Value {
    match keys {
        [one] => Value::from(one.as_str()),
        many => Value::Array(many.iter().map(String::as_str).collect()),
    }
}

/// A new entrance as an inline table: `{ name, on, code, about }`.
fn entrance_line(spec: &EntranceSpec, code: &[String]) -> InlineTable {
    let mut line = InlineTable::new();
    line.insert("name", Value::from(spec.name.as_str()));
    if let Some(Some(on)) = &spec.on {
        line.insert("on", Value::from(on.as_str()));
    }
    line.insert("code", code_value(code));
    if let Some(Some(about)) = &spec.about {
        line.insert("about", Value::from(about.as_str()));
    }
    line.fmt();
    line
}

/// A value's text without its formatting, to tell whether an edit changes it.
fn bare(value: &Value) -> String {
    let mut value = value.clone();
    value.decor_mut().clear();
    if let Value::Array(array) = &mut value {
        array.fmt();
    }
    value.to_string()
}

/// An entrance's line after `spec`, or `None` when `spec` changes none of its fields. Its name
/// stays, and so do the fields `spec` leaves out and any others the line has.
fn changed_line(line: &InlineTable, spec: &EntranceSpec) -> Option<InlineTable> {
    let given = |field: &str, value: &Option<Option<String>>| match value {
        None => line.get(field).cloned(),
        Some(value) => value.as_deref().map(Value::from),
    };
    let code = match &spec.code {
        None => line.get("code").cloned(),
        Some(keys) => Some(code_value(keys)),
    };
    let fields = [
        ("on", given("on", &spec.on)),
        ("code", code),
        ("about", given("about", &spec.about)),
    ];
    if fields
        .iter()
        .all(|(field, value)| line.get(field).map(bare) == value.as_ref().map(bare))
    {
        return None;
    }
    let mut fresh = InlineTable::new();
    if let Some(name) = line.get("name") {
        fresh.insert("name", name.clone());
    }
    for (field, value) in fields {
        if let Some(value) = value {
            fresh.insert(field, value);
        }
    }
    for (key, value) in line {
        if !matches!(key, "name" | "on" | "code" | "about") {
            fresh.insert(key, value.clone());
        }
    }
    fresh.fmt();
    Some(fresh)
}

fn remove_entrance(doc: &mut DocumentMut, app: &str, entrance: Option<&str>) -> Result<EntryEdit> {
    let Some(apps) = apps(doc)? else {
        return Err(no_app(None, app));
    };
    let index = app_position(apps, app).ok_or_else(|| no_app(Some(apps), app))?;
    let app_name = apps.get(index).and_then(name_of).unwrap_or(app).to_owned();
    let Some(entrance) = entrance else {
        apps.remove(index);
        if apps.is_empty() {
            doc.remove("app");
        }
        return Ok(EntryEdit {
            change: EntryChange::Removed,
            app: app_name,
            entrance: None,
            new_app: false,
        });
    };
    let table = apps.get_mut(index).ok_or_else(|| no_app(None, app))?;
    let Some(lines) = table.get_mut("entrances").and_then(Item::as_array_mut) else {
        return Err(no_entrance(None, &app_name, entrance));
    };
    let position = entrance_position(lines, &app_name, entrance)
        .ok_or_else(|| no_entrance(Some(lines), &app_name, entrance))?;
    let name = lines
        .get(position)
        .and_then(line_name)
        .unwrap_or(entrance)
        .to_owned();
    lines.remove(position);
    if lines.is_empty() {
        table.remove("entrances");
    }
    Ok(EntryEdit {
        change: EntryChange::Removed,
        app: app_name,
        entrance: Some(name),
        new_app: false,
    })
}

/// Writes `code` in place of the `code` key at `index` of the entrance `entrance` of the app
/// `app` (or of the app itself when `entrance` is `None`), which must be `old`, keeping its
/// formatting and everything else in the line; refused when the file has another key there (or
/// none).
fn replace_code(
    doc: &mut DocumentMut,
    app: &str,
    entrance: Option<&str>,
    index: u32,
    old: &str,
    code: &str,
) -> Result<EntryEdit> {
    let Some(apps) = apps(doc)? else {
        return Err(no_app(None, app));
    };
    let position = app_position(apps, app).ok_or_else(|| no_app(Some(apps), app))?;
    let table = apps.get_mut(position).ok_or_else(|| no_app(None, app))?;
    let app_name = name_of(table).unwrap_or(app).to_owned();
    let (value, entrance) = match entrance {
        None => (table.get_mut("code").and_then(Item::as_value_mut), None),
        Some(given) => {
            let Some(lines) = table.get_mut("entrances").and_then(Item::as_array_mut) else {
                return Err(no_entrance(None, &app_name, given));
            };
            let position = entrance_position(lines, &app_name, given)
                .ok_or_else(|| no_entrance(Some(lines), &app_name, given))?;
            let line = lines.get_mut(position).and_then(Value::as_inline_table_mut);
            let name = line
                .as_ref()
                .and_then(|line| line.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(given)
                .to_owned();
            (line.and_then(|line| line.get_mut("code")), Some(name))
        }
    };
    let key = match value {
        Some(Value::Array(keys)) => usize::try_from(index).ok().and_then(|index| keys.get_mut(index)),
        Some(key) if index == 0 => Some(key),
        _ => None,
    };
    let Some(key) = key.filter(|key| key.as_str() == Some(old)) else {
        let whose = match &entrance {
            Some(name) => format!("“{name}” in {app_name}"),
            None => app_name,
        };
        return Err(conflict(format!(
            "the code of {whose} no longer has `{old}` in that place: unfold.toml changed since it was shown"
        )));
    };
    let mut fresh = Value::from(code);
    *fresh.decor_mut() = key.decor().clone();
    *key = fresh;
    Ok(EntryEdit {
        change: EntryChange::Changed,
        app: app_name,
        entrance,
        new_app: false,
    })
}

#[cfg(test)]
mod tests;
