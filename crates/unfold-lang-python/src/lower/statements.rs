//! Lowering statements: definitions with the calls that run when they are created, control
//! blocks and their parts, exits, imports, and the simple statements whose calls are blocks.

use ruff_python_ast::{
    Alias, Decorator, ExceptHandler, Expr, MatchCase, Pattern, PatternMatchAs, Stmt, StmtClassDef, StmtFor,
    StmtFunctionDef, StmtIf, StmtImportFrom, StmtMatch, StmtTry, StmtWhile, StmtWith, TypeParam, TypeParams,
};
use ruff_text_size::{Ranged, TextRange, TextSize};
use unfold_core::key::Child;
use unfold_core::vocab::{CallRole, Flag, Form};
use unfold_engine::SourceFailure;
use unfold_engine::facts::ImportFact;

use super::expressions::statement_address;
use super::{Import, Lowering, Place, Value, docstring, span};
use crate::ast::bytes;
use crate::payload::ImportSyntax;
use crate::typed;

/// One part of a statement's control block.
struct Clause<'a> {
    form: Form,
    range: TextRange,
    /// Its test or clause.
    head: Option<TextRange>,
    /// The expression its head holds, whose calls are `test` calls.
    test: Option<&'a Expr>,
    body: &'a [Stmt],
    /// The loop a `break` or `continue` in its body leaves.
    target: Option<u32>,
}

/// The written range of a suite: its first statement to its last.
fn suite(body: &[Stmt], fallback: TextSize) -> TextRange {
    match (body.first(), body.last()) {
        (Some(first), Some(last)) => span(first.start(), last.end()),
        _ => TextRange::empty(fallback),
    }
}

/// `case _:`, the guardless wildcard: a match's `default` part.
fn is_default(case: &MatchCase) -> bool {
    case.guard.is_none()
        && matches!(
            case.pattern,
            Pattern::MatchAs(PatternMatchAs {
                pattern: None,
                name: None,
                ..
            })
        )
}

/// A `yield` or `yield from`, the value a statement may consist of.
fn is_yield(expr: &Expr) -> bool {
    matches!(expr, Expr::Yield(_) | Expr::YieldFrom(_))
}

/// A dotted module path as written, without the whitespace and line continuations Python
/// allows between its parts.
fn dotted(written: &str) -> String {
    written
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\\')
        .collect()
}

/// Whether a `from` import name may import a child module of its own: every name but `*` and
/// dunder names (`__x__`), which the import system never loads as submodules from a from-list.
/// Whether it does depends on other files, so its link is conditional and linking decides.
fn is_child_candidate(alias: &Alias) -> bool {
    let name = alias.name.id.as_str();
    name != "*" && !(name.len() >= 4 && name.starts_with("__") && name.ends_with("__"))
}

/// The bounds and defaults of type parameters, which run (lazily) where the definition is.
fn type_parameter_values(parameters: Option<&TypeParams>) -> Vec<&Expr> {
    let mut values = Vec::new();
    for parameter in parameters.map_or(&[][..], |parameters| &parameters.type_params) {
        let (bound, default) = match parameter {
            TypeParam::TypeVar(node) => (node.bound.as_deref(), node.default.as_deref()),
            TypeParam::ParamSpec(node) => (None, node.default.as_deref()),
            TypeParam::TypeVarTuple(node) => (None, node.default.as_deref()),
        };
        values.extend(bound);
        values.extend(default);
    }
    values
}

impl<'a> Lowering<'a> {
    pub(super) fn body(&mut self, body: &'a [Stmt], place: Place) -> Result<(), SourceFailure> {
        for stmt in body {
            self.grow(|this| this.stmt(stmt, place))?;
        }
        Ok(())
    }

