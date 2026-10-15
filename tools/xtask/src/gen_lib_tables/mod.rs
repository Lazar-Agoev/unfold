//! `cargo xtask gen-lib-tables <lib-dir> --node-types <dir> [--check]`: the TypeScript front end's
//! generated tables of known globals, library receiver types, and Node built-in modules.
//!
//! The tables are read from declaration files at a pinned version, parsed with oxc
//! ([`scan`]), never from a running compiler: every `lib.*.d.ts` of a TypeScript package
//! (the proof of concept used TypeScript 6.0.3's `built/local`), plus `@types/node`'s `index.d.ts`
//! and every file it reaches through `/// <reference path="…" />`. The ECMAScript tables (the
//! globals of the language itself, their types, the class types and what each type extends, and
//! the built-in types' methods) come from the standard editions' files alone (`lib.es5.d.ts`,
//! `lib.es2015.*` to the latest year), without the DOM, Node, or `lib.esnext.*` proposals. The result is written to
//! `crates/unfold-lang-typescript/src/tables/generated.rs` as sorted static arrays, with the
//! source versions and a SHA-256 of the declaration set in its header. With `--check`, the file
//! is compared instead of written.
//!
//! `cargo xtask gen-lib-tables <lib-dir> --aliases [--check]` reads only the `lib.*.d.ts` files
//! and writes (or checks) `crates/unfold-lang-typescript/src/tables/aliases.rs`: the default
//! library's global type aliases (`Partial`, `Record`, `ReturnType`, …), which type links label
//! `global`, with the TypeScript version and the library files' SHA-256 in its header.

mod scan;

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use sha2::{Digest, Sha256};

use crate::repo;
use scan::Model;

/// Arguments of `gen-lib-tables`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// The folder holding TypeScript's `lib.*.d.ts` files (a TypeScript package's `lib/`, or a
    /// compiler build's `built/local/`).
    lib_dir: PathBuf,
    /// The `@types/node` package folder (required unless `--aliases`).
    #[arg(long, required_unless_present = "aliases")]
    node_types: Option<PathBuf>,
    /// Generate only the global type alias table, from the `lib.*.d.ts` files alone.
    #[arg(long)]
    aliases: bool,
    /// Compare the generated file with the checked-in one instead of writing it.
    #[arg(long)]
    check: bool,
}

/// Where the tables are checked in.
fn output_path() -> PathBuf {
    repo::root().join("crates/unfold-lang-typescript/src/tables/generated.rs")
}

/// Where the global type alias table is checked in.
fn aliases_path() -> PathBuf {
    repo::root().join("crates/unfold-lang-typescript/src/tables/aliases.rs")
}

/// Generates the tables and writes or checks them.
pub(crate) fn run(args: &Args) -> anyhow::Result<()> {
    if args.aliases {
        let libraries = library_files(&args.lib_dir)?;
        let generated = render_aliases(&typescript_version(&args.lib_dir)?, &libraries)?;
        repo::check_or_write(
            &aliases_path(),
            &generated,
            args.check,
            "cargo xtask gen-lib-tables <lib-dir> --aliases",
        )?;
        return Ok(());
    }
    let Some(node_types) = &args.node_types else {
        bail!("--node-types is required unless --aliases is given");
    };
    let sources = Sources::read(&args.lib_dir, node_types)?;
    let generated = sources.render()?;
    repo::check_or_write(
        &output_path(),
        &generated,
        args.check,
        "cargo xtask gen-lib-tables",
    )?;
    Ok(())
}

/// One declaration file: its name in the header's hash and its text.
struct Source {
    name: String,
    text: String,
}

/// The declaration files and their versions.
struct Sources {
    typescript_version: String,
    node_version: String,
    libraries: Vec<Source>,
    node: Vec<Source>,
}

/// Every `lib.*.d.ts` file of `lib_dir`, sorted by name.
fn library_files(lib_dir: &Path) -> anyhow::Result<Vec<Source>> {
    let mut libraries = Vec::new();
    let entries = std::fs::read_dir(lib_dir).with_context(|| format!("cannot read {}", lib_dir.display()))?;
    for entry in entries {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if is_library_file(name) {
            let text = read(&path)?;
            libraries.push(Source {
                name: name.to_owned(),
                text,
            });
        }
    }
    if libraries.is_empty() {
        bail!("{} has no lib.*.d.ts files", lib_dir.display());
    }
    libraries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(libraries)
}

/// SHA-256 over `files` (sorted by name): each name, NUL, its bytes, NUL.
fn digest<'s>(files: impl IntoIterator<Item = &'s Source>) -> String {
    let mut files: Vec<&Source> = files.into_iter().collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.name.as_bytes());
        hasher.update([0]);
        hasher.update(file.text.as_bytes());
        hasher.update([0]);
    }
    hasher.finalize().iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// The global type alias table of the library files.
