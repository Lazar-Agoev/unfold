//! Settings commands (docs/engine.md#shared-config, shared settings): the app settings, a
//! project's `unfold.toml` (read, and changed through the engine's validated edits that keep its
//! comments, `unfold_engine::settings::edit`), and what the UI needs to sign its edits.

use serde::{Deserialize, Serialize};
use unfold_core::vocab::Language;
use unfold_engine::settings::SettingsFile;
use unfold_engine::settings::edit::{EntryEdit, SettingsEdit};

use super::entrances::{AppView, app_views};
use super::{NoArgs, ProjectArgs};
use crate::error::WorkspaceError;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::state::AppSettings;
use crate::workspace::{settings_error, settings_text};

/// `get_settings`: the app settings (defaults when none are saved).
pub fn get_settings(ctx: &Ctx<'_>, _args: NoArgs) -> CommandResult<AppSettings> {
    Ok(ctx.projects().app().settings())
}

/// The arguments of `set_settings`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SetSettingsArgs {
    /// Every setting (read them with `get_settings`, change some, send them back).
    pub settings: AppSettings,
}

/// `set_settings`: saves the app settings and returns them. Main process only: the editor command
/// is a program `open_in_editor` runs, so only the Settings dialog, through the main process, sets
/// it.
pub fn set_settings(ctx: &Ctx<'_>, args: SetSettingsArgs) -> CommandResult<AppSettings> {
    ctx.require_trusted("set_settings")?;
    let settings = args.settings;
    if settings
        .editor_command
        .as_deref()
        .is_some_and(|command| command.contains('\0'))
    {
        return Err(CommandError::invalid_args(
            "the editor command contains a NUL character",
        ));
    }
    ctx.projects().app().set_settings(&settings)?;
    Ok(settings)
}

/// A settings problem with its position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SettingsProblem {
    /// What is wrong.
    pub message: String,
    /// The one-based line, when known.
    pub line: Option<u32>,
    /// The one-based column, when known.
    pub column: Option<u32>,
}

/// One `[[python.project]]` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PythonProjectView {
    /// The Python project's folder (`""` is the project's folder).
    pub root: String,
    /// Import roots relative to the project's folder; absent means the front end's default.
    pub source_roots: Option<Vec<String>>,
    /// The grammar version, such as `3.14`.
    pub version: String,
}

/// The validated settings and apps, as the settings editor shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    /// The languages to graph; absent means every supported language found.
    pub languages: Option<Vec<Language>>,
    /// `[scope] include` patterns.
    pub include: Vec<String>,
    /// `[scope] exclude` patterns.
    pub exclude: Vec<String>,
    /// Python projects.
    pub python_projects: Vec<PythonProjectView>,
    /// `[evidence] files` patterns, as written.
    pub evidence: Vec<String>,
    /// The `[[app]]` tables as written, in file order (their code without states: `entrances`
    /// has those).
    pub apps: Vec<AppView>,
}

impl SettingsView {
    /// The view of `file`, whose text is `text`.
    fn new(file: &SettingsFile, text: Option<&str>) -> Self {
        let settings = &file.settings;
        let patterns = |patterns: &[Box<str>]| patterns.iter().map(ToString::to_string).collect();
        Self {
            languages: settings.languages.clone(),
            include: patterns(settings.scope.include()),
            exclude: patterns(settings.scope.exclude()),
            python_projects: settings
                .python_projects
                .iter()
                .map(|project| PythonProjectView {
                    root: project.root.as_str().to_owned(),
                    source_roots: project
                        .source_roots
                        .as_ref()
                        .map(|roots| roots.iter().map(|root| root.as_str().to_owned()).collect()),
                    version: project.version.as_str().to_owned(),
                })
                .collect(),
            evidence: patterns(settings.evidence.patterns()),
            apps: app_views(&file.apps, text, None),
        }
    }
}

/// A project's `unfold.toml`: its text and what it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfig {
    /// Whether the file exists (absent means every default).
    pub exists: bool,
    /// The file's text.
    pub text: Option<String>,
    /// The validated settings and apps, unless the file is invalid.
    pub settings: Option<SettingsView>,
    /// Why the file is invalid, with its position.
    pub error: Option<SettingsProblem>,
}

