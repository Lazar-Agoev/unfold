//! `cargo xtask notices`: writes `THIRD_PARTY_NOTICES.md` (desktop/README.md#packaging, migration Step 11),
//! which ships inside the app. It covers:
//! - the Rust crates linked into the engine addon (`unfold-node` and its normal dependencies, on
//!   every platform), from `cargo metadata`;
//! - the npm production dependencies of `desktop` and `ui`, which are bundled into the app, from
//!   `pnpm licenses list` (type-only `@types/*` packages are left out);
//! - explicit notices for Electron and Chromium, oxc, Ruff, napi-rs, and the TypeScript `lib` files
//!   the generated tables come from.
//!
//! Each package's own license files are reproduced, identical texts once. A package without one gets
//! the standard text of its license. The output holds no paths or dates, so it is reproducible;
//! `--check` fails when the committed file is stale.

mod texts;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::repo;

/// The file this command writes, relative to the repository root.
const OUTPUT: &str = "THIRD_PARTY_NOTICES.md";
/// The crate whose dependency tree ships.
const SHIPPED_CRATE: &str = "unfold-node";
/// The npm workspace packages whose production dependencies ship.
const SHIPPED_PACKAGES: &[&str] = &["@unfold/desktop", "@unfold/ui"];
/// License files are read up to this size; nothing legitimate is larger.
const MAX_LICENSE_BYTES: u64 = 512 * 1024;

/// Options for `notices`.
#[derive(clap::Args)]
pub(crate) struct Args {
    /// Fail if the notices file is out of date, instead of writing it.
    #[arg(long)]
    check: bool,
}

/// One third-party package and the license texts it ships.
struct Component {
    name: String,
    version: String,
    license: String,
    source: Option<String>,
    texts: Vec<String>,
}

pub(crate) fn run(args: &Args) -> Result<()> {
    let root = repo::root();
    let crates = rust_components(root)?;
    let packages = npm_components(root)?;
    let typescript = read_license(&root.join("node_modules/typescript/LICENSE.txt"))
        .context("TypeScript's LICENSE.txt is missing: run `pnpm install` first")?;
    let text = render(&crates, &packages, &typescript);

    if repo::check_or_write(&root.join(OUTPUT), &text, args.check, "cargo xtask notices")? {
        println!("({} crates, {} npm packages)", crates.len(), packages.len());
    }
    Ok(())
}

// --- Rust crates -------------------------------------------------------------------------------

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<MetadataPackage>,
    resolve: Resolve,
}

#[derive(Deserialize)]
struct MetadataPackage {
    id: String,
    name: String,
    version: String,
    license: Option<String>,
    license_file: Option<String>,
    repository: Option<String>,
    authors: Vec<String>,
    manifest_path: PathBuf,
    /// `None` for workspace members.
    source: Option<String>,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<ResolveNode>,
}

#[derive(Deserialize)]
struct ResolveNode {
    id: String,
    deps: Vec<NodeDep>,
}

#[derive(Deserialize)]
struct NodeDep {
    pkg: String,
    dep_kinds: Vec<DepKind>,
}

#[derive(Deserialize)]
struct DepKind {
    /// `None` for a normal dependency, `"build"` or `"dev"` otherwise.
    kind: Option<String>,
}

