//! Lowering expressions: calls (with their normalized callees), decorators that are not calls,
//! lambdas, conditional expressions and comprehensions that contain a call, and the flags an
//! expression's position gives its calls (`await`, the right side of `and`/`or`). Every other
//! expression is code, and lowering looks through it for the blocks inside.

use ruff_python_ast::token::TokenKind;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    ArgOrKeyword, Arguments, Comprehension, Expr, ExprCall, ExprIf, ExprLambda, ModModule, Stmt,
};
use ruff_text_size::{Ranged, TextSize};
use rustc_hash::FxHashSet;
use unfold_core::key::{Callee, Child};
use unfold_core::vocab::{CallRole, Flag, Form, Inline};
use unfold_engine::SourceFailure;
use unfold_engine::facts::ImportFact;

use super::{Called, Import, Lowering, Place, Value, span};
use crate::ast::{arguments, bytes, expr_children, string_literal};
use crate::payload::ImportSyntax;
use crate::typed;

/// An expression's identity in the tree.
pub(crate) fn address(expr: &Expr) -> usize {
    std::ptr::from_ref(expr) as usize
}

/// A statement's identity in the tree.
pub(super) fn statement_address(stmt: &Stmt) -> usize {
    std::ptr::from_ref(stmt) as usize
}

/// The `if` expressions and comprehensions of `module` that contain a call (by address), and the
/// `def` statements and lambdas whose own body yields.
pub(super) fn shapes(module: &ModModule) -> (FxHashSet<usize>, FxHashSet<usize>) {
    let mut shapes = Shapes::default();
    shapes.visit_body(&module.body);
    (shapes.calling, shapes.generators)
}

#[derive(Default)]
struct Shapes {
    calling: FxHashSet<usize>,
    generators: FxHashSet<usize>,
    /// The `def` or lambda whose body is being visited.
    function: Option<usize>,
    /// Whether the expression being visited contains a call so far.
    called: bool,
}

impl<'a> Visitor<'a> for Shapes {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        stacker::maybe_grow(super::RED_ZONE, super::STACK_GROWTH, || {
            let Stmt::FunctionDef(function) = stmt else {
                visitor::walk_stmt(self, stmt);
                return;
            };
            for decorator in &function.decorator_list {
                self.visit_decorator(decorator);
            }
            if let Some(parameters) = &function.type_params {
                self.visit_type_params(parameters);
            }
            self.visit_parameters(&function.parameters);
            if let Some(returns) = &function.returns {
                self.visit_annotation(returns);
            }
            let outer = self.function.replace(statement_address(stmt));
            self.visit_body(&function.body);
            self.function = outer;
        });
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        stacker::maybe_grow(super::RED_ZONE, super::STACK_GROWTH, || {
            let outer = std::mem::replace(&mut self.called, false);
            match expr {
                Expr::Lambda(lambda) => {
                    if let Some(parameters) = &lambda.parameters {
                        self.visit_parameters(parameters);
                    }
                    let enclosing = self.function.replace(address(expr));
                    self.visit_expr(&lambda.body);
                    self.function = enclosing;
                }
                Expr::Yield(_) | Expr::YieldFrom(_) => {
                    self.generators.extend(self.function);
                    visitor::walk_expr(self, expr);
                }
                _ => visitor::walk_expr(self, expr),
            }
            self.called |= matches!(expr, Expr::Call(_));
            let decides = matches!(
                expr,
                Expr::If(_) | Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::Generator(_)
            );
            if self.called && decides {
                self.calling.insert(address(expr));
            }
            self.called |= outer;
        });
    }
}

