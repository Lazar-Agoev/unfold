//! The per-file pre-pass that runs before block selection: one iterative walk over the whole
//! syntax tree that
//!
//! - fails the file on the compile-time errors in [`checks`];
//! - measures the tree's depth, so it can be dropped safely ([`crate::parse::release`]);
//! - takes a per-file write census ([`scope`]) and proves the identities that shape structure
//!   ([`Proofs`]): `builtins.property` (getter and setter blocks), `typing.TYPE_CHECKING`
//!   (erased imports), and `importlib.import_module` (literal dynamic imports become `import`
//!   blocks).
//!
//! **Per file by design.** These proofs decide which blocks exist, so they must be settled at
//! extraction, which sees one file. The proof of concept decided them with its whole-program
//! binder, which also saw what a star import binds and whether another module mutates an imported
//! module's attribute; here a star import is assumed not to rebind `property`, `TYPE_CHECKING`, or
//! an `importlib` alias, and only same-file attribute writes (`typing.TYPE_CHECKING = True`)
//! invalidate an imported identity. Everything else is the binder's ([`crate::bind`],
//! [`crate::link`]), which sees the whole program.

pub(crate) mod checks;
mod proofs;
pub(crate) mod scope;

use ruff_python_ast::{
    Decorator, Expr, ExprCall, ExprContext, ModModule, Parameters, Pattern, Stmt, StmtFunctionDef, TypeParam,
    TypeParams,
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use unfold_engine::SourceFailure;

use self::checks::{Flags, Transfer};
pub(crate) use self::proofs::Proofs;
use self::proofs::{Identities, member_kind};
use self::scope::{Census, MODULE, Origin, ScopeKind, Write};
use crate::ast::{expr_children, single_name_argument, string_literal};

/// What the pre-pass found.
#[derive(Debug)]
pub(crate) struct Analysis {
    /// The deepest nesting of statements, expressions, and patterns.
    pub(crate) depth: usize,
    /// The first compile-time error, if any.
    pub(crate) failure: Option<SourceFailure>,
    /// The structural proofs.
    pub(crate) proofs: Proofs,
}

/// The innermost definition around a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owner {
    Module,
    Class,
    Function,
}

/// What a node inherits from its ancestors.
#[derive(Debug, Clone, Copy)]
struct Context {
    scope: u32,
    flags: Flags,
    owner: Owner,
    /// The innermost `if` whose then-branch contains the node (0 for none; guard `i` is
    /// `guards[i - 1]`).
    guard: u32,
    depth: u32,
}

impl Context {
    fn child(self) -> Self {
        Self {
            depth: self.depth + 1,
            ..self
        }
    }
}

enum Task<'a> {
    Stmt(&'a Stmt, Context),
    Expr(&'a Expr, Context),
}

/// An `if` test that may guard imports in its then-branch.
struct Guard<'a> {
    test: &'a Expr,
    scope: u32,
    parent: u32,
}

