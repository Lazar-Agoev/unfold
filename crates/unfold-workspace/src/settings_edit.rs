//! Validated edits to `unfold.toml` that keep its comments and formatting (architecture
//! §9.2, shared settings).
//!
//! Each [`SettingsEdit`] changes the TOML document through `toml_edit`, so everything the edit
//! doesn't touch (comments, blank lines, key order, quoting) stays byte for byte. The result is
//! then parsed with the engine's strict settings parser; an edit that would make the file
//! invalid is refused with the parser's message and position, and nothing is written. The file
//! is written atomically (a temporary file renamed over it), and only when it would differ from
//! the defaults: an edit that leaves an absent file's settings at their defaults writes nothing.
//! Edits to applications, entrances, and overrides name them by ID (an override by its
//! `match`), never by position.
//!
//! | Edit | Effect |
//! | --- | --- |
//! | Exclude from graph | Removes the path from `include` if listed there (and others remain), otherwise adds it to `exclude` |
//! | Include in graph | Removes the path from `exclude` if listed there; adds it to a nonempty `include` when that list doesn't cover it. A broader `exclude` pattern is a conflict, reported, never silently rewritten |
//! | Graph only this folder | Sets `include` to the one folder |
//! | Remove a scope pattern | Removes one pattern, exactly as written, from `include` or `exclude` (the folder settings' remove buttons); an emptied list's key goes |
//! | Manual entrances | Adds an `[[entrance]]` table, or replaces or removes the one with an ID |
//! | Applications | Adds or replaces the `[[application]]` table with an ID, or removes it; removing one that entrances or overrides still name is refused, since the file would be invalid |
//! | Overrides | Adds or replaces the `[[entrance_override]]` table with a `match`, or removes it |
//! | Python project | Adds or replaces the `[[python.project]]` table with that root, or removes it |
//! | Detector toggle | Adds the detector to, or removes it from, `[detectors] disabled` |
//! | Project name | Sets `[project] name`, keeping the value's comments; no name removes the `[project]` table |
//! | Evidence files | Adds a pattern to `[evidence] files` (unless it is listed), or removes one exactly as written; an emptied list's key goes, and so does the emptied table |

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};
use ts_rs::TS;
use unfold_engine::launch::LaunchTarget;
use unfold_engine::settings::{
    Application, EntranceOverride, FolderPath, OverrideMatch, ScopeStatus, Settings, SettingsError,
};

use crate::error::{Result, WorkspaceError};

/// One change to the settings file. See the [module documentation](self).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
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
        /// The pattern, as `get_folder_config` lists it.
        pattern: String,
    },
    /// Append a manual entrance.
    AddEntrance {
        /// The entrance.
        entrance: EntranceSpec,
    },
    /// Replace the manual entrance with this ID.
    ReplaceEntrance {
        /// The entrance's current ID.
        id: String,
        /// The new entrance, which may have a new ID.
        entrance: EntranceSpec,
    },
    /// Remove the manual entrance with this ID.
    RemoveEntrance {
        /// The entrance's ID.
        id: String,
    },
    /// Add or replace the application with this ID.
    SetApplication {
        /// The application.
        application: ApplicationSpec,
    },
    /// Remove the application with this ID.
    RemoveApplication {
        /// The application's ID.
        id: String,
    },
    /// Add or replace the override with this `match`.
    SetOverride {
        /// The override.
        #[serde(rename = "override")]
        entrance_override: OverrideSpec,
    },
    /// Remove the override with this `match`.
    RemoveOverride {
        /// The override's `match`.
        #[serde(rename = "match")]
        selector: MatchSpec,
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
    /// Turn a detector on or off.
    SetDetector {
        /// The detector ID, such as `node-cron.schedule`.
        detector: String,
        /// Whether it runs.
        enabled: bool,
    },
    /// Set the project's shared name, or remove it (`null`) for the default.
    SetProjectName {
        /// The name; it must match `/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/`.
        name: Option<String>,
    },
    /// Add a pattern to `[evidence] files`, unless it is listed already.
    AddEvidencePattern {
        /// The pattern: a file, a folder, or a glob, relative to the project's folder.
        pattern: String,
    },
    /// Remove one `[evidence] files` pattern exactly as written.
    RemoveEvidencePattern {
        /// The pattern, as `project_config` lists it.
        pattern: String,
    },
}

/// One of the `[scope]` pattern lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
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

/// A manual entrance as the editor submits it (docs/engine.md#shared-config `[[entrance]]`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct EntranceSpec {
    /// The ID its key derives from.
    pub id: String,
    /// The entrance kind, such as `command`.
    pub kind: String,
    /// The `[[application]]` it belongs to.
    pub application: Option<String>,
    /// The trigger, if any.
    pub trigger: Option<String>,
    /// The HTTP method (`http` entrances only).
    pub method: Option<String>,
    /// A display label.
    pub label: Option<String>,
    /// A description.
    pub description: Option<String>,
    /// Handler locators, middleware first (every kind but `process`).
    pub handler: Vec<String>,
    /// The working folder (`process` only).
    pub cwd: Option<String>,
    /// The command line (`process` only).
    pub command: Option<String>,
    /// The startup locator (`process` only).
    pub startup: Option<String>,
    /// The launch target (`process` only).
    pub target: Option<LaunchTargetSpec>,
    /// The import path, in order (`process` only).
    pub python_paths: Option<Vec<String>>,
    /// Declared environment variables (`process` only); empty writes none.
    pub env: BTreeMap<String, String>,
    /// The schedule, as written (`process` only).
    pub schedule: Option<String>,
}

