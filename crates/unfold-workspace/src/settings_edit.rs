//! Validated edits to `.unfold/config.toml` that keep its comments and formatting (architecture
//! §9.2, decision U-23).
//!
//! Each [`SettingsEdit`] changes the TOML document through `toml_edit`, so everything the edit
//! doesn't touch (comments, blank lines, key order, quoting) stays byte for byte. The result is
//! then parsed with the engine's strict settings parser; an edit that would make the file
//! invalid is refused with the parser's message and position, and nothing is written. The file
//! is written atomically (a temporary file renamed over it), and only when it would differ from
//! the defaults: an edit that leaves an absent file's settings at their defaults writes nothing.
//!
//! | Edit | Effect |
//! | --- | --- |
//! | Exclude from graph | Removes the path from `include` if listed there (and others remain), otherwise adds it to `exclude` |
//! | Include in graph | Removes the path from `exclude` if listed there; adds it to a nonempty `include` when that list doesn't cover it. A broader `exclude` pattern is a conflict, reported, never silently rewritten |
//! | Graph only this folder | Sets `include` to the one folder |
//! | Remove a scope pattern | Removes one pattern, exactly as written, from `include` or `exclude` (the folder settings' remove buttons); an emptied list's key goes |
//! | Manual entrances | Adds, replaces, or removes one `[[entrance]]` table by its position |
//! | Python project | Adds or replaces the `[[python.project]]` table with that root, or removes it |
//! | Detector toggle | Adds the detector to, or removes it from, `[detectors] disabled` |
//! | Project name | Sets `[project] name`, keeping the value's comments; no name removes the `[project]` table |
//! | Evidence files | Adds a pattern to `[evidence] files` (unless it is listed), or removes one exactly as written; an emptied list's key goes, and so does the emptied table |

use serde::Deserialize;
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};
use ts_rs::TS;
use unfold_engine::settings::{FolderPath, ScopeStatus, Settings, SettingsError};

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
    /// Replace the manual entrance at `index` (its position among the `[[entrance]]` tables).
    ReplaceEntrance {
        /// The table's position.
        index: usize,
        /// The new entrance.
        entrance: EntranceSpec,
    },
    /// Remove the manual entrance at `index`.
    RemoveEntrance {
        /// The table's position.
        index: usize,
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

/// A manual entrance as the editor submits it (architecture §9.2 `[[entrance]]`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct EntranceSpec {
    /// The entrance kind, such as `command`.
    pub kind: String,
    /// The trigger, if any.
    pub trigger: Option<String>,
    /// The HTTP method (`http` entrances only).
    pub method: Option<String>,
    /// A display label.
    pub label: Option<String>,
    /// Handler locators, middleware first (every kind but `process`).
    pub handler: Vec<String>,
    /// The startup locator (`process` only).
    pub startup: Option<String>,
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
        Settings::parse(source).map_err(|error| invalid(&error, "fix .unfold/config.toml first: "))?;
    }
    let mut doc: DocumentMut =
        source
            .parse()
            .map_err(|error: toml_edit::TomlError| WorkspaceError::InvalidSettings {
                message: format!("fix .unfold/config.toml first: {error}"),
                line: None,
                column: None,
            })?;
    if current.is_none() {
        doc.insert("version", toml_edit::value(1));
    }
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
            entrances(doc)?.push(entrance_table(entrance));
            Ok(())
        }
        SettingsEdit::ReplaceEntrance { index, entrance } => {
            let tables = entrances(doc)?;
            let slot = tables.get_mut(*index).ok_or_else(|| no_entrance(*index))?;
            // Keep the header's comments and position; replace the keys.
            slot.clear();
            for (key, value) in entrance_table(entrance) {
                slot.insert(&key, value);
            }
            Ok(())
        }
        SettingsEdit::RemoveEntrance { index } => {
            let tables = entrances(doc)?;
            if *index >= tables.len() {
                return Err(no_entrance(*index));
            }
            tables.remove(*index);
            if tables.is_empty() {
                doc.remove("entrance");
            }
            Ok(())
        }
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

fn no_entrance(index: usize) -> WorkspaceError {
    WorkspaceError::SettingsConflict(format!("there is no manual entrance at position {index}"))
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
                "`{path}` is excluded by a broader pattern (`{patterns}`); edit .unfold/config.toml to include it"
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
    item.as_table_mut().ok_or_else(|| {
        WorkspaceError::SettingsConflict(format!("`{key}` in .unfold/config.toml is not a table"))
    })
}