    fn stmt(&mut self, stmt: &'a Stmt, place: Place) -> Result<(), SourceFailure> {
        match stmt {
            Stmt::FunctionDef(function) => self.function(stmt, function, place),
            Stmt::ClassDef(class) => self.class(stmt, class, place),
            Stmt::If(node) => self.decision(node, place),
            Stmt::Match(node) => self.matching(node, place),
            Stmt::For(node) => self.for_loop(node, place),
            Stmt::While(node) => self.while_loop(node, place),
            Stmt::Try(node) => self.attempt(node, place),
            Stmt::With(node) => self.with(node, place),
            Stmt::Return(node) => self.exit(Form::Return, node.range, place, node.value.as_deref(), None),
            Stmt::Raise(node) => self.exit(
                Form::Raise,
                node.range,
                place,
                node.exc.as_deref(),
                node.cause.as_deref(),
            ),
            Stmt::Break(node) => self.jump(Form::Break, node.range, place),
            Stmt::Continue(node) => self.jump(Form::Continue, node.range, place),
            Stmt::Expr(node) if is_yield(&node.value) => self.yielded(&node.value, node.range, place),
            Stmt::Expr(node) => {
                self.simple(node.range, |this| this.expr(&node.value, place, Value::default()))
            }
            Stmt::Assign(node) => self.simple(node.range, |this| {
                if let ([Expr::Name(target)], Expr::Lambda(lambda)) = (node.targets.as_slice(), &*node.value)
                {
                    let name = this.slice(target.range);
                    return this.lambda(&node.value, lambda, place, Some(name));
                }
                if is_yield(&node.value) {
                    this.yielded(&node.value, node.range, place)?;
                } else {
                    let binds = match (node.targets.first(), node.targets.last()) {
                        (Some(first), Some(last)) => Some(this.slice(span(first.start(), last.end()))),
                        _ => None,
                    };
                    let value = Value {
                        binds,
                        ..Value::default()
                    };
                    this.expr(&node.value, place, value)?;
                }
                for target in &node.targets {
                    this.expr(target, place, Value::default())?;
                }
                Ok(())
            }),
            Stmt::AnnAssign(node) => self.simple(node.range, |this| {
                let target = this.slice(node.target.range());
                match node.value.as_deref() {
                    Some(value @ Expr::Lambda(lambda)) if node.target.is_name_expr() => {
                        this.lambda(value, lambda, place, Some(target))?;
                    }
                    Some(value) if is_yield(value) => this.yielded(value, node.range, place)?,
                    Some(value) => {
                        let binds = Value {
                            binds: Some(target),
                            ..Value::default()
                        };
                        this.expr(value, place, binds)?;
                    }
                    None => {}
                }
                this.expr(&node.target, place, Value::default())?;
                this.expr(&node.annotation, place, Value::default())
            }),
            Stmt::AugAssign(node) => self.simple(node.range, |this| {
                this.expr(&node.target, place, Value::default())?;
                if is_yield(&node.value) {
                    this.yielded(&node.value, node.range, place)
                } else {
                    this.expr(&node.value, place, Value::default())
                }
            }),
            Stmt::Delete(node) => self.simple(node.range, |this| {
                for target in &node.targets {
                    this.expr(target, place, Value::default())?;
                }
                Ok(())
            }),
            Stmt::Assert(node) => self.simple(node.range, |this| {
                this.expr(&node.test, place, Value::default())?;
                match node.msg.as_deref() {
                    Some(message) => this.expr(message, place, Value::default()),
                    None => Ok(()),
                }
            }),
            Stmt::TypeAlias(node) => self.simple(node.range, |this| {
                for value in type_parameter_values(node.type_params.as_deref()) {
                    this.expr(value, place, Value::default())?;
                }
                this.expr(&node.value, place, Value::default())
            }),
            Stmt::Import(node) => {
                if !self.proofs.is_erased(node.range.start()) {
                    for alias in &node.names {
                        let specifier = dotted(self.slice(alias.name.range));
                        let syntax = ImportSyntax::Module {
                            module: alias.name.id.as_str().into(),
                            alias: alias.asname.as_ref().map(|name| name.id.as_str().into()),
                        };
                        self.import(place, alias.name.range, specifier, false, syntax, None);
                    }
                }
                Ok(())
            }
            Stmt::ImportFrom(node) => {
                if !self.proofs.is_erased(node.range.start()) {
                    self.import_from(node, place);
                }
                Ok(())
            }
            Stmt::Global(_) | Stmt::Nonlocal(_) | Stmt::Pass(_) | Stmt::IpyEscapeCommand(_) => Ok(()),
        }
    }

    /// Lowers a simple statement's expressions: the first block records the statement.
    fn simple(
        &mut self,
        statement: TextRange,
        lower: impl FnOnce(&mut Self) -> Result<(), SourceFailure>,
    ) -> Result<(), SourceFailure> {
        self.statement = Some(statement);
        let lowered = lower(self);
        self.statement = None;
        lowered
    }

