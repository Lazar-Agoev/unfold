//! The discover stage: the settings, then the files in scope, either from a full discovery or
//! by applying the changed paths a watcher reported to the previous result.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::vocab::{InputRole, Language as LanguageId};
use unfold_core::{RepoPath, Snapshot};

use super::state::{SettingsState, State};
use super::{Changes, Engine, Problem, ProblemKind, Verify};
use crate::discovery::{FileClass, FileStamp, IgnoreRules, classify, discover, is_default_excluded};
use crate::settings::{SETTINGS_PATH, Settings, SettingsFile};

/// One indexed source after the scan.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Source {
    pub(crate) language: LanguageId,
    pub(crate) stamp: Option<FileStamp>,
}

/// One discovered configuration input (other than the settings file).
#[derive(Debug, Clone, Copy)]
pub(crate) struct FoundInput {
    pub(crate) role: InputRole,
    pub(crate) stamp: Option<FileStamp>,
}

/// Which files may have changed.
#[derive(Debug, Clone)]
pub(crate) enum Check {
    /// Every file.
    All,
    /// Only these paths.
    Paths(BTreeSet<RepoPath>),
}

impl Check {
    pub(crate) fn contains(&self, path: &RepoPath) -> bool {
        match self {
            Self::All => true,
            Self::Paths(paths) => paths.contains(path),
        }
    }
}

/// The files in scope after the discover stage.
#[derive(Debug)]
pub(crate) struct Scan {
    pub(crate) full: bool,
    pub(crate) sources: BTreeMap<RepoPath, Source>,
    pub(crate) inputs: BTreeMap<RepoPath, FoundInput>,
    pub(crate) unindexed: BTreeSet<RepoPath>,
    pub(crate) context: BTreeSet<RepoPath>,
    pub(crate) check: Check,
    /// What discovery couldn't read; `None` keeps the previous full discovery's problems.
    pub(crate) problems: Option<Vec<Problem>>,
}

/// Loads `unfold.toml` when it may have changed. An unreadable or invalid file keeps
/// `previous`'s settings and apps in effect and records the problem.
pub(crate) fn load_settings(
    root: &Path,
    previous: &SettingsState,
    changes: &Changes,
    verify: Verify,
) -> SettingsState {
    let config = RepoPath::new(SETTINGS_PATH).expect("the settings path is a valid folder-relative path");
    let may_have_changed = match changes {
        Changes::All => true,
        Changes::Paths(paths) => paths.contains(&config),
    };
    if !may_have_changed {
        return previous.clone();
    }
    let keep = |stamp: Option<FileStamp>, message: String| SettingsState {
        stamp,
        problem: Some(Problem {
            kind: ProblemKind::Settings,
            path: SETTINGS_PATH.into(),
            message: message.into(),
        }),
        ..previous.clone()
    };
    let full = root.join(SETTINGS_PATH);
    let metadata = match std::fs::metadata(&full) {
        Ok(metadata) if metadata.is_file() => metadata,
        Ok(_) => return SettingsState::default(),
        Err(error) if error.kind() == ErrorKind::NotFound => return SettingsState::default(),
        Err(error) => return keep(None, format!("cannot read the settings: {error}")),
    };
    let stamp = Some(stamp_of(&metadata));
    if verify == Verify::Stamps && previous.stamp.is_some() && previous.stamp == stamp {
        return previous.clone();
    }
    let bytes = match std::fs::read(&full) {
        Ok(bytes) => bytes,
        Err(error) => return keep(stamp, format!("cannot read the settings: {error}")),
    };
    let snapshot = match Snapshot::new(config, bytes) {
        Ok(snapshot) => snapshot,
        Err(error) => return keep(stamp, error.to_string()),
    };
    match SettingsFile::parse(snapshot.text()) {
        Ok(SettingsFile { settings, apps }) => SettingsState {
            settings,
            apps,
            source: Some(snapshot),
            stamp,
            problem: None,
        },
        Err(error) => keep(stamp, error.to_string()),
    }
}

pub(crate) fn stamp_of(metadata: &std::fs::Metadata) -> FileStamp {
    FileStamp {
        size: metadata.len(),
        modified: metadata.modified().ok(),
    }
}

/// Scans the folder: a full discovery when `changes` is everything, when nothing was
/// discovered yet, when the settings changed, or when a changed path can't be applied on its
/// own; otherwise the changed paths applied to `previous`.
pub(crate) fn scan(
    engine: &Engine,
    previous: &State,
    settings: &Settings,
    settings_changed: bool,
    changes: &Changes,
) -> Result<Scan, super::UpdateError> {
    if let Changes::Paths(paths) = changes
        && previous.discovered
        && !settings_changed
        && let Some(scan) = apply_paths(engine, previous, settings, paths)
    {
        return Ok(scan);
    }
    full_scan(engine, settings)
}

fn full_scan(engine: &Engine, settings: &Settings) -> Result<Scan, super::UpdateError> {
    let discovery = discover(&engine.root, settings, &engine.options.discover)?;
    let mut sources = BTreeMap::new();
    let mut unindexed: BTreeSet<RepoPath> = discovery.unindexed.into_iter().map(|file| file.path).collect();
    for file in discovery.sources {
        if engine.owner(&file.path, file.language, settings) {
            let source = Source {
                language: file.language,
                stamp: Some(file.stamp),
            };
            sources.insert(file.path, source);
        } else {
            unindexed.insert(file.path);
        }
    }
    let context = discovery.context.into_iter().map(|file| file.path).collect();
    let inputs = discovery
        .inputs
        .into_iter()
        .filter(|input| input.role != InputRole::UnfoldConfig)
        .map(|input| {
            let found = FoundInput {
                role: input.role,
                stamp: Some(input.stamp),
            };
            (input.path, found)
        })
        .collect();
    let problems = discovery
        .problems
        .into_iter()
        .map(|problem| Problem {
            kind: ProblemKind::Discovery,
            path: problem.path,
            message: problem.message,
        })
        .collect();
    Ok(Scan {
        full: true,
        sources,
        inputs,
        unindexed,
        context,
        check: Check::All,
        problems: Some(problems),
    })
}

