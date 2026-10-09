//! Module resolution: where a specifier written in a TypeScript file leads (architecture §6.3,
//! "Modules"). It follows TypeScript's rules with each file's nearest configuration, over an
//! in-memory view of the folder ([`vfs`]), never the disk, the network, or installed packages:
//!
//! 1. A relative specifier (`./x`, `../x`, `.`) resolves against the file's folder.
//! 2. Otherwise, a matching `paths` pattern (an exact pattern first, then the one with the
//!    longest prefix) tries each substitution in order, against `baseUrl` when it is set and the
//!    configuration that wrote `paths` otherwise; then `baseUrl` itself; then the nearest
//!    `package.json`'s `imports` (`#x`) and a package's reference to itself; then an
//!    in-repository package with that `name` (through its `exports`, or its files).
//! 3. A file resolves by TypeScript's extension rules: `.js` names `.ts`, `.tsx`, or `.d.ts`
//!    (and `.mjs`/`.cjs` name `.mts`/`.cts`); a written `.ts` extension is used as is; and, except
//!    in ECMAScript-module mode, an extensionless path tries `.ts`, `.tsx`, `.d.ts`, a
//!    `package.json`'s `types`/`typings`/`main`, then `index`. JavaScript resolves only with
//!    `allowJs`, JSON only with `resolveJsonModule` and its extension written.
//! 4. Under Node resolution (`node16`/`nodenext`), the mode is the file's module format for an
//!    `import` or `export … from` (`.mts` is ESM, `.cts` CommonJS, a `.ts` file follows its
//!    nearest `package.json` `"type"`), CommonJS for `import x = require(…)`, and ESM for a
//!    dynamic `import()`. Package `exports` and `imports` use the conditions `types`, `node`,
//!    and `import` or `require`, plus `customConditions`; bundler resolution uses `types` and
//!    `import` or `require`; node10 ignores `exports`.
//!
//! What a resolved file is decides the outcome ([`Resolved`]). A specifier that names nothing is
//! `not_found` when it is relative, absolute, or matched a `paths` pattern, `not_indexed` when a
//! relative path names an existing file that isn't a resolvable module (such as JavaScript
//! without `allowJs`), and otherwise a package, which is `external` with its package name
//! whether or not it is installed.

mod vfs;

use std::path::PathBuf;
use std::sync::Arc;

use oxc_resolver::{EnforceExtension, ResolveOptions, ResolverGeneric};
use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::RepoPath;
use unfold_core::vocab::{InputRole, Language};
use unfold_engine::evidence::{ExternalLocation, ModuleTarget};
use unfold_engine::{FactsSet, FileId, ProjectInputs};

use crate::config::{Config, ConfigSet, Resolution};
use crate::extract::is_typescript_source;
use crate::labels;
use crate::paths;

/// How a specifier is written, which picks TypeScript's resolution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecifierKind {
    /// An `import` declaration or `export … from`.
    Import,
    /// `import x = require(…)`.
    Require,
    /// A dynamic `import(…)`.
    Dynamic,
}

/// Where a specifier leads.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Resolved {
    /// A program file with facts.
    Indexed(FileId),
    /// A program file that has never parsed, so it has no blocks: `not_indexed`.
    Failed(FileId),
    /// An existing repository file outside the program: a source excluded from the index
    /// (`excluded`), or a declaration, JavaScript, or JSON file. External, labeled with its path.
    Unindexed {
        /// The file.
        path: RepoPath,
        /// Whether it is a TypeScript source left out by the scope (rather than a file Unfold
        /// never indexes).
        excluded: bool,
    },
    /// A package, by its name: external whether or not it is installed.
    Package(Box<str>),
    /// A relative, absolute, or `paths` specifier that names nothing.
    NotFound,
    /// A relative specifier naming something that exists but isn't a resolvable module.
    NotIndexed,
}

impl Resolved {
    /// The outcome of a runtime import of this module.
    pub fn module_target(&self) -> ModuleTarget {
        match self {
            Self::Indexed(file) => ModuleTarget::Indexed(*file),
            Self::Failed(_) | Self::NotIndexed => ModuleTarget::NotIndexed,
            Self::Unindexed { path, .. } => {
                ModuleTarget::External(ExternalLocation::File(path.as_str().into()))
            }
            Self::Package(name) => ModuleTarget::External(ExternalLocation::Package(name.clone())),
            Self::NotFound => ModuleTarget::NotFound,
        }
    }

    /// The program file it leads to, if any.
    pub fn file(&self) -> Option<FileId> {
        match self {
            Self::Indexed(file) | Self::Failed(file) => Some(*file),
            _ => None,
        }
    }
}

