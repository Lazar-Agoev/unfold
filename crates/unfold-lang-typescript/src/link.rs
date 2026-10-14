//! TypeScript linking: the module layer of the program and the answers the shared backend asks
//! for (`docs/languages/typescript.md`).
//!
//! What linking answers:
//!
//! - **Modules.** Each written module dependency's outcome ([`Linked::module_dependencies`],
//!   aligned with the file's import facts), resolved by [`ModuleResolver`] with the file's
//!   `tsconfig.json`.
//! - **Exports.** Each module's runtime exports with re-export chains followed
//!   ([`ExportTable`]), which import bindings read.
//! - **Evidence.** Callee, value, type, and annotation evidence, declaration semantics, and
//!   class shapes, computed by the `evidence` module over the files' bindings and tables built
//!   once here (script globals, class members and bases, narrowed receivers, members that may
//!   override unknown bases, and the mutation census).
//! - **Labels and inputs.** Module-label spelling, and the `tsconfig.json` chains the graph used
//!   ([`Linked::inputs_read`]).
//! - **Validation.** The independent census.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rayon::prelude::*;
use rustc_hash::FxHashMap;
use unfold_core::RepoPath;
use unfold_core::validate::Census;
use unfold_core::vocab::Language;
use unfold_engine::evidence::{
    ClassShape, ConcreteTarget, Evidence, ExternalLocation, MemberFact, ModuleTarget, OriginDeclaration,
    Stability, ValueContext,
};
use unfold_engine::{
    CallRef, DeclRef, FactsSet, FileId, LanguageId, Linked, NodeRef, ProjectInputs, TypeRef, ValueRef,
};

use crate::census::TypeScriptCensus;
use crate::config::{BindingContext, ConfigSet, Detection, Jsx, Resolution};
use crate::evidence::{GlobalSymbol, Tables, Walk, WrittenMembers};
use crate::exports::{ExportTable, ModuleFile, Specifiers};
use crate::labels;
use crate::modules::{ModuleResolver, Resolved, SpecifierKind};
use crate::payload::binding::{Alias, DeclFlags};
use crate::payload::{ImportKind, ImportSource, ModuleFacts, Payload};

/// The linked TypeScript part of a program.
pub struct TypeScriptLinked<'a> {
    program: &'a FactsSet,
    resolver: ModuleResolver<'a>,
    /// By file ID: the decoded payload of each TypeScript file with facts.
    payloads: Vec<Option<Payload>>,
    /// By file ID: whether the file is a module.
    modules: Vec<bool>,
    dependencies: Vec<Vec<ModuleTarget>>,
    /// By file ID: where each module import binding leads (aligned with the module facts'
    /// imports; `None` for qualified aliases), resolved once per link.
    import_resolutions: Vec<Vec<Option<Resolved>>>,
    /// By file ID: where each dynamic-import alias leads (aligned with the binding's aliases;
    /// `None` for other aliases).
    alias_resolutions: Vec<Vec<Option<Resolved>>>,
    exports: ExportTable,
    inputs_read: Vec<RepoPath>,
    census: TypeScriptCensus,
    /// The tables evidence reads.
    pub(crate) tables: Tables,
    /// Every class's shape, for the override census.
    shapes: Vec<ClassShape>,
}