fn render_aliases(typescript_version: &str, libraries: &[Source]) -> anyhow::Result<String> {
    let mut model = Model::default();
    for source in libraries {
        model.add_file(&source.name, &source.text)?;
    }
    let mut out = String::new();
    out.push_str("//! Generated by `cargo xtask gen-lib-tables <lib-dir> --aliases`; do not edit by hand.\n");
    let _ = writeln!(
        out,
        "//! Source: TypeScript {typescript_version} (`lib.*.d.ts`, {} files).",
        libraries.len()
    );
    let _ = writeln!(out, "//! Declaration-set SHA-256: {}.", digest(libraries));
    array(
        &mut out,
        "/// Global type alias names of the default library, qualified through namespaces\n\
         /// (`Intl.LocalesArgument`), sorted by bytes. Each also exists as `globalThis.<name>`;\n\
         /// lookups strip one leading `globalThis.`.",
        "TYPE_ALIASES",
        &model.library_type_aliases(),
    );
    Ok(out)
}

impl Sources {
    fn read(lib_dir: &Path, node_dir: &Path) -> anyhow::Result<Self> {
        let libraries = library_files(lib_dir)?;
        let node = node_files(node_dir)?;
        Ok(Self {
            typescript_version: typescript_version(lib_dir)?,
            node_version: package_version(&node_dir.join("package.json"))?,
            libraries,
            node,
        })
    }

    /// The generated Rust file.
    fn render(&self) -> anyhow::Result<String> {
        let mut model = Model::default();
        let mut node_model = Model::default();
        let mut ecmascript_model = Model::default();
        for source in &self.libraries {
            model.add_file(&source.name, &source.text)?;
            if is_ecmascript_file(&source.name) {
                ecmascript_model.add_file(&source.name, &source.text)?;
            }
        }
        for source in &self.node {
            model.add_file(&source.name, &source.text)?;
            node_model.add_file(&source.name, &source.text)?;
        }
        let builtins: Vec<String> = node_model.ambient_modules.into_iter().collect();
        let ecmascript = Ecmascript {
            files: self
                .libraries
                .iter()
                .filter(|source| is_ecmascript_file(&source.name))
                .count(),
            globals: ecmascript_model.global_values(),
            value_types: ecmascript_model.global_value_types(),
            types: ecmascript_model.library_types(),
            classes: ecmascript_model.classes(),
            supertypes: ecmascript_model.supertypes(),
            methods: ecmascript_model.methods()?,
        };
        Ok(render(
            &Header {
                typescript_version: &self.typescript_version,
                library_files: self.libraries.len(),
                node_version: &self.node_version,
                digest: &self.digest(),
            },
            &model.global_values(),
            &model.library_types(),
            &builtins,
            &ecmascript,
        ))
    }

    /// SHA-256 over every file, sorted by name: its name, NUL, its bytes, NUL.
    fn digest(&self) -> String {
        digest(self.libraries.iter().chain(&self.node))
    }
}

/// Whether a default-library file declares a standard edition of ECMAScript (`lib.es5.d.ts`,
/// `lib.es2015.*`, …), not a host such as the DOM or a proposal (`lib.esnext.*`).
fn is_ecmascript_file(name: &str) -> bool {
    is_library_file(name) && name.starts_with("lib.es") && !name.starts_with("lib.esnext")
}

/// Whether a file name is a default-library file: `lib.<something>.d.ts` (not `lib.d.ts`).
fn is_library_file(name: &str) -> bool {
    name.strip_prefix("lib.")
        .and_then(|rest| rest.strip_suffix(".d.ts"))
        .is_some_and(|middle| !middle.is_empty())
}

fn read(path: &Path) -> anyhow::Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))
}

/// `@types/node`'s `index.d.ts` and every file reachable through `/// <reference path />`,
/// named `@types/node/<relative path>`, sorted by name.
fn node_files(node_dir: &Path) -> anyhow::Result<Vec<Source>> {
    let root =
        std::fs::canonicalize(node_dir).with_context(|| format!("cannot open {}", node_dir.display()))?;
    let mut seen = BTreeSet::new();
    let mut files = Vec::new();
    let mut pending = vec![root.join("index.d.ts")];
    while let Some(path) = pending.pop() {
        let path = std::fs::canonicalize(&path).with_context(|| format!("cannot open {}", path.display()))?;
        if !seen.insert(path.clone()) {
            continue;
        }
        let Ok(relative) = path.strip_prefix(&root) else {
            bail!("{} references a file outside the package", path.display());
        };
        let text = read(&path)?;
        let folder = path.parent().unwrap_or(&root).to_owned();
        pending.extend(
            reference_paths(&text)
                .into_iter()
                .map(|reference| folder.join(reference)),
        );
        let relative: Vec<String> = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        files.push(Source {
            name: format!("@types/node/{}", relative.join("/")),
            text,
        });
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(files)
}

/// The targets of `/// <reference path="…" />` directives.
fn reference_paths(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| {
            let directive = line.trim_start().strip_prefix("///")?.trim_start();
            let rest = directive.strip_prefix("<reference")?;
            let value = &rest[rest.find("path=")? + "path=".len()..];
            let quote = value
                .chars()
                .next()
                .filter(|quote| *quote == '"' || *quote == '\'')?;
            let value = &value[1..];
            Some(&value[..value.find(quote)?])
        })
        .collect()
}

