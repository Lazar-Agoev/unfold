//! Module-label spelling for TypeScript (the shared order of rules is in
//! `unfold_engine::labels`).
//!
//! | Written or located | Label |
//! | --- | --- |
//! | A Node built-in: `fs`, `fs/promises`, `node:fs` | `node:fs`, `node:fs/promises`, `node:fs` (subpaths kept) |
//! | A package specifier: `express/router`, `@scope/name/sub` | `express`, `@scope/name` |
//! | A package location `@types/x`, `@types/a__b` | `x`, `@a/b` |
//! | ECMAScript's standard objects (the language's builtins) | `ecmascript` |
//! | The rest of the default library or a known global (the DOM, Node) | `global` |
//! | An unindexed repository file | Its repository-relative path |
//!
//! A specifier is a package specifier unless it is empty, relative (`.`), or absolute (`/`, or a
//! drive such as `C:\`).

use unfold_engine::evidence::ExternalLocation;

use crate::tables;

/// The label of undeclared, unimported globals.
pub const GLOBAL: &str = "global";

/// The label of ECMAScript's standard objects, the language's builtins.
pub const ECMASCRIPT: &str = "ecmascript";

/// Whether `specifier` names a package rather than a path.
pub fn is_package_specifier(specifier: &str) -> bool {
    let bytes = specifier.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');
    !specifier.is_empty() && !specifier.starts_with('.') && !specifier.starts_with('/') && !drive
}

/// The package a package specifier names: the first segment, or the first two for a scoped
/// name (`@scope/name`).
pub fn package_name(specifier: &str) -> &str {
    let mut segments = specifier.match_indices('/').map(|(index, _)| index);
    let end = if specifier.starts_with('@') {
        segments.nth(1)
    } else {
        segments.next()
    };
    &specifier[..end.unwrap_or(specifier.len())]
}

/// The label for a package specifier: a Node built-in in its `node:` form (subpaths kept), or
/// the package name.
pub fn label_for_specifier(specifier: &str) -> String {
    if tables::is_builtin_module(specifier) {
        return if specifier.starts_with("node:") {
            specifier.to_owned()
        } else {
            format!("node:{specifier}")
        };
    }
    package_name(specifier).to_owned()
}

/// The label for an external declaration location.
pub fn label_for_location(location: &ExternalLocation) -> String {
    match location {
        ExternalLocation::Library => GLOBAL.to_owned(),
        ExternalLocation::Builtin => ECMASCRIPT.to_owned(),
        ExternalLocation::File(path) => path.to_string(),
        ExternalLocation::Package(name) => match name.strip_prefix("@types/") {
            Some(typed) => match typed.split_once("__") {
                Some((scope, rest)) => format!("@{scope}/{rest}"),
                None => typed.to_owned(),
            },
            None => label_for_specifier(name),
        },
    }
}

/// Whether a package specifier can be labeled: its label is nonempty and isn't a JSON array.
/// No installable package has a name that fails this, so such an import names nothing.
pub fn is_labelable(specifier: &str) -> bool {
    let label = label_for_specifier(specifier);
    !label.is_empty() && !label.starts_with('[')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_package_and_builtin_labels() {
        assert_eq!(label_for_specifier("express/router"), "express");
        assert_eq!(label_for_specifier("express"), "express");
        assert_eq!(label_for_specifier("@scope/name/x"), "@scope/name");
        assert_eq!(label_for_specifier("@scope"), "@scope");
        assert_eq!(label_for_specifier("fs"), "node:fs");
        assert_eq!(label_for_specifier("node:fs"), "node:fs");
        assert_eq!(label_for_specifier("fs/promises"), "node:fs/promises");
        assert_eq!(label_for_specifier("node:fs/promises"), "node:fs/promises");
        assert_eq!(label_for_specifier("node:test"), "node:test");
        assert_eq!(label_for_specifier("test"), "test");
        assert_eq!(label_for_specifier("fs/unknown"), "fs");
    }

    #[test]
    fn recognizes_package_specifiers() {
        for specifier in ["express", "@a/b", "node:fs", "#internal", "x/../y"] {
            assert!(is_package_specifier(specifier), "{specifier}");
        }
        for specifier in ["", "./a", "../a", ".", "/abs", "C:\\x", "c:/x"] {
            assert!(!is_package_specifier(specifier), "{specifier}");
        }
        assert!(!is_labelable("[x]"));
        assert!(is_labelable("x[0]"));
    }

    #[test]
    fn spells_location_labels() {
        let package = |name: &str| ExternalLocation::Package(name.into());
        assert_eq!(label_for_location(&ExternalLocation::Library), "global");
        assert_eq!(label_for_location(&ExternalLocation::Builtin), "ecmascript");
        assert_eq!(
            label_for_location(&ExternalLocation::File("legacy/a.ts".into())),
            "legacy/a.ts"
        );
        assert_eq!(label_for_location(&package("@types/node")), "node");
        assert_eq!(label_for_location(&package("@types/a__b")), "@a/b");
        assert_eq!(label_for_location(&package("express")), "express");
        assert_eq!(label_for_location(&package("fs")), "node:fs");
        assert_eq!(label_for_location(&package("@scope/name")), "@scope/name");
    }
}
