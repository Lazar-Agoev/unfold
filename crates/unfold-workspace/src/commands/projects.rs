//! Project commands for Home and Project settings (contract §4.3): the summary behind Home's
//! header and Health line, what a Compile would graph and skip (the first-Compile screen,
//! architecture §11.6), and one level of the scope tree.

use serde::Serialize;
use ts_rs::TS;
use unfold_core::RepoPath;
use unfold_core::vocab::Language;
use unfold_engine::discovery::{IgnoreRules, SkipReason, Unindexed, discover, is_default_excluded};
use unfold_engine::settings::{Scope, ScopeStatus, Settings};

use super::{ProjectArgs, ProjectPathArgs, board_hooks, count, entrances};
use crate::git::{self, GitInfo};
use crate::ipc::context::Ctx;
use crate::ipc::error::{CommandError, CommandResult};
use crate::layout::{FallbackReason, Location};
use crate::problems::{ProblemCategory, project_problem_count};
use crate::projects::ProjectId;
use crate::status::{GraphStatus, ProjectState};

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
    /// The folder, relative to the project's folder (`""` is the folder itself).
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
pub struct CompilePreview {
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
    /// Why the settings can't be used (the preview then uses the defaults).
    pub settings_error: Option<String>,
    /// Parts of the folder discovery couldn't read.
    pub problems: Vec<String>,
}

/// `compile_preview`: what a Compile would graph and skip. Walks the folder; writes nothing.
pub fn compile_preview(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<CompilePreview> {
    let workspace = ctx.live(&args.project)?;
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
    Ok(CompilePreview {
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

/// The most folders one `scope_dir` level returns.
pub const MAX_SCOPE_FOLDERS: usize = 2_000;

/// One folder of the scope tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScopeFolder {
    /// The folder's name.
    pub name: String,
    /// Its path relative to the project's folder.
    pub path: String,
    /// Why discovery skips it or `[scope]` leaves it out, if it does.
    pub skipped: Option<SkipKind>,
    /// Source files discovery finds under it, ignoring `[scope]`.
    pub sources: u32,
}

/// One level of the scope tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScopeDir {
    /// The listed folder (`""` for the project's folder).
    pub path: String,
    /// Its folders, by name.
    pub folders: Vec<ScopeFolder>,
    /// It has more than [`MAX_SCOPE_FOLDERS`] folders; the rest are left out.
    pub truncated: bool,
}

/// `scope_dir`: one level of the scope tree, folders only, with what skips each and how many
/// source files are under it.
pub fn scope_dir(ctx: &Ctx<'_>, args: ProjectPathArgs) -> CommandResult<ScopeDir> {
    let workspace = ctx.live(&args.project)?;
    let folder = workspace.confine(&args.path)?;
    let read = std::fs::read_dir(folder.absolute())
        .map_err(|error| CommandError::not_found(format!("cannot list `{}`: {error}", args.path)))?;
    let settings = workspace.settings().unwrap_or_default();
    let options = &workspace.options().engine.discover;
    let mut names: Vec<String> = read
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect();
    names.sort();
    let truncated = names.len() > MAX_SCOPE_FOLDERS;
    names.truncate(MAX_SCOPE_FOLDERS);
    ctx.check_cancelled()?;
    // Sources ignoring `[scope]`: what each folder would hold if it were in scope.
    let unscoped = Settings {
        scope: Scope::default(),
        ..settings.clone()
    };
    let mut sources: Vec<String> = discover(workspace.root(), &unscoped, options)
        .map_err(|error| CommandError::new("not_a_folder", error.to_string()))?
        .sources
        .into_iter()
        .map(|source| source.path.as_str().to_owned())
        .collect();
    sources.sort_unstable();
    let mut ignore = IgnoreRules::new(workspace.root(), options);
    let mut folders = Vec::with_capacity(names.len());
    for name in names {
        let path = if folder.as_str().is_empty() {
            name.clone()
        } else {
            format!("{}/{name}", folder.as_str())
        };
        let Ok(relative) = RepoPath::new(&path) else {
            continue;
        };
        let skipped = if path == crate::layout::UNFOLD_DIR || ignore.is_ignored(&relative, true) {
            Some(
                if is_default_excluded(&path) || path == crate::layout::UNFOLD_DIR {
                    SkipKind::Default
                } else {
                    SkipKind::Ignored
                },
            )
        } else {
            folder_scope(&relative, &settings)
        };
        let prefix = format!("{path}/");
        let start = sources.partition_point(|source| source.as_str() < prefix.as_str());
        let under = sources[start..]
            .iter()
            .take_while(|source| source.starts_with(&prefix))
            .count();
        folders.push(ScopeFolder {
            name,
            path,
            skipped,
            sources: count(under),
        });
    }
    Ok(ScopeDir {
        path: folder.as_str().to_owned(),
        folders,
        truncated,
    })
}

/// Whether `[scope]` leaves a whole folder out: excluded, or outside every `include` pattern
/// with none of them inside it.
fn folder_scope(folder: &RepoPath, settings: &Settings) -> Option<SkipKind> {
    match settings.scope.status(folder) {
        ScopeStatus::InScope => None,
        ScopeStatus::Excluded => Some(SkipKind::Excluded),
        ScopeStatus::NotIncluded => {
            let prefix = format!("{}/", folder.as_str());
            let holds_included = settings
                .scope
                .include()
                .iter()
                .any(|pattern| pattern.starts_with(&prefix) || pattern.starts_with("**"));
            (!holds_included).then_some(SkipKind::NotIncluded)
        }
    }
}

/// A board whose health has items (Home's Health line).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardHealthCount {
    /// The board's id.
    pub id: String,
    /// Its title.
    pub title: String,
    /// How many health items it has.
    pub items: u32,
}

/// The most boards the Health line lists.
pub const MAX_HEALTH_BOARDS: usize = 20;

/// Home's Health line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHealth {
    /// The problems that make the row's state, equal to its `problems` count: files that don't
    /// parse or can't be read, settings errors, discovery, input, and cache problems, and a failed
    /// compile. Board health isn't counted (boards that need review are counted apart); the
    /// `problems` command lists both.
    pub problems: u32,
    /// Files that don't parse or can't be read.
    pub doesnt_parse: u32,
    /// Why the settings file can't be used, if it can't.
    pub settings_error: Option<String>,
    /// Entrances no launch reaches.
    pub unreached_entrances: u32,
    /// Launches into code that isn't indexed.
    pub unindexed_launches: u32,
    /// Boards whose health has items.
    pub boards_needing_review: u32,
    /// The boards with health items, most items first (at most 20).
    pub boards: Vec<BoardHealthCount>,
}

