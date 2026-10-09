//! The binder's extraction pass: one iterative walk over a parsed module that records scopes,
//! declarations, the complete write census, star imports, and the mutation passes' inputs, then
//! interns every expression they mention ([`super::exprs`]) and resolves `global` and `nonlocal`
//! redirection.
//!
//! Scopes follow the proof of concept's binder: a `def`'s decorators and name belong to the
//! enclosing scope; with type parameters, its annotations, defaults, and return annotation are
//! evaluated in the type-parameter scope, otherwise in the enclosing scope; its parameters and
//! body are in its own scope. A class's bases and keywords are evaluated like annotations. A
//! comprehension's first iterable is evaluated outside it. A write is *conditional* when an
//! `if`, `try`, `match`, `for`, or `while` inside the same function, class, or module contains
//! it (the loop's own targets are not inside the loop).

use ruff_python_ast::{Alias, ArgOrKeyword, Expr, ModModule, Stmt};
use rustc_hash::FxHashMap;

use super::exprs::Interner;
use super::ir::{
    BindFacts, ClassFact, Effect, ExprId, ImportWrite, MODULE_SCOPE, ModuleSpec, NodeFact, NodeId, NodeKind,
    ScopeId, ScopeKind, StarFact, TargetKind, WriteFact, WriteKind,
};
use crate::ast::arguments;
use crate::values::address;

/// What the binder pass reads besides the module.
pub(crate) struct Inputs<'a> {
    /// The block of every selected `def`, `class`, and `lambda`, by the node's address.
    pub(crate) blocks: &'a FxHashMap<usize, u32>,
    /// The expression of each value-table entry, in table order.
    pub(crate) values: &'a [&'a Expr],
    /// For each call that is a `call` block, its callee's index in the value table, by the
    /// call's address.
    pub(crate) callees: &'a FxHashMap<usize, u32>,
}

/// The binder facts, and the declaration of each `from` import name (by the alias's address).
pub(crate) struct Built {
    pub(crate) facts: BindFacts,
    pub(crate) aliases: FxHashMap<usize, NodeId>,
}

/// What a node inherits from its ancestors.
#[derive(Debug, Clone, Copy)]
pub(super) struct Ctx {
    pub(super) scope: ScopeId,
    /// The nearest enclosing function or lambda.
    pub(super) function: Option<NodeId>,
    /// The nearest enclosing function, class, or lambda.
    pub(super) enclosing: Option<NodeId>,
    pub(super) conditional: bool,
}

impl Ctx {
    pub(super) fn conditional(self) -> Self {
        Self {
            conditional: true,
            ..self
        }
    }

    pub(super) fn in_scope(self, scope: ScopeId) -> Self {
        Self { scope, ..self }
    }
}

pub(super) enum Task<'a> {
    Stmt(&'a Stmt, Ctx),
    Expr(&'a Expr, Ctx),
}

/// A write before `global`/`nonlocal` redirection.
pub(super) struct RawWrite<'a> {
    pub(super) scope: ScopeId,
    pub(super) name: &'a str,
    pub(super) walrus: bool,
    pub(super) write: WriteFact,
}

pub(super) struct Scope<'a> {
    pub(super) kind: ScopeKind,
    pub(super) parent: Option<ScopeId>,
    pub(super) globals: Vec<&'a str>,
    pub(super) nonlocals: Vec<&'a str>,
    /// Names written here lexically, before redirection.
    pub(super) lexical: rustc_hash::FxHashSet<&'a str>,
}

pub(super) struct Builder<'a> {
    pub(super) inputs: &'a Inputs<'a>,
    pub(super) interner: Interner<'a>,
    pub(super) scopes: Vec<Scope<'a>>,
    pub(super) nodes: Vec<NodeFact>,
    pub(super) writes: Vec<RawWrite<'a>>,
    pub(super) stars: Vec<StarFact>,
    pub(super) effects: Vec<Effect>,
    pub(super) aliases: FxHashMap<usize, NodeId>,
    pub(super) stack: Vec<Task<'a>>,
}