/// Whether a change to `path` can change which files are in scope beyond `path` itself.
fn changes_rules(path: &RepoPath) -> bool {
    matches!(path.file_name(), ".gitignore" | ".ignore") || path.as_str() == ".git/info/exclude"
}

/// Applies changed paths to the previous scan, or `None` when one of them needs a full
/// discovery: an ignore file, a folder, a symlink, or a path that can't be examined.
fn apply_paths(engine: &Engine, previous: &State, settings: &Settings, paths: &[RepoPath]) -> Option<Scan> {
    let mut sources: BTreeMap<RepoPath, Source> = previous
        .files
        .iter()
        .map(|(path, entry)| {
            let source = Source {
                language: entry.language,
                stamp: entry.disk.stamp,
            };
            (path.clone(), source)
        })
        .collect();
    let mut inputs: BTreeMap<RepoPath, FoundInput> = previous
        .inputs
        .iter()
        .filter(|(_, input)| !input.requested)
        .map(|(path, input)| {
            let found = FoundInput {
                role: input.role,
                stamp: input.disk.stamp,
            };
            (path.clone(), found)
        })
        .collect();
    let mut unindexed = previous.unindexed.clone();
    let mut context = previous.context.clone();
    let mut rules: Option<IgnoreRules> = None;
    let mut listings = Listings::default();
    let mut check = BTreeSet::new();
    for path in paths {
        // The settings file was loaded already; nothing else in an always-excluded folder
        // (`.unfold/boards/`, `node_modules/`, ...) is ever in scope.
        if path.as_str() == SETTINGS_PATH || is_default_excluded(path.as_str()) {
            continue;
        }
        if changes_rules(path) {
            return None;
        }
        let metadata = match std::fs::symlink_metadata(engine.root.join(path.as_str())) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                let gone = |candidate: &RepoPath| {
                    candidate == path
                        || candidate
                            .as_str()
                            .strip_prefix(path.as_str())
                            .is_some_and(|rest| rest.starts_with('/'))
                };
                sources.retain(|candidate, _| !gone(candidate));
                inputs.retain(|candidate, _| !gone(candidate));
                unindexed.retain(|candidate| !gone(candidate));
                context.retain(|candidate| !gone(candidate));
                continue;
            }
            Err(_) => return None,
        };
        if metadata.is_dir() || metadata.file_type().is_symlink() || under_symlink(&engine.root, path) {
            return None;
        }
        // On a case-insensitive file system the old name of a case-only rename (`logger.ts`
        // after `Logger.ts` replaced it) still finds the file. Discovery sees the names as
        // spelled on disk.
        if !listings.spelled_exactly(&engine.root.join(path.as_str()))? {
            return None;
        }
        sources.remove(path);
        inputs.remove(path);
        unindexed.remove(path);
        context.remove(path);
        let rules = rules.get_or_insert_with(|| IgnoreRules::new(&engine.root, &engine.options.discover));
        if rules.is_ignored(path, false) {
            continue;
        }
        let stamp = Some(stamp_of(&metadata));
        match classify(path, settings) {
            FileClass::Source(language) if engine.owner(path, language, settings) => {
                sources.insert(path.clone(), Source { language, stamp });
            }
            FileClass::Source(_) | FileClass::Unindexed(_) => {
                unindexed.insert(path.clone());
            }
            FileClass::Input(role) if role != InputRole::UnfoldConfig => {
                inputs.insert(path.clone(), FoundInput { role, stamp });
            }
            FileClass::Context(..) => {
                context.insert(path.clone());
            }
            FileClass::Input(..) | FileClass::Other => {}
        }
        check.insert(path.clone());
    }
    Some(Scan {
        full: false,
        sources,
        inputs,
        unindexed,
        context,
        check: Check::Paths(check),
        problems: None,
    })
}

/// The folder listings read while applying changed paths, by folder.
#[derive(Default)]
struct Listings(FxHashMap<PathBuf, FxHashSet<OsString>>);

impl Listings {
    /// Whether the folder of `full` (an existing path) holds an entry spelled exactly like its
    /// file name, byte for byte; `None` when the folder can't be listed.
    fn spelled_exactly(&mut self, full: &Path) -> Option<bool> {
        let (folder, name) = (full.parent()?, full.file_name()?);
        if !self.0.contains_key(folder) {
            let names = std::fs::read_dir(folder)
                .ok()?
                .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
                .collect();
            self.0.insert(folder.to_owned(), names);
        }
        Some(self.0.get(folder).is_some_and(|names| names.contains(name)))
    }
}

/// Whether a folder on the way to `path` is a symbolic link (or can't be inspected). Discovery
/// never follows linked folders, so a changed path under one needs a full discovery to decide
/// that it is out of scope.
fn under_symlink(root: &Path, path: &RepoPath) -> bool {
    let text = path.as_str();
    text.match_indices('/').any(|(end, _)| {
        std::fs::symlink_metadata(root.join(&text[..end]))
            .map_or(true, |metadata| metadata.file_type().is_symlink())
    })
}
