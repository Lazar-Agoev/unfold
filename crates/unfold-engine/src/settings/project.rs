//! The project's shared name: `[project] name`, which other projects' boards use to name this
//! one, or a default made from a folder's name.

use unicode_normalization::UnicodeNormalization as _;
use unicode_normalization::char::is_combining_mark;

/// The longest project name, in characters (a valid name is ASCII).
pub const MAX_PROJECT_NAME: usize = 64;

/// Whether `name` is a valid project name: an ASCII letter or digit, then at most 63 ASCII
/// letters, digits, `.`, `_`, or `-` (`/^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/`).
pub fn is_project_name(name: &str) -> bool {
    name.len() <= MAX_PROJECT_NAME
        && name
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
}

/// The default project name for a folder named `basename` (the caller picks the folder): the
/// basename itself when it is a valid project name, else its slug, as a board's file name is
/// made from its title: `My Repo` → `my-repo`, `Café` → `cafe`. Accents are dropped, every
/// other run of characters that aren't ASCII letters or digits becomes one `-`, and the result
/// has at most [`MAX_PROJECT_NAME`] characters; a basename with nothing left becomes `project`.
pub fn default_project_name(basename: &str) -> String {
    if is_project_name(basename) {
        return basename.to_owned();
    }
    let mut slug = String::new();
    let mut pending_dash = false;
    for character in basename.nfkd().filter(|&character| !is_combining_mark(character)) {
        let lower = character.to_ascii_lowercase();
        if lower.is_ascii_lowercase() || lower.is_ascii_digit() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(lower);
        } else {
            pending_dash = true;
        }
    }
    slug.truncate(MAX_PROJECT_NAME);
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "project".to_owned()
    } else {
        slug.to_owned()
    }
}