/// The resolution mode of one lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Mode {
    /// Node resolution of an ECMAScript-module import: no extension probing, no `index`.
    NodeImport,
    /// Node resolution of a CommonJS import.
    NodeRequire,
    BundlerImport,
    BundlerRequire,
    /// node10 or classic: CommonJS without `exports`.
    Node10,
}

/// Everything that changes a resolver's options.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Flavor {
    mode: Mode,
    allow_js: bool,
    json: bool,
    conditions: Vec<String>,
}

impl Flavor {
    fn of(config: &Config, mode: Mode) -> Self {
        let options = &config.options;
        Self {
            mode,
            allow_js: options.allow_js.unwrap_or(false),
            json: options.resolve_json_module.unwrap_or(false),
            conditions: options.custom_conditions.clone().unwrap_or_default(),
        }
    }

    fn modes(config: &Config) -> &'static [Mode] {
        match config.options.resolution() {
            Resolution::NodeNext => &[Mode::NodeImport, Mode::NodeRequire],
            Resolution::Bundler => &[Mode::BundlerImport, Mode::BundlerRequire],
            Resolution::Node10 => &[Mode::Node10],
        }
    }

    fn options(&self) -> ResolveOptions {
        let mut conditions: Vec<String> = match self.mode {
            Mode::NodeImport => vec!["types".into(), "node".into(), "import".into()],
            Mode::NodeRequire => vec!["types".into(), "node".into(), "require".into()],
            Mode::BundlerImport => vec!["types".into(), "import".into()],
            Mode::BundlerRequire => vec!["types".into(), "require".into()],
            Mode::Node10 => Vec::new(),
        };
        conditions.extend(self.conditions.iter().cloned());
        let js = |extensions: &[&str], javascript: &str| {
            let mut list: Vec<String> = extensions
                .iter()
                .map(|extension| (*extension).to_owned())
                .collect();
            if self.allow_js {
                list.push(javascript.to_owned());
            }
            list
        };
        let mut extensions = vec![".ts".to_owned(), ".tsx".to_owned(), ".d.ts".to_owned()];
        if self.allow_js {
            extensions.extend([".js".to_owned(), ".jsx".to_owned()]);
        }
        let node10 = self.mode == Mode::Node10;
        ResolveOptions {
            cwd: Some(vfs::absolute("")),
            tsconfig: None,
            condition_names: conditions,
            enforce_extension: EnforceExtension::Disabled,
            exports_fields: if node10 {
                Vec::new()
            } else {
                vec![vec!["exports".into()]]
            },
            imports_fields: if node10 {
                Vec::new()
            } else {
                vec![vec!["imports".into()]]
            },
            extension_alias: vec![
                (".js".into(), js(&[".ts", ".tsx", ".d.ts"], ".js")),
                (".jsx".into(), js(&[".tsx", ".d.ts"], ".jsx")),
                (".mjs".into(), js(&[".mts", ".d.mts"], ".mjs")),
                (".cjs".into(), js(&[".cts", ".d.cts"], ".cjs")),
            ],
            extensions,
            fully_specified: self.mode == Mode::NodeImport,
            main_fields: vec!["types".into(), "typings".into(), "main".into()],
            main_files: vec!["index".into()],
            modules: vec!["node_modules".into()],
            symlinks: false,
            node_path: false,
            builtin_modules: false,
            ..ResolveOptions::default()
        }
    }

    /// Whether a resolved file is a module TypeScript accepts under these options.
    fn accepts(&self, path: &str) -> bool {
        let ends = |suffixes: &[&str]| suffixes.iter().any(|suffix| path.ends_with(suffix));
        ends(&[".ts", ".tsx", ".mts", ".cts"])
            || (self.allow_js && ends(&[".js", ".jsx", ".mjs", ".cjs"]))
            || (self.json && path.ends_with(".json"))
    }
}

/// Extensions tried when deciding whether an unresolvable relative path names something.
const PROBES: &[&str] = &[
    ".ts", ".tsx", ".d.ts", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs", ".json",
];

/// Resolves specifiers for one linked program.
pub struct ModuleResolver<'a> {
    program: &'a FactsSet,
    inputs: &'a ProjectInputs,
    configs: Arc<ConfigSet>,
    resolvers: FxHashMap<Flavor, ResolverGeneric<vfs::Vfs>>,
    /// Unindexed TypeScript sources (left out by the scope).
    excluded: FxHashSet<&'a str>,
}