/// A process's `target` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LaunchTargetSpec {
    /// A file launched as a script.
    File {
        /// The folder-relative file.
        path: String,
    },
    /// A module run as `python -m`.
    PythonModule {
        /// The dotted module name.
        module: String,
    },
    /// A callable imported from a module and invoked.
    PythonCallable {
        /// The dotted module name.
        module: String,
        /// The callable's name.
        attribute: String,
    },
    /// An ASGI application served from a module.
    PythonAsgi {
        /// The dotted module name.
        module: String,
        /// The application's name.
        attribute: String,
        /// Whether the attribute is a factory.
        #[serde(default)]
        factory: bool,
    },
}

impl From<&LaunchTarget> for LaunchTargetSpec {
    fn from(target: &LaunchTarget) -> Self {
        match target {
            LaunchTarget::File { path } => Self::File {
                path: path.to_string(),
            },
            LaunchTarget::PythonModule { module } => Self::PythonModule {
                module: module.to_string(),
            },
            LaunchTarget::PythonCallable { module, attribute } => Self::PythonCallable {
                module: module.to_string(),
                attribute: attribute.to_string(),
            },
            LaunchTarget::PythonAsgi {
                module,
                attribute,
                factory,
            } => Self::PythonAsgi {
                module: module.to_string(),
                attribute: attribute.to_string(),
                factory: *factory,
            },
        }
    }
}

/// An `[[application]]` table, as the editor submits it and the settings view lists it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationSpec {
    /// The application's ID.
    pub id: String,
    /// The display name.
    pub name: String,
    /// A description.
    #[serde(default)]
    pub description: Option<String>,
}

impl From<&Application> for ApplicationSpec {
    fn from(application: &Application) -> Self {
        Self {
            id: application.id.to_string(),
            name: application.name.to_string(),
            description: application.description.as_deref().map(str::to_owned),
        }
    }
}

/// An `[[entrance_override]]` table, as the editor submits it and the settings view lists it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OverrideSpec {
    /// The detected launch it applies to.
    #[serde(rename = "match")]
    pub selector: MatchSpec,
    /// The `[[application]]` the launch belongs to.
    #[serde(default)]
    pub application: Option<String>,
    /// A display label.
    #[serde(default)]
    pub label: Option<String>,
    /// A description.
    #[serde(default)]
    pub description: Option<String>,
    /// A schedule, as written.
    #[serde(default)]
    pub schedule: Option<String>,
}

impl From<&EntranceOverride> for OverrideSpec {
    fn from(entrance_override: &EntranceOverride) -> Self {
        Self {
            selector: MatchSpec::from(&entrance_override.selector),
            application: entrance_override.application.as_deref().map(str::to_owned),
            label: entrance_override.label.as_deref().map(str::to_owned),
            description: entrance_override.description.as_deref().map(str::to_owned),
            schedule: entrance_override.schedule.as_deref().map(str::to_owned),
        }
    }
}

/// An override's `match`: a launch detector, its input file, and the pointer into it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MatchSpec {
    /// `package.script`, `package.bin`, or `pyproject.script`.
    pub detector: String,
    /// The input file, such as `apps/api/package.json`.
    pub path: String,
    /// The pointer to the launch's member, such as `/scripts/start`.
    pub pointer: String,
}

impl From<&OverrideMatch> for MatchSpec {
    fn from(selector: &OverrideMatch) -> Self {
        Self {
            detector: selector.detector.to_string(),
            path: selector.path.to_string(),
            pointer: selector.pointer.to_string(),
        }
    }
}

/// A `[[python.project]]` table as the editor submits it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct PythonProjectSpec {
    /// The project folder, relative to the open folder.
    pub root: String,
    /// Import roots relative to `root`; `None` keeps the front end's default.
    pub source_roots: Option<Vec<String>>,
    /// The grammar version, such as `3.12`; `None` means the default.
    pub version: Option<String>,
}

/// The settings file after a set of edits.
#[derive(Debug, Clone)]
pub struct EditedSettings {
    /// The new file text.
    pub text: String,
    /// The validated settings it holds.
    pub settings: Settings,
}

