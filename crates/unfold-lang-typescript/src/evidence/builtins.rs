//! What can make a library member reach project code: project classes that may be instances of
//! a library type, and project writes to ECMAScript's own objects.
//!
//! A written library type is read nominally, as a project class type is: its values are its
//! instances or instances of the project classes that may be one. A project class may be an
//! instance of the ECMAScript type `T` when its `extends` chain reaches `T` (or a built-in type
//! that extends `T`: an `AggregateError` is an `Error`), or when an `implements` clause, through
//! project interfaces, names one. A class whose chain ends at an unknown base, at a library or
//! package base, or that implements anything else outside the project (or an unknown name) may
//! be an instance of anything. Its members (and those its project bases give it) then override
//! the library member of the same name: the call is `overridden`.
//!
//! The project patches an ECMAScript global when it writes to it (`JSON.parse = …`), to its
//! prototype (`Array.prototype.map = …`), or reflectively (`Object.defineProperty(String.prototype,
//! "trim", …)`), or replaces it (`globalThis.JSON = …`, `JSON = …`). A call that may reach a
//! patched member is `dynamic`. An alias of a global or prototype (`const p = Array.prototype`) is
//! not followed.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use unfold_engine::{DeclRef, FileId};

use super::{Receiver, Walk, WrittenMembers};
use crate::link::TypeScriptLinked;
use crate::payload::binding::{DeclFlags, DeclKind, Expr, Lookup, Meanings, TypeRef};
use crate::tables;

/// The names the global object goes by when nothing declares them.
const GLOBAL_OBJECTS: [&str; 4] = ["globalThis", "global", "window", "self"];

/// The library types a project class may be an instance of.
#[derive(Debug, Clone, Default)]
pub(crate) struct Lineage {
    /// Any type at all.
    any: bool,
    /// These ECMAScript types (with the types each extends).
    types: SmallVec<[&'static str; 2]>,
}

impl Lineage {
    fn is_empty(&self) -> bool {
        !self.any && self.types.is_empty()
    }

    fn merge(&mut self, other: &Self) {
        self.any |= other.any;
        for &found in &other.types {
            self.add(found);
        }
    }

    fn add(&mut self, name: &'static str) {
        if !self.types.contains(&name) {
            self.types.push(name);
        }
    }

    /// An ECMAScript type, with every type it extends.
    fn add_ecmascript(&mut self, name: &'static str) {
        self.add(name);
        for &supertype in tables::ecmascript_supertypes(name) {
            self.add(supertype);
        }
    }

    fn anything() -> Self {
        Self {
            any: true,
            ..Self::default()
        }
    }
}

/// The project's writes to ECMAScript's own objects.
#[derive(Debug, Default)]
pub(crate) struct Patches {
    /// By type (an ECMAScript global's value type for its own members, the class type for its
    /// prototype's members): the members written.
    members: FxHashMap<&'static str, WrittenMembers>,
    /// The ECMAScript globals replaced.
    replaced: FxHashSet<&'static str>,
    /// A write with a computed key to the global object (`globalThis[name] = …`) may replace
    /// any global.
    all: bool,
}

/// How a member call on a library receiver may dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dispatch {
    /// To the library member.
    Library,
    /// Possibly to a project class's member.
    Overridden,
    /// Possibly to a function the project wrote into an ECMAScript object.
    Patched,
}

