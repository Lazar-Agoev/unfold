//! Value resolution: what an expression denotes, by the proof of concept's binder rules.
//!
//! A [`Value`] is what one expression can be proven to be: candidate declarations, a module, a
//! class or an instance of one, an external origin, or a refusal. Resolution follows names to
//! their bindings (a binding with competing writes is never followed), imports to modules and
//! their members, attributes through C3 linearizations and instance fields, and `super()`
//! across every linearization that can reach it. A call is an instance of the class it
//! constructs, or of the class its one proven `def`'s written return type names (an `async
//! def`'s through `await`); nothing is inferred from a body. The standard library's stubs
//! ([`super::library`]) type what the covered modules' functions and classes give, the
//! standard-library classes written as annotations or return types, and literals, and their
//! instances' members are looked up through the stub classes: only a member the class has, and
//! only while the stubs write its type, so a further hop on a value they write no type for is
//! unknown. A member of an instance of a written type is overridden when another class may
//! define it: any class for `object` and protocols, and a project class that derives from the
//! written class and defines the member. It only ever withholds: a value it cannot prove is
//! empty, ambiguous, or refused.

use smallvec::SmallVec;
use unfold_engine::evidence::Refusal;

use super::binder::{Binder, Ctx, Seen, SeenKey};
use super::library::{self, Instance, Known, Stub};
use super::modules::Module;
use super::program::{ExprRef, Imported, NodeRef, Sym};
use crate::bind::ir::{Argument, ExprKind, NodeKind, WriteKind};
use crate::stubs::StubClass;

/// An origin outside the repository: a module specifier and a dotted member path (`*` for the
/// module itself).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct External {
    pub(crate) module: Box<str>,
    pub(crate) member: Box<str>,
}

impl External {
    pub(crate) fn is(&self, module: &str, member: &str) -> bool {
        &*self.module == module && &*self.member == member
    }
}

/// `prefix.name`, without the formatting machinery (identities are built on hot paths).
pub(crate) fn dotted(prefix: &str, name: &str) -> String {
    let mut text = String::with_capacity(prefix.len() + 1 + name.len());
    text.push_str(prefix);
    text.push('.');
    text.push_str(name);
    text
}

/// Which meaning a name is looked up in: as a runtime value (assignments are followed to their
/// values), or as a type (an assignment is its own declaration, never followed;
/// [`super::types`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Meaning {
    Value,
    Type,
}

/// What an expression is proven to be.
#[derive(Debug, Clone, Default)]
pub(crate) struct Value {
    pub(crate) declarations: SmallVec<[NodeRef; 2]>,
    pub(crate) binding: Option<u32>,
    pub(crate) module: Option<Module>,
    pub(crate) class: Option<NodeRef>,
    pub(crate) instance: bool,
    pub(crate) class_parameter: bool,
    pub(crate) external: Option<External>,
    pub(crate) ambiguous: bool,
    pub(crate) refusal: Option<Refusal>,
    pub(crate) excluded: Option<Box<str>>,
    pub(crate) missing: bool,
    pub(crate) opaque_module: bool,
    pub(crate) overridden: bool,
    pub(crate) receiver_class: Option<NodeRef>,
    pub(crate) static_access: bool,
    pub(crate) class_parameter_access: bool,
    pub(crate) super_access: bool,
    pub(crate) super_owner: Option<NodeRef>,
    /// What the standard library's stubs prove it is: an instance of one of their classes, or
    /// a method looked up on one.
    pub(crate) stub: Option<Stub>,
}

impl Value {
    pub(crate) fn refused(refusal: Refusal) -> Self {
        Self {
            refusal: Some(refusal),
            ..Self::default()
        }
    }

    pub(crate) fn ambiguous() -> Self {
        Self {
            ambiguous: true,
            ..Self::default()
        }
    }

    pub(crate) fn external(module: &str, member: &str) -> Self {
        Self {
            external: Some(External {
                module: module.into(),
                member: member.into(),
            }),
            ..Self::default()
        }
    }

    /// An instance of a standard-library class.
    pub(crate) fn library_instance(instance: Option<Instance>) -> Self {
        Self {
            stub: instance.map(Stub::Instance),
            ..Self::default()
        }
    }