impl<'a> TypeScriptLinked<'a> {
    /// Links the TypeScript files of `program` with the project inputs and their parsed
    /// configurations.
    pub(crate) fn new(program: &'a FactsSet, inputs: &'a ProjectInputs, configs: Arc<ConfigSet>) -> Self {
        let resolver = ModuleResolver::new(program, inputs, Arc::clone(&configs));
        let configs = &*configs;
        let mut payloads: Vec<Option<Payload>> = program
            .files()
            .par_iter()
            .map(|file| {
                let facts = file
                    .facts
                    .as_deref()
                    .filter(|_| file.language == Language::TypeScript)?;
                Payload::from_bytes(&facts.payload).ok()
            })
            .collect();
        let modules: Vec<bool> = program
            .files()
            .iter()
            .zip(&payloads)
            .map(|(file, payload)| {
                payload
                    .as_ref()
                    .is_some_and(|payload| is_module(configs, &file.path, &payload.module))
            })
            .collect();
        let dependencies: Vec<Vec<ModuleTarget>> = program
            .files()
            .par_iter()
            .zip(&payloads)
            .map(|(file, payload)| {
                let (Some(facts), Some(payload)) = (file.facts.as_deref(), payload) else {
                    return Vec::new();
                };
                facts
                    .graph
                    .imports()
                    .iter()
                    .enumerate()
                    .map(|(index, import)| {
                        let Some(specifier) = &import.specifier else {
                            return ModuleTarget::Dynamic;
                        };
                        let kind = specifier_kind(payload.imports.get(index).copied());
                        resolver.resolve(&file.path, specifier, kind).module_target()
                    })
                    .collect()
            })
            .collect();
        let (import_resolutions, alias_resolutions): (Vec<_>, Vec<_>) = program
            .files()
            .par_iter()
            .zip(&payloads)
            .map(|(file, payload)| {
                let Some(payload) = payload else {
                    return (Vec::new(), Vec::new());
                };
                let imports = payload
                    .module
                    .imports
                    .iter()
                    .map(|binding| match &binding.source {
                        ImportSource::Module { specifier, require } => {
                            Some(resolver.resolve(&file.path, specifier, import_kind(*require)))
                        }
                        ImportSource::Qualified(_) => None,
                    })
                    .collect();
                let aliases = payload
                    .binding
                    .aliases
                    .iter()
                    .map(|alias| match alias {
                        Alias::Dynamic { specifier, .. } => {
                            Some(resolver.resolve(&file.path, specifier, SpecifierKind::Dynamic))
                        }
                        _ => None,
                    })
                    .collect();
                (imports, aliases)
            })
            .unzip();
        let files: Vec<Option<ModuleFile<'_>>> = program
            .files()
            .iter()
            .zip(&payloads)
            .zip(&modules)
            .map(|((file, payload), &is_module)| {
                let payload = payload.as_ref()?;
                let config = configs.nearest(&file.path);
                let options = &config.options;
                let node_module = config.path.is_none()
                    || options.module.is_some_and(|module| {
                        module.is_node() || module == crate::config::ModuleKind::Preserve
                    });
                let symbols = (0..payload.module.declarations.len())
                    .map(|index| {
                        payload
                            .binding
                            .declarations
                            .get(index)
                            .and_then(|decl| decl.symbol)
                    })
                    .collect();
                Some(ModuleFile {
                    facts: &payload.module,
                    is_module,
                    synthetic_default: options
                        .allow_synthetic_default_imports
                        .or(options.es_module_interop)
                        .unwrap_or(node_module),
                    symbols,
                })
            })
            .collect();
        // The export tables resolve the same import specifiers again; start from these.
        let mut seeded = SpecifierCache::default();
        for (index, (payload, resolutions)) in payloads.iter().zip(&import_resolutions).enumerate() {
            let Some(payload) = payload else {
                continue;
            };
            for (binding, resolved) in payload.module.imports.iter().zip(resolutions) {
                if let (ImportSource::Module { specifier, require }, Some(resolved)) =
                    (&binding.source, resolved)
                {
                    seeded
                        .entry((FileId(index as u32), import_kind(*require)))
                        .or_default()
                        .insert(specifier.clone(), resolved.clone());
                }
            }
        }
        let cached = CachedSpecifiers {
            program,
            resolver: &resolver,
            cache: Mutex::new(seeded),
        };
        let exports = ExportTable::build(&files, &cached);
        drop(cached);
        drop(files);
        let mut inputs_read: Vec<RepoPath> = Vec::new();
        for (file, payload) in program.files().iter().zip(&payloads) {
            if payload.is_some() {
                inputs_read.extend(configs.nearest(&file.path).chain.iter().cloned());
            }
        }
        inputs_read.sort();
        inputs_read.dedup();
        // Nothing else reads a payload's census, so the census takes it rather than a copy.
        let census = TypeScriptCensus::new(program.files().iter().zip(&mut payloads).filter_map(
            |(file, payload)| Some((file.path.clone(), std::mem::take(&mut payload.as_mut()?.census))),
        ));
        let mut group_ids: FxHashMap<Option<RepoPath>, u32> = FxHashMap::default();
        let groups: Vec<u32> = program
            .files()
            .iter()
            .map(|file| {
                let key = configs.nearest(&file.path).path.clone();
                let next = group_ids.len() as u32;
                *group_ids.entry(key).or_insert(next)
            })
            .collect();
        let mut linked = Self {
            program,
            resolver,
            payloads,
            modules,
            dependencies,
            import_resolutions,
            alias_resolutions,
            exports,
            inputs_read,
            census,
            tables: Tables::default(),
            shapes: Vec::new(),
        };
        linked.link_tables(groups, group_ids.len());
        linked
    }