/// The array at `key` in `table`, created if absent.
fn array<'t>(table: &'t mut Table, key: &str) -> Result<&'t mut Array> {
    let item = table
        .entry(key)
        .or_insert_with(|| Item::Value(Value::Array(Array::new())));
    item.as_array_mut().ok_or_else(|| {
        WorkspaceError::SettingsConflict(format!("`{key}` in .unfold/config.toml is not an array"))
    })
}

fn entrances(doc: &mut DocumentMut) -> Result<&mut ArrayOfTables> {
    let item = doc
        .entry("entrance")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
    item.as_array_of_tables_mut().ok_or_else(|| {
        WorkspaceError::SettingsConflict("`entrance` in .unfold/config.toml is not a list of tables".into())
    })
}

fn entrance_table(spec: &EntranceSpec) -> Table {
    let mut table = Table::new();
    table.insert("kind", toml_edit::value(spec.kind.as_str()));
    if let Some(trigger) = &spec.trigger {
        table.insert("trigger", toml_edit::value(trigger.as_str()));
    }
    if let Some(method) = &spec.method {
        table.insert("method", toml_edit::value(method.as_str()));
    }
    if let Some(label) = &spec.label {
        table.insert("label", toml_edit::value(label.as_str()));
    }
    match spec.handler.as_slice() {
        [] => {}
        [one] => {
            table.insert("handler", toml_edit::value(one.as_str()));
        }
        many => {
            table.insert(
                "handler",
                Item::Value(Value::Array(many.iter().map(String::as_str).collect())),
            );
        }
    }
    if let Some(startup) = &spec.startup {
        table.insert("startup", toml_edit::value(startup.as_str()));
    }
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
    use super::*;

    const COMMENTED: &str = "# Shared settings.\nversion = 1\n\n[scope]\n# generated code\nexclude = [\"**/generated/**\"]\n\n# CLI dispatch\n[[entrance]]\nkind = \"command\"\ntrigger = \"import\"\nhandler = \"src/a.ts#run\"\n";

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
            edited
                .text
                .starts_with("# Shared settings.\nversion = 1\n\n[scope]\n# generated code\n")
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
            Some("version = 1\n[scope]\nexclude = [\"src/**\"]\n"),
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
    fn entrances_python_projects_and_detectors() {
        let spec = EntranceSpec {
            kind: "event".into(),
            trigger: Some("SIGTERM".into()),
            handler: vec!["src/server.ts#shutdown".into()],
            ..EntranceSpec::default()
        };
        let added =
            apply_edits(Some(COMMENTED), &[SettingsEdit::AddEntrance { entrance: spec }]).expect("add");
        assert_eq!(added.settings.entrances.len(), 2);
        let replaced = apply_edits(
            Some(&added.text),
            &[SettingsEdit::ReplaceEntrance {
                index: 0,
                entrance: EntranceSpec {
                    kind: "command".into(),
                    trigger: Some("export".into()),
                    handler: vec!["src/a.ts#auth".into(), "src/a.ts#run".into()],
                    ..EntranceSpec::default()
                },
            }],
        )
        .expect("replace");
        assert!(replaced.text.contains("# CLI dispatch\n[[entrance]]"));
        assert_eq!(replaced.settings.entrances[0].trigger.as_deref(), Some("export"));
        let removed =
            apply_edits(Some(&replaced.text), &[SettingsEdit::RemoveEntrance { index: 1 }]).expect("remove");
        assert_eq!(removed.settings.entrances.len(), 1);
        assert!(apply_edits(Some(&removed.text), &[SettingsEdit::RemoveEntrance { index: 5 }]).is_err());

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
        let text = "version = 1\n\n[scope]\n# keep me\ninclude = [\"src\"]\nexclude = [\"**/generated/**\", \"src/legacy\"]\n";
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
        assert_eq!(fresh.text, "version = 1\n\n[project]\nname = \"orders\"\n");
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
            Some("version = 1\nbogus = 2\n"),
            &[SettingsEdit::GraphOnly { path: "src".into() }],
        )
        .expect_err("invalid");
        assert!(matches!(
            error,
            WorkspaceError::InvalidSettings { line: Some(2), .. }
        ));
    }
}