    /// The start of a definition's own keyword (`def`, `async`, `class`), after its decorators.
    fn keyword(&self, statement: TextRange, decorators: &[Decorator]) -> TextSize {
        match decorators.last() {
            Some(last) => self
                .tokens
                .after(last.end())
                .map_or(statement.start(), Ranged::start),
            None => statement.start(),
        }
    }

    /// A header's range: from `start` to the end of the last token before the suite's colon.
    fn header(&self, start: TextSize, suite: TextRange, fallback: TextSize) -> TextRange {
        span(start, self.tokens.header_end(suite.start()).unwrap_or(fallback))
    }

    /// The decorators of a definition: calls in the enclosing body, role `decorator`.
    fn decorators(&mut self, decorators: &'a [Decorator], place: Place) -> Result<(), SourceFailure> {
        let place = place.with_role(CallRole::Decorator);
        for decorator in decorators {
            let expr = &decorator.expression;
            match expr {
                Expr::Call(call) => self.call(expr, call, place, Value::default())?,
                _ => self.decorator_call(expr, place)?,
            }
        }
        Ok(())
    }

    /// `def` and `async def`: its decorators, then what runs when it is defined (type parameter
    /// bounds, defaults, and annotations, role `default`), then the function and its body.
    fn function(
        &mut self,
        stmt: &'a Stmt,
        function: &'a StmtFunctionDef,
        place: Place,
    ) -> Result<(), SourceFailure> {
        let start = self.owned.len();
        self.decorators(&function.decorator_list, place)?;
        let header = place.with_role(CallRole::Default);
        for value in type_parameter_values(function.type_params.as_deref()) {
            self.expr(value, header, Value::default())?;
        }
        for parameter in &function.parameters {
            for value in [parameter.annotation(), parameter.default()]
                .into_iter()
                .flatten()
            {
                self.expr(value, header, Value::default())?;
            }
        }
        if let Some(returns) = function.returns.as_deref() {
            self.expr(returns, header, Value::default())?;
        }
        let keyword = self.keyword(stmt.range(), &function.decorator_list);
        let name = self.slice(function.name.range);
        let form = self.proofs.function_form(function);
        let id = self.push(place, Child::Definition(name), form, span(keyword, stmt.end()))?;
        self.own(start, id);
        let body = suite(&function.body, stmt.end());
        let signature_end = function
            .returns
            .as_deref()
            .map_or(function.parameters.end(), Ranged::end);
        let head = self.header(keyword, body, signature_end);
        let generator = self.generators.contains(&statement_address(stmt));
        let node = self.node(id);
        node.head = Some(bytes(head));
        node.body = Some(bytes(body));
        node.doc = docstring(&function.body).map(bytes);
        if function.is_async {
            node.flags.insert(Flag::Async);
        }
        if generator {
            node.flags.insert(Flag::Generator);
        }
        self.lowered.definitions.insert(statement_address(stmt), id);
        typed::signature(&mut self.lowered, id, function.returns.as_deref(), self.text);
        typed::parameters(&mut self.lowered, id, &function.parameters, self.text);
        if self.inside(id, place).is_some() {
            self.body(&function.body, place.definition(id))?;
        }
        Ok(())
    }

    /// `class`: its decorators, then the calls in its header (role `base`), then the class and
    /// its body. A base written as a name is an `extends` link, which the binder records.
    fn class(&mut self, stmt: &'a Stmt, class: &'a StmtClassDef, place: Place) -> Result<(), SourceFailure> {
        let start = self.owned.len();
        self.decorators(&class.decorator_list, place)?;
        let header = place.with_role(CallRole::Base);
        for value in type_parameter_values(class.type_params.as_deref()) {
            self.expr(value, header, Value::default())?;
        }
        if let Some(arguments) = class.arguments.as_deref() {
            for argument in crate::ast::arguments(arguments) {
                self.expr(argument.value(), header, Value::default())?;
            }
        }
        let keyword = self.keyword(stmt.range(), &class.decorator_list);
        let name = self.slice(class.name.range);
        let id = self.push(
            place,
            Child::Definition(name),
            Form::Class,
            span(keyword, stmt.end()),
        )?;
        self.own(start, id);
        let body = suite(&class.body, stmt.end());
        let signature_end = class
            .arguments
            .as_deref()
            .map(Ranged::end)
            .or_else(|| class.type_params.as_deref().map(Ranged::end))
            .unwrap_or(class.name.end());
        let head = self.header(keyword, body, signature_end);
        let node = self.node(id);
        node.head = Some(bytes(head));
        node.body = Some(bytes(body));
        node.doc = docstring(&class.body).map(bytes);
        self.lowered.definitions.insert(statement_address(stmt), id);
        typed::members(&mut self.lowered, id, &class.body, self.text);
        if self.inside(id, place).is_some() {
            self.body(&class.body, place.definition(id))?;
        }
        Ok(())
    }

