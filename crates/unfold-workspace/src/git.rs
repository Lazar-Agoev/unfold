//! Branches and worktrees from Git's own files (worktree metadata). Unfold never runs
//! Git: it reads `.git`, `HEAD`, `commondir`, and `worktrees/*/gitdir` as files, each capped at
//! 4 KiB, and a file it can't read means "no repository" for that step, never a failed command.
//!
//! ```text
//! find:      walk up from the project folder P; the first R with R/.git wins; subpath = P relative to R
//! gitdir:    R/.git is a directory -> R/.git; a file "gitdir: <p>" -> <p> (relative to R unless absolute)
//! commondir: gitdir/commondir's first line, relative to gitdir; else gitdir itself
//! HEAD:      "ref: refs/heads/<name>" -> branch <name>; "ref: <other>" -> branch <other>;
//!            40 or 64 hex digits -> detached (the first 7 shown)
//! worktrees: main = parent(commondir) when commondir is named .git (HEAD: commondir/HEAD);
//!            linked = each commondir/worktrees/W whose W/gitdir names "<root>/.git" (HEAD: W/HEAD)
//!            each one's candidate project folder is root/subpath; available when it is a folder
//! order:     main first, then by path
//! identity:  (canonical commondir, subpath): one project row per repository
//! ```
//!
//! HEAD is read on every call (worktree metadata): nothing watches `.git`.

use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unfold_engine::settings::default_project_name;

/// The most bytes read from any one of Git's files.
const MAX_READ: u64 = 4 * 1024;
/// The most worktrees listed.
pub const MAX_WORKTREES: usize = 64;

/// Which repository a project belongs to, and where in it (`workspace.json` `repository`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryId {
    /// The repository's common Git folder, canonical.
    pub common_dir: String,
    /// The project folder relative to its worktree's root, `/`-separated (`""` for the root).
    pub subpath: String,
}

/// A repository found from a project folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Repository {
    /// The worktree's own Git folder (`R/.git`, or the folder a `.git` file names).
    gitdir: PathBuf,
    /// The common Git folder all worktrees share.
    common_dir: PathBuf,
    /// The project folder relative to the worktree root, `/`-separated.
    subpath: String,
}

impl Repository {
    /// The repository's identity.
    pub(crate) fn id(&self) -> RepositoryId {
        RepositoryId {
            common_dir: self.common_dir.to_string_lossy().into_owned(),
            subpath: self.subpath.clone(),
        }
    }
}

/// What `HEAD` says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Head {
    /// The branch checked out.
    pub(crate) branch: Option<String>,
    /// The first 7 hex digits of a detached HEAD's commit.
    pub(crate) detached: Option<String>,
}

/// One worktree of a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    /// The worktree's root folder.
    pub root: String,
    /// The project's folder in this worktree (`root` joined with the project's subpath): what
    /// `switch_worktree` takes.
    pub path: String,
    /// The branch checked out there.
    pub branch: Option<String>,
    /// The first 7 hex digits of a detached HEAD's commit.
    pub detached: Option<String>,
    /// Whether it is the repository's main worktree.
    pub main: bool,
    /// Whether the project's row points at it now.
    pub current: bool,
    /// Whether the project's folder exists in it.
    pub available: bool,
}

/// The branch and worktrees of a project's repository (`git_info`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GitInfo {
    /// The branch checked out in the project's worktree.
    pub branch: Option<String>,
    /// The first 7 hex digits of a detached HEAD's commit.
    pub detached: Option<String>,
    /// The repository's worktrees: the main one first, then by path (at most 64).
    pub worktrees: Vec<Worktree>,
}

/// A project row's Git summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GitSummary {
    /// The branch checked out in the project's worktree.
    pub branch: Option<String>,
    /// The first 7 hex digits of a detached HEAD's commit.
    pub detached: Option<String>,
    /// How many worktrees the repository has.
    pub worktrees: u32,
}

impl GitInfo {
    /// The row's summary of this information.
    pub(crate) fn summary(&self) -> GitSummary {
        GitSummary {
            branch: self.branch.clone(),
            detached: self.detached.clone(),
            worktrees: u32::try_from(self.worktrees.len()).unwrap_or(u32::MAX),
        }
    }
}

/// Reads at most [`MAX_READ`] bytes of a UTF-8 file.
fn read_capped(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_READ).read_to_end(&mut bytes).ok()?;
    String::from_utf8(bytes).ok()
}

