//! The parallel walk behind [`super::discover`], and single-path ignore checks for watchers.

use std::ffi::OsStr;
use std::path::{Component, Path};
use std::sync::mpsc;

use ignore::{DirEntry, IncrementalIgnore, WalkBuilder, WalkState};
use rustc_hash::FxHashSet;
use unfold_core::RepoPath;
use unfold_core::vocab::InputRole;

use super::classify::{
    FileClass, Unindexed, classify, is_default_excluded, is_default_excluded_name, is_source_like,
};
use super::report::{SkipEvent, SkipReason, aggregate};
use super::{
    ContextFile, DiscoverOptions, Discovery, FileStamp, InputFile, Problem, SourceFile, UnindexedFile,
};
use crate::settings::{FolderPath, SETTINGS_PATH, ScopeStatus, Settings};

/// What one walker thread reports.
enum Found {
    File(RepoPath, FileClass, FileStamp),
    Skip(SkipEvent),
    /// A walked folder's subfolders and source-like files, to find what ignore rules hid.
    Folder(String, Vec<(String, bool)>),
    /// A yielded entry that a [`Found::Folder`] may list.
    Seen(String),
    Problem(Problem),
}

fn builder(root: &Path, options: &DiscoverOptions) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .parents(true)
        .ignore(true)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(options.global_ignore)
        // Honor .gitignore in plain folders too: Unfold doesn't need a repository.
        .require_git(false)
        .follow_links(false)
        .threads(options.threads);
    builder
}

pub(super) fn walk(root: &Path, settings: &Settings, options: &DiscoverOptions) -> Discovery {
    let (sender, receiver) = mpsc::channel();
    builder(root, options).build_parallel().run(|| {
        let sender = sender.clone();
        Box::new(move |result| {
            let mut send = |found: Found| {
                // The receiver outlives the walk, so sending can't fail.
                let _ = sender.send(found);
            };
            match result {
                Ok(entry) => visit(root, settings, &entry, &mut send),
                Err(error) => {
                    send(Found::Problem(problem(root, &error)));
                    WalkState::Continue
                }
            }
        })
    });
    drop(sender);
    collect(root, receiver.into_iter())
}

fn visit(root: &Path, settings: &Settings, entry: &DirEntry, send: &mut impl FnMut(Found)) -> WalkState {
    if entry.depth() == 0 {
        send(Found::Folder(String::new(), children(entry.path())));
        return WalkState::Continue;
    }
    let Some(relative) = relative(root, entry.path()) else {
        let parent = entry.path().parent().and_then(|parent| relative(root, parent));
        send(Found::Skip(SkipEvent {
            folder: folder_of(parent.as_deref()),
            reason: SkipReason::InvalidPath,
            whole_folder: false,
        }));
        return WalkState::Skip;
    };
    let name = entry.file_name().to_string_lossy();
    let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
    if is_dir || is_source_like(&name) || entry.path_is_symlink() {
        send(Found::Seen(relative.clone()));
    }
    let Ok(path) = RepoPath::new(&relative) else {
        send(Found::Skip(SkipEvent {
            folder: folder_of(parent_of(&relative)),
            reason: SkipReason::InvalidPath,
            whole_folder: is_dir,
        }));
        return WalkState::Skip;
    };
    if entry.path_is_symlink() {
        symlink(root, settings, entry.path(), path, send);
        return WalkState::Continue;
    }
    if is_dir {
        if is_default_excluded_name(&name) {
            if !is_unlisted_default(&name) {
                send(Found::Skip(SkipEvent {
                    folder: FolderPath::parse(&relative).unwrap_or_default(),
                    reason: SkipReason::Default,
                    whole_folder: true,
                }));
            }
            return WalkState::Skip;
        }
        send(Found::Folder(relative, children(entry.path())));
        return WalkState::Continue;
    }
    let class = classify(&path, settings);
    if class != FileClass::Other {
        match entry.metadata() {
            Ok(metadata) => file(path, class, FileStamp::of(&metadata), send),
            Err(error) => send(Found::Problem(problem(root, &error))),
        }
    }
    WalkState::Continue
}

