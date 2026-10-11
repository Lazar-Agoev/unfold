//! Project configuration: each TypeScript file's nearest `tsconfig.json`, its `extends` chain,
//! and the compiler options the chain sets (docs/languages/typescript.md#project-configuration).
//!
//! - **Nearest.** A file uses the `tsconfig.json` in its folder or the closest ancestor folder
//!   inside the open folder (other `tsconfig*.json` files only matter when extended). A file
//!   with none uses ESNext and NodeNext, as the proof of concept did.
//! - **Extends.** `extends` is a string or an array; later entries override earlier ones, and the
//!   file's own options override them all. A relative or `./`-style path names a file (`.json`
//!   is added when the path itself doesn't exist); anything else names a package, looked up in
//!   `node_modules` from the configuration's folder upward: the exact file, then with `.json`,
//!   then the package's `"tsconfig"` field, then its `tsconfig.json`. The engine reads these
//!   candidates on request ([`ConfigSet::requests`]), so a chain into an installed package works
//!   without ever walking `node_modules`. A base that can't be found, a cycle, or a file that
//!   doesn't parse is an input problem ([`ConfigSet::problems`]); the configuration carries on
//!   without it, never fails the update.
//! - **Paths.** Path options resolve against the configuration that wrote them, with
//!   `${configDir}` meaning the extending configuration's folder. Paths outside the open folder
//!   are dropped.
//! - **Selectors.** `files`, `include`, and `exclude` never change what is graphed; only the emit
//!   map reads them.

pub(crate) mod jsonc;
pub(crate) mod options;
pub(crate) mod packages;

use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::RepoPath;
use unfold_core::vocab::InputRole;
use unfold_engine::{InputProblem, ProjectInputs};

pub(crate) use options::{CompilerOptions, Detection, Jsx, ModuleKind, Placed, Resolution, Target};
use options::{RawConfig, parse_config};
pub(crate) use packages::Packages;

use crate::paths;

/// How deep an `extends` chain may go before it is reported instead of followed.
const MAX_CHAIN: usize = 32;

/// One `tsconfig.json` with its chain merged, or the defaults for files that have none.
#[derive(Debug, Clone)]
pub(crate) struct Config {
    /// The `tsconfig.json`; `None` for the defaults.
    pub(crate) path: Option<RepoPath>,
    /// Its folder (`""` for the root, and for the defaults).
    pub(crate) dir: String,
    /// The files the chain read: the configuration itself, then each base's chain.
    pub(crate) chain: Vec<RepoPath>,
    /// The merged options.
    pub(crate) options: CompilerOptions,
    /// The merged selectors, each with the folder of the file that wrote it.
    pub(crate) files: Option<Placed<Vec<String>>>,
    pub(crate) include: Option<Placed<Vec<String>>>,
    pub(crate) exclude: Option<Placed<Vec<String>>>,
}

impl Config {
    fn defaults() -> Self {
        Self {
            path: None,
            dir: String::new(),
            chain: Vec::new(),
            options: CompilerOptions::without_config(),
            files: None,
            include: None,
            exclude: None,
        }
    }

    /// A path option resolved for this configuration (`None` when unset or outside the folder).
    pub(crate) fn path_option(&self, option: Option<&Placed>) -> Option<String> {
        option.and_then(|placed| placed.resolve(&self.dir))
    }

    /// The target-dependent binding semantics this configuration selects.
    pub(crate) fn binding_context(&self) -> BindingContext {
        let target = self.options.effective_target();
        BindingContext {
            preserve_parameters: target >= Target::ES2015,
            lower_optional_chains: target < Target::ES2020,
            lower_object_rest: target < Target::ES2017,
            standard_class_fields: self.options.use_define_for_class_fields != Some(false)
                && target >= Target::ES2022,
        }
    }
}

/// The binding semantics that depend on `target` and `useDefineForClassFields`, as the proof of
/// concept's parameter-scope context derived them: whether parameter evaluation keeps its own
/// scope, whether optional chains and object rest are lowered (which can move parameter
/// initializers into the body), and whether class fields have standard (define) semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct BindingContext {
    /// The target is ES2015 or later.
    pub preserve_parameters: bool,
    /// The target is below ES2020.
    pub lower_optional_chains: bool,
    /// The target is below ES2017.
    pub lower_object_rest: bool,
    /// `useDefineForClassFields` isn't `false` and the target is ES2022 or later.
    pub standard_class_fields: bool,
}

