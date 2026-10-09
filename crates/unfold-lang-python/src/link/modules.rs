//! Source roots and module resolution, without an interpreter, a virtual environment, or any
//! installed package (the proof of concept's `PythonModules`).
//!
//! **Source roots** come from the file's `[[python.project]]` (`source_roots`). Without them,
//! the project folder is the configured project's root, or else the nearest folder above the
//! file that holds a `pyproject.toml`, or else the open folder; its roots are `src/` when it has
//! a src layout (a `src` folder with Python files) and the folder itself otherwise.
//!
//! **Resolution.** An absolute specifier `a.b` is looked for under every source root as `a/b.py`,
//! then the package `a/b/__init__.py`, then the namespace package folder `a/b`. A relative one
//! (`..m`) needs the importing file to lie in a source root deep enough for its dots, and is
//! looked for from the file's folder. Finding more than one file or folder is `ambiguous`;
//! nothing at all is `missing` for a relative specifier and external (labeled by its first part)
//! for an absolute one. Existence comes from the program's files, discovery's unindexed and
//! context files, and, for paths discovery never walked, the folder on disk.

use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use unfold_core::RepoPath;
use unfold_core::vocab::{InputRole, Language};
use unfold_engine::{FactsSet, FileId, ProjectInputs};

/// What an import names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ModuleValue {
    /// The specifier as resolved (dots included).
    pub(crate) specifier: Box<str>,
    /// Module files and package `__init__.py` files, sorted.
    pub(crate) paths: Vec<Box<str>>,
    /// Package and namespace folders, sorted.
    pub(crate) directories: Vec<Box<str>>,
    /// For a module outside the repository, the first part of its specifier.
    pub(crate) external: Option<Box<str>>,
    /// A relative specifier that names nothing.
    pub(crate) missing: bool,
    /// More than one file or folder could be meant.
    pub(crate) ambiguous: bool,
}

/// A shared module value.
pub(crate) type Module = Arc<ModuleValue>;

/// The program's files and folders, and each Python file's source roots.
pub(crate) struct Modules<'a> {
    inputs: &'a ProjectInputs,
    /// Parsed Python files, by path.
    indexed: FxHashMap<&'a str, FileId>,
    /// Every file discovery knows: program files, unindexed and context files, inputs.
    files: FxHashSet<&'a str>,
    /// Every folder that holds a known file.
    folders: FxHashSet<String>,
    /// Folders that hold Python files.
    python_folders: FxHashSet<String>,
    /// Folders that hold a `pyproject.toml`.
    pyprojects: FxHashSet<String>,
    /// Each distinct set of source roots.
    roots: Vec<Vec<String>>,
    /// Each file's roots (an index into `roots`), by file ID.
    file_roots: Vec<u32>,
    cache: FxHashMap<(u32, String, Box<str>), Module>,
}

/// The parent folder of a path (`""` for the open folder).
pub(crate) fn dirname(path: &str) -> &str {
    path.rfind('/').map_or("", |at| &path[..at])
}

/// `folder/child`, where `""` is the open folder.
pub(crate) fn join(folder: &str, child: &str) -> String {
    match (folder.is_empty(), child.is_empty()) {
        (true, _) => child.to_owned(),
        (_, true) => folder.to_owned(),
        _ => format!("{folder}/{child}"),
    }
}

impl<'a> Modules<'a> {
    /// The modules of `program` under `inputs`.
    pub(crate) fn new(program: &'a FactsSet, inputs: &'a ProjectInputs) -> Self {
        let mut modules = Self {
            inputs,
            indexed: FxHashMap::default(),
            files: FxHashSet::default(),
            folders: FxHashSet::default(),
            python_folders: FxHashSet::default(),
            pyprojects: inputs
                .inputs
                .iter()
                .filter(|(_, input)| input.role == InputRole::Pyproject)
                .map(|(path, _)| dirname(path.as_str()).to_owned())
                .collect(),
            roots: Vec::new(),
            file_roots: Vec::new(),
            cache: FxHashMap::default(),
        };
        let known = program
            .files()
            .iter()
            .map(|file| &file.path)
            .chain(&inputs.unindexed)
            .chain(&inputs.context)
            .chain(inputs.inputs.keys());
        for path in known {
            let text = path.as_str();
            modules.files.insert(text);
            let python = text.ends_with(".py");
            let mut folder = dirname(text);
            // Both sets hold every ancestor of what they hold, so the walk stops at the first
            // folder that adds nothing.
            loop {
                let new_python = python && !modules.python_folders.contains(folder);
                if new_python {
                    modules.python_folders.insert(folder.to_owned());
                }
                let new_folder = !modules.folders.contains(folder);
                if new_folder {
                    modules.folders.insert(folder.to_owned());
                }
                if !(new_folder || new_python) || folder.is_empty() {
                    break;
                }
                folder = dirname(folder);
            }
        }
        for (id, file) in program.iter() {
            if file.language == Language::Python && file.facts.is_some() {
                modules.indexed.insert(file.path.as_str(), id);
            }
        }
        let mut sets: FxHashMap<Vec<String>, u32> = FxHashMap::default();
        for (_, file) in program.iter() {
            let roots = modules.source_roots(&file.path);
            let next = modules.roots.len() as u32;
            let id = *sets.entry(roots.clone()).or_insert(next);
            if id == next {
                modules.roots.push(roots);
            }
            modules.file_roots.push(id);
        }
        modules
    }

