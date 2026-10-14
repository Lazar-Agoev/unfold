//! Validated edits to `unfold.toml` that keep its comments and formatting (architecture
//! §9.2, shared settings).
//!
//! Each [`SettingsEdit`] changes the TOML document through `toml_edit`, so everything the edit
//! doesn't touch (comments, blank lines, key order, quoting) stays byte for byte. The result is
//! then parsed with the strict settings parser; an edit that would make the file invalid, or
//! would leave the app or entrance it sets with an empty key, is refused with the parser's
//! message and position. [`apply_edits`] writes nothing: it returns the new text, what it holds,
//! and the diff from the old text.
//!
//! Apps and entrances are addressed by name. A name stands for the entry whose key it makes
//! ([`Key::app`], [`Key::entrance`]), so `"shop api"` addresses the app named `"Shop API"`, and
//! setting an entry also writes the name as given. Keys are written as TOML strings, in single
//! quotes when they contain `"`.
//!
//! | Edit | Effect |
//! | --- | --- |
//! | Exclude from graph | Removes the path from `include` if listed there (and others remain), otherwise adds it to `exclude` |
//! | Include in graph | Removes the path from `exclude` if listed there; adds it to a nonempty `include` when that list doesn't cover it. A broader `exclude` pattern is a conflict, reported, never silently rewritten |
//! | Graph only this folder | Sets `include` to the one folder |
//! | Remove a scope pattern | Removes one pattern, exactly as written, from `include` or `exclude` (the folder settings' remove buttons); an emptied list's key goes |
//! | Python project | Adds or replaces the `[[python.project]]` table with that root, or removes it |
//! | Evidence files | Adds a pattern to `[evidence] files` (unless it is listed), or removes one exactly as written; an emptied list's key goes, and so does the emptied table |
//! | Set app | Adds an `[[app]]` table, or sets an existing one's `run`, `code`, and `about` to exactly those given (absent ones are removed); its entrances stay |
//! | Set entrance | Adds one line to an app's `entrances`, creating the app when it is new, or rewrites the line of the entrance with that name |
//! | Remove entrance | Removes one entrance's line (an emptied list's key goes), or a whole app |

