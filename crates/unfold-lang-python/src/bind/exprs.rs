//! Interning the expressions binding evaluates into the closed vocabulary of
//! [`ExprKind`]. Expressions are reserved while the binder walks (so facts can name them before
//! the lambdas inside them have declarations) and filled afterwards from a queue, so nothing
//! recurses over the tree. Each expression is interned once, by its address; the names of a
//! string annotation, which have no expression of their own, are synthesized
//! ([`Interner::synthetic`]).

use std::borrow::Cow;

use ruff_python_ast::{ArgOrKeyword, Expr, Number};
use rustc_hash::FxHashMap;

use super::ir::{Argument, ExprFact, ExprId, ExprKind, Literal, NameId, NodeId, ScopeId};
use crate::ast::{arguments, string_literal};
use crate::lower::address;

/// The expression and name tables of one file under construction.
#[derive(Default)]
pub(super) struct Interner<'a> {
    names: Vec<Box<str>>,
    name_ids: FxHashMap<Cow<'a, str>, NameId>,
    exprs: Vec<ExprFact>,
    memo: FxHashMap<usize, ExprId>,
    queue: Vec<(ExprId, &'a Expr)>,
    lambdas: FxHashMap<usize, NodeId>,
    functions: FxHashMap<usize, NodeId>,
}

impl<'a> Interner<'a> {
    /// The ID of `name`, interning it.
    pub(super) fn name(&mut self, name: &'a str) -> NameId {
        self.name_text(Cow::Borrowed(name))
    }

    /// The ID of `name`, borrowed from the tree or (normalized) owned, interning it.
    pub(super) fn name_text(&mut self, name: Cow<'a, str>) -> NameId {
        if let Some(&id) = self.name_ids.get(&*name) {
            return id;
        }
        let id = self.names.len() as NameId;
        self.names.push((*name).into());
        self.name_ids.insert(name, id);
        id
    }

    /// A new expression of `kind` evaluated in `scope` that no tree node stands for.
    pub(super) fn synthetic(&mut self, kind: ExprKind, scope: ScopeId) -> ExprId {
        let id = self.exprs.len() as ExprId;
        self.exprs.push(ExprFact { kind, scope });
        id
    }

    /// The ID `expr` has (or will have once filled), evaluated in `scope`.
    pub(super) fn reserve(&mut self, expr: &'a Expr, scope: ScopeId) -> ExprId {
        let key = address(expr);
        if let Some(&id) = self.memo.get(&key) {
            return id;
        }
        let id = self.exprs.len() as ExprId;
        self.exprs.push(ExprFact {
            kind: ExprKind::Other,
            scope,
        });
        self.memo.insert(key, id);
        self.queue.push((id, expr));
        id
    }

    /// A synthetic outer chain level whose value is `value`'s.
    pub(super) fn chain(&mut self, value: ExprId, scope: ScopeId) -> ExprId {
        self.synthetic(ExprKind::Chain(value), scope)
    }

    /// Records the declaration of a lambda.
    pub(super) fn lambda(&mut self, expr: &'a Expr, node: NodeId) {
        self.lambdas.insert(address(expr), node);
    }

    /// Records the nearest function or lambda around the call `id`.
    pub(super) fn set_function(&mut self, id: ExprId, function: Option<NodeId>) {
        if let Some(function) = function {
            self.functions.insert(id as usize, function);
        }
    }

    /// The ID of an expression that was interned, or a new opaque one.
    pub(super) fn find(&mut self, expr: &Expr) -> ExprId {
        if let Some(&id) = self.memo.get(&address(expr)) {
            return id;
        }
        let id = self.exprs.len() as ExprId;
        self.exprs.push(ExprFact {
            kind: ExprKind::Other,
            scope: 0,
        });
        id
    }

    /// Fills every reserved expression, reserving the parts each one names.
    pub(super) fn fill(&mut self) {
        while let Some((id, expr)) = self.queue.pop() {
            let scope = self.exprs[id as usize].scope;
            let kind = self.convert(id, expr, scope);
            self.exprs[id as usize].kind = kind;
        }
    }

    fn convert(&mut self, id: ExprId, expr: &'a Expr, scope: ScopeId) -> ExprKind {
        match expr {
            Expr::Name(name) => ExprKind::Name(self.name(name.id.as_str())),
            Expr::Attribute(access) => ExprKind::Member {
                base: self.reserve(&access.value, scope),
                name: self.name(access.attr.id.as_str()),
            },
            Expr::Call(call) => {
                let callee = self.reserve(&call.func, scope);
                let written: Vec<ArgOrKeyword<'a>> = arguments(&call.arguments).collect();
                let arguments = written
                    .into_iter()
                    .map(|argument| {
                        let (keyword, value, expansion) = match argument {
                            ArgOrKeyword::Arg(Expr::Starred(starred)) => (None, &*starred.value, true),
                            ArgOrKeyword::Arg(expr) => (None, expr, false),
                            ArgOrKeyword::Keyword(keyword) => (
                                keyword.arg.as_ref().map(|name| name.id.as_str()),
                                &keyword.value,
                                keyword.arg.is_none(),
                            ),
                        };
                        Argument {
                            keyword: keyword.map(|name| self.name(name)),
                            value: self.reserve(value, scope),
                            expansion,
                        }
                    })
                    .collect();
                ExprKind::Call {
                    callee,
                    arguments,
                    function: self.functions.get(&(id as usize)).copied(),
                }
            }
            Expr::Subscript(subscript) => ExprKind::Index {
                base: self.reserve(&subscript.value, scope),
                key: string_literal(&subscript.slice).map(|key| self.name(key)),
            },
            Expr::Await(node) => ExprKind::Await(self.reserve(&node.value, scope)),
            Expr::If(_) => ExprKind::Ternary,
            Expr::StringLiteral(_) => {
                string_literal(expr).map_or(ExprKind::Other, |text| ExprKind::Str(self.name(text)))
            }
            Expr::List(list) => ExprKind::Sequence {
                items: list.elts.iter().map(|item| self.reserve(item, scope)).collect(),
                tuple: false,
            },
            Expr::Tuple(tuple) => ExprKind::Sequence {
                items: tuple.elts.iter().map(|item| self.reserve(item, scope)).collect(),
                tuple: true,
            },
            Expr::Dict(dict) => ExprKind::Dict(
                dict.items
                    .iter()
                    .map(|item| self.reserve(&item.value, scope))
                    .collect(),
            ),
            Expr::FString(_) => ExprKind::Literal(Literal::Str),
            Expr::BytesLiteral(_) => ExprKind::Literal(Literal::Bytes),
            Expr::NumberLiteral(number) => ExprKind::Literal(match number.value {
                Number::Int(_) => Literal::Int,
                Number::Float(_) => Literal::Float,
                Number::Complex { .. } => Literal::Complex,
            }),
            Expr::ListComp(_) => ExprKind::Literal(Literal::List),
            Expr::Set(_) | Expr::SetComp(_) => ExprKind::Literal(Literal::Set),
            Expr::DictComp(_) => ExprKind::Literal(Literal::Dict),
            Expr::Lambda(_) => self
                .lambdas
                .get(&address(expr))
                .map_or(ExprKind::Other, |&node| ExprKind::Lambda(node)),
            _ => ExprKind::Other,
        }
    }

    /// The finished name and expression tables.
    pub(super) fn finish(self) -> (Vec<Box<str>>, Vec<ExprFact>) {
        (self.names, self.exprs)
    }
}
