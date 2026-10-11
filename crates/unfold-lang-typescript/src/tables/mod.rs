//! Tables generated from TypeScript's default library and `@types/node` at a pinned version
//! (`cargo xtask gen-lib-tables`; the versions are in [`generated`]): the known global values
//! (evidence level 2: an undeclared `console` or `setTimeout` is a library global), the global
//! library class and interface names (evidence level 4: a parameter written `: Map<K, V>` is a
//! library receiver), and Node's built-in module specifiers (module labels: `fs` is `node:fs`).
//! The default library's global type aliases (`Partial`, `Record`, `ReturnType`, …), which type
//! links label `global`, are a separate table generated from the library files alone
//! (`cargo xtask gen-lib-tables <lib-dir> --aliases`; the version is in [`aliases`]).
//!
//! Each table is a sorted static array searched by bisection.

mod aliases;
mod generated;

/// The TypeScript version the default-library tables come from.
pub const TYPESCRIPT_VERSION: &str = generated::TYPESCRIPT_VERSION;
/// The `@types/node` version the Node tables come from.
pub const NODE_TYPES_VERSION: &str = generated::NODE_TYPES_VERSION;

/// Whether `name` is a known global value of the default library or Node (`console`, `Math`,
/// `process`, `setTimeout`, `globalThis`, `undefined`, …).
pub fn is_known_global(name: &str) -> bool {
    generated::GLOBALS.binary_search(&name).is_ok()
}

/// Whether the qualified type name (`Map`, `Intl.Collator`, `NodeJS.Process`, or any of them
/// prefixed with `globalThis.`) is a global library class or interface.
pub fn is_library_type(qualified: &str) -> bool {
    let name = qualified.strip_prefix("globalThis.").unwrap_or(qualified);
    generated::LIBRARY_TYPES.binary_search(&name).is_ok()
}

/// Whether the qualified type name (`Partial`, `Record`, or either prefixed with `globalThis.`)
/// is a global type alias of the default library.
pub fn is_library_type_alias(qualified: &str) -> bool {
    let name = qualified.strip_prefix("globalThis.").unwrap_or(qualified);
    aliases::TYPE_ALIASES.binary_search(&name).is_ok()
}

/// Whether `specifier` names a Node built-in module (`fs`, `fs/promises`, `node:fs`,
/// `node:test`; but not `test`, which Node only provides with the `node:` prefix).
pub fn is_builtin_module(specifier: &str) -> bool {
    generated::BUILTIN_MODULES.binary_search(&specifier).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_sorted_and_unique() {
        for table in [
            generated::GLOBALS,
            generated::LIBRARY_TYPES,
            generated::BUILTIN_MODULES,
            aliases::TYPE_ALIASES,
        ] {
            assert!(table.windows(2).all(|pair| pair[0] < pair[1]));
        }
    }

    #[test]
    fn answers_known_names() {
        for name in [
            "console",
            "Math",
            "process",
            "setTimeout",
            "globalThis",
            "undefined",
            "Buffer",
        ] {
            assert!(is_known_global(name), "{name}");
        }
        for name in ["express", "require_x", "NodeJS", ""] {
            assert!(!is_known_global(name), "{name}");
        }
        for name in [
            "Map",
            "Array",
            "globalThis.Array",
            "Intl.Collator",
            "NodeJS.Process",
            "Promise",
        ] {
            assert!(is_library_type(name), "{name}");
        }
        assert!(!is_library_type("globalThis.globalThis.Array"));
        assert!(!is_library_type("Express"));
        for name in [
            "Partial",
            "Record",
            "Pick",
            "Omit",
            "ReturnType",
            "Awaited",
            "Readonly",
            "Required",
            "NonNullable",
            "Parameters",
            "globalThis.Partial",
        ] {
            assert!(is_library_type_alias(name), "{name}");
        }
        for name in ["Promise", "Map", "Express", ""] {
            assert!(!is_library_type_alias(name), "{name}");
        }
        for name in ["fs", "fs/promises", "node:fs", "node:test", "path", "node:crypto"] {
            assert!(is_builtin_module(name), "{name}");
        }
        for name in ["test", "express", "node:express", ""] {
            assert!(!is_builtin_module(name), "{name}");
        }
        assert!(!TYPESCRIPT_VERSION.is_empty() && !NODE_TYPES_VERSION.is_empty());
    }
}