    /// Pushes one part of the control block `control` (lowered at `place`), then its test and
    /// its body, unless lowering stops there.
    fn clause(&mut self, control: u32, place: Place, clause: Clause<'a>) -> Result<u32, SourceFailure> {
        let parts = place.under(control);
        let id = self.push(parts, Child::Form(clause.form), clause.form, clause.range)?;
        let node = self.node(id);
        node.head = clause.head.map(bytes);
        node.body = Some(bytes(suite(clause.body, clause.range.end())));
        if let Some(inside) = self.inside(id, parts) {
            if let Some(test) = clause.test {
                self.expr(test, inside.with_role(CallRole::Test), Value::default())?;
            }
            let inside = Place {
                target: clause.target,
                ..inside
            };
            self.body(clause.body, inside)?;
        }
        Ok(id)
    }

    /// `if`/`elif`/`else`: one decision with ordered parts, each test its part's head.
    fn decision(&mut self, node: &'a StmtIf, place: Place) -> Result<(), SourceFailure> {
        let id = self.push(place, Child::Form(Form::If), Form::If, node.range)?;
        let body_end = suite(&node.body, node.range.end()).end();
        self.clause(
            id,
            place,
            Clause {
                form: Form::Then,
                range: span(node.range.start(), body_end),
                head: Some(node.test.range()),
                test: Some(&node.test),
                body: &node.body,
                target: place.target,
            },
        )?;
        for branch in &node.elif_else_clauses {
            self.clause(
                id,
                place,
                Clause {
                    form: if branch.test.is_some() {
                        Form::Elif
                    } else {
                        Form::Else
                    },
                    range: branch.range,
                    head: branch.test.as_ref().map(Ranged::range),
                    test: branch.test.as_ref(),
                    body: &branch.body,
                    target: place.target,
                },
            )?;
        }
        Ok(())
    }

    /// `match`: a decision headed `match subject`, with a `case[n]` part per case (its pattern
    /// and guard are its test) and `default` for the guardless `case _:`.
    fn matching(&mut self, node: &'a StmtMatch, place: Place) -> Result<(), SourceFailure> {
        let id = self.push(place, Child::Form(Form::Match), Form::Match, node.range)?;
        let first_case = node
            .cases
            .first()
            .map_or_else(|| TextRange::empty(node.range.end()), Ranged::range);
        let head = self.header(node.range.start(), first_case, node.subject.end());
        self.node(id).head = Some(bytes(head));
        if let Some(inside) = self.inside(id, place) {
            self.expr(&node.subject, inside, Value::default())?;
        }
        for case in &node.cases {
            let body = suite(&case.body, case.range.end());
            let test_end = case.guard.as_deref().map_or(case.pattern.end(), Ranged::end);
            self.clause(
                id,
                place,
                Clause {
                    form: if is_default(case) {
                        Form::Default
                    } else {
                        Form::Case
                    },
                    range: case.range,
                    head: Some(self.header(case.pattern.start(), body, test_end)),
                    test: case.guard.as_deref(),
                    body: &case.body,
                    target: place.target,
                },
            )?;
        }
        Ok(())
    }

    /// A loop's `else` part, whose `break` and `continue` leave the loop around it.
    fn loop_else(&mut self, id: u32, place: Place, orelse: &'a [Stmt]) -> Result<(), SourceFailure> {
        let Some(first) = orelse.first() else {
            return Ok(());
        };
        let body = suite(orelse, first.start());
        let start = self.tokens.clause_start(body.start()).unwrap_or(body.start());
        self.clause(
            id,
            place,
            Clause {
                form: Form::Else,
                range: span(start, body.end()),
                head: None,
                test: None,
                body: orelse,
                target: place.target,
            },
        )?;
        Ok(())
    }

