//! `cargo xtask gen-lib-tables <lib-dir> --node-types <dir> [--check]`: the TypeScript front end's
//! generated tables of known globals, library receiver types, and Node built-in modules.
//!
//! The tables are read from declaration files at a pinned version, parsed with oxc
//! ([`scan`]), never from a running compiler: every `lib.*.d.ts` of a TypeScript package
//! (the proof of concept used TypeScript 6.0.3's `built/local`), plus `@types/node`'s `index.d.ts`
//! and every file it reaches through `/// <reference path="…" />`. The result is written to
//! `crates/unfold-lang-typescript/src/tables/generated.rs` as sorted static arrays, with the
//! source versions and a SHA-256 of the declaration set in its header. With `--check`, the file
//! is compared instead of written.

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
    /// The `@types/node` package folder.
    #[arg(long)]
    node_types: PathBuf,
    /// Compare the generated file with the checked-in one instead of writing it.
    #[arg(long)]
    check: bool,
}

/// Where the tables are checked in.
fn output_path() -> PathBuf {
    repo::root().join("crates/unfold-lang-typescript/src/tables/generated.rs")
}

/// Generates the tables and writes or checks them.
pub(crate) fn run(args: &Args) -> anyhow::Result<()> {
    let sources = Sources::read(&args.lib_dir, &args.node_types)?;
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

impl Sources {
    fn read(lib_dir: &Path, node_dir: &Path) -> anyhow::Result<Self> {
        let mut libraries = Vec::new();
        let entries =
            std::fs::read_dir(lib_dir).with_context(|| format!("cannot read {}", lib_dir.display()))?;
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
        for source in &self.libraries {
            model.add_file(&source.name, &source.text)?;
        }
        for source in &self.node {
            model.add_file(&source.name, &source.text)?;
            node_model.add_file(&source.name, &source.text)?;
        }
        let builtins: Vec<String> = node_model.ambient_modules.into_iter().collect();
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
        ))
    }

    /// SHA-256 over every file, sorted by name: its name, NUL, its bytes, NUL.
    fn digest(&self) -> String {
        let mut files: Vec<&Source> = self.libraries.iter().chain(&self.node).collect();
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

/// The widest generated line.
const WIDTH: usize = 100;

fn render(header: &Header<'_>, globals: &[String], library_types: &[String], builtins: &[String]) -> String {
    let mut out = String::new();
    out.push_str("//! Generated by `cargo xtask gen-lib-tables`; do not edit by hand.\n");
    let _ = writeln!(
        out,
        "//! Sources: TypeScript {} (`lib.*.d.ts`, {} files) and @types/node {}.",
        header.typescript_version, header.library_files, header.node_version
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
    out
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
    let mut line = String::from("   ");
    for item in &sorted {
        let quoted = format!(" {item:?},");
        if line.len() + quoted.len() > WIDTH && line.len() > 3 {
            out.push_str(&line);
            out.push('\n');
            line = String::from("   ");
        }
        line.push_str(&quoted);
    }
    if line.len() > 3 {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("];\n");
}

#[cfg(test)]
mod tests;
