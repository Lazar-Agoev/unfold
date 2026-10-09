//! Turning a launch's `(cwd, path)` pair into a folder-relative file path.

use unfold_core::RepoPath;

/// The extensions probed, in order, for a launch path written without one (`node dist/server`
/// may run `dist/server.js` or `dist/server/index.js`).
pub const LAUNCH_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".mts", ".cts", ".js", ".mjs", ".cjs"];

/// The folder-relative file a launch names: `path` joined onto `cwd`, which is folder-relative
/// or absolute, with `.` and `..` applied. `root` is the open folder's absolute path with `/`
/// separators.
///
/// Returns `None` for a launch the backend rejects: one leaving the folder, naming the folder
/// itself, passing through `node_modules`, or containing NUL, a backslash, or a drive prefix.
/// Detectors drop such launches with a note instead of producing an entrance.
pub fn launch_file(root: &str, cwd: &str, path: &str) -> Option<RepoPath> {
    if [cwd, path].iter().any(|value| is_rejected(value)) {
        return None;
    }
    let root_parts = normalize(root.split('/'))?;
    let full = if path.starts_with('/') {
        normalize(path.split('/'))?
    } else if cwd.starts_with('/') {
        normalize(cwd.split('/').chain(path.split('/')))?
    } else {
        normalize(root.split('/').chain(cwd.split('/')).chain(path.split('/')))?
    };
    let relative = full.strip_prefix(root_parts.as_slice())?;
    if relative.is_empty() || relative.contains(&"node_modules") {
        return None;
    }
    RepoPath::new(&relative.join("/")).ok()
}

fn is_rejected(value: &str) -> bool {
    let bytes = value.as_bytes();
    value.contains('\0')
        || value.contains('\\')
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

/// The components of an absolute path after applying `.` and `..`; `None` if `..` climbs above
/// the filesystem root.
fn normalize<'a>(parts: impl Iterator<Item = &'a str>) -> Option<Vec<&'a str>> {
    let mut out = Vec::new();
    for part in parts {
        match part {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            part => out.push(part),
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(cwd: &str, path: &str) -> Option<String> {
        launch_file("/work/repo", cwd, path).map(|path| path.as_str().to_owned())
    }

    #[test]
    fn joins_relative_and_absolute_directories() {
        assert_eq!(file(".", "dist/server.js").as_deref(), Some("dist/server.js"));
        assert_eq!(file("src", "./scheduler.ts").as_deref(), Some("src/scheduler.ts"));
        assert_eq!(
            file("apps/api", "../web/main.ts").as_deref(),
            Some("apps/web/main.ts")
        );
        assert_eq!(file("/work/repo/src", "a.ts").as_deref(), Some("src/a.ts"));
        assert_eq!(file("src", "/work/repo/b.ts").as_deref(), Some("b.ts"));
    }

    #[test]
    fn rejects_paths_outside_the_folder() {
        assert_eq!(file("..", "outside.js"), None);
        assert_eq!(file(".", "../repo2/a.js"), None);
        assert_eq!(file("/elsewhere", "a.js"), None);
        assert_eq!(file(".", "node_modules/x/cli.js"), None);
        assert_eq!(file(".", "."), None);
        assert_eq!(file(".", "a\\b.js"), None);
        assert_eq!(file("C:/x", "a.js"), None);
        assert_eq!(file(".", "a\0.js"), None);
    }
}