/// Applies `edits` to the settings text `current` (`None` when the file is absent) and
/// validates the result. Writes nothing.
pub fn apply_edits(current: Option<&str>, edits: &[SettingsEdit]) -> Result<EditedSettings> {
    let source = current.unwrap_or("");
    if current.is_some() {
        Settings::parse(source).map_err(|error| invalid(&error, "fix unfold.toml first: "))?;
    }
    let mut doc: DocumentMut =
        source
            .parse()
            .map_err(|error: toml_edit::TomlError| WorkspaceError::InvalidSettings {
                message: format!("fix unfold.toml first: {error}"),
                line: None,
                column: None,
            })?;
    for edit in edits {
        apply(&mut doc, edit)?;
    }
    let text = doc.to_string();
    let settings = Settings::parse(&text).map_err(|error| invalid(&error, ""))?;
    Ok(EditedSettings { text, settings })
}

fn invalid(error: &SettingsError, prefix: &str) -> WorkspaceError {
    let (line, column) = match error {
        SettingsError::Invalid { line, column, .. } => (Some(*line), Some(*column)),
        _ => (None, None),
    };
    WorkspaceError::InvalidSettings {
        message: format!("{prefix}{error}"),
        line,
        column,
    }
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
        SettingsEdit::AddEntrance { entrance } => {
            tables(doc, "entrance")?.push(entrance_table(entrance));
            Ok(())
        }
        SettingsEdit::ReplaceEntrance { id, entrance } => {
            let tables = tables(doc, "entrance")?;
            let index = with_id(tables, id).ok_or_else(|| missing("entrance", id))?;
            if let Some(table) = tables.get_mut(index) {
                replace(table, entrance_table(entrance));
            }
            Ok(())
        }
        SettingsEdit::RemoveEntrance { id } => remove_table(
            doc,
            "entrance",
            |tables| with_id(tables, id),
            || missing("entrance", id),
        ),
        SettingsEdit::SetApplication { application } => {
            let tables = tables(doc, "application")?;
            let fresh = application_table(application);
            match with_id(tables, &application.id).and_then(|index| tables.get_mut(index)) {
                Some(table) => replace(table, fresh),
                None => tables.push(fresh),
            }
            Ok(())
        }
        SettingsEdit::RemoveApplication { id } => remove_table(
            doc,
            "application",
            |tables| with_id(tables, id),
            || missing("application", id),
        ),
        SettingsEdit::SetOverride { entrance_override } => {
            let tables = tables(doc, "entrance_override")?;
            let fresh = override_table(entrance_override);
            match with_match(tables, &entrance_override.selector).and_then(|index| tables.get_mut(index)) {
                Some(table) => replace(table, fresh),
                None => tables.push(fresh),
            }
            Ok(())
        }
        SettingsEdit::RemoveOverride { selector } => remove_table(
            doc,
            "entrance_override",
            |tables| with_match(tables, selector),
            || {
                WorkspaceError::SettingsConflict(format!(
                    "no override matches {} `{}` at `{}`",
                    selector.detector, selector.path, selector.pointer
                ))
            },
        ),
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
                .ok_or_else(|| {
                    WorkspaceError::SettingsConflict(format!("no Python project has root `{root}`"))
                })?;
            projects.remove(position);
            Ok(())
        }
        SettingsEdit::SetDetector { detector, enabled } => {
            let detectors = table(doc, "detectors")?;
            let disabled = array(detectors, "disabled")?;
            let listed = disabled.iter().position(|value| value.as_str() == Some(detector));
            match (enabled, listed) {
                (false, None) => disabled.push(detector.as_str()),
                (true, Some(index)) => {
                    disabled.remove(index);
                }
                _ => {}
            }
            Ok(())
        }
        SettingsEdit::SetProjectName { name } => set_project_name(doc, name.as_deref()),
        SettingsEdit::AddEvidencePattern { pattern } => {
            let files = array(table(doc, "evidence")?, "files")?;
            if position(files, pattern).is_none() {
                files.push(pattern.as_str());
            }
            Ok(())
        }
        SettingsEdit::RemoveEvidencePattern { pattern } => remove_evidence(doc, pattern),
    }
}

/// Removes `pattern`, exactly as written, from `[evidence] files`; an emptied list's key goes,
/// and so does the `[evidence]` table it emptied.
fn remove_evidence(doc: &mut DocumentMut, pattern: &str) -> Result<()> {
    let missing = || WorkspaceError::SettingsConflict(format!("`{pattern}` is not in `[evidence] files`"));
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

/// Sets `[project] name`, or removes the `[project]` table (which holds only the name). A value
/// that is already the name is left as written.
fn set_project_name(doc: &mut DocumentMut, name: Option<&str>) -> Result<()> {
    let Some(name) = name else {
        doc.remove("project");
        return Ok(());
    };
    let project = table(doc, "project")?;
    match project.get_mut("name").and_then(Item::as_value_mut) {
        Some(value) if value.as_str() == Some(name) => {}
        Some(value) => {
            // Keep the comments around the value.
            let decor = value.decor().clone();
            *value = Value::from(name);
            *value.decor_mut() = decor;
        }
        None => {
            project.insert("name", toml_edit::value(name));
        }
    }
    Ok(())
}

fn missing(table: &str, id: &str) -> WorkspaceError {
    WorkspaceError::SettingsConflict(format!("no `[[{table}]]` has the id `{id}`"))
}

/// The scope as the document says now.
fn scope_of(doc: &DocumentMut) -> Result<unfold_engine::settings::Scope> {
    Ok(Settings::parse(&doc.to_string())
        .map_err(|error| invalid(&error, ""))?
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
            Err(WorkspaceError::SettingsConflict(format!(
                "`{path}` is excluded by a broader pattern (`{patterns}`); edit unfold.toml to include it"
            )))
        }
    }
}

