//! The folder's files as search and text search see them: every regular file the walk reaches
//! under discovery's ignore rules (`.gitignore`, `.ignore`, `.git/info/exclude`, the global
//! excludes when the workspace uses them, and the always-excluded folders, `.unfold/` among
//! them). Symlinks are never followed and never listed. The list is sorted by path, so results
//! built from it are deterministic, and it is cached per folder until files change.

use std::path::Path;
use std::sync::Mutex;

use ignore::{WalkBuilder, WalkState};
use unfold_engine::discovery::{DiscoverOptions, is_default_excluded_name};

use crate::ipc::error::{CommandError, CommandResult};
use crate::shared::lock;

/// The most files one folder's list holds; past it, search and text search see the first ones
/// in walk order and say the list was cut.
pub const MAX_FILES: usize = 200_000;

/// The folder's files. See the [module documentation](self).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileSet {
    /// Folder-relative paths, sorted.
    pub paths: Vec<Box<str>>,
    /// More files exist than [`MAX_FILES`].
    pub truncated: bool,
}

impl FileSet {
    /// Walks the folder at canonical `root`.
    pub fn walk(root: &Path, options: &DiscoverOptions) -> CommandResult<Self> {
        if !root.is_dir() {
            return Err(CommandError::not_found(format!(
                "{} is not a folder",
                root.display()
            )));
        }
        let found: Mutex<Vec<Box<str>>> = Mutex::new(Vec::new());
        let mut builder = WalkBuilder::new(root);
        builder
            .hidden(false)
            .parents(true)
            .ignore(true)
            .git_ignore(true)
            .git_exclude(true)
            .git_global(options.global_ignore)
            .require_git(false)
            .follow_links(false)
            .threads(options.threads)
            .filter_entry(|entry| {
                let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
                !(is_dir
                    && entry.depth() > 0
                    && entry.file_name().to_str().is_none_or(is_default_excluded_name))
            });
        builder.build_parallel().run(|| {
            let found = &found;
            Box::new(move |entry| {
                let Ok(entry) = entry else {
                    return WalkState::Continue;
                };
                if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                    return WalkState::Continue;
                }
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .ok()
                    .and_then(Path::to_str)
                    .map(|text| text.replace('\\', "/"));
                if let Some(relative) = relative.filter(|text| !text.is_empty()) {
                    let mut found = lock(found);
                    if found.len() > MAX_FILES {
                        return WalkState::Quit;
                    }
                    found.push(relative.into());
                }
                WalkState::Continue
            })
        });
        let mut paths = found
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        paths.sort_unstable();
        let truncated = paths.len() > MAX_FILES;
        paths.truncate(MAX_FILES);
        Ok(Self { paths, truncated })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_with_ignore_rules_and_skips_unfold() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("canonical");
        for (path, text) in [
            ("src/a.ts", "a"),
            ("src/b.md", "b"),
            ("dist/out.js", "x"),
            ("node_modules/x/index.js", "x"),
            (".unfold/boards/a.json", "{}"),
            (".gitignore", "dist/\n"),
        ] {
            let full = root.join(path);
            std::fs::create_dir_all(full.parent().expect("parent")).expect("mkdir");
            std::fs::write(full, text).expect("write");
        }
        let options = DiscoverOptions {
            threads: 1,
            global_ignore: false,
        };
        let files = FileSet::walk(&root, &options).expect("walk");
        let paths: Vec<&str> = files.paths.iter().map(AsRef::as_ref).collect();
        assert_eq!(paths, [".gitignore", "src/a.ts", "src/b.md"]);
        assert!(!files.truncated);
    }
}