impl Default for BindingContext {
    /// The latest standard target, which a file without a configuration uses.
    fn default() -> Self {
        Self {
            preserve_parameters: true,
            lower_optional_chains: false,
            lower_object_rest: false,
            standard_class_fields: true,
        }
    }
}

/// A configuration file merged with its bases.
#[derive(Debug, Clone, Default)]
struct Merged {
    chain: Vec<RepoPath>,
    raw: RawConfig,
}

/// Where an `extends` entry leads.
enum Base {
    Found(RepoPath),
    /// Not among the inputs: these candidates were (or will be) requested.
    Missing {
        candidates: Vec<RepoPath>,
        package: bool,
    },
    /// It names a path outside the open folder.
    Outside,
}

/// Every configuration in the folder, parsed once per set of inputs.
#[derive(Debug)]
pub(crate) struct ConfigSet {
    /// Every `tsconfig.json`, in path order.
    configs: Vec<Arc<Config>>,
    /// The folder of each `tsconfig.json` → its index in `configs`.
    by_dir: FxHashMap<String, usize>,
    default: Arc<Config>,
    packages: Packages,
    problems: Vec<InputProblem>,
    requests: Vec<RepoPath>,
}

impl ConfigSet {
    /// Parses every `tsconfig.json` among `inputs` with its chain.
    pub(crate) fn new(inputs: &ProjectInputs) -> Self {
        let mut loader = Loader {
            inputs,
            merged: FxHashMap::default(),
            problems: Vec::new(),
            requests: FxHashSet::default(),
        };
        let mut set = Self {
            configs: Vec::new(),
            by_dir: FxHashMap::default(),
            default: Arc::new(Config::defaults()),
            packages: Packages::new(inputs),
            problems: Vec::new(),
            requests: Vec::new(),
        };
        for (path, input) in &inputs.inputs {
            if input.role != InputRole::Tsconfig || path.file_name() != "tsconfig.json" {
                continue;
            }
            let merged = loader.load(path, &mut Vec::new()).unwrap_or_default();
            let dir = path.parent().unwrap_or("").to_owned();
            let Merged { chain, raw } = merged.as_ref().clone();
            let config = Config {
                path: Some(path.clone()),
                dir: dir.clone(),
                chain,
                options: raw.options,
                files: raw.files,
                include: raw.include,
                exclude: raw.exclude,
            };
            set.by_dir.insert(dir, set.configs.len());
            set.configs.push(Arc::new(config));
        }
        loader.problems.sort();
        loader.problems.dedup();
        set.problems = loader.problems;
        let mut requests: Vec<RepoPath> = loader.requests.into_iter().collect();
        requests.sort();
        set.requests = requests;
        set
    }

    /// The configuration `path` uses: its nearest `tsconfig.json`, or the defaults.
    pub(crate) fn nearest(&self, path: &RepoPath) -> &Arc<Config> {
        paths::ancestors(paths::dir_of(path.as_str()))
            .find_map(|dir| self.by_dir.get(dir))
            .map_or(&self.default, |&index| &self.configs[index])
    }

    /// The `package.json` files.
    pub(crate) fn packages(&self) -> &Packages {
        &self.packages
    }

    /// What is wrong with the configurations, sorted.
    pub(crate) fn problems(&self) -> &[InputProblem] {
        &self.problems
    }

    /// Files that `extends` chains may name and that aren't among the inputs yet, sorted.
    pub(crate) fn requests(&self) -> &[RepoPath] {
        &self.requests
    }
}

/// Parses configuration files on demand, following and merging their chains.
struct Loader<'a> {
    inputs: &'a ProjectInputs,
    merged: FxHashMap<RepoPath, Option<Arc<Merged>>>,
    problems: Vec<InputProblem>,
    requests: FxHashSet<RepoPath>,
}