fn remove_pattern(doc: &mut DocumentMut, list: ScopeList, pattern: &str) -> Result<()> {
    let key = list.key();
    let missing = || WorkspaceError::SettingsConflict(format!("`{pattern}` is not in `[scope] {key}`"));
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
        .ok_or_else(|| WorkspaceError::SettingsConflict(format!("`{key}` in unfold.toml is not a table")))
}

/// The array at `key` in `table`, created if absent.
fn array<'t>(table: &'t mut Table, key: &str) -> Result<&'t mut Array> {
    let item = table
        .entry(key)
        .or_insert_with(|| Item::Value(Value::Array(Array::new())));
    item.as_array_mut()
        .ok_or_else(|| WorkspaceError::SettingsConflict(format!("`{key}` in unfold.toml is not an array")))
}

/// The array of tables at `key`, created if absent.
fn tables<'d>(doc: &'d mut DocumentMut, key: &str) -> Result<&'d mut ArrayOfTables> {
    let item = doc
        .entry(key)
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
    item.as_array_of_tables_mut().ok_or_else(|| {
        WorkspaceError::SettingsConflict(format!("`{key}` in unfold.toml is not a list of tables"))
    })
}

/// The position of the table whose `id` is `id`.
fn with_id(tables: &ArrayOfTables, id: &str) -> Option<usize> {
    tables
        .iter()
        .position(|table| table.get("id").and_then(Item::as_str) == Some(id))
}

/// The position of the override table whose `match` is `selector`.
fn with_match(tables: &ArrayOfTables, selector: &MatchSpec) -> Option<usize> {
    tables.iter().position(|table| {
        let Some(found) = table.get("match").and_then(Item::as_table_like) else {
            return false;
        };
        let field = |key: &str| found.get(key).and_then(Item::as_str);
        field("detector") == Some(selector.detector.as_str())
            && field("path") == Some(selector.path.as_str())
            && field("pointer") == Some(selector.pointer.as_str())
    })
}

/// Removes the table `find` picks from the array at `key`; an emptied array's key goes.
fn remove_table(
    doc: &mut DocumentMut,
    key: &str,
    find: impl FnOnce(&ArrayOfTables) -> Option<usize>,
    missing: impl FnOnce() -> WorkspaceError,
) -> Result<()> {
    let tables = tables(doc, key)?;
    let index = find(tables).ok_or_else(missing)?;
    tables.remove(index);
    if tables.is_empty() {
        doc.remove(key);
    }
    Ok(())
}

/// Replaces `table`'s keys with `fresh`'s, keeping the header's comments and position.
fn replace(table: &mut Table, fresh: Table) {
    table.clear();
    for (key, value) in fresh {
        table.insert(&key, value);
    }
}

fn text(table: &mut Table, key: &str, value: Option<&String>) {
    if let Some(value) = value {
        table.insert(key, toml_edit::value(value.as_str()));
    }
}

fn strings<'s>(values: impl IntoIterator<Item = &'s String>) -> Item {
    Item::Value(Value::Array(values.into_iter().map(String::as_str).collect()))
}

fn entrance_table(spec: &EntranceSpec) -> Table {
    let mut table = Table::new();
    table.insert("id", toml_edit::value(spec.id.as_str()));
    table.insert("kind", toml_edit::value(spec.kind.as_str()));
    text(&mut table, "application", spec.application.as_ref());
    text(&mut table, "label", spec.label.as_ref());
    text(&mut table, "description", spec.description.as_ref());
    text(&mut table, "trigger", spec.trigger.as_ref());
    text(&mut table, "method", spec.method.as_ref());
    match spec.handler.as_slice() {
        [] => {}
        [one] => {
            table.insert("handler", toml_edit::value(one.as_str()));
        }
        many => {
            table.insert("handler", strings(many));
        }
    }
    text(&mut table, "cwd", spec.cwd.as_ref());
    text(&mut table, "command", spec.command.as_ref());
    text(&mut table, "startup", spec.startup.as_ref());
    if let Some(target) = &spec.target {
        table.insert("target", toml_edit::value(target_table(target)));
    }
    if let Some(paths) = &spec.python_paths {
        table.insert("python_paths", strings(paths));
    }
    if !spec.env.is_empty() {
        let env: InlineTable = spec
            .env
            .iter()
            .map(|(name, value)| (name.as_str(), Value::from(value.as_str())))
            .collect();
        table.insert("env", toml_edit::value(env));
    }
    text(&mut table, "schedule", spec.schedule.as_ref());
    table
}

