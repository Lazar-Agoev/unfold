//! Settings commands (docs/engine.md#shared-config, shared settings): the app preferences, a
//! project's `unfold.toml` with its shared name (read, and changed through validated
//! edits that keep its comments), and what the UI needs to sign its edits.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::vocab::{DetectorId, EntranceKind, HttpMethod, Language};
use unfold_engine::settings::{ManualTargets, ProcessStart, Settings};

use super::{NoArgs, ProjectArgs};
use crate::error::WorkspaceError;
use crate::events::WorkspaceEvent;
use crate::git;
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::projects::ProjectId;
use crate::settings_edit::{ApplicationSpec, LaunchTargetSpec, OverrideSpec, SettingsEdit};
use crate::state::Preferences;
use crate::workspace::{load_settings, settings_text};

/// `get_preferences`: the app preferences (defaults when none are saved).
pub fn get_preferences(ctx: &Ctx<'_>, _args: NoArgs) -> CommandResult<Preferences> {
    Ok(ctx.projects().app().preferences())
}

/// The arguments of `set_preferences`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetPreferencesArgs {
    /// Every preference (read them with `get_preferences`, change some, send them back).
    pub preferences: Preferences,
}

/// The most compile threads a preference may ask for.
const MAX_THREADS: u32 = 256;
/// The lowest port agents may connect on: ports below it are reserved for system services.
const MIN_AGENT_PORT: u16 = 1024;

/// `set_preferences`: saves the preferences and sends `preferences_changed` (with them) to every
/// window, so other windows apply a new theme or key bindings at once, and the main process starts
/// or stops the agents' server. The thread limit and background-update default apply to projects
/// opened afterwards.
pub fn set_preferences(ctx: &Ctx<'_>, args: SetPreferencesArgs) -> CommandResult<Preferences> {
    let preferences = args.preferences;
    if preferences
        .thread_limit
        .is_some_and(|limit| limit == 0 || limit > MAX_THREADS)
    {
        return Err(CommandError::invalid_args(format!(
            "the thread limit must be between 1 and {MAX_THREADS}"
        )));
    }
    if preferences.agents.port < MIN_AGENT_PORT {
        return Err(CommandError::invalid_args(format!(
            "the agents' port must be between {MIN_AGENT_PORT} and {}",
            u16::MAX
        )));
    }
    if preferences
        .editor_command
        .as_deref()
        .is_some_and(|command| command.contains('\0'))
    {
        return Err(CommandError::invalid_args(
            "the editor command contains a NUL character",
        ));
    }
    ctx.projects().app().set_preferences(&preferences)?;
    ctx.broadcast(&WorkspaceEvent::PreferencesChanged {
        preferences: preferences.clone(),
    });
    Ok(preferences)
}

/// A settings problem with its position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PythonProjectView {
    /// The Python project's folder (`""` is the project's folder).
    pub root: String,
    /// Import roots relative to the project's folder; absent means the front end's default.
    pub source_roots: Option<Vec<String>>,
    /// The grammar version, such as `3.14`.
    pub version: String,
}

/// One `[[entrance]]` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ManualEntranceView {
    /// Its ID (what edits name).
    pub id: String,
    /// The kind.
    pub kind: EntranceKind,
    /// The `[[application]]` it belongs to.
    pub application: Option<String>,
    /// The trigger.
    pub trigger: Option<String>,
    /// The HTTP method.
    pub method: Option<HttpMethod>,
    /// The label.
    pub label: Option<String>,
    /// The description.
    pub description: Option<String>,
    /// Handler locators, middleware first (every kind but `process`).
    pub handler: Vec<String>,
    /// The working folder (`process`; `.` is the project's folder).
    pub cwd: Option<String>,
    /// The command line (`process`).
    pub command: Option<String>,
    /// The startup locator (`process`).
    pub startup: Option<String>,
    /// The launch target (`process`).
    pub target: Option<LaunchTargetSpec>,
    /// The import path, in order (`process`), when set.
    pub python_paths: Option<Vec<String>>,
    /// Declared environment variables (`process`).
    pub env: BTreeMap<String, String>,
    /// The schedule, as written (`process`).
    pub schedule: Option<String>,
    /// The table's line in the file.
    pub line: u32,
}

/// The validated settings, as the settings editor shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    /// The languages to graph; absent means every supported language found.
    pub languages: Option<Vec<Language>>,
    /// `[scope] include` patterns.
    pub include: Vec<String>,
    /// `[scope] exclude` patterns.
    pub exclude: Vec<String>,
    /// Detectors turned off.
    pub disabled_detectors: Vec<DetectorId>,
    /// Python projects.
    pub python_projects: Vec<PythonProjectView>,
    /// Applications, in file order.
    pub applications: Vec<ApplicationSpec>,
    /// Manual entrances, in file order.
    pub entrances: Vec<ManualEntranceView>,
    /// Overrides of detected launches, in file order.
    pub overrides: Vec<OverrideSpec>,
    /// `[evidence] files` patterns, as written.
    pub evidence: Vec<String>,
}

