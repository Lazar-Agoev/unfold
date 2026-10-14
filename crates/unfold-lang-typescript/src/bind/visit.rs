//! The binding walk over oxc's tree: scopes, declarations, `this` frames, narrowing sites, and
//! member writes, with every expression converted on the way back up.
//!
//! Scopes follow the proof of concept's binder: `var` binds in the nearest function, arrow,
//! static block, namespace, or file scope; `let`, `const`, classes, and function declarations
//! stay in their block. A callable's parameters and body share one scope; its parameter list
//! is evaluated with the callable's body variables hidden when the target keeps parameter
//! evaluation separate ([`uses_outer_scope`]). Decorators, computed member names, and class
//! heritage are evaluated in the surrounding scope. Type syntax is never walked: the types
//! evidence reads are converted where they are written.

use oxc_allocator::UnstableAddress;
use oxc_ast::ast::{
    Argument, ArrowFunctionBody, BinaryExpression, BindingPattern, BlockStatement, CallExpression,
    CatchClause, Class, ClassType, ConditionalExpression, DoWhileStatement, ExportDefaultDeclaration,
    ExportDefaultDeclarationKind, ExportNamedDeclaration, Expression, ForInStatement, ForOfStatement,
    ForStatement, Function, FunctionType, IfStatement, ImportDeclaration, ImportDeclarationSpecifier,
    ImportOrExportKind, LogicalExpression, ObjectExpression, PrivateFieldExpression, Program,
    SimpleAssignmentTarget, Statement, SwitchStatement, TSEnumDeclaration, TSEnumMemberName,
    TSExportAssignment, TSExternalModuleDeclaration, TSGlobalDeclaration, TSImportEqualsDeclaration,
    TSInterfaceDeclaration, TSModuleReference, TSNamespaceDeclaration, TSType, TSTypeAliasDeclaration,
    TSTypeAnnotation, TSTypeName, TSTypeParameterDeclaration, TSTypeParameterInstantiation, ThisExpression,
    UnaryExpression, VariableDeclaration, VariableDeclarationKind, WhileStatement, WithStatement,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::GetSpan;
use oxc_syntax::operator::{BinaryOperator, LogicalOperator, UnaryOperator};
use oxc_syntax::scope::ScopeFlags;

use super::callables::PatternOwner;
use super::forms::{literal_name, uses_outer_scope};
use super::ir::unwrap;
use super::scope::ScopeKind;
use super::{AliasDraft, Binder, ClassDraft, Frame, ModuleKey};
use crate::payload::binding::{AliasRef, DeclFlags, DeclKind, Meanings, VarKind};
use crate::spans::range;

impl<'a> Binder<'a> {
    /// Walks the whole file.
    pub(super) fn walk(&mut self, program: &Program<'a>) {
        self.container = self.new_container();
        for statement in &program.body {
            self.visit_statement(statement);
        }
    }

    /// Runs `inside` in a new scope of `kind` with its own container.
    pub(super) fn scoped(&mut self, kind: ScopeKind, inside: impl FnOnce(&mut Self)) {
        let scope = self.scopes.push(kind, Some(self.scope));
        let outer = std::mem::replace(&mut self.scope, scope);
        let container = self.container;
        self.container = self.new_container();
        inside(self);
        self.scope = outer;
        self.container = container;
    }

    /// Runs `inside` as part of a condition.
    pub(super) fn in_condition(&mut self, inside: impl FnOnce(&mut Self)) {
        self.condition += 1;
        inside(self);
        self.condition -= 1;
    }

    /// Runs `inside` with `frame` as the innermost `this` frame.
    pub(super) fn framed(&mut self, frame: Frame, inside: impl FnOnce(&mut Self)) {
        self.frames.push(frame);
        inside(self);
        self.frames.pop();
    }

    /// Runs `inside` as ambient when `declare` is written.
    pub(super) fn ambient_if(&mut self, declare: bool, inside: impl FnOnce(&mut Self)) {
        let outer = self.ambient;
        self.ambient = outer || declare;
        inside(self);
        self.ambient = outer;
    }

    pub(super) fn statements(&mut self, statements: &[Statement<'a>]) {
        for statement in statements {
            self.visit_statement(statement);
        }
    }

    /// Declares a callable's type parameters in the current scope.
    pub(super) fn type_parameters(&mut self, parameters: Option<&TSTypeParameterDeclaration<'a>>) {
        for parameter in parameters.into_iter().flat_map(|parameters| &parameters.params) {
            let name = parameter.name.name.as_str();
            let decl = self.blank(DeclKind::TypeParameter, Some(name), range(parameter.span));
            let index = self.add(decl, None);
            self.declare(self.scope, name, index, Meanings::TYPE, false);
        }
    }

    /// A written type annotation, converted with names looked up from the current scope (and
    /// its type names recorded when it is linked).
    pub(super) fn annotation(&mut self, annotation: Option<&TSTypeAnnotation<'a>>) -> Option<u32> {
        self.link_annotation(annotation);
        annotation.map(|annotation| self.ty(&annotation.type_annotation, self.scope))
    }

    /// A callable's written return type and what `await` of a call to it gives, as
    /// `Decl::returns` and `Decl::awaits` hold them: the written type converted like
    /// [`Binder::annotation`], with `null` and `undefined` dropped from a union of one other type
    /// and them; and, for an `async` callable whose written return type is `Promise<T>`, `T`,
    /// read the same way.
    pub(super) fn returns(
        &mut self,
        annotation: Option<&TSTypeAnnotation<'a>>,
        asynchronous: bool,
    ) -> (Option<u32>, Option<u32>) {
        self.link_annotation(annotation);
        let Some(annotation) = annotation else {
            return (None, None);
        };
        let written = &annotation.type_annotation;
        let returns = self.ty(non_nullable(written), self.scope);
        let awaits = promised(written)
            .filter(|_| asynchronous)
            .map(|awaited| self.ty(non_nullable(awaited), self.scope));
        (Some(returns), awaits)
    }
}

/// A union of one type with `null` or `undefined` (`Repo | null`, `undefined | Repo`) as that
/// type; any other type as itself.
fn non_nullable<'t, 'a>(written: &'t TSType<'a>) -> &'t TSType<'a> {
    let mut ty = written;
    while let TSType::TSParenthesizedType(inner) = ty {
        ty = &inner.type_annotation;
    }
    let TSType::TSUnionType(union) = ty else {
        return written;
    };
    let mut others = union
        .types
        .iter()
        .filter(|member| !matches!(member, TSType::TSNullKeyword(_) | TSType::TSUndefinedKeyword(_)));
    match (others.next(), others.next()) {
        (Some(only), None) => only,
        _ => written,
    }
}

/// The type argument of a written `Promise<T>`.
fn promised<'t, 'a>(written: &'t TSType<'a>) -> Option<&'t TSType<'a>> {
    let mut ty = written;
    while let TSType::TSParenthesizedType(inner) = ty {
        ty = &inner.type_annotation;
    }
    let TSType::TSTypeReference(reference) = ty else {
        return None;
    };
    let TSTypeName::IdentifierReference(name) = &reference.type_name else {
        return None;
    };
    match reference.type_arguments.as_deref()?.params.as_slice() {
        [argument] if name.name == "Promise" => Some(argument),
        _ => None,
    }
}

impl<'a> Visit<'a> for Binder<'a> {
    fn visit_expression(&mut self, it: &Expression<'a>) {
        walk::walk_expression(self, it);
        self.convert(it);
    }

    fn visit_this_expression(&mut self, it: &ThisExpression) {
        self.context_here(it.span.start, false);
    }

    fn visit_super(&mut self, it: &oxc_ast::ast::Super) {
        self.context_here(it.span.start, true);
    }

    fn visit_private_field_expression(&mut self, it: &PrivateFieldExpression<'a>) {
        self.visit_expression(&it.object);
    }

    // --- Types are not walked ---

    fn visit_ts_type_annotation(&mut self, _it: &TSTypeAnnotation<'a>) {}

    fn visit_ts_type(&mut self, _it: &TSType<'a>) {}

    fn visit_ts_type_parameter_declaration(&mut self, _it: &TSTypeParameterDeclaration<'a>) {}

    fn visit_ts_type_parameter_instantiation(&mut self, _it: &TSTypeParameterInstantiation<'a>) {}

    fn visit_ts_interface_declaration(&mut self, it: &TSInterfaceDeclaration<'a>) {
        let exported = std::mem::take(&mut self.exporting);
        let name = it.id.name.as_str();
        let mut decl = self.blank(DeclKind::Interface, Some(name), range(it.span));
        decl.node = self.node(it.unstable_address());
        let index = self.add(decl, Some(ModuleKey::Interface));
        self.declare(self.scope, name, index, Meanings::TYPE, exported);
        self.link_declaration(range(it.body.span), it.type_parameters.as_deref(), |binder| {
            binder.interface_heritage(&it.extends);
            for member in &it.body.body {
                binder.signature_names(member, 0);
            }
        });
    }

    fn visit_ts_type_alias_declaration(&mut self, it: &TSTypeAliasDeclaration<'a>) {
        let exported = std::mem::take(&mut self.exporting);
        let name = it.id.name.as_str();
        let mut decl = self.blank(DeclKind::TypeAlias, Some(name), range(it.span));
        decl.node = self.node(it.unstable_address());
        let index = self.add(decl, Some(ModuleKey::TypeAlias));
        self.declare(self.scope, name, index, Meanings::TYPE, exported);
        let aliased = range(it.type_annotation.span());
        self.link_declaration(aliased, it.type_parameters.as_deref(), |binder| {
            binder.type_names(&it.type_annotation, 0);
        });
        if it.type_parameters.is_none() {
            let ty = self.ty(&it.type_annotation, self.scope);
            self.decl_mut(index).ty = Some(ty);
        }
    }

    // --- Statements ---

    fn visit_block_statement(&mut self, it: &BlockStatement<'a>) {
        self.scoped(ScopeKind::Block, |binder| binder.statements(&it.body));
    }

    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        self.scoped(ScopeKind::Loop, |binder| {
            if let Some(init) = &it.init {
                binder.visit_for_statement_init(init);
            }
            if let Some(test) = &it.test {
                binder.in_condition(|binder| binder.visit_expression(test));
            }
            if let Some(update) = &it.update {
                binder.visit_expression(update);
            }
            binder.visit_statement(&it.body);
        });
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        self.scoped(ScopeKind::Loop, |binder| walk::walk_for_in_statement(binder, it));
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        self.scoped(ScopeKind::Loop, |binder| walk::walk_for_of_statement(binder, it));
    }

    fn visit_while_statement(&mut self, it: &WhileStatement<'a>) {
        self.in_condition(|binder| binder.visit_expression(&it.test));
        self.visit_statement(&it.body);
    }

    fn visit_do_while_statement(&mut self, it: &DoWhileStatement<'a>) {
        self.visit_statement(&it.body);
        self.in_condition(|binder| binder.visit_expression(&it.test));
    }

    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        self.in_condition(|binder| binder.visit_expression(&it.test));
        self.visit_statement(&it.consequent);
        if let Some(alternate) = &it.alternate {
            self.visit_statement(alternate);
        }
    }

    fn visit_conditional_expression(&mut self, it: &ConditionalExpression<'a>) {
        self.in_condition(|binder| binder.visit_expression(&it.test));
        self.visit_expression(&it.consequent);
        self.visit_expression(&it.alternate);
    }

    fn visit_logical_expression(&mut self, it: &LogicalExpression<'a>) {
        if matches!(it.operator, LogicalOperator::And | LogicalOperator::Or) {
            self.in_condition(|binder| binder.visit_expression(&it.left));
        } else {
            self.visit_expression(&it.left);
        }
        self.visit_expression(&it.right);
    }

    fn visit_switch_statement(&mut self, it: &SwitchStatement<'a>) {
        self.visit_expression(&it.discriminant);
        self.scoped(ScopeKind::Switch, |binder| {
            for case in &it.cases {
                if let Some(test) = &case.test {
                    binder.visit_expression(test);
                }
                let container = std::mem::replace(&mut binder.container, binder.next_container);
                binder.next_container += 1;
                binder.statements(&case.consequent);
                binder.container = container;
            }
        });
    }

    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        self.scoped(ScopeKind::Catch, |binder| {
            if let Some(parameter) = &it.param {
                binder.link_annotation(parameter.type_annotation.as_deref());
                match &parameter.pattern {
                    BindingPattern::BindingIdentifier(id) => {
                        let name = id.name.as_str();
                        let mut decl = binder.blank(DeclKind::Variable, Some(name), range(parameter.span));
                        decl.flags = decl.flags.with(DeclFlags::CATCH, true);
                        let index = binder.add(decl, None);
                        binder.declare(binder.scope, name, index, Meanings::VALUE, false);
                    }
                    pattern => {
                        let owner = PatternOwner::Variable {
                            var_kind: None,
                            scope: binder.scope,
                            exported: false,
                            catch: true,
                        };
                        binder.pattern(pattern, owner);
                    }
                }
            }
            binder.visit_block_statement(&it.body);
        });
    }

    fn visit_with_statement(&mut self, it: &WithStatement<'a>) {
        self.visit_expression(&it.object);
        let scope = self.scopes.push(ScopeKind::With, Some(self.scope));
        let outer = std::mem::replace(&mut self.scope, scope);
        self.visit_statement(&it.body);
        self.scope = outer;
    }

    // --- Declarations ---

    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        let var_kind = match it.kind {
            VariableDeclarationKind::Var => VarKind::Var,
            VariableDeclarationKind::Let => VarKind::Let,
            VariableDeclarationKind::Const => VarKind::Const,
            VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing => VarKind::Using,
        };
        let exported = std::mem::take(&mut self.exporting);
        self.ambient_if(it.declare, |binder| {
            for declarator in &it.declarations {
                binder.declarator(declarator, var_kind, exported);
            }
        });
    }

    fn visit_function(&mut self, it: &Function<'a>, _flags: ScopeFlags) {
        let declaration = matches!(
            it.r#type,
            FunctionType::FunctionDeclaration | FunctionType::TSDeclareFunction
        );
        let exported = std::mem::take(&mut self.exporting);
        let address = it.unstable_address();
        let name = it.id.as_ref().map(|id| id.name.as_str());
        let kind = if declaration {
            DeclKind::Function
        } else {
            DeclKind::FunctionExpression
        };
        let has_body = it.body.is_some();
        let mut decl = self.blank(kind, name, range(it.span));
        decl.flags = decl
            .flags
            .with(DeclFlags::HAS_BODY, has_body)
            .with(DeclFlags::AMBIENT, it.declare);
        decl.node = self.node(address).filter(|_| has_body);
        let index = self.add(decl, declaration.then_some(ModuleKey::Function));
        if declaration {
            if let Some(id) = &it.id {
                self.declare(self.scope, id.name.as_str(), index, Meanings::VALUE, exported);
            }
            self.callable(index, it, Frame::Function, None);
        } else {
            self.inline_decls.insert(address, index);
            self.callable(index, it, Frame::Function, it.id.as_ref());
        }
    }

    fn visit_arrow_function_expression(&mut self, it: &oxc_ast::ast::ArrowFunctionExpression<'a>) {
        let address = it.unstable_address();
        let mut decl = self.blank(DeclKind::Arrow, None, range(it.span));
        decl.flags = decl.flags.with(DeclFlags::HAS_BODY, true);
        decl.node = self.node(address);
        let index = self.add(decl, None);
        self.inline_decls.insert(address, index);
        self.parameter_decorators(&it.params);
        let inner = self.scopes.push(ScopeKind::Arrow, Some(self.scope));
        self.scopes.scope_mut(inner).body = Some(range(it.body.span()));
        let outer_scope = std::mem::replace(&mut self.scope, inner);
        let outer_parameters = self.parameters;
        let outer_container = self.container;
        self.type_parameters(it.type_parameters.as_deref());
        let separate = uses_outer_scope(&it.params, self.context).then_some(inner);
        self.parameters = self.parameter_set(outer_parameters, separate);
        self.formal_parameters(index, &it.params);
        let (returns, awaits) = self.returns(it.return_type.as_deref(), it.r#async);
        let decl = self.decl_mut(index);
        decl.returns = returns;
        decl.awaits = awaits;
        self.parameters = outer_parameters;
        self.container = self.new_container();
        match &it.body {
            ArrowFunctionBody::FunctionBody(body) => self.statements(&body.statements),
            body => {
                if let Some(expression) = body.as_expression() {
                    self.visit_expression(expression);
                }
            }
        }
        self.container = outer_container;
        self.scope = outer_scope;
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        let declaration = it.r#type == ClassType::ClassDeclaration;
        let exported = std::mem::take(&mut self.exporting);
        let address = it.unstable_address();
        let outer_ambient = self.ambient;
        self.ambient |= it.declare;
        let name = it.id.as_ref().map(|id| id.name.as_str());
        let kind = if declaration {
            DeclKind::Class
        } else {
            DeclKind::ClassExpression
        };
        let mut decl = self.blank(kind, name, range(it.span));
        decl.flags = decl.flags.with(DeclFlags::DECORATED, !it.decorators.is_empty());
        decl.node = if self.ambient { None } else { self.node(address) };
        let selected = decl.node.is_some();
        let index = self.add(decl, declaration.then_some(ModuleKey::Class));
        let symbol = match (&it.id, declaration) {
            (Some(id), true) => {
                Some(self.declare(self.scope, id.name.as_str(), index, Meanings::BOTH, exported))
            }
            (_, false) => {
                self.inline_decls.insert(address, index);
                None
            }
            _ => None,
        };
        let class = self.classes.len() as u32;
        self.classes.push(ClassDraft {
            decl: index,
            heritage: None,
            members: Vec::new(),
            private: Default::default(),
        });
        self.class_stack.push(class);
        for decorator in &it.decorators {
            self.visit_decorator(decorator);
        }
        if let Some(heritage) = &it.heritage {
            self.visit_expression(&heritage.expression);
            let converted = self.converted(&heritage.expression);
            self.classes[class as usize].heritage = Some(converted);
        }
        let inner = self.scopes.push(ScopeKind::Class, Some(self.scope));
        if let Some(id) = &it.id {
            let name = id.name.as_str();
            match symbol {
                Some(symbol) => self.scopes.alias(inner, name, symbol, Meanings::BOTH),
                None => {
                    self.declare(inner, name, index, Meanings::BOTH, false);
                }
            }
        }
        let outer_scope = std::mem::replace(&mut self.scope, inner);
        let outer_container = self.container;
        self.container = self.new_container();
        self.type_parameters(it.type_parameters.as_deref());
        if selected {
            self.class_heritage(it);
        }
        let members = self.class_members(class, &it.body.body);
        for (element, member) in it.body.body.iter().zip(members) {
            self.class_element(index, element, member);
        }
        let constructor = self.classes[class as usize]
            .members
            .iter()
            .copied()
            .find(|&member| {
                self.decl(member).is_some_and(|decl| {
                    decl.kind == DeclKind::Constructor && decl.flags.has(DeclFlags::HAS_BODY)
                })
            });
        self.decl_mut(index).constructor = constructor;
        self.container = outer_container;
        self.scope = outer_scope;
        self.class_stack.pop();
        self.ambient = outer_ambient;
    }

    fn visit_object_expression(&mut self, it: &ObjectExpression<'a>) {
        self.object(it);
    }

    fn visit_ts_namespace_declaration(&mut self, it: &TSNamespaceDeclaration<'a>) {
        let exported = std::mem::take(&mut self.exporting);
        self.namespace(it, exported, true);
    }

    fn visit_ts_global_declaration(&mut self, it: &TSGlobalDeclaration<'a>) {
        let scope = self.scopes.push(ScopeKind::Global, None);
        let outer = std::mem::replace(&mut self.scope, scope);
        self.ambient_if(true, |binder| {
            binder.framed(Frame::Boundary, |binder| binder.statements(&it.body.body));
        });
        self.scope = outer;
    }

    fn visit_ts_external_module_declaration(&mut self, it: &TSExternalModuleDeclaration<'a>) {
        self.ambient_if(true, |binder| {
            let name = it.id.value.as_str();
            let decl = binder.blank(DeclKind::Namespace, Some(name), range(it.span));
            let index = binder.add(decl, None);
            let per = binder.namespace_scope(name, index, false, ScopeKind::Namespace);
            binder.framed(Frame::Boundary, |binder| {
                let outer = std::mem::replace(&mut binder.scope, per);
                if let Some(body) = &it.body {
                    binder.statements(&body.body);
                }
                binder.scope = outer;
            });
        });
    }

    fn visit_ts_enum_declaration(&mut self, it: &TSEnumDeclaration<'a>) {
        let exported = std::mem::take(&mut self.exporting);
        self.ambient_if(it.declare, |binder| {
            let name = it.id.name.as_str();
            let mut decl = binder.blank(DeclKind::Enum, Some(name), range(it.span));
            decl.node = binder.node(it.unstable_address());
            let index = binder.add(decl, Some(ModuleKey::Enum));
            let scope = binder.namespace_scope(name, index, exported, ScopeKind::Enum);
            binder.framed(Frame::Boundary, |binder| {
                let outer = std::mem::replace(&mut binder.scope, scope);
                for member in &it.body.members {
                    let name = match &member.id {
                        TSEnumMemberName::Identifier(id) => Some(id.name.as_str()),
                        _ => None,
                    };
                    let decl = binder.blank(DeclKind::EnumMember, name, range(member.span));
                    let member_index = binder.add(decl, None);
                    if let Some(name) = name {
                        binder.declare(scope, name, member_index, Meanings::VALUE, true);
                    }
                    if let Some(initializer) = &member.initializer {
                        binder.visit_expression(initializer);
                    }
                }
                binder.scope = outer;
            });
        });
    }

    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        let Some(specifiers) = &it.specifiers else {
            return;
        };
        for specifier in specifiers {
            let local = match specifier {
                ImportDeclarationSpecifier::ImportSpecifier(it) => &it.local,
                ImportDeclarationSpecifier::ImportDefaultSpecifier(it) => &it.local,
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(it) => &it.local,
            };
            let alias = match self.module_imports.get(&local.span.start) {
                Some(&index) => AliasRef::Module(index),
                None => self.unsupported_alias(),
            };
            self.import_alias(local, false, alias);
        }
    }

    fn visit_ts_import_equals_declaration(&mut self, it: &TSImportEqualsDeclaration<'a>) {
        let exported = std::mem::take(&mut self.exporting);
        let type_only = it.import_kind == ImportOrExportKind::Type;
        let alias = match &it.module_reference {
            TSModuleReference::ExternalModuleReference(_) => match self.module_imports.get(&it.id.span.start)
            {
                Some(&index) => AliasRef::Module(index),
                None => self.unsupported_alias(),
            },
            TSModuleReference::IdentifierReference(id) => {
                self.aliases.push(AliasDraft::Qualified {
                    first: id.name.as_str(),
                    place: self.place(),
                    rest: Vec::new(),
                    type_only,
                });
                AliasRef::Local((self.aliases.len() - 1) as u32)
            }
            TSModuleReference::QualifiedName(qualified) => {
                let mut rest: Vec<Box<str>> = vec![qualified.right.name.as_str().into()];
                let mut current = &qualified.left;
                let first = loop {
                    match current {
                        TSTypeName::IdentifierReference(id) => break Some(id.name.as_str()),
                        TSTypeName::QualifiedName(inner) => {
                            rest.push(inner.right.name.as_str().into());
                            current = &inner.left;
                        }
                        TSTypeName::ThisExpression(_) => break None,
                    }
                };
                rest.reverse();
                match first {
                    Some(first) => {
                        self.aliases.push(AliasDraft::Qualified {
                            first,
                            place: self.place(),
                            rest,
                            type_only,
                        });
                        AliasRef::Local((self.aliases.len() - 1) as u32)
                    }
                    None => self.unsupported_alias(),
                }
            }
        };
        self.import_alias(&it.id, exported, alias);
    }

    fn visit_export_named_declaration(&mut self, _it: &ExportNamedDeclaration<'a>) {}

    fn visit_export_declaration(&mut self, it: &oxc_ast::ast::ExportDeclaration<'a>) {
        self.exporting = true;
        self.visit_declaration(&it.declaration);
        self.exporting = false;
    }

    fn visit_export_default_declaration(&mut self, it: &ExportDefaultDeclaration<'a>) {
        match &it.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                self.visit_function(function, ScopeFlags::Function);
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => self.visit_class(class),
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                self.visit_ts_interface_declaration(interface);
            }
            other => {
                if let Some(expression) = other.as_expression() {
                    self.export_expression(expression);
                }
            }
        }
    }

    fn visit_ts_export_assignment(&mut self, it: &TSExportAssignment<'a>) {
        self.export_expression(&it.expression);
    }

    // --- Censuses ---

    fn visit_simple_assignment_target(&mut self, it: &SimpleAssignmentTarget<'a>) {
        walk::walk_simple_assignment_target(self, it);
        self.member_target(it);
    }

    fn visit_unary_expression(&mut self, it: &UnaryExpression<'a>) {
        walk::walk_unary_expression(self, it);
        if it.operator == UnaryOperator::Delete {
            match unwrap(&it.argument) {
                Expression::StaticMemberExpression(member) => {
                    self.write(&member.object, Some(member.property.name.as_str().into()));
                }
                Expression::ComputedMemberExpression(member) => {
                    let name = literal_name(&member.expression);
                    self.write(&member.object, name);
                }
                _ => {}
            }
        }
    }

    fn visit_binary_expression(&mut self, it: &BinaryExpression<'a>) {
        walk::walk_binary_expression(self, it);
        if it.operator == BinaryOperator::Instanceof
            && let Some(key) = self.key_draft(&it.left)
        {
            self.instanceof.push(key);
        }
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        walk::walk_call_expression(self, it);
        // Outside a condition, only a call to a type predicate narrows, and only a callee
        // written as a name path can be one.
        let callee = self.path_draft(&it.callee);
        let condition = self.condition > 0;
        if callee.is_none() && !condition {
            return;
        }
        let keys: Vec<_> = it
            .arguments
            .iter()
            .filter_map(|argument| match argument {
                Argument::SpreadElement(_) => None,
                _ => argument
                    .as_expression()
                    .and_then(|argument| self.key_draft(argument)),
            })
            .collect();
        if !keys.is_empty() {
            self.calls.push((callee, condition, keys));
        }
    }
}