/// A call's callee as keys and names write it: arguments are `()`, subscripts `[]`, a string
/// `""`, any other expression `(…)`; `await` and parentheses (which the tree doesn't keep) are
/// dropped. Names keep their written spelling.
fn callee(text: &str, expr: &Expr) -> Callee {
    enum Step<'s> {
        Member(&'s str),
        Call,
        Index,
    }
    let slice = |range: ruff_text_size::TextRange| {
        text.get(range.start().to_usize()..range.end().to_usize())
            .unwrap_or_default()
    };
    let mut steps = Vec::new();
    let mut current = expr;
    let root = loop {
        match current {
            Expr::Attribute(access) => {
                steps.push(Step::Member(slice(access.attr.range)));
                current = &access.value;
            }
            Expr::Call(call) => {
                steps.push(Step::Call);
                current = &call.func;
            }
            Expr::Subscript(subscript) => {
                steps.push(Step::Index);
                current = &subscript.value;
            }
            Expr::Await(awaited) => current = &awaited.value,
            Expr::Name(name) => break Callee::name(slice(name.range)),
            Expr::StringLiteral(_) | Expr::BytesLiteral(_) | Expr::FString(_) | Expr::TString(_) => {
                break Callee::string();
            }
            _ => break Callee::expression(),
        }
    };
    steps.into_iter().rev().fold(root, |callee, step| match step {
        Step::Member(name) => callee.member(name),
        Step::Call => callee.call(),
        Step::Index => callee.index(),
    })
}

/// The value of a call's first argument, when it is written positionally as a plain string.
fn first_literal(written: &Arguments) -> Option<&str> {
    match arguments(written).next()? {
        ArgOrKeyword::Arg(first) => string_literal(first),
        ArgOrKeyword::Keyword(_) => None,
    }
}

impl<'a> Lowering<'a> {
    /// Lowers `expr`, whose own value is used as `value`.
    pub(super) fn expr(
        &mut self,
        expr: &'a Expr,
        place: Place,
        value: Value<'a>,
    ) -> Result<(), SourceFailure> {
        self.grow(|this| this.expr_here(expr, place, value))
    }

    fn expr_here(&mut self, expr: &'a Expr, place: Place, value: Value<'a>) -> Result<(), SourceFailure> {
        match expr {
            Expr::Call(call) => self.call(expr, call, place, value),
            Expr::Await(awaited) => self.expr(
                &awaited.value,
                place,
                Value {
                    awaited: true,
                    ..value
                },
            ),
            Expr::Lambda(lambda) => self.lambda(expr, lambda, place, None),
            Expr::If(node) if self.calling.contains(&address(expr)) => self.ternary(expr, node, place, value),
            Expr::ListComp(node) if self.calling.contains(&address(expr)) => {
                self.comprehension(expr, &[&node.elt], &node.generators, place, value)
            }
            Expr::SetComp(node) if self.calling.contains(&address(expr)) => {
                self.comprehension(expr, &[&node.elt], &node.generators, place, value)
            }
            Expr::Generator(node) if self.calling.contains(&address(expr)) => {
                self.comprehension(expr, &[&node.elt], &node.generators, place, value)
            }
            Expr::DictComp(node) if self.calling.contains(&address(expr)) => {
                let elements: Vec<&'a Expr> = node.key.as_deref().into_iter().chain([&*node.value]).collect();
                self.comprehension(expr, &elements, &node.generators, place, value)
            }
            Expr::BoolOp(node) => {
                for (index, operand) in node.values.iter().enumerate() {
                    let operand_place = Place {
                        conditional: place.conditional || index > 0,
                        ..place
                    };
                    self.expr(operand, operand_place, Value::default())?;
                }
                Ok(())
            }
            Expr::Named(named) => {
                let binds = Value {
                    binds: Some(self.slice(named.target.range())),
                    ..value
                };
                self.expr(&named.value, place, binds)
            }
            _ => {
                let mut children: Vec<&'a Expr> = Vec::new();
                expr_children(expr, &mut children);
                for child in children {
                    self.expr(child, place, Value::default())?;
                }
                Ok(())
            }
        }
    }

    /// Whether lowering `expr` reaches an unnamed function before any other block: one written
    /// in a call's arguments this way is its callback.
    fn reaches_inline(&self, expr: &'a Expr) -> bool {
        let mut pending = vec![expr];
        let mut children = Vec::new();
        while let Some(expr) = pending.pop() {
            match expr {
                Expr::Lambda(_) => return true,
                Expr::Call(_) => {}
                Expr::If(_)
                | Expr::ListComp(_)
                | Expr::SetComp(_)
                | Expr::DictComp(_)
                | Expr::Generator(_)
                    if self.calling.contains(&address(expr)) => {}
                _ => {
                    children.clear();
                    expr_children(expr, &mut children);
                    pending.extend(children.iter().copied());
                }
            }
        }
        false
    }