impl TypeScriptLinked<'_> {
    /// Whether expression `index` of `file` is the global object (`globalThis`, or a host's
    /// `global`, `window`, or `self`) by an undeclared name.
    fn is_global_object(&self, file: FileId, index: u32) -> bool {
        let index = self.unwrap_expr(file, index);
        matches!(
            self.expr(file, index),
            Some(Expr::Name { name, lookup })
                if *lookup != Lookup::Dynamic
                    && GLOBAL_OBJECTS.contains(&&**name)
                    && !self.has_symbol(file, name, *lookup)
        )
    }

    /// The ECMAScript global that expression `index` of `file` reads: an undeclared name
    /// (`JSON`), or a member of the global object (`globalThis.JSON`).
    pub(crate) fn global_value(&self, file: FileId, index: u32) -> Option<&'static str> {
        let index = self.unwrap_expr(file, index);
        match self.expr(file, index)? {
            Expr::Name { name, lookup } => {
                if *lookup == Lookup::Dynamic || self.has_symbol(file, name, *lookup) {
                    return None;
                }
                tables::ecmascript_global(name)
            }
            Expr::Member { object, name, .. } if self.is_global_object(file, *object) => {
                tables::ecmascript_global(name)
            }
            Expr::Element {
                object,
                key: Some(key),
                ..
            } if self.is_global_object(file, *object) => tables::ecmascript_global(key),
            _ => None,
        }
    }

    /// The ECMAScript global whose prototype expression `index` of `file` reads
    /// (`Array.prototype`).
    fn global_prototype(&self, file: FileId, index: u32) -> Option<&'static str> {
        let index = self.unwrap_expr(file, index);
        let object = match self.expr(file, index)? {
            Expr::Member { object, name, .. } if &**name == "prototype" => *object,
            Expr::Element {
                object,
                key: Some(key),
                ..
            } if &**key == "prototype" => *object,
            _ => return None,
        };
        self.global_value(file, object)
    }

    /// Every write of the program to an ECMAScript global, its members, or its prototype's
    /// members.
    pub(crate) fn find_patches(&self) -> Patches {
        let mut patches = Patches::default();
        for index in 0..self.program().files().len() {
            let file = FileId(index as u32);
            let Some(binding) = self.binding(file) else {
                continue;
            };
            for write in &binding.writes {
                if self.is_global_object(file, write.object) {
                    match write.name.as_deref() {
                        Some(name) => {
                            if let Some(global) = tables::ecmascript_global(name) {
                                patches.replaced.insert(global);
                            }
                        }
                        None => patches.all = true,
                    }
                    continue;
                }
                let owner = match self.global_value(file, write.object) {
                    Some(global) => tables::ecmascript_value_type(global),
                    None => self.global_prototype(file, write.object),
                };
                if let Some(owner) = owner {
                    let members = patches.members.entry(owner).or_default();
                    match &write.name {
                        Some(name) => {
                            members.names.insert(name.clone());
                        }
                        None => members.any = true,
                    }
                }
            }
            for assignment in &binding.assignments {
                if matches!(assignment.lookup, Lookup::Unresolved | Lookup::Namespaced(_))
                    && !self.has_symbol(file, &assignment.name, assignment.lookup)
                    && let Some(global) = tables::ecmascript_global(&assignment.name)
                {
                    patches.replaced.insert(global);
                }
            }
        }
        patches
    }

    /// Whether the project may have replaced the ECMAScript global `global`.
    pub(crate) fn global_replaced(&self, global: &str) -> bool {
        let patches = &self.tables.patches;
        patches.all || patches.replaced.contains(global)
    }

    /// Whether the project may have written member `name` of a value of the ECMAScript type
    /// `type_name`: an ECMAScript global's value type, or an instance type, whose prototype (and
    /// those of the types it extends) count.
    fn member_patched(&self, type_name: &str, name: &str) -> bool {
        std::iter::once(type_name)
            .chain(tables::ecmascript_supertypes(type_name).iter().copied())
            .filter_map(|owner| self.tables.patches.members.get(owner))
            .any(|written| written.any || written.names.contains(name))
    }

    /// How member `name` of the library receiver `receiver` (written as expression `object` of
    /// `file`) may dispatch.
    pub(crate) fn library_dispatch(
        &self,
        file: FileId,
        object: u32,
        receiver: &Receiver,
        name: &str,
    ) -> Dispatch {
        let patched = match self.global_value(file, object) {
            Some(global) => {
                self.global_replaced(global)
                    || tables::ecmascript_value_type(global)
                        .is_some_and(|value_type| self.member_patched(value_type, name))
            }
            None => false,
        } || receiver
            .builtin
            .is_some_and(|type_name| self.member_patched(type_name, name));
        if patched {
            return Dispatch::Patched;
        }
        if receiver.nominal && self.library_overridden(receiver.builtin, name) {
            return Dispatch::Overridden;
        }
        Dispatch::Library
    }

    /// Whether a project class that may be an instance of the written library type (the
    /// ECMAScript type `type_name`, or another library type when `None`) has a member `name`.
    fn library_overridden(&self, type_name: Option<&str>, name: &str) -> bool {
        [name, "*"].iter().any(|key| {
            self.tables.library_overrides.get(*key).is_some_and(|lineage| {
                lineage.any || type_name.is_some_and(|type_name| lineage.types.contains(&type_name))
            })
        })
    }

    /// The members of project classes that may be instances of a library type, by name (`*`
    /// for a computed name), with the library types each class may be an instance of.
    pub(crate) fn find_library_overrides(&self) -> FxHashMap<Box<str>, Lineage> {
        let interfaces: FxHashMap<DeclRef, &[u32]> = (0..self.program().files().len())
            .map(|index| FileId(index as u32))
            .filter_map(|file| Some((file, self.binding(file)?)))
            .flat_map(|(file, binding)| {
                binding.interfaces.iter().map(move |facts| {
                    (
                        DeclRef {
                            file,
                            index: facts.decl,
                        },
                        facts.extends.as_slice(),
                    )
                })
            })
            .collect();
        let mut lineages = Lineages {
            interfaces,
            memo: FxHashMap::default(),
            visiting: FxHashSet::default(),
        };
        let mut classes: Vec<DeclRef> = self.tables.classes.keys().copied().collect();
        classes.sort_unstable();
        let mut overrides: FxHashMap<Box<str>, Lineage> = FxHashMap::default();
        for class in classes {
            let lineage = self.class_lineage(&mut lineages, class);
            if lineage.is_empty() {
                continue;
            }
            let mut visited = FxHashSet::default();
            let mut current = Some(class);
            while let Some(owner) = current {
                if !visited.insert(owner) {
                    break;
                }
                for &member in self
                    .tables
                    .classes
                    .get(&owner)
                    .map_or(&[][..], |tables| tables.members.as_slice())
                {
                    let Some(decl) = self.decl(member) else {
                        continue;
                    };
                    let overriding = !decl.flags.has(DeclFlags::STATIC)
                        && !decl.flags.has(DeclFlags::AMBIENT)
                        && !decl.flags.has(DeclFlags::PRIVATE)
                        && decl.kind != DeclKind::Constructor
                        && (decl.flags.has(DeclFlags::HAS_BODY)
                            || matches!(decl.kind, DeclKind::Property | DeclKind::Parameter));
                    if overriding {
                        let name: Box<str> = decl.name.clone().unwrap_or_else(|| "*".into());
                        overrides.entry(name).or_default().merge(&lineage);
                    }
                }
                current = self.tables.bases.get(&owner).and_then(|base| base.class);
            }
        }
        overrides
    }

    /// The library types a project class may be an instance of.
    fn class_lineage(&self, lineages: &mut Lineages<'_>, class: DeclRef) -> Lineage {
        if let Some(done) = lineages.memo.get(&class) {
            return done.clone();
        }
        if !lineages.visiting.insert(class) {
            return Lineage::default();
        }
        let mut lineage = Lineage::default();
        let base = self.tables.bases.get(&class).cloned().unwrap_or_default();
        if let Some(parent) = base.class {
            lineage.merge(&self.class_lineage(lineages, parent));
        } else if base.unknown || base.excluded {
            lineage.any = true;
        } else if base.external {
            let builtin = self
                .tables
                .classes
                .get(&class)
                .and_then(|tables| tables.heritage)
                .and_then(|heritage| self.global_value(class.file, heritage));
            match builtin {
                Some(global) => lineage.add_ecmascript(global),
                None => lineage.any = true,
            }
        }
        let implements = self
            .binding(class.file)
            .and_then(|binding| binding.classes.iter().find(|facts| facts.decl == class.index))
            .map(|facts| facts.implements.clone())
            .unwrap_or_default();
        for ty in implements {
            lineage.merge(&self.heritage_lineage(lineages, class.file, ty));
        }
        lineages.visiting.remove(&class);
        lineages.memo.insert(class, lineage.clone());
        lineage
    }

    /// The library types a value of a type name written in a heritage clause (an `implements`
    /// or an interface's `extends`) may be an instance of.
    fn heritage_lineage(&self, lineages: &mut Lineages<'_>, file: FileId, ty: u32) -> Lineage {
        let Some(TypeRef::Names {
            names,
            value: false,
            lookup,
            ..
        }) = self
            .binding(file)
            .and_then(|binding| binding.types.get(ty as usize))
        else {
            return Lineage::anything();
        };
        let Some((first, rest)) = names.split_first() else {
            return Lineage::anything();
        };
        let mut walk = Walk::default();
        let found = match lookup {
            Lookup::Symbol(symbol) => Some(self.resolve_symbol(&mut walk, file, *symbol, Meanings::TYPE)),
            Lookup::Unresolved | Lookup::Namespaced(_) => {
                self.unresolved(&mut walk, file, first, *lookup, Meanings::TYPE)
            }
            Lookup::Dynamic => return Lineage::anything(),
        };
        let Some(mut target) = found else {
            let qualified = names.join(".");
            let qualified = qualified.strip_prefix("globalThis.").unwrap_or(&qualified);
            let mut lineage = Lineage::default();
            match tables::ecmascript_type_name(qualified) {
                Some(name) => lineage.add_ecmascript(name),
                None => lineage.any = true,
            }
            return lineage;
        };
        for name in rest {
            target = self.member(&mut walk, &target, name, Meanings::TYPE);
        }
        if target.ambiguous
            || !target.external.is_empty()
            || !target.excluded.is_empty()
            || target.declarations.is_empty()
        {
            return Lineage::anything();
        }
        let mut lineage = Lineage::default();
        for declaration in target.declarations {
            let Some(decl) = self.decl(declaration) else {
                continue;
            };
            match decl.kind {
                DeclKind::Class | DeclKind::ClassExpression => {
                    lineage.merge(&self.class_lineage(lineages, declaration));
                }
                DeclKind::Interface => {
                    lineage.merge(&self.interface_lineage(lineages, declaration));
                }
                DeclKind::Namespace => {}
                _ => lineage.any = true,
            }
        }
        lineage
    }

    /// The library types a value of a project interface may be an instance of: those of the
    /// types it extends.
    fn interface_lineage(&self, lineages: &mut Lineages<'_>, interface: DeclRef) -> Lineage {
        if let Some(done) = lineages.memo.get(&interface) {
            return done.clone();
        }
        if !lineages.visiting.insert(interface) {
            return Lineage::default();
        }
        let extends = lineages
            .interfaces
            .get(&interface)
            .map(|extends| extends.to_vec())
            .unwrap_or_default();
        let mut lineage = Lineage::default();
        for ty in extends {
            lineage.merge(&self.heritage_lineage(lineages, interface.file, ty));
        }
        lineages.visiting.remove(&interface);
        lineages.memo.insert(interface, lineage.clone());
        lineage
    }
}

/// One computation of class lineages: the interfaces' `extends` clauses, the lineages found,
/// and the classes and interfaces being computed (a cycle adds nothing).
struct Lineages<'p> {
    interfaces: FxHashMap<DeclRef, &'p [u32]>,
    memo: FxHashMap<DeclRef, Lineage>,
    visiting: FxHashSet<DeclRef>,
}
