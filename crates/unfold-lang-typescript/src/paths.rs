//! Folder-relative path arithmetic on `/`-separated strings, where `""` is the folder itself.
//!
//! Configuration values (`extends`, `baseUrl`, `paths`, `rootDir`, `outDir`, `include`) and
//! module specifiers are joined onto folders with these helpers. A path that would leave the
//! open folder, or that is absolute, has no folder-relative form: Unfold only ever reads inside
//! the folder.

/// The folder containing `path` (`""` for a file at the root).
pub(crate) fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// `dir` followed by each of its ancestors, ending with the folder itself (`""`).
pub(crate) fn ancestors(dir: &str) -> impl Iterator<Item = &str> {
    let mut next = Some(dir);
    std::iter::from_fn(move || {
        let current = next?;
        next = if current.is_empty() {
            None
        } else {
            Some(dir_of(current))
        };
        Some(current)
    })
}

/// Whether `path` is written as an absolute path (`/x`, `\x`, or a drive such as `C:`).
pub(crate) fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with('\\')
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

/// `relative` joined onto the folder-relative `dir` and normalized (`.` and empty components
/// dropped, `..` applied). `None` when the result would leave the folder or `relative` is
/// absolute. Backslashes count as separators, as TypeScript reads them in configuration.
pub(crate) fn join(dir: &str, relative: &str) -> Option<String> {
    if is_absolute(relative) {
        return None;
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    for part in relative.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// `base` and `name` joined with one `/` (either may be empty).
pub(crate) fn child(base: &str, name: &str) -> String {
    match (base.is_empty(), name.is_empty()) {
        (true, _) => name.to_owned(),
        (_, true) => base.to_owned(),
        _ => format!("{base}/{name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_and_normalizes_within_the_folder() {
        assert_eq!(join("src", "./a/../b.ts").as_deref(), Some("src/b.ts"));
        assert_eq!(join("src/x", "../../a").as_deref(), Some("a"));
        assert_eq!(join("", ".").as_deref(), Some(""));
        assert_eq!(join("src", "..\\lib\\a.ts").as_deref(), Some("lib/a.ts"));
        assert_eq!(join("src", "../../a"), None);
        assert_eq!(join("src", "/abs"), None);
        assert_eq!(join("src", "C:/x"), None);
    }

    #[test]
    fn walks_ancestors_and_joins_names() {
        assert_eq!(ancestors("a/b").collect::<Vec<_>>(), ["a/b", "a", ""]);
        assert_eq!(ancestors("").collect::<Vec<_>>(), [""]);
        assert_eq!(dir_of("a/b.ts"), "a");
        assert_eq!(dir_of("b.ts"), "");
        assert_eq!(child("", "a"), "a");
        assert_eq!(child("a", "b"), "a/b");
    }
}
