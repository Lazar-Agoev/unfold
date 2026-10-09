//! The negative census: stored locations, indexed subtypes, and every write or call that could
//! replace a framework API.

use rustc_hash::FxHashSet;

use super::{Census, DICT_CHANGES, Flow, Identity, Origins, Relation};
use crate::bind::ir::{Effect, ExprKind, NodeKind};
use crate::link::children::Raw;
use crate::link::program::ExprRef;

impl Origins<'_, '_, '_, '_> {
    /// An annotation admits indexed subtypes as actual arguments; without a proof that they
    /// keep the framework's registration APIs, annotation-only receivers of that family are
    /// uncertain.
    pub(super) fn collect_subtypes(&mut self) {
        let binder = self.source.binder;
        let mut families = Vec::new();
        for &class in &binder.program.classes {
            let Some(fact) = binder.class_fact(class) else {
                continue;
            };
            for argument in fact
                .arguments
                .iter()
                .filter(|argument| argument.keyword.is_none())
            {
                let base = Raw::Expr(ExprRef {
                    file: class.file,
                    expr: argument.value,
                });
                for identity in self.possible_imports(base).iter() {
                    if let Some(spec) = self.spec(identity) {
                        families.push(spec.family);
                    }
                }
            }
        }
        self.uncertain_annotations.extend(families);
    }

    pub(super) fn collect_mutations(&mut self) {
        let mut census = Census::default();
        for (file, effect) in self.effects() {
            match effect {
                Effect::Target {
                    target, augmented, ..
                } => {
                    let target = ExprRef { file, expr: target };
                    self.target(target, &mut census, 0);
                    if augmented {
                        self.taint(Raw::Expr(target), &mut census, &mut FxHashSet::default(), 0);
                    }
                }
                Effect::Call(call) => self.mutating_call(ExprRef { file, expr: call }, &mut census),
            }
        }
        self.invalid_families.extend(census.families);
        self.invalid_receivers.extend(census.receivers);
    }

    fn mutating_call(&self, call: ExprRef, census: &mut Census) {
        let source = self.source;
        let Some(callee) = source.callee(call) else {
            return;
        };
        let identities = self.possible_imports(Raw::Expr(callee));
        let setter = identities
            .iter()
            .any(|identity| matches!(&**identity, "builtins.setattr" | "builtins.delattr"));
        let mutator = identities.iter().any(|identity| {
            identity
                .strip_prefix("builtins.dict.")
                .is_some_and(|method| DICT_CHANGES.contains(&method))
        });
        for (parameter, applies) in [("object", setter), ("self", mutator)] {
            if !applies {
                continue;
            }
            let args = source.args(call);
            if args.uncertain() {
                for argument in args.all() {
                    self.taint_contents(Raw::Expr(argument.expr), census, &mut FxHashSet::default(), 0);
                }
            } else if let Some(object) = args.get(parameter, Some(0)) {
                self.taint(Raw::Expr(object), census, &mut FxHashSet::default(), 0);
            }
        }
        // Exempt only this exact written call; its arguments stay in the census.
        if !self.safe_update(call) {
            self.mutation_method(Raw::Expr(callee), census, &mut FxHashSet::default(), 0);
        }
    }

    /// A written target: an attribute or item write mutates its receiver.
    fn target(&self, expr: ExprRef, census: &mut Census, depth: u32) {
        if !self.enter(depth) {
            return;
        }
        match self.source.kind(expr) {
            Some(&(ExprKind::Member { base, .. } | ExprKind::Index { base, .. })) => {
                self.taint(
                    Raw::Expr(Flow::local(expr, base)),
                    census,
                    &mut FxHashSet::default(),
                    0,
                );
            }
            Some(ExprKind::Sequence(items)) => {
                for &item in items {
                    self.target(Flow::local(expr, item), census, depth + 1);
                }
            }
            Some(&ExprKind::Starred(inner)) => self.target(Flow::local(expr, inner), census, depth + 1),
            _ => {}
        }
    }

