//! Member lookup on classes and their instances: through the C3 linearization and instance
//! fields, `super()` agreement across every linearization that can reach it, and whether a
//! receiver family may dispatch elsewhere.

use smallvec::SmallVec;
use unfold_engine::evidence::Refusal;

use super::binder::{Binder, Ctx, Seen};
use super::library::{self, Stub};
use super::program::{NodeRef, Sym};
use super::value::Value;
use crate::stubs::StubClass;

/// The class an instance field holds an instance of.
#[derive(Debug, Clone, Copy)]
enum Held {
    Class(NodeRef),
    Library(&'static StubClass),
}

impl Held {
    fn same(self, other: Self) -> bool {
        match (self, other) {
            (Self::Class(left), Self::Class(right)) => left == right,
            (Self::Library(left), Self::Library(right)) => left.name == right.name,
            _ => false,
        }
    }
}

impl Binder<'_> {
    pub(crate) fn has_field(&self, class: NodeRef, member: Sym) -> bool {
        self.fields
            .get(&class)
            .is_some_and(|fields| fields.contains_key(&member))
    }

    pub(crate) fn reflective(&self, class: NodeRef) -> bool {
        self.class_binding(class, self.program.known.getattribute)
            .is_some()
    }

    /// A member of a class or of its instances, through the linearization and instance fields.
    pub(crate) fn class_member(
        &self,
        receiver: &Value,
        owner: NodeRef,
        member: Sym,
        seen: &mut Seen,
        ctx: &mut Ctx,
    ) -> Value {
        if self.wrapper(owner, ctx) {
            return Value::refused(Refusal::Wrapper);
        }
        if self.invalid_member(owner, member) {
            return Value::refused(Refusal::Dynamic);
        }
        let Some(mro) = self.mro(owner, ctx) else {
            let own = self.class_binding(owner, member);
            let abstract_only = own.is_some_and(|binding| {
                self.program
                    .binding(binding)
                    .writes
                    .iter()
                    .all(|&write| self.is_abstract(self.program.write(write).node, ctx))
            });
            return if abstract_only {
                Value::refused(Refusal::NotConcrete)
            } else {
                Value::default()
            };
        };
        if receiver.instance && mro.iter().any(|&base| self.reflective(base)) {
            return Value::refused(Refusal::Dynamic);
        }
        if mro.iter().any(|&base| self.invalid_member(base, member)) {
            return Value::refused(Refusal::Dynamic);
        }
        if receiver.instance
            && let Some(field) = self.field_value(owner, &mro, member, seen, ctx)
        {
            return field;
        }
        for &base in mro.iter() {
            // A builtin class's members are not known: the lookup proves nothing past it.
            if base.is_builtin() {
                break;
            }
            let Some(binding) = self.class_binding(base, member) else {
                continue;
            };
            let value = self.binding_value(binding, seen, ctx);
            let dispatches = receiver.instance || receiver.class_parameter;
            let overridden = dispatches && self.receiver_may_differ(owner, member, binding, ctx);
            let result = Value {
                receiver_class: Some(owner),
                static_access: !receiver.instance && !receiver.class_parameter,
                class_parameter_access: receiver.class_parameter,
                overridden: value.overridden || overridden,
                ..value
            };
            return if self.class_access_proven(&result, ctx) {
                result
            } else {
                Value {
                    refusal: result.refusal.or(Some(Refusal::Unsupported)),
                    ..result
                }
            };
        }
        Value::default()
    }

