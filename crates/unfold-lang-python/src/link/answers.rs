//! The binder's answers to the shared backend, phrased with program IDs: evidence, concrete
//! targets, declaration facts, provenance, class shapes, and module dependencies.
//!
//! A Python [`DeclRef`] is a declaration of the binder facts: `index` is its
//! [`NodeId`](crate::bind::ir::NodeId) in the declaring file.

use smallvec::SmallVec;
use unfold_engine::evidence::{
    Candidate, ClassMember, ClassShape, ConcreteTarget, Evidence, ExternalLocation, ImportBinding,
    ImportTarget, MemberFact, ModuleTarget, Origin, OriginDeclaration, Refusal, Stability, ValueContext,
};
use unfold_engine::{DeclRef, NodeRef as GraphNode, ValueRef};

use super::binder::{Binder, Ctx, Seen, SeenKey};
use super::modules::ModuleValue;
use super::program::{ExprRef, NodeRef};
use super::value::Value;
use crate::bind::ir::NodeKind;

pub(crate) fn node_of(declaration: DeclRef) -> NodeRef {
    NodeRef {
        file: declaration.file,
        node: declaration.index,
    }
}

pub(crate) fn decl_of(node: NodeRef) -> DeclRef {
    DeclRef {
        file: node.file,
        index: node.node,
    }
}

/// The context a concrete target is selected for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
    Call,
    Construct,
    /// A definition passed as a value: a class is itself.
    Reference,
}

impl From<ValueContext> for Context {
    fn from(context: ValueContext) -> Self {
        match context {
            ValueContext::Call => Self::Call,
            ValueContext::Construct => Self::Construct,
            // A type is never selected through `concrete` (`Binder::concrete_target` answers it
            // first); a class is its declaration whether or not it is concrete.
            ValueContext::Reference | ValueContext::Type => Self::Reference,
        }
    }
}

/// How an import's module resolved, as a module dependency.
pub(crate) fn module_target(binder: &Binder<'_>, module: &ModuleValue) -> ModuleTarget {
    if module.ambiguous {
        return ModuleTarget::Ambiguous;
    }
    if module.missing {
        return ModuleTarget::NotFound;
    }
    if let Some(external) = &module.external {
        return ModuleTarget::External(ExternalLocation::Package(external.clone()));
    }
    match module.paths.as_slice() {
        [path] => binder
            .modules
            .indexed(path)
            .map_or(ModuleTarget::NotIndexed, ModuleTarget::Indexed),
        _ => ModuleTarget::NotIndexed,
    }
}

/// The import binding a value's root proves: an external origin, or an import that names
/// nothing.
pub(crate) fn import_binding(value: &Value) -> Option<ImportBinding> {
    match (&value.external, value.missing) {
        (Some(external), _) => Some(ImportBinding {
            specifier: external.module.clone(),
            imported: external.member.clone(),
            target: ImportTarget::External,
            location: Some(ExternalLocation::Package(external.module.clone())),
        }),
        (None, true) => Some(ImportBinding {
            specifier: "".into(),
            imported: "".into(),
            target: ImportTarget::NotFound,
            location: None,
        }),
        (None, false) => None,
    }
}

