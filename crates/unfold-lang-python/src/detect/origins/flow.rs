//! What values flow where, independently of any framework: the values written into attribute
//! and item locations, the identities a value may have through every write, the items a literal
//! container may hold, and the binder's answers the walks need. One flow serves every source
//! detector of a link; each [`super::Origins`] adds its framework's census on top.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustc_hash::{FxHashMap, FxHashSet};

use super::{MAX_DEPTH, Path, Relation, Segment};
use crate::bind::ir::ScopeId;
use crate::bind::ir::{Effect, ExprKind, MODULE_SCOPE, NodeKind, TargetKind};
use crate::detect::Source;
use crate::link::binder::Ctx;
use crate::link::children::Raw;
use crate::link::program::{ExprRef, NodeRef, Sym};
use unfold_engine::FileId;

/// The program's value flow, computed once and shared by the source detectors.
pub(in crate::detect) struct Flow<'s, 'b, 'a> {
    pub(super) source: &'s Source<'b, 'a>,
    /// Values written into attributes and items, by the declaration their location starts at
    /// and the path from it.
    stored: FxHashMap<(Raw, Path), Vec<ExprRef>>,
    /// The stored paths under each root, for the rare query that needs every extension.
    stored_paths: FxHashMap<Raw, Vec<Path>>,
    identities: RefCell<FxHashMap<ExprRef, Option<Rc<str>>>>,
    memo: Memo,
    /// Set when a walk ran out of depth; every census then withholds everything.
    pub(super) exhausted: Cell<bool>,
}

/// Kept answers: each query starts with nothing seen, so its answer doesn't depend on who asks.
#[derive(Default)]
struct Memo {
    imports: RefCell<FxHashMap<Raw, Rc<FxHashSet<Rc<str>>>>>,
    containers: RefCell<FxHashMap<Raw, Rc<[ExprRef]>>>,
    /// The declarations (or writes) a location's root name may be.
    roots: RefCell<FxHashMap<ExprRef, Rc<[Raw]>>>,
    /// The first name expression of each name in each scope (see [`Flow::canonical`]).
    names: RefCell<FxHashMap<(FileId, ScopeId, Sym), ExprRef>>,
}

impl<'s, 'b, 'a> Flow<'s, 'b, 'a> {
    /// Records every stored value of the program.
    pub(in crate::detect) fn new(source: &'s Source<'b, 'a>) -> Self {
        let mut flow = Self {
            source,
            stored: FxHashMap::default(),
            stored_paths: FxHashMap::default(),
            identities: RefCell::default(),
            memo: Memo::default(),
            exhausted: Cell::new(false),
        };
        flow.collect_storage();
        flow
    }

    /// The proven imported identity of an expression (`fastapi.FastAPI`).
    pub(in crate::detect) fn imported(&self, expr: Option<ExprRef>) -> Option<Rc<str>> {
        let expr = expr?;
        if let Some(found) = self.identities.borrow().get(&expr) {
            return found.clone();
        }
        let found: Option<Rc<str>> = self.source.binder.identity(expr, &mut Ctx::new()).map(Rc::from);
        self.identities.borrow_mut().insert(expr, found.clone());
        found
    }

    /// The value a chained assignment level stands for.
    pub(super) fn unchain(&self, mut expr: ExprRef) -> ExprRef {
        for _ in 0..MAX_DEPTH {
            match self.source.kind(expr) {
                Some(&ExprKind::Chain(value)) => {
                    expr = ExprRef {
                        file: expr.file,
                        expr: value,
                    };
                }
                _ => break,
            }
        }
        expr
    }

    pub(super) fn enter(&self, depth: u32) -> bool {
        if depth >= MAX_DEPTH {
            self.exhausted.set(true);
            return false;
        }
        true
    }

    pub(super) fn local(expr: ExprRef, id: u32) -> ExprRef {
        ExprRef {
            file: expr.file,
            expr: id,
        }
    }

    /// The writes of a name or import declaration, however ambiguous.
    pub(super) fn writes(&self, raw: Raw) -> Vec<u32> {
        let is_name = match raw {
            Raw::Expr(expr) => matches!(self.source.kind(expr), Some(ExprKind::Name(_))),
            Raw::Node(node) => matches!(
                self.source.binder.program.node(node).map(|fact| &fact.kind),
                Some(NodeKind::Import)
            ),
        };
        if is_name {
            self.source.binder.raw_writes(raw)
        } else {
            Vec::new()
        }
    }