    /// An instance of exactly a standard-library class: a literal or a display.
    fn exact(class: Option<&'static StubClass>) -> Self {
        Self::library_instance(class.and_then(|class| library::instance(class, true)))
    }

    pub(crate) fn declared(node: NodeRef, binding: u32) -> Self {
        Self {
            declarations: SmallVec::from_elem(node, 1),
            binding: Some(binding),
            ..Self::default()
        }
    }
}

impl Binder<'_> {
    /// Resolves an expression on a fresh path.
    pub(crate) fn resolve(&self, expr: ExprRef, ctx: &mut Ctx) -> Value {
        self.resolve_seen(expr, &mut Seen::default(), ctx)
    }

    pub(crate) fn resolve_seen(&self, expr: ExprRef, seen: &mut Seen, ctx: &mut Ctx) -> Value {
        self.resolve_in(expr, false, seen, ctx)
    }

    /// Resolves an expression; `awaited` when it is the operand of `await`.
    fn resolve_in(&self, expr: ExprRef, awaited: bool, seen: &mut Seen, ctx: &mut Ctx) -> Value {
        if seen.has(SeenKey::Expr(expr)) || !ctx.enter() {
            return Value::default();
        }
        seen.add(SeenKey::Expr(expr));
        let value = self.resolve_kind(expr, awaited, seen, ctx);
        ctx.leave();
        value
    }

    fn resolve_kind(&self, expr: ExprRef, awaited: bool, seen: &mut Seen, ctx: &mut Ctx) -> Value {
        let program = &self.program;
        let Some(fact) = program.expr(expr) else {
            return Value::default();
        };
        if program.unsupported(expr.file) {
            return Value::refused(Refusal::Unsupported);
        }
        let file = expr.file;
        let local = |id| ExprRef { file, expr: id };
        match &fact.kind {
            &ExprKind::Name(name) => {
                let sym = program.sym(file, name);
                if let Some(binding) = program.lookup(file, fact.scope, sym) {
                    return self.binding_value(binding, seen, ctx);
                }
                if program.unknown_star(file, fact.scope) {
                    return Value::ambiguous();
                }
                let text = program.text(sym);
                if library::builtin(text) {
                    return Value::external("builtins", text);
                }
                Value::default()
            }
            &ExprKind::Lambda(node) => Value {
                declarations: SmallVec::from_elem(NodeRef { file, node }, 1),
                ..Value::default()
            },
            &ExprKind::Member { base, name } => {
                let receiver = self.resolve_seen(local(base), seen, ctx);
                self.member_value(receiver, program.sym(file, name), seen, ctx)
            }
            ExprKind::Call { .. } => self.call_value(expr, awaited, seen, ctx),
            ExprKind::Ternary => Value::ambiguous(),
            &ExprKind::Await(inner) => self.resolve_in(local(inner), true, seen, ctx),
            ExprKind::Index { .. } => Value::refused(Refusal::Dynamic),
            ExprKind::Str(_) => Value::exact(library::class("builtins.str")),
            &ExprKind::Sequence { tuple, .. } => Value::exact(library::class(if tuple {
                "builtins.tuple"
            } else {
                "builtins.list"
            })),
            ExprKind::Dict(_) => Value::exact(library::class("builtins.dict")),
            &ExprKind::Literal(literal) => Value::exact(library::literal(literal)),
            ExprKind::Chain(_) | ExprKind::Other => Value::default(),
        }
    }

    /// What a declaration denotes when it is resolved itself: a function or lambda, or a class.
    pub(crate) fn resolve_node(&self, node: NodeRef) -> Value {
        match self.program.node(node).map(|fact| &fact.kind) {
            Some(NodeKind::Function(_)) => Value {
                declarations: SmallVec::from_elem(node, 1),
                ..Value::default()
            },
            Some(NodeKind::Class(_)) => Value {
                declarations: SmallVec::from_elem(node, 1),
                class: Some(node),
                ..Value::default()
            },
            _ => Value::default(),
        }
    }

    /// What a call is proven to be: an instance of the proven, concrete class it constructs, of
    /// the class its one proven function's written return type names ([`Binder::returned`]),
    /// or of the standard-library class a stub class or function gives ([`library::called`]).
    /// `awaited` when the call is the operand of `await`.
    fn call_value(&self, call: ExprRef, awaited: bool, seen: &mut Seen, ctx: &mut Ctx) -> Value {
        let Some(ExprKind::Call {
            callee,
            arguments,
            function,
        }) = self.program.expr(call).map(|fact| &fact.kind)
        else {
            return Value::default();
        };
        let callee = ExprRef {
            file: call.file,
            expr: *callee,
        };
        let value = self.resolve_seen(callee, seen, ctx);
        if let Some(external) = &value.external {
            if external.is("builtins", "super") {
                return self.super_call(call, arguments, *function, ctx);
            }
            if external.is("builtins", "getattr") {
                return Value::refused(Refusal::Dynamic);
            }
            if external.is("functools", "partial") {
                return Value::refused(Refusal::Wrapper);
            }
            if external.is("importlib", "import_module") {
                return if self.literal_module_name(call.file, arguments).is_some() {
                    Value {
                        opaque_module: true,
                        ..Value::default()
                    }
                } else {
                    Value::refused(Refusal::Dynamic)
                };
            }
        }
        if let Some(class) = value.class
            && !value.instance
            && !value.class_parameter
            && value.refusal.is_none()
            && self.class_access_proven(&value, ctx)
            && self.class_concrete(class, ctx)
        {
            return Value {
                class: Some(class),
                instance: true,
                ..Value::default()
            };
        }
        if let Some(returned) = self.returned(&value, awaited, seen, ctx) {
            return returned;
        }
        if let Some(instance) = library_call(&value, awaited) {
            return Value::library_instance(Some(instance));
        }
        Value {
            ambiguous: value.ambiguous,
            refusal: value.refusal,
            ..Value::default()
        }
    }

    /// What a call through `callee` returns: an instance of the class its written return type
    /// names ([`ReturnFact`](crate::bind::ir::ReturnFact)), when the callee is exactly one
    /// proven, concrete `def`. An `async def`'s return type is read only through `await`, and
    /// any other's only without it. The class must be a proven class that is not wrapped, or a
    /// standard-library class, and an `Optional` must be `typing.Optional`.
    fn returned(&self, callee: &Value, awaited: bool, seen: &Seen, ctx: &mut Ctx) -> Option<Value> {
        let [declaration] = callee.declarations.as_slice() else {
            return None;
        };
        if callee.class.is_some()
            || callee.ambiguous
            || callee.refusal.is_some()
            || callee.overridden
            || !self.class_access_proven(callee, ctx)
            || callee.binding.is_some_and(|binding| !self.stable(binding, ctx))
        {
            return None;
        }
        let function = self.concrete_call(*declaration, ctx)?;
        let Some(NodeKind::Function(fact)) = self.program.node(function).map(|fact| &fact.kind) else {
            return None;
        };
        let returns = fact.returns.filter(|_| fact.asynchronous == awaited)?;
        let written = |expr| ExprRef {
            file: function.file,
            expr,
        };
        if let Some(optional) = returns.optional {
            let identity = self.identity(written(optional), ctx);
            if !matches!(
                identity.as_deref(),
                Some("typing.Optional" | "typing_extensions.Optional")
            ) {
                return None;
            }
        }
        let class = self.resolve_seen(written(returns.class), &mut seen.clone(), ctx);
        if let Some(library) = library_class(&class) {
            return Some(Value::library_instance(library::instance(library, false)));
        }
        let proven = class.class.filter(|_| {
            !class.instance
                && !class.class_parameter
                && !class.ambiguous
                && class.refusal.is_none()
                && self.class_access_proven(&class, ctx)
        })?;
        Some(Value {
            class: Some(proven),
            instance: true,
            ..Value::default()
        })
    }

    /// `super()`: only without arguments, in a method with a named first parameter that is
    /// not a `staticmethod`.
    fn super_call(
        &self,
        call: ExprRef,
        arguments: &[Argument],
        function: Option<u32>,
        ctx: &mut Ctx,
    ) -> Value {
        let method = function.map(|node| NodeRef {
            file: call.file,
            node,
        });
        let owner = method.and_then(|method| self.program.enclosing_definition(method));
        let has_parameters = method.and_then(|method| self.program.node(method)).is_some_and(
            |fact| matches!(&fact.kind, NodeKind::Function(function) if !function.parameters.is_empty()),
        );
        let (Some(method), Some(owner)) = (method, owner) else {
            return Value::refused(Refusal::Unsupported);
        };
        if !arguments.is_empty()
            || !has_parameters
            || !self.program.is_class(owner)
            || self.has_decorator(method, "builtins.staticmethod", ctx)
        {
            return Value::refused(Refusal::Unsupported);
        }
        Value {
            super_owner: Some(owner),
            ..Value::default()
        }
    }

    /// The module a literal `importlib.import_module` call names: one argument, positional or
    /// `name=`, a string literal that is not relative.
    pub(crate) fn literal_module_name(
        &self,
        file: unfold_engine::FileId,
        arguments: &[Argument],
    ) -> Option<&str> {
        let [argument] = arguments else {
            return None;
        };
        if argument.expansion
            || argument
                .keyword
                .is_some_and(|keyword| self.program.text(self.program.sym(file, keyword)) != "name")
        {
            return None;
        }
        let ExprKind::Str(text) = self
            .program
            .expr(ExprRef {
                file,
                expr: argument.value,
            })?
            .kind
        else {
            return None;
        };
        let text = self.program.text(self.program.sym(file, text));
        (!text.is_empty() && !text.starts_with('.')).then_some(text)
    }

    /// Whether a binding has exactly one effective write: one write; a property getter and its
    /// setter; or overload signatures beside one implementation. A mutated binding never does.
    pub(crate) fn stable(&self, binding: u32, ctx: &mut Ctx) -> bool {
        let found = self.program.binding(binding);
        if found.mutated {
            return false;
        }
        let writes = &found.writes;
        if writes.len() == 1 {
            return true;
        }
        let functions = writes
            .iter()
            .all(|&write| self.is_def(self.program.write(write).node));
        if !functions {
            return false;
        }
        if writes.len() == 2
            && writes
                .iter()
                .all(|&write| self.accessor(self.program.write(write).node, ctx))
        {
            return true;
        }
        writes
            .iter()
            .filter(|&&write| !self.overload(self.program.write(write).node, ctx))
            .count()
            == 1
    }

    /// Whether a declaration is a `def` (not a lambda or class).
    pub(crate) fn is_def(&self, node: NodeRef) -> bool {
        matches!(self.program.node(node).map(|fact| &fact.kind), Some(NodeKind::Function(function)) if function.name.is_some())
    }

    /// What a binding denotes.
    pub(crate) fn binding_value(&self, binding: u32, seen: &mut Seen, ctx: &mut Ctx) -> Value {
        let program = &self.program;
        let found = program.binding(binding);
        if program.own_unknown_star(found.file, found.scope) {
            return Value::ambiguous();
        }
        if program.unsupported(found.file) {
            return Value::refused(Refusal::Unsupported);
        }
        let writes = &found.writes;
        if !self.stable(binding, ctx) {
            let declarations = writes.iter().map(|&write| program.write(write).node).collect();
            let imports = writes
                .iter()
                .all(|&write| program.write(write).kind == WriteKind::Import);
            let conditional_import = writes.iter().any(|&write| {
                let write = program.write(write);
                write.conditional && write.kind == WriteKind::Import
            });
            return Value {
                declarations,
                binding: Some(binding),
                ambiguous: imports || conditional_import,
                ..Value::default()
            };
        }
        let chosen = writes
            .iter()
            .copied()
            .find(|&write| !self.overload(program.write(write).node, ctx))
            .or_else(|| writes.first().copied());
        let Some(chosen) = chosen else {
            return Value::default();
        };
        let write = program.write(chosen);
        let node = write.node;
        if matches!(write.kind, WriteKind::Import | WriteKind::Assignment) && seen.has(SeenKey::Node(node)) {
            return Value::default();
        }
        seen.add(SeenKey::Node(node));
        match write.kind {
            WriteKind::Definition => {
                let mut value = Value::declared(node, binding);
                if program.is_class(node) {
                    value.class = Some(node);
                }
                if self.wrapper(node, ctx) {
                    value.refusal = Some(Refusal::Wrapper);
                }
                value
            }
            WriteKind::Import => match &write.import {
                Some(imported) => Value {
                    binding: Some(binding),
                    ..self.import_value(imported, seen, ctx)
                },
                None => Value::declared(node, binding),
            },
            WriteKind::Parameter => self.parameter_value(node, binding, write.annotation, seen, ctx),
            WriteKind::Assignment | WriteKind::Other => {
                match write.value.filter(|_| write.kind == WriteKind::Assignment) {
                    Some(assigned) => {
                        let value = self.resolve_seen(assigned, seen, ctx);
                        let declarations = if value.declarations.is_empty() {
                            SmallVec::from_elem(node, 1)
                        } else {
                            value.declarations.clone()
                        };
                        Value {
                            binding: Some(binding),
                            declarations,
                            ..value
                        }
                    }
                    None => self.annotated(Value::declared(node, binding), write.annotation, seen, ctx),
                }
            }
        }
    }

    /// A value whose written annotation proves an instance of a class, or of a
    /// standard-library class ([`Binder::annotated_library`]).
    fn annotated(&self, mut value: Value, annotation: Option<ExprRef>, seen: &Seen, ctx: &mut Ctx) -> Value {
        if let Some(annotation) = annotation {
            let annotated = self.resolve_seen(annotation, &mut seen.clone(), ctx);
            if let Some(class) = annotated.class
                && self.class_access_proven(&annotated, ctx)
            {
                value.class = Some(class);
                value.instance = true;
            } else {
                value.stub = self
                    .annotated_library(annotation, seen, ctx)
                    .and_then(|class| library::instance(class, false))
                    .map(Stub::Instance);
            }
        }
        value
    }

    /// The standard-library class a written annotation names, its type arguments left out
    /// (`str`, `dict[str, Any]`, `logging.Logger`).
    pub(crate) fn annotated_library(
        &self,
        annotation: ExprRef,
        seen: &Seen,
        ctx: &mut Ctx,
    ) -> Option<&'static StubClass> {
        let written = match self.program.expr(annotation)?.kind {
            ExprKind::Index { base, .. } => ExprRef {
                file: annotation.file,
                expr: base,
            },
            _ => annotation,
        };
        library_class(&self.resolve_seen(written, &mut seen.clone(), ctx))
    }

    /// A parameter: the receiver of an ordinary method (an instance) or a proven `classmethod`
    /// (the class), or else an instance of its annotation's class.
    fn parameter_value(
        &self,
        node: NodeRef,
        binding: u32,
        annotation: Option<ExprRef>,
        seen: &Seen,
        ctx: &mut Ctx,
    ) -> Value {
        let program = &self.program;
        let function = match program.node(node).map(|fact| &fact.kind) {
            Some(&NodeKind::Parameter { function, .. }) => NodeRef {
                file: node.file,
                node: function,
            },
            _ => return Value::declared(node, binding),
        };
        let first = match program.node(function).map(|fact| &fact.kind) {
            Some(NodeKind::Function(fact)) if fact.name.is_some() => fact.parameters.first().copied(),
            _ => None,
        };
        let class = program
            .enclosing_definition(function)
            .filter(|&owner| program.is_class(owner));
        if let Some(class) = class
            && self.is_def(function)
            && first == Some(node.node)
            && !self.has_decorator(function, "builtins.staticmethod", ctx)
        {
            let class_method = self.has_decorator(function, "builtins.classmethod", ctx);
            return Value {
                class: Some(class),
                instance: !class_method,
                class_parameter: class_method,
                ..Value::declared(node, binding)
            };
        }
        self.annotated(Value::declared(node, binding), annotation, seen, ctx)
    }

    /// What an import binds.
    pub(crate) fn import_value(&self, imported: &Imported, seen: &Seen, ctx: &mut Ctx) -> Value {
        self.import_in(imported, Meaning::Value, seen, ctx)
    }

    /// What an import binds, its member looked up in `meaning`.
    pub(crate) fn import_in(
        &self,
        imported: &Imported,
        meaning: Meaning,
        seen: &Seen,
        ctx: &mut Ctx,
    ) -> Value {
        let module = &imported.module;
        if module.ambiguous {
            return Value {
                module: Some(module.clone()),
                ambiguous: true,
                ..Value::default()
            };
        }
        if module.missing {
            return Value {
                module: Some(module.clone()),
                missing: true,
                ..Value::default()
            };
        }
        if module.external.is_some() {
            let member = imported.member.map(|member| self.program.text(member));
            if let Some(member) = member
                && self.external_invalid(&module.specifier, member)
            {
                return Value::refused(Refusal::Dynamic);
            }
            return Value {
                module: member.is_none().then(|| module.clone()),
                ..Value::external(&module.specifier, member.unwrap_or("*"))
            };
        }
        match imported.member {
            None => Value {
                module: Some(module.clone()),
                ..Value::default()
            },
            Some(member) => self.module_member_in(module, member, meaning, seen, ctx),
        }
    }

    fn external_invalid(&self, specifier: &str, member: &str) -> bool {
        let key = format!("external:{specifier}");
        self.invalid_modules.get(key.as_str()).is_some_and(|invalid| {
            invalid.contains(&self.program.known.star)
                || self
                    .program
                    .names
                    .get(member)
                    .is_some_and(|member| invalid.contains(&member))
        })
    }

    /// A module's member: a module-level binding of each file the module is, or a child module.
    pub(crate) fn module_member(&self, module: &Module, member: Sym, seen: &Seen, ctx: &mut Ctx) -> Value {
        self.module_member_in(module, member, Meaning::Value, seen, ctx)
    }

    /// A module's member, its binding looked up in `meaning`.
    pub(crate) fn module_member_in(
        &self,
        module: &Module,
        member: Sym,
        meaning: Meaning,
        seen: &Seen,
        ctx: &mut Ctx,
    ) -> Value {
        if module.ambiguous {
            return Value::ambiguous();
        }
        let text = self.program.text(member);
        if module.external.is_some() {
            if self.external_invalid(&module.specifier, text) {
                return Value::refused(Refusal::Dynamic);
            }
            return Value::external(&module.specifier, text);
        }
        let star = self.program.known.star;
        let mut values: Vec<Value> = Vec::new();
        for path in &module.paths {
            let file = self.modules.indexed(path);
            let invalid = self
                .invalid_modules
                .get(&**path)
                .is_some_and(|invalid| invalid.contains(&member) || invalid.contains(&star));
            if invalid {
                let binding = file.and_then(|file| self.program.module_binding(file, member));
                return match binding {
                    Some(binding) => Value {
                        declarations: self
                            .program
                            .binding(binding)
                            .writes
                            .iter()
                            .map(|&write| self.program.write(write).node)
                            .collect(),
                        binding: Some(binding),
                        ..Value::default()
                    },
                    None => Value::refused(Refusal::Dynamic),
                };
            }
            let Some(file) = file else {
                values.push(Value {
                    excluded: Some(path.clone()),
                    ..Value::default()
                });
                continue;
            };
            if let Some(binding) = self.program.module_binding(file, member) {
                let value = match meaning {
                    Meaning::Value => self.binding_value(binding, &mut seen.clone(), ctx),
                    Meaning::Type => self.type_binding(binding, &mut seen.clone(), ctx),
                };
                values.push(value);
            }
        }
        match values.len() {
            1 => values.pop().unwrap_or_default(),
            0 => match self.modules.member(module, text) {
                Some(child) => Value {
                    ambiguous: child.ambiguous,
                    module: Some(child),
                    ..Value::default()
                },
                None => Value::default(),
            },
            _ => Value {
                declarations: values
                    .iter()
                    .flat_map(|value| value.declarations.iter().copied())
                    .collect(),
                ambiguous: true,
                ..Value::default()
            },
        }
    }

    /// `receiver.member`.
    pub(crate) fn member_value(&self, receiver: Value, member: Sym, seen: &mut Seen, ctx: &mut Ctx) -> Value {
        if receiver.refusal.is_some() || receiver.ambiguous {
            return receiver;
        }
        if !self.class_access_proven(&receiver, ctx) {
            return Value {
                refusal: Some(Refusal::Unsupported),
                ..receiver
            };
        }
        if receiver.opaque_module {
            return Value::refused(Refusal::NotConcrete);
        }
        if let Some(module) = &receiver.module {
            return self.module_member(module, member, seen, ctx);
        }
        if let Some(Stub::Instance(instance)) = receiver.stub {
            return self.library_member(instance, member);
        }
        if let Some(external) = &receiver.external {
            return self.external_member(external, member);
        }
        if let Some(owner) = receiver.super_owner {
            return self.super_member(owner, member, ctx);
        }
        match receiver.class {
            Some(owner) => self.class_member(&receiver, owner, member, seen, ctx),
            None => Value::default(),
        }
    }

    /// Member `member` of an external origin the stubs prove no instance: a member of a module,
    /// or of anything outside the stubs, is a longer path; so is a member a stub class has; a
    /// module variable's member is looked up on its class. Anything else (a member a class
    /// doesn't have, a member of a function or of a value the stubs write no type for) is
    /// unknown.
    fn external_member(&self, external: &External, member: Sym) -> Value {
        let name = self.program.text(member);
        let longer = || Value::external(&external.module, &dotted(&external.member, name));
        match library::known(external) {
            Known::Stub(Stub::Module(_)) | Known::Uncovered => longer(),
            Known::Stub(Stub::Class(class)) if library::lookup(class, name).is_some() => longer(),
            Known::Stub(Stub::Instance(instance)) => self.library_member(instance, member),
            Known::Stub(Stub::Class(_) | Stub::Function { .. }) | Known::Untyped => Value::default(),
        }
    }

    /// Member `member` of an instance of a standard-library class: an external origin in the
    /// class's module, with what the stubs prove it is. A class that declares `__getattr__`
    /// along its linearization makes a member it doesn't declare dynamic; any other member it
    /// doesn't have is unknown. On an instance of a written type, a member another class may
    /// define ([`Binder::library_overridden`]) is overridden, and nothing more is known of it.
    fn library_member(&self, instance: Instance, member: Sym) -> Value {
        let class = instance.class;
        let name = self.program.text(member);
        let Some(found) = library::lookup(class, name) else {
            return if library::lookup(class, "__getattr__").is_some() {
                Value::refused(Refusal::Dynamic)
            } else {
                Value::default()
            };
        };
        let external = Some(library::member_origin(class, name));
        if !instance.exact && self.library_overridden(class, member) {
            return Value {
                external,
                overridden: true,
                ..Value::default()
            };
        }
        Value {
            external,
            stub: library::on_instance(instance, found).filter(|stub| !matches!(stub, Stub::Class(_))),
            ..Value::default()
        }
    }

    /// Whether a class other than a written standard-library `class` may define `member` for its
    /// instances: any class, for `object` and protocols ([`library::open`]); else a project
    /// class that derives from it and defines the member, binds every member through
    /// `__getattribute__`, or has its members written from outside.
    fn library_overridden(&self, class: &StubClass, member: Sym) -> bool {
        if library::open(class) {
            return true;
        }
        let known = &self.program.known;
        self.classes
            .library_overrides
            .get(class.name)
            .is_some_and(|members| {
                members.contains(&member)
                    || members.contains(&known.star)
                    || members.contains(&known.getattribute)
            })
    }
}

/// The standard-library class a value is, as the class itself.
fn library_class(value: &Value) -> Option<&'static StubClass> {
    if value.ambiguous || value.refusal.is_some() {
        return None;
    }
    match library::external(value.external.as_ref()?)? {
        Stub::Class(class) => Some(class),
        _ => None,
    }
}

/// The standard-library instance calling `callee` gives: through a method looked up on an
/// instance, or a stub function or class its external origin denotes. Nothing is known of a
/// call through an overridden method.
fn library_call(callee: &Value, awaited: bool) -> Option<Instance> {
    if callee.ambiguous || callee.refusal.is_some() || callee.overridden {
        return None;
    }
    let stub = match callee.stub {
        Some(method @ Stub::Function { .. }) => method,
        _ => library::external(callee.external.as_ref()?)?,
    };
    library::called(stub, awaited)
}