    /// Builds the tables evidence reads, each from the ones before it.
    fn link_tables(&mut self, groups: Vec<u32>, group_count: usize) {
        let mut file_scope = Vec::with_capacity(self.payloads.len());
        let mut globals: Vec<FxHashMap<Box<str>, GlobalSymbol>> = vec![FxHashMap::default(); group_count];
        let mut objects = FxHashMap::default();
        for (index, payload) in self.payloads.iter().enumerate() {
            let file = FileId(index as u32);
            let Some(payload) = payload else {
                file_scope.push(Vec::new());
                continue;
            };
            let binding = &payload.binding;
            let mut scope = vec![false; binding.symbols.len()];
            for &symbol in &binding.file_symbols {
                if let Some(slot) = scope.get_mut(symbol as usize) {
                    *slot = true;
                }
            }
            file_scope.push(scope);
            let script = !self.modules.get(index).copied().unwrap_or(false);
            let script_symbols = binding.file_symbols.iter().filter(|_| script);
            let group = &mut globals[groups[index] as usize];
            for &symbol in script_symbols.chain(&binding.global_symbols) {
                let Some(found) = binding.symbols.get(symbol as usize) else {
                    continue;
                };
                let global = group.entry(found.name.clone()).or_default();
                global.members.push((file, symbol));
                global.meanings = crate::payload::binding::Meanings(global.meanings.0 | found.meanings.0);
            }
            for (object, facts) in binding.objects.iter().enumerate() {
                objects.insert(
                    DeclRef {
                        file,
                        index: facts.variable,
                    },
                    object as u32,
                );
            }
        }
        self.tables.groups = groups;
        self.tables.merged_namespaces = self.merge_globals(&mut globals);
        self.tables.globals = globals;
        self.tables.file_scope = file_scope;
        self.tables.objects = objects;
        self.tables.classes = self.index_classes();
        self.tables.narrowed = self.find_narrowed();
        let mut classes: Vec<DeclRef> = self.tables.classes.keys().copied().collect();
        classes.sort_unstable();
        let bases: FxHashMap<DeclRef, crate::evidence::Base> = classes
            .par_iter()
            .map(|&class| (class, self.compute_base(&mut Walk::default(), class)))
            .collect();
        self.tables.bases = bases;
        self.tables.unknown_overrides = self.find_unknown_overrides();
        self.tables.written = self.find_writes();
        self.shapes = self.class_shapes();
    }

    /// The object members written anywhere, by object variable.
    fn find_writes(&self) -> FxHashMap<DeclRef, WrittenMembers> {
        let mut written: FxHashMap<DeclRef, WrittenMembers> = FxHashMap::default();
        for (index, payload) in self.payloads.iter().enumerate() {
            let file = FileId(index as u32);
            let Some(payload) = payload else {
                continue;
            };
            for write in &payload.binding.writes {
                let target = self.target(&mut Walk::default(), file, write.object);
                for declaration in target.declarations {
                    if self.tables.objects.contains_key(&declaration) {
                        let members = written.entry(declaration).or_default();
                        match &write.name {
                            Some(name) => {
                                members.names.insert(name.clone());
                            }
                            None => members.any = true,
                        }
                    }
                }
            }
        }
        written
    }