    /// The module-level declarations of `member` (or of every name) in an indexed import's
    /// module files.
    pub(super) fn imported_declarations(&self, write: u32, all: bool) -> Vec<NodeRef> {
        let binder = self.source.binder;
        let write = binder.program.write(write);
        let Some(imported) = &write.import else {
            return Vec::new();
        };
        if imported.module.external.is_some() {
            return Vec::new();
        }
        let mut declarations = Vec::new();
        for path in &imported.module.paths {
            let Some(file) = binder.modules.indexed(path) else {
                continue;
            };
            let bindings: Vec<u32> = match imported.member {
                Some(member) => binder.program.module_binding(file, member).into_iter().collect(),
                None if all => binder.program.bindings_in(file, MODULE_SCOPE).to_vec(),
                None => Vec::new(),
            };
            for binding in bindings {
                declarations.extend(
                    binder
                        .program
                        .binding(binding)
                        .writes
                        .iter()
                        .map(|&found| binder.program.write(found).node),
                );
            }
        }
        declarations
    }

    /// `module.member` (or `module.*`) for an external import write.
    pub(super) fn external_import(&self, write: u32) -> Option<String> {
        let binder = self.source.binder;
        let imported = binder.program.write(write).import.as_ref()?;
        imported.module.external.as_ref()?;
        let member = imported.member.map_or("*", |member| binder.program.text(member));
        Some(format!("{}.{member}", imported.module.specifier))
    }

    /// The locations an attribute or item expression denotes: each declaration its root name
    /// may be, and the path of attributes and items from it.
    pub(super) fn locations(&self, expr: ExprRef) -> Vec<(Raw, Path)> {
        let source = self.source;
        let mut path = Path::new();
        let mut current = expr;
        for _ in 0..MAX_DEPTH {
            match source.kind(current) {
                Some(&ExprKind::Member { base, name }) => {
                    path.push(Segment::Member(source.binder.program.sym(current.file, name)));
                    current = Self::local(current, base);
                }
                Some(&ExprKind::Index { base, .. }) => {
                    path.push(Segment::Item);
                    current = Self::local(current, base);
                }
                Some(ExprKind::Name(_)) => {
                    path.reverse();
                    return self
                        .roots(current)
                        .iter()
                        .map(|&root| (root, path.clone()))
                        .collect();
                }
                _ => return Vec::new(),
            }
        }
        Vec::new()
    }

    /// The declarations a root name may be, or (when it resolves to none) its writes.
    pub(super) fn roots(&self, name: ExprRef) -> Rc<[Raw]> {
        if let Some(found) = self.memo.roots.borrow().get(&name) {
            return Rc::clone(found);
        }
        let binder = self.source.binder;
        let value = binder.resolve(name, &mut Ctx::new());
        let roots: Rc<[Raw]> = if value.declarations.is_empty() {
            self.writes(Raw::Expr(name))
                .into_iter()
                .map(|write| Raw::Node(binder.program.write(write).node))
                .collect()
        } else {
            value.declarations.iter().map(|&node| Raw::Node(node)).collect()
        };
        self.memo.roots.borrow_mut().insert(name, Rc::clone(&roots));
        roots
    }

    /// The stored values whose location is a prefix of (`prefix`), equal to (`exact`), or
    /// extends (`extends`) one of `raw`'s locations.
    pub(super) fn stored_at(&self, raw: Raw, relation: Relation) -> Vec<ExprRef> {
        let Raw::Expr(expr) = raw else {
            return Vec::new();
        };
        if self.stored.is_empty() {
            return Vec::new();
        }
        // First occurrences, in order.
        let mut found = Vec::new();
        let mut seen = FxHashSet::default();
        let mut add = |values: Option<&Vec<ExprRef>>| {
            for &value in values.map_or(&[][..], Vec::as_slice) {
                if seen.insert(value) {
                    found.push(value);
                }
            }
        };
        for (root, path) in self.locations(expr) {
            match relation {
                Relation::Exact => add(self.stored.get(&(root, path))),
                Relation::Prefix => {
                    for length in 0..=path.len() {
                        add(self.stored.get(&(root, Path::from_slice(&path[..length]))));
                    }
                }
                Relation::Extends => {
                    for stored in self.stored_paths.get(&root).map_or(&[][..], Vec::as_slice) {
                        if stored.starts_with(&path) {
                            add(self.stored.get(&(root, stored.clone())));
                        }
                    }
                }
            }
        }
        found
    }

    /// One representative for every name expression of a name in a scope: Python looks a name
    /// up by its scope alone, so every question about them has the same answer, which a query
    /// can then keep once.
    pub(in crate::detect) fn canonical(&self, raw: Raw) -> Raw {
        let Raw::Expr(expr) = raw else {
            return raw;
        };
        let Some(fact) = self.source.binder.program.expr(expr) else {
            return raw;
        };
        let ExprKind::Name(name) = fact.kind else {
            return raw;
        };
        let key = (
            expr.file,
            fact.scope,
            self.source.binder.program.sym(expr.file, name),
        );
        Raw::Expr(*self.memo.names.borrow_mut().entry(key).or_insert(expr))
    }

