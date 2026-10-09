//! The `package.json` files in the folder, as module resolution reads them: a package's name
//! (so in-repository packages resolve by name), its `"type"` (a `.ts` file's module format under
//! Node resolution), and whether it declares `exports`.

use rustc_hash::FxHashMap;
use unfold_core::RepoPath;
use unfold_core::vocab::InputRole;
use unfold_engine::ProjectInputs;

use crate::config::jsonc;
use crate::paths;

/// One `package.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Package {
    /// The folder holding it.
    pub(crate) dir: String,
    /// Its `name`, if it has one.
    pub(crate) name: Option<String>,
    /// `"type": "module"`.
    pub(crate) module: bool,
    /// Whether it has an `exports` field.
    pub(crate) exports: bool,
}

/// Every discovered `package.json` outside `node_modules`.
#[derive(Debug, Clone, Default)]
pub(crate) struct Packages {
    by_dir: FxHashMap<String, Package>,
    /// Package name → the folder of the first package (in path order) with that name.
    by_name: FxHashMap<String, String>,
}

impl Packages {
    /// The packages among `inputs`. A `package.json` that doesn't parse counts as a package
    /// with no fields.
    pub(crate) fn new(inputs: &ProjectInputs) -> Self {
        let mut packages = Self::default();
        for (path, input) in &inputs.inputs {
            if input.role != InputRole::Package || path.file_name() != "package.json" {
                continue;
            }
            let package = parse(path, input.snapshot.text());
            if let Some(name) = &package.name {
                packages
                    .by_name
                    .entry(name.clone())
                    .or_insert_with(|| package.dir.clone());
            }
            packages.by_dir.insert(package.dir.clone(), package);
        }
        packages
    }

    /// The nearest package at or above folder `dir`.
    pub(crate) fn scope(&self, dir: &str) -> Option<&Package> {
        paths::ancestors(dir).find_map(|dir| self.by_dir.get(dir))
    }

    /// The in-repository package named `name`.
    pub(crate) fn named(&self, name: &str) -> Option<&Package> {
        self.by_name.get(name).and_then(|dir| self.by_dir.get(dir))
    }
}

fn parse(path: &RepoPath, text: &str) -> Package {
    let value = jsonc::parse(text).unwrap_or_default();
    let field = |name: &str| value.get(name).and_then(|value| value.as_str());
    Package {
        dir: path.parent().unwrap_or("").to_owned(),
        name: field("name").filter(|name| !name.is_empty()).map(str::to_owned),
        module: field("type") == Some("module"),
        exports: value.get("exports").is_some_and(|exports| !exports.is_null()),
    }
}

/// The `"tsconfig"` field of a package's `package.json` text, which an `extends` naming the
/// package follows.
pub(crate) fn tsconfig_field(text: &str) -> Option<String> {
    let value = jsonc::parse(text).ok()?;
    value.get("tsconfig")?.as_str().map(str::to_owned)
}
