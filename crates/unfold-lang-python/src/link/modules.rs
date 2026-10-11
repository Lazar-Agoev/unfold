//! Source roots and module resolution, without an interpreter, a virtual environment, or any
//! installed package (the proof of concept's `PythonModules`).
//!
//! **Source roots** come from the file's `[[python.project]]` (`source_roots`). Without them,
//! the project folder is the configured project's root, or else the nearest folder above the
//! file that holds a `pyproject.toml`, or else the open folder; its roots are `src/` when it has
//! a src layout (a `src` folder with Python files) and the folder itself otherwise.
//!
//! **Resolution.** An absolute specifier `a.b` is looked for under every source root as the
//! package `a/b/__init__.py`, then `a/b.py`, then the namespace package folder `a/b` (a package
//! comes before a same-named module). A relative one (`..m`) needs the importing file to lie in a
//! source root deep enough for its dots, and is looked for from the file's folder. Finding more
//! than one file or folder is `ambiguous`; nothing at all is `missing` for a relative specifier
//! and external (labeled by its first part) for an absolute one. Existence comes from the
//! program's files, discovery's unindexed and context files, and, for paths discovery never
//! walked, the folder on disk.
//!
//! **Launch contexts** ([`ProjectInputs::python_contexts`]). Each launch reaches files from its
//! entry (a script, or a module with the packages it initializes) by following imports, looked
//! up with the launch's import path in order, as Python does: each dotted part is searched in
//! the folders the previous part found, the first regular package or module settles the search
//! (a package before a module within one folder), and namespace portions accumulate until
//! something settles it. A launch's script has no package, so its relative imports name nothing.
//! A file some launch reaches resolves each import under every launch that reaches it: when they
//! agree, that is the result; when they don't, the import is `ambiguous`. Files no launch reaches
//! keep their source roots. Launches with equal import paths share their lookups.

use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use unfold_core::RepoPath;
use unfold_core::vocab::{InputRole, Language};
use unfold_engine::launch::{PythonContext, PythonEntry};
use unfold_engine::{FactsSet, FileId, ProjectInputs};

/// What an import names.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ModuleValue {
    /// The specifier as resolved (dots included).
    pub(crate) specifier: Box<str>,
    /// Module files and package `__init__.py` files, sorted.
    pub(crate) paths: Vec<Box<str>>,
    /// Package and namespace folders, sorted (a launch's namespace portions in search order).
    pub(crate) directories: Vec<Box<str>>,
    /// For a module outside the repository, the first part of its specifier.
    pub(crate) external: Option<Box<str>>,
    /// A relative specifier that names nothing.
    pub(crate) missing: bool,
    /// More than one file or folder could be meant, or the launches that reach the importing
    /// file disagree.
    pub(crate) ambiguous: bool,
}

/// A shared module value.
pub(crate) type Module = Arc<ModuleValue>;

/// How a file's imports are looked up: with a set of source roots (an index into
/// [`Modules::roots`]), or in a launch context (an index into [`Modules::launch_roots`], and
/// whether the file is the launch's script).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Lookup {
    Default(u32),
    Launch { roots: u32, script: bool },
}

