//! Export tables: what each module exports at run time, with re-export chains followed
//! (docs/languages/typescript.md#project-configuration; the proof of concept's native `modules.ts` export queries).
//!
//! An export name links to the declarations it was written with:
//!
//! - a declaration written with `export` (its name), or `export default` (`default` only);
//! - `export { local as name }`, `export import name = …`: the local name's declarations, or
//!   the import binding's target;
//! - `export default <expression>` / `export = <expression>`: an access path (`a.b`) is
//!   followed; any other expression is its own declaration;
//! - `export { a as name } from "m"`, `export * as name from "m"`: the module's export `a`, or
//!   the module itself;
//! - `export * from "m"`: every export of the module except `default` and `export=`, unless the
//!   file exports that name explicitly (explicit exports win). Two stars that export different
//!   declarations under one name make it ambiguous.
//!
//! Following a link: an import binding's target is the imported module's export (`*` is the
//! module itself; `export=` falls back to the module; a missing `default` falls back to
//! `export=` when synthetic default imports are on). A module whose `export =` names a value
//! exposes that value's members. A module outside the program is external (or, for a source the
//! scope excludes or a file that failed, an excluded boundary) with its location. Names are
//! followed as values (type-only links name nothing) and as types (type-only links are
//! followed). Cycles stop at the first repeated `(module, name)`.

mod builder;
mod index;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use unfold_engine::evidence::ExternalLocation;
use unfold_engine::{DeclRef, FileId};

use crate::modules::{Resolved, SpecifierKind};
use crate::payload::ModuleFacts;
use builder::{Builder, Mode, Visiting, indexes};

/// What a name, binding, or module resolves to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportTarget {
    /// Indexed declarations (module-scope declarations, exported expressions, or a module's
    /// file declaration for a namespace), deduplicated, in discovery order.
    pub declarations: Vec<DeclRef>,
    /// A module whose exports are reachable through this target (an import or re-export of
    /// the whole module).
    pub namespace: Option<FileId>,
    /// External locations: packages, and repository files outside the program.
    pub external: Vec<ExternalLocation>,
    /// Excluded boundaries: sources the scope leaves out, and files that failed.
    pub excluded: Vec<ExternalLocation>,
    /// More than one declaration could be meant.
    pub ambiguous: bool,
    identities: Vec<Identity>,
}

impl ExportTarget {
    /// Whether the target names anything.
    pub fn is_empty(&self) -> bool {
        self.declarations.is_empty()
            && self.namespace.is_none()
            && self.external.is_empty()
            && self.excluded.is_empty()
            && !self.ambiguous
    }

    fn declaration(declaration: DeclRef, identity: Identity) -> Self {
        Self {
            declarations: vec![declaration],
            identities: vec![identity],
            ..Self::default()
        }
    }
}

/// What makes two targets the same, for star-export conflicts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Identity {
    /// A bound symbol of a file (an index into its binding's symbols).
    Bound(FileId, u32),
    /// One declaration (an exported expression).
    Declaration(DeclRef),
    /// A module.
    Module(FileId),
    /// Something outside the program, by where it is and the member path into it.
    External(Box<str>),
}

/// One file as the export tables see it.
pub(crate) struct ModuleFile<'a> {
    pub(crate) facts: &'a ModuleFacts,
    /// Whether the file is a module (scripts export nothing).
    pub(crate) is_module: bool,
    /// Whether a missing `default` import falls back to `export=`, as evidence follows imports:
    /// `allowSyntheticDefaultImports`, else `esModuleInterop`, else whether `module` is a Node
    /// kind or `preserve`.
    pub(crate) synthetic_default: bool,
    /// Each module declaration's bound symbol, aligned with `facts.declarations`.
    pub(crate) symbols: Vec<Option<u32>>,
}

/// How one specifier written in a file resolves (the module resolver, for this file set).
/// Resolution is a pure function of its arguments, so the export tables of every mode can
/// share one resolver across threads.
pub(crate) trait Specifiers: Sync {
    /// Where `specifier`, written in `file` as `kind`, leads.
    fn resolve(&self, file: FileId, specifier: &str, kind: SpecifierKind) -> Resolved;
    /// The path of a program file.
    fn path(&self, file: FileId) -> Option<&str>;
}

/// Every file's export names and import-binding targets, followed as values and as types.
#[derive(Debug, Default)]
pub struct ExportTable {
    /// By file ID: whether `export=` is among its explicit exports.
    export_equals: Vec<bool>,
    /// By file ID: every export name and its target, followed as evidence follows values.
    value_names: Vec<FxHashMap<Box<str>, ExportTarget>>,
    /// By file ID: each import binding's target, aligned with the file's import bindings.
    value_bindings: Vec<Vec<ExportTarget>>,
    /// The same names and bindings followed for their types (written annotations).
    type_names: Vec<FxHashMap<Box<str>, ExportTarget>>,
    type_bindings: Vec<Vec<ExportTarget>>,
}

/// One mode's tables: every export name's target and every import binding's target.
struct Followed {
    names: Vec<FxHashMap<Box<str>, ExportTarget>>,
    bindings: Vec<Vec<ExportTarget>>,
}