    /// The source roots of the Python file at `path`.
    fn source_roots(&self, path: &RepoPath) -> Vec<String> {
        let settings = &self.inputs.settings;
        let configured = settings.python_project_for(path);
        if let Some(roots) = configured.and_then(|project| project.source_roots.as_ref()) {
            return roots.iter().map(|root| folder_text(root.as_str())).collect();
        }
        let folder = match configured {
            Some(project) => folder_text(project.root.as_str()),
            None => self.pyproject_folder(path.as_str()),
        };
        let src = join(&folder, "src");
        if self.python_folders.contains(&src) {
            vec![src]
        } else {
            vec![folder]
        }
    }

    /// The nearest folder above `path` that holds a `pyproject.toml`, or the open folder.
    fn pyproject_folder(&self, path: &str) -> String {
        let mut folder = dirname(path);
        loop {
            if self.pyprojects.contains(folder) {
                return folder.to_owned();
            }
            if folder.is_empty() {
                return String::new();
            }
            folder = dirname(folder);
        }
    }

    /// The parsed Python file at `path`.
    pub(crate) fn indexed(&self, path: &str) -> Option<FileId> {
        self.indexed.get(path).copied()
    }

    fn is_file(&self, path: &str) -> bool {
        self.files.contains(path) || self.on_disk(path)
    }

    fn is_folder(&self, path: &str) -> bool {
        self.folders.contains(path) || self.on_disk(path)
    }

    /// The disk check, for paths under folders discovery never walks.
    fn on_disk(&self, path: &str) -> bool {
        RepoPath::new(path).is_ok_and(|path| self.inputs.exists(&path))
    }

    /// What `specifier` names when written in `file` (cached by roots, folder, and text).
    pub(crate) fn resolve(&mut self, file: FileId, path: &str, specifier: &str) -> Module {
        let roots = self.file_roots.get(file.index()).copied().unwrap_or_default();
        let relative = specifier.starts_with('.');
        let folder = if relative {
            dirname(path).to_owned()
        } else {
            String::new()
        };
        let key = (roots, folder, specifier.into());
        if let Some(found) = self.cache.get(&key) {
            return Arc::clone(found);
        }
        let module = Arc::new(self.compute(roots, path, specifier));
        self.cache.insert(key, Arc::clone(&module));
        module
    }

    /// What the absolute `specifier` names from the source roots of `file`, without the cache
    /// (for the readers of a frozen binder, such as the console-script detector).
    pub(crate) fn lookup(&self, file: FileId, specifier: &str) -> ModuleValue {
        let roots = self.file_roots.get(file.index()).copied().unwrap_or_default();
        self.compute(roots, "", specifier.trim_start_matches('.'))
    }

    /// The source-root set of a program file (equal sets share an index).
    pub(crate) fn roots_of(&self, file: FileId) -> Option<u32> {
        self.file_roots.get(file.index()).copied()
    }