impl Binder<'_> {
    /// The candidates a value supplies.
    pub(crate) fn candidates(&self, value: &Value) -> SmallVec<[Candidate; 2]> {
        if let Some(path) = &value.excluded {
            return SmallVec::from_elem(
                Candidate {
                    origin: Origin::Excluded,
                    declaration: None,
                    augmentation: false,
                    location: Some(ExternalLocation::File(path.clone())),
                },
                1,
            );
        }
        if let Some(external) = &value.external {
            let location = if &*external.module == "builtins" {
                ExternalLocation::Library
            } else {
                ExternalLocation::Package(external.module.clone())
            };
            return SmallVec::from_elem(Candidate::external(location), 1);
        }
        value
            .declarations
            .iter()
            .map(|&node| Candidate::indexed(decl_of(node)))
            .collect()
    }

    /// The evidence for an expression: a callable instance is called through its `__call__`.
    pub(crate) fn evidence(&self, expr: ExprRef) -> Evidence {
        let mut ctx = Ctx::new();
        let mut value = self.resolve(expr, &mut ctx);
        if value.instance && value.class.is_some() {
            value = self.member_value(value, self.program.known.call, &mut Seen::default(), &mut ctx);
        }
        let import_binding = import_binding(&value);
        let mut refusal = value.refusal;
        if !self.class_access_proven(&value, &mut ctx) {
            refusal = refusal.or(Some(Refusal::Unsupported));
        }
        if let Some(class) = value.class
            && !value.instance
            && !self.class_concrete(class, &mut ctx)
        {
            let wrapped = self.wrapper(class, &mut ctx);
            refusal = refusal.or(Some(if wrapped {
                Refusal::Wrapper
            } else {
                Refusal::Unsupported
            }));
        }
        if value.class_parameter {
            return Evidence {
                refusal: Some(Refusal::NotConcrete),
                ..Evidence::default()
            };
        }
        Evidence {
            candidates: self.candidates(&value),
            refusal,
            ambiguous: value.ambiguous,
            overridden: value.overridden,
            receiver_class: value.receiver_class.map(decl_of),
            super_access: value.super_access,
            static_access: value.static_access,
            unbound_this: false,
            import_binding,
            root_is_indexed: None,
        }
    }

    /// The evidence for a value of the file's value table.
    pub(crate) fn value_evidence(&self, value: ValueRef) -> Evidence {
        match self.value_expr(value) {
            Some(expr) => self.evidence(expr),
            None => Evidence::default(),
        }
    }

    pub(crate) fn value_expr(&self, value: ValueRef) -> Option<ExprRef> {
        let expr = *self.program.file(value.file)?.values.get(value.value as usize)?;
        Some(ExprRef {
            file: value.file,
            expr,
        })
    }

    /// Whether a callee is a proven class whose call constructs an instance.
    pub(crate) fn construction(&self, callee: ExprRef) -> bool {
        let mut ctx = Ctx::new();
        let value = self.resolve(callee, &mut ctx);
        let Some(class) = value.class else {
            return false;
        };
        !value.instance
            && !value.class_parameter
            && value.refusal.is_none()
            && self.class_access_proven(&value, &mut ctx)
            && !value.ambiguous
            && self.class_concrete(class, &mut ctx)
            && value.binding.is_none_or(|binding| self.stable(binding, &mut ctx))
    }

    /// The node of a lowered `def`, `lambda`, or `class` declaration.
    fn graph_node(&self, node: NodeRef) -> Option<GraphNode> {
        let local = match &self.program.node(node)?.kind {
            NodeKind::Function(function) => function.local?,
            NodeKind::Class(class) => class.local?,
            _ => return None,
        };
        Some(GraphNode {
            file: node.file,
            local,
        })
    }

    /// A declaration's concrete body for a context (see [`unfold_engine::Linked::concrete_target`]).
    /// In the type context, a class is its own type, decorated or not, as long as its name is
    /// bound once (a rebound class name is the policy's `mutable_binding`).
    pub(crate) fn concrete_target(
        &self,
        declaration: DeclRef,
        context: ValueContext,
    ) -> Option<ConcreteTarget> {
        if context == ValueContext::Type {
            let node = node_of(declaration);
            self.class_fact(node)?;
            let mut ctx = Ctx::new();
            if self
                .program
                .binding_of(node)
                .is_some_and(|binding| !self.stable(binding, &mut ctx))
            {
                return None;
            }
            return Some(ConcreteTarget {
                target: self.graph_node(node)?,
                implementation: declaration,
            });
        }
        let mut ctx = Ctx::new();
        let target = self.concrete(
            node_of(declaration),
            context.into(),
            &mut Seen::default(),
            &mut ctx,
        )?;
        Some(ConcreteTarget {
            target: self.graph_node(target)?,
            implementation: decl_of(target),
        })
    }

    /// The concrete body a call through `node` reaches.
    pub(crate) fn concrete_call(&self, node: NodeRef, ctx: &mut Ctx) -> Option<NodeRef> {
        self.concrete(node, Context::Call, &mut Seen::default(), ctx)
    }

    fn concrete(&self, node: NodeRef, context: Context, seen: &mut Seen, ctx: &mut Ctx) -> Option<NodeRef> {
        if seen.has(SeenKey::Node(node)) || seen.full() {
            return None;
        }
        seen.add(SeenKey::Node(node));
        let binding = self.program.binding_of(node);
        if binding.is_some_and(|binding| !self.stable(binding, ctx)) {
            return None;
        }
        if self.wrapper(node, ctx)
            || self.is_abstract(node, ctx)
            || self.overload(node, ctx)
            || self.accessor(node, ctx)
        {
            return None;
        }
        match &self.program.node(node)?.kind {
            NodeKind::Function(_) => return Some(node),
            NodeKind::Class(class) => {
                if !self.class_concrete(node, ctx) {
                    return None;
                }
                if context == Context::Construct
                    && let Some(constructor) =
                        self.program
                            .binding_in(node.file, class.body, self.program.known.init)
                {
                    let writes = self.program.binding(constructor).writes.clone();
                    let targets: SmallVec<[NodeRef; 2]> = writes
                        .iter()
                        .filter_map(|&write| {
                            self.concrete(
                                self.program.write(write).node,
                                Context::Call,
                                &mut seen.clone(),
                                ctx,
                            )
                        })
                        .collect();
                    return match targets.as_slice() {
                        [target] => Some(*target),
                        _ => None,
                    };
                }
                return Some(node);
            }
            _ => {}
        }
        let binding = binding?;
        let value = self.binding_value(binding, &mut seen.clone(), ctx);
        match value.declarations.as_slice() {
            [declaration] if !value.ambiguous && value.refusal.is_none() && *declaration != node => {
                self.concrete(*declaration, context, seen, ctx)
            }
            _ => None,
        }
    }

    /// Whether the declaration is a parameter.
    pub(crate) fn is_parameter(&self, declaration: DeclRef) -> bool {
        matches!(
            self.program.node(node_of(declaration)).map(|fact| &fact.kind),
            Some(NodeKind::Parameter { .. })
        )
    }

    pub(crate) fn stability(&self, declaration: DeclRef) -> Stability {
        match self.program.binding_of(node_of(declaration)) {
            Some(binding) if self.stable(binding, &mut Ctx::new()) => Stability::Stable,
            Some(_) => Stability::Rebindable,
            None => Stability::Unknown,
        }
    }

    pub(crate) fn is_accessor(&self, declaration: DeclRef) -> bool {
        self.accessor(node_of(declaration), &mut Ctx::new())
    }

    /// A declaration's written name: a function's, a class's, or a parameter's.
    fn node_name(&self, node: NodeRef) -> Option<&str> {
        let name = match &self.program.node(node)?.kind {
            NodeKind::Function(function) => function.name?,
            NodeKind::Class(class) => class.name,
            NodeKind::Parameter { name, .. } => *name,
            _ => return None,
        };
        Some(self.program.text(self.program.sym(node.file, name)))
    }

    pub(crate) fn member_fact(&self, declaration: DeclRef) -> MemberFact<'_> {
        let node = node_of(declaration);
        let mut ctx = Ctx::new();
        let owner = self.program.owner_class(node);
        let is_static = self.has_decorator(node, "builtins.staticmethod", &mut ctx);
        let is_final = self.has_decorator(node, "typing.final", &mut ctx)
            || owner.is_some_and(|owner| self.has_decorator(owner, "typing.final", &mut ctx));
        let method = owner.is_some() && self.is_def(node);
        MemberFact {
            owner: owner.map(decl_of),
            name: self.node_name(node),
            is_static,
            is_private: is_final,
            check_override: method && !is_static && !is_final,
            can_override: method && !is_static,
        }
    }

    /// Whether `signature` is a proven `typing.overload` of `implementation`: same name, same
    /// scope.
    pub(crate) fn overload_of(&self, signature: DeclRef, implementation: DeclRef) -> bool {
        let (left, right) = (node_of(signature), node_of(implementation));
        left.file == right.file
            && self.overload(left, &mut Ctx::new())
            && self.node_name(left) == self.node_name(right)
            && self.program.node(left).map(|fact| fact.scope)
                == self.program.node(right).map(|fact| fact.scope)
    }

    pub(crate) fn origin(&self, declaration: DeclRef) -> OriginDeclaration {
        let node = node_of(declaration);
        let (factory, annotation) = match self.program.node(node).map(|fact| &fact.kind) {
            Some(NodeKind::Assignment {
                factory, annotation, ..
            }) => (*factory, annotation.is_some()),
            Some(NodeKind::Parameter { annotation, .. }) => (None, annotation.is_some()),
            _ => (None, false),
        };
        let stable = self.stability(declaration) == Stability::Stable;
        OriginDeclaration {
            factory_callee: factory.filter(|_| stable).map(|value| ValueRef {
                file: node.file,
                value,
            }),
            annotation,
        }
    }

    pub(crate) fn annotation(&self, declaration: DeclRef) -> Option<Evidence> {
        let node = node_of(declaration);
        let annotation = match &self.program.node(node)?.kind {
            NodeKind::Assignment { annotation, .. } | NodeKind::Parameter { annotation, .. } => {
                (*annotation)?
            }
            _ => return None,
        };
        Some(self.evidence(ExprRef {
            file: node.file,
            expr: annotation,
        }))
    }

    /// Every class with its bases and its own methods, for the override census.
    pub(crate) fn class_shapes(&self) -> Vec<ClassShape> {
        let mut ctx = Ctx::new();
        self.program
            .classes
            .iter()
            .map(|&class| {
                let bases = self
                    .base_classes(class, &mut ctx)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|base| !base.is_builtin())
                    .map(decl_of)
                    .collect();
                let body = self.class_fact(class).map_or(u32::MAX, |fact| fact.body);
                let members = self
                    .program
                    .bindings_in(class.file, body)
                    .iter()
                    .flat_map(|&binding| {
                        self.program
                            .binding(binding)
                            .writes
                            .iter()
                            .map(|&write| self.program.write(write).node)
                    })
                    .filter(|&node| self.is_def(node))
                    .map(|node| ClassMember {
                        declaration: decl_of(node),
                        is_static: self.has_decorator(node, "builtins.staticmethod", &mut ctx),
                    })
                    .collect();
                ClassShape {
                    class: decl_of(class),
                    bases,
                    members,
                }
            })
            .collect()
    }
}
