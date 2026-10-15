//! The parallel walk behind [`super::discover`], and single-path ignore checks for watchers.

use std::ffi::OsStr;
use std::path::{Component, Path};
use std::sync::mpsc;

use ignore::{DirEntry, IncrementalIgnore, WalkBuilder, WalkState};
use unfold_core::RepoPath;
use unfold_core::vocab::InputRole;

use super::classify::{FileClass, classify, is_default_excluded, is_default_excluded_name};
use super::{
    ContextFile, DiscoverOptions, Discovery, FileStamp, InputFile, Problem, SourceFile, UnindexedFile,
};
use crate::settings::{SETTINGS_PATH, Settings};

/// The most names that can't be project paths one problem lists.
const MAX_SKIPPED_LISTED: usize = 10;

/// What one walker thread reports.
enum Found {
    File(RepoPath, FileClass, FileStamp),
    /// A file or folder whose name can't be a project path, folder-relative and lossily decoded.
    Skipped(String),
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
                    problems(root, &error, &mut send);
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
        ignore_problems(root, entry, send);
        return WalkState::Continue;
    }
    let Some(path) = relative(root, entry.path()).and_then(|relative| RepoPath::new(&relative).ok()) else {
        let skipped = entry.path().strip_prefix(root).unwrap_or(entry.path());
        send(Found::Skipped(skipped.to_string_lossy().into_owned()));
        return WalkState::Skip;
    };
    if entry.path_is_symlink() {
        symlink(root, settings, entry.path(), path, send);
        return WalkState::Continue;
    }
    if entry.file_type().is_some_and(|kind| kind.is_dir()) {
        if is_default_excluded_name(&entry.file_name().to_string_lossy()) {
            return WalkState::Skip;
        }
        ignore_problems(root, entry, send);
        return WalkState::Continue;
    }
    let class = classify(&path, settings);
    if class != FileClass::Other {
        match entry.metadata() {
            Ok(metadata) => send(Found::File(path, class, FileStamp::of(&metadata))),
            Err(error) => problems(root, &error, send),
        }
    }
    WalkState::Continue
}

/// A symlink entry: a folder is never followed; a file counts only when its target is a file
/// inside the folder.
fn symlink(root: &Path, settings: &Settings, full: &Path, path: RepoPath, send: &mut impl FnMut(Found)) {
    let class = classify(&path, settings);
    if class == FileClass::Other {
        return;
    }
    if let Ok(metadata) = std::fs::metadata(full)
        && metadata.is_file()
        && std::fs::canonicalize(full).is_ok_and(|target| target.starts_with(root))
    {
        send(Found::File(path, class, FileStamp::of(&metadata)));
    }
}

fn collect(root: &Path, found: impl Iterator<Item = Found>) -> Discovery {
    let mut discovery = Discovery::default();
    let mut skipped = Vec::new();
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
                FileClass::Input(role) => discovery.inputs.push(InputFile { path, role, stamp }),
                FileClass::Other => {}
            },
            Found::Skipped(path) => skipped.push(path),
            Found::Problem(problem) => discovery.problems.push(problem),
        }
    }
    if !skipped.is_empty() {
        discovery.problems.push(skipped_problem(skipped));
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
    discovery
}

/// `unfold.toml`, whether or not an ignore file lists it.
fn settings_input(root: &Path) -> Option<InputFile> {
    let metadata = std::fs::metadata(root.join(SETTINGS_PATH))
        .ok()
        .filter(std::fs::Metadata::is_file)?;
    Some(InputFile {
        path: RepoPath::new(SETTINGS_PATH).ok()?,
        role: InputRole::UnfoldConfig,
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

/// The problems of the ignore files a folder's entry read: rules that don't parse, which the
/// walk leaves out.
fn ignore_problems(root: &Path, entry: &DirEntry, send: &mut impl FnMut(Found)) {
    if let Some(error) = entry.error() {
        problems(root, error, send);
    }
}

/// One problem per error `error` holds, each with its folder-relative path; a path outside the
/// folder (an ignore file above it) stays in the message.
fn problems(root: &Path, error: &ignore::Error, send: &mut impl FnMut(Found)) {
    match error {
        ignore::Error::Partial(errors) => {
            for error in errors {
                problems(root, error, send);
            }
        }
        ignore::Error::WithDepth { err, .. } => problems(root, err, send),
        ignore::Error::WithPath { path, err } if let Some(path) = relative(root, path) => {
            send(Found::Problem(Problem {
                path: path.into(),
                message: err.to_string().into(),
            }));
        }
        _ => send(Found::Problem(Problem {
            path: "".into(),
            message: error.to_string().into(),
        })),
    }
}

/// The one problem that lists the files and folders whose names can't be project paths, the
/// first [`MAX_SKIPPED_LISTED`] in order.
fn skipped_problem(mut skipped: Vec<String>) -> Problem {
    skipped.sort();
    let listed: Vec<String> = skipped
        .iter()
        .take(MAX_SKIPPED_LISTED)
        .map(|path| format!("`{path}`"))
        .collect();
    let mut message = format!(
        "names that can't be project paths, left out: {}",
        listed.join(", ")
    );
    if skipped.len() > MAX_SKIPPED_LISTED {
        message.push_str(&format!(", and {} more", skipped.len() - MAX_SKIPPED_LISTED));
    }
    Problem {
        path: "".into(),
        message: message.into(),
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