fn target_table(target: &LaunchTargetSpec) -> InlineTable {
    let mut table = InlineTable::new();
    let mut field = |key: &str, value: Value| {
        table.insert(key, value);
    };
    match target {
        LaunchTargetSpec::File { path } => {
            field("type", "file".into());
            field("path", path.as_str().into());
        }
        LaunchTargetSpec::PythonModule { module } => {
            field("type", "python_module".into());
            field("module", module.as_str().into());
        }
        LaunchTargetSpec::PythonCallable { module, attribute } => {
            field("type", "python_callable".into());
            field("module", module.as_str().into());
            field("attribute", attribute.as_str().into());
        }
        LaunchTargetSpec::PythonAsgi {
            module,
            attribute,
            factory,
        } => {
            field("type", "python_asgi".into());
            field("module", module.as_str().into());
            field("attribute", attribute.as_str().into());
            if *factory {
                field("factory", true.into());
            }
        }
    }
    table
}

fn application_table(spec: &ApplicationSpec) -> Table {
    let mut table = Table::new();
    table.insert("id", toml_edit::value(spec.id.as_str()));
    table.insert("name", toml_edit::value(spec.name.as_str()));
    text(&mut table, "description", spec.description.as_ref());
    table
}

fn override_table(spec: &OverrideSpec) -> Table {
    let mut selector = InlineTable::new();
    selector.insert("detector", spec.selector.detector.as_str().into());
    selector.insert("path", spec.selector.path.as_str().into());
    selector.insert("pointer", spec.selector.pointer.as_str().into());
    let mut table = Table::new();
    table.insert("match", toml_edit::value(selector));
    text(&mut table, "application", spec.application.as_ref());
    text(&mut table, "label", spec.label.as_ref());
    text(&mut table, "description", spec.description.as_ref());
    text(&mut table, "schedule", spec.schedule.as_ref());
    table
}

