//! The type meaning, and how many parameters a call binds itself.
//!
//! **Type names.** A written type name (`crate::bind::types`) is looked up like a value name,
//! but what a binding denotes differs: an assignment or any other write (`Money = Decimal`,
//! `T = TypeVar("T")`) is its own declaration and is never followed to its value, so the policy
//! finds no type declaration (`not_concrete`, or `mutable_binding` when the name is rebound).
//!
//! | Binding | Denotes |
//! | --- | --- |
//! | `class` | the class, decorated or not (a `@dataclass` is its type) |
//! | `def` | the function's declaration, which is no type (`not_concrete`) |
//! | import | followed: a module, an external origin (`typing.Optional` → `python:typing`), or a module member looked up as a type |
//! | assignment, parameter, any other write | that declaration, not its value |
//! | competing writes | as for values: the policy decides (`mutable_binding`, or `ambiguous` for competing imports) |
//! | none | `ambiguous` under a star import with unknown names; external `builtins` for a name typeshed's `builtins` defines ([`crate::stdlib::TYPE_BUILTIN_NAMES`]); else nothing (`unknown_name`) |
//!
//! A qualified name resolves its receiver in the type meaning too (a module, a class for a
//! nested class, an external origin), then the member: a module's member as above, a class's
//! through its linearization (its own body when the linearization is unknown), an external
//! origin's as a longer path.
//!
//! **Bound parameters** (pairing's language question, [`Binder::bound_parameters`]): a
//! construction that reaches `__init__` binds the new instance; a `staticmethod` binds nothing;
//! a `classmethod` binds the class when reached through the class, `cls`, an instance, or
//! `super()`; any other function binds the instance when reached through an instance or
//! `super()`, and nothing when reached through a class, `cls`, a name, or a module. A
//! `classmethod` reached any other way is not proven. A callable passed as an argument follows
//! the same rules, without construction ([`Binder::bound_reference`]): `obj.method` binds the
//! instance, `Class.method` nothing, `Class.classmethod` the class.

use unfold_engine::TypeRef;
use unfold_engine::evidence::{Evidence, Refusal};

use super::answers::import_binding;
use super::binder::{Binder, Ctx, Seen, SeenKey};
use super::program::{ExprRef, NodeRef, Sym};
use super::value::{Meaning, Value, dotted};
use crate::bind::ir::{ExprKind, NodeKind, WriteKind};
use crate::stdlib::is_builtin_type_name;