impl<'a> ModuleResolver<'a> {
    /// A resolver over the program's files and the project inputs.
    pub(crate) fn new(program: &'a FactsSet, inputs: &'a ProjectInputs, configs: Arc<ConfigSet>) -> Self {
        let mut files = vfs::Vfs::default();
        for (_, file) in program.iter() {
            files.add(file.path.as_str());
        }
        for path in inputs.unindexed.iter().chain(&inputs.context) {
            files.add(path.as_str());
        }
        for (path, input) in &inputs.inputs {
            if input.role == InputRole::Package && path.file_name() == "package.json" {
                files.add_with_text(path.as_str(), Arc::from(input.snapshot.text()));
            } else {
                files.add(path.as_str());
            }
        }
        let mut flavors: Vec<Flavor> = Vec::new();
        let mut seen: FxHashSet<*const Config> = FxHashSet::default();
        for (_, file) in program.iter() {
            if file.language != Language::TypeScript {
                continue;
            }
            let config = configs.nearest(&file.path);
            if seen.insert(Arc::as_ptr(config)) {
                for &mode in Flavor::modes(config) {
                    let flavor = Flavor::of(config, mode);
                    if !flavors.contains(&flavor) {
                        flavors.push(flavor);
                    }
                }
            }
        }
        // Every flavor shares one cache of the view.
        let quiet = ResolveOptions {
            node_path: false,
            ..ResolveOptions::default()
        };
        let base = ResolverGeneric::new_with_file_system(files, quiet);
        let resolvers = flavors
            .into_iter()
            .map(|flavor| {
                let resolver = base.clone_with_options(flavor.options());
                (flavor, resolver)
            })
            .collect();
        let excluded = inputs
            .unindexed
            .iter()
            .map(RepoPath::as_str)
            .filter(|path| is_typescript_source(path))
            .collect();
        Self {
            program,
            inputs,
            configs,
            resolvers,
            excluded,
        }
    }

    /// The configurations the resolver uses.
    pub(crate) fn configs(&self) -> &ConfigSet {
        &self.configs
    }

    /// Where `specifier`, written in `file` as `kind`, leads.
    pub fn resolve(&self, file: &RepoPath, specifier: &str, kind: SpecifierKind) -> Resolved {
        let config = self.configs.nearest(file);
        let mode = self.mode(config, file, kind);
        let flavor = Flavor::of(config, mode);
        let Some(resolver) = self.resolvers.get(&flavor) else {
            return Resolved::NotFound;
        };
        let lookup = Lookup {
            resolver,
            flavor: &flavor,
        };
        let dir = paths::dir_of(file.as_str());
        if specifier.is_empty() || paths::is_absolute(specifier) {
            return Resolved::NotFound;
        }
        if is_relative(specifier) {
            if let Some(found) = lookup.in_dir(dir, specifier) {
                return self.classify(&found);
            }
            return match paths::join(dir, specifier) {
                Some(path) if self.exists_unresolved(&path) => Resolved::NotIndexed,
                _ => Resolved::NotFound,
            };
        }
        let mut aliased = false;
        let options = &config.options;
        let base_url = config.path_option(options.base_url.as_ref());
        if let Some(paths) = &options.paths
            && let Some((substitutions, star)) = best_pattern(&paths.entries, specifier)
        {
            aliased = true;
            let base = base_url
                .clone()
                .or_else(|| crate::config::options::resolve_value(".", &paths.dir, &config.dir));
            for substitution in substitutions {
                let written = substitution.replacen('*', star, 1);
                let target = match &base {
                    Some(base) if !written.starts_with("${configDir}") => paths::join(base, &written),
                    _ => crate::config::options::resolve_value(&written, &paths.dir, &config.dir),
                };
                if let Some(found) = target.and_then(|target| lookup.path(&target)) {
                    return self.classify(&found);
                }
            }
        }
        if let Some(found) = base_url
            .and_then(|base| paths::join(&base, specifier))
            .and_then(|target| lookup.path(&target))
        {
            return self.classify(&found);
        }
        if let Some(found) = lookup.in_dir(dir, specifier) {
            return self.classify(&found);
        }
        let name = labels::package_name(specifier);
        if let Some(package) = self.configs.packages().named(name) {
            let found = if package.exports {
                lookup.in_dir(&package.dir, specifier)
            } else {
                let subpath = specifier[name.len()..].trim_start_matches('/');
                paths::join(&package.dir, subpath).and_then(|target| lookup.path(&target))
            };
            if let Some(found) = found {
                return self.classify(&found);
            }
        }
        if aliased || !labels::is_labelable(specifier) {
            Resolved::NotFound
        } else {
            Resolved::Package(name.into())
        }
    }

