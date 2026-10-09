//! Decorators, proven by identity rather than spelling.
//!
//! A decorator's identity is the external origin its expression (or, for `@d(...)`, its
//! callee) resolves to, such as `builtins.staticmethod` or `functools.lru_cache`;
//! `builtins.property.setter` for `@x.setter` where `x` is a proven property; `unknown` for
//! anything else. A body stays concrete only when every decorator is transparent
//! (`staticmethod`, `classmethod`, `functools.cache`, `functools.lru_cache`, `typing.final`,
//! `typing.override`) or marks it as something other than a callable body (`typing.overload`,
//! `abc.abstractmethod`, `property` and its setter), which the policy handles separately.

use std::sync::Arc;

use super::binder::{Binder, Ctx, Guard, Identities};
use super::program::{ExprRef, NodeRef};
use super::value::dotted;
use crate::bind::ir::{ExprKind, NodeKind};

/// Decorators that keep the decorated body.
const TRANSPARENT: &[&str] = &[
    "builtins.staticmethod",
    "builtins.classmethod",
    "functools.cache",
    "functools.lru_cache",
    "typing.final",
    "typing.override",
];

/// Decorators that are not wrappers, though they change what the declaration is.
const MARKERS: &[&str] = &[
    "typing.overload",
    "abc.abstractmethod",
    "builtins.property",
    "builtins.property.setter",
];

const UNKNOWN: &str = "unknown";

impl Binder<'_> {
    /// The proven external identity of an expression (`module.member`).
    pub(crate) fn identity(&self, expr: ExprRef, ctx: &mut Ctx) -> Option<String> {
        let value = self.resolve(expr, ctx);
        if value.ambiguous || value.refusal.is_some() || !self.class_access_proven(&value, ctx) {
            return None;
        }
        let external = value.external?;
        Some(dotted(&external.module, &external.member))
    }

    fn decorator_exprs(&self, node: NodeRef) -> &[u32] {
        match self.program.node(node).map(|fact| &fact.kind) {
            Some(NodeKind::Function(function)) => &function.decorators,
            Some(NodeKind::Class(class)) => &class.decorators,
            _ => &[],
        }
    }

    /// The identities of a declaration's decorators, in written order.
    pub(crate) fn decorators(&self, node: NodeRef, ctx: &mut Ctx) -> Identities {
        if self.decorator_exprs(node).is_empty() {
            return Arc::from([]);
        }
        if let Some(found) = self.memo_decorators(node) {
            return found;
        }
        if ctx.guarded(Guard::Decorators(node)) {
            return Arc::from([Box::<str>::from(UNKNOWN)]);
        }
        if !ctx.enter() {
            return Arc::from([Box::<str>::from(UNKNOWN)]);
        }
        let (identities, independent) = ctx.with(Guard::Decorators(node), |ctx| {
            self.decorator_exprs(node)
                .iter()
                .map(|&expr| {
                    let decorator = ExprRef {
                        file: node.file,
                        expr,
                    };
                    self.decorator_identity(decorator, ctx)
                        .map_or_else(|| Box::<str>::from(UNKNOWN), Box::from)
                })
                .collect::<Identities>()
        });
        ctx.leave();
        if independent {
            self.keep_decorators(node, Arc::clone(&identities));
        }
        identities
    }

    fn decorator_identity(&self, decorator: ExprRef, ctx: &mut Ctx) -> Option<String> {
        let fact = self.program.expr(decorator)?;
        if let ExprKind::Member { base, name } = fact.kind
            && self.program.text(self.program.sym(decorator.file, name)) == "setter"
            && self.setter_of_property(decorator, base, ctx)
        {
            return Some("builtins.property.setter".to_owned());
        }
        let target = match fact.kind {
            ExprKind::Call { callee, .. } => ExprRef {
                file: decorator.file,
                expr: callee,
            },
            _ => decorator,
        };
        self.identity(target, ctx)
    }

    /// Whether `@x.setter` names `x` bound by a declaration another of whose decorators is a
    /// proven `property`.
    fn setter_of_property(&self, decorator: ExprRef, base: u32, ctx: &mut Ctx) -> bool {
        let file = decorator.file;
        let Some(base) = self.program.expr(ExprRef { file, expr: base }) else {
            return false;
        };
        let ExprKind::Name(name) = base.kind else {
            return false;
        };
        let Some(binding) = self
            .program
            .lookup(file, base.scope, self.program.sym(file, name))
        else {
            return false;
        };
        self.program.binding(binding).writes.iter().any(|&write| {
            let node = self.program.write(write).node;
            self.decorator_exprs(node).iter().any(|&item| {
                let item = ExprRef {
                    file: node.file,
                    expr: item,
                };
                item != decorator && self.identity(item, ctx).as_deref() == Some("builtins.property")
            })
        })
    }

    /// Whether a declaration has a decorator with this identity.
    pub(crate) fn has_decorator(&self, node: NodeRef, identity: &str, ctx: &mut Ctx) -> bool {
        self.decorators(node, ctx)
            .iter()
            .any(|found| &**found == identity)
    }

    pub(crate) fn overload(&self, node: NodeRef, ctx: &mut Ctx) -> bool {
        self.has_decorator(node, "typing.overload", ctx)
    }

    pub(crate) fn is_abstract(&self, node: NodeRef, ctx: &mut Ctx) -> bool {
        self.has_decorator(node, "abc.abstractmethod", ctx)
    }

    /// Whether a declaration is a property getter or setter.
    pub(crate) fn accessor(&self, node: NodeRef, ctx: &mut Ctx) -> bool {
        self.decorators(node, ctx)
            .iter()
            .any(|found| matches!(&**found, "builtins.property" | "builtins.property.setter"))
    }

    /// Whether a decorator other than the allowlist replaces the declaration (or, for a member,
    /// its class's decorators replace the class).
    pub(crate) fn wrapper(&self, node: NodeRef, ctx: &mut Ctx) -> bool {
        let mut current = Some(node);
        while let Some(declaration) = current {
            let is_class = self.program.is_class(declaration);
            if declaration != node && !is_class {
                break;
            }
            let wraps = self
                .decorators(declaration, ctx)
                .iter()
                .any(|identity| !TRANSPARENT.contains(&&**identity) && !MARKERS.contains(&&**identity));
            if wraps {
                return true;
            }
            current = if is_class {
                None
            } else {
                self.program.enclosing_definition(declaration)
            };
        }
        false
    }
}