use serde::Deserialize;
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};
use unfold_core::Key;
use unfold_core::key::slug;

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
    /// Add an app, or set an existing app's `run`, `code`, and `about`.
    SetApp {
        /// The app.
        app: AppSpec,
    },
    /// Add or rewrite one entrance of an app, creating the app when it is new.
    SetEntrance {
        /// The app's name.
        app: String,
        /// The entrance.
        entrance: EntranceSpec,
    },
    /// Remove one entrance of an app, or the whole app.
    RemoveEntrance {
        /// The app's name.
        app: String,
        /// The entrance's name; `None` removes the app.
        entrance: Option<String>,
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

/// An `[[app]]` table as the editor submits it: everything but its entrances.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default)]
pub struct AppSpec {
    /// The app's name, which addresses it.
    pub name: String,
    /// The command that starts it.
    pub run: Option<String>,
    /// The block key where that command starts.
    pub code: Option<String>,
    /// One sentence for its card.
    pub about: Option<String>,
}

/// One entrance as the editor submits it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default)]
pub struct EntranceSpec {
    /// The entrance's name, which addresses it within its app.
    pub name: String,
    /// What starts it.
    pub on: Option<String>,
    /// The block keys it starts at, in order; at least one. One key is written as a string,
    /// several as a list.
    pub code: Vec<String>,
    /// One sentence for its card.
    pub about: Option<String>,
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
    let mut doc: DocumentMut = body
        .parse()
        .map_err(|error: toml_edit::TomlError| conflict(format!("fix unfold.toml first: {error}")))?;
    for edit in edits {
        apply(&mut doc, edit)?;
    }
    let text = format!("{bom}{doc}");
    let file = SettingsFile::parse(&text).map_err(EditError::Invalid)?;
    if let Some(problem) = edits.iter().find_map(|edit| empty_key_written(&file, edit)) {
        return Err(EditError::Invalid(entry_error(&text, problem)));
    }
    let diff = unified(source, &text);
    Ok(EditedSettings { text, file, diff })
}

/// The empty-key problem of the app or entrance `edit` sets, as `file` now holds it. Only that
/// entry is checked: a problem elsewhere was there before the edit.
fn empty_key_written<'f>(file: &'f SettingsFile, edit: &SettingsEdit) -> Option<&'f EntryProblem> {
    let (app, entrance) = match edit {
        SettingsEdit::SetApp { app } => (app.name.as_str(), None),
        SettingsEdit::SetEntrance { app, entrance } => (app.as_str(), Some(entrance.name.as_str())),
        _ => return None,
    };
    let app_key = Key::app(app).ok()?;
    let written = file
        .apps
        .iter()
        .find(|entry| entry.key.as_ref() == Some(&app_key))?;
    let problem = match entrance {
        None => written.problem.as_ref(),
        Some(entrance) => {
            let key = Key::entrance(app, entrance).ok()?;
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

fn apply(doc: &mut DocumentMut, edit: &SettingsEdit) -> Result<()> {
    match edit {
        SettingsEdit::ExcludeFromGraph { path } => exclude(doc, path),
        SettingsEdit::IncludeInGraph { path } => include(doc, path),
        SettingsEdit::GraphOnly { path } => {
            let mut include = Array::new();
            include.push(escape_glob(path));
            table(doc, "scope")?.insert("include", Item::Value(Value::Array(include)));
            Ok(())
        }
        SettingsEdit::RemoveScopePattern { list, pattern } => remove_pattern(doc, *list, pattern),
        SettingsEdit::SetPythonProject { project } => set_python_project(doc, project),
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
            Ok(())
        }
        SettingsEdit::AddEvidencePattern { pattern } => {
            let files = array(table(doc, "evidence")?, "files")?;
            if position(files, pattern).is_none() {
                files.push(pattern.as_str());
            }
            Ok(())
        }
        SettingsEdit::RemoveEvidencePattern { pattern } => remove_evidence(doc, pattern),
        SettingsEdit::SetApp { app } => set_app(doc, app),
        SettingsEdit::SetEntrance { app, entrance } => set_entrance(doc, app, entrance),
        SettingsEdit::RemoveEntrance { app, entrance } => remove_entrance(doc, app, entrance.as_deref()),
    }
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

/// The key `name` makes for an app, or a conflict naming the problem.
fn app_key(name: &str) -> Result<Key> {
    Key::app(name).map_err(|error| conflict(format!("an app's {error}")))
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

/// The position of the first app whose name makes `key`.
fn app_position(apps: &ArrayOfTables, key: &Key) -> Option<usize> {
    apps.iter().position(|table| {
        table
            .get("name")
            .and_then(Item::as_str)
            .is_some_and(|name| Key::app(name).as_ref() == Ok(key))
    })
}

/// The app whose name makes the same key as `name`, added with just its name when there is
/// none.
fn app_table<'d>(doc: &'d mut DocumentMut, name: &str) -> Result<&'d mut Table> {
    let key = app_key(name)?;
    let written = doc.iter().next().is_some();
    let item = doc
        .entry("app")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
    let apps = item
        .as_array_of_tables_mut()
        .ok_or_else(|| conflict("`app` in unfold.toml is not written as `[[app]]` tables"))?;
    let index = match app_position(apps, &key) {
        Some(index) => index,
        None => {
            let mut table = Table::new();
            if written {
                table.decor_mut().set_prefix("\n");
            }
            table.insert("name", toml_edit::value(name));
            apps.push(table);
            apps.len() - 1
        }
    };
    apps.get_mut(index)
        .ok_or_else(|| conflict(format!("no app is named `{name}`")))
}

fn set_app(doc: &mut DocumentMut, spec: &AppSpec) -> Result<()> {
    let table = app_table(doc, &spec.name)?;
    set_field(table, "name", Some(&spec.name));
    set_field(table, "run", spec.run.as_deref());
    set_field(table, "code", spec.code.as_deref());
    set_field(table, "about", spec.about.as_deref());
    Ok(())
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

fn set_entrance(doc: &mut DocumentMut, app: &str, spec: &EntranceSpec) -> Result<()> {
    let wanted = slug(&spec.name).map_err(|error| conflict(format!("an entrance's {error}")))?;
    if spec.code.is_empty() {
        return Err(conflict(format!(
            "entrance `{}` needs at least one code key",
            spec.name
        )));
    }
    let table = app_table(doc, app)?;
    let line = Value::InlineTable(entrance_line(spec));
    let entrances = entrances(table)?;
    match entrance_position(entrances, &wanted) {
        Some(index) => {
            entrances.replace(index, line);
        }
        None => {
            // A new line is indented like the last one (without its comments), or goes on
            // its own line.
            let mut line = line;
            let prefix = match entrances.iter().last() {
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
            if entrances.is_empty() {
                entrances.set_trailing("\n");
                entrances.set_trailing_comma(true);
            }
            entrances.push_formatted(line);
        }
    }
    Ok(())
}

/// An app's `entrances` list, created empty if absent.
fn entrances(table: &mut Table) -> Result<&mut Array> {
    let item = table
        .entry("entrances")
        .or_insert_with(|| Item::Value(Value::Array(Array::new())));
    item.as_array_mut()
        .ok_or_else(|| conflict("this app's `entrances` is not a list of inline tables"))
}

/// The position of the entrance whose name makes the slug `wanted`.
fn entrance_position(entrances: &Array, wanted: &str) -> Option<usize> {
    entrances.iter().position(|line| {
        line.as_inline_table()
            .and_then(|line| line.get("name"))
            .and_then(Value::as_str)
            .is_some_and(|name| slug(name).is_ok_and(|name| name == wanted))
    })
}

/// One entrance as an inline table: `{ name, on, code, about }`.
fn entrance_line(spec: &EntranceSpec) -> InlineTable {
    let mut line = InlineTable::new();
    line.insert("name", Value::from(spec.name.as_str()));
    if let Some(on) = &spec.on {
        line.insert("on", Value::from(on.as_str()));
    }
    let code = match spec.code.as_slice() {
        [one] => Value::from(one.as_str()),
        many => Value::Array(many.iter().map(String::as_str).collect()),
    };
    line.insert("code", code);
    if let Some(about) = &spec.about {
        line.insert("about", Value::from(about.as_str()));
    }
    line.fmt();
    line
}

fn remove_entrance(doc: &mut DocumentMut, app: &str, entrance: Option<&str>) -> Result<()> {
    let key = app_key(app)?;
    let missing = || conflict(format!("no app is named `{app}`"));
    let apps = apps(doc)?.ok_or_else(missing)?;
    let index = app_position(apps, &key).ok_or_else(missing)?;
    let Some(entrance) = entrance else {
        apps.remove(index);
        if apps.is_empty() {
            doc.remove("app");
        }
        return Ok(());
    };
    let missing = || conflict(format!("app `{app}` has no entrance named `{entrance}`"));
    let wanted = slug(entrance).map_err(|_| missing())?;
    let table = apps.get_mut(index).ok_or_else(missing)?;
    let lines = table
        .get_mut("entrances")
        .and_then(Item::as_array_mut)
        .ok_or_else(missing)?;
    let position = entrance_position(lines, &wanted).ok_or_else(missing)?;
    lines.remove(position);
    if lines.is_empty() {
        table.remove("entrances");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