impl Loader<'_> {
    fn problem(&mut self, path: &RepoPath, message: String) {
        self.problems.push(InputProblem {
            path: path.clone(),
            message: message.into(),
        });
    }

    /// The configuration at `path` merged with its bases; `None` if it isn't an input.
    /// `stack` holds the files being loaded, to stop cycles.
    fn load(&mut self, path: &RepoPath, stack: &mut Vec<RepoPath>) -> Option<Arc<Merged>> {
        if let Some(done) = self.merged.get(path) {
            return done.clone();
        }
        let input = self.inputs.inputs.get(path)?;
        let dir = path.parent().unwrap_or("").to_owned();
        let mut complaints = Vec::new();
        let raw = match jsonc::parse(input.snapshot.text()) {
            Ok(value) => parse_config(&value, &dir, &mut complaints),
            Err(error) => {
                complaints.push(format!("the configuration is not valid JSON: {error}"));
                RawConfig::default()
            }
        };
        for complaint in complaints {
            self.problem(path, complaint);
        }
        let mut chain = vec![path.clone()];
        let mut base = RawConfig::default();
        stack.push(path.clone());
        for written in &raw.extends {
            let found = match self.find(&dir, written) {
                Base::Found(found) => found,
                Base::Missing { candidates, package } => {
                    let hint = if package {
                        " (is the package installed?)"
                    } else {
                        ""
                    };
                    self.problem(
                        path,
                        format!("`extends` names `{written}`, which was not found{hint}"),
                    );
                    self.requests.extend(candidates);
                    continue;
                }
                Base::Outside => {
                    self.problem(
                        path,
                        format!("`extends` names `{written}`, which is outside the folder"),
                    );
                    continue;
                }
            };
            if stack.contains(&found) {
                self.problem(
                    path,
                    format!("`extends` names `{written}`, which extends this file again"),
                );
                continue;
            }
            if stack.len() > MAX_CHAIN {
                self.problem(
                    path,
                    format!("the `extends` chain is more than {MAX_CHAIN} files deep"),
                );
                continue;
            }
            let Some(loaded) = self.load(&found, stack) else {
                continue;
            };
            for link in &loaded.chain {
                if !chain.contains(link) {
                    chain.push(link.clone());
                }
            }
            base = RawConfig {
                extends: Vec::new(),
                options: loaded.raw.options.clone().over(&base.options),
                files: loaded.raw.files.clone().or(base.files),
                include: loaded.raw.include.clone().or(base.include),
                exclude: loaded.raw.exclude.clone().or(base.exclude),
            };
        }
        stack.pop();
        let merged = Arc::new(Merged {
            chain,
            raw: RawConfig {
                extends: raw.extends,
                options: raw.options.over(&base.options),
                files: raw.files.or(base.files),
                include: raw.include.or(base.include),
                exclude: raw.exclude.or(base.exclude),
            },
        });
        self.merged.insert(path.clone(), Some(Arc::clone(&merged)));
        Some(merged)
    }

    /// Where `extends: written` in folder `dir` leads.
    fn find(&self, dir: &str, written: &str) -> Base {
        let normalized = written.replace('\\', "/");
        let relative =
            paths::is_absolute(written) || normalized.starts_with("./") || normalized.starts_with("../");
        let candidates: Vec<String> = if relative {
            let Some(path) = paths::join(dir, written) else {
                return Base::Outside;
            };
            let mut candidates = vec![path.clone()];
            if !path.ends_with(".json") {
                candidates.push(format!("{path}.json"));
            }
            candidates
        } else {
            self.package_candidates(dir, written)
        };
        let candidates: Vec<RepoPath> = candidates
            .iter()
            .filter_map(|candidate| RepoPath::new(candidate).ok())
            .collect();
        match candidates.iter().find(|candidate| self.is_config(candidate)) {
            Some(found) => Base::Found(found.clone()),
            None => Base::Missing {
                candidates: candidates
                    .into_iter()
                    .filter(|candidate| !self.inputs.inputs.contains_key(candidate))
                    .collect(),
                package: !relative,
            },
        }
    }

    /// Whether `path` is a readable input that is not a `package.json` pointer.
    fn is_config(&self, path: &RepoPath) -> bool {
        path.file_name() != "package.json" && self.inputs.inputs.contains_key(path)
    }

    /// The files a package `extends` may name, nearest `node_modules` first.
    fn package_candidates(&self, dir: &str, written: &str) -> Vec<String> {
        let mut candidates = Vec::new();
        for ancestor in paths::ancestors(dir) {
            let Some(base) = paths::join(&paths::child(ancestor, "node_modules"), written) else {
                continue;
            };
            candidates.push(base.clone());
            if !base.ends_with(".json") {
                candidates.push(format!("{base}.json"));
            }
            let manifest = format!("{base}/package.json");
            let field = RepoPath::new(&manifest)
                .ok()
                .and_then(|manifest| self.inputs.inputs.get(&manifest))
                .and_then(|input| packages::tsconfig_field(input.snapshot.text()));
            candidates.push(manifest);
            if let Some(target) = field.and_then(|field| paths::join(&base, &field)) {
                candidates.push(target);
            }
            candidates.push(format!("{base}/tsconfig.json"));
        }
        candidates
    }
}