/// Runs the binder pass over `module`.
pub(crate) fn build<'a>(module: &'a ModModule, inputs: &'a Inputs<'a>) -> Built {
    let mut builder = Builder {
        inputs,
        interner: Interner::default(),
        scopes: Vec::new(),
        nodes: Vec::new(),
        writes: Vec::new(),
        stars: Vec::new(),
        effects: Vec::new(),
        aliases: FxHashMap::default(),
        stack: Vec::new(),
    };
    builder.scope(ScopeKind::Module, None);
    let root = Ctx {
        scope: MODULE_SCOPE,
        function: None,
        enclosing: None,
        conditional: false,
    };
    builder.statements(&module.body, root);
    while let Some(task) = builder.stack.pop() {
        match task {
            Task::Stmt(stmt, ctx) => builder.stmt(stmt, ctx),
            Task::Expr(expr, ctx) => builder.expr(expr, ctx),
        }
    }
    builder.finish()
}

impl<'a> Builder<'a> {
    pub(super) fn scope(&mut self, kind: ScopeKind, parent: Option<ScopeId>) -> ScopeId {
        let id = self.scopes.len() as ScopeId;
        self.scopes.push(Scope {
            kind,
            parent,
            globals: Vec::new(),
            nonlocals: Vec::new(),
            lexical: rustc_hash::FxHashSet::default(),
        });
        id
    }

    pub(super) fn node(&mut self, kind: NodeKind, ctx: Ctx) -> NodeId {
        let id = self.nodes.len() as NodeId;
        self.nodes.push(NodeFact {
            kind,
            scope: ctx.scope,
            enclosing: ctx.enclosing,
        });
        id
    }

    pub(super) fn reserve(&mut self, expr: &'a Expr, scope: ScopeId) -> ExprId {
        self.interner.reserve(expr, scope)
    }

    pub(super) fn write(&mut self, ctx: Ctx, name: &'a str, walrus: bool, write: WriteFact) {
        self.scopes[ctx.scope as usize].lexical.insert(name);
        self.writes.push(RawWrite {
            scope: ctx.scope,
            name,
            walrus,
            write,
        });
    }

    pub(super) fn simple_write(&mut self, ctx: Ctx, name: &'a str, node: NodeId, kind: WriteKind) {
        self.write(
            ctx,
            name,
            false,
            WriteFact {
                node,
                kind,
                value: None,
                annotation: None,
                import: None,
                conditional: ctx.conditional,
            },
        );
    }

    pub(super) fn statements(&mut self, body: &'a [Stmt], ctx: Ctx) {
        self.stack
            .extend(body.iter().rev().map(|stmt| Task::Stmt(stmt, ctx)));
    }

