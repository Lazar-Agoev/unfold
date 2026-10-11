//! The binder's answers to the shared backend, phrased with program IDs: evidence, concrete
//! targets, declaration facts, provenance, locators, class shapes, and module dependencies.
//!
//! A Python [`DeclRef`] is a declaration of the binder facts: `index` is its
//! [`NodeId`](crate::bind::ir::NodeId) in the declaring file.

use smallvec::SmallVec;
use unfold_core::vocab::BlockKind;
use unfold_engine::evidence::{
    Candidate, ClassMember, ClassShape, ConcreteTarget, Evidence, Export, ExternalLocation, ImportBinding,
    ImportTarget, LocatorMembers, MemberFact, ModuleTarget, Origin, OriginDeclaration, ParameterFact,
    Refusal, Stability, ValueContext,
};
use unfold_engine::{BlockRef, DeclRef, FileId, ValueRef};

use super::binder::{Binder, Ctx, Seen, SeenKey};
use super::modules::ModuleValue;
use super::program::{ExprRef, NodeRef};
use super::value::Value;
use crate::bind::ir::{MODULE_SCOPE, NodeKind};

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
    Handler,
}

impl From<ValueContext> for Context {
    fn from(context: ValueContext) -> Self {
        match context {
            ValueContext::Call => Self::Call,
            ValueContext::Construct => Self::Construct,
            // A type is never selected through `concrete` (`Binder::concrete_target` answers it
            // first); a class is its declaration whether or not it is concrete.
            ValueContext::Reference | ValueContext::Handler | ValueContext::Type => Self::Handler,
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

/// The import binding a value's root proves: an external origin, or a relative import that
/// names nothing.
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

    fn block_of(&self, node: NodeRef) -> Option<BlockRef> {
        let local = match &self.program.node(node)?.kind {
            NodeKind::Function(function) => function.block,
            NodeKind::Class(class) => class.block,
            _ => return None,
        };
        Some(BlockRef {
            file: node.file,
            local,
        })
    }

    /// The block that binds a module-level name: a `def` or `class` block, or the `variable`
    /// block of an assignment written directly in the module (`app = FastAPI()`). This is what
    /// a `module:attribute` launch target and a FastAPI route's application binding resolve to.
    pub(crate) fn binding_block(&self, node: NodeRef) -> Option<BlockRef> {
        let fact = self.program.node(node)?;
        match fact.kind {
            NodeKind::Function(_) | NodeKind::Class(_) => self.block_of(node),
            NodeKind::Assignment { .. } if fact.scope == MODULE_SCOPE => {
                let binding = self.program.binding(self.program.binding_of(node)?);
                let name = self.program.text(binding.name);
                let blocks = self.program.facts.facts(node.file)?.graph.blocks();
                let mut statements = blocks.iter().filter(|block| {
                    block.parent == Some(0)
                        && block.kind == BlockKind::Variable
                        && block.name.as_deref() == Some(name)
                });
                match (statements.next(), statements.next()) {
                    (Some(block), None) => Some(BlockRef {
                        file: node.file,
                        local: block.local_id,
                    }),
                    _ => None,
                }
            }
            _ => None,
        }
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
                target: self.block_of(node)?,
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
            target: self.block_of(target)?,
            implementation: decl_of(target),
        })
    }

    /// The block a locator names: a concrete body in the handler context.
    pub(crate) fn locator_block(&self, declaration: DeclRef) -> Option<BlockRef> {
        let mut ctx = Ctx::new();
        let target = self.concrete(
            node_of(declaration),
            Context::Handler,
            &mut Seen::default(),
            &mut ctx,
        )?;
        self.block_of(target)
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

    pub(crate) fn parameter(&self, declaration: DeclRef) -> Option<ParameterFact> {
        let node = node_of(declaration);
        let &NodeKind::Parameter {
            function, position, ..
        } = &self.program.node(node)?.kind
        else {
            return None;
        };
        let owner = self.block_of(NodeRef {
            file: node.file,
            node: function,
        });
        Some(ParameterFact {
            parameter: declaration,
            owner,
            position: Some(position),
        })
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

    /// The declarations a top-level name of `file` has (NFKC-normalized).
    pub(crate) fn top_level(&self, file: FileId, name: &str) -> Vec<DeclRef> {
        self.writes_named(file, MODULE_SCOPE, name)
    }

    fn writes_named(&self, file: FileId, scope: u32, name: &str) -> Vec<DeclRef> {
        let name: String = unicode_normalization::UnicodeNormalization::nfkc(name).collect();
        let Some(sym) = self.program.names.get(&name) else {
            return Vec::new();
        };
        self.program
            .binding_in(file, scope, sym)
            .map(|binding| {
                self.program
                    .binding(binding)
                    .writes
                    .iter()
                    .map(|&write| decl_of(self.program.write(write).node))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn members_named(&self, owner: DeclRef, name: &str) -> LocatorMembers {
        let node = node_of(owner);
        match self.class_fact(node) {
            Some(class) => LocatorMembers {
                supported: true,
                declarations: self.writes_named(node.file, class.body, name),
            },
            None => LocatorMembers::default(),
        }
    }

    /// The runtime exports of a file: its module-level bindings, sorted by name.
    pub(crate) fn exports(&self, file: FileId) -> Vec<Export> {
        let mut exports: Vec<Export> = self
            .program
            .bindings_in(file, MODULE_SCOPE)
            .iter()
            .map(|&index| {
                let binding = self.program.binding(index);
                let value = self.binding_value(index, &mut Seen::default(), &mut Ctx::new());
                let candidates = match (&value.excluded, &value.external) {
                    (None, Some(external)) => vec![Candidate::external(ExternalLocation::Package(
                        external.module.clone(),
                    ))],
                    _ => self.candidates(&value).into_vec(),
                };
                Export {
                    name: self.program.text(binding.name).into(),
                    candidates,
                }
            })
            .collect();
        exports.sort_by(|a, b| a.name.cmp(&b.name));
        exports
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
