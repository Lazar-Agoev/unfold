//! The binder pass over definitions: `def` (decorators, type parameters, parameters, body) and
//! the type-parameter scopes of definitions and `type` aliases.

use ruff_python_ast::{Parameters, Stmt, StmtFunctionDef, TypeParam, TypeParams};

use super::build::{Builder, Ctx};
use super::ir::{FunctionFact, NodeId, NodeKind, ScopeId, ScopeKind, WriteFact, WriteKind};

impl<'a> Builder<'a> {
    pub(super) fn function(&mut self, stmt: &'a Stmt, function: &'a StmtFunctionDef, ctx: Ctx) {
        let scope = ctx.scope;
        let type_scope = self.type_scope(function.type_params.as_deref(), ctx);
        let body = self.scope(ScopeKind::Function, Some(type_scope));
        let decorators = function
            .decorator_list
            .iter()
            .map(|decorator| self.reserve(&decorator.expression, scope))
            .collect();
        let kind = NodeKind::Function(FunctionFact {
            block: self.block(stmt),
            name: Some(self.interner.name(function.name.id.as_str())),
            decorators,
            parameters: Vec::new(),
            first_parameter: self.first_parameter(&function.parameters),
            body,
        });
        let node = self.node(kind, ctx);
        self.simple_write(ctx, function.name.id.as_str(), node, WriteKind::Definition);
        let header = Ctx {
            enclosing: Some(node),
            function: Some(node),
            conditional: false,
            ..ctx
        };
        self.parameters(&function.parameters, node, body, header.in_scope(type_scope));
        if let Some(returns) = function.returns.as_deref() {
            self.annotation(returns, type_scope);
        }
        let inside = Ctx {
            scope: body,
            ..header
        };
        self.statements(&function.body, inside);
        self.visit(
            function
                .returns
                .as_deref()
                .map(|returns| (returns, header.in_scope(type_scope))),
        );
        self.parameter_values(&function.parameters, header.in_scope(type_scope));
        self.type_parameter_values(function.type_params.as_deref(), header.in_scope(type_scope));
        self.visit(
            function
                .decorator_list
                .iter()
                .map(|decorator| (&decorator.expression, header)),
        );
    }

    /// The name of the first written parameter, unless it is a bare `*`.
    pub(super) fn first_parameter(&mut self, parameters: &'a Parameters) -> Option<u32> {
        let first = parameters
            .posonlyargs
            .first()
            .or_else(|| parameters.args.first())
            .map(|parameter| &parameter.parameter.name)
            .or_else(|| parameters.vararg.as_deref().map(|parameter| &parameter.name))
            .or_else(|| {
                parameters
                    .kwonlyargs
                    .is_empty()
                    .then(|| parameters.kwarg.as_deref().map(|parameter| &parameter.name))
                    .flatten()
            })?;
        Some(self.interner.name(first.id.as_str()))
    }

    /// Declares and binds the parameters of `function` in `body`; their annotations are
    /// evaluated in `outer`'s scope.
    pub(super) fn parameters(
        &mut self,
        parameters: &'a Parameters,
        function: NodeId,
        body: ScopeId,
        outer: Ctx,
    ) {
        let mut declared = Vec::new();
        for (position, parameter) in parameters.iter().enumerate() {
            let annotation = parameter.annotation().map(|annotation| {
                self.annotation(annotation, outer.scope);
                self.reserve(annotation, outer.scope)
            });
            let name = parameter.name().id.as_str();
            let kind = NodeKind::Parameter {
                name: self.interner.name(name),
                function,
                position: position as u32,
                annotation,
            };
            let inside = Ctx { scope: body, ..outer };
            let node = self.node(kind, inside);
            declared.push(node);
            self.write(
                inside,
                name,
                false,
                WriteFact {
                    node,
                    kind: WriteKind::Parameter,
                    value: None,
                    annotation,
                    import: None,
                    conditional: false,
                },
            );
        }
        if let NodeKind::Function(fact) = &mut self.nodes[function as usize].kind {
            fact.parameters = declared;
        }
    }

    /// Queues parameter annotations and defaults, in written order.
    pub(super) fn parameter_values(&mut self, parameters: &'a Parameters, ctx: Ctx) {
        let mut children = Vec::new();
        for parameter in parameters {
            children.extend(parameter.annotation().map(|annotation| (annotation, ctx)));
            children.extend(parameter.default().map(|default| (default, ctx)));
        }
        self.visit(children);
    }

    /// Opens the type-parameter scope of a definition that has type parameters and binds them;
    /// returns the scope annotations are evaluated in.
    pub(super) fn type_scope(&mut self, parameters: Option<&'a TypeParams>, ctx: Ctx) -> ScopeId {
        let Some(parameters) = parameters else {
            return ctx.scope;
        };
        let scope = self.scope(ScopeKind::Type, Some(ctx.scope));
        let inside = Ctx {
            scope,
            conditional: false,
            ..ctx
        };
        for parameter in &parameters.type_params {
            let name = match parameter {
                TypeParam::TypeVar(node) => &node.name,
                TypeParam::ParamSpec(node) => &node.name,
                TypeParam::TypeVarTuple(node) => &node.name,
            };
            let node = self.node(NodeKind::Other, inside);
            self.simple_write(inside, name.id.as_str(), node, WriteKind::Other);
        }
        scope
    }

    pub(super) fn type_parameter_values(&mut self, parameters: Option<&'a TypeParams>, ctx: Ctx) {
        let Some(parameters) = parameters else {
            return;
        };
        let mut children = Vec::new();
        for parameter in &parameters.type_params {
            let (bound, default) = match parameter {
                TypeParam::TypeVar(node) => (node.bound.as_deref(), node.default.as_deref()),
                TypeParam::ParamSpec(node) => (None, node.default.as_deref()),
                TypeParam::TypeVarTuple(node) => (None, node.default.as_deref()),
            };
            children.extend(bound.map(|bound| (bound, ctx)));
            children.extend(default.map(|default| (default, ctx)));
        }
        self.visit(children);
    }
}