impl Binder<'_> {
    /// The expression a written type name resolves through.
    pub(crate) fn type_expr(&self, reference: TypeRef) -> Option<ExprRef> {
        let expr = *self
            .program
            .file(reference.file)?
            .types
            .get(reference.index as usize)?;
        Some(ExprRef {
            file: reference.file,
            expr,
        })
    }

    /// The evidence for a written type name, looked up in the type meaning.
    pub(crate) fn type_evidence(&self, expr: ExprRef) -> Evidence {
        let value = self.type_value(expr, &mut Seen::default(), &mut Ctx::new());
        Evidence {
            candidates: self.candidates(&value),
            refusal: value.refusal,
            ambiguous: value.ambiguous,
            import_binding: import_binding(&value),
            ..Evidence::default()
        }
    }

    /// What a name or member chain denotes as a type.
    fn type_value(&self, expr: ExprRef, seen: &mut Seen, ctx: &mut Ctx) -> Value {
        if seen.has(SeenKey::Expr(expr)) || !ctx.enter() {
            return Value::default();
        }
        seen.add(SeenKey::Expr(expr));
        let value = self.type_kind(expr, seen, ctx);
        ctx.leave();
        value
    }

    fn type_kind(&self, expr: ExprRef, seen: &mut Seen, ctx: &mut Ctx) -> Value {
        let program = &self.program;
        let Some(fact) = program.expr(expr) else {
            return Value::default();
        };
        if program.unsupported(expr.file) {
            return Value::refused(Refusal::Unsupported);
        }
        match fact.kind {
            ExprKind::Name(name) => {
                let sym = program.sym(expr.file, name);
                if let Some(binding) = program.lookup(expr.file, fact.scope, sym) {
                    return self.type_binding(binding, seen, ctx);
                }
                if program.unknown_star(expr.file, fact.scope) {
                    return Value::ambiguous();
                }
                let text = program.text(sym);
                if is_builtin_type_name(text) {
                    return Value::external("builtins", text);
                }
                Value::default()
            }
            ExprKind::Member { base, name } => {
                let base = ExprRef {
                    file: expr.file,
                    expr: base,
                };
                let receiver = self.type_value(base, seen, ctx);
                self.type_member(receiver, program.sym(expr.file, name), seen, ctx)
            }
            _ => Value::default(),
        }
    }

    /// What a binding denotes as a type: a class or a definition, an import followed, or any
    /// other write as its own declaration.
    pub(crate) fn type_binding(&self, binding: u32, seen: &mut Seen, ctx: &mut Ctx) -> Value {
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
            let all_imports = writes
                .iter()
                .all(|&write| program.write(write).kind == WriteKind::Import);
            let conditional_import = writes.iter().any(|&write| {
                let write = program.write(write);
                write.conditional && write.kind == WriteKind::Import
            });
            return Value {
                declarations: writes.iter().map(|&write| program.write(write).node).collect(),
                binding: Some(binding),
                ambiguous: all_imports || conditional_import,
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
        match (write.kind, &write.import) {
            (WriteKind::Import, Some(imported)) => {
                // A chain of re-exports is followed once per import and only so deep.
                if seen.has(SeenKey::Node(node)) || !ctx.enter() {
                    return Value::default();
                }
                seen.add(SeenKey::Node(node));
                let value = self.import_in(imported, Meaning::Type, seen, ctx);
                ctx.leave();
                Value {
                    binding: Some(binding),
                    ..value
                }
            }
            _ => {
                let mut value = Value::declared(node, binding);
                if write.kind == WriteKind::Definition && program.is_class(node) {
                    value.class = Some(node);
                }
                value
            }
        }
    }

    /// `receiver.member` as a type: a module's member, an external origin's longer path, or a
    /// class's member through its linearization.
    fn type_member(&self, receiver: Value, member: Sym, seen: &mut Seen, ctx: &mut Ctx) -> Value {
        if receiver.refusal.is_some() || receiver.ambiguous {
            return receiver;
        }
        if let Some(module) = &receiver.module {
            return self.module_member_in(module, member, Meaning::Type, seen, ctx);
        }
        if let Some(external) = &receiver.external {
            let path = dotted(&external.member, self.program.text(member));
            return Value::external(&external.module, &path);
        }
        match receiver.class {
            Some(owner) if !receiver.instance => self.class_type_member(owner, member, seen, ctx),
            _ => Value::default(),
        }
    }

    /// A class's member as a type (`Outer.Inner`): the first binding along its linearization,
    /// or in its own body when the linearization is unknown. A member written from outside the
    /// class is not proven.
    fn class_type_member(&self, owner: NodeRef, member: Sym, seen: &mut Seen, ctx: &mut Ctx) -> Value {
        let lineage: Vec<NodeRef> = match self.mro(owner, ctx) {
            Some(mro) => mro.to_vec(),
            None => vec![owner],
        };
        for class in lineage.into_iter().take_while(|class| !class.is_builtin()) {
            if self.invalid_member(class, member) {
                return Value::refused(Refusal::Dynamic);
            }
            if let Some(binding) = self.class_binding(class, member) {
                return self.type_binding(binding, seen, ctx);
            }
        }
        Value::default()
    }

    /// How many of `function`'s leading parameters a call through `callee` binds itself (see
    /// the module documentation); `None` when that isn't proven.
    pub(crate) fn bound_parameters(&self, callee: ExprRef, function: NodeRef) -> Option<u32> {
        let mut ctx = Ctx::new();
        let static_method = self.has_decorator(function, "builtins.staticmethod", &mut ctx);
        let class_method = self.has_decorator(function, "builtins.classmethod", &mut ctx);
        if self.construction(callee) {
            // A construction's target is the class's own `__init__`, which binds the instance.
            let init = match self.program.node(function).map(|fact| &fact.kind) {
                Some(NodeKind::Function(fact)) => fact
                    .name
                    .is_some_and(|name| self.program.sym(function.file, name) == self.program.known.init),
                _ => false,
            };
            return (init && !static_method && !class_method).then_some(1);
        }
        self.bound_through(callee, static_method, class_method, &mut ctx)
    }

    /// How many of `function`'s leading parameters a callable passed as `value` binds itself:
    /// the same rules as a call through it, without construction.
    pub(crate) fn bound_reference(&self, value: ExprRef, function: NodeRef) -> Option<u32> {
        let mut ctx = Ctx::new();
        let static_method = self.has_decorator(function, "builtins.staticmethod", &mut ctx);
        let class_method = self.has_decorator(function, "builtins.classmethod", &mut ctx);
        self.bound_through(value, static_method, class_method, &mut ctx)
    }

    /// How many leading parameters reaching a function through `callee` binds (not a
    /// construction), given the function's `staticmethod` and `classmethod` decorators: see the
    /// module documentation.
    fn bound_through(
        &self,
        callee: ExprRef,
        static_method: bool,
        class_method: bool,
        ctx: &mut Ctx,
    ) -> Option<u32> {
        if static_method {
            return Some(0);
        }
        // The callee as the policy saw it: a callable instance is called through `__call__`.
        let mut value = self.resolve(callee, ctx);
        if value.instance && value.class.is_some() {
            value = self.member_value(value, self.program.known.call, &mut Seen::default(), ctx);
        }
        if value.super_access {
            return Some(1);
        }
        match value.receiver_class {
            Some(_) if class_method => Some(1),
            Some(_) if value.static_access || value.class_parameter_access => Some(0),
            Some(_) => Some(1),
            None if class_method => None,
            None => Some(0),
        }
    }
}