/// Follows every export name and import binding of `files` with `builder`.
fn follow(files: &[Option<ModuleFile<'_>>], builder: &Builder<'_, '_, '_>) -> Followed {
    let mut followed = Followed {
        names: Vec::with_capacity(files.len()),
        bindings: Vec::with_capacity(files.len()),
    };
    for (index, file) in files.iter().enumerate() {
        let id = FileId(index as u32);
        let Some(file) = file.as_ref().filter(|file| file.is_module) else {
            followed.names.push(FxHashMap::default());
            followed
                .bindings
                .push(file.as_ref().map_or_else(Vec::new, |file| {
                    (0..file.facts.imports.len())
                        .map(|_| ExportTarget::default())
                        .collect()
                }));
            continue;
        };
        let mut names: Vec<&str> = builder.export_names(id).collect();
        names.sort_unstable();
        let targets = names
            .into_iter()
            .map(|name| {
                let target = builder.export(id, name, &mut Visiting::default());
                (Box::from(name), target)
            })
            .collect();
        let bindings = (0..file.facts.imports.len())
            .map(|binding| builder.binding(id, binding as u32, &mut Visiting::default()))
            .collect();
        followed.names.push(targets);
        followed.bindings.push(bindings);
    }
    followed
}

impl ExportTable {
    /// Builds the tables of `files` (by file ID; `None` for files that aren't TypeScript
    /// modules with facts).
    pub(crate) fn build(files: &[Option<ModuleFile<'_>>], specifiers: &dyn Specifiers) -> Self {
        let indexes = indexes(files);
        // The two modes share nothing but the indexes and the (pure) specifier resolution, so
        // they follow in parallel; each one's results are independent of the other's.
        let follow_mode = |mode| follow(files, &Builder::new(files, &indexes, specifiers, mode));
        let (values, types) = rayon::join(|| follow_mode(Mode::Value), || follow_mode(Mode::Type));
        let export_equals = indexes
            .iter()
            .map(|index| {
                index
                    .as_ref()
                    .is_some_and(|index| index.explicit.contains_key("export="))
            })
            .collect();
        Self {
            export_equals,
            value_names: values.names,
            value_bindings: values.bindings,
            type_names: types.names,
            type_bindings: types.bindings,
        }
    }

    /// The target of import binding `binding` (an index into the file's
    /// [`ModuleFacts::imports`]) followed as a value, as evidence follows it.
    pub fn value_binding(&self, file: FileId, binding: u32) -> Option<&ExportTarget> {
        self.value_bindings.get(file.index())?.get(binding as usize)
    }

    /// The target of import binding `binding` followed for its type: type-only imports and
    /// re-exports are followed, and a name's type declarations are its target.
    pub fn type_binding(&self, file: FileId, binding: u32) -> Option<&ExportTarget> {
        self.type_bindings.get(file.index())?.get(binding as usize)
    }

    /// Member `name` of a target, followed as a value: a module's export (or, when the module
    /// has `export =`, the member of what it assigns), or an external member. Ambiguous and
    /// excluded targets stay as they are; anything else has no members here.
    pub fn value_member(&self, target: &ExportTarget, name: &str) -> ExportTarget {
        self.member_in(&self.value_names, target, name)
    }

    /// [`ExportTable::value_member`] followed for types.
    pub fn type_member(&self, target: &ExportTarget, name: &str) -> ExportTarget {
        self.member_in(&self.type_names, target, name)
    }

    fn member_in(
        &self,
        names: &[FxHashMap<Box<str>, ExportTarget>],
        target: &ExportTarget,
        name: &str,
    ) -> ExportTarget {
        // `export =` hops are followed in a loop; a module met twice names nothing.
        let mut target = target;
        let mut hops: SmallVec<[FileId; 4]> = SmallVec::new();
        loop {
            if target.ambiguous || !target.excluded.is_empty() {
                return target.clone();
            }
            if !target.external.is_empty() {
                return external_member(target, name);
            }
            let Some(module) = target.namespace else {
                return ExportTarget::default();
            };
            let exported = names.get(module.index());
            if let Some(found) = exported.and_then(|names| names.get(name))
                && !found.is_empty()
            {
                return found.clone();
            }
            let assigned = exported.and_then(|names| names.get("export="));
            match assigned {
                Some(assigned)
                    if name != "export="
                        && self.export_equals.get(module.index()).copied().unwrap_or(false)
                        && !hops.contains(&module) =>
                {
                    hops.push(module);
                    target = assigned;
                }
                _ => return ExportTarget::default(),
            }
        }
    }
}

impl ExportTarget {
    /// A whole module, as a namespace import names it.
    pub fn module(file: FileId) -> Self {
        Self {
            namespace: Some(file),
            identities: vec![Identity::Module(file)],
            ..Self::default()
        }
    }
}

/// The member `name` of an external target: the same locations, a longer identity.
fn external_member(target: &ExportTarget, name: &str) -> ExportTarget {
    ExportTarget {
        external: target.external.clone(),
        identities: target
            .identities
            .iter()
            .map(|identity| match identity {
                Identity::External(path) => Identity::External(format!("{path}.{name}").into()),
                other => other.clone(),
            })
            .collect(),
        ..ExportTarget::default()
    }
}

#[cfg(test)]
mod tests;