/// What one name is in an ordered list of folders.
enum Found {
    /// A regular package: its folder.
    Package(String),
    /// A module file.
    Module(String),
    /// Namespace portions, in search order.
    Namespace(Vec<String>),
    Nothing,
}

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
    /// Each distinct launch import path, in search order.
    launch_roots: Vec<Vec<String>>,
    /// The launch lookups of each file some launch reaches, by file ID.
    reaching: Vec<SmallVec<[Lookup; 1]>>,
    cache: FxHashMap<(Lookup, String, Box<str>), Module>,
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
            launch_roots: Vec::new(),
            reaching: vec![SmallVec::new(); program.files().len()],
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
        let mut contexts = FxHashSet::default();
        for context in &inputs.python_contexts {
            if !contexts.insert(context) {
                continue;
            }
            let roots = launch_roots(context);
            let index = match modules.launch_roots.iter().position(|known| *known == roots) {
                Some(index) => index as u32,
                None => {
                    modules.launch_roots.push(roots);
                    modules.launch_roots.len() as u32 - 1
                }
            };
            for (file, script) in modules.reached(program, context) {
                let lookup = Lookup::Launch { roots: index, script };
                if let Some(lookups) = modules.reaching.get_mut(file.index())
                    && !lookups.contains(&lookup)
                {
                    lookups.push(lookup);
                }
            }
        }
        modules
    }

    /// The indexed files a launch reaches, in breadth-first order, each with whether it is the
    /// launch's script: the entry (a package entry with its `__main__.py`, which `-m` runs), then
    /// the repository modules each import of a reached file names, with the packages it
    /// initializes on the way.
    pub(crate) fn reached(&self, program: &FactsSet, context: &PythonContext) -> Vec<(FileId, bool)> {
        let roots = launch_roots(context);
        let mut queue: Vec<FileId> = Vec::new();
        let script = match &context.entry {
            PythonEntry::Script(path) => {
                let script = self.indexed(path.as_str());
                queue.extend(script);
                script
            }
            PythonEntry::Module(module) => {
                queue.extend(self.initialized(&roots, false, "", module));
                let found = self.ordered(&roots, "", module, false);
                if let ([_], [package]) = (found.paths.as_slice(), found.directories.as_slice()) {
                    queue.extend(self.indexed(&join(package, "__main__.py")));
                }
                None
            }
        };
        // Each import's files, by whether it is written in the script, its folder (for a
        // relative import), and its text.
        let mut imported: FxHashMap<(bool, String, Box<str>), Vec<FileId>> = FxHashMap::default();
        let mut seen: FxHashSet<FileId> = FxHashSet::default();
        let mut reached = Vec::new();
        let mut next = 0;
        while let Some(&file) = queue.get(next) {
            next += 1;
            if !seen.insert(file) {
                continue;
            }
            let is_script = Some(file) == script;
            reached.push((file, is_script));
            let Some(entry) = program.file(file) else {
                continue;
            };
            let Some(facts) = entry.facts.as_deref() else {
                continue;
            };
            let path = entry.path.as_str();
            for import in facts.graph.imports() {
                let Some(specifier) = import.specifier.as_deref() else {
                    continue;
                };
                let folder = if specifier.starts_with('.') {
                    dirname(path)
                } else {
                    ""
                };
                let files = imported
                    .entry((is_script, folder.to_owned(), specifier.into()))
                    .or_insert_with(|| self.initialized(&roots, is_script, path, specifier));
                queue.extend(files.iter().copied());
            }
        }
        reached
    }

    /// The indexed files importing `specifier` from `path` runs in a launch: the package of each
    /// dotted prefix, then the module itself.
    fn initialized(&self, roots: &[String], script: bool, path: &str, specifier: &str) -> Vec<FileId> {
        let dots = specifier.len() - specifier.trim_start_matches('.').len();
        let mut prefix = specifier[..dots].to_owned();
        let mut prefixes = Vec::new();
        for part in specifier[dots..].split('.').filter(|part| !part.is_empty()) {
            if !prefix.is_empty() && !prefix.ends_with('.') {
                prefix.push('.');
            }
            prefix.push_str(part);
            prefixes.push(prefix.clone());
        }
        if prefixes.is_empty() {
            // `from . import name`: the importing file's own package.
            prefixes.push(prefix);
        }
        prefixes
            .iter()
            .flat_map(|prefix| self.ordered(roots, path, prefix, script).paths)
            .filter_map(|found| self.indexed(&found))
            .collect()
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

    pub(crate) fn is_file(&self, path: &str) -> bool {
        self.files.contains(path) || self.on_disk(path)
    }

    fn is_folder(&self, path: &str) -> bool {
        self.folders.contains(path) || self.on_disk(path)
    }

    /// The disk check, for paths under folders discovery never walks.
    fn on_disk(&self, path: &str) -> bool {
        RepoPath::new(path).is_ok_and(|path| self.inputs.exists(&path))
    }

    /// What `specifier` names when written in `file`: under every launch that reaches the file
    /// (`ambiguous` when they disagree), or else from its source roots.
    pub(crate) fn resolve(&mut self, file: FileId, path: &str, specifier: &str) -> Module {
        let launches = self.reaching.get(file.index()).cloned().unwrap_or_default();
        let Some((&first, rest)) = launches.split_first() else {
            let roots = self.file_roots.get(file.index()).copied().unwrap_or_default();
            return self.cached(Lookup::Default(roots), path, specifier);
        };
        let module = self.cached(first, path, specifier);
        let others: Vec<Module> = rest
            .iter()
            .map(|&lookup| self.cached(lookup, path, specifier))
            .collect();
        if others.iter().all(|other| *other == module) {
            return module;
        }
        let mut paths = module.paths.clone();
        let mut directories = module.directories.clone();
        for other in &others {
            paths.extend(other.paths.iter().cloned());
            directories.extend(other.directories.iter().cloned());
        }
        Arc::new(ModuleValue {
            ambiguous: true,
            ..found(specifier.into(), paths, directories)
        })
    }

    /// What `specifier` names from `path` in one lookup (cached by lookup, folder, and text).
    fn cached(&mut self, lookup: Lookup, path: &str, specifier: &str) -> Module {
        let relative = specifier.starts_with('.');
        let folder = if relative {
            dirname(path).to_owned()
        } else {
            String::new()
        };
        let key = (lookup, folder, specifier.into());
        if let Some(found) = self.cache.get(&key) {
            return Arc::clone(found);
        }
        let module = Arc::new(match lookup {
            Lookup::Default(roots) => self.compute(roots, path, specifier),
            Lookup::Launch { roots, script } => {
                let roots = self
                    .launch_roots
                    .get(roots as usize)
                    .map_or(&[][..], Vec::as_slice);
                self.ordered(roots, path, specifier, script)
            }
        });
        self.cache.insert(key, Arc::clone(&module));
        module
    }

    /// What `specifier` names from `path` in a launch context (for a file that is the launch's
    /// script when `script`), without the cache.
    pub(crate) fn launch_lookup(
        &self,
        context: &PythonContext,
        script: bool,
        path: &str,
        specifier: &str,
    ) -> ModuleValue {
        self.ordered(&launch_roots(context), path, specifier, script)
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

    fn compute(&self, roots: u32, path: &str, specifier: &str) -> ModuleValue {
        let roots = self.roots.get(roots as usize).map_or(&[][..], Vec::as_slice);
        let dots = specifier.len() - specifier.trim_start_matches('.').len();
        let suffix: String = specifier[dots..]
            .split('.')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("/");
        let bases: Vec<String> = if dots > 0 {
            match package_folder(roots, path, dots) {
                Some(package) => vec![join(&package, &suffix)],
                None => return not_found(specifier),
            }
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
        not_found(specifier)
    }

    /// What `specifier` names from `path` with a launch's ordered import path: each dotted part
    /// is looked for in the folders the previous part found ([`Modules::scan`]). A relative
    /// specifier starts from the importing file's package; a launch's script has none.
    fn ordered(&self, roots: &[String], path: &str, specifier: &str, script: bool) -> ModuleValue {
        let dots = specifier.len() - specifier.trim_start_matches('.').len();
        let parts: Vec<&str> = specifier[dots..]
            .split('.')
            .filter(|part| !part.is_empty())
            .collect();
        let mut search = if dots > 0 {
            match package_folder(roots, path, dots).filter(|_| !script) {
                Some(package) => vec![package],
                None => return not_found(specifier),
            }
        } else {
            roots.to_vec()
        };
        if parts.is_empty() {
            // `from . import name` names the package itself.
            let Some(package) = search.pop().filter(|_| dots > 0) else {
                return not_found(specifier);
            };
            let init = join(&package, "__init__.py");
            let paths = if self.is_file(&init) {
                vec![init.into()]
            } else {
                Vec::new()
            };
            return found(specifier.into(), paths, vec![package.into()]);
        }
        for (index, part) in parts.iter().enumerate() {
            let last = index + 1 == parts.len();
            match self.scan(&search, part) {
                Found::Package(package) if last => {
                    let init = join(&package, "__init__.py");
                    return found(specifier.into(), vec![init.into()], vec![package.into()]);
                }
                Found::Package(package) => search = vec![package],
                Found::Module(file) if last => return found(specifier.into(), vec![file.into()], Vec::new()),
                Found::Namespace(portions) if last => {
                    return ModuleValue {
                        specifier: specifier.into(),
                        paths: Vec::new(),
                        directories: portions.into_iter().map(Into::into).collect(),
                        external: None,
                        missing: false,
                        ambiguous: false,
                    };
                }
                Found::Namespace(portions) => search = portions,
                // A module has no submodules.
                Found::Module(_) | Found::Nothing => break,
            }
        }
        not_found(specifier)
    }

    /// What `name` is in `folders`, scanned in order: the first regular package or module
    /// settles it (a package before a same-named module in one folder); until then, every
    /// folder named `name` is a namespace portion.
    fn scan(&self, folders: &[String], name: &str) -> Found {
        let mut portions = Vec::new();
        for folder in folders {
            let base = join(folder, name);
            if self.is_file(&join(&base, "__init__.py")) {
                return Found::Package(base);
            }
            let file = format!("{base}.py");
            if self.is_file(&file) {
                return Found::Module(file);
            }
            if self.is_folder(&base) {
                portions.push(base);
            }
        }
        if portions.is_empty() {
            Found::Nothing
        } else {
            Found::Namespace(portions)
        }
    }

    /// Looks for the module at `base`: a package, a file, or a namespace folder.
    fn look(&self, base: &str, paths: &mut Vec<Box<str>>, directories: &mut Vec<Box<str>>) {
        if base.is_empty() {
            return;
        }
        let file = format!("{base}.py");
        let init = join(base, "__init__.py");
        if self.is_file(&init) {
            paths.push(init.into());
            directories.push(base.into());
        } else if self.is_file(&file) {
            paths.push(file.into());
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

/// A launch's import path, as folders (`""` for the open folder).
fn launch_roots(context: &PythonContext) -> Vec<String> {
    context
        .roots
        .iter()
        .map(|root| folder_text(root.as_str()))
        .collect()
}

/// The package folder a relative import with `dots` dots starts from in the file at `path`:
/// the file needs a containing package, and the import may never climb out of its roots.
fn package_folder(roots: &[String], path: &str, dots: usize) -> Option<String> {
    let parent = dirname(path);
    let depths: Vec<usize> = roots
        .iter()
        .filter_map(|root| depth_below(root, parent))
        .collect();
    if depths.is_empty() || depths.iter().any(|&depth| dots > depth) {
        return None;
    }
    let mut base = parent;
    for _ in 1..dots {
        base = dirname(base);
    }
    Some(base.to_owned())
}

/// Nothing found: `missing` for a relative specifier, external (labeled by its first part) for
/// an absolute one.
fn not_found(specifier: &str) -> ModuleValue {
    let relative = specifier.starts_with('.');
    ModuleValue {
        specifier: specifier.into(),
        paths: Vec::new(),
        directories: Vec::new(),
        external: (!relative).then(|| specifier.split('.').next().unwrap_or(specifier).into()),
        missing: relative,
        ambiguous: false,
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

    /// What `specifier` names from `path` with the ordered `roots`, as its files and folders.
    fn ordered(files: &[&str], roots: &[&str], path: &str, specifier: &str, script: bool) -> String {
        let files: Vec<unfold_engine::ProgramFile> = files
            .iter()
            .map(|path| unfold_engine::ProgramFile {
                path: RepoPath::new(path).expect("a path"),
                language: Language::Python,
                facts: None,
            })
            .collect();
        let program = FactsSet::new(files).expect("distinct paths");
        let inputs = ProjectInputs::default();
        let modules = Modules::new(&program, &inputs);
        let roots: Vec<String> = roots.iter().map(|root| (*root).to_owned()).collect();
        let found = modules.ordered(&roots, path, specifier, script);
        if found.missing {
            return "missing".to_owned();
        }
        if let Some(external) = &found.external {
            return format!("external {external}");
        }
        format!("{:?} {:?}", found.paths, found.directories)
    }

    #[test]
    fn ordered_lookup_settles_on_the_first_package_or_module() {
        let files = [
            "a/tools.py",
            "a/tools/__init__.py",
            "b/tools.py",
            "a/ns/one.py",
            "b/ns/two.py",
            "a/mixed/portion.py",
            "b/mixed/__init__.py",
            "a/plain.py",
            "a/plain/child.py",
            "a/pkg/__init__.py",
            "a/pkg/run.py",
        ];
        let roots = ["a", "b"];
        // In one folder a package beats a module, and the first root settles the search.
        assert_eq!(
            ordered(&files, &roots, "", "tools", false),
            r#"["a/tools/__init__.py"] ["a/tools"]"#
        );
        // Namespace portions accumulate across roots, in search order.
        assert_eq!(ordered(&files, &roots, "", "ns", false), r#"[] ["a/ns", "b/ns"]"#);
        assert_eq!(
            ordered(&files, &roots, "", "ns.two", false),
            r#"["b/ns/two.py"] []"#
        );
        // A later regular package settles over earlier portions.
        assert_eq!(
            ordered(&files, &roots, "", "mixed", false),
            r#"["b/mixed/__init__.py"] ["b/mixed"]"#
        );
        // A module has no submodules, whatever folder sits beside it.
        assert_eq!(
            ordered(&files, &roots, "", "plain.child", false),
            "external plain"
        );
        // A relative import starts from the file's package, unless the file is the script.
        assert_eq!(
            ordered(&files, &roots, "a/pkg/run.py", ".", false),
            r#"["a/pkg/__init__.py"] ["a/pkg"]"#
        );
        assert_eq!(ordered(&files, &roots, "a/pkg/run.py", ".", true), "missing");
    }
}