    fn taint(&self, raw: Raw, census: &mut Census, seen: &mut FxHashSet<Raw>, depth: u32) {
        if !self.enter(depth) || !seen.insert(raw) {
            return;
        }
        let next = depth + 1;
        for value in self.stored_at(raw, Relation::Prefix) {
            self.taint(Raw::Expr(value), census, seen, next);
        }
        if let Some(receiver) = self.unguarded_receiver(raw) {
            if receiver.annotation_only {
                census.families.insert(receiver.spec.family);
            } else {
                census.receivers.insert(receiver.identity);
            }
            return;
        }
        if let Raw::Expr(expr) = raw
            && let Some(identity) = self.imported(Some(expr))
            && let Some(spec) = self.specs.iter().find(|spec| {
                spec.imports.iter().any(|name| {
                    *identity == **name
                        || identity
                            .strip_prefix(name)
                            .is_some_and(|rest| rest.starts_with('.'))
                })
            })
        {
            census.families.insert(spec.family);
            return;
        }
        let source = self.source;
        let binder = source.binder;
        match raw {
            Raw::Expr(expr) => match source.kind(expr) {
                // Writes through `__dict__`, a delegated router, or another field can replace a
                // delegated API: they can't preserve the containing receiver's proof.
                Some(&(ExprKind::Member { base, .. } | ExprKind::Index { base, .. })) => {
                    self.taint(Raw::Expr(Flow::local(expr, base)), census, seen, next);
                }
                Some(ExprKind::Sequence(items) | ExprKind::Dict(items)) => {
                    for &item in items {
                        self.taint(Raw::Expr(Flow::local(expr, item)), census, seen, next);
                    }
                }
                Some(&(ExprKind::Starred(inner) | ExprKind::Chain(inner))) => {
                    self.taint(Raw::Expr(Flow::local(expr, inner)), census, seen, next);
                }
                Some(ExprKind::Name(_)) => self.taint_writes(raw, census, seen, next),
                _ => {}
            },
            Raw::Node(node) => match binder.program.node(node).map(|fact| &fact.kind) {
                Some(&NodeKind::Assignment {
                    value: Some(value), ..
                }) => {
                    let value = self.unchain(ExprRef {
                        file: node.file,
                        expr: value,
                    });
                    let constructed = matches!(source.kind(value), Some(ExprKind::Call { .. }))
                        && self.spec_of(source.callee(value)).is_some();
                    if constructed {
                        census.receivers.insert(Identity::Node(node));
                    } else {
                        self.taint(Raw::Expr(value), census, seen, next);
                    }
                }
                Some(&NodeKind::Assignment {
                    value: None,
                    annotation: Some(annotation),
                    ..
                })
                | Some(&NodeKind::Parameter {
                    annotation: Some(annotation),
                    ..
                }) => {
                    let annotation = ExprRef {
                        file: node.file,
                        expr: annotation,
                    };
                    self.taint(Raw::Expr(annotation), census, seen, next);
                }
                Some(NodeKind::Import) => self.taint_writes(raw, census, seen, next),
                _ => {}
            },
        }
    }

    /// Negative evidence follows every possible write of a name, rebound aliases included.
    fn taint_writes(&self, raw: Raw, census: &mut Census, seen: &mut FxHashSet<Raw>, depth: u32) {
        let binder = self.source.binder;
        for write in self.writes(raw) {
            let found = binder.program.write(write);
            for expr in [found.value, found.annotation].into_iter().flatten() {
                self.taint(Raw::Expr(expr), census, seen, depth);
            }
            if let Some(identity) = self.external_import(write) {
                if let Some(spec) = self.spec(&identity) {
                    census.families.insert(spec.family);
                }
                continue;
            }
            for declaration in self.imported_declarations(write, true) {
                self.taint(Raw::Node(declaration), census, seen, depth);
            }
        }
    }