/// A file's first line, without its terminator.
fn first_line(path: &Path) -> Option<String> {
    let text = read_capped(path)?;
    let line = text.lines().next()?.trim_end_matches('\r');
    (!line.is_empty()).then(|| line.to_owned())
}

/// `path` resolved against `base` when relative, with `.` and `..` folded away.
fn resolve(base: &Path, path: &str) -> PathBuf {
    let joined = base.join(path);
    std::fs::canonicalize(&joined).unwrap_or_else(|_| {
        let mut folded = PathBuf::new();
        for component in joined.components() {
            match component {
                Component::ParentDir => {
                    folded.pop();
                }
                Component::CurDir => {}
                other => folded.push(other),
            }
        }
        folded
    })
}

/// The repository holding the canonical project folder `project`, if any.
pub(crate) fn find(project: &Path) -> Option<Repository> {
    let root = project
        .ancestors()
        .find(|dir| std::fs::symlink_metadata(dir.join(".git")).is_ok())?;
    let dot_git = root.join(".git");
    let gitdir = if dot_git.is_dir() {
        dot_git
    } else {
        let line = first_line(&dot_git)?;
        resolve(root, line.strip_prefix("gitdir:")?.trim())
    };
    let common_dir = match first_line(&gitdir.join("commondir")) {
        Some(line) => resolve(&gitdir, &line),
        None => std::fs::canonicalize(&gitdir).unwrap_or_else(|_| gitdir.clone()),
    };
    let subpath = project
        .strip_prefix(root)
        .ok()?
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    Some(Repository {
        gitdir,
        common_dir,
        subpath,
    })
}

/// What `<dir>/HEAD` says.
fn head(dir: &Path) -> Head {
    let Some(line) = first_line(&dir.join("HEAD")) else {
        return Head::default();
    };
    if let Some(reference) = line.strip_prefix("ref:") {
        let reference = reference.trim();
        let branch = reference.strip_prefix("refs/heads/").unwrap_or(reference);
        return Head {
            branch: (!branch.is_empty()).then(|| branch.to_owned()),
            detached: None,
        };
    }
    let commit = line.trim();
    let hex = matches!(commit.len(), 40 | 64) && commit.bytes().all(|byte| byte.is_ascii_hexdigit());
    Head {
        branch: None,
        detached: hex.then(|| commit[..7].to_owned()),
    }
}

/// The branch and worktrees of the repository holding the canonical project folder `project`,
/// or `None` when it is in no repository.
pub(crate) fn info(project: &Path) -> Option<GitInfo> {
    let repository = find(project)?;
    let own = head(&repository.gitdir);
    let mut found: Vec<(PathBuf, Head, bool)> = Vec::new();
    if repository
        .common_dir
        .file_name()
        .is_some_and(|name| name == ".git")
        && let Some(main) = repository.common_dir.parent()
    {
        found.push((main.to_path_buf(), head(&repository.common_dir), true));
    }
    let mut linked: Vec<(PathBuf, Head, bool)> = std::fs::read_dir(repository.common_dir.join("worktrees"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let dir = entry.path();
            let named = PathBuf::from(first_line(&dir.join("gitdir"))?);
            let root = named.parent()?.to_path_buf();
            Some((root, head(&dir), false))
        })
        .collect();
    linked.sort_by(|a, b| a.0.cmp(&b.0));
    found.extend(linked);
    found.truncate(MAX_WORKTREES);
    let worktrees = found
        .into_iter()
        .map(|(root, head, main)| {
            let path = if repository.subpath.is_empty() {
                root.clone()
            } else {
                root.join(&repository.subpath)
            };
            let available = path.is_dir();
            let current = available && std::fs::canonicalize(&path).is_ok_and(|path| path == project);
            Worktree {
                root: root.to_string_lossy().into_owned(),
                path: path.to_string_lossy().into_owned(),
                branch: head.branch,
                detached: head.detached,
                main,
                current,
                available,
            }
        })
        .collect();
    Some(GitInfo {
        branch: own.branch,
        detached: own.detached,
        worktrees,
    })
}