/// A symlink entry: a folder is never followed; a file counts only when its target is a file
/// inside the folder.
fn symlink(root: &Path, settings: &Settings, full: &Path, path: RepoPath, send: &mut impl FnMut(Found)) {
    let skip = |whole_folder: bool, path: &RepoPath| {
        let folder = if whole_folder {
            FolderPath::parse(path.as_str()).unwrap_or_default()
        } else {
            folder_of(path.parent())
        };
        Found::Skip(SkipEvent {
            folder,
            reason: SkipReason::Symlink,
            whole_folder,
        })
    };
    let class = classify(&path, settings);
    let target = std::fs::metadata(full);
    match target {
        Ok(metadata) if metadata.is_dir() => send(skip(true, &path)),
        Ok(metadata) if metadata.is_file() => {
            let inside = std::fs::canonicalize(full).is_ok_and(|target| target.starts_with(root));
            if !inside {
                if class != FileClass::Other {
                    send(skip(false, &path));
                }
            } else if class != FileClass::Other {
                file(path, class, FileStamp::of(&metadata), send);
            }
        }
        // Dangling, or something other than a file or folder.
        _ => {
            if class != FileClass::Other {
                send(skip(false, &path));
            }
        }
    }
}

fn file(path: RepoPath, class: FileClass, stamp: FileStamp, send: &mut impl FnMut(Found)) {
    if let FileClass::Unindexed(reason) = class {
        let reason = match reason {
            Unindexed::Excluded => SkipReason::Excluded,
            Unindexed::NotIncluded => SkipReason::NotIncluded,
            Unindexed::LanguageOff => SkipReason::LanguageOff,
            Unindexed::JavaScript => SkipReason::JavaScript,
        };
        send(Found::Skip(SkipEvent {
            folder: folder_of(path.parent()),
            reason,
            whole_folder: false,
        }));
    }
    send(Found::File(path, class, stamp));
}

/// Git's and Unfold's own folders are always there; listing them in the report is noise.
fn is_unlisted_default(name: &str) -> bool {
    name == ".git" || name == ".unfold"
}

/// A folder's subfolders and source-like files: `(name, is_folder)`. Entries whose names
/// aren't UTF-8 are left out; the walk reports those itself.
fn children(folder: &Path) -> Vec<(String, bool)> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let is_dir = entry.file_type().is_ok_and(|kind| kind.is_dir());
            (is_dir || is_source_like(&name)).then_some((name, is_dir))
        })
        .collect()
}

fn collect(root: &Path, found: impl Iterator<Item = Found>) -> Discovery {
    let mut discovery = Discovery::default();
    let mut skips = Vec::new();
    let mut folders = Vec::new();
    let mut seen = FxHashSet::default();
    for item in found {
        match item {
            Found::File(path, class, stamp) => match class {
                FileClass::Source(language) => discovery.sources.push(SourceFile {
                    path,
                    language,
                    stamp,
                }),
                FileClass::Context(language, scope) => {
                    discovery.context.push(ContextFile {
                        path,
                        language,
                        scope,
                        stamp,
                    });
                }
                FileClass::Unindexed(reason) => {
                    discovery.unindexed.push(UnindexedFile { path, reason, stamp });
                }
                FileClass::Input(role, scope) => discovery.inputs.push(InputFile {
                    path,
                    role,
                    scope,
                    stamp,
                }),
                FileClass::Other => {}
            },
            Found::Skip(event) => skips.push(event),
            Found::Folder(path, children) => folders.push((path, children)),
            Found::Seen(path) => {
                seen.insert(path);
            }
            Found::Problem(problem) => discovery.problems.push(problem),
        }
    }
    // Children a walked folder holds that the walker never yielded were hidden by ignore rules.
    for (folder, children) in folders {
        for (name, is_dir) in children {
            let path = if folder.is_empty() {
                name.clone()
            } else {
                format!("{folder}/{name}")
            };
            if seen.contains(&path) {
                continue;
            }
            // An ignored `node_modules` is reported as the default exclusion it also is.
            let reason = if is_default_excluded_name(&name) {
                SkipReason::Default
            } else {
                SkipReason::Ignored
            };
            if (reason == SkipReason::Default && (!is_dir || is_unlisted_default(&name)))
                || is_default_excluded(&folder)
            {
                continue;
            }
            let at = if is_dir {
                Some(path.as_str())
            } else {
                parent_of(&path)
            };
            skips.push(SkipEvent {
                folder: folder_of(at),
                reason,
                whole_folder: is_dir,
            });
        }
    }
    // The walk finds the settings file too unless an ignore file lists it; it counts either way.
    discovery
        .inputs
        .retain(|input| input.role != InputRole::UnfoldConfig);
    if let Some(config) = settings_input(root) {
        discovery.inputs.push(config);
    }
    discovery.sources.sort_by(|a, b| a.path.cmp(&b.path));
    discovery.context.sort_by(|a, b| a.path.cmp(&b.path));
    discovery.unindexed.sort_by(|a, b| a.path.cmp(&b.path));
    discovery.inputs.sort_by(|a, b| a.path.cmp(&b.path));
    discovery.problems.sort();
    lift(&mut skips, &discovery.sources);
    discovery.skipped = aggregate(skips);
    discovery
}