    /// Every identity `raw` may have, through stored locations, attributes, and every write.
    /// A query starts with nothing seen, so its answer is complete and is kept.
    pub(in crate::detect) fn possible_imports(&self, raw: Raw) -> Rc<FxHashSet<Rc<str>>> {
        let raw = self.canonical(raw);
        if let Some(found) = self.memo.imports.borrow().get(&raw) {
            return Rc::clone(found);
        }
        let mut found = FxHashSet::default();
        self.collect_imports(raw, &mut found, &mut FxHashSet::default(), 0);
        let found = Rc::new(found);
        self.memo.imports.borrow_mut().insert(raw, Rc::clone(&found));
        found
    }

    pub(super) fn collect_imports(
        &self,
        raw: Raw,
        found: &mut FxHashSet<Rc<str>>,
        seen: &mut FxHashSet<Raw>,
        depth: u32,
    ) {
        if !self.enter(depth) || !seen.insert(raw) {
            return;
        }
        let next = depth + 1;
        let source = self.source;
        if let Raw::Expr(expr) = raw {
            if let Some(identity) = self.imported(Some(expr)) {
                found.insert(identity);
            }
            for value in self.stored_at(raw, Relation::Exact) {
                self.collect_imports(Raw::Expr(value), found, seen, next);
            }
            match source.kind(expr) {
                Some(ExprKind::Member { .. }) => {
                    if let Some((base, name)) = source.member(expr) {
                        let mut bases = FxHashSet::default();
                        self.collect_imports(Raw::Expr(base), &mut bases, seen, next);
                        for base in bases {
                            let base = base.strip_suffix(".*").unwrap_or(&base);
                            found.insert(Rc::from(format!("{base}.{name}")));
                        }
                    }
                }
                Some(&ExprKind::Chain(value)) => {
                    self.collect_imports(Raw::Expr(Self::local(expr, value)), found, seen, next);
                }
                Some(ExprKind::Name(_)) => self.import_writes(raw, found, seen, next),
                _ => {}
            }
            return;
        }
        let Raw::Node(node) = raw else {
            return;
        };
        match source.binder.program.node(node).map(|fact| &fact.kind) {
            Some(&NodeKind::Assignment {
                value: Some(value), ..
            }) => {
                let value = ExprRef {
                    file: node.file,
                    expr: value,
                };
                self.collect_imports(Raw::Expr(value), found, seen, next);
            }
            Some(NodeKind::Import) => self.import_writes(raw, found, seen, next),
            _ => {}
        }
    }

    pub(super) fn import_writes(
        &self,
        raw: Raw,
        found: &mut FxHashSet<Rc<str>>,
        seen: &mut FxHashSet<Raw>,
        depth: u32,
    ) {
        for write in self.writes(raw) {
            if let Some(value) = self.source.binder.program.write(write).value {
                self.collect_imports(Raw::Expr(value), found, seen, depth);
            }
            if let Some(identity) = self.external_import(write) {
                found.insert(Rc::from(identity));
                continue;
            }
            for declaration in self.imported_declarations(write, false) {
                self.collect_imports(Raw::Node(declaration), found, seen, depth);
            }
        }
    }

    /// Values possibly stored at `expr`'s location, or held by the container it indexes. For
    /// withholding only.
    pub(in crate::detect) fn stored_values(&self, expr: ExprRef) -> Vec<ExprRef> {
        let mut found = self.stored_at(Raw::Expr(expr), Relation::Prefix);
        if let Some(&ExprKind::Index { base, .. }) = self.source.kind(expr) {
            for &value in self.container_values(Raw::Expr(Self::local(expr, base))).iter() {
                if !found.contains(&value) {
                    found.push(value);
                }
            }
        }
        found
    }

    /// The items a literal container value may hold, through aliases, expansions, stored
    /// locations, and module attributes.
    pub(in crate::detect) fn container_values(&self, raw: Raw) -> Rc<[ExprRef]> {
        let raw = self.canonical(raw);
        if let Some(found) = self.memo.containers.borrow().get(&raw) {
            return Rc::clone(found);
        }
        let mut found = Vec::new();
        self.collect_container(raw, &mut found, &mut FxHashSet::default(), 0);
        let found: Rc<[ExprRef]> = found.into();
        self.memo.containers.borrow_mut().insert(raw, Rc::clone(&found));
        found
    }