/// The default shared name of the canonical project folder `project` (shared project names): for a
/// repository whose common folder is `.git`, the main worktree's folder name when the project is
/// a worktree's root (so every worktree of one repository shares it); otherwise the project
/// folder's name. A name that isn't a valid project name becomes its slug.
pub(crate) fn default_name(project: &Path) -> String {
    let main = find(project)
        .filter(|repository| {
            repository.subpath.is_empty()
                && repository
                    .common_dir
                    .file_name()
                    .is_some_and(|name| name == ".git")
        })
        .and_then(|repository| repository.common_dir.parent().map(Path::to_path_buf));
    let folder = main.as_deref().unwrap_or(project);
    let basename = folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    default_project_name(&basename)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Writes a file, creating its folder.
    pub(crate) fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }

    /// A fake repository at `<base>/repo` on `main`, with a linked worktree at `<base>/wt` on
    /// `feature` and a detached one at `<base>/old`. Every worktree has `app/`.
    pub(crate) fn fake_repository(base: &Path) {
        let repo = base.join("repo");
        write(&repo.join(".git/HEAD"), "ref: refs/heads/main\n");
        write(&repo.join("app/a.ts"), "export const a = 1;\n");
        for (name, head) in [
            ("wt", "ref: refs/heads/feature\n"),
            ("old", "0123456789abcdef0123456789abcdef01234567\n"),
        ] {
            let meta = repo.join(".git/worktrees").join(name);
            write(&meta.join("HEAD"), head);
            write(&meta.join("commondir"), "../..\n");
            write(
                &meta.join("gitdir"),
                &format!("{}\n", base.join(name).join(".git").display()),
            );
            write(
                &base.join(name).join(".git"),
                &format!("gitdir: {}\n", meta.display()),
            );
            write(&base.join(name).join("app/a.ts"), "export const a = 1;\n");
        }
    }

    fn canonical(path: PathBuf) -> PathBuf {
        std::fs::canonicalize(path).expect("canonical")
    }

    #[test]
    fn reads_branches_and_worktrees_from_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let base = canonical(dir.path().to_path_buf());
        fake_repository(&base);
        let project = base.join("repo/app");
        let found = info_of(&project);
        assert_eq!(found.branch.as_deref(), Some("main"));
        assert_eq!(found.detached, None);
        let paths: Vec<&str> = found.worktrees.iter().map(|tree| tree.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                base.join("repo/app").to_str().expect("utf-8"),
                base.join("old/app").to_str().expect("utf-8"),
                base.join("wt/app").to_str().expect("utf-8"),
            ]
        );
        assert!(found.worktrees[0].main && found.worktrees[0].current);
        assert_eq!(found.worktrees[1].detached.as_deref(), Some("0123456"));
        assert_eq!(found.worktrees[2].branch.as_deref(), Some("feature"));
        assert!(found.worktrees.iter().all(|tree| tree.available));

        let linked = info_of(&base.join("wt/app"));
        assert_eq!(linked.branch.as_deref(), Some("feature"));
        assert!(linked.worktrees[2].current);
        assert_eq!(
            find(&base.join("wt/app")).expect("repo").id(),
            find(&project).expect("repo").id(),
            "worktrees of one repository share its identity"
        );
        assert_ne!(
            find(&base.join("repo")).expect("repo").id(),
            find(&project).expect("repo").id(),
            "a subfolder is its own project"
        );
        assert_eq!(default_name(&base.join("wt")), "repo");
        assert_eq!(default_name(&base.join("wt/app")), "app");
        assert!(info(&base).is_none());
        assert_eq!(default_name(&base.join("repo")), "repo");
    }

    fn info_of(path: &Path) -> GitInfo {
        info(path).expect("a repository")
    }

    #[test]
    fn unreadable_files_mean_no_repository_or_no_head() {
        let dir = tempfile::tempdir().expect("temp dir");
        let base = canonical(dir.path().to_path_buf());
        write(&base.join("broken/.git"), "not a gitdir line\n");
        assert!(info(&base.join("broken")).is_none());
        std::fs::create_dir_all(base.join("bare/.git")).expect("mkdir");
        let bare = info(&base.join("bare")).expect("a repository");
        assert_eq!(bare.branch, None);
        assert_eq!(bare.detached, None);
        assert_eq!(bare.worktrees.len(), 1);
        write(&base.join("long/.git/HEAD"), &"x".repeat(10_000));
        assert_eq!(info(&base.join("long")).expect("repo").branch, None);
        write(&base.join("tag/.git/HEAD"), "ref: refs/tags/v1\n");
        assert_eq!(
            info(&base.join("tag")).expect("repo").branch.as_deref(),
            Some("refs/tags/v1")
        );
        assert_eq!(default_name(&base.join("My Repo")), "my-repo");
    }
}