/// Moves each skipped-file event up to the highest folder below the root that holds no indexed
/// source, so the report names `legacy` rather than `legacy/orders_legacy/sub`.
fn lift(skips: &mut [SkipEvent], sources: &[SourceFile]) {
    let mut holds_sources: FxHashSet<&str> = FxHashSet::default();
    for source in sources {
        let mut folder = source.path.parent();
        while let Some(path) = folder {
            if !holds_sources.insert(path) {
                break;
            }
            folder = parent_of(path);
        }
    }
    for event in skips.iter_mut().filter(|event| !event.whole_folder) {
        let Some(mut folder) = event.folder.as_repo_path().map(RepoPath::as_str) else {
            continue;
        };
        if holds_sources.contains(folder) {
            continue;
        }
        while let Some(parent) = parent_of(folder).filter(|parent| !holds_sources.contains(parent)) {
            folder = parent;
        }
        event.folder = folder_of(Some(folder));
    }
}

/// `unfold.toml`, whether or not an ignore file lists it.
fn settings_input(root: &Path) -> Option<InputFile> {
    let metadata = std::fs::metadata(root.join(SETTINGS_PATH))
        .ok()
        .filter(std::fs::Metadata::is_file)?;
    Some(InputFile {
        path: RepoPath::new(SETTINGS_PATH).ok()?,
        role: InputRole::UnfoldConfig,
        scope: ScopeStatus::InScope,
        stamp: FileStamp::of(&metadata),
    })
}

/// `full` relative to `root`, `/`-separated, if every component is UTF-8.
fn relative(root: &Path, full: &Path) -> Option<String> {
    let mut text = String::new();
    for component in full.strip_prefix(root).ok()?.components() {
        let Component::Normal(part) = component else {
            return None;
        };
        if !text.is_empty() {
            text.push('/');
        }
        text.push_str(part.to_str()?);
    }
    Some(text)
}

fn parent_of(path: &str) -> Option<&str> {
    path.rsplit_once('/').map(|(parent, _)| parent)
}

fn folder_of(folder: Option<&str>) -> FolderPath {
    folder
        .and_then(|folder| FolderPath::parse(folder).ok())
        .unwrap_or_default()
}

fn problem(root: &Path, error: &ignore::Error) -> Problem {
    let path = error_path(error)
        .and_then(|path| relative(root, path))
        .unwrap_or_default();
    Problem {
        path: path.into(),
        message: error.to_string().into(),
    }
}

fn error_path(error: &ignore::Error) -> Option<&Path> {
    match error {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::WithLineNumber { err, .. } | ignore::Error::WithDepth { err, .. } => error_path(err),
        ignore::Error::Partial(errors) => errors.iter().find_map(error_path),
        _ => None,
    }
}

/// Ignore rules for checking single paths, such as files a watcher reports, with the same rules
/// as [`super::discover`]: the always-excluded folders plus every ignore file that applies.
pub struct IgnoreRules {
    matcher: Option<IncrementalIgnore>,
}

impl IgnoreRules {
    /// Rules for the folder at `root`.
    pub fn new(root: &Path, options: &DiscoverOptions) -> Self {
        Self {
            matcher: builder(root, options).build_matchers().into_iter().next(),
        }
    }

    /// Whether `path` (a folder when `is_dir`) is skipped by an always-excluded folder on its
    /// way or by an ignore rule. Ignore files are read once and cached per folder; build new
    /// rules after one changes.
    pub fn is_ignored(&mut self, path: &RepoPath, is_dir: bool) -> bool {
        if is_default_excluded(path.as_str()) {
            return true;
        }
        let Some(matcher) = self.matcher.as_mut() else {
            return false;
        };
        // A path is hidden when it or any folder above it matches.
        let mut prefix = String::new();
        let components: Vec<&str> = path.as_str().split('/').collect();
        for (index, part) in components.iter().enumerate() {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            let last = index + 1 == components.len();
            if matcher.matched(OsStr::new(&prefix), !last || is_dir).is_ignore() {
                return true;
            }
        }
        false
    }
}