    pub(super) fn collect_container(
        &self,
        raw: Raw,
        found: &mut Vec<ExprRef>,
        seen: &mut FxHashSet<Raw>,
        depth: u32,
    ) {
        if !self.enter(depth) || !seen.insert(raw) {
            return;
        }
        let next = depth + 1;
        let source = self.source;
        let binder = source.binder;
        let add = |found: &mut Vec<ExprRef>, item: ExprRef| {
            if !found.contains(&item) {
                found.push(item);
            }
        };
        match raw {
            Raw::Expr(expr) => {
                match source.kind(expr) {
                    Some(ExprKind::Sequence(items)) => {
                        for &item in items {
                            let item = Self::local(expr, item);
                            match source.kind(item) {
                                Some(&ExprKind::Starred(inner)) => {
                                    self.collect_container(
                                        Raw::Expr(Self::local(item, inner)),
                                        found,
                                        seen,
                                        next,
                                    );
                                }
                                _ => add(found, item),
                            }
                        }
                    }
                    // A dict display's values and `**` expansions are recorded alike: each is
                    // both a possible value and a container whose values may be spread.
                    Some(ExprKind::Dict(items)) => {
                        for &item in items {
                            let item = Self::local(expr, item);
                            add(found, item);
                            self.collect_container(Raw::Expr(item), found, seen, next);
                        }
                    }
                    Some(&ExprKind::Index { base, .. }) => {
                        for &selected in self.container_values(Raw::Expr(Self::local(expr, base))).iter() {
                            self.collect_container(Raw::Expr(selected), found, seen, next);
                        }
                    }
                    Some(&ExprKind::Member { base, name }) => {
                        let module = binder.resolve(Self::local(expr, base), &mut Ctx::new()).module;
                        let member = binder.program.sym(expr.file, name);
                        for declaration in module
                            .iter()
                            .flat_map(|module| binder.module_declarations(module, member))
                        {
                            self.collect_container(Raw::Node(declaration), found, seen, next);
                        }
                    }
                    Some(ExprKind::Name(_)) => {
                        for write in self.writes(raw) {
                            if let Some(value) = binder.program.write(write).value {
                                self.collect_container(Raw::Expr(value), found, seen, next);
                            }
                            for declaration in self.imported_declarations(write, false) {
                                self.collect_container(Raw::Node(declaration), found, seen, next);
                            }
                        }
                    }
                    Some(&ExprKind::Chain(value)) => {
                        self.collect_container(Raw::Expr(Self::local(expr, value)), found, seen, next);
                    }
                    _ => {}
                }
                for stored in self.stored_at(raw, Relation::Prefix) {
                    self.collect_container(Raw::Expr(stored), found, seen, next);
                }
                let value = binder.resolve(expr, &mut Ctx::new());
                for &declaration in &value.declarations {
                    self.collect_container(Raw::Node(declaration), found, seen, next);
                }
            }
            Raw::Node(node) => match binder.program.node(node).map(|fact| &fact.kind) {
                Some(&NodeKind::Assignment {
                    value: Some(value), ..
                }) => {
                    let value = ExprRef {
                        file: node.file,
                        expr: value,
                    };
                    self.collect_container(Raw::Expr(value), found, seen, next);
                }
                Some(NodeKind::Import) => {
                    for write in self.writes(raw) {
                        if let Some(value) = binder.program.write(write).value {
                            self.collect_container(Raw::Expr(value), found, seen, next);
                        }
                        for declaration in self.imported_declarations(write, false) {
                            self.collect_container(Raw::Node(declaration), found, seen, next);
                        }
                    }
                }
                _ => {}
            },
        }
    }

    pub(super) fn effects(&self) -> Vec<(FileId, Effect)> {
        let program = &self.source.binder.program;
        program
            .files
            .iter()
            .enumerate()
            .filter_map(|(index, bind)| Some((FileId(index as u32), bind.as_ref()?)))
            .flat_map(|(file, bind)| bind.effects.iter().map(move |effect| (file, effect.clone())))
            .collect()
    }

    pub(super) fn collect_storage(&mut self) {
        for (file, effect) in self.effects() {
            if let Effect::Target {
                target,
                kind: TargetKind::Assignment,
                value: Some(value),
                ..
            } = effect
            {
                let target = ExprRef { file, expr: target };
                let value = self.unchain(ExprRef { file, expr: value });
                self.record(target, value, 0);
            }
        }
    }

    pub(super) fn record(&mut self, target: ExprRef, value: ExprRef, depth: u32) {
        if !self.enter(depth) {
            return;
        }
        let source = self.source;
        match source.kind(target) {
            Some(ExprKind::Sequence(targets)) => {
                if let Some(ExprKind::Sequence(values)) = source.kind(value) {
                    for (&item, &assigned) in targets.iter().zip(values) {
                        self.record(Self::local(target, item), Self::local(value, assigned), depth + 1);
                    }
                }
            }
            Some(ExprKind::Member { .. } | ExprKind::Index { .. }) => {
                for (root, path) in self.locations(target) {
                    let values = self.stored.entry((root, path.clone())).or_default();
                    if values.is_empty() {
                        self.stored_paths.entry(root).or_default().push(path);
                    }
                    values.push(value);
                }
            }
            _ => {}
        }
    }
}