    /// A call: its node, then the blocks in its callee and its arguments, in that order.
    pub(super) fn call(
        &mut self,
        expr: &'a Expr,
        call: &'a ExprCall,
        place: Place,
        value: Value<'a>,
    ) -> Result<(), SourceFailure> {
        let callee = callee(self.text, &call.func);
        let takes_callback = arguments(&call.arguments).any(|argument| self.reaches_inline(argument.value()));
        let child = Child::Call {
            callee: &callee,
            first_literal: first_literal(&call.arguments),
            takes_callback,
        };
        let id = self.push(place, child, Form::Call, call.range())?;
        self.called(id, place, value, call.func.range());
        self.lowered.calls.push(Called {
            node: id,
            expr,
            callee: &call.func,
            arguments: Some(&call.arguments),
        });
        let Some(inside) = self.inside(id, place) else {
            return Ok(());
        };
        if let Some((name, literal)) = self.proofs.dynamic_import(call) {
            self.lowered.imports.push(Import {
                fact: ImportFact {
                    from: id,
                    range: bytes(literal),
                    specifier: Some(name.into()),
                    conditional: false,
                },
                syntax: ImportSyntax::Dynamic,
                child: None,
            });
        }
        self.expr(&call.func, inside, Value::default())?;
        let in_arguments = Place {
            argument: true,
            ..inside
        };
        for argument in arguments(&call.arguments) {
            self.expr(argument.value(), in_arguments, Value::default())?;
        }
        Ok(())
    }

    /// A decorator that is not a call expression, such as `@property`: a call of what it names.
    pub(super) fn decorator_call(&mut self, expr: &'a Expr, place: Place) -> Result<(), SourceFailure> {
        let callee = callee(self.text, expr);
        let child = Child::Call {
            callee: &callee,
            first_literal: None,
            takes_callback: false,
        };
        let id = self.push(place, child, Form::Call, expr.range())?;
        self.called(id, place, Value::default(), expr.range());
        self.lowered.calls.push(Called {
            node: id,
            expr,
            callee: expr,
            arguments: None,
        });
        match self.inside(id, place) {
            Some(inside) => self.expr(expr, inside, Value::default()),
            None => Ok(()),
        }
    }

    /// Fills a call node's callee range, role, value, and flags.
    fn called(&mut self, id: u32, place: Place, value: Value<'a>, head: ruff_text_size::TextRange) {
        let node = self.node(id);
        node.head = Some(bytes(head));
        node.role = place.role;
        node.binds = value.binds.map(Into::into);
        for (flag, set) in [
            (Flag::ExitValue, value.exit_value),
            (Flag::Awaited, value.awaited),
            (Flag::Conditional, place.conditional),
        ] {
            if set {
                node.flags.insert(flag);
            }
        }
        if matches!(
            place.role,
            Some(CallRole::Decorator | CallRole::Default | CallRole::Base)
        ) {
            self.owned.push(id);
        }
    }

    /// Records what a ternary or comprehension's value is used as.
    fn valued(&mut self, id: u32, value: Value<'a>) {
        let node = self.node(id);
        node.binds = value.binds.map(Into::into);
        if value.exit_value {
            node.flags.insert(Flag::ExitValue);
        }
    }

    /// A lambda, named when it is exactly a simple assignment's value: its defaults (calls beside
    /// it, role `default`), then the function and its body.
    pub(super) fn lambda(
        &mut self,
        expr: &'a Expr,
        lambda: &'a ExprLambda,
        place: Place,
        name: Option<&'a str>,
    ) -> Result<(), SourceFailure> {
        let start = self.owned.len();
        if let Some(parameters) = &lambda.parameters {
            for default in parameters.iter().filter_map(|parameter| parameter.default()) {
                self.expr(default, place.with_role(CallRole::Default), Value::default())?;
            }
        }
        let child = match name {
            Some(name) => Child::Definition(name),
            None if place.argument => Child::Inline(Inline::Callback),
            None => Child::Inline(Inline::Lambda),
        };
        let id = self.push(place, child, Form::Lambda, lambda.range)?;
        self.own(start, id);
        let keyword_end = lambda.range.start() + TextSize::of("lambda");
        let head_end = lambda
            .parameters
            .as_deref()
            .map_or(keyword_end, |parameters| parameters.range.end());
        let generator = self.generators.contains(&address(expr));
        let node = self.node(id);
        node.head = Some(bytes(span(lambda.range.start(), head_end)));
        node.body = Some(bytes(lambda.body.range()));
        if generator {
            node.flags.insert(Flag::Generator);
        }
        self.lowered.definitions.insert(address(expr), id);
        typed::signature(&mut self.lowered, id, None, self.text);
        if let Some(parameters) = &lambda.parameters {
            typed::parameters(&mut self.lowered, id, parameters, self.text);
        }
        if self.inside(id, place).is_some() {
            self.expr(&lambda.body, place.definition(id), Value::default())?;
        }
        Ok(())
    }