fn project_config_of(text: Option<String>, file: Result<SettingsFile, WorkspaceError>) -> ProjectConfig {
    let (settings, error) = match file {
        Ok(file) => (Some(SettingsView::new(&file, text.as_deref())), None),
        Err(WorkspaceError::InvalidSettings {
            message,
            line,
            column,
        }) => (
            None,
            Some(SettingsProblem {
                message,
                line,
                column,
            }),
        ),
        Err(other) => (
            None,
            Some(SettingsProblem {
                message: other.to_string(),
                line: None,
                column: None,
            }),
        ),
    };
    ProjectConfig {
        exists: text.is_some(),
        text,
        settings,
        error,
    }
}

/// `project_config`: the settings file's text, and its validated settings and apps or why it
/// is invalid. Read from disk; never wakes the project.
pub fn project_config(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<ProjectConfig> {
    let project = ctx.project(&args.project)?;
    let root = project.root();
    let text = settings_text(root)?;
    let file = match text.as_deref() {
        Some(text) => SettingsFile::parse(text).map_err(settings_error),
        None => Ok(SettingsFile::default()),
    };
    Ok(project_config_of(text, file))
}

/// The arguments of `update_project_config`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct UpdateProjectConfigArgs {
    /// The project.
    pub project: ProjectId,
    /// The edits, applied in order; all or nothing.
    pub edits: Vec<SettingsEdit>,
}

/// What `update_project_config` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfigUpdate {
    /// The file after the edits.
    pub config: ProjectConfig,
    /// The unified diff from the old text to the new; empty when the edits changed nothing.
    pub diff: String,
    /// What each app and entrance edit did, in order: added, changed, or removed, with the
    /// names the file writes; empty when the edits had none.
    pub entries: Vec<EntryEdit>,
}

/// The most edits one `update_project_config` applies.
pub const MAX_EDITS: usize = 200;

/// `update_project_config`: applies validated edits (scope, Python projects, evidence files,
/// and apps and entrances: `set_app`, `set_entrance`, `add_entrance`, `remove_entrance`,
/// `replace_code`), keeping comments and formatting and the file's line endings, and returns the
/// diff and what each app and entrance edit did; an edit that would make the file invalid is
/// refused with `invalid_settings`, and one that doesn't fit it (an app or entrance it doesn't
/// have, a key `add_entrance` finds taken, a code key `replace_code` no longer finds in its
/// place) with `settings_conflict`, and nothing is written. Sends `config_changed`. An edit of only `[[app]]`
/// tables resolves the entrances again without compiling before it returns
/// (`entrances_changed` follows when they changed); other edits apply with the next background
/// update.
pub fn update_project_config(
    ctx: &Ctx<'_>,
    args: UpdateProjectConfigArgs,
) -> CommandResult<ProjectConfigUpdate> {
    if args.edits.len() > MAX_EDITS {
        return Err(CommandError::invalid_args(format!(
            "at most {MAX_EDITS} edits at once"
        )));
    }
    let workspace = ctx.live(&args.project)?;
    let outcome = workspace.edit_settings(&args.edits)?;
    let exists = outcome.written || workspace.settings_text()?.is_some();
    let config = ProjectConfig {
        exists,
        settings: Some(SettingsView::new(&outcome.file, Some(&outcome.text))),
        text: exists.then_some(outcome.text),
        error: None,
    };
    Ok(ProjectConfigUpdate {
        config,
        diff: outcome.diff,
        entries: outcome.entries,
    })
}

/// Facts about the app the UI can't get itself (it has no Node APIs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// The person's username, which signs their board edits (`person:<username>`).
    pub username: String,
    /// The engine version.
    pub engine_version: String,
}

/// `app_info`: the username that signs edits, and the engine version.
pub fn app_info(_ctx: &Ctx<'_>, _args: NoArgs) -> CommandResult<AppInfo> {
    Ok(AppInfo {
        username: username().unwrap_or_else(|| "unknown".to_owned()),
        engine_version: unfold_engine::ENGINE_VERSION.to_owned(),
    })
}

/// The username from the environment.
fn username() -> Option<String> {
    ["USER", "LOGNAME"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.trim().is_empty()))
}