    /// `for` and `async for`: a loop headed `for target in iterable`, whose header calls are its
    /// children, then its `body` and `else` parts.
    fn for_loop(&mut self, node: &'a StmtFor, place: Place) -> Result<(), SourceFailure> {
        let id = self.push(place, Child::Form(Form::For), Form::For, node.range)?;
        let body = suite(&node.body, node.range.end());
        let head = self.header(node.range.start(), body, node.iter.end());
        self.node(id).head = Some(bytes(head));
        if let Some(inside) = self.inside(id, place) {
            self.expr(&node.iter, inside, Value::default())?;
            self.expr(&node.target, inside, Value::default())?;
        }
        self.clause(
            id,
            place,
            Clause {
                form: Form::Body,
                range: body,
                head: None,
                test: None,
                body: &node.body,
                target: Some(id),
            },
        )?;
        self.loop_else(id, place, &node.orelse)
    }

    /// `while`: a loop headed `while test`.
    fn while_loop(&mut self, node: &'a StmtWhile, place: Place) -> Result<(), SourceFailure> {
        let id = self.push(place, Child::Form(Form::While), Form::While, node.range)?;
        let body = suite(&node.body, node.range.end());
        let head = self.header(node.range.start(), body, node.test.end());
        self.node(id).head = Some(bytes(head));
        if let Some(inside) = self.inside(id, place) {
            self.expr(&node.test, inside, Value::default())?;
        }
        self.clause(
            id,
            place,
            Clause {
                form: Form::Body,
                range: body,
                head: None,
                test: None,
                body: &node.body,
                target: Some(id),
            },
        )?;
        self.loop_else(id, place, &node.orelse)
    }

    /// `try`: its `body`, an `except[n]` part per handler (the clause is its head, flag `group`
    /// for `except*`), `else`, and `finally`.
    fn attempt(&mut self, node: &'a StmtTry, place: Place) -> Result<(), SourceFailure> {
        let id = self.push(place, Child::Form(Form::Try), Form::Try, node.range)?;
        let body = suite(&node.body, node.range.start());
        self.clause(
            id,
            place,
            Clause {
                form: Form::Body,
                range: body,
                head: None,
                test: None,
                body: &node.body,
                target: place.target,
            },
        )?;
        for ExceptHandler::ExceptHandler(handler) in &node.handlers {
            let suite = suite(&handler.body, handler.range.end());
            let clause_end = handler
                .name
                .as_ref()
                .map(Ranged::end)
                .or_else(|| handler.type_.as_deref().map(Ranged::end))
                .unwrap_or(handler.range.start());
            let part = self.clause(
                id,
                place,
                Clause {
                    form: Form::Except,
                    range: handler.range,
                    head: Some(self.header(handler.range.start(), suite, clause_end)),
                    test: handler.type_.as_deref(),
                    body: &handler.body,
                    target: place.target,
                },
            )?;
            if node.is_star {
                self.node(part).flags.insert(Flag::Group);
            }
        }
        for (form, block) in [(Form::Else, &node.orelse), (Form::Finally, &node.finalbody)] {
            let Some(first) = block.first() else {
                continue;
            };
            let body = suite(block, first.start());
            let start = self.tokens.clause_start(body.start()).unwrap_or(body.start());
            self.clause(
                id,
                place,
                Clause {
                    form,
                    range: span(start, body.end()),
                    head: None,
                    test: None,
                    body: block,
                    target: place.target,
                },
            )?;
        }
        Ok(())
    }

    /// `with` and `async with` (flag `async`): header calls, then its `body`.
    fn with(&mut self, node: &'a StmtWith, place: Place) -> Result<(), SourceFailure> {
        let id = self.push(place, Child::Form(Form::With), Form::With, node.range)?;
        let body = suite(&node.body, node.range.end());
        let items_end = node.items.last().map_or(node.range.start(), Ranged::end);
        let head = self.header(node.range.start(), body, items_end);
        let with = self.node(id);
        with.head = Some(bytes(head));
        if node.is_async {
            with.flags.insert(Flag::Async);
        }
        if let Some(inside) = self.inside(id, place) {
            for item in &node.items {
                self.expr(&item.context_expr, inside, Value::default())?;
                if let Some(target) = item.optional_vars.as_deref() {
                    self.expr(target, inside, Value::default())?;
                }
            }
        }
        self.clause(
            id,
            place,
            Clause {
                form: Form::Body,
                range: body,
                head: None,
                test: None,
                body: &node.body,
                target: place.target,
            },
        )?;
        Ok(())
    }