/// The version of the nearest `package.json` named `typescript` at or above `lib_dir`.
fn typescript_version(lib_dir: &Path) -> anyhow::Result<String> {
    let start =
        std::fs::canonicalize(lib_dir).with_context(|| format!("cannot open {}", lib_dir.display()))?;
    for folder in start.ancestors() {
        let manifest = folder.join("package.json");
        if !manifest.is_file() {
            continue;
        }
        let json: serde_json::Value = serde_json::from_str(&read(&manifest)?)
            .with_context(|| format!("{} is not JSON", manifest.display()))?;
        if json.get("name").and_then(serde_json::Value::as_str) == Some("typescript") {
            return version_of(&json, &manifest);
        }
    }
    bail!(
        "no package.json named `typescript` at or above {}",
        lib_dir.display()
    )
}

fn package_version(manifest: &Path) -> anyhow::Result<String> {
    let json: serde_json::Value = serde_json::from_str(&read(manifest)?)
        .with_context(|| format!("{} is not JSON", manifest.display()))?;
    version_of(&json, manifest)
}

fn version_of(json: &serde_json::Value, manifest: &Path) -> anyhow::Result<String> {
    json.get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("{} has no version", manifest.display()))
}

/// What the generated file's header says.
struct Header<'a> {
    typescript_version: &'a str,
    library_files: usize,
    node_version: &'a str,
    digest: &'a str,
}

/// The ECMAScript tables, from the standard editions' `lib.es*.d.ts` files alone.
struct Ecmascript {
    files: usize,
    globals: Vec<String>,
    value_types: Vec<(String, String)>,
    types: Vec<String>,
    classes: Vec<String>,
    supertypes: Vec<(String, Vec<String>)>,
    methods: Vec<(String, Vec<String>)>,
}

/// The widest generated line.
const WIDTH: usize = 100;

fn render(
    header: &Header<'_>,
    globals: &[String],
    library_types: &[String],
    builtins: &[String],
    ecmascript: &Ecmascript,
) -> String {
    let mut out = String::new();
    out.push_str("//! Generated by `cargo xtask gen-lib-tables`; do not edit by hand.\n");
    let _ = writeln!(
        out,
        "//! Sources: TypeScript {} (`lib.*.d.ts`, {} files) and @types/node {}.",
        header.typescript_version, header.library_files, header.node_version
    );
    let _ = writeln!(
        out,
        "//! The ECMAScript tables come from the standard editions' `lib.es*.d.ts` files alone ({} files).",
        ecmascript.files
    );
    let _ = writeln!(out, "//! Declaration-set SHA-256: {}.", header.digest);
    out.push('\n');
    out.push_str("/// The TypeScript version the default-library tables come from.\n");
    let _ = writeln!(
        out,
        "pub(crate) const TYPESCRIPT_VERSION: &str = {:?};",
        header.typescript_version
    );
    out.push_str("/// The `@types/node` version the Node tables come from.\n");
    let _ = writeln!(
        out,
        "pub(crate) const NODE_TYPES_VERSION: &str = {:?};",
        header.node_version
    );
    array(
        &mut out,
        "/// Known global value names, sorted by bytes.",
        "GLOBALS",
        globals,
    );
    array(
        &mut out,
        "/// Global library class and interface names, qualified through namespaces (`Intl.Collator`),\n\
         /// sorted by bytes. Each also exists as `globalThis.<name>`; lookups strip one leading\n\
         /// `globalThis.`.",
        "LIBRARY_TYPES",
        library_types,
    );
    array(
        &mut out,
        "/// Node built-in module specifiers (`fs`, `fs/promises`, `node:fs`, `node:test`), sorted by\n\
         /// bytes.",
        "BUILTIN_MODULES",
        builtins,
    );
    array(
        &mut out,
        "/// ECMAScript's own global value names (`JSON`, `Math`, `Promise`, `parseInt`), declared in the\n\
         /// standard editions' `lib.es*.d.ts` files: not the DOM's or Node's (`console`, `fetch`,\n\
         /// `setTimeout`), nor a proposal's (`Temporal`). Sorted by bytes.",
        "ECMASCRIPT_GLOBALS",
        &ecmascript.globals,
    );
    pairs(
        &mut out,
        "/// The type of each ECMAScript global value that has one, for member calls on it: the type its\n\
         /// declaration writes (`Object` → `ObjectConstructor`), or the value namespace itself\n\
         /// (`Reflect`). Sorted by name.",
        "ECMASCRIPT_VALUE_TYPES",
        &ecmascript.value_types,
    );
    array(
        &mut out,
        "/// ECMAScript's class and interface names, qualified through namespaces (`Iterable`,\n\
         /// `Intl.Collator`), sorted by bytes.",
        "ECMASCRIPT_TYPES",
        &ecmascript.types,
    );
    array(
        &mut out,
        "/// ECMAScript's class types: each global value, qualified through namespaces, whose type\n\
         /// constructs the type of the same name (`Array`, `Map`, `Error`, `Intl.Collator`). Sorted by\n\
         /// bytes.",
        "ECMASCRIPT_CLASSES",
        &ecmascript.classes,
    );
    lists(
        &mut out,
        "/// Each ECMAScript class or interface that extends others (`AggregateError`), with every\n\
         /// interface it extends, directly or not (`Error`). Sorted by name, each list sorted by bytes.",
        "ECMASCRIPT_SUPERTYPES",
        &ecmascript.supertypes,
    );
    lists(
        &mut out,
        "/// The method names of each ECMAScript interface (`Array`, `String`, `ObjectConstructor`), with\n\
         /// those of the interfaces it extends, and the values of each value namespace (`Reflect`'s\n\
         /// functions, `Intl`'s `Collator`). Sorted by name, each with its names sorted by bytes.",
        "ECMASCRIPT_METHODS",
        &ecmascript.methods,
    );
    out
}

