//! Classes (the proof of concept's `frontend/native/classes.ts`): member tables, bases, member
//! lookup through bases, `this` and `super`, class identity of constant aliases, and the facts
//! the override census reads (class shapes and members that may override unknown bases).
//!
//! A class's base is the class its written `extends` expression denotes (directly, through a
//! static receiver, or through `const` aliases of a class); an external or excluded base ends a
//! member lookup at that boundary. A base expression that names nothing known (and isn't
//! `null`) is an *unknown base*: the class may be an indexed subclass of anything, so its
//! overriding members make calls to those names on other receivers ambiguous.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use unfold_engine::evidence::{ClassMember, ClassShape, ExternalLocation};
use unfold_engine::{DeclRef, FileId};

use super::{Receiver, Target, Walk};
use crate::link::TypeScriptLinked;
use crate::payload::binding::{DeclFlags, DeclKind, Expr, VarKind};

/// A class's members by name (instance and static tables), its members in order, and its
/// written base expression.
#[derive(Debug, Clone, Default)]
pub(crate) struct ClassIndex {
    pub(crate) instance: FxHashMap<Box<str>, SmallVec<[DeclRef; 1]>>,
    pub(crate) statics: FxHashMap<Box<str>, SmallVec<[DeclRef; 1]>>,
    pub(crate) members: Vec<DeclRef>,
    pub(crate) heritage: Option<u32>,
}

/// A class's resolved base.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Base {
    pub(crate) class: Option<DeclRef>,
    pub(crate) external: bool,
    pub(crate) excluded: bool,
    pub(crate) locations: SmallVec<[ExternalLocation; 1]>,
    /// The base expression names nothing known.
    pub(crate) unknown: bool,
}

impl Base {
    /// The base as a receiver.
    fn receiver(&self, is_static: bool) -> Receiver {
        Receiver {
            class: self.class,
            external: self.external,
            excluded: self.excluded,
            locations: self.locations.clone(),
            is_static,
            ..Receiver::default()
        }
    }
}

