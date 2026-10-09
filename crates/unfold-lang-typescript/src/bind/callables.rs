//! Callables, parameters, destructuring patterns, and variable declarations: the declarations
//! they create and the scopes their parameters and bodies bind in.

use oxc_allocator::UnstableAddress;
use oxc_ast::ast::{
    BindingIdentifier, BindingPattern, Expression, FormalParameters, Function, VariableDeclarator,
};
use oxc_ast_visit::Visit;
use oxc_span::Span;

use super::forms::{asserted, awaited_import, key_name, uses_outer_scope};
use super::ir::{factory_callee, unwrap};
use super::scope::ScopeKind;
use super::{AliasDraft, Binder, Frame, ModuleKey};
use crate::build::range;
use crate::payload::binding::{AliasRef, DeclFlags, DeclKind, Meanings, ParamInfo, VarKind};

/// Who binds the identifiers of a destructuring pattern.
#[derive(Debug, Clone, Copy)]
pub(super) enum PatternOwner {
    /// A variable declaration (or catch clause, `catch`), binding into `scope`.
    Variable {
        var_kind: Option<VarKind>,
        scope: u32,
        exported: bool,
        catch: bool,
    },
    /// A parameter: its declaration, the callable, and its position.
    Parameter(ParamInfo),
}

impl<'a> Binder<'a> {
    // --- Callables ------------------------------------------------------------------------

    /// The scope, parameters, and body of a function-like declaration `decl`. `frame` is the
    /// `this` frame of its body (`None` for an arrow), `own_name` a function expression's
    /// self binding.
    pub(super) fn callable(
        &mut self,
        decl: u32,
        function: &Function<'a>,
        frame: Frame,
        own_name: Option<&BindingIdentifier<'a>>,
    ) {
        let outer_ambient = self.ambient;
        self.ambient |= function.declare;
        // Parameter decorators are evaluated outside the callable.
        self.parameter_decorators(&function.params);
        let inner = self.scopes.push(ScopeKind::Function, Some(self.scope));
        self.scopes.scope_mut(inner).body = function.body.as_ref().map(|body| range(body.span));
        let outer_scope = std::mem::replace(&mut self.scope, inner);
        let outer_parameters = self.parameters;
        let outer_container = self.container;
        self.frames.push(frame);
        if let Some(id) = own_name {
            self.declare(inner, id.name.as_str(), decl, Meanings::VALUE, false);
        }
        self.type_parameters(function.type_parameters.as_deref());
        let separate = uses_outer_scope(&function.params, self.context).then_some(inner);
        self.parameters = self.parameter_set(outer_parameters, separate);
        self.formal_parameters(decl, &function.params);
        let returns = self.annotation(function.return_type.as_deref());
        if self.decl_mut(decl).returns.is_none() {
            self.decl_mut(decl).returns = returns;
        }
        self.parameters = outer_parameters;
        if let Some(body) = &function.body {
            self.container = self.new_container();
            self.statements(&body.statements);
        }
        self.frames.pop();
        self.container = outer_container;
        self.scope = outer_scope;
        self.ambient = outer_ambient;
    }

    pub(super) fn parameter_decorators(&mut self, parameters: &FormalParameters<'a>) {
        for parameter in &parameters.items {
            for decorator in &parameter.decorators {
                self.visit_decorator(decorator);
            }
        }
        if let Some(rest) = &parameters.rest {
            for decorator in &rest.decorators {
                self.visit_decorator(decorator);
            }
        }
    }

    /// Declares a callable's parameters in the current scope. Positions count the written
    /// parameters, so an explicit `this` parameter (which oxc keeps apart) is skipped (C-4).
    pub(super) fn formal_parameters(&mut self, owner: u32, parameters: &FormalParameters<'a>) {
        let constructor = self
            .decl(owner)
            .is_some_and(|decl| decl.kind == DeclKind::Constructor);
        for (position, parameter) in parameters.items.iter().enumerate() {
            let property = parameter.accessibility.is_some() || parameter.readonly || parameter.r#override;
            let (pattern, default) = match (&parameter.pattern, &parameter.initializer) {
                (BindingPattern::AssignmentPattern(it), None) => (&it.left, Some(&it.right)),
                (pattern, initializer) => (pattern, initializer.as_deref()),
            };
            let index = self.parameter(owner, position as u32, pattern, parameter.span);
            let ty = self.annotation(parameter.type_annotation.as_deref());
            self.decl_mut(index).ty = ty;
            if let Some(default) = default {
                self.visit_expression(default);
                let init = self.converted(default);
                self.decl_mut(index).initializer = Some(init);
            }
            if property
                && constructor
                && matches!(pattern, BindingPattern::BindingIdentifier(_))
                && let Some(&class) = self.class_stack.last()
            {
                self.classes[class as usize].members.push(index);
            }
        }
        if let Some(rest) = &parameters.rest {
            let position = parameters.items.len() as u32;
            let index = self.parameter(owner, position, &rest.rest.argument, rest.span);
            let ty = self.annotation(rest.type_annotation.as_deref());
            self.decl_mut(index).ty = ty;
        }
    }

    /// One parameter's declaration and the identifiers its pattern binds.
    pub(super) fn parameter(
        &mut self,
        owner: u32,
        position: u32,
        pattern: &BindingPattern<'a>,
        span: Span,
    ) -> u32 {
        let name = match pattern {
            BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
            _ => None,
        };
        let decl = self.blank(DeclKind::Parameter, name, range(span));
        let index = self.add(decl, None);
        let info = ParamInfo {
            parameter: index,
            owner,
            position,
        };
        self.decl_mut(index).parameter = Some(info);
        match pattern {
            BindingPattern::BindingIdentifier(id) => {
                self.declare(self.scope, id.name.as_str(), index, Meanings::VALUE, false);
            }
            other => self.pattern(other, PatternOwner::Parameter(info)),
        }
        index
    }