    /// `body if test else orelse` with a call: a `ternary` decision whose `then` part has the test
    /// and the body, and whose `else` part has the alternative.
    fn ternary(
        &mut self,
        expr: &'a Expr,
        node: &'a ExprIf,
        place: Place,
        value: Value<'a>,
    ) -> Result<(), SourceFailure> {
        let id = self.push(place, Child::Form(Form::Ternary), Form::Ternary, expr.range())?;
        self.valued(id, value);
        let keyword = self.tokens.last_before(TokenKind::Else, node.orelse.start());
        let then_end = keyword
            .and_then(|keyword| self.tokens.before(keyword.start()))
            .map_or(node.test.end(), Ranged::end);
        let else_start = keyword.map_or(node.orelse.start(), Ranged::start);
        let parts = place.under(id);
        let then = self.push(
            parts,
            Child::Form(Form::Then),
            Form::Then,
            span(node.body.start(), then_end),
        )?;
        let part = self.node(then);
        part.head = Some(bytes(node.test.range()));
        part.body = Some(bytes(node.body.range()));
        if let Some(inside) = self.inside(then, parts) {
            self.expr(&node.test, inside.with_role(CallRole::Test), Value::default())?;
            self.expr(&node.body, inside, Value::default())?;
        }
        let otherwise = self.push(
            parts,
            Child::Form(Form::Else),
            Form::Else,
            span(else_start, node.orelse.end()),
        )?;
        self.node(otherwise).body = Some(bytes(node.orelse.range()));
        if let Some(inside) = self.inside(otherwise, parts) {
            self.expr(&node.orelse, inside, Value::default())?;
        }
        Ok(())
    }

    /// A comprehension or generator expression with a call: a `comprehension` loop whose header
    /// is its `for` clauses, and whose body part has the `if` clauses as its test and the
    /// element as its body.
    fn comprehension(
        &mut self,
        expr: &'a Expr,
        elements: &[&'a Expr],
        generators: &'a [Comprehension],
        place: Place,
        value: Value<'a>,
    ) -> Result<(), SourceFailure> {
        let id = self.push(
            place,
            Child::Form(Form::Comprehension),
            Form::Comprehension,
            expr.range(),
        )?;
        self.valued(id, value);
        if let (Some(first), Some(last)) = (generators.first(), generators.last()) {
            self.node(id).head = Some(bytes(span(first.range.start(), last.iter.end())));
        }
        if let Some(inside) = self.inside(id, place) {
            for generator in generators {
                self.expr(&generator.iter, inside, Value::default())?;
                self.expr(&generator.target, inside, Value::default())?;
            }
        }
        let tests: Vec<&'a Expr> = generators.iter().flat_map(|generator| &generator.ifs).collect();
        let element_start = elements.first().map_or(expr.start(), |element| element.start());
        let element_end = elements.last().map_or(expr.end(), |element| element.end());
        let test = match (tests.first(), tests.last()) {
            (Some(first), Some(last)) => Some(span(first.start(), last.end())),
            _ => None,
        };
        // The shortest range holding the element and its tests: it covers the `for` clauses
        // written before the last test (Python writes the element first), never one after it.
        let end = test.map_or(element_end, |test| test.end().max(element_end));
        let parts = place.under(id);
        let body = self.push(
            parts,
            Child::Form(Form::Body),
            Form::Body,
            span(element_start, end),
        )?;
        let part = self.node(body);
        part.head = test.map(bytes);
        part.body = Some(bytes(span(element_start, element_end)));
        if let Some(inside) = self.inside(body, parts) {
            for test in tests {
                self.expr(test, inside.with_role(CallRole::Test), Value::default())?;
            }
            for element in elements {
                self.expr(element, inside, Value::default())?;
            }
        }
        Ok(())
    }
}