/// Home's header and Health line for one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    /// The project.
    pub project: ProjectId,
    /// Its shared name.
    pub name: String,
    /// The canonical path of its folder.
    pub path: String,
    /// Its row state.
    pub state: ProjectState,
    /// Its graph's status (fallback location, watching, background updates, …).
    pub status: GraphStatus,
    /// When the graph was last committed, in milliseconds since the Unix epoch.
    pub updated_at_ms: Option<i64>,
    /// Indexed files.
    pub files: u32,
    /// The languages graphed, sorted.
    pub languages: Vec<Language>,
    /// Applications (distinct startups).
    pub applications: u32,
    /// Entrances other than launches.
    pub entrances: u32,
    /// The Health line.
    pub health: ProjectHealth,
    /// The repository's branch and worktrees, or `None` outside a repository.
    pub git: Option<GitInfo>,
}

/// `project_summary`: Home's header and Health line.
pub fn project_summary(ctx: &Ctx<'_>, args: ProjectArgs) -> CommandResult<ProjectSummary> {
    let project = ctx.project(&args.project)?;
    let workspace = project.wake()?;
    let problems = workspace.problems();
    let (files, _, languages) = workspace.graph_counts();
    let (updated_at_ms, catalog) = match workspace.readers() {
        Ok(pool) => {
            let reader = pool.get()?;
            let updated = reader.meta()?.graph.map(|graph| graph.compiled_at_ms);
            (updated, entrances::Counts::of(&reader)?)
        }
        Err(_) => (None, entrances::Counts::default()),
    };
    let doesnt_parse = problems
        .iter()
        .filter(|problem| {
            matches!(
                problem.category,
                ProblemCategory::DoesntParse | ProblemCategory::CantRead
            )
        })
        .count();
    let mut boards = board_hooks::health_rows(&project.extras());
    let boards_needing_review = count(boards.len());
    boards.truncate(MAX_HEALTH_BOARDS);
    Ok(ProjectSummary {
        project: args.project,
        name: project.name().to_owned(),
        path: project.root().to_string_lossy().into_owned(),
        state: workspace.project_state(),
        status: workspace.status(),
        updated_at_ms,
        files,
        languages,
        applications: catalog.applications,
        entrances: catalog.entrances,
        health: ProjectHealth {
            problems: project_problem_count(&problems),
            doesnt_parse: count(doesnt_parse),
            settings_error: workspace.settings().err().map(|error| error.to_string()),
            unreached_entrances: catalog.unreached,
            unindexed_launches: catalog.unindexed_launches,
            boards_needing_review,
            boards,
        },
        git: git::info(project.root()),
    })
}
