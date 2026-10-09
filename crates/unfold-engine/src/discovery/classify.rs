//! What one folder-relative path is to Unfold, decided from the path alone.

use unfold_core::RepoPath;
use unfold_core::vocab::{InputRole, Language};

use crate::settings::{SETTINGS_PATH, ScopeStatus, Settings};

/// Folder names that are never walked, wherever they appear (U-16).
pub const DEFAULT_EXCLUDED: &[&str] = &[
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".git",
    ".unfold",
];

/// Whether a folder with this name is always excluded.
pub fn is_default_excluded_name(name: &str) -> bool {
    DEFAULT_EXCLUDED.contains(&name)
}

/// Whether any component of `path` is an always-excluded folder.
pub fn is_default_excluded(path: &str) -> bool {
    path.split('/').any(is_default_excluded_name)
}

/// What a file is to Unfold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileClass {
    /// An indexed source file: it gets blocks.
    Source(Language),
    /// A file a language reads for context but never gives blocks: TypeScript declaration
    /// files (`.d.ts`, `.d.mts`, `.d.cts`).
    Context(Language, ScopeStatus),
    /// A file with a source extension that exists but isn't indexed. References into it
    /// resolve as `not_indexed` rather than `not_found`.
    Unindexed(Unindexed),
    /// A configuration input whose bytes are fingerprinted.
    Input(InputRole, ScopeStatus),
    /// Anything else (documentation, images, lockfiles, ...): not tracked.
    Other,
}

/// Why an existing source-like file isn't indexed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unindexed {
    /// An `exclude` pattern matches it.
    Excluded,
    /// `include` patterns are set and none matches it.
    NotIncluded,
    /// Its language is left out of `languages`.
    LanguageOff,
    /// JavaScript (`.js`, `.mjs`, `.cjs`, `.jsx`), which V1 doesn't index.
    #[serde(rename = "javascript")]
    JavaScript,
}

/// Classifies `path`. Ignore files and symlinks are the walk's concern; this looks only at the
/// path, the default exclusions, and the settings.
pub fn classify(path: &RepoPath, settings: &Settings) -> FileClass {
    let text = path.as_str();
    if text == SETTINGS_PATH {
        return FileClass::Input(InputRole::UnfoldConfig, ScopeStatus::InScope);
    }
    if is_default_excluded(text) {
        return FileClass::Other;
    }
    let name = path.file_name();
    if let Some(role) = input_role(name) {
        return FileClass::Input(role, settings.scope.status(path));
    }
    let Some(kind) = source_kind(name) else {
        return FileClass::Other;
    };
    let language = match kind {
        SourceKind::JavaScript => return FileClass::Unindexed(Unindexed::JavaScript),
        SourceKind::Declaration => {
            return if settings.language_enabled(Language::TypeScript) {
                FileClass::Context(Language::TypeScript, settings.scope.status(path))
            } else {
                FileClass::Other
            };
        }
        SourceKind::Indexable(language) => language,
    };
    if !settings.language_enabled(language) {
        return FileClass::Unindexed(Unindexed::LanguageOff);
    }
    match settings.scope.status(path) {
        ScopeStatus::InScope => FileClass::Source(language),
        ScopeStatus::Excluded => FileClass::Unindexed(Unindexed::Excluded),
        ScopeStatus::NotIncluded => FileClass::Unindexed(Unindexed::NotIncluded),
    }
}

/// The configuration input a file name denotes: `package.json`, `pyproject.toml`, or
/// `tsconfig*.json` (such as `tsconfig.build.json`).
fn input_role(name: &str) -> Option<InputRole> {
    match name {
        "package.json" => Some(InputRole::Package),
        "pyproject.toml" => Some(InputRole::Pyproject),
        _ if name.starts_with("tsconfig") && name.ends_with(".json") => Some(InputRole::Tsconfig),
        _ => None,
    }
}

enum SourceKind {
    Indexable(Language),
    Declaration,
    JavaScript,
}