impl TypeScriptLinked<'_> {
    /// Every class's member tables.
    pub(crate) fn index_classes(&self) -> FxHashMap<DeclRef, ClassIndex> {
        let mut classes = FxHashMap::default();
        for (index, _) in self.program().files().iter().enumerate() {
            let file = FileId(index as u32);
            let Some(binding) = self.binding(file) else {
                continue;
            };
            for class in &binding.classes {
                let mut tables = ClassIndex {
                    heritage: class.heritage,
                    members: class
                        .members
                        .iter()
                        .map(|&index| DeclRef { file, index })
                        .collect(),
                    ..ClassIndex::default()
                };
                for &member in &class.members {
                    let Some(decl) = binding.declarations.get(member as usize) else {
                        continue;
                    };
                    let Some(name) = &decl.name else {
                        continue;
                    };
                    if decl.kind == DeclKind::Constructor {
                        continue;
                    }
                    let table = if decl.flags.has(DeclFlags::STATIC) {
                        &mut tables.statics
                    } else {
                        &mut tables.instance
                    };
                    table
                        .entry(name.clone())
                        .or_default()
                        .push(DeclRef { file, index: member });
                }
                classes.insert(
                    DeclRef {
                        file,
                        index: class.decl,
                    },
                    tables,
                );
            }
        }
        classes
    }

    /// The `extends` expression of a class.
    fn heritage(&self, class: DeclRef) -> Option<u32> {
        self.tables.classes.get(&class)?.heritage
    }

    /// A class's base: precomputed once bases are linked, computed on demand before that.
    pub(crate) fn base_of(&self, walk: &mut Walk, class: DeclRef) -> Base {
        if let Some(base) = self.tables.bases.get(&class) {
            return base.clone();
        }
        self.compute_base(walk, class)
    }

    /// Resolves a class's written base.
    pub(crate) fn compute_base(&self, walk: &mut Walk, class: DeclRef) -> Base {
        let Some(expression) = self.heritage(class) else {
            return Base::default();
        };
        if !walk.bases.insert(class) || !walk.enter() {
            return Base::default();
        }
        let file = class.file;
        let target = self.target(walk, file, expression);
        // A builtin base's members are only known to be outside the project: a member other
        // than an ECMAScript method may come from the host (Node's `Error.captureStackTrace`).
        let mut base = Base {
            class: self.class_for_target(&target),
            external: !target.external.is_empty(),
            excluded: !target.excluded.is_empty(),
            locations: if target.external.is_empty() {
                target.excluded.clone()
            } else {
                target
                    .external
                    .iter()
                    .map(|location| match location {
                        ExternalLocation::Builtin => ExternalLocation::Library,
                        other => other.clone(),
                    })
                    .collect()
            },
            unknown: false,
        };
        if base.class.is_none() && !target.ambiguous && target.excluded.is_empty() {
            let receiver = self.receiver(walk, file, expression);
            if receiver.is_static {
                base.class = receiver.class;
            }
            base.external |= receiver.external;
            if base.locations.is_empty() {
                base.locations = receiver.locations;
            }
        }
        if base.class.is_none() {
            base.class = self.base_class_for_target(walk, &target, &mut FxHashSet::default());
        }
        let unwrapped = self.unwrap_expr(file, expression);
        base.unknown =
            base.class.is_none() && !base.external && !matches!(self.expr(file, unwrapped), Some(Expr::Null));
        walk.leave();
        walk.bases.remove(&class);
        base
    }

    /// The one class every non-augmenting declaration of a target is (a class, or a `const`
    /// initialized with a class expression).
    pub(crate) fn class_for_target(&self, target: &Target) -> Option<DeclRef> {
        if target.ambiguous || !target.external.is_empty() || !target.excluded.is_empty() {
            return None;
        }
        let mut found: Option<DeclRef> = None;
        for &declaration in &target.declarations {
            let decl = self.decl(declaration)?;
            if decl.kind.is_augmentation() {
                continue;
            }
            let class = self.class_for_declaration(declaration)?;
            match found {
                None => found = Some(class),
                Some(existing) if existing == class => {}
                Some(_) => return None,
            }
        }
        found
    }

    /// The class a declaration is: itself, or a `const`'s class-expression initializer.
    pub(crate) fn class_for_declaration(&self, declaration: DeclRef) -> Option<DeclRef> {
        let decl = self.decl(declaration)?;
        match decl.kind {
            DeclKind::Class | DeclKind::ClassExpression => Some(declaration),
            DeclKind::Variable if decl.var_kind == Some(VarKind::Const) => {
                let init = self.unwrap_expr(declaration.file, decl.initializer?);
                match self.expr(declaration.file, init)? {
                    Expr::Class(class) => Some(DeclRef {
                        file: declaration.file,
                        index: *class,
                    }),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// A base class reached through variables initialized with a class (or with another such
    /// variable), even when the alias itself is not a constant. A written annotation stays
    /// authoritative.
    fn base_class_for_target(
        &self,
        walk: &mut Walk,
        target: &Target,
        seen: &mut FxHashSet<DeclRef>,
    ) -> Option<DeclRef> {
        let direct = self.class_for_target(target);
        if direct.is_some() || target.ambiguous || !target.external.is_empty() || !target.excluded.is_empty()
        {
            return direct;
        }
        let mut found: Option<DeclRef> = None;
        for &declaration in &target.declarations {
            let decl = self.decl(declaration)?;
            if matches!(decl.kind, DeclKind::Namespace | DeclKind::Interface) {
                continue;
            }
            if !seen.insert(declaration) || decl.kind != DeclKind::Variable || decl.ty.is_some() {
                return None;
            }
            let init = decl.initializer?;
            let inner = self.target(walk, declaration.file, init);
            let class = self.base_class_for_target(walk, &inner, seen)?;
            match found {
                None => found = Some(class),
                Some(existing) if existing == class => {}
                Some(_) => return None,
            }
        }
        found
    }

    /// Member `name` of a class or its bases (an instance or static member).
    pub(crate) fn lookup_member(
        &self,
        walk: &mut Walk,
        class: DeclRef,
        name: &str,
        is_static: bool,
    ) -> Target {
        let mut visited = FxHashSet::default();
        let mut current = Some(class);
        while let Some(owner) = current {
            if !visited.insert(owner) {
                break;
            }
            if let Some(tables) = self.tables.classes.get(&owner) {
                let table = if is_static {
                    &tables.statics
                } else {
                    &tables.instance
                };
                if let Some(members) = table.get(name) {
                    return Target::of(members.iter().copied());
                }
            }
            let base = self.base_of(walk, owner);
            if base.excluded {
                return Target::excluded(base.locations);
            }
            if base.external {
                return Target::external(base.locations);
            }
            current = base.class;
        }
        Target::default()
    }

    /// What `this` means in context `context` of `file`.
    pub(crate) fn this_receiver(&self, file: FileId, context: u32) -> Receiver {
        match self
            .binding(file)
            .and_then(|binding| binding.contexts.get(context as usize))
            .and_then(|context| context.receiver)
        {
            Some((class, is_static)) => Receiver::class(DeclRef { file, index: class }, is_static),
            None => Receiver::unbound(),
        }
    }

    /// What `super` means as a receiver: the enclosing member's class's base.
    pub(crate) fn super_receiver(&self, walk: &mut Walk, file: FileId, context: u32) -> Receiver {
        match self
            .binding(file)
            .and_then(|binding| binding.contexts.get(context as usize))
            .and_then(|context| context.receiver)
        {
            Some((class, is_static)) => self
                .base_of(walk, DeclRef { file, index: class })
                .receiver(is_static),
            None => Receiver::default(),
        }
    }

    /// What `super` denotes: the written base expression of the nearest enclosing class.
    pub(crate) fn super_target(&self, walk: &mut Walk, file: FileId, context: u32) -> Target {
        let Some(class) = self
            .binding(file)
            .and_then(|binding| binding.contexts.get(context as usize))
            .and_then(|context| context.containing)
        else {
            return Target::default();
        };
        let Some(mut expression) = self.heritage(DeclRef { file, index: class }) else {
            return Target::default();
        };
        while let Some(Expr::Instantiation(inner)) = self.expr(file, expression) {
            expression = *inner;
        }
        match self.expr(file, expression) {
            Some(Expr::Name { .. } | Expr::Member { .. } | Expr::Element { .. }) => {
                self.target(walk, file, expression)
            }
            _ => Target::default(),
        }
    }

    /// The nearest class enclosing a `this` or `super` occurrence.
    pub(crate) fn containing_class(&self, file: FileId, context: u32) -> Option<DeclRef> {
        let class = self.binding(file)?.contexts.get(context as usize)?.containing?;
        Some(DeclRef { file, index: class })
    }

    /// The receiver evidence of the receiver expression `index` (the proof of concept's
    /// `classReceiver`): an assertion's type, a narrowed receiver withholds, `this`/`super`,
    /// an annotated variable's declared type, a class itself (a static receiver), or else the
    /// declared-type receiver.
    pub(crate) fn class_receiver(&self, walk: &mut Walk, file: FileId, index: u32) -> Receiver {
        if let Some(done) = walk.class_receiver_memo.get(&(file, index)) {
            return done.clone();
        }
        let receiver = self.class_receiver_uncached(walk, file, index);
        walk.class_receiver_memo.insert((file, index), receiver.clone());
        receiver
    }

    fn class_receiver_uncached(&self, walk: &mut Walk, file: FileId, index: u32) -> Receiver {
        let original = self.unwrap_receiver(file, index);
        if matches!(self.expr(file, original), Some(Expr::Assert { .. })) {
            return self.receiver(walk, file, index);
        }
        if self.is_narrowed(file, index) {
            return Receiver::default();
        }
        let expression = self.unwrap_expr(file, index);
        match self.expr(file, expression) {
            Some(Expr::This(context)) => return self.this_receiver(file, *context),
            Some(Expr::Super(context)) => return self.super_receiver(walk, file, *context),
            _ => {}
        }
        let target = self.target(walk, file, index);
        let annotated = target.declarations.iter().any(|&declaration| {
            self.decl(declaration)
                .is_some_and(|decl| decl.kind == DeclKind::Variable && decl.ty.is_some())
        });
        if annotated {
            return self.receiver(walk, file, index);
        }
        match self.class_for_target(&target) {
            Some(class) => Receiver::class(class, true),
            None => self.receiver(walk, file, index),
        }
    }

    /// The class properties written after their declaration, so their initializers are not
    /// their values: an assignment, update, `delete`, or reflective write whose object is the
    /// class or an instance of it, as written or under any assertion (a write with an unknown
    /// key writes every property), and a subclass member of the same name. A write through an
    /// object whose class isn't known writes nothing known, as in Python.
    pub(crate) fn find_written_fields(&self) -> FxHashSet<DeclRef> {
        let mut written = FxHashSet::default();
        for index in 0..self.program().files().len() {
            let file = FileId(index as u32);
            let Some(binding) = self.binding(file) else {
                continue;
            };
            for write in &binding.writes {
                let mut walk = Walk::default();
                let unwrapped = self.unwrap_expr(file, write.object);
                for object in [write.object, unwrapped] {
                    let receiver = self.class_receiver(&mut walk, file, object);
                    let Some(class) = receiver.class.filter(|_| !receiver.unbound_this) else {
                        continue;
                    };
                    match &write.name {
                        Some(name) => written.extend(
                            self.lookup_member(&mut walk, class, name, receiver.is_static)
                                .declarations,
                        ),
                        None => written.extend(self.lineage_members(&mut walk, class)),
                    }
                }
            }
        }
        let mut classes: Vec<DeclRef> = self.tables.classes.keys().copied().collect();
        classes.sort_unstable();
        for class in classes {
            let mut walk = Walk::default();
            let Some(base) = self.base_of(&mut walk, class).class else {
                continue;
            };
            for member in self.class_members(class) {
                let Some(decl) = self.decl(member) else {
                    continue;
                };
                // A private name is its own class's.
                let Some(name) = decl.name.as_deref().filter(|name| !name.starts_with('#')) else {
                    continue;
                };
                let is_static = decl.flags.has(DeclFlags::STATIC);
                written.extend(self.lookup_member(&mut walk, base, name, is_static).declarations);
            }
        }
        written.retain(|&declaration| {
            self.decl(declaration)
                .is_some_and(|decl| decl.kind == DeclKind::Property)
        });
        written
    }

    /// The members of a class and of its known bases.
    fn lineage_members(&self, walk: &mut Walk, class: DeclRef) -> Vec<DeclRef> {
        let mut members = Vec::new();
        let mut visited = FxHashSet::default();
        let mut current = Some(class);
        while let Some(owner) = current.filter(|&owner| visited.insert(owner)) {
            members.extend(self.class_members(owner));
            current = self.base_of(walk, owner).class;
        }
        members
    }

    /// Classes whose base chain reaches an unknown base, by the names of their overriding
    /// members (`*` for a member whose name isn't known).
    pub(crate) fn find_unknown_overrides(&self) -> FxHashMap<Box<str>, SmallVec<[DeclRef; 2]>> {
        let mut overrides: FxHashMap<Box<str>, SmallVec<[DeclRef; 2]>> = FxHashMap::default();
        let mut classes: Vec<&DeclRef> = self.tables.classes.keys().collect();
        classes.sort_unstable();
        for &class in classes {
            let mut visited = FxHashSet::default();
            let mut current = Some(class);
            let mut unknown = false;
            while let Some(owner) = current {
                if !visited.insert(owner) {
                    break;
                }
                let base = self.tables.bases.get(&owner).cloned().unwrap_or_default();
                if base.unknown {
                    unknown = true;
                    break;
                }
                current = base.class;
            }
            if !unknown {
                continue;
            }
            for member in self.class_members(class) {
                let Some(decl) = self.decl(member) else {
                    continue;
                };
                let overriding = !decl.flags.has(DeclFlags::STATIC)
                    && !decl.flags.has(DeclFlags::AMBIENT)
                    && decl.kind != DeclKind::Constructor
                    && (decl.flags.has(DeclFlags::HAS_BODY)
                        || matches!(decl.kind, DeclKind::Property | DeclKind::Parameter));
                if !overriding {
                    continue;
                }
                let name: Box<str> = decl.name.clone().unwrap_or_else(|| "*".into());
                let owners = overrides.entry(name).or_default();
                if !owners.contains(&class) {
                    owners.push(class);
                }
            }
        }
        overrides
    }

    /// Whether a class with an unlinked local base, other than `receiver`, declares an
    /// overriding member `name`: it may be an indexed subclass of the receiver.
    pub(crate) fn may_have_unknown_override(&self, receiver: Option<DeclRef>, name: &str) -> bool {
        if name.starts_with('#') {
            return false;
        }
        [name, "*"].iter().any(|key| {
            self.tables
                .unknown_overrides
                .get(*key)
                .is_some_and(|owners| owners.iter().any(|&owner| Some(owner) != receiver))
        })
    }

    /// A class's members: its elements, then its parameter properties.
    fn class_members(&self, class: DeclRef) -> impl Iterator<Item = DeclRef> + '_ {
        self.tables
            .classes
            .get(&class)
            .into_iter()
            .flat_map(|tables| tables.members.iter().copied())
    }

    /// Every class's shape for the override census, in file and class order.
    pub(crate) fn class_shapes(&self) -> Vec<ClassShape> {
        let mut shapes = Vec::new();
        for (index, _) in self.program().files().iter().enumerate() {
            let file = FileId(index as u32);
            let Some(binding) = self.binding(file) else {
                continue;
            };
            for facts in &binding.classes {
                let class = DeclRef {
                    file,
                    index: facts.decl,
                };
                let bases = self
                    .tables
                    .bases
                    .get(&class)
                    .and_then(|base| base.class)
                    .into_iter()
                    .collect();
                let members = facts
                    .members
                    .iter()
                    .filter_map(|&member| {
                        let decl = binding.declarations.get(member as usize)?;
                        Some(ClassMember {
                            declaration: DeclRef { file, index: member },
                            is_static: decl.flags.has(DeclFlags::STATIC),
                        })
                    })
                    .collect();
                shapes.push(ClassShape {
                    class,
                    bases,
                    members,
                });
            }
        }
        shapes
    }
}