/// One sorted static array of string pairs, several per line.
fn pairs(out: &mut String, docs: &str, name: &str, items: &[(String, String)]) {
    let mut sorted = items.to_vec();
    sorted.sort();
    sorted.dedup();
    let quoted: Vec<String> = sorted
        .iter()
        .map(|(key, value)| format!("({key:?}, {value:?}),"))
        .collect();
    out.push('\n');
    out.push_str(docs);
    out.push('\n');
    out.push_str("#[rustfmt::skip]\n");
    let _ = writeln!(out, "pub(crate) static {name}: &[(&str, &str)] = &[");
    pack(out, "   ", &quoted);
    out.push_str("];\n");
}

/// One sorted static array of names, each with its sorted list of strings.
fn lists(out: &mut String, docs: &str, name: &str, items: &[(String, Vec<String>)]) {
    let mut sorted = items.to_vec();
    sorted.sort();
    sorted.dedup_by(|a, b| a.0 == b.0);
    out.push('\n');
    out.push_str(docs);
    out.push('\n');
    out.push_str("#[rustfmt::skip]\n");
    let _ = writeln!(out, "pub(crate) static {name}: &[(&str, &[&str])] = &[");
    for (key, values) in &sorted {
        let mut values = values.clone();
        values.sort();
        values.dedup();
        let _ = writeln!(out, "    ({key:?}, &[");
        let quoted: Vec<String> = values.iter().map(|value| format!("{value:?},")).collect();
        pack(out, "       ", &quoted);
        out.push_str("    ]),\n");
    }
    out.push_str("];\n");
}

/// Writes `items` (each already quoted and followed by a comma) after `indent`, as many to a
/// line as fit in [`WIDTH`].
fn pack(out: &mut String, indent: &str, items: &[String]) {
    let mut line = String::from(indent);
    for item in items {
        if line.len() + 1 + item.len() > WIDTH && line.len() > indent.len() {
            out.push_str(&line);
            out.push('\n');
            line = String::from(indent);
        }
        line.push(' ');
        line.push_str(item);
    }
    if line.len() > indent.len() {
        out.push_str(&line);
        out.push('\n');
    }
}

/// One sorted static array, several strings per line.
fn array(out: &mut String, docs: &str, name: &str, items: &[String]) {
    let mut sorted = items.to_vec();
    sorted.sort();
    sorted.dedup();
    out.push('\n');
    out.push_str(docs);
    out.push('\n');
    out.push_str("#[rustfmt::skip]\n");
    let _ = writeln!(out, "pub(crate) static {name}: &[&str] = &[");
    let quoted: Vec<String> = sorted.iter().map(|item| format!("{item:?},")).collect();
    pack(out, "   ", &quoted);
    out.push_str("];\n");
}

#[cfg(test)]
mod tests;