fn rust_components(root: &Path) -> Result<Vec<Component>> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(root)
        .output()
        .context("running cargo metadata")?;
    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let metadata: Metadata = serde_json::from_slice(&output.stdout).context("reading cargo metadata")?;
    let packages: BTreeMap<&str, &MetadataPackage> = metadata
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect();
    let nodes: BTreeMap<&str, &ResolveNode> = metadata
        .resolve
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let start = metadata
        .packages
        .iter()
        .find(|package| package.name == SHIPPED_CRATE && package.source.is_none())
        .with_context(|| format!("{SHIPPED_CRATE} is not in the workspace"))?;

    // Normal dependencies only: build scripts, proc-macro hosts' dev tools, and tests don't ship.
    let mut shipped = BTreeSet::new();
    let mut stack = vec![start.id.as_str()];
    while let Some(id) = stack.pop() {
        if !shipped.insert(id) {
            continue;
        }
        if let Some(node) = nodes.get(id) {
            for dep in &node.deps {
                if dep.dep_kinds.iter().any(|kind| kind.kind.is_none()) {
                    stack.push(dep.pkg.as_str());
                }
            }
        }
    }

    let mut components = Vec::new();
    for id in shipped {
        let Some(package) = packages.get(id) else {
            continue;
        };
        if package.source.is_none() {
            continue; // Unfold's own crates.
        }
        let license = package
            .license
            .clone()
            .unwrap_or_else(|| "NOASSERTION".to_owned());
        let dir = package.manifest_path.parent().unwrap_or(Path::new("."));
        let mut texts = license_texts(dir)?;
        if let Some(file) = &package.license_file {
            texts.extend(read_license(&dir.join(file)));
        }
        if texts.is_empty() {
            // Git dependencies (Ruff) keep the license at their repository root.
            texts = workspace_root_license(dir)?;
        }
        if texts.is_empty() && offers_mit(&license) {
            texts.push(standard_mit(&package.name, &package.authors));
        }
        texts.sort();
        texts.dedup();
        components.push(Component {
            name: package.name.clone(),
            version: package.version.clone(),
            license,
            source: package.repository.clone(),
            texts,
        });
    }
    components.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    Ok(components)
}

/// The license files of the Cargo workspace a crate sits in, up to three folders above it.
fn workspace_root_license(dir: &Path) -> Result<Vec<String>> {
    for ancestor in dir.ancestors().skip(1).take(3) {
        let manifest = ancestor.join("Cargo.toml");
        let is_workspace = std::fs::read_to_string(&manifest)
            .is_ok_and(|text| text.lines().any(|line| line.trim() == "[workspace]"));
        if is_workspace {
            return license_texts(ancestor);
        }
    }
    Ok(Vec::new())
}

/// Whether a license expression is MIT or lets the recipient choose MIT.
fn offers_mit(license: &str) -> bool {
    license
        .trim_matches(|c| c == '(' || c == ')')
        .split(" OR ")
        .any(|choice| choice.trim() == "MIT")
}

/// The MIT text for a package that ships no license file, with its known or declared copyright.
fn standard_mit(name: &str, authors: &[String]) -> String {
    let copyright = texts::MIT_COPYRIGHTS
        .iter()
        .find(|(prefix, _)| name.starts_with(prefix))
        .map(|(_, line)| (*line).to_owned())
        .unwrap_or_else(|| {
            // Names only: the e-mail addresses in crate metadata don't belong in the app.
            let names: Vec<&str> = authors
                .iter()
                .map(|author| author.split('<').next().unwrap_or(author).trim())
                .filter(|author| !author.is_empty())
                .collect();
            if names.is_empty() {
                format!("Copyright (c) the {name} authors")
            } else {
                format!("Copyright (c) {}", names.join(", "))
            }
        });
    format!("MIT License\n\n{copyright}\n\n{}", texts::MIT_BODY)
}

// --- npm packages ------------------------------------------------------------------------------

#[derive(Deserialize)]
struct NpmPackage {
    name: String,
    versions: Vec<String>,
    paths: Vec<PathBuf>,
    license: String,
    author: Option<String>,
    homepage: Option<String>,
}