    /// Queues expressions to visit in the given order.
    pub(super) fn visit(&mut self, exprs: impl IntoIterator<Item = (&'a Expr, Ctx)>) {
        let start = self.stack.len();
        self.stack
            .extend(exprs.into_iter().map(|(expr, ctx)| Task::Expr(expr, ctx)));
        self.stack[start..].reverse();
    }

    pub(super) fn stmt(&mut self, stmt: &'a Stmt, ctx: Ctx) {
        match stmt {
            Stmt::FunctionDef(function) => self.function(stmt, function, ctx),
            Stmt::ClassDef(class) => {
                let scope = ctx.scope;
                let type_scope = self.type_scope(class.type_params.as_deref(), ctx);
                let body = self.scope(ScopeKind::Class, Some(type_scope));
                let decorators = class
                    .decorator_list
                    .iter()
                    .map(|decorator| self.reserve(&decorator.expression, scope))
                    .collect();
                let written: Vec<ArgOrKeyword<'a>> = class
                    .arguments
                    .as_deref()
                    .map_or_else(Vec::new, |list| arguments(list).collect());
                let arguments = written
                    .iter()
                    .map(|&argument| self.argument(argument, type_scope))
                    .collect();
                let kind = NodeKind::Class(ClassFact {
                    block: self.block(stmt),
                    name: self.interner.name(class.name.id.as_str()),
                    decorators,
                    arguments,
                    body,
                });
                let node = self.node(kind, ctx);
                self.simple_write(ctx, class.name.id.as_str(), node, WriteKind::Definition);
                let header = Ctx {
                    enclosing: Some(node),
                    conditional: false,
                    ..ctx
                };
                let inside = Ctx {
                    scope: body,
                    enclosing: Some(node),
                    conditional: false,
                    function: ctx.function,
                };
                self.statements(&class.body, inside);
                self.visit(
                    written
                        .iter()
                        .map(|argument| (argument.value(), header.in_scope(type_scope))),
                );
                self.type_parameter_values(class.type_params.as_deref(), header.in_scope(type_scope));
                self.visit(
                    class
                        .decorator_list
                        .iter()
                        .map(|decorator| (&decorator.expression, header)),
                );
            }
            Stmt::Return(node) => self.visit(node.value.as_deref().map(|value| (value, ctx))),
            Stmt::Delete(node) => {
                let id = self.node(NodeKind::Other, ctx);
                for target in &node.targets {
                    self.names(target, ctx, id, WriteKind::Other, None, None);
                    self.target_effect(target, TargetKind::Other, None, None, ctx, false);
                }
                self.visit(node.targets.iter().map(|target| (target, ctx)));
            }
            Stmt::TypeAlias(node) => {
                let id = self.node(NodeKind::Other, ctx);
                self.names(&node.name, ctx, id, WriteKind::Other, None, None);
                let type_scope = self.type_scope(node.type_params.as_deref(), ctx);
                self.visit([(&*node.value, ctx.in_scope(type_scope))]);
                self.type_parameter_values(node.type_params.as_deref(), ctx.in_scope(type_scope));
            }
            Stmt::Assign(node) => self.assignment(node, ctx),
            Stmt::AugAssign(node) => {
                let id = self.node(NodeKind::Other, ctx);
                self.names(&node.target, ctx, id, WriteKind::Other, None, None);
                self.target_effect(&node.target, TargetKind::Other, None, None, ctx, true);
                self.visit([(&*node.target, ctx), (&*node.value, ctx)]);
            }
            Stmt::AnnAssign(node) => {
                let annotation = self.reserve(&node.annotation, ctx.scope);
                let value = node.value.as_deref().map(|value| self.reserve(value, ctx.scope));
                let factory = node.value.as_deref().and_then(|value| self.factory(value));
                let id = self.node(
                    NodeKind::Assignment {
                        value,
                        annotation: Some(annotation),
                        factory,
                    },
                    ctx,
                );
                self.names(
                    &node.target,
                    ctx,
                    id,
                    WriteKind::Assignment,
                    value,
                    Some(annotation),
                );
                if value.is_some() {
                    self.target_effect(
                        &node.target,
                        TargetKind::Assignment,
                        value,
                        Some(annotation),
                        ctx,
                        false,
                    );
                }
                self.target_effect(
                    &node.target,
                    TargetKind::Annotation,
                    None,
                    Some(annotation),
                    ctx,
                    false,
                );
                let mut children = vec![(&*node.target, ctx), (&*node.annotation, ctx)];
                children.extend(node.value.as_deref().map(|value| (value, ctx)));
                self.visit(children);
            }
            Stmt::For(node) => {
                let id = self.node(NodeKind::Other, ctx);
                self.names(&node.target, ctx, id, WriteKind::Other, None, None);
                self.target_effect(&node.target, TargetKind::Other, None, None, ctx, false);
                let inside = ctx.conditional();
                self.statements(&node.orelse, inside);
                self.statements(&node.body, inside);
                self.visit([(&*node.target, inside), (&*node.iter, inside)]);
            }
            Stmt::While(node) => {
                let inside = ctx.conditional();
                self.statements(&node.orelse, inside);
                self.statements(&node.body, inside);
                self.visit([(&*node.test, inside)]);
            }
            Stmt::If(node) => {
                let inside = ctx.conditional();
                for clause in node.elif_else_clauses.iter().rev() {
                    self.statements(&clause.body, inside);
                    self.visit(clause.test.as_ref().map(|test| (test, inside)));
                }
                self.statements(&node.body, inside);
                self.visit([(&*node.test, inside)]);
            }
            Stmt::With(node) => {
                let mut children = Vec::new();
                for item in &node.items {
                    children.push((&item.context_expr, ctx));
                    if let Some(target) = item.optional_vars.as_deref() {
                        let id = self.node(NodeKind::Other, ctx);
                        self.names(target, ctx, id, WriteKind::Other, None, None);
                        self.target_effect(target, TargetKind::Other, None, None, ctx, false);
                        children.push((target, ctx));
                    }
                }
                self.statements(&node.body, ctx);
                self.visit(children);
            }
            Stmt::Match(node) => {
                let inside = ctx.conditional();
                for case in node.cases.iter().rev() {
                    self.statements(&case.body, inside);
                    self.visit(case.guard.as_deref().map(|guard| (guard, inside)));
                    self.pattern(&case.pattern, inside);
                }
                self.visit([(&*node.subject, inside)]);
            }
            Stmt::Raise(node) => {
                let mut children: Vec<(&'a Expr, Ctx)> =
                    node.exc.as_deref().map(|exc| (exc, ctx)).into_iter().collect();
                children.extend(node.cause.as_deref().map(|cause| (cause, ctx)));
                self.visit(children);
            }
            Stmt::Try(node) => {
                let inside = ctx.conditional();
                self.statements(&node.finalbody, inside);
                self.statements(&node.orelse, inside);
                for ruff_python_ast::ExceptHandler::ExceptHandler(handler) in node.handlers.iter().rev() {
                    self.statements(&handler.body, inside);
                    if let Some(name) = &handler.name {
                        let id = self.node(NodeKind::Other, inside);
                        self.simple_write(inside, name.id.as_str(), id, WriteKind::Other);
                    }
                    self.visit(handler.type_.as_deref().map(|kind| (kind, inside)));
                }
                self.statements(&node.body, inside);
            }
            Stmt::Assert(node) => {
                let mut children = vec![(&*node.test, ctx)];
                children.extend(node.msg.as_deref().map(|message| (message, ctx)));
                self.visit(children);
            }
            Stmt::Import(node) => {
                for alias in &node.names {
                    self.import(alias, ctx);
                }
            }
            Stmt::ImportFrom(node) => {
                let module = ModuleSpec {
                    level: node.level,
                    module: node
                        .module
                        .as_ref()
                        .map_or_else(Box::default, |module| module.id.as_str().into()),
                };
                if node.names.iter().any(|alias| alias.name.id.as_str() == "*") {
                    let id = self.node(NodeKind::Import, ctx);
                    self.stars.push(StarFact {
                        scope: ctx.scope,
                        module,
                        node: id,
                        conditional: ctx.conditional,
                    });
                    return;
                }
                for alias in &node.names {
                    let id = self.node(NodeKind::Import, ctx);
                    self.aliases.insert(alias_address(alias), id);
                    let bound = alias.asname.as_ref().unwrap_or(&alias.name).id.as_str();
                    let member = Some(self.interner.name(alias.name.id.as_str()));
                    self.import_write(ctx, bound, id, module.clone(), member);
                }
            }
            Stmt::Global(node) => {
                let scope = &mut self.scopes[ctx.scope as usize];
                scope
                    .globals
                    .extend(node.names.iter().map(|name| name.id.as_str()));
            }
            Stmt::Nonlocal(node) => {
                let scope = &mut self.scopes[ctx.scope as usize];
                scope
                    .nonlocals
                    .extend(node.names.iter().map(|name| name.id.as_str()));
            }
            Stmt::Expr(node) => self.visit([(&*node.value, ctx)]),
            Stmt::Pass(_) | Stmt::Break(_) | Stmt::Continue(_) | Stmt::IpyEscapeCommand(_) => {}
        }
    }

    pub(super) fn block(&self, node: &Stmt) -> u32 {
        self.inputs
            .blocks
            .get(&(std::ptr::from_ref(node) as usize))
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn argument(&mut self, argument: ArgOrKeyword<'a>, scope: ScopeId) -> super::ir::Argument {
        let (keyword, value, expansion) = match argument {
            ArgOrKeyword::Arg(Expr::Starred(starred)) => (None, &*starred.value, true),
            ArgOrKeyword::Arg(expr) => (None, expr, false),
            ArgOrKeyword::Keyword(keyword) => (
                keyword.arg.as_ref().map(|name| name.id.as_str()),
                &keyword.value,
                keyword.arg.is_none(),
            ),
        };
        super::ir::Argument {
            keyword: keyword.map(|name| self.interner.name(name)),
            value: self.reserve(value, scope),
            expansion,
        }
    }

    /// When `value` is a call block's call, its callee's index in the value table.
    pub(super) fn factory(&self, value: &Expr) -> Option<u32> {
        match value {
            Expr::Call(_) => self.inputs.callees.get(&address(value)).copied(),
            _ => None,
        }
    }

    /// One assignment statement: a node per level of a chain, the last target outermost.
    pub(super) fn assignment(&mut self, node: &'a ruff_python_ast::StmtAssign, ctx: Ctx) {
        let value = self.reserve(&node.value, ctx.scope);
        let chained = (node.targets.len() > 1).then(|| self.interner.chain(value, ctx.scope));
        for (level, target) in node.targets.iter().enumerate().rev() {
            let (level_value, factory) = match chained.filter(|_| level > 0) {
                Some(chain) => (chain, None),
                None => (value, self.factory(&node.value)),
            };
            let id = self.node(
                NodeKind::Assignment {
                    value: Some(level_value),
                    annotation: None,
                    factory,
                },
                ctx,
            );
            self.names(target, ctx, id, WriteKind::Assignment, Some(level_value), None);
            self.target_effect(
                target,
                TargetKind::Assignment,
                Some(level_value),
                None,
                ctx,
                false,
            );
        }
        let mut children: Vec<(&'a Expr, Ctx)> =
            node.targets.iter().rev().map(|target| (target, ctx)).collect();
        children.push((&node.value, ctx));
        self.visit(children);
    }

    /// Writes every name a target binds (through tuples, lists, and starred items).
    pub(super) fn names(
        &mut self,
        target: &'a Expr,
        ctx: Ctx,
        node: NodeId,
        kind: WriteKind,
        value: Option<ExprId>,
        annotation: Option<ExprId>,
    ) {
        let mut pending = vec![target];
        let mut found = Vec::new();
        while let Some(expr) = pending.pop() {
            match expr {
                Expr::Name(name) => found.push(name.id.as_str()),
                Expr::Tuple(tuple) => pending.extend(tuple.elts.iter().rev()),
                Expr::List(list) => pending.extend(list.elts.iter().rev()),
                Expr::Starred(starred) => pending.push(&starred.value),
                _ => {}
            }
        }
        for name in found {
            self.write(
                ctx,
                name,
                false,
                WriteFact {
                    node,
                    kind,
                    value,
                    annotation,
                    import: None,
                    conditional: ctx.conditional,
                },
            );
        }
    }

    /// Records a target the mutation passes read: one that writes an attribute or item, or
    /// any augmented target.
    pub(super) fn target_effect(
        &mut self,
        target: &'a Expr,
        kind: TargetKind,
        value: Option<ExprId>,
        annotation: Option<ExprId>,
        ctx: Ctx,
        augmented: bool,
    ) {
        if !augmented && !writes_attribute(target) {
            return;
        }
        let target = self.reserve(target, ctx.scope);
        self.effects.push(Effect::Target {
            target,
            kind,
            value,
            annotation,
            function: ctx.function,
            augmented,
        });
    }

    pub(super) fn import(&mut self, alias: &'a Alias, ctx: Ctx) {
        let id = self.node(NodeKind::Import, ctx);
        let written = alias.name.id.as_str();
        let (bound, module) = match &alias.asname {
            Some(asname) => (asname.id.as_str(), written),
            None => {
                let first = written.split('.').next().unwrap_or(written);
                (first, first)
            }
        };
        let module = ModuleSpec {
            level: 0,
            module: module.into(),
        };
        self.import_write(ctx, bound, id, module, None);
    }

    pub(super) fn import_write(
        &mut self,
        ctx: Ctx,
        bound: &'a str,
        node: NodeId,
        module: ModuleSpec,
        member: Option<u32>,
    ) {
        self.write(
            ctx,
            bound,
            false,
            WriteFact {
                node,
                kind: WriteKind::Import,
                value: None,
                annotation: None,
                import: Some(ImportWrite { module, member }),
                conditional: ctx.conditional,
            },
        );
    }
}

/// Whether a target writes an attribute or an item (through tuples, lists, and starred items).
fn writes_attribute(target: &Expr) -> bool {
    let mut pending = vec![target];
    while let Some(expr) = pending.pop() {
        match expr {
            Expr::Attribute(_) | Expr::Subscript(_) => return true,
            Expr::Tuple(tuple) => pending.extend(tuple.elts.iter()),
            Expr::List(list) => pending.extend(list.elts.iter()),
            Expr::Starred(starred) => pending.push(&starred.value),
            _ => {}
        }
    }
    false
}

/// An `import` alias's identity, for looking up its declaration.
pub(crate) fn alias_address(alias: &Alias) -> usize {
    std::ptr::from_ref(alias) as usize
}
