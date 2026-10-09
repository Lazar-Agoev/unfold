//! Withholding queries: every identity, container value, and framework family a value may
//! have. Detectors use them only to withhold proof.

use std::rc::Rc;

use rustc_hash::FxHashSet;

use super::{Flow, Origins};
use crate::bind::ir::{ExprKind, NodeKind};
use crate::link::binder::Ctx;
use crate::link::children::Raw;
use crate::link::program::ExprRef;

impl Origins<'_, '_, '_, '_> {
    /// Whether `raw` may belong to `family` (see [`Origins::possible_families`]). A family no
    /// import of the program can name is never possible.
    pub(in crate::detect) fn may_be(&self, raw: Raw, family: &str) -> bool {
        self.reachable.contains(family) && self.possible_families(raw).contains(family)
    }

    /// Every framework family `raw` may belong to, including uncertain aliases and subclasses.
    /// For withholding only.
    fn possible_families(&self, raw: Raw) -> Rc<FxHashSet<&'static str>> {
        let raw = self.canonical(raw);
        if let Some(found) = self.families.borrow().get(&raw) {
            return Rc::clone(found);
        }
        let mut families = FxHashSet::default();
        self.visit_families(raw, &mut families, &mut FxHashSet::default(), 0);
        if self.exhausted.get() {
            families.extend(self.specs.iter().map(|spec| spec.family));
        }
        let families = Rc::new(families);
        self.families.borrow_mut().insert(raw, Rc::clone(&families));
        families
    }

    fn visit_families(
        &self,
        raw: Raw,
        families: &mut FxHashSet<&'static str>,
        seen: &mut FxHashSet<Raw>,
        depth: u32,
    ) {
        if !self.enter(depth) || !seen.insert(raw) {
            return;
        }
        let next = depth + 1;
        let source = self.source;
        let binder = source.binder;
        if let Some(receiver) = self.unguarded_receiver(raw) {
            families.insert(receiver.spec.family);
        }
        for identity in self.possible_imports(raw).iter() {
            if let Some(spec) = self.spec(identity) {
                families.insert(spec.family);
            }
        }
        let visit = |raw: Raw, families: &mut FxHashSet<&'static str>, seen: &mut FxHashSet<Raw>| {
            self.visit_families(raw, families, seen, next);
        };
        match raw {
            Raw::Expr(expr) => {
                for value in self.stored_values(expr) {
                    visit(Raw::Expr(value), families, seen);
                }
                match source.kind(expr) {
                    Some(ExprKind::Sequence(_) | ExprKind::Dict(_)) => {
                        for &value in self.container_values(raw).iter() {
                            visit(Raw::Expr(value), families, seen);
                        }
                    }
                    Some(
                        &(ExprKind::Call { callee: base, .. }
                        | ExprKind::Member { base, .. }
                        | ExprKind::Chain(base)),
                    ) => visit(Raw::Expr(Flow::local(expr, base)), families, seen),
                    _ => {}
                }
                for &declaration in &binder.resolve(expr, &mut Ctx::new()).declarations {
                    visit(Raw::Node(declaration), families, seen);
                }
            }
            Raw::Node(node) => match binder.program.node(node).map(|fact| &fact.kind) {
                Some(NodeKind::Class(class)) => {
                    for argument in class
                        .arguments
                        .iter()
                        .filter(|argument| argument.keyword.is_none())
                    {
                        let base = ExprRef {
                            file: node.file,
                            expr: argument.value,
                        };
                        visit(Raw::Expr(base), families, seen);
                    }
                }
                Some(&NodeKind::Assignment {
                    value, annotation, ..
                }) => {
                    for expr in [value, annotation].into_iter().flatten() {
                        visit(
                            Raw::Expr(ExprRef {
                                file: node.file,
                                expr,
                            }),
                            families,
                            seen,
                        );
                    }
                }
                Some(&NodeKind::Parameter {
                    annotation: Some(annotation),
                    ..
                }) => visit(
                    Raw::Expr(ExprRef {
                        file: node.file,
                        expr: annotation,
                    }),
                    families,
                    seen,
                ),
                _ => {}
            },
        }
        let is_name = match raw {
            Raw::Expr(expr) => matches!(source.kind(expr), Some(ExprKind::Name(_))),
            Raw::Node(_) => true,
        };
        if !is_name {
            return;
        }
        for write in self.writes(raw) {
            let found = binder.program.write(write);
            for expr in [found.value, found.annotation].into_iter().flatten() {
                visit(Raw::Expr(expr), families, seen);
            }
            for declaration in self.imported_declarations(write, false) {
                visit(Raw::Node(declaration), families, seen);
            }
        }
    }
}