fn npm_components(root: &Path) -> Result<Vec<Component>> {
    let pnpm = if cfg!(windows) { "pnpm.cmd" } else { "pnpm" };
    let mut command = Command::new(pnpm);
    command.args(["licenses", "list", "--json", "--prod"]);
    for package in SHIPPED_PACKAGES {
        command.args(["--filter", package]);
    }
    let output = command
        .current_dir(root)
        .output()
        .context("running pnpm licenses list (is pnpm installed?)")?;
    if !output.status.success() {
        bail!(
            "pnpm licenses list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let by_license: BTreeMap<String, Vec<NpmPackage>> =
        serde_json::from_slice(&output.stdout).context("reading pnpm licenses list")?;

    let mut components = Vec::new();
    for package in by_license.into_values().flatten() {
        if package.name.starts_with("@types/") {
            continue; // Type declarations only; nothing of them ships.
        }
        for (version, path) in package.versions.iter().zip(&package.paths) {
            let mut texts = license_texts(path)?;
            if texts.is_empty() && offers_mit(&package.license) {
                let authors: Vec<String> = package.author.iter().cloned().collect();
                texts.push(standard_mit(&package.name, &authors));
            }
            texts.sort();
            texts.dedup();
            components.push(Component {
                name: package.name.clone(),
                version: version.clone(),
                license: package.license.clone(),
                source: package.homepage.clone(),
                texts,
            });
        }
    }
    components.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    Ok(components)
}

// --- License files -----------------------------------------------------------------------------

/// Whether a file name looks like a license or notice file.
fn is_license_file(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "LICENSE",
        "LICENCE",
        "COPYING",
        "NOTICE",
        "COPYRIGHT",
        "UNLICENSE",
    ]
    .iter()
    .any(|prefix| upper.starts_with(prefix))
}

/// The license files directly inside `dir`, normalized.
fn license_texts(dir: &Path) -> Result<Vec<String>> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(Vec::new());
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("listing {}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_license_file(&name) && entry.file_type().is_ok_and(|kind| kind.is_file()) {
            names.push(name);
        }
    }
    names.sort();
    Ok(names
        .iter()
        .filter_map(|name| read_license(&dir.join(name)))
        .collect())
}

/// A license file's text with line endings and trailing spaces normalized, if it is readable.
fn read_license(path: &Path) -> Option<String> {
    let size = std::fs::metadata(path).ok()?.len();
    if size > MAX_LICENSE_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let normalized: Vec<&str> = text.lines().map(str::trim_end).collect();
    let joined = normalized.join("\n");
    let trimmed = joined.trim_matches('\n');
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

// --- Output ------------------------------------------------------------------------------------

fn render(crates: &[Component], packages: &[Component], typescript_license: &str) -> String {
    let mut out = String::new();
    out.push_str(
        "# Third-party notices\n\n\
         Unfold includes the open-source software listed here. This file is generated by \
         `cargo xtask notices`; don't edit it by hand.\n\n",
    );
    out.push_str(
        &texts::EXPLICIT
            .replace("{mit_body}", texts::MIT_BODY)
            .replace("{typescript_license}", typescript_license),
    );

    // Identical texts are printed once and numbered; tables point at their numbers.
    let mut numbers: BTreeMap<&str, usize> = BTreeMap::new();
    let mut ordered: Vec<(&str, Vec<String>)> = Vec::new();
    for component in crates.iter().chain(packages) {
        for text in &component.texts {
            let number = *numbers.entry(text.as_str()).or_insert_with(|| {
                ordered.push((text.as_str(), Vec::new()));
                ordered.len()
            });
            if let Some((_, users)) = ordered.get_mut(number - 1) {
                users.push(format!("{} {}", component.name, component.version));
            }
        }
    }

    for (title, components) in [
        ("Rust crates (the engine)", crates),
        ("npm packages (the app's main process and UI)", packages),
    ] {
        let _ = write!(
            out,
            "\n## {title}\n\n| Package | Version | License | Source | License text |\n| --- | --- | --- | --- | --- |\n"
        );
        for component in components {
            let refs: Vec<String> = component
                .texts
                .iter()
                .filter_map(|text| numbers.get(text.as_str()))
                .map(|number| format!("[{number}](#license-text-{number})"))
                .collect();
            let refs = if refs.is_empty() {
                "standard text of its license".to_owned()
            } else {
                refs.join(", ")
            };
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} |",
                component.name,
                component.version,
                component.license.replace('|', "\\|"),
                component.source.as_deref().unwrap_or(""),
                refs
            );
        }
    }

    out.push_str("\n## License texts\n");
    for (index, (text, users)) in ordered.iter().enumerate() {
        let number = index + 1;
        let fence = if text.contains("```") { "~~~~" } else { "```" };
        let _ = write!(
            out,
            "\n### License text {number}\n\n<a id=\"license-text-{number}\"></a>Used by: {}\n\n{fence}text\n{text}\n{fence}\n",
            users.join(", ")
        );
    }
    out
}