struct Walker<'a> {
    census: Census<'a>,
    stack: Vec<Task<'a>>,
    depth: usize,
    pep765: bool,
    guards: Vec<Guard<'a>>,
    definitions: Vec<(&'a StmtFunctionDef, u32)>,
    imports: Vec<(TextSize, u32)>,
    /// Each import statement's start with the range of one runtime dependency it declares.
    import_ranges: Vec<(TextSize, TextRange)>,
    failure: Option<SourceFailure>,
    dynamic: Vec<(&'a ExprCall, u32, &'a str)>,
    evaluations: Vec<(&'a Expr, u32)>,
    mutations: Vec<(u32, &'a str, &'a str)>,
    scratch: Vec<&'a Expr>,
}

/// Walks `module` once: checks, depth, census, and proofs. `pep765` turns on the grammar 3.14
/// checks for transfers out of `finally`. The walk always completes, even after a failure, so
/// the depth is known whatever the tree.
pub(crate) fn analyze(module: &ModModule, pep765: bool) -> Analysis {
    let mut walker = Walker {
        census: Census::new(),
        stack: Vec::new(),
        depth: 0,
        pep765,
        guards: Vec::new(),
        definitions: Vec::new(),
        imports: Vec::new(),
        import_ranges: Vec::new(),
        failure: None,
        dynamic: Vec::new(),
        evaluations: Vec::new(),
        mutations: Vec::new(),
        scratch: Vec::new(),
    };
    let root = Context {
        scope: MODULE,
        flags: Flags::default(),
        owner: Owner::Module,
        guard: 0,
        depth: 1,
    };
    walker.statements(&module.body, root);
    while let Some(task) = walker.stack.pop() {
        match task {
            Task::Stmt(stmt, context) => {
                if let Err(failure) = walker.stmt(stmt, context) {
                    walker.failure.get_or_insert(failure);
                }
            }
            Task::Expr(expr, context) => walker.expr(expr, context),
        }
    }
    walker.census.resolve();
    let proofs = walker.prove();
    Analysis {
        depth: walker.depth,
        failure: walker.failure,
        proofs,
    }
}

impl<'a> Walker<'a> {
    fn statements(&mut self, body: &'a [Stmt], context: Context) {
        self.stack
            .extend(body.iter().rev().map(|stmt| Task::Stmt(stmt, context)));
    }

    fn exprs(&mut self, exprs: impl IntoIterator<Item = &'a Expr>, context: Context) {
        let start = self.stack.len();
        self.stack
            .extend(exprs.into_iter().map(|expr| Task::Expr(expr, context)));
        self.stack[start..].reverse();
    }

    fn expr_task(&mut self, expr: Option<&'a Expr>, context: Context) {
        if let Some(expr) = expr {
            self.stack.push(Task::Expr(expr, context));
        }
    }

    fn note_depth(&mut self, depth: u32) {
        self.depth = self.depth.max(depth as usize);
    }

    fn stmt(&mut self, stmt: &'a Stmt, context: Context) -> Result<(), SourceFailure> {
        self.note_depth(context.depth);
        let inner = context.child();
        let scope = context.scope;
        // Children are pushed last-first, so they are visited in source order.
        match stmt {
            Stmt::FunctionDef(function) => {
                self.census.write(scope, &function.name.id, Write::Def(function));
                checks::parameters(&function.parameters)?;
                if context.owner == Owner::Class {
                    self.definitions.push((function, scope));
                }
                let outer = self.type_parameters(function.type_params.as_deref(), inner);
                let body_scope = self.census.push(ScopeKind::Function, outer.scope);
                self.bind_parameters(&function.parameters, body_scope);
                let body = Context {
                    scope: body_scope,
                    flags: Flags::FUNCTION,
                    owner: Owner::Function,
                    ..inner
                };
                self.statements(&function.body, body);
                self.expr_task(function.returns.as_deref(), outer);
                self.parameter_expressions(&function.parameters, inner, outer);
                self.decorators(&function.decorator_list, inner);
            }
            Stmt::ClassDef(class) => {
                self.census.write(scope, &class.name.id, Write::Other);
                let outer = self.type_parameters(class.type_params.as_deref(), inner);
                let body = Context {
                    scope: self.census.push(ScopeKind::Class, outer.scope),
                    flags: Flags::default(),
                    owner: Owner::Class,
                    ..inner
                };
                self.statements(&class.body, body);
                if let Some(arguments) = &class.arguments {
                    let values: Vec<&'a Expr> = crate::ast::arguments(arguments)
                        .map(|argument| argument.value())
                        .collect();
                    self.exprs(values, outer);
                }
                self.decorators(&class.decorator_list, inner);
            }
            Stmt::Return(node) => {
                checks::transfer(context.flags, Transfer::Return, node.range.start(), self.pep765)?;
                self.expr_task(node.value.as_deref(), inner);
            }
            Stmt::Break(node) => {
                checks::transfer(context.flags, Transfer::Break, node.range.start(), self.pep765)?;
            }
            Stmt::Continue(node) => {
                checks::transfer(context.flags, Transfer::Continue, node.range.start(), self.pep765)?;
            }
            Stmt::Delete(node) => self.exprs(node.targets.iter(), inner),
            Stmt::TypeAlias(node) => {
                let outer = self.type_parameters(node.type_params.as_deref(), inner);
                self.expr_task(Some(&node.value), outer);
                self.expr_task(Some(&node.name), inner);
            }
            Stmt::Assign(node) => {
                self.expr_task(Some(&node.value), inner);
                self.exprs(node.targets.iter(), inner);
            }
            Stmt::AugAssign(node) => self.exprs([&*node.target, &*node.value], inner),
            Stmt::AnnAssign(node) => {
                self.expr_task(node.value.as_deref(), inner);
                self.exprs([&*node.target, &*node.annotation], inner);
            }
            Stmt::For(node) => {
                self.statements(&node.orelse, inner);
                self.statements(
                    &node.body,
                    Context {
                        flags: inner.flags.enter_loop(),
                        ..inner
                    },
                );
                self.exprs([&*node.target, &*node.iter], inner);
            }
            Stmt::While(node) => {
                self.statements(&node.orelse, inner);
                self.statements(
                    &node.body,
                    Context {
                        flags: inner.flags.enter_loop(),
                        ..inner
                    },
                );
                self.expr_task(Some(&node.test), inner);
            }
            Stmt::If(node) => {
                for clause in node.elif_else_clauses.iter().rev() {
                    let body = match &clause.test {
                        Some(test) => self.guarded(test, inner),
                        None => inner,
                    };
                    self.statements(&clause.body, body);
                    self.expr_task(clause.test.as_ref(), inner);
                }
                let body = self.guarded(&node.test, inner);
                self.statements(&node.body, body);
                self.expr_task(Some(&node.test), inner);
            }
            Stmt::With(node) => {
                self.statements(&node.body, inner);
                let mut exprs = Vec::new();
                for item in &node.items {
                    exprs.push(&item.context_expr);
                    exprs.extend(item.optional_vars.as_deref());
                }
                self.exprs(exprs, inner);
            }
            Stmt::Match(node) => {
                let depth = checks::cases(&node.cases)?;
                self.note_depth(inner.depth + depth as u32);
                for case in node.cases.iter().rev() {
                    self.statements(&case.body, inner);
                    self.expr_task(case.guard.as_deref(), inner);
                    self.bind_pattern(&case.pattern, scope);
                }
                self.expr_task(Some(&node.subject), inner);
            }
            Stmt::Raise(node) => {
                self.expr_task(node.cause.as_deref(), inner);
                self.expr_task(node.exc.as_deref(), inner);
            }
            Stmt::Try(node) => {
                checks::handlers(&node.handlers)?;
                self.statements(
                    &node.finalbody,
                    Context {
                        flags: inner.flags.enter_finally(),
                        ..inner
                    },
                );
                self.statements(&node.orelse, inner);
                for ruff_python_ast::ExceptHandler::ExceptHandler(handler) in node.handlers.iter().rev() {
                    let flags = Flags {
                        except_group: inner.flags.except_group || node.is_star,
                        ..inner.flags
                    };
                    self.statements(&handler.body, Context { flags, ..inner });
                    if let Some(name) = &handler.name {
                        self.census.write(scope, &name.id, Write::Other);
                    }
                    self.expr_task(handler.type_.as_deref(), inner);
                }
                self.statements(&node.body, inner);
            }
            Stmt::Assert(node) => {
                self.expr_task(node.msg.as_deref(), inner);
                self.expr_task(Some(&node.test), inner);
            }
            Stmt::Import(node) => {
                self.imports.push((node.range.start(), context.guard));
                for alias in &node.names {
                    self.import_ranges.push((node.range.start(), alias.range));
                    let (bound, origin) = match &alias.asname {
                        Some(asname) => (asname.id.as_str(), alias.name.id.as_str()),
                        None => {
                            let first = alias.name.id.split('.').next().unwrap_or_default();
                            (first, first)
                        }
                    };
                    self.census
                        .write(scope, bound, Write::Import(Origin::Module(origin)));
                }
            }
            Stmt::ImportFrom(node) => {
                self.imports.push((node.range.start(), context.guard));
                self.import_ranges.push((node.range.start(), node.range));
                for alias in &node.names {
                    if alias.name.id.as_str() == "*" {
                        continue;
                    }
                    let bound = alias.asname.as_ref().unwrap_or(&alias.name);
                    let origin = match (&node.module, node.level) {
                        (Some(module), 0) => Origin::Member(&module.id, &alias.name.id),
                        _ => Origin::Relative,
                    };
                    self.census.write(scope, &bound.id, Write::Import(origin));
                }
            }
            Stmt::Global(node) => {
                for name in &node.names {
                    self.census.declare_global(scope, &name.id);
                }
            }
            Stmt::Nonlocal(node) => {
                for name in &node.names {
                    self.census.declare_nonlocal(scope, &name.id);
                }
            }
            Stmt::Expr(node) => self.expr_task(Some(&node.value), inner),
            Stmt::Pass(_) | Stmt::IpyEscapeCommand(_) => {}
        }
        Ok(())
    }

    fn expr(&mut self, expr: &'a Expr, context: Context) {
        self.note_depth(context.depth);
        let inner = context.child();
        let scope = context.scope;
        match expr {
            Expr::Name(name) => {
                if matches!(name.ctx, ExprContext::Store | ExprContext::Del) {
                    self.census.write(scope, &name.id, Write::Other);
                }
            }
            Expr::Named(node) => {
                if let Expr::Name(target) = &*node.target {
                    let binds = self.census.walrus_target(scope);
                    self.census.write(binds, &target.id, Write::Other);
                } else {
                    self.expr_task(Some(&node.target), inner);
                }
                self.expr_task(Some(&node.value), inner);
            }
            Expr::Lambda(lambda) => {
                let body_scope = self.census.push(ScopeKind::Function, scope);
                if let Some(parameters) = &lambda.parameters {
                    if let Err(failure) = checks::parameters(parameters) {
                        self.failure.get_or_insert(failure);
                    }
                    self.bind_parameters(parameters, body_scope);
                }
                let body = Context {
                    scope: body_scope,
                    owner: Owner::Function,
                    ..inner
                };
                self.expr_task(Some(&lambda.body), body);
                if let Some(parameters) = &lambda.parameters {
                    self.exprs(
                        parameters.iter().filter_map(|parameter| parameter.default()),
                        inner,
                    );
                }
            }
            Expr::ListComp(_) | Expr::SetComp(_) | Expr::Generator(_) | Expr::DictComp(_) => {
                self.comprehension(expr, inner);
            }
            Expr::Attribute(node) => {
                if matches!(node.ctx, ExprContext::Store | ExprContext::Del)
                    && let Expr::Name(base) = &*node.value
                {
                    self.mutations.push((scope, &base.id, &node.attr.id));
                }
                self.expr_task(Some(&node.value), inner);
            }
            Expr::Call(call) => {
                if let Expr::Name(name) = &*call.func
                    && matches!(name.id.as_str(), "exec" | "eval")
                {
                    self.evaluations.push((&call.func, scope));
                }
                if let Some(name) = single_name_argument(call).and_then(string_literal)
                    && !name.is_empty()
                    && !name.starts_with('.')
                {
                    self.dynamic.push((call, scope, name));
                }
                self.children(expr, inner);
            }
            _ => self.children(expr, inner),
        }
    }

    fn children(&mut self, expr: &'a Expr, context: Context) {
        let mut children = std::mem::take(&mut self.scratch);
        children.clear();
        expr_children(expr, &mut children);
        self.exprs(children.iter().copied(), context);
        self.scratch = children;
    }

    /// A comprehension: its first iterable is evaluated in the enclosing scope, everything else
    /// in the comprehension's own scope.
    fn comprehension(&mut self, expr: &'a Expr, context: Context) {
        let (elements, generators): (Vec<&'a Expr>, &'a [ruff_python_ast::Comprehension]) = match expr {
            Expr::ListComp(node) => (vec![&node.elt], &node.generators),
            Expr::SetComp(node) => (vec![&node.elt], &node.generators),
            Expr::Generator(node) => (vec![&node.elt], &node.generators),
            Expr::DictComp(node) => (
                node.key.as_deref().into_iter().chain([&*node.value]).collect(),
                &node.generators,
            ),
            _ => return,
        };
        let inside = Context {
            scope: self.census.push(ScopeKind::Comprehension, context.scope),
            ..context
        };
        let mut tasks = Vec::new();
        for (index, generator) in generators.iter().enumerate() {
            tasks.push(Task::Expr(&generator.target, inside));
            let iter_context = if index == 0 { context } else { inside };
            tasks.push(Task::Expr(&generator.iter, iter_context));
            tasks.extend(generator.ifs.iter().map(|test| Task::Expr(test, inside)));
        }
        tasks.extend(elements.into_iter().map(|element| Task::Expr(element, inside)));
        self.stack.extend(tasks.into_iter().rev());
    }

    fn decorators(&mut self, decorators: &'a [Decorator], context: Context) {
        self.exprs(decorators.iter().map(|decorator| &decorator.expression), context);
    }

    /// Opens a type-parameter scope if there are type parameters, binds them, and queues their
    /// bounds and defaults. Returns the context annotations are evaluated in.
    fn type_parameters(&mut self, parameters: Option<&'a TypeParams>, context: Context) -> Context {
        let Some(parameters) = parameters else {
            return context;
        };
        let scope = self.census.push(ScopeKind::TypeParameters, context.scope);
        let inside = Context { scope, ..context };
        let mut exprs = Vec::new();
        for parameter in &parameters.type_params {
            let (name, bound, default) = match parameter {
                TypeParam::TypeVar(node) => (&node.name, node.bound.as_deref(), node.default.as_deref()),
                TypeParam::ParamSpec(node) => (&node.name, None, node.default.as_deref()),
                TypeParam::TypeVarTuple(node) => (&node.name, None, node.default.as_deref()),
            };
            self.census.write(scope, &name.id, Write::Other);
            exprs.extend(bound);
            exprs.extend(default);
        }
        self.exprs(exprs, inside);
        inside
    }

    fn bind_parameters(&mut self, parameters: &'a Parameters, scope: u32) {
        for parameter in parameters {
            self.census.write(scope, &parameter.name().id, Write::Other);
        }
    }

    /// Queues defaults (evaluated where the function is defined) and annotations (evaluated in
    /// the type-parameter scope, if any).
    fn parameter_expressions(&mut self, parameters: &'a Parameters, defaults: Context, annotations: Context) {
        let mut tasks = Vec::new();
        for parameter in parameters {
            if let Some(annotation) = parameter.annotation() {
                tasks.push(Task::Expr(annotation, annotations));
            }
            if let Some(default) = parameter.default() {
                tasks.push(Task::Expr(default, defaults));
            }
        }
        self.stack.extend(tasks.into_iter().rev());
    }

    fn bind_pattern(&mut self, pattern: &'a Pattern, scope: u32) {
        let mut stack = vec![pattern];
        while let Some(pattern) = stack.pop() {
            let name = match pattern {
                Pattern::MatchAs(node) => {
                    stack.extend(node.pattern.as_deref());
                    node.name.as_ref()
                }
                Pattern::MatchStar(node) => node.name.as_ref(),
                Pattern::MatchMapping(node) => {
                    stack.extend(&node.patterns);
                    node.rest.as_ref()
                }
                Pattern::MatchSequence(node) => {
                    stack.extend(&node.patterns);
                    None
                }
                Pattern::MatchClass(node) => {
                    stack.extend(&node.arguments.patterns);
                    stack.extend(node.arguments.keywords.iter().map(|keyword| &keyword.pattern));
                    None
                }
                Pattern::MatchOr(node) => {
                    stack.extend(&node.patterns);
                    None
                }
                Pattern::MatchValue(_) | Pattern::MatchSingleton(_) => None,
            };
            if let Some(name) = name {
                self.census.write(scope, &name.id, Write::Other);
            }
        }
    }

    /// The context of an `if` branch guarded by `test`.
    fn guarded(&mut self, test: &'a Expr, context: Context) -> Context {
        self.guards.push(Guard {
            test,
            scope: context.scope,
            parent: context.guard,
        });
        Context {
            guard: self.guards.len() as u32,
            ..context
        }
    }

    fn prove(&self) -> Proofs {
        let identities = Identities::new(&self.census, &self.evaluations, &self.mutations);
        let mut proofs = Proofs::default();
        for &(function, scope) in &self.definitions {
            proofs
                .kinds
                .insert(function.range.start(), member_kind(function, scope, &identities));
        }
        let mut guards: Vec<Option<bool>> = vec![None; self.guards.len()];
        for &(start, guard) in &self.imports {
            if self.erases(guard, &identities, &mut guards) {
                proofs.erased.insert(start);
            }
        }
        for &(call, scope, name) in &self.dynamic {
            if identities.is(&call.func, scope, "importlib.import_module") {
                proofs.dynamic.insert(call.range(), name.into());
            }
        }
        proofs.dependencies = self
            .import_ranges
            .iter()
            .filter(|(start, _)| !proofs.erased.contains(start))
            .map(|&(_, range)| range)
            .chain(proofs.dynamic.keys().copied())
            .collect();
        proofs
            .dependencies
            .sort_by_key(|range| (range.start(), range.end()));
        proofs
    }

    /// Whether any `if` on the guard chain from `guard` proves `typing.TYPE_CHECKING`.
    fn erases(&self, mut guard: u32, identities: &Identities<'_, 'a>, cache: &mut [Option<bool>]) -> bool {
        while guard > 0 {
            let index = (guard - 1) as usize;
            let entry = &self.guards[index];
            let proven = *cache[index]
                .get_or_insert_with(|| identities.is(entry.test, entry.scope, "typing.TYPE_CHECKING"));
            if proven {
                return true;
            }
            guard = entry.parent;
        }
        false
    }
}