    /// Taints a value and everything stored in or held by it.
    fn taint_contents(&self, raw: Raw, census: &mut Census, seen: &mut FxHashSet<Raw>, depth: u32) {
        if !self.enter(depth) || !seen.insert(raw) {
            return;
        }
        let next = depth + 1;
        self.taint(raw, census, &mut FxHashSet::default(), 0);
        for value in self.stored_at(raw, Relation::Extends) {
            self.taint_contents(Raw::Expr(value), census, seen, next);
        }
        let Raw::Expr(expr) = raw else {
            return;
        };
        match self.source.kind(expr) {
            Some(ExprKind::Name(_)) => {
                for write in self.writes(raw) {
                    if let Some(value) = self.source.binder.program.write(write).value {
                        self.taint_contents(Raw::Expr(value), census, seen, next);
                    }
                }
            }
            Some(ExprKind::Sequence(items) | ExprKind::Dict(items)) => {
                for &item in items {
                    self.taint_contents(Raw::Expr(Flow::local(expr, item)), census, seen, next);
                }
            }
            _ => {}
        }
    }

    /// A called mutating method (`d.update`, a stored or aliased `d.__setitem__`) mutates its
    /// receiver.
    fn mutation_method(&self, raw: Raw, census: &mut Census, seen: &mut FxHashSet<Raw>, depth: u32) {
        if !self.enter(depth) || !seen.insert(raw) {
            return;
        }
        let next = depth + 1;
        for value in self.stored_at(raw, Relation::Exact) {
            self.mutation_method(Raw::Expr(value), census, seen, next);
        }
        let source = self.source;
        match raw {
            Raw::Expr(expr) => match source.kind(expr) {
                Some(ExprKind::Member { .. }) => {
                    if let Some((base, name)) = source.member(expr)
                        && DICT_CHANGES.contains(&name)
                    {
                        self.taint(Raw::Expr(base), census, &mut FxHashSet::default(), 0);
                    }
                }
                Some(ExprKind::Name(_)) => self.mutation_writes(raw, census, seen, next),
                _ => {}
            },
            Raw::Node(node) => match source.binder.program.node(node).map(|fact| &fact.kind) {
                Some(&NodeKind::Assignment {
                    value: Some(value), ..
                }) => {
                    let value = ExprRef {
                        file: node.file,
                        expr: value,
                    };
                    self.mutation_method(Raw::Expr(value), census, seen, next);
                }
                Some(NodeKind::Import) => self.mutation_writes(raw, census, seen, next),
                _ => {}
            },
        }
    }

    fn mutation_writes(&self, raw: Raw, census: &mut Census, seen: &mut FxHashSet<Raw>, depth: u32) {
        for write in self.writes(raw) {
            if let Some(value) = self.source.binder.program.write(write).value {
                self.mutation_method(Raw::Expr(value), census, seen, depth);
            }
            for declaration in self.imported_declarations(write, false) {
                self.mutation_method(Raw::Node(declaration), census, seen, depth);
            }
        }
    }

    /// Whether a call is an allowed update: `<proven receiver>.<path>(...)` with only unique,
    /// allowed keywords.
    fn safe_update(&self, call: ExprRef) -> bool {
        let source = self.source;
        for spec in self.specs {
            for rule in spec.safe_updates {
                let mut receiver = source.callee(call);
                for member in rule.path.iter().rev() {
                    receiver = receiver
                        .and_then(|expr| source.member(expr))
                        .filter(|(_, name)| name == member)
                        .map(|(base, _)| base);
                }
                let Some(origin) = receiver.and_then(|expr| self.unguarded_receiver(Raw::Expr(expr))) else {
                    continue;
                };
                if origin.annotation_only || !std::ptr::eq(origin.spec, spec) {
                    continue;
                }
                let args = source.args(call);
                let mut keywords: Vec<&str> = Vec::new();
                let allowed = args.all().iter().all(|argument| match argument.keyword {
                    Some(keyword)
                        if !argument.expansion
                            && !keywords.contains(&keyword)
                            && rule.keywords.contains(&keyword) =>
                    {
                        keywords.push(keyword);
                        true
                    }
                    _ => false,
                });
                if allowed {
                    return true;
                }
            }
        }
        false
    }
}