    /// `return` and `raise`: an exit over the statement, holding its value's blocks. A raise's
    /// value runs through its cause (`raise Error() from cause`); only the exception is the
    /// exit's value.
    fn exit(
        &mut self,
        form: Form,
        statement: TextRange,
        place: Place,
        value: Option<&'a Expr>,
        cause: Option<&'a Expr>,
    ) -> Result<(), SourceFailure> {
        let id = self.push(place, Child::Form(form), form, statement)?;
        let node = self.node(id);
        node.stmt = Some(bytes(statement));
        node.value = value.map(|value| bytes(span(value.start(), cause.map_or(value.end(), Ranged::end))));
        if let Some(inside) = self.inside(id, place) {
            if let Some(value) = value {
                let exit_value = Value {
                    exit_value: true,
                    ..Value::default()
                };
                self.expr(value, inside, exit_value)?;
            }
            if let Some(cause) = cause {
                self.expr(cause, inside, Value::default())?;
            }
        }
        Ok(())
    }

    /// `break` and `continue`: exits that name their loop as `target`.
    fn jump(&mut self, form: Form, statement: TextRange, place: Place) -> Result<(), SourceFailure> {
        let id = self.push(place, Child::Form(form), form, statement)?;
        let node = self.node(id);
        node.stmt = Some(bytes(statement));
        node.target = place.target;
        Ok(())
    }

    /// A `yield` or `yield from` that is a statement or the whole value an assignment assigns:
    /// an exit over the expression, recording the statement.
    fn yielded(&mut self, expr: &'a Expr, statement: TextRange, place: Place) -> Result<(), SourceFailure> {
        let value = match expr {
            Expr::Yield(node) => node.value.as_deref(),
            Expr::YieldFrom(node) => Some(&*node.value),
            _ => None,
        };
        let id = self.push(place, Child::Form(Form::Yield), Form::Yield, expr.range())?;
        let node = self.node(id);
        node.stmt = Some(bytes(statement));
        node.value = value.map(|value| bytes(value.range()));
        if let (Some(inside), Some(value)) = (self.inside(id, place), value) {
            let exit_value = Value {
                exit_value: true,
                ..Value::default()
            };
            self.expr(value, inside, exit_value)?;
        }
        Ok(())
    }

    /// One written module dependency: an `imports` link from the definition the statement is
    /// written in.
    fn import(
        &mut self,
        place: Place,
        range: TextRange,
        specifier: String,
        conditional: bool,
        syntax: ImportSyntax,
        child: Option<&'a Alias>,
    ) {
        self.lowered.imports.push(Import {
            fact: ImportFact {
                from: place.definition,
                range: bytes(range),
                specifier: Some(specifier.into()),
                conditional,
            },
            syntax,
            child,
        });
    }

    /// `from m import a, b`: the module `m`, then each name that may import a child module of
    /// its own, conditionally.
    fn import_from(&mut self, node: &'a StmtImportFrom, place: Place) {
        let module_end = node.module.as_ref().map(Ranged::end);
        let Some(path) = self.tokens.import_module_path(node.range.start(), module_end) else {
            return;
        };
        let parent = dotted(self.slice(path));
        let index = u32::try_from(self.lowered.imports.len()).unwrap_or(u32::MAX);
        let names = node
            .names
            .iter()
            .map(|alias| {
                let name: Box<str> = alias.name.id.as_str().into();
                (
                    name,
                    alias.asname.as_ref().map(|asname| asname.id.as_str().into()),
                )
            })
            .collect();
        let syntax = ImportSyntax::From {
            level: node.level,
            module: node.module.as_ref().map(|name| name.id.as_str().into()),
            names,
        };
        let before = self.lowered.imports.len();
        self.import(place, path, parent.clone(), false, syntax, None);
        if self.lowered.imports.len() == before {
            return;
        }
        let separator = if parent.ends_with('.') { "" } else { "." };
        for alias in node.names.iter().filter(|alias| is_child_candidate(alias)) {
            let member = self.slice(alias.name.range);
            let syntax = ImportSyntax::Child {
                from: index,
                name: alias.name.id.as_str().into(),
                // Filled in once the binder has declared the name.
                node: 0,
            };
            let specifier = format!("{parent}{separator}{member}");
            self.import(place, alias.name.range, specifier, true, syntax, Some(alias));
        }
    }
}
