//! Following export links: names to declarations, through imports, re-exports, stars, and
//! `export =`, with cycles cut and finished results memoized.

use std::borrow::Cow;
use std::cell::{Cell, OnceCell, RefCell};

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use unfold_engine::evidence::{Candidate, ExternalLocation, Origin};
use unfold_engine::{DeclRef, FileId};

use super::index::{Indexed, Link, index};
use super::{ExportTarget, Identity, ModuleFile, Specifiers, external_member};
use crate::modules::{Resolved, SpecifierKind};
use crate::payload::{ImportSource, ModuleFacts};

/// The `(module, name)` pairs and local names being followed, to stop cycles.
#[derive(Default)]
pub(super) struct Visiting {
    exports: FxHashSet<(FileId, Box<str>)>,
    locals: FxHashSet<(FileId, Box<str>)>,
}

pub(super) struct Builder<'f, 'i, 's> {
    /// By file ID: each TypeScript file.
    files: &'f [Option<ModuleFile<'f>>],
    /// By file ID: the indexes of each module (scripts and other files have none).
    indexes: &'i [Option<Indexed<'f>>],
    specifiers: &'s dyn Specifiers,
    /// By file ID: finished targets by name that no cycle cut short.
    memo: RefCell<Vec<FxHashMap<Box<str>, ExportTarget>>>,
    /// By file ID: the names a module can export, computed on first use.
    reach: Vec<OnceCell<Reach<'f>>>,
    /// By file ID: the modules a module's stars name (aligned with [`Builder::stars`]),
    /// resolved on first use.
    star_targets: Vec<OnceCell<Vec<ExportTarget>>>,
    /// How many cycles were cut so far, to tell whether a result depends on one.
    cuts: Cell<u64>,
    /// What names are followed for.
    mode: Mode,
}

/// What export names are followed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// The module's runtime exports (`Linked::module_exports`): type-only links and names
    /// whose declarations are all types are type-only.
    Runtime,
    /// A value, as evidence follows it (the proof of concept's callee linker): type-only links
    /// name nothing, a name's every declaration is its target, each merged symbol is its own
    /// identity, and `import * as` of a module with `export =` names what it assigns.
    Value,
    /// A written type: like [`Mode::Value`], but type-only links are followed.
    Type,
}

/// The names a module can export: its explicit names and, through stars, every name but
/// `default` and `export=` of the modules it re-exports (what [`Builder::export_names`]
/// returns).
pub(super) struct Reach<'f> {
    names: FxHashSet<&'f str>,
    /// Whether a name outside `names` can still answer: some module in the star closure has
    /// `export =` (whose value's members answer), or a star leads outside the program.
    open: bool,
}