    /// Whether a file or folder exists at `path` with exactly this spelling: a known file, a
    /// folder holding one, or (for paths discovery never walked) every component listed by its
    /// parent folder, so a case-insensitive disk can't turn another spelling into proof.
    pub(crate) fn exact(&self, path: &str) -> bool {
        if self.files.contains(path) || self.folders.contains(path) {
            return true;
        }
        let Some(mut folder) = self.inputs.root.clone() else {
            return false;
        };
        for component in path.split('/') {
            let listed = std::fs::read_dir(&folder).is_ok_and(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|entry| entry.file_name().to_str() == Some(component))
            });
            if !listed {
                return false;
            }
            folder.push(component);
        }
        // The last component must not lead out of the folder through a symlink.
        RepoPath::new(path).is_ok_and(|path| self.inputs.exists(&path))
    }

    /// Whether a regular file exists at `path` with exactly this spelling.
    pub(crate) fn exact_file(&self, path: &str) -> bool {
        if self.files.contains(path) {
            return true;
        }
        if self.folders.contains(path) || !self.exact(path) {
            return false;
        }
        self.inputs
            .root
            .as_ref()
            .is_some_and(|root| root.join(path).is_file())
    }

    fn compute(&self, roots: u32, path: &str, specifier: &str) -> ModuleValue {
        let roots = self.roots.get(roots as usize).map_or(&[][..], Vec::as_slice);
        let dots = specifier.len() - specifier.trim_start_matches('.').len();
        let suffix: String = specifier[dots..]
            .split('.')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("/");
        let missing = || ModuleValue {
            specifier: specifier.into(),
            paths: Vec::new(),
            directories: Vec::new(),
            external: None,
            missing: true,
            ambiguous: false,
        };
        let bases: Vec<String> = if dots > 0 {
            // A relative import needs a containing package and may never climb out of its
            // source root.
            let parent = dirname(path);
            let depths: Vec<usize> = roots
                .iter()
                .filter_map(|root| depth_below(root, parent))
                .collect();
            if depths.is_empty() || depths.iter().any(|&depth| dots > depth) {
                return missing();
            }
            let mut base = parent;
            for _ in 1..dots {
                base = dirname(base);
            }
            vec![join(base, &suffix)]
        } else {
            roots.iter().map(|root| join(root, &suffix)).collect()
        };
        let mut paths = Vec::new();
        let mut directories = Vec::new();
        for base in &bases {
            self.look(base, &mut paths, &mut directories);
        }
        if !paths.is_empty() || !directories.is_empty() {
            return found(specifier.into(), paths, directories);
        }
        if dots > 0 {
            return missing();
        }
        ModuleValue {
            specifier: specifier.into(),
            paths: Vec::new(),
            directories: Vec::new(),
            external: Some(specifier.split('.').next().unwrap_or(specifier).into()),
            missing: false,
            ambiguous: false,
        }
    }

    /// Looks for the module at `base`: a file, a package, or a namespace folder.
    fn look(&self, base: &str, paths: &mut Vec<Box<str>>, directories: &mut Vec<Box<str>>) {
        if base.is_empty() {
            return;
        }
        let file = format!("{base}.py");
        let init = join(base, "__init__.py");
        if self.is_file(&file) {
            paths.push(file.into());
        } else if self.is_file(&init) {
            paths.push(init.into());
            directories.push(base.into());
        } else if self.is_folder(base) {
            directories.push(base.into());
        }
    }

    /// The child module `member` of a package or namespace, if one exists.
    pub(crate) fn member(&self, module: &ModuleValue, member: &str) -> Option<Module> {
        let specifier: Box<str> = format!("{}.{member}", module.specifier).into();
        if let Some(external) = &module.external {
            return Some(Arc::new(ModuleValue {
                specifier,
                paths: Vec::new(),
                directories: Vec::new(),
                external: Some(external.clone()),
                missing: false,
                ambiguous: false,
            }));
        }
        let mut paths = Vec::new();
        let mut directories = Vec::new();
        for directory in &module.directories {
            self.look(&join(directory, member), &mut paths, &mut directories);
        }
        (!paths.is_empty() || !directories.is_empty()).then(|| Arc::new(found(specifier, paths, directories)))
    }
}

/// `.` (the open folder) as `""`.
fn folder_text(folder: &str) -> String {
    if folder == "." {
        String::new()
    } else {
        folder.to_owned()
    }
}

/// How many folders below `root` the folder `folder` is, if it lies in it.
fn depth_below(root: &str, folder: &str) -> Option<usize> {
    if root.is_empty() {
        return Some(if folder.is_empty() {
            0
        } else {
            folder.split('/').count()
        });
    }
    if folder == root {
        return Some(0);
    }
    let rest = folder.strip_prefix(root)?.strip_prefix('/')?;
    Some(rest.split('/').count())
}

fn found(specifier: Box<str>, mut paths: Vec<Box<str>>, mut directories: Vec<Box<str>>) -> ModuleValue {
    paths.sort();
    paths.dedup();
    directories.sort();
    directories.dedup();
    let ambiguous = paths.len() > 1 || directories.len() > 1;
    ModuleValue {
        specifier,
        paths,
        directories,
        external: None,
        missing: false,
        ambiguous,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_join_and_climb_from_the_open_folder() {
        assert_eq!(dirname("a/b/c.py"), "a/b");
        assert_eq!(dirname("c.py"), "");
        assert_eq!(join("", "a"), "a");
        assert_eq!(join("a", ""), "a");
        assert_eq!(join("a", "b/c"), "a/b/c");
        assert_eq!(depth_below("", "a/b"), Some(2));
        assert_eq!(depth_below("src", "src"), Some(0));
        assert_eq!(depth_below("src", "src/pkg"), Some(1));
        assert_eq!(depth_below("src", "srcx/pkg"), None);
        assert_eq!(depth_below("src", "lib"), None);
    }
}