    /// The program this links.
    pub(crate) fn program(&self) -> &'a FactsSet {
        self.program
    }

    /// The module resolver, for specifiers written in a file.
    pub fn resolver(&self) -> &ModuleResolver<'a> {
        &self.resolver
    }

    /// Where `specifier`, written in `file` as `kind`, leads.
    pub fn resolve(&self, file: FileId, specifier: &str, kind: SpecifierKind) -> Resolved {
        match self.program.files().get(file.index()) {
            Some(entry) => self.resolver.resolve(&entry.path, specifier, kind),
            None => Resolved::NotFound,
        }
    }

    /// Where module import binding `index` of `file` leads (`None` for a qualified alias).
    pub(crate) fn import_resolution(&self, file: FileId, index: u32) -> Option<&Resolved> {
        self.import_resolutions
            .get(file.index())?
            .get(index as usize)?
            .as_ref()
    }

    /// Where dynamic-import alias `index` of `file` leads.
    pub(crate) fn alias_resolution(&self, file: FileId, index: u32) -> Option<&Resolved> {
        self.alias_resolutions
            .get(file.index())?
            .get(index as usize)?
            .as_ref()
    }

    /// The export tables and import-binding targets.
    pub fn exports(&self) -> &ExportTable {
        &self.exports
    }

    /// A TypeScript file's module facts (module-scope declarations, imports, and exports).
    pub fn module_facts(&self, file: FileId) -> Option<&ModuleFacts> {
        Some(&self.payloads.get(file.index())?.as_ref()?.module)
    }

    /// A TypeScript file's decoded payload.
    pub fn payload(&self, file: FileId) -> Option<&Payload> {
        self.payloads.get(file.index())?.as_ref()
    }

    /// The target-dependent binding semantics of a file's configuration.
    pub fn binding_context(&self, file: FileId) -> Option<BindingContext> {
        let entry = self.program.files().get(file.index())?;
        Some(self.resolver.configs().nearest(&entry.path).binding_context())
    }

    /// Whether a TypeScript file is a module (rather than a script whose top-level
    /// declarations are global).
    pub fn is_module(&self, file: FileId) -> bool {
        self.modules.get(file.index()).copied().unwrap_or(false)
    }
}

/// The resolution mode of a module import binding: `import x = require(…)` or an import.
pub(crate) fn import_kind(require: bool) -> SpecifierKind {
    if require {
        SpecifierKind::Require
    } else {
        SpecifierKind::Import
    }
}

/// How an import fact's syntax picks the resolution mode.
fn specifier_kind(kind: Option<ImportKind>) -> SpecifierKind {
    match kind {
        Some(ImportKind::ImportEquals) => SpecifierKind::Require,
        Some(ImportKind::Dynamic) => SpecifierKind::Dynamic,
        Some(ImportKind::Import | ImportKind::Reexport) | None => SpecifierKind::Import,
    }
}

/// Whether a file is a module under its configuration's module detection: always with `force`;
/// with `legacy` when it has module syntax; with `auto` also when its format is ECMAScript
/// (`.mts`, `.cts`, or a `.ts` file in a `"type": "module"` package under Node resolution) or
/// it uses JSX with an automatic runtime (`react-jsx`, `react-jsxdev`).
fn is_module(configs: &ConfigSet, path: &RepoPath, facts: &ModuleFacts) -> bool {
    let options = &configs.nearest(path).options;
    match options.detection() {
        Detection::Force => true,
        Detection::Legacy => facts.module_syntax,
        Detection::Auto => {
            let name = path.as_str();
            let format = name.ends_with(".mts")
                || name.ends_with(".cts")
                || (options.resolution() == Resolution::NodeNext
                    && configs
                        .packages()
                        .scope(crate::paths::dir_of(name))
                        .is_some_and(|package| package.module));
            let automatic_jsx = matches!(options.jsx, Some(Jsx::Automatic | Jsx::AutomaticDev)) && facts.jsx;
            facts.module_syntax || format || automatic_jsx
        }
    }
}

/// The module resolver with a cache, for building the export tables: by file and import
/// kind, each specifier's outcome.
struct CachedSpecifiers<'r, 'a> {
    program: &'a FactsSet,
    resolver: &'r ModuleResolver<'a>,
    cache: Mutex<SpecifierCache>,
}

/// By file and import kind, each specifier's outcome.
type SpecifierCache = FxHashMap<(FileId, SpecifierKind), FxHashMap<Box<str>, Resolved>>;