fn folder(text: &str) -> Result<FolderPath> {
    FolderPath::parse(text).map_err(|error| WorkspaceError::SettingsConflict(format!("`{text}`: {error}")))
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
        .ok_or_else(|| WorkspaceError::SettingsConflict("`python.project` is not a list of tables".into()))
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

#[cfg(test)]
mod tests {
    use unfold_engine::settings::ManualTargets;

    use super::*;

    const COMMENTED: &str = "# Shared settings.\nlanguages = [\"typescript\"]\n\n[scope]\n# generated code\nexclude = [\"**/generated/**\"]\n\n# CLI dispatch\n[[entrance]]\nid = \"import\"\nkind = \"command\"\ntrigger = \"import\"\nhandler = \"src/a.ts#run\"\n";

    #[test]
    fn edits_keep_comments_and_formatting() {
        let edited = apply_edits(
            Some(COMMENTED),
            &[SettingsEdit::ExcludeFromGraph {
                path: "src/legacy".into(),
            }],
        )
        .expect("edit");
        assert!(
            edited.text.starts_with(
                "# Shared settings.\nlanguages = [\"typescript\"]\n\n[scope]\n# generated code\n"
            )
        );
        assert!(edited.text.contains("# CLI dispatch\n[[entrance]]"));
        assert_eq!(
            edited.settings.scope.exclude(),
            ["**/generated/**".into(), "src/legacy".into()]
        );
        let back = apply_edits(
            Some(&edited.text),
            &[SettingsEdit::IncludeInGraph {
                path: "src/legacy".into(),
            }],
        )
        .expect("edit");
        assert_eq!(back.settings.scope.exclude(), ["**/generated/**".into()]);
        assert!(back.text.contains("# generated code"));
    }

    #[test]
    fn scope_edits_follow_the_rules() {
        let only = apply_edits(None, &[SettingsEdit::GraphOnly { path: "src".into() }]).expect("only");
        assert_eq!(only.settings.scope.include(), ["src".into()]);
        let conflict = apply_edits(
            Some("[scope]\nexclude = [\"src/**\"]\n"),
            &[SettingsEdit::IncludeInGraph {
                path: "src/a.ts".into(),
            }],
        )
        .expect_err("conflict");
        assert_eq!(conflict.code(), "settings_conflict");
        let widened = apply_edits(
            Some(&only.text),
            &[SettingsEdit::IncludeInGraph {
                path: "scripts".into(),
            }],
        )
        .expect("include");
        assert_eq!(widened.settings.scope.include(), ["src".into(), "scripts".into()]);
        let narrowed = apply_edits(
            Some(&widened.text),
            &[SettingsEdit::ExcludeFromGraph { path: "src".into() }],
        )
        .expect("exclude");
        assert_eq!(narrowed.settings.scope.include(), ["scripts".into()]);
        assert_eq!(escape_glob("a[1]/*.ts"), "a\\[1\\]/\\*.ts");
    }

    #[test]
    fn entrances_are_edited_by_id() {
        let spec = EntranceSpec {
            id: "shutdown".into(),
            kind: "event".into(),
            trigger: Some("SIGTERM".into()),
            handler: vec!["src/server.ts#shutdown".into()],
            ..EntranceSpec::default()
        };
        let added = apply_edits(
            Some(COMMENTED),
            &[SettingsEdit::AddEntrance {
                entrance: spec.clone(),
            }],
        )
        .expect("add");
        assert_eq!(added.settings.entrances.len(), 2);
        let twice = apply_edits(Some(&added.text), &[SettingsEdit::AddEntrance { entrance: spec }])
            .expect_err("a second entrance with the same ID");
        assert!(matches!(
            twice,
            WorkspaceError::InvalidSettings { line: Some(_), .. }
        ));
        let replaced = apply_edits(
            Some(&added.text),
            &[SettingsEdit::ReplaceEntrance {
                id: "import".into(),
                entrance: EntranceSpec {
                    id: "export".into(),
                    kind: "command".into(),
                    trigger: Some("export".into()),
                    handler: vec!["src/a.ts#auth".into(), "src/a.ts#run".into()],
                    ..EntranceSpec::default()
                },
            }],
        )
        .expect("replace");
        assert!(
            replaced
                .text
                .contains("# CLI dispatch\n[[entrance]]\nid = \"export\"")
        );
        assert_eq!(replaced.settings.entrances[0].trigger.as_deref(), Some("export"));
        let removed = apply_edits(
            Some(&replaced.text),
            &[SettingsEdit::RemoveEntrance {
                id: "shutdown".into(),
            }],
        )
        .expect("remove");
        let ids: Vec<&str> = removed
            .settings
            .entrances
            .iter()
            .map(|entrance| &*entrance.id)
            .collect();
        assert_eq!(ids, ["export"]);
        for edit in [
            SettingsEdit::RemoveEntrance { id: "import".into() },
            SettingsEdit::ReplaceEntrance {
                id: "import".into(),
                entrance: EntranceSpec::default(),
            },
        ] {
            let missing = apply_edits(Some(&removed.text), &[edit]).expect_err("no such entrance");
            assert_eq!(missing.code(), "settings_conflict");
        }
        let emptied = apply_edits(
            Some(&removed.text),
            &[SettingsEdit::RemoveEntrance { id: "export".into() }],
        )
        .expect("remove");
        assert!(!emptied.text.contains("entrance"), "{}", emptied.text);
    }

    #[test]
    fn process_entrances_write_every_field() {
        let entrance = EntranceSpec {
            id: "recipe-cost-start".into(),
            kind: "process".into(),
            label: Some("Start API".into()),
            cwd: Some("data-pipelines".into()),
            command: Some("python3 -m uvicorn api:app --app-dir food-cost".into()),
            target: Some(LaunchTargetSpec::PythonAsgi {
                module: "api".into(),
                attribute: "app".into(),
                factory: false,
            }),
            python_paths: Some(vec!["data-pipelines/food-cost".into()]),
            env: BTreeMap::from([("PYTHONPATH".into(), ".".into())]),
            schedule: Some("30 7 * * *".into()),
            ..EntranceSpec::default()
        };
        let edited = apply_edits(None, &[SettingsEdit::AddEntrance { entrance }]).expect("add");
        assert_eq!(
            edited.text,
            "[[entrance]]\nid = \"recipe-cost-start\"\nkind = \"process\"\nlabel = \"Start API\"\ncwd = \"data-pipelines\"\ncommand = \"python3 -m uvicorn api:app --app-dir food-cost\"\ntarget = { type = \"python_asgi\", module = \"api\", attribute = \"app\" }\npython_paths = [\"data-pipelines/food-cost\"]\nenv = { PYTHONPATH = \".\" }\nschedule = \"30 7 * * *\"\n"
        );
        let ManualTargets::Process(process) = &edited.settings.entrances[0].targets else {
            panic!("a process")
        };
        assert_eq!(process.schedule.as_deref(), Some("30 7 * * *"));
        let edit: SettingsEdit = serde_json::from_str(
            r#"{"op": "add_entrance", "entrance": {"id": "a", "kind": "process", "target": {"type": "python_module", "module": "a.b"}, "pythonPaths": ["src"]}}"#,
        )
        .expect("deserialize");
        let SettingsEdit::AddEntrance { entrance } = edit else {
            panic!("add_entrance")
        };
        assert_eq!(
            entrance.target,
            Some(LaunchTargetSpec::PythonModule { module: "a.b".into() })
        );
        assert_eq!(entrance.python_paths, Some(vec!["src".into()]));
    }

    #[test]
    fn applications_and_overrides_are_set_and_removed() {
        let application = |name: &str| SettingsEdit::SetApplication {
            application: ApplicationSpec {
                id: "orders-api".into(),
                name: name.into(),
                description: None,
            },
        };
        let selector = MatchSpec {
            detector: "package.script".into(),
            path: "apps/api/package.json".into(),
            pointer: "/scripts/start".into(),
        };
        let set_override = |label: &str| SettingsEdit::SetOverride {
            entrance_override: OverrideSpec {
                selector: selector.clone(),
                application: Some("orders-api".into()),
                label: Some(label.into()),
                ..OverrideSpec::default()
            },
        };
        let edited =
            apply_edits(Some(COMMENTED), &[application("Orders"), set_override("Start")]).expect("set");
        assert!(edited.text.starts_with(COMMENTED), "{}", edited.text);
        assert!(
            edited.text.ends_with("[[application]]\nid = \"orders-api\"\nname = \"Orders\"\n\n[[entrance_override]]\nmatch = { detector = \"package.script\", path = \"apps/api/package.json\", pointer = \"/scripts/start\" }\napplication = \"orders-api\"\nlabel = \"Start\"\n"),
            "{}",
            edited.text
        );
        let renamed = apply_edits(
            Some(&edited.text),
            &[application("Orders API"), set_override("Start API")],
        )
        .expect("replace");
        assert_eq!(renamed.settings.applications.len(), 1);
        assert_eq!(&*renamed.settings.applications[0].name, "Orders API");
        assert_eq!(renamed.settings.overrides.len(), 1);
        assert_eq!(renamed.settings.overrides[0].label.as_deref(), Some("Start API"));

        let remove_application = SettingsEdit::RemoveApplication {
            id: "orders-api".into(),
        };
        let named = apply_edits(Some(&renamed.text), std::slice::from_ref(&remove_application))
            .expect_err("an override still names it");
        assert!(matches!(
            named,
            WorkspaceError::InvalidSettings { line: Some(_), .. }
        ));
        let removed = apply_edits(
            Some(&renamed.text),
            &[
                SettingsEdit::RemoveOverride {
                    selector: selector.clone(),
                },
                remove_application.clone(),
            ],
        )
        .expect("remove");
        assert_eq!(removed.text, COMMENTED);
        let missing = apply_edits(Some(COMMENTED), &[SettingsEdit::RemoveOverride { selector }])
            .expect_err("no such override");
        assert_eq!(missing.code(), "settings_conflict");
        let missing = apply_edits(Some(COMMENTED), &[remove_application]).expect_err("no such application");
        assert_eq!(missing.code(), "settings_conflict");
        let edit: SettingsEdit = serde_json::from_str(
            r#"{"op": "remove_override", "match": {"detector": "package.bin", "path": "package.json", "pointer": "/bin"}}"#,
        )
        .expect("deserialize");
        assert!(matches!(edit, SettingsEdit::RemoveOverride { .. }));
    }

    #[test]
    fn python_projects_and_detectors() {
        let python = apply_edits(
            None,
            &[SettingsEdit::SetPythonProject {
                project: PythonProjectSpec {
                    root: "py".into(),
                    source_roots: Some(vec!["src".into()]),
                    version: Some("3.12".into()),
                },
            }],
        )
        .expect("python");
        assert_eq!(python.settings.python_projects.len(), 1);
        assert!(!python.text.contains("[python]\n"), "{}", python.text);
        let bad = apply_edits(
            Some(&python.text),
            &[SettingsEdit::SetPythonProject {
                project: PythonProjectSpec {
                    root: "py".into(),
                    version: Some("2.7".into()),
                    ..PythonProjectSpec::default()
                },
            }],
        )
        .expect_err("unsupported grammar");
        assert!(matches!(
            bad,
            WorkspaceError::InvalidSettings { line: Some(_), .. }
        ));
        let gone = apply_edits(
            Some(&python.text),
            &[SettingsEdit::RemovePythonProject { root: "./py/".into() }],
        )
        .expect("remove");
        assert!(gone.settings.python_projects.is_empty());

        let off = apply_edits(
            None,
            &[SettingsEdit::SetDetector {
                detector: "node-cron.schedule".into(),
                enabled: false,
            }],
        )
        .expect("off");
        assert_eq!(off.settings.disabled_detectors.len(), 1);
        let on = apply_edits(
            Some(&off.text),
            &[SettingsEdit::SetDetector {
                detector: "node-cron.schedule".into(),
                enabled: true,
            }],
        )
        .expect("on");
        assert!(on.settings.disabled_detectors.is_empty());
        let manual = apply_edits(
            None,
            &[SettingsEdit::SetDetector {
                detector: "manual".into(),
                enabled: false,
            }],
        );
        assert!(manual.is_err());
    }

    #[test]
    fn scope_patterns_are_removed_as_written() {
        let text =
            "[scope]\n# keep me\ninclude = [\"src\"]\nexclude = [\"**/generated/**\", \"src/legacy\"]\n";
        let edited = apply_edits(
            Some(text),
            &[SettingsEdit::RemoveScopePattern {
                list: ScopeList::Exclude,
                pattern: "**/generated/**".into(),
            }],
        )
        .expect("remove");
        assert_eq!(edited.settings.scope.exclude(), ["src/legacy".into()]);
        assert!(edited.text.contains("# keep me"), "{}", edited.text);
        let emptied = apply_edits(
            Some(&edited.text),
            &[SettingsEdit::RemoveScopePattern {
                list: ScopeList::Include,
                pattern: "src".into(),
            }],
        )
        .expect("remove");
        assert!(emptied.settings.scope.include().is_empty());
        assert!(!emptied.text.contains("include"), "{}", emptied.text);
        let missing = apply_edits(
            Some(&emptied.text),
            &[SettingsEdit::RemoveScopePattern {
                list: ScopeList::Include,
                pattern: "src".into(),
            }],
        )
        .expect_err("not listed");
        assert_eq!(missing.code(), "settings_conflict");
    }

    #[test]
    fn the_project_name_is_set_renamed_and_removed() {
        let name = |name: Option<&str>| SettingsEdit::SetProjectName {
            name: name.map(str::to_owned),
        };
        let fresh = apply_edits(None, &[name(Some("orders"))]).expect("set");
        assert_eq!(fresh.text, "[project]\nname = \"orders\"\n");
        assert_eq!(fresh.settings.project_name.as_deref(), Some("orders"));

        let named = apply_edits(Some(COMMENTED), &[name(Some("orders"))]).expect("set");
        assert!(named.text.starts_with(COMMENTED), "{}", named.text);
        let commented = named
            .text
            .replace("name = \"orders\"", "# shared\nname = \"orders\" # keep");
        let renamed = apply_edits(Some(&commented), &[name(Some("shop"))]).expect("rename");
        assert!(
            renamed.text.contains("# shared\nname = \"shop\" # keep\n"),
            "{}",
            renamed.text
        );
        assert_eq!(renamed.settings.project_name.as_deref(), Some("shop"));
        let same = apply_edits(Some(&commented), &[name(Some("orders"))]).expect("same");
        assert_eq!(
            same.text, commented,
            "an unchanged name leaves the text as it was"
        );

        let removed = apply_edits(Some(&renamed.text), &[name(None)]).expect("remove");
        assert_eq!(removed.text, COMMENTED);
        assert_eq!(removed.settings.project_name, None);
        let absent = apply_edits(Some(COMMENTED), &[name(None)]).expect("remove");
        assert_eq!(absent.text, COMMENTED);

        let invalid = apply_edits(Some(COMMENTED), &[name(Some("my orders"))]).expect_err("invalid");
        assert!(matches!(
            invalid,
            WorkspaceError::InvalidSettings { line: Some(_), .. }
        ));
        let edit: SettingsEdit =
            serde_json::from_str(r#"{"op": "set_project_name", "name": null}"#).expect("deserialize");
        assert_eq!(edit, name(None));
    }

    #[test]
    fn evidence_patterns_are_added_once_and_removed_as_written() {
        let add = |pattern: &str| SettingsEdit::AddEvidencePattern {
            pattern: pattern.into(),
        };
        let remove = |pattern: &str| SettingsEdit::RemoveEvidencePattern {
            pattern: pattern.into(),
        };
        let added = apply_edits(
            Some(COMMENTED),
            &[
                add("prisma/schema.prisma"),
                add("db/migrations/*.sql"),
                add("prisma/schema.prisma"),
            ],
        )
        .expect("add");
        assert!(added.text.starts_with(COMMENTED), "{}", added.text);
        assert_eq!(
            added.settings.evidence.patterns(),
            ["prisma/schema.prisma".into(), "db/migrations/*.sql".into()]
        );
        let commented = added.text.replace("[evidence]\n", "[evidence]\n# for boards\n");
        let removed = apply_edits(Some(&commented), &[remove("prisma/schema.prisma")]).expect("remove");
        assert_eq!(
            removed.settings.evidence.patterns(),
            ["db/migrations/*.sql".into()]
        );
        assert!(removed.text.contains("# for boards"), "{}", removed.text);
        let emptied = apply_edits(
            Some(&added.text),
            &[remove("prisma/schema.prisma"), remove("db/migrations/*.sql")],
        )
        .expect("remove");
        assert_eq!(emptied.text, COMMENTED, "the emptied table goes");
        let missing = apply_edits(Some(COMMENTED), &[remove("db")]).expect_err("not listed");
        assert_eq!(missing.code(), "settings_conflict");
        let outside = apply_edits(None, &[add("../secrets.sql")]).expect_err("confined");
        assert!(matches!(
            outside,
            WorkspaceError::InvalidSettings { line: Some(_), .. }
        ));
        let edit: SettingsEdit =
            serde_json::from_str(r#"{"op": "add_evidence_pattern", "pattern": "db"}"#).expect("deserialize");
        assert_eq!(edit, add("db"));
    }

    #[test]
    fn invalid_files_are_not_edited() {
        let error = apply_edits(
            Some("[scope]\nbogus = 2\n"),
            &[SettingsEdit::GraphOnly { path: "src".into() }],
        )
        .expect_err("invalid");
        assert!(matches!(
            error,
            WorkspaceError::InvalidSettings { line: Some(2), .. }
        ));
    }
}
