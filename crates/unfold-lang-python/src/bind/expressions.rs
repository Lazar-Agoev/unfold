//! The binder pass over expressions and patterns: calls (the mutation passes' inputs), lambdas,
//! comprehensions and their scopes, assignment expressions, and pattern captures.

use ruff_python_ast::{Comprehension, Expr, Pattern};

use super::build::{Builder, Ctx};
use super::ir::{Effect, FunctionFact, NodeKind, ScopeKind, TargetKind, WriteFact, WriteKind};
use crate::ast::{arguments, expr_children};
use crate::lower::address;

impl<'a> Builder<'a> {
    /// Pattern captures (`case x`, `case [*rest]`, `case {**rest}`, `case P() as x`, and the
    /// wildcard `_`, which the proof of concept bound) write in the case's scope.
    pub(super) fn pattern(&mut self, pattern: &'a Pattern, ctx: Ctx) {
        let mut pending = vec![pattern];
        let mut names: Vec<&'a str> = Vec::new();
        while let Some(pattern) = pending.pop() {
            match pattern {
                Pattern::MatchAs(node) => {
                    pending.extend(node.pattern.as_deref());
                    match (&node.name, &node.pattern) {
                        (Some(name), _) => names.push(name.id.as_str()),
                        (None, None) => names.push("_"),
                        (None, Some(_)) => {}
                    }
                }
                Pattern::MatchStar(node) => {
                    names.push(node.name.as_ref().map_or("_", |name| name.id.as_str()));
                }
                Pattern::MatchMapping(node) => {
                    pending.extend(node.patterns.iter().rev());
                    names.extend(node.rest.as_ref().map(|rest| rest.id.as_str()));
                }
                Pattern::MatchSequence(node) => pending.extend(node.patterns.iter().rev()),
                Pattern::MatchClass(node) => {
                    pending.extend(
                        node.arguments
                            .keywords
                            .iter()
                            .rev()
                            .map(|keyword| &keyword.pattern),
                    );
                    pending.extend(node.arguments.patterns.iter().rev());
                }
                Pattern::MatchOr(node) => pending.extend(node.patterns.iter().rev()),
                Pattern::MatchValue(_) | Pattern::MatchSingleton(_) => {}
            }
        }
        for name in names {
            let node = self.node(NodeKind::Other, ctx);
            self.simple_write(ctx, name, node, WriteKind::Other);
        }
    }

    pub(super) fn expr(&mut self, expr: &'a Expr, ctx: Ctx) {
        match expr {
            Expr::Call(call) => {
                let id = self.reserve(expr, ctx.scope);
                self.interner.set_function(id, ctx.function);
                self.effects.push(Effect::Call(id));
                let mut children = vec![(&*call.func, ctx)];
                children.extend(arguments(&call.arguments).map(|argument| (argument.value(), ctx)));
                self.visit(children);
            }
            Expr::Lambda(lambda) => {
                let body = self.scope(ScopeKind::Function, Some(ctx.scope));
                let first_parameter = lambda
                    .parameters
                    .as_deref()
                    .and_then(|parameters| self.first_parameter(parameters));
                let kind = NodeKind::Function(FunctionFact {
                    local: self.inputs.definitions.get(&address(expr)).copied(),
                    name: None,
                    decorators: Vec::new(),
                    parameters: Vec::new(),
                    first_parameter,
                    body,
                    asynchronous: false,
                    returns: None,
                });
                let node = self.node(kind, ctx);
                self.interner.lambda(expr, node);
                let header = Ctx {
                    function: Some(node),
                    enclosing: Some(node),
                    conditional: false,
                    ..ctx
                };
                if let Some(parameters) = lambda.parameters.as_deref() {
                    self.parameters(parameters, node, body, header);
                }
                self.visit([(&*lambda.body, header.in_scope(body))]);
                if let Some(parameters) = lambda.parameters.as_deref() {
                    let defaults: Vec<(&'a Expr, Ctx)> = parameters
                        .iter()
                        .filter_map(|parameter| parameter.default().map(|default| (default, header)))
                        .collect();
                    self.visit(defaults);
                }
            }
            Expr::ListComp(node) => self.comprehension(&[&*node.elt], &node.generators, ctx),
            Expr::SetComp(node) => self.comprehension(&[&*node.elt], &node.generators, ctx),
            Expr::Generator(node) => self.comprehension(&[&*node.elt], &node.generators, ctx),
            Expr::DictComp(node) => {
                let elements: Vec<&'a Expr> = node.key.as_deref().into_iter().chain([&*node.value]).collect();
                self.comprehension(&elements, &node.generators, ctx);
            }
            Expr::Named(named) => {
                let value = self.reserve(&named.value, ctx.scope);
                let id = self.node(
                    NodeKind::Assignment {
                        value: Some(value),
                        annotation: None,
                        factory: self.factory(&named.value),
                    },
                    ctx,
                );
                if let Expr::Name(target) = &*named.target {
                    self.write(
                        ctx,
                        target.id.as_str(),
                        true,
                        WriteFact {
                            node: id,
                            kind: WriteKind::Assignment,
                            value: Some(value),
                            annotation: None,
                            import: None,
                            conditional: ctx.conditional,
                        },
                    );
                }
                self.visit([(&*named.target, ctx), (&*named.value, ctx)]);
            }
            _ => {
                let mut children: Vec<&'a Expr> = Vec::new();
                expr_children(expr, &mut children);
                self.visit(children.into_iter().map(|child| (child, ctx)));
            }
        }
    }

    /// A comprehension: its first iterable is evaluated outside, everything else (targets,
    /// conditions, elements) in its own scope.
    pub(super) fn comprehension(&mut self, elements: &[&'a Expr], generators: &'a [Comprehension], ctx: Ctx) {
        let scope = self.scope(ScopeKind::Comprehension, Some(ctx.scope));
        let inside = ctx.in_scope(scope);
        let mut children: Vec<(&'a Expr, Ctx)> = elements.iter().map(|&element| (element, inside)).collect();
        for (index, generator) in generators.iter().enumerate() {
            let id = self.node(NodeKind::Other, inside);
            self.names(&generator.target, inside, id, WriteKind::Other, None, None);
            self.target_effect(&generator.target, TargetKind::Other, None, None, inside, false);
            children.push((&generator.target, inside));
            children.push((&generator.iter, if index == 0 { ctx } else { inside }));
            children.extend(generator.ifs.iter().map(|test| (test, inside)));
        }
        self.visit(children);
    }
}