impl CachedSpecifiers<'_, '_> {
    /// The cache; a panic elsewhere can't leave a half-written entry, so a poisoned lock is
    /// still usable.
    fn cache(&self) -> MutexGuard<'_, SpecifierCache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Specifiers for CachedSpecifiers<'_, '_> {
    fn resolve(&self, file: FileId, specifier: &str, kind: SpecifierKind) -> Resolved {
        if let Some(found) = self
            .cache()
            .get(&(file, kind))
            .and_then(|cache| cache.get(specifier))
        {
            return found.clone();
        }
        let resolved = match self.program.files().get(file.index()) {
            Some(entry) => self.resolver.resolve(&entry.path, specifier, kind),
            None => Resolved::NotFound,
        };
        // Resolved outside the lock: two threads may resolve the same specifier, and get the
        // same outcome.
        self.cache()
            .entry((file, kind))
            .or_default()
            .insert(specifier.into(), resolved.clone());
        resolved
    }

    fn path(&self, file: FileId) -> Option<&str> {
        self.program
            .files()
            .get(file.index())
            .map(|entry| entry.path.as_str())
    }
}

impl Linked for TypeScriptLinked<'_> {
    fn language(&self) -> LanguageId {
        Language::TypeScript
    }

    fn callee_evidence(&self, call: CallRef) -> Evidence {
        match self.program.call(call) {
            Some(fact) => self.value_facts(call.file, fact.callee),
            None => Evidence::default(),
        }
    }

    fn value_evidence(&self, value: ValueRef) -> Evidence {
        self.value_facts(value.file, value.value)
    }

    fn type_evidence(&self, reference: TypeRef) -> Evidence {
        self.type_facts(reference)
    }

    /// A class without a direct constructor takes no arguments when it has no `extends` clause
    /// (its implicit constructor is empty) and no decorator (which could replace the class).
    fn class_takes_no_arguments(&self, class: NodeRef) -> bool {
        let Some(binding) = self.binding(class.file) else {
            return false;
        };
        binding.classes.iter().any(|facts| {
            facts.heritage.is_none()
                && binding.declarations.get(facts.decl as usize).is_some_and(|decl| {
                    decl.node == Some(class.local) && !decl.flags.has(DeclFlags::DECORATED)
                })
        })
    }

    fn annotation_evidence(&self, declaration: DeclRef) -> Option<Evidence> {
        self.annotation_facts(declaration)
    }

    fn concrete_target(&self, declaration: DeclRef, context: ValueContext) -> Option<ConcreteTarget> {
        self.concrete(declaration, context)
    }

    fn is_parameter(&self, declaration: DeclRef) -> bool {
        self.in_parameter(declaration)
    }

    fn binding_stability(&self, declaration: DeclRef) -> Stability {
        self.stability(declaration)
    }

    fn accessor(&self, declaration: DeclRef) -> bool {
        self.is_accessor(declaration)
    }

    fn member(&self, declaration: DeclRef) -> MemberFact<'_> {
        self.member_facts(declaration)
    }

    fn overload_of(&self, signature: DeclRef, implementation: DeclRef) -> bool {
        self.is_overload(signature, implementation)
    }

    fn origin_declaration(&self, declaration: DeclRef) -> OriginDeclaration {
        self.origin(declaration)
    }

    fn module_dependencies(&self, file: FileId) -> &[ModuleTarget] {
        self.dependencies.get(file.index()).map_or(&[], Vec::as_slice)
    }

    fn classes(&self) -> &[ClassShape] {
        &self.shapes
    }

    fn is_package_specifier(&self, _file: FileId, specifier: &str) -> bool {
        labels::is_package_specifier(specifier)
    }

    fn label_for_specifier(&self, _file: FileId, specifier: &str) -> String {
        labels::label_for_specifier(specifier)
    }

    fn label_for_location(&self, _file: FileId, location: &ExternalLocation) -> String {
        labels::label_for_location(location)
    }

    fn global_label(&self, _file: FileId) -> String {
        labels::GLOBAL.to_owned()
    }

    fn inputs_read(&self) -> Vec<RepoPath> {
        self.inputs_read.clone()
    }

    fn census(&self) -> Option<&dyn Census> {
        Some(&self.census)
    }
}
