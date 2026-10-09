//! Declaration semantics (the proof of concept's `frontend/typescript-shared/semantics.ts`):
//! concrete bodies per value context, parameters and their runtime positions, binding
//! stability, accessors, class-member facts, overload signatures, provenance facts, and the
//! manual-locator lookups (whose rules deliberately differ from the value policy).

use unfold_engine::evidence::{
    ConcreteTarget, LocatorMembers, MemberFact, OriginDeclaration, ParameterFact, Stability, ValueContext,
};
use unfold_engine::{BlockRef, DeclRef, FileId, ValueRef};

use crate::link::TypeScriptLinked;
use crate::payload::binding::{Decl, DeclFlags, DeclKind, Expr, VarKind};

impl TypeScriptLinked<'_> {
    fn block_ref(file: FileId, local: Option<u32>) -> Option<BlockRef> {
        local.map(|local| BlockRef { file, local })
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

    /// The declaration's concrete body for `context`.
    pub(crate) fn concrete(&self, declaration: DeclRef, context: ValueContext) -> Option<ConcreteTarget> {
        let decl = self.decl(declaration)?;
        if decl.flags.has(DeclFlags::AMBIENT) {
            return None;
        }
        let file = declaration.file;
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
                    target: Self::block_ref(file, decl.block)?,
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
                            .and_then(|constructor| constructor.block);
                        Self::block_ref(file, constructor.or(decl.block))?
                    }
                    ValueContext::Reference | ValueContext::Handler => Self::block_ref(file, decl.block)?,
                    ValueContext::Call => return None,
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

    /// Parameter facts of a parameter or a binding element of one.
    pub(crate) fn parameter_facts(&self, declaration: DeclRef) -> Option<ParameterFact> {
        let info = self.decl(declaration)?.parameter?;
        let file = declaration.file;
        let owner = self
            .decl(DeclRef {
                file,
                index: info.owner,
            })
            .and_then(|owner| owner.block);
        Some(ParameterFact {
            parameter: DeclRef {
                file,
                index: info.parameter,
            },
            owner: Self::block_ref(file, owner),
            position: Some(info.position),
        })
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

    // --- Locators -------------------------------------------------------------------------

    /// The top-level function, class, and variable declarations of `file` named `name`.
    pub(crate) fn top_level(&self, file: FileId, name: &str) -> Vec<DeclRef> {
        let Some(binding) = self.binding(file) else {
            return Vec::new();
        };
        binding
            .declarations
            .iter()
            .enumerate()
            .filter(|(_, decl)| {
                decl.flags.has(DeclFlags::TOP_LEVEL)
                    && matches!(
                        decl.kind,
                        DeclKind::Function | DeclKind::Class | DeclKind::Variable
                    )
                    && decl.name.as_deref() == Some(name)
            })
            .map(|(index, _)| DeclRef {
                file,
                index: index as u32,
            })
            .collect()
    }

    /// The block a locator names for a declaration.
    pub(crate) fn locator_block(&self, declaration: DeclRef) -> Option<BlockRef> {
        let decl = self.decl(declaration)?;
        if decl.flags.has(DeclFlags::AMBIENT) {
            return None;
        }
        let file = declaration.file;
        match decl.kind {
            DeclKind::Function | DeclKind::FunctionExpression | DeclKind::Arrow | DeclKind::Method => {
                Self::block_ref(file, decl.block.filter(|_| decl.flags.has(DeclFlags::HAS_BODY)))
            }
            DeclKind::Variable => {
                if decl.var_kind != Some(VarKind::Const) {
                    return None;
                }
                let (inline, _) = self.inline_initializer(declaration, decl)?;
                self.locator_block(inline)
            }
            DeclKind::Expression => self.locator_block(DeclRef {
                file,
                index: decl.inline?,
            }),
            DeclKind::Class
            | DeclKind::ClassExpression
            | DeclKind::Constructor
            | DeclKind::Getter
            | DeclKind::Setter => Self::block_ref(file, decl.block),
            _ => None,
        }
    }

    /// The members named `name` of a class or constant object literal a locator names.
    pub(crate) fn members_named(&self, owner: DeclRef, name: &str) -> LocatorMembers {
        let Some(decl) = self.decl(owner) else {
            return LocatorMembers::default();
        };
        let file = owner.file;
        let mut class = matches!(decl.kind, DeclKind::Class | DeclKind::ClassExpression).then_some(owner);
        let mut object = None;
        if decl.kind == DeclKind::Variable && decl.var_kind == Some(VarKind::Const) {
            if let Some((inline, true)) = self.inline_initializer(owner, decl) {
                class = Some(inline);
            }
            object = self.tables.objects.get(&owner).copied();
        }
        let members: Vec<u32> = if let Some(class) = class {
            self.tables
                .classes
                .get(&class)
                .map(|tables| {
                    tables
                        .members
                        .iter()
                        .filter(|member| {
                            self.decl(**member)
                                .is_some_and(|decl| decl.kind != DeclKind::Parameter)
                        })
                        .map(|member| member.index)
                        .collect()
                })
                .unwrap_or_default()
        } else if let Some(object) = object {
            self.binding(file)
                .and_then(|binding| binding.objects.get(object as usize))
                .map(|facts| facts.members.clone())
                .unwrap_or_default()
        } else {
            return LocatorMembers::default();
        };
        let mut declarations = Vec::new();
        for member in members {
            let reference = DeclRef { file, index: member };
            let Some(found) = self.decl(reference) else {
                continue;
            };
            let spelled = if found.kind == DeclKind::Constructor {
                Some("constructor")
            } else {
                found.name.as_deref()
            };
            if spelled != Some(name) {
                continue;
            }
            let declaration = match (found.kind, found.inline) {
                (DeclKind::PropertyAssignment, Some(inline)) => DeclRef { file, index: inline },
                _ => reference,
            };
            declarations.push(declaration);
        }
        LocatorMembers {
            supported: true,
            declarations,
        }
    }

    /// Whether `signature` is an overload signature of `implementation` by locator rules.
    pub(crate) fn is_locator_overload(&self, signature: DeclRef, implementation: DeclRef) -> bool {
        self.is_overload(signature, implementation)
    }
}