/// The source kind by file name, matched by suffix as in the proof of concept. `.pyi` stubs are
/// not sources: the proof of concept's Python front end reads only `.py` files, and stubs carry
/// no runtime bodies.
fn source_kind(name: &str) -> Option<SourceKind> {
    let ends = |suffixes: &[&str]| suffixes.iter().any(|suffix| name.ends_with(suffix));
    if ends(&[".d.ts", ".d.mts", ".d.cts"]) {
        Some(SourceKind::Declaration)
    } else if ends(&[".ts", ".tsx", ".mts", ".cts"]) {
        Some(SourceKind::Indexable(Language::TypeScript))
    } else if name.ends_with(".py") {
        Some(SourceKind::Indexable(Language::Python))
    } else if ends(&[".js", ".mjs", ".cjs", ".jsx"]) {
        Some(SourceKind::JavaScript)
    } else {
        None
    }
}

/// Whether a file name has any source-like extension (indexable, declaration, or JavaScript).
pub(super) fn is_source_like(name: &str) -> bool {
    source_kind(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(path: &str, settings: &Settings) -> FileClass {
        classify(&RepoPath::new(path).unwrap(), settings)
    }

    #[test]
    fn classifies_by_extension_and_name() {
        let settings = Settings::default();
        let ts = FileClass::Source(Language::TypeScript);
        for path in ["a.ts", "a.tsx", "src/a.mts", "a.cts", "a.d.tsx", ".ts", "d.ts"] {
            assert_eq!(class(path, &settings), ts, "{path}");
        }
        for path in ["a.d.ts", "types/a.d.mts", "a.d.cts", ".d.ts"] {
            assert_eq!(
                class(path, &settings),
                FileClass::Context(Language::TypeScript, ScopeStatus::InScope),
                "{path}"
            );
        }
        assert_eq!(class("a.py", &settings), FileClass::Source(Language::Python));
        for path in [
            "a.pyi",
            "README.md",
            "a.json",
            "Makefile",
            "a.tsbuildinfo",
            "a.ts.map",
        ] {
            assert_eq!(class(path, &settings), FileClass::Other, "{path}");
        }
        for path in ["a.js", "a.mjs", "a.cjs", "a.jsx"] {
            assert_eq!(
                class(path, &settings),
                FileClass::Unindexed(Unindexed::JavaScript),
                "{path}"
            );
        }
        let input = |role| FileClass::Input(role, ScopeStatus::InScope);
        assert_eq!(class("package.json", &settings), input(InputRole::Package));
        assert_eq!(class("a/pyproject.toml", &settings), input(InputRole::Pyproject));
        assert_eq!(class("tsconfig.json", &settings), input(InputRole::Tsconfig));
        assert_eq!(
            class("pkg/tsconfig.build.json", &settings),
            input(InputRole::Tsconfig)
        );
        assert_eq!(
            class(".unfold/config.toml", &settings),
            input(InputRole::UnfoldConfig)
        );
        assert_eq!(class("sub/.unfold/config.toml", &settings), FileClass::Other);
        assert_eq!(class("node_modules/x/index.ts", &settings), FileClass::Other);
        assert_eq!(class("a/__pycache__/b.py", &settings), FileClass::Other);
    }

    #[test]
    fn applies_scope_and_languages() {
        let settings =
            Settings::parse("languages = [\"python\"]\n[scope]\nexclude = [\"legacy\"]\n").unwrap();
        assert_eq!(
            class("a.ts", &settings),
            FileClass::Unindexed(Unindexed::LanguageOff)
        );
        assert_eq!(class("a.d.ts", &settings), FileClass::Other);
        assert_eq!(
            class("legacy/a.py", &settings),
            FileClass::Unindexed(Unindexed::Excluded)
        );
        assert_eq!(
            class("legacy/pyproject.toml", &settings),
            FileClass::Input(InputRole::Pyproject, ScopeStatus::Excluded)
        );
        let narrowed = Settings::parse("[scope]\ninclude = [\"src\"]\n").unwrap();
        assert_eq!(
            class("tests/a.py", &narrowed),
            FileClass::Unindexed(Unindexed::NotIncluded)
        );
        assert_eq!(class("src/a.py", &narrowed), FileClass::Source(Language::Python));
    }
}
