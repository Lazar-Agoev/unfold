//! Folder commands: the first-Compile summary (architecture §11.6) and the recent folders.
//! Closing a window's folder is the main process's `close_folder` window command: it re-attaches
//! the window with no folder, and the engine host's `closeWorkspace` detaches it here.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_core::vocab::Language;
use unfold_engine::discovery::{SkipReason, Unindexed, discover};
use unfold_engine::settings::Settings;

use super::{NoArgs, count};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::layout::{FallbackReason, Location};
use crate::state::RecentState;
use crate::status::GraphStatus;

/// Why discovery skipped something (the engine's `SkipReason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SkipKind {
    /// An always-excluded folder such as `node_modules`.
    Default,
    /// A `.gitignore`, `.ignore`, or excludes rule.
    Ignored,
    /// An `exclude` pattern in `[scope]`.
    Excluded,
    /// Outside the `include` patterns.
    NotIncluded,
    /// The language is turned off in settings.
    LanguageOff,
    /// JavaScript, which V1 doesn't graph.
    #[serde(rename = "javascript")]
    JavaScript,
    /// A symlink that isn't followed.
    Symlink,
    /// A file name Unfold can't represent.
    InvalidPath,
}

impl From<SkipReason> for SkipKind {
    fn from(reason: SkipReason) -> Self {
        match reason {
            SkipReason::Default => Self::Default,
            SkipReason::Ignored => Self::Ignored,
            SkipReason::Excluded => Self::Excluded,
            SkipReason::NotIncluded => Self::NotIncluded,
            SkipReason::LanguageOff => Self::LanguageOff,
            SkipReason::JavaScript => Self::JavaScript,
            SkipReason::Symlink => Self::Symlink,
            SkipReason::InvalidPath => Self::InvalidPath,
        }
    }
}

impl From<Unindexed> for SkipKind {
    fn from(reason: Unindexed) -> Self {
        match reason {
            Unindexed::Excluded => Self::Excluded,
            Unindexed::NotIncluded => Self::NotIncluded,
            Unindexed::LanguageOff => Self::LanguageOff,
            Unindexed::JavaScript => Self::JavaScript,
        }
    }
}

/// How many files of one language would be graphed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LanguageCount {
    /// The language.
    pub language: Language,
    /// Its indexed files.
    pub files: u32,
}

/// One line of the skipped-files report ("Skipped: node_modules, .venv, dist").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SkippedFolder {
    /// The folder, relative to the open folder (`""` is the folder itself).
    pub folder: String,
    /// Why.
    pub reason: SkipKind,
    /// The reason in words.
    pub reason_text: String,
    /// The folder wasn't walked at all, so its files weren't counted.
    pub whole_folder: bool,
    /// Skipped source-like files counted in it.
    pub files: u32,
}

/// What the first-Compile screen shows before anything is written (architecture §11.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FolderSummary {
    /// The canonical folder path.
    pub folder: String,
    /// The folder's name.
    pub name: String,
    /// Whether the folder has a graph (or one is being built).
    pub compiled: bool,
    /// The graph status.
    pub status: GraphStatus,
    /// Files that would be graphed, by language.
    pub languages: Vec<LanguageCount>,
    /// Every file that would be graphed.
    pub files: u32,
    /// Source-like files that exist but wouldn't be graphed.
    pub unindexed: u32,
    /// What was skipped, aggregated by folder.
    pub skipped: Vec<SkippedFolder>,
    /// Why `local/` would live in the fallback location, if it would.
    pub fallback: Option<FallbackReason>,
    /// Where the graph and drafts would be kept: `.unfold/local` in the folder, or the absolute
    /// fallback location.
    pub local_path: String,
    /// Whether `.unfold/config.toml` exists.
    pub config_exists: bool,
    /// Why the settings can't be used (the summary then uses the defaults).
    pub settings_error: Option<String>,
    /// Parts of the folder discovery couldn't read.
    pub problems: Vec<String>,
}

/// `folder_summary`: what a Compile would graph and skip. Reads the folder; writes nothing.
pub fn folder_summary(ctx: &Ctx<'_>, _args: NoArgs) -> CommandResult<FolderSummary> {
    let workspace = ctx.workspace()?;
    let (settings, settings_error) = match workspace.settings() {
        Ok(settings) => (settings, None),
        Err(error) => (Settings::default(), Some(error.to_string())),
    };
    ctx.check_cancelled()?;
    let found = discover(workspace.root(), &settings, &workspace.options().engine.discover)
        .map_err(|error| CommandError::new("not_a_folder", error.to_string()))?;
    let mut languages: Vec<LanguageCount> = Vec::new();
    for language in Language::ALL {
        let files = found.sources_of(*language).count();
        if files > 0 {
            languages.push(LanguageCount {
                language: *language,
                files: count(files),
            });
        }
    }
    let layout = workspace.layout();
    let fallback = match layout.location() {
        Location::InFolder => None,
        Location::Fallback(reason) => Some(reason),
    };
    let local_path = match fallback {
        None => crate::layout::LOCAL_PATH.to_owned(),
        Some(_) => layout.local().to_string_lossy().into_owned(),
    };
    Ok(FolderSummary {
        folder: workspace.folder().to_owned(),
        name: workspace.name().to_owned(),
        compiled: workspace.is_compiled(),
        status: workspace.status(),
        languages,
        files: count(found.sources.len()),
        unindexed: count(found.unindexed.len()),
        skipped: found
            .skipped
            .iter()
            .map(|skipped| SkippedFolder {
                folder: skipped.folder.as_str().to_owned(),
                reason: skipped.reason.into(),
                reason_text: skipped.reason.describe().to_owned(),
                whole_folder: skipped.whole_folder,
                files: skipped.files,
            })
            .collect(),
        fallback,
        local_path,
        config_exists: layout.config().is_file(),
        settings_error,
        problems: found
            .problems
            .iter()
            .map(|problem| {
                if problem.path.is_empty() {
                    problem.message.to_string()
                } else {
                    format!("{}: {}", problem.path, problem.message)
                }
            })
            .collect(),
    })
}

/// `recent_folders`: the recent folders, most recent first, and the welcome window's bounds.
pub fn recent_folders(ctx: &Ctx<'_>, _args: NoArgs) -> CommandResult<RecentState> {
    Ok(ctx.registry().app().recent())
}

/// The arguments of `forget_recent_folder`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ForgetRecentArgs {
    /// The folder's canonical path, as `recent_folders` lists it.
    pub path: String,
}

/// `forget_recent_folder`: removes a folder from the recent list.
pub fn forget_recent_folder(ctx: &Ctx<'_>, args: ForgetRecentArgs) -> CommandResult<RecentState> {
    Ok(ctx.registry().app().remove_recent(&args.path)?)
}