/// Builds the indexes of every module in `files`, by file ID, for the builders of every mode.
pub(super) fn indexes<'f>(files: &'f [Option<ModuleFile<'f>>]) -> Vec<Option<Indexed<'f>>> {
    files
        .iter()
        .map(|file| {
            let file = file.as_ref().filter(|file| file.is_module)?;
            Some(index(file.facts))
        })
        .collect()
}

impl<'f, 'i, 's> Builder<'f, 'i, 's> {
    pub(super) fn new(
        files: &'f [Option<ModuleFile<'f>>],
        indexes: &'i [Option<Indexed<'f>>],
        specifiers: &'s dyn Specifiers,
        mode: Mode,
    ) -> Self {
        Self {
            files,
            indexes,
            specifiers,
            memo: RefCell::new(vec![FxHashMap::default(); files.len()]),
            reach: (0..files.len()).map(|_| OnceCell::new()).collect(),
            star_targets: (0..files.len()).map(|_| OnceCell::new()).collect(),
            cuts: Cell::new(0),
            mode,
        }
    }

    /// A module and its indexes (scripts have none).
    fn file(&self, file: FileId) -> Option<(&'f ModuleFile<'f>, &'i Indexed<'f>)> {
        let module = self.files.get(file.index())?.as_ref()?;
        let index = self.indexes.get(file.index())?.as_ref()?;
        Some((module, index))
    }

    /// Any TypeScript file's module facts.
    fn facts(&self, file: FileId) -> Option<&'f ModuleFacts> {
        Some(self.files.get(file.index())?.as_ref()?.facts)
    }

    pub(super) fn has_explicit(&self, file: FileId, name: &str) -> bool {
        self.file(file)
            .is_some_and(|(_, index)| index.explicit.contains_key(name))
    }

    /// Every export name of `file`: its explicit names and, through stars, every name but
    /// `default` and `export=` of the modules it re-exports.
    pub(super) fn export_names(&self, file: FileId) -> impl Iterator<Item = &'f str> + '_ {
        self.reach(file)
            .into_iter()
            .flat_map(|reach| reach.names.iter().copied())
    }

    /// What `file` can export, computed once per module.
    fn reach(&self, file: FileId) -> Option<&Reach<'f>> {
        let cell = self.reach.get(file.index())?;
        Some(cell.get_or_init(|| self.compute_reach(file)))
    }

    fn compute_reach(&self, file: FileId) -> Reach<'f> {
        let mut reach = Reach {
            names: FxHashSet::default(),
            open: false,
        };
        let mut visited = FxHashSet::default();
        let mut pending = vec![(file, true)];
        while let Some((current, include_default)) = pending.pop() {
            if !visited.insert(current) {
                continue;
            }
            let Some((_, index)) = self.file(current) else {
                continue;
            };
            reach.open |= index.explicit.contains_key("export=");
            for &name in index.explicit.keys() {
                if include_default || (name != "default" && name != "export=") {
                    reach.names.insert(name);
                }
            }
            for target in self.star_targets(current, index) {
                match target.namespace {
                    Some(module) => pending.push((module, false)),
                    // An external or excluded star answers any name.
                    None => reach.open |= !target.is_empty(),
                }
            }
        }
        reach
    }

    /// Whether export `name` (neither `default` nor `export=`) of `module`, followed as a star
    /// member, can be anything but empty. When it can't, following it would only produce an
    /// empty target (or cut a cycle into one), so skipping it changes no result; it saves
    /// following every name of a barrel through every module the barrel re-exports.
    fn may_export(&self, module: FileId, name: &str) -> bool {
        self.reach(module)
            .is_none_or(|reach| reach.open || reach.names.contains(name))
    }

    /// The module targets of the stars of `file` (whose indexes are `index`).
    fn star_targets(&self, file: FileId, index: &Indexed<'f>) -> &[ExportTarget] {
        match self.star_targets.get(file.index()) {
            Some(cell) => cell.get_or_init(|| {
                self.stars(index)
                    .iter()
                    .map(|specifier| self.module(file, specifier, SpecifierKind::Import))
                    .collect()
            }),
            None => &[],
        }
    }

    /// The star re-exports names are followed through.
    fn stars<'x>(&self, index: &'x Indexed<'f>) -> &'x [&'f str] {
        if self.mode == Mode::Type {
            &index.all_stars
        } else {
            &index.stars
        }
    }

    /// The target of export `name` of `file`.
    pub(super) fn export(&self, file: FileId, name: &str, visiting: &mut Visiting) -> ExportTarget {
        if let Some(done) = self
            .memo
            .borrow()
            .get(file.index())
            .and_then(|names| names.get(name))
        {
            return done.clone();
        }
        let key = (file, Box::<str>::from(name));
        if !visiting.exports.insert(key.clone()) {
            self.cuts.set(self.cuts.get() + 1);
            return ExportTarget::default();
        }
        let cuts = self.cuts.get();
        let target = self.export_uncached(file, name, visiting);
        visiting.exports.remove(&key);
        if self.cuts.get() == cuts
            && let Some(names) = self.memo.borrow_mut().get_mut(file.index())
        {
            names.insert(key.1, target.clone());
        }
        target
    }

    fn export_uncached(&self, file: FileId, name: &str, visiting: &mut Visiting) -> ExportTarget {
        let Some((module, index)) = self.file(file) else {
            return ExportTarget::default();
        };
        let mut targets = Vec::new();
        let links = index.explicit.get(name);
        match links {
            Some(links) => {
                for link in links {
                    targets.push(match link {
                        Link::Declared(local) => self.local(file, local, visiting),
                        Link::Local { local, type_only } => match self.skip(*type_only) {
                            Some(skipped) => skipped,
                            None => self.local(file, local, visiting),
                        },
                        Link::Path(path) => self.path(file, path, visiting),
                        Link::Expression(declaration) => ExportTarget::declaration(
                            DeclRef {
                                file,
                                index: *declaration,
                            },
                            Identity::Declaration(DeclRef {
                                file,
                                index: *declaration,
                            }),
                        ),
                        Link::Module {
                            name: imported,
                            specifier,
                            type_only,
                        } => match self.skip(*type_only) {
                            Some(skipped) => skipped,
                            None => {
                                let target = self.module(file, specifier, SpecifierKind::Import);
                                if self.mode == Mode::Runtime {
                                    self.imported(&target, imported, module.synthetic_default, visiting)
                                } else if *imported == "*" {
                                    self.module_value(&target, visiting)
                                } else {
                                    self.member(&target, imported, visiting)
                                }
                            }
                        },
                    });
                }
            }
            None if name != "default" && name != "export=" => {
                for target in self.star_targets(file, index) {
                    if target
                        .namespace
                        .is_some_and(|module| !self.may_export(module, name))
                    {
                        continue;
                    }
                    targets.push(self.member(target, name, visiting));
                }
            }
            None => {}
        }
        join(targets, links.is_none())
    }

    /// What a type-only link is: type-only at run time, nothing as a value, followed as a
    /// type (`None`).
    fn skip(&self, type_only: bool) -> Option<ExportTarget> {
        match (type_only, self.mode) {
            (false, _) | (true, Mode::Type) => None,
            (true, Mode::Runtime) => Some(ExportTarget::type_only()),
            (true, Mode::Value) => Some(ExportTarget::default()),
        }
    }

    /// A whole module as a value: what its `export =` assigns, if it has one.
    fn module_value(&self, module: &ExportTarget, visiting: &mut Visiting) -> ExportTarget {
        match module.namespace {
            Some(file) if self.has_explicit(file, "export=") => self.export(file, "export=", visiting),
            _ => module.clone(),
        }
    }

    /// What `imported` of a module target is, for an import or re-export.
    fn imported(
        &self,
        module: &ExportTarget,
        imported: &str,
        synthetic: bool,
        visiting: &mut Visiting,
    ) -> ExportTarget {
        if imported == "*" {
            return if self.mode == Mode::Runtime {
                namespace_value(module)
            } else {
                self.module_value(module, visiting)
            };
        }
        let target = self.member(module, imported, visiting);
        if !target.is_empty() || target.type_only {
            return target;
        }
        match imported {
            "export=" => module.clone(),
            "default" if synthetic && module.namespace.is_some() => self.member(module, "export=", visiting),
            _ => target,
        }
    }

    /// The target of import binding `binding` of `file`.
    pub(super) fn binding(&self, file: FileId, binding: u32, visiting: &mut Visiting) -> ExportTarget {
        let Some((module, _)) = self.file(file) else {
            return ExportTarget::default();
        };
        let Some(entry) = module.facts.imports.get(binding as usize) else {
            return ExportTarget::default();
        };
        if let Some(skipped) = self.skip(entry.type_only) {
            return skipped;
        }
        let synthetic = if self.mode == Mode::Runtime {
            module.synthetic_default
        } else {
            module.callee_synthetic_default
        };
        match &entry.source {
            ImportSource::Module { specifier, require } => {
                let kind = if *require {
                    SpecifierKind::Require
                } else {
                    SpecifierKind::Import
                };
                let target = self.module(file, specifier, kind);
                self.imported(&target, &entry.imported, synthetic, visiting)
            }
            ImportSource::Qualified(names) => {
                let Some((first, rest)) = names.split_first() else {
                    return ExportTarget::default();
                };
                let mut target = self.local(file, first, visiting);
                if target.namespace.is_none() && target.external.is_empty() && !target.type_only {
                    return ExportTarget::default();
                }
                for name in rest {
                    target = self.member(&target, name, visiting);
                }
                target
            }
        }
    }

    /// The target of a module-scope name of `file`: an import binding's target, or the name's
    /// declarations.
    fn local(&self, file: FileId, name: &str, visiting: &mut Visiting) -> ExportTarget {
        let Some((module, index)) = self.file(file) else {
            return ExportTarget::default();
        };
        if let Some(declarations) = index.declarations.get(name) {
            let declarations: Vec<DeclRef> = declarations
                .iter()
                .map(|&index| DeclRef { file, index })
                .collect();
            let value = declarations.iter().any(|declaration| {
                module.facts.declarations[declaration.index as usize]
                    .kind
                    .is_value()
            });
            if self.mode != Mode::Runtime {
                // Each merged symbol is its own identity (an interface and a value of the same
                // name are two symbols when declared apart).
                let mut identities: Vec<Identity> = Vec::new();
                for declaration in &declarations {
                    let symbol = module
                        .symbols
                        .get(declaration.index as usize)
                        .copied()
                        .flatten()
                        .map_or_else(
                            || Identity::Declaration(*declaration),
                            |symbol| Identity::Bound(file, symbol),
                        );
                    if !identities.contains(&symbol) {
                        identities.push(symbol);
                    }
                }
                return ExportTarget {
                    declarations,
                    identities,
                    ..ExportTarget::default()
                };
            }
            return if value {
                ExportTarget {
                    declarations,
                    identities: vec![Identity::Symbol(file, name.into())],
                    ..ExportTarget::default()
                }
            } else {
                ExportTarget::type_only()
            };
        }
        let Some(&binding) = index.imports.get(name) else {
            return ExportTarget::default();
        };
        let key = (file, Box::<str>::from(name));
        if !visiting.locals.insert(key.clone()) {
            self.cuts.set(self.cuts.get() + 1);
            return ExportTarget::default();
        }
        let target = self.binding(file, binding, visiting);
        visiting.locals.remove(&key);
        target
    }

    /// `root.member.member…` from module scope.
    fn path(&self, file: FileId, path: &[Box<str>], visiting: &mut Visiting) -> ExportTarget {
        let Some((root, members)) = path.split_first() else {
            return ExportTarget::default();
        };
        let mut target = self.local(file, root, visiting);
        for member in members {
            target = self.member(&target, member, visiting);
        }
        target
    }

    /// Member `name` of a target (see [`ExportTable::member`]).
    ///
    /// A module whose `export =` assigns a module (or itself) is followed hop by hop; a module
    /// met twice on that path names nothing.
    fn member(&self, target: &ExportTarget, name: &str, visiting: &mut Visiting) -> ExportTarget {
        let mut target = Cow::Borrowed(target);
        let mut hops: SmallVec<[FileId; 4]> = SmallVec::new();
        loop {
            if target.ambiguous || !target.excluded.is_empty() || target.type_only {
                return target.into_owned();
            }
            if !target.external.is_empty() {
                return external_member(&target, name);
            }
            let Some(module) = target.namespace else {
                return ExportTarget::default();
            };
            let found = self.export(module, name, visiting);
            if !found.is_empty() || found.type_only {
                return found;
            }
            if name == "export=" || !self.has_explicit(module, "export=") || hops.contains(&module) {
                return ExportTarget::default();
            }
            hops.push(module);
            target = Cow::Owned(self.export(module, "export=", visiting));
        }
    }

    /// The module a specifier written in `file` names, as a target.
    fn module(&self, file: FileId, specifier: &str, kind: SpecifierKind) -> ExportTarget {
        match self.specifiers.resolve(file, specifier, kind) {
            Resolved::Indexed(module) => ExportTarget {
                namespace: Some(module),
                identities: vec![Identity::Module(module)],
                ..ExportTarget::default()
            },
            Resolved::Failed(module) => self.failed(module),
            Resolved::NotFound | Resolved::NotIndexed => ExportTarget::default(),
            Resolved::Unindexed { path, excluded: true } => ExportTarget {
                excluded: vec![ExternalLocation::File(path.as_str().into())],
                ..ExportTarget::default()
            },
            Resolved::Unindexed {
                path,
                excluded: false,
            } => ExportTarget {
                external: vec![ExternalLocation::File(path.as_str().into())],
                identities: vec![Identity::External(format!("file:{path}").into())],
                ..ExportTarget::default()
            },
            Resolved::Package(name) => ExportTarget {
                external: vec![ExternalLocation::Package(name)],
                identities: vec![Identity::External(format!("package:{specifier}").into())],
                ..ExportTarget::default()
            },
        }
    }

    /// A program file that never parsed: an excluded boundary at its path.
    fn failed(&self, module: FileId) -> ExportTarget {
        let path = self.specifiers.path(module);
        ExportTarget {
            excluded: path
                .map(|path| ExternalLocation::File(path.into()))
                .into_iter()
                .collect(),
            ..ExportTarget::default()
        }
    }

    /// The declaration candidates of a target, sorted as the proof of concept sorted them.
    pub(super) fn candidates(&self, target: &ExportTarget) -> Vec<Candidate> {
        let mut keyed: Vec<(String, Candidate)> = Vec::new();
        for location in &target.external {
            keyed.push((
                external_key("external", location),
                Candidate::external(location.clone()),
            ));
        }
        for location in &target.excluded {
            let candidate = Candidate {
                origin: Origin::Excluded,
                declaration: None,
                augmentation: false,
                location: Some(location.clone()),
            };
            keyed.push((external_key("excluded", location), candidate));
        }
        for &declaration in &target.declarations {
            let Some((facts, path)) = self
                .facts(declaration.file)
                .zip(self.specifiers.path(declaration.file))
            else {
                continue;
            };
            let Some(fact) = facts.declarations.get(declaration.index as usize) else {
                continue;
            };
            let key = format!("{path}\0{:012}\0{:012}", fact.range.start, fact.range.end);
            let mut candidate = Candidate::indexed(declaration);
            candidate.augmentation = fact.kind.is_augmentation();
            keyed.push((key, candidate));
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        keyed.into_iter().map(|(_, candidate)| candidate).collect()
    }
}