impl From<&Settings> for SettingsView {
    fn from(settings: &Settings) -> Self {
        Self {
            languages: settings.languages.clone(),
            include: settings
                .scope
                .include()
                .iter()
                .map(|pattern| pattern.to_string())
                .collect(),
            exclude: settings
                .scope
                .exclude()
                .iter()
                .map(|pattern| pattern.to_string())
                .collect(),
            disabled_detectors: settings.disabled_detectors.clone(),
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
            applications: settings.applications.iter().map(ApplicationSpec::from).collect(),
            entrances: settings
                .entrances
                .iter()
                .map(|entrance| {
                    let text = |text: Option<&str>| text.map(str::to_owned);
                    let mut view = ManualEntranceView {
                        id: entrance.id.to_string(),
                        kind: entrance.kind,
                        application: text(entrance.application.as_deref()),
                        trigger: text(entrance.trigger.as_deref()),
                        method: entrance.method,
                        label: text(entrance.label.as_deref()),
                        description: text(entrance.description.as_deref()),
                        handler: Vec::new(),
                        cwd: None,
                        command: None,
                        startup: None,
                        target: None,
                        python_paths: None,
                        env: BTreeMap::new(),
                        schedule: None,
                        line: entrance.line,
                    };
                    match &entrance.targets {
                        ManualTargets::Handlers(locators) => {
                            view.handler = locators.iter().map(ToString::to_string).collect();
                        }
                        ManualTargets::Process(process) => {
                            view.cwd = Some(process.cwd.as_str().to_owned());
                            view.command = text(process.command.as_deref());
                            match &process.start {
                                Some(ProcessStart::Startup(locator)) => {
                                    view.startup = Some(locator.to_string());
                                }
                                Some(ProcessStart::Target(target)) => {
                                    view.target = Some(LaunchTargetSpec::from(target));
                                }
                                None => {}
                            }
                            view.python_paths = process
                                .python_paths
                                .as_ref()
                                .map(|paths| paths.iter().map(|path| path.as_str().to_owned()).collect());
                            view.env = process
                                .env
                                .iter()
                                .map(|(name, value)| (name.to_string(), value.to_string()))
                                .collect();
                            view.schedule = text(process.schedule.as_deref());
                        }
                    }
                    view
                })
                .collect(),
            overrides: settings.overrides.iter().map(OverrideSpec::from).collect(),
            evidence: settings
                .evidence
                .patterns()
                .iter()
                .map(|pattern| pattern.to_string())
                .collect(),
        }
    }
}

/// A project's shared name, as Project settings shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SharedName {
    /// The name other projects' boards use for this one.
    pub value: String,
    /// Whether it is the default made from the folder (no `[project] name`).
    pub is_default: bool,
}

/// `unfold.toml` as Project settings sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfig {
    /// Whether the file exists (absent means every default).
    pub exists: bool,
    /// The file's text.
    pub text: Option<String>,
    /// The validated settings, unless the file is invalid.
    pub settings: Option<SettingsView>,
    /// Why the file is invalid, with its position.
    pub error: Option<SettingsProblem>,
    /// The project's shared name (the default while the file is invalid).
    pub name: SharedName,
}

fn project_config_of(
    root: &std::path::Path,
    text: Option<String>,
    settings: Result<Settings, WorkspaceError>,
) -> ProjectConfig {
    let name = match settings
        .as_ref()
        .ok()
        .and_then(|settings| settings.project_name.clone())
    {
        Some(value) => SharedName {
            value,
            is_default: false,
        },
        None => SharedName {
            value: git::default_name(root),
            is_default: true,
        },
    };
    let (settings, error) = match settings {
        Ok(settings) => (Some(SettingsView::from(&settings)), None),
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
        name,
    }
}

/// `project_config`: the settings file's text, its validated settings or why it is invalid,
/// and the project's shared name. Read from disk; never wakes the project.
pub fn project_config(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<ProjectConfig> {
    let project = ctx.project(&args.project)?;
    let root = project.root();
    Ok(project_config_of(root, settings_text(root)?, load_settings(root)))
}

/// The arguments of `update_project_config`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateProjectConfigArgs {
    /// The project.
    pub project: ProjectId,
    /// The edits, applied in order; all or nothing.
    pub edits: Vec<SettingsEdit>,
}

/// `update_project_config`: applies validated edits (scope, applications, manual entrances,
/// overrides, Python projects, detector toggles, the shared name, evidence files), keeping
/// comments and formatting; an edit that would make
/// the file invalid is refused with `invalid_settings` and nothing is written. Sends
/// `config_changed`; a new shared name reaches `workspace.json` and every window
/// (`workspace_changed`) as the host observes it.
pub fn update_project_config(ctx: &Ctx<'_>, args: UpdateProjectConfigArgs) -> CommandResult<ProjectConfig> {
    let workspace = ctx.live(&args.project)?;
    let outcome = workspace.edit_settings(&args.edits)?;
    let text = workspace.settings_text()?;
    Ok(project_config_of(workspace.root(), text, Ok(outcome.settings)))
}

/// Facts about the app the UI can't get itself (it has no Node APIs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// The person's username, which signs their board edits (`person:<username>`).
    pub username: String,
    /// The engine version.
    pub engine_version: String,
}

/// `app_info`: the username that signs edits, and the engine version.
pub fn app_info(_ctx: &Ctx<'_>, _args: NoArgs) -> CommandResult<AppInfo> {
    let username = super::diagnostics::username().unwrap_or_else(|| "unknown".to_owned());
    Ok(AppInfo {
        username,
        engine_version: unfold_engine::ENGINE_VERSION.to_owned(),
    })
}
