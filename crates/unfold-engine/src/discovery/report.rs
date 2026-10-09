//! The skipped-files report: what discovery left out and why, aggregated by folder for the
//! first-Compile screen ("Skipped: node_modules, .venv, dist (from .gitignore and defaults)").

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

use crate::settings::FolderPath;

/// Why something was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// An always-excluded folder such as `node_modules` or `.venv`.
    Default,
    /// A `.gitignore`, `.ignore`, `.git/info/exclude`, or global excludes rule.
    Ignored,
    /// An `exclude` pattern in `[scope]`.
    Excluded,
    /// Outside the `include` patterns in `[scope]`.
    NotIncluded,
    /// The file's language is left out of `languages`.
    LanguageOff,
    /// JavaScript, which V1 doesn't index.
    #[serde(rename = "javascript")]
    JavaScript,
    /// A symlinked folder (never followed), or a symlinked file that dangles or points outside
    /// the folder.
    Symlink,
    /// A name that isn't valid UTF-8 or can't be a folder-relative path (such as one containing
    /// a backslash).
    InvalidPath,
}

impl SkipReason {
    /// A short description for the UI.
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Default => "always excluded",
            Self::Ignored => "ignored by .gitignore or .ignore",
            Self::Excluded => "excluded in settings",
            Self::NotIncluded => "outside the included folders",
            Self::LanguageOff => "language turned off in settings",
            Self::JavaScript => "JavaScript isn't graphed",
            Self::Symlink => "symlink not followed",
            Self::InvalidPath => "unsupported file name",
        }
    }
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.describe())
    }
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Skipped {
    /// The folder the line is about.
    pub folder: FolderPath,
    /// Why.
    pub reason: SkipReason,
    /// The folder itself was not walked (an always-excluded, ignored, or symlinked folder), so
    /// its contents weren't counted.
    pub whole_folder: bool,
    /// Skipped source-like files counted in this folder and its subfolders.
    pub files: u32,
}

/// One skipped item as the walk saw it.
pub(super) struct SkipEvent {
    /// The pruned folder, or the skipped file's parent folder.
    pub folder: FolderPath,
    pub reason: SkipReason,
    /// `true` for a pruned folder, `false` for one skipped file.
    pub whole_folder: bool,
}

/// Merges events by folder and reason, then folds each line of skipped files into the nearest
/// ancestor line with the same reason (never into the folder root, which would hide where
/// things are). Lines for folders that weren't walked are always kept.
pub(super) fn aggregate(events: impl IntoIterator<Item = SkipEvent>) -> Vec<Skipped> {
    let mut merged: BTreeMap<(FolderPath, SkipReason), (bool, u32)> = BTreeMap::new();
    for event in events {
        let entry = merged.entry((event.folder, event.reason)).or_default();
        entry.0 |= event.whole_folder;
        if !event.whole_folder {
            entry.1 += 1;
        }
    }
    let mut by_depth: Vec<_> = merged.into_iter().collect();
    by_depth.sort_by(|((a, ra), _), ((b, rb), _)| {
        a.depth().cmp(&b.depth()).then_with(|| a.cmp(b)).then(ra.cmp(rb))
    });
    let mut kept: BTreeMap<(FolderPath, SkipReason), (bool, u32)> = BTreeMap::new();
    for ((folder, reason), (whole, files)) in by_depth {
        // A folder that wasn't walked keeps its own line: its name is the point of the report.
        let ancestor = if whole {
            None
        } else {
            ancestors(&folder).find(|ancestor| kept.contains_key(&(ancestor.clone(), reason)))
        };
        match ancestor {
            Some(ancestor) => {
                if let Some(line) = kept.get_mut(&(ancestor, reason)) {
                    line.1 += files;
                }
            }
            None => {
                kept.insert((folder, reason), (whole, files));
            }
        }
    }
    kept.into_iter()
        .map(|((folder, reason), (whole_folder, files))| Skipped {
            folder,
            reason,
            whole_folder,
            files,
        })
        .collect()
}

/// The proper ancestors of `folder`, nearest first, excluding the folder root.
fn ancestors(folder: &FolderPath) -> impl Iterator<Item = FolderPath> + '_ {
    let text = folder.as_repo_path().map_or("", |path| path.as_str());
    text.rmatch_indices('/')
        .filter_map(|(at, _)| FolderPath::parse(&text[..at]).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(folder: &str, reason: SkipReason, whole_folder: bool) -> SkipEvent {
        SkipEvent {
            folder: FolderPath::parse(folder).unwrap(),
            reason,
            whole_folder,
        }
    }

    #[test]
    fn aggregates_by_folder_and_folds_into_ancestors() {
        let lines = aggregate([
            event("legacy", SkipReason::Excluded, false),
            event("legacy/deep", SkipReason::Excluded, false),
            event("legacy/deep", SkipReason::Excluded, false),
            event("node_modules", SkipReason::Default, true),
            event("web/node_modules", SkipReason::Default, true),
            event(".", SkipReason::JavaScript, false),
            event("scripts", SkipReason::JavaScript, false),
            event("dist", SkipReason::Ignored, true),
            event("legacy/deep", SkipReason::Ignored, true),
        ]);
        let summary: Vec<(String, SkipReason, bool, u32)> = lines
            .iter()
            .map(|line| {
                (
                    line.folder.to_string(),
                    line.reason,
                    line.whole_folder,
                    line.files,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (".".into(), SkipReason::JavaScript, false, 1),
                ("dist".into(), SkipReason::Ignored, true, 0),
                ("legacy".into(), SkipReason::Excluded, false, 3),
                ("legacy/deep".into(), SkipReason::Ignored, true, 0),
                ("node_modules".into(), SkipReason::Default, true, 0),
                ("scripts".into(), SkipReason::JavaScript, false, 1),
                ("web/node_modules".into(), SkipReason::Default, true, 0),
            ]
        );
    }
}
