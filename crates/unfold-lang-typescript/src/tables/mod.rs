//! Tables generated from TypeScript's default library and `@types/node` at a pinned version
//! (`cargo xtask gen-lib-tables`; the versions are in [`generated`]): the known global values
//! (evidence level 2: an undeclared `console` or `setTimeout` is a library global), the global
//! library class and interface names (evidence level 4: a parameter written `: Map<K, V>` is a
//! library receiver), and Node's built-in module specifiers (module labels: `fs` is `node:fs`).
//! The default library's global type aliases (`Partial`, `Record`, `ReturnType`, …), which type
//! links label `global`, are a separate table generated from the library files alone
//! (`cargo xtask gen-lib-tables <lib-dir> --aliases`; the version is in [`aliases`]).
//!
//! The ECMAScript tables come from the standard editions' `lib.es*.d.ts` files alone (not the
//! `lib.esnext.*` proposals): the language's own globals (`JSON`, `Math`, `Promise`; not the
//! DOM's or Node's `console`), the type of each, its class types (`Array`, `Map`, `Error`) and
//! what each of its types extends, and the methods of its built-in types (`Array`, `String`,
//! `ObjectConstructor`) and the values of its namespaces (`Reflect.ownKeys`, `Intl.Collator`), so
//! a built-in method can be named on a literal or a written type.
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

/// Whether `name` is one of ECMAScript's own global values (`JSON`, `Math`, `Promise`, `String`,
/// `parseInt`, `globalThis`, `undefined`), declared by the `lib.es*.d.ts` files: not a DOM or Node
/// global such as `console`, `fetch`, or `setTimeout`.
pub fn is_ecmascript_global(name: &str) -> bool {
    ecmascript_global(name).is_some()
}

/// ECMAScript's own global value `name`, as the table spells it.
pub fn ecmascript_global(name: &str) -> Option<&'static str> {
    let table = generated::ECMASCRIPT_GLOBALS;
    table.get(table.binary_search(&name).ok()?).copied()
}

/// The ECMAScript class or interface `name` (or `name` prefixed with `globalThis.`), as the
/// table spells it (`Iterable`, `Map`, `Intl.Collator`).
pub fn ecmascript_type_name(name: &str) -> Option<&'static str> {
    let name = name.strip_prefix("globalThis.").unwrap_or(name);
    let table = generated::ECMASCRIPT_TYPES;
    table.get(table.binary_search(&name).ok()?).copied()
}

/// The ECMAScript class type `name` (or `name` prefixed with `globalThis.`), as the table spells
/// it: the type a global constructor constructs (`Array`, `Map`, `Error`, `Intl.Collator`), not a
/// protocol only values of other types implement (`Iterator`, `PromiseLike`, `ReadonlyArray`).
pub fn ecmascript_class(name: &str) -> Option<&'static str> {
    let name = name.strip_prefix("globalThis.").unwrap_or(name);
    let table = generated::ECMASCRIPT_CLASSES;
    table.get(table.binary_search(&name).ok()?).copied()
}

/// Every ECMAScript interface the ECMAScript type `name` extends, directly or not
/// (`AggregateError` → `Error`).
pub fn ecmascript_supertypes(name: &str) -> &'static [&'static str] {
    let name = name.strip_prefix("globalThis.").unwrap_or(name);
    let table = generated::ECMASCRIPT_SUPERTYPES;
    table
        .binary_search_by(|(key, _)| (*key).cmp(name))
        .ok()
        .and_then(|index| table.get(index))
        .map_or(&[], |(_, supertypes)| supertypes)
}

/// The type whose methods an ECMAScript global value has: the type its declaration writes
/// (`Object` → `ObjectConstructor`, `JSON` → `JSON`), or the value namespace itself (`Reflect`).
pub fn ecmascript_value_type(name: &str) -> Option<&'static str> {
    let table = generated::ECMASCRIPT_VALUE_TYPES;
    let index = table.binary_search_by(|(key, _)| key.cmp(&name)).ok()?;
    table.get(index).map(|(_, written)| *written)
}

/// The type whose members the ECMAScript class `global`'s instances have (`Array`), or, when
/// `is_static`, the class itself (`ArrayConstructor`).
pub fn ecmascript_member_type(global: &str, is_static: bool) -> Option<&'static str> {
    if is_static {
        ecmascript_value_type(global)
    } else {
        ecmascript_class(global)
    }
}

/// The ECMAScript interface `name` (`Array`, `Map`, `ReadonlyArray`, `PromiseLike`), or `name`
/// prefixed with `globalThis.`, as the methods table spells it: one with methods of its own or
/// inherited.
pub fn ecmascript_type(name: &str) -> Option<&'static str> {
    let name = name.strip_prefix("globalThis.").unwrap_or(name);
    let table = generated::ECMASCRIPT_METHODS;
    let index = table.binary_search_by(|(key, _)| (*key).cmp(name)).ok()?;
    table.get(index).map(|(key, _)| *key)
}

/// Whether the qualified type name is one of ECMAScript's own types: an interface with methods
/// ([`ecmascript_type`]) or the type of the same name as an ECMAScript global (`Error`).
pub fn is_ecmascript_type(qualified: &str) -> bool {
    let name = qualified.strip_prefix("globalThis.").unwrap_or(qualified);
    ecmascript_type(name).is_some() || is_ecmascript_global(name)
}