    /// An instance field found along the linearization: an instance of one class (or
    /// standard-library class) proven by its annotations and a single constructor assignment,
    /// unless a descendant may change it.
    /// `None` when no class in the linearization has the field.
    pub(crate) fn field_value(
        &self,
        owner: NodeRef,
        mro: &[NodeRef],
        member: Sym,
        seen: &Seen,
        ctx: &mut Ctx,
    ) -> Option<Value> {
        let field = mro.iter().find_map(|base| self.fields.get(base)?.get(&member))?;
        let changed = self
            .descendants(owner)
            .filter(|&candidate| candidate != owner)
            .any(|candidate| {
                let Some(theirs) = self.mro(candidate, ctx) else {
                    return true;
                };
                theirs.iter().any(|&ancestor| {
                    !mro.contains(&ancestor)
                        && (self.has_field(ancestor, member)
                            || self.invalid_member(ancestor, member)
                            || self.reflective(ancestor))
                })
            });
        if changed {
            return Some(Value::refused(Refusal::Dynamic));
        }
        if field.writes.len() > 1
            || field
                .writes
                .iter()
                .any(|&(value, constructor)| !constructor || value.is_none())
        {
            return Some(Value::default());
        }
        let mut classes: SmallVec<[Held; 2]> = SmallVec::new();
        for &annotation in &field.annotations {
            let value = self.written_type(annotation, seen, ctx);
            if !self.class_access_proven(&value, ctx) {
                return Some(Value::default());
            }
            match value.class {
                Some(class) => classes.push(Held::Class(class)),
                None => classes.extend(self.annotated_library(annotation, seen, ctx).map(Held::Library)),
            }
        }
        let mut assigned = SmallVec::<[Held; 2]>::new();
        for &(value, _) in &field.writes {
            let Some(value) = value else {
                return Some(Value::default());
            };
            let value = self.resolve_seen(value, &mut seen.clone(), ctx);
            if value.refusal.is_some() || value.ambiguous {
                return Some(Value::default());
            }
            match (value.class, value.stub) {
                (Some(class), _) if value.instance => assigned.push(Held::Class(class)),
                (None, Some(Stub::Instance(instance))) => assigned.push(Held::Library(instance.class)),
                _ => return Some(Value::default()),
            }
        }
        classes.extend(assigned);
        match classes.first() {
            Some(&first) if classes.iter().all(|&other| other.same(first)) => Some(match first {
                Held::Class(class) => Value {
                    class: Some(class),
                    instance: true,
                    ..Value::default()
                },
                Held::Library(class) => Value::library_instance(library::instance(class, false)),
            }),
            _ => Some(Value::default()),
        }
    }

    /// `super().member` in `owner`: the member that follows `owner` must be the same in the
    /// linearization of every class that can reach it.
    pub(crate) fn super_member(&self, owner: NodeRef, member: Sym, ctx: &mut Ctx) -> Value {
        let mut targets: SmallVec<[NodeRef; 2]> = SmallVec::new();
        let candidates: Vec<NodeRef> = self.descendants(owner).collect();
        for candidate in candidates {
            let Some(mro) = self.mro(candidate, ctx) else {
                return Value::refused(Refusal::Unsupported);
            };
            let start = mro.iter().position(|&base| base == owner).map_or(0, |at| at + 1);
            for &base in &mro[start..] {
                // A builtin class may define the member: `super()` can't be followed past it.
                if base.is_builtin() {
                    return Value::refused(Refusal::Unsupported);
                }
                if self.invalid_member(base, member) {
                    return Value::refused(Refusal::Dynamic);
                }
                if let Some(binding) = self.class_binding(base, member) {
                    for target in self
                        .binding_value(binding, &mut Seen::default(), ctx)
                        .declarations
                    {
                        if !targets.contains(&target) {
                            targets.push(target);
                        }
                    }
                    break;
                }
            }
        }
        Value {
            ambiguous: targets.len() > 1,
            declarations: targets,
            receiver_class: Some(owner),
            super_access: true,
            ..Value::default()
        }
    }

    /// Whether a receiver of `owner`'s family may dispatch `member` to another binding than
    /// `expected`: a descendant without a linearization, one that changes the member's lookup,
    /// or one whose linearization finds another binding first.
    pub(crate) fn receiver_may_differ(
        &self,
        owner: NodeRef,
        member: Sym,
        expected: u32,
        ctx: &mut Ctx,
    ) -> bool {
        let candidates: Vec<NodeRef> = self.descendants(owner).collect();
        for candidate in candidates {
            let Some(mro) = self.mro(candidate, ctx) else {
                return true;
            };
            if mro.iter().any(|&base| {
                self.invalid_member(base, member) || self.has_field(base, member) || self.reflective(base)
            }) {
                return true;
            }
            let actual = mro
                .iter()
                .take_while(|base| !base.is_builtin())
                .find_map(|&base| self.class_binding(base, member));
            if actual != Some(expected) {
                return true;
            }
        }
        false
    }
}
