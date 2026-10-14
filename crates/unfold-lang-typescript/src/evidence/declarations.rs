//! Declaration semantics (the proof of concept's `frontend/typescript-shared/semantics.ts`):
//! concrete bodies per value context, parameters, binding stability, accessors, class-member
//! facts, overload signatures, and provenance facts.

use unfold_engine::evidence::{ConcreteTarget, MemberFact, OriginDeclaration, Stability, ValueContext};
use unfold_engine::{DeclRef, FileId, NodeRef, ValueRef};

use crate::link::TypeScriptLinked;
use crate::payload::binding::{Decl, DeclFlags, DeclKind, Expr, VarKind};

impl TypeScriptLinked<'_> {
    fn node_ref(file: FileId, local: Option<u32>) -> Option<NodeRef> {
        local.map(|local| NodeRef { file, local })
    }

    /// The inline function or class a declaration's initializer is (after transparent
    /// wrappers), if any.
    fn inline_initializer(&self, declaration: DeclRef, decl: &Decl) -> Option<(DeclRef, bool)> {
        let init = self.unwrap_expr(declaration.file, decl.initializer?);
        match self.expr(declaration.file, init)? {
            Expr::Function(function) => Some((
                DeclRef {
                    file: declaration.file,
                    index: *function,
                },
                false,
            )),
            Expr::Class(class) => Some((
                DeclRef {
                    file: declaration.file,
                    index: *class,
                },
                true,
            )),
            _ => None,
        }
    }

    /// Whether an object-literal member has been written somewhere (the mutation census).
    fn member_written(&self, decl: &Decl, file: FileId) -> bool {
        let Some(object) = decl.object else {
            return false;
        };
        let Some(facts) = self
            .binding(file)
            .and_then(|binding| binding.objects.get(object as usize))
        else {
            return false;
        };
        let variable = DeclRef {
            file,
            index: facts.variable,
        };
        self.tables.written.get(&variable).is_some_and(|written| {
            written.any
                || decl
                    .name
                    .as_deref()
                    .is_some_and(|name| written.names.contains(name))
        })
    }

    /// The declaration's concrete body for `context`. In the type context, only a class,
    /// interface, type alias, or enum declaration is concrete: its own block.
    pub(crate) fn concrete(&self, declaration: DeclRef, context: ValueContext) -> Option<ConcreteTarget> {
        let decl = self.decl(declaration)?;
        if decl.flags.has(DeclFlags::AMBIENT) {
            return None;
        }
        let file = declaration.file;
        if context == ValueContext::Type {
            return match decl.kind {
                DeclKind::Class | DeclKind::Interface | DeclKind::TypeAlias | DeclKind::Enum => {
                    Some(ConcreteTarget {
                        target: Self::node_ref(file, decl.node)?,
                        implementation: declaration,
                    })
                }
                _ => None,
            };
        }
        match decl.kind {
            DeclKind::Function
            | DeclKind::FunctionExpression
            | DeclKind::Arrow
            | DeclKind::Method
            | DeclKind::Constructor => {
                if !decl.flags.has(DeclFlags::HAS_BODY) || self.member_written(decl, file) {
                    return None;
                }
                Some(ConcreteTarget {
                    target: Self::node_ref(file, decl.node)?,
                    implementation: declaration,
                })
            }
            DeclKind::Class | DeclKind::ClassExpression => {
                let target = match context {
                    ValueContext::Construct => {
                        let constructor = decl
                            .constructor
                            .and_then(|constructor| {
                                self.decl(DeclRef {
                                    file,
                                    index: constructor,
                                })
                            })
                            .and_then(|constructor| constructor.node);
                        Self::node_ref(file, constructor.or(decl.node))?
                    }
                    ValueContext::Reference => Self::node_ref(file, decl.node)?,
                    ValueContext::Call | ValueContext::Type => return None,
                };
                Some(ConcreteTarget {
                    target,
                    implementation: declaration,
                })
            }
            DeclKind::Variable | DeclKind::Property => {
                if decl.kind == DeclKind::Variable && decl.var_kind != Some(VarKind::Const) {
                    return None;
                }
                let (inline, class) = self.inline_initializer(declaration, decl)?;
                if class && decl.kind == DeclKind::Property {
                    return None;
                }
                self.concrete(inline, context)
            }
            DeclKind::Expression => self.concrete(
                DeclRef {
                    file,
                    index: decl.inline?,
                },
                context,
            ),
            DeclKind::PropertyAssignment => {
                if self.member_written(decl, file) {
                    return None;
                }
                self.concrete(
                    DeclRef {
                        file,
                        index: decl.inline?,
                    },
                    context,
                )
            }
            _ => None,
        }
    }

    /// Whether a declaration is a parameter or a binding element of one.
    pub(crate) fn in_parameter(&self, declaration: DeclRef) -> bool {
        self.decl(declaration)
            .is_some_and(|decl| decl.kind == DeclKind::Parameter || decl.flags.has(DeclFlags::IN_PARAMETER))
    }

    /// Whether a declaration's binding can be reassigned: `const` is stable, `let` and `var`
    /// are not, and a written member of a constant object literal is not either.
    pub(crate) fn stability(&self, declaration: DeclRef) -> Stability {
        let Some(decl) = self.decl(declaration) else {
            return Stability::Unknown;
        };
        if self.member_written(decl, declaration.file) {
            return Stability::Rebindable;
        }
        let variable = decl.kind == DeclKind::Variable
            || (decl.kind == DeclKind::BindingElement && !decl.flags.has(DeclFlags::IN_PARAMETER));
        if !variable || decl.flags.has(DeclFlags::CATCH) {
            return Stability::Unknown;
        }
        match decl.var_kind {
            Some(VarKind::Const) => Stability::Stable,
            Some(VarKind::Let | VarKind::Var) => Stability::Rebindable,
            _ => Stability::Unknown,
        }
    }

    /// Whether a declaration is a getter or setter.
    pub(crate) fn is_accessor(&self, declaration: DeclRef) -> bool {
        self.decl(declaration)
            .is_some_and(|decl| matches!(decl.kind, DeclKind::Getter | DeclKind::Setter))
    }

    /// Class-member facts for the override census.
    pub(crate) fn member_facts(&self, declaration: DeclRef) -> MemberFact<'_> {
        let Some(decl) = self.decl(declaration) else {
            return MemberFact::default();
        };
        MemberFact {
            owner: decl.owner.map(|owner| DeclRef {
                file: declaration.file,
                index: owner,
            }),
            name: decl.name.as_deref(),
            is_static: decl.flags.has(DeclFlags::STATIC),
            is_private: decl.flags.has(DeclFlags::PRIVATE),
            check_override: matches!(decl.kind, DeclKind::Method | DeclKind::Property),
            can_override: decl.kind != DeclKind::Constructor
                && !decl.flags.has(DeclFlags::AMBIENT)
                && (decl.flags.has(DeclFlags::HAS_BODY)
                    || matches!(decl.kind, DeclKind::Property | DeclKind::Parameter)),
        }
    }

    /// Whether `signature` is a compatible overload signature of `implementation`.
    pub(crate) fn is_overload(&self, signature: DeclRef, implementation: DeclRef) -> bool {
        let (Some(sig), Some(body)) = (self.decl(signature), self.decl(implementation)) else {
            return false;
        };
        signature.file == implementation.file
            && sig.kind.is_overloadable()
            && !sig.flags.has(DeclFlags::HAS_BODY)
            && !sig.flags.has(DeclFlags::AMBIENT)
            && sig.kind == body.kind
            && sig.container == body.container
            && sig.name == body.name
    }

    /// What provenance may follow from a declaration.
    pub(crate) fn origin(&self, declaration: DeclRef) -> OriginDeclaration {
        let Some(decl) = self.decl(declaration) else {
            return OriginDeclaration::default();
        };
        OriginDeclaration {
            factory_callee: decl.factory.map(|value| ValueRef {
                file: declaration.file,
                value,
            }),
            annotation: matches!(
                decl.kind,
                DeclKind::Parameter | DeclKind::Variable | DeclKind::Property
            ),
        }
    }
}