/// Whether `method` is a method of the ECMAScript type `type_name` (`Array`, `String`,
/// `Promise`, `ObjectConstructor`), the interfaces it extends included, or a value of the value
/// namespace `type_name` (`Reflect.ownKeys`, `Intl.Collator`). Every ECMAScript type also has
/// `Object`'s methods (`toString`, `valueOf`), as TypeScript gives every object type.
pub fn is_ecmascript_method(type_name: &str, method: &str) -> bool {
    let declares = |name: &str| {
        let table = generated::ECMASCRIPT_METHODS;
        table
            .binary_search_by(|(key, _)| (*key).cmp(name))
            .ok()
            .and_then(|index| table.get(index))
            .is_some_and(|(_, methods)| methods.binary_search(&method).is_ok())
    };
    declares(type_name) || (is_ecmascript_type(type_name) && declares("Object"))
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
            generated::ECMASCRIPT_GLOBALS,
            generated::ECMASCRIPT_CLASSES,
            generated::ECMASCRIPT_TYPES,
            aliases::TYPE_ALIASES,
        ] {
            assert!(table.windows(2).all(|pair| pair[0] < pair[1]));
        }
        let supertypes = generated::ECMASCRIPT_SUPERTYPES;
        assert!(supertypes.windows(2).all(|pair| pair[0].0 < pair[1].0));
        for (_, names) in supertypes {
            assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
        }
        let types = generated::ECMASCRIPT_VALUE_TYPES;
        assert!(types.windows(2).all(|pair| pair[0].0 < pair[1].0));
        let methods = generated::ECMASCRIPT_METHODS;
        assert!(methods.windows(2).all(|pair| pair[0].0 < pair[1].0));
        for (_, names) in methods {
            assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
        }
    }

    #[test]
    fn ecmascript_globals_and_methods() {
        for name in [
            "JSON",
            "Object",
            "Math",
            "Promise",
            "String",
            "Error",
            "Array",
            "Number",
            "Map",
            "Set",
            "Date",
            "RegExp",
            "Symbol",
            "Reflect",
            "parseInt",
            "Intl",
            "globalThis",
            "undefined",
        ] {
            assert!(is_ecmascript_global(name), "{name}");
        }
        // Every ECMAScript global is a known global (a library receiver by name).
        for name in generated::ECMASCRIPT_GLOBALS {
            assert!(is_known_global(name), "{name}");
        }
        for name in [
            "console",
            "fetch",
            "setTimeout",
            "process",
            "Buffer",
            "document",
            "window",
            "Temporal",
            "DisposableStack",
        ] {
            assert!(!is_ecmascript_global(name), "{name}");
        }
        assert_eq!(ecmascript_global("JSON"), Some("JSON"));
        for name in [
            "Array",
            "Map",
            "Promise",
            "Error",
            "AggregateError",
            "String",
            "globalThis.Set",
        ] {
            assert!(ecmascript_class(name).is_some(), "{name}");
        }
        for name in [
            "Iterator",
            "IteratorObject",
            "PromiseLike",
            "ArrayLike",
            "ReadonlyArray",
            "Iterable",
            "Disposable",
            "JSON",
            "Math",
        ] {
            assert!(ecmascript_class(name).is_none(), "{name}");
        }
        for name in [
            "Iterable",
            "PromiseLike",
            "Map",
            "Intl.Collator",
            "globalThis.Iterator",
        ] {
            assert!(ecmascript_type_name(name).is_some(), "{name}");
        }
        for name in [
            "HTMLElement",
            "NodeJS.Process",
            "Temporal.Instant",
            "Disposable",
            "Partial",
        ] {
            assert!(ecmascript_type_name(name).is_none(), "{name}");
        }
        assert_eq!(ecmascript_supertypes("AggregateError"), ["Error"]);
        assert_eq!(ecmascript_supertypes("RegExpMatchArray"), ["Array"]);
        assert!(ecmascript_supertypes("Map").is_empty());
        assert_eq!(ecmascript_value_type("Object"), Some("ObjectConstructor"));
        assert_eq!(ecmascript_value_type("JSON"), Some("JSON"));
        assert_eq!(ecmascript_value_type("Reflect"), Some("Reflect"));
        assert_eq!(ecmascript_value_type("console"), None);
        assert_eq!(ecmascript_member_type("Array", false), Some("Array"));
        assert_eq!(ecmascript_member_type("Array", true), Some("ArrayConstructor"));
        assert_eq!(ecmascript_member_type("Error", false), Some("Error"));
        assert_eq!(ecmascript_member_type("JSON", false), None);
        for (type_name, method) in [
            ("JSON", "parse"),
            ("ObjectConstructor", "keys"),
            ("Math", "max"),
            ("PromiseConstructor", "all"),
            ("Reflect", "ownKeys"),
            ("Intl", "Collator"),
            ("Intl", "getCanonicalLocales"),
            ("Array", "map"),
            ("Array", "includes"),
            ("Array", "flatMap"),
            ("Array", "at"),
            ("ReadonlyArray", "map"),
            ("String", "trim"),
            ("String", "replaceAll"),
            ("Promise", "then"),
            ("Promise", "finally"),
            ("Map", "get"),
            ("RegExp", "test"),
            ("ArrayIterator", "next"),
        ] {
            assert!(is_ecmascript_method(type_name, method), "{type_name}.{method}");
        }
        assert_eq!(ecmascript_type("ReadonlyArray"), Some("ReadonlyArray"));
        assert_eq!(ecmascript_type("globalThis.Map"), Some("Map"));
        assert_eq!(ecmascript_type("Console"), None);
        for name in ["Promise", "Error", "globalThis.Array", "PromiseLike", "Date"] {
            assert!(is_ecmascript_type(name), "{name}");
        }
        for name in ["HTMLElement", "Console", "NodeJS.Process", "Partial", "Express"] {
            assert!(!is_ecmascript_type(name), "{name}");
        }
        assert!(!is_ecmascript_method("Array", "length"));
        assert!(!is_ecmascript_method("Array", "trim"));
        assert!(!is_ecmascript_method("Console", "log"));
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