    /// Binds every identifier of a destructuring pattern as a binding element, visiting
    /// defaults and computed keys as expressions.
    pub(super) fn pattern(&mut self, pattern: &BindingPattern<'a>, owner: PatternOwner) {
        match pattern {
            BindingPattern::BindingIdentifier(id) => {
                self.binding_element(id, owner);
            }
            BindingPattern::ObjectPattern(it) => {
                for property in &it.properties {
                    if property.computed
                        && let Some(key) = property.key.as_expression()
                    {
                        self.visit_expression(key);
                    }
                    self.pattern(&property.value, owner);
                }
                if let Some(rest) = &it.rest {
                    self.pattern(&rest.argument, owner);
                }
            }
            BindingPattern::ArrayPattern(it) => {
                for element in it.elements.iter().flatten() {
                    self.pattern(element, owner);
                }
                if let Some(rest) = &it.rest {
                    self.pattern(&rest.argument, owner);
                }
            }
            BindingPattern::AssignmentPattern(it) => {
                self.pattern(&it.left, owner);
                self.visit_expression(&it.right);
            }
        }
    }

    pub(super) fn binding_element(&mut self, id: &BindingIdentifier<'a>, owner: PatternOwner) -> u32 {
        let name = id.name.as_str();
        let mut decl = self.blank(DeclKind::BindingElement, Some(name), range(id.span));
        let (scope, exported) = match owner {
            PatternOwner::Variable {
                var_kind,
                scope,
                exported,
                catch,
            } => {
                decl.var_kind = var_kind;
                decl.flags = decl.flags.with(DeclFlags::CATCH, catch);
                (scope, exported)
            }
            PatternOwner::Parameter(info) => {
                decl.flags = decl.flags.with(DeclFlags::IN_PARAMETER, true);
                decl.parameter = Some(info);
                (self.scope, false)
            }
        };
        let index = self.add(decl, Some(ModuleKey::Variable));
        self.declare(scope, name, index, Meanings::VALUE, exported)
    }

    /// `const { a, b: c } = await import("./m.js")`: each plainly destructured property reads
    /// the module's export of that name, so its binding is an alias of the export (C-8).
    /// Defaults, nested patterns, computed keys, and rest elements bind as usual.
    pub(super) fn dynamic_import_pattern(
        &mut self,
        pattern: &oxc_ast::ast::ObjectPattern<'a>,
        specifier: &str,
        owner: PatternOwner,
    ) {
        for property in &pattern.properties {
            let imported = if property.computed {
                None
            } else {
                key_name(&property.key, false)
            };
            match (&property.value, imported) {
                (BindingPattern::BindingIdentifier(id), Some(imported)) => {
                    let symbol = self.binding_element(id, owner);
                    self.aliases.push(AliasDraft::Dynamic {
                        specifier: specifier.into(),
                        imported: imported.into(),
                    });
                    self.alias_refs
                        .insert(symbol, AliasRef::Local((self.aliases.len() - 1) as u32));
                }
                _ => {
                    if property.computed
                        && let Some(key) = property.key.as_expression()
                    {
                        self.visit_expression(key);
                    }
                    self.pattern(&property.value, owner);
                }
            }
        }
        if let Some(rest) = &pattern.rest {
            self.pattern(&rest.argument, owner);
        }
    }

    // --- Variables ------------------------------------------------------------------------

    pub(super) fn declarator(
        &mut self,
        declarator: &VariableDeclarator<'a>,
        var_kind: VarKind,
        top: bool,
        exported: bool,
    ) {
        let scope = if var_kind == VarKind::Var {
            self.scopes.var_scope(self.scope)
        } else {
            self.scope
        };
        let BindingPattern::BindingIdentifier(id) = &declarator.id else {
            let owner = PatternOwner::Variable {
                var_kind: Some(var_kind),
                scope,
                exported,
                catch: false,
            };
            let dynamic = declarator
                .init
                .as_ref()
                .filter(|_| var_kind == VarKind::Const && !self.ambient)
                .and_then(awaited_import);
            match (&declarator.id, dynamic) {
                (BindingPattern::ObjectPattern(pattern), Some(specifier)) => {
                    self.dynamic_import_pattern(pattern, specifier, owner);
                }
                _ => self.pattern(&declarator.id, owner),
            }
            if let Some(init) = &declarator.init {
                self.visit_expression(init);
            }
            return;
        };
        let name = id.name.as_str();
        let mut decl = self.blank(DeclKind::Variable, Some(name), range(declarator.span));
        decl.var_kind = Some(var_kind);
        decl.flags = decl.flags.with(DeclFlags::TOP_LEVEL, top);
        let index = self.add(decl, Some(ModuleKey::Variable));
        self.declare(scope, name, index, Meanings::VALUE, exported);
        let ty = self.annotation(declarator.type_annotation.as_deref());
        self.decl_mut(index).ty = ty;
        let Some(init) = &declarator.init else {
            return;
        };
        if var_kind == VarKind::Const && !self.ambient {
            if let Expression::ObjectExpression(object) = unwrap(init) {
                let plain = declarator.type_annotation.is_none() && !asserted(init);
                self.pending_objects
                    .insert(object.unstable_address(), (index, plain));
            }
            if let Some(callee) = factory_callee(init) {
                self.factories.push((index, callee));
            }
        }
        self.visit_expression(init);
        let converted = self.converted(init);
        self.decl_mut(index).initializer = Some(converted);
    }
}