/// A module's value as a namespace: the module itself, as its file declaration.
fn namespace_value(target: &ExportTarget) -> ExportTarget {
    match target.namespace {
        Some(module) if target.declarations.is_empty() => ExportTarget {
            declarations: vec![DeclRef {
                file: module,
                index: 0,
            }],
            ..target.clone()
        },
        _ => target.clone(),
    }
}

/// The sort key of an external or excluded candidate.
fn external_key(origin: &str, location: &ExternalLocation) -> String {
    let location = match location {
        ExternalLocation::Library => r#"{"kind":"library"}"#.to_owned(),
        ExternalLocation::Package(name) => format!(r#"{{"kind":"package","name":{}}}"#, json_string(name)),
        ExternalLocation::File(path) => format!(r#"{{"kind":"file","filePath":{}}}"#, json_string(path)),
    };
    format!("{origin}\0{location}")
}

fn json_string(text: &str) -> String {
    serde_json::Value::String(text.to_owned()).to_string()
}

/// Several links of one name joined: everything they name, ambiguous when star exports
/// (`conflict`) name different things or several modules are meant.
fn join(targets: Vec<ExportTarget>, conflict: bool) -> ExportTarget {
    let mut joined = ExportTarget::default();
    let mut namespaces: SmallVec<[FileId; 2]> = SmallVec::new();
    let mut any_type_only = false;
    for target in targets {
        any_type_only |= target.type_only;
        joined.ambiguous |= target.ambiguous;
        for declaration in target.declarations {
            if !joined.declarations.contains(&declaration) {
                joined.declarations.push(declaration);
            }
        }
        if let Some(namespace) = target.namespace
            && !namespaces.contains(&namespace)
        {
            namespaces.push(namespace);
        }
        for location in target.external {
            if !joined.external.contains(&location) {
                joined.external.push(location);
            }
        }
        for location in target.excluded {
            if !joined.excluded.contains(&location) {
                joined.excluded.push(location);
            }
        }
        for identity in target.identities {
            if !joined.identities.contains(&identity) {
                joined.identities.push(identity);
            }
        }
    }
    joined.ambiguous |= (conflict && joined.identities.len() > 1) || namespaces.len() > 1;
    if namespaces.len() == 1 {
        joined.namespace = Some(namespaces[0]);
    }
    joined.type_only = joined.is_empty() && any_type_only;
    joined
}