    fn mode(&self, config: &Config, file: &RepoPath, kind: SpecifierKind) -> Mode {
        match config.options.resolution() {
            Resolution::Node10 => Mode::Node10,
            Resolution::Bundler => match kind {
                SpecifierKind::Require => Mode::BundlerRequire,
                SpecifierKind::Import | SpecifierKind::Dynamic => Mode::BundlerImport,
            },
            Resolution::NodeNext => match kind {
                SpecifierKind::Require => Mode::NodeRequire,
                SpecifierKind::Dynamic => Mode::NodeImport,
                SpecifierKind::Import => {
                    let name = file.as_str();
                    let module = name.ends_with(".mts")
                        || (!name.ends_with(".cts")
                            && self
                                .configs
                                .packages()
                                .scope(paths::dir_of(name))
                                .is_some_and(|package| package.module));
                    if module {
                        Mode::NodeImport
                    } else {
                        Mode::NodeRequire
                    }
                }
            },
        }
    }

    /// What a resolved repository file is.
    fn classify(&self, path: &str) -> Resolved {
        if let Some(file) = self.program.find(path) {
            return if self.program.facts(file).is_some() {
                Resolved::Indexed(file)
            } else {
                Resolved::Failed(file)
            };
        }
        match RepoPath::new(path) {
            Ok(repo) => Resolved::Unindexed {
                excluded: self.excluded.contains(path),
                path: repo,
            },
            Err(_) => Resolved::NotFound,
        }
    }

    /// Whether something that isn't a program file exists at `path` or a probe of it, for a
    /// relative specifier that didn't resolve.
    fn exists_unresolved(&self, path: &str) -> bool {
        let mut candidates: Vec<String> = vec![path.to_owned()];
        for base in [path.to_owned(), paths::child(path, "index")] {
            candidates.extend(PROBES.iter().map(|extension| format!("{base}{extension}")));
        }
        for (written, replacements) in [
            (".js", &[".ts", ".tsx", ".d.ts"][..]),
            (".jsx", &[".tsx", ".d.ts"][..]),
            (".mjs", &[".mts", ".d.mts"][..]),
            (".cjs", &[".cts", ".d.cts"][..]),
        ] {
            if let Some(stem) = path.strip_suffix(written) {
                candidates.extend(replacements.iter().map(|extension| format!("{stem}{extension}")));
            }
        }
        candidates.iter().any(|candidate| {
            self.program.find(candidate).is_none()
                && RepoPath::new(candidate).is_ok_and(|candidate| self.inputs.exists(&candidate))
        })
    }
}

/// One resolver and the options it was built with.
struct Lookup<'r> {
    resolver: &'r ResolverGeneric<vfs::Vfs>,
    flavor: &'r Flavor,
}

impl Lookup<'_> {
    /// `specifier` resolved from folder `dir`, if it leads to a module these options accept.
    fn in_dir(&self, dir: &str, specifier: &str) -> Option<String> {
        let resolution = self.resolver.resolve(vfs::absolute(dir), specifier).ok()?;
        let path = vfs::relative(resolution.path())?;
        self.flavor.accepts(&path).then_some(path)
    }

    /// The folder-relative `target` resolved as a path.
    fn path(&self, target: &str) -> Option<String> {
        let absolute: PathBuf = vfs::absolute(target);
        let specifier = absolute.to_str()?;
        self.in_dir("", specifier)
    }
}

/// Whether a specifier is written relative to its file.
fn is_relative(specifier: &str) -> bool {
    specifier == "." || specifier == ".." || specifier.starts_with("./") || specifier.starts_with("../")
}

/// The `paths` entry `specifier` matches: an exact pattern first, then the wildcard pattern with
/// the longest prefix. Returns its substitutions and what `*` matched.
fn best_pattern<'p, 's>(
    entries: &'p [(String, Vec<String>)],
    specifier: &'s str,
) -> Option<(&'p [String], &'s str)> {
    if let Some((_, substitutions)) = entries.iter().find(|(pattern, _)| pattern == specifier) {
        return Some((substitutions, ""));
    }
    let mut best: Option<(usize, &'p [String], &'s str)> = None;
    for (pattern, substitutions) in entries {
        let Some((prefix, suffix)) = pattern.split_once('*') else {
            continue;
        };
        if suffix.contains('*') || specifier.len() < prefix.len() + suffix.len() {
            continue;
        }
        if specifier.starts_with(prefix) && specifier.ends_with(suffix) {
            let star = &specifier[prefix.len()..specifier.len() - suffix.len()];
            if best.is_none_or(|(length, _, _)| prefix.len() > length) {
                best = Some((prefix.len(), substitutions, star));
            }
        }
    }
    best.map(|(_, substitutions, star)| (substitutions, star))
}

#[cfg(test)]
mod tests;
