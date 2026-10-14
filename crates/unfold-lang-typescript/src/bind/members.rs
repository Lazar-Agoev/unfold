//! Classes and their members, namespaces and enums, module declarations (exported expressions
//! and import aliases), object literals (the members of constant ones), and member writes.

use oxc_allocator::{GetAddress, UnstableAddress};
use oxc_ast::ast::{
    BindingIdentifier, ClassElement, Expression, MethodDefinitionKind, ObjectExpression, ObjectPropertyKind,
    PropertyKey, PropertyKind, SimpleAssignmentTarget, StaticBlock, TSNamespaceDeclaration,
    TSNamespaceDeclarationBody,
};
use oxc_ast_visit::Visit;
use oxc_span::GetSpan;

use super::forms::{key_name, literal_name, private_name};
use super::ir::unwrap;
use super::scope::ScopeKind;
use super::{AliasDraft, Binder, Frame, ModuleKey};
use crate::module_syntax;
use crate::payload::binding::{AliasRef, DeclFlags, DeclKind, Meanings, MemberWrite, ObjectFacts};
use crate::spans::range;

impl<'a> Binder<'a> {
    // --- Classes --------------------------------------------------------------------------

    /// The declarations of a class's members, before any member body is walked (so private
    /// names resolve anywhere in the class).
    pub(super) fn class_members(&mut self, class: u32, elements: &[ClassElement<'a>]) -> Vec<Option<u32>> {
        let mut members = Vec::with_capacity(elements.len());
        for element in elements {
            let member = match element {
                ClassElement::MethodDefinition(it) => {
                    let kind = match it.kind {
                        MethodDefinitionKind::Constructor => DeclKind::Constructor,
                        MethodDefinitionKind::Method => DeclKind::Method,
                        MethodDefinitionKind::Get => DeclKind::Getter,
                        MethodDefinitionKind::Set => DeclKind::Setter,
                    };
                    let name = if kind == DeclKind::Constructor {
                        None
                    } else {
                        key_name(&it.key, it.computed)
                    };
                    let has_body = it.value.body.is_some();
                    let mut decl = self.blank(kind, name.as_deref(), range(it.span));
                    decl.flags = decl
                        .flags
                        .with(DeclFlags::STATIC, it.r#static)
                        .with(DeclFlags::HAS_BODY, has_body)
                        .with(DeclFlags::AMBIENT, it.value.declare)
                        .with(
                            DeclFlags::PRIVATE,
                            matches!(it.key, PropertyKey::PrivateIdentifier(_)),
                        );
                    decl.node = self.node(it.unstable_address()).filter(|_| has_body);
                    if kind == DeclKind::Getter {
                        decl.ty = self.annotation(it.value.return_type.as_deref());
                    }
                    Some((self.add(decl, None), private_name(&it.key)))
                }
                ClassElement::PropertyDefinition(it) => {
                    let name = key_name(&it.key, it.computed);
                    let mut decl = self.blank(DeclKind::Property, name.as_deref(), range(it.span));
                    decl.flags = decl
                        .flags
                        .with(DeclFlags::STATIC, it.r#static)
                        .with(DeclFlags::AMBIENT, it.declare)
                        .with(
                            DeclFlags::PRIVATE,
                            matches!(it.key, PropertyKey::PrivateIdentifier(_)),
                        );
                    decl.ty = self.annotation(it.type_annotation.as_deref());
                    Some((self.add(decl, None), private_name(&it.key)))
                }
                ClassElement::AccessorProperty(it) => {
                    let name = key_name(&it.key, it.computed);
                    let mut decl = self.blank(DeclKind::Property, name.as_deref(), range(it.span));
                    decl.flags = decl.flags.with(DeclFlags::STATIC, it.r#static).with(
                        DeclFlags::PRIVATE,
                        matches!(it.key, PropertyKey::PrivateIdentifier(_)),
                    );
                    decl.ty = self.annotation(it.type_annotation.as_deref());
                    Some((self.add(decl, None), private_name(&it.key)))
                }
                ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => None,
            };
            if let Some((index, private)) = member {
                let draft = &mut self.classes[class as usize];
                draft.members.push(index);
                if let Some(private) = private {
                    draft.private.entry(private).or_default().push(index);
                }
            }
            members.push(member.map(|(index, _)| index));
        }
        members
    }

    /// Walks one class member whose declaration is `member`.
    pub(super) fn class_element(&mut self, class_decl: u32, element: &ClassElement<'a>, member: Option<u32>) {
        let key = |binder: &mut Self, key: &PropertyKey<'a>, computed: bool| {
            if computed && let Some(expression) = key.as_expression() {
                binder.framed(Frame::MemberKey, |binder| binder.visit_expression(expression));
            }
        };
        match (element, member) {
            (ClassElement::MethodDefinition(it), Some(member)) => {
                for decorator in &it.decorators {
                    self.visit_decorator(decorator);
                }
                key(self, &it.key, it.computed);
                let frame = Frame::Member {
                    class: class_decl,
                    is_static: it.r#static,
                };
                self.callable(member, &it.value, frame, None);
            }
            (ClassElement::PropertyDefinition(it), Some(member)) => {
                for decorator in &it.decorators {
                    self.visit_decorator(decorator);
                }
                key(self, &it.key, it.computed);
                if let Some(value) = &it.value {
                    let frame = Frame::Member {
                        class: class_decl,
                        is_static: it.r#static,
                    };
                    self.ambient_if(it.declare, |binder| {
                        binder.framed(frame, |binder| binder.visit_expression(value));
                    });
                    let converted = self.converted(value);
                    self.decl_mut(member).initializer = Some(converted);
                }
            }
            (ClassElement::AccessorProperty(it), Some(member)) => {
                for decorator in &it.decorators {
                    self.visit_decorator(decorator);
                }
                key(self, &it.key, it.computed);
                if let Some(value) = &it.value {
                    let frame = Frame::Member {
                        class: class_decl,
                        is_static: it.r#static,
                    };
                    self.framed(frame, |binder| binder.visit_expression(value));
                    let converted = self.converted(value);
                    self.decl_mut(member).initializer = Some(converted);
                }
            }
            (ClassElement::StaticBlock(it), _) => self.static_block(class_decl, it),
            _ => {}
        }
    }

    pub(super) fn static_block(&mut self, class_decl: u32, block: &StaticBlock<'a>) {
        let frame = Frame::Member {
            class: class_decl,
            is_static: true,
        };
        self.scoped(ScopeKind::StaticBlock, |binder| {
            binder.framed(frame, |binder| binder.statements(&block.body));
        });
    }

    // --- Namespaces and modules -----------------------------------------------------------

    /// A namespace declaration; `outermost` is false for the inner names of `namespace A.B`.
    pub(super) fn namespace(&mut self, it: &TSNamespaceDeclaration<'a>, exported: bool, outermost: bool) {
        self.ambient_if(it.declare, |binder| {
            let name = it.id.name.as_str();
            let decl = binder.blank(DeclKind::Namespace, Some(name), range(it.span));
            let index = binder.add(decl, outermost.then_some(ModuleKey::Namespace));
            let per = binder.namespace_scope(name, index, exported, ScopeKind::Namespace);
            binder.framed(Frame::Boundary, |binder| {
                let outer = std::mem::replace(&mut binder.scope, per);
                let container = binder.container;
                binder.container = binder.new_container();
                match &it.body {
                    TSNamespaceDeclarationBody::TSNamespaceDeclaration(inner) => {
                        binder.namespace(inner, true, false);
                    }
                    TSNamespaceDeclarationBody::TSModuleBlock(block) => binder.statements(&block.body),
                }
                binder.container = container;
                binder.scope = outer;
            });
        });
    }

    /// Declares a namespace (or enum) name and returns the scope its body binds in: for a
    /// namespace, a scope of its own whose parent is the members shared by every declaration
    /// of the name; for an enum, the shared scope itself.
    pub(super) fn namespace_scope(
        &mut self,
        name: &'a str,
        decl: u32,
        exported: bool,
        kind: ScopeKind,
    ) -> u32 {
        let symbol = self.declare(self.scope, name, decl, Meanings::BOTH, exported);
        let shared = match self.scopes.symbols[symbol as usize].namespace {
            Some(shared) => shared,
            None => {
                let shared_kind = if kind == ScopeKind::Enum {
                    ScopeKind::Enum
                } else {
                    ScopeKind::Namespace
                };
                let shared = self.scopes.push(shared_kind, Some(self.scope));
                self.scopes.symbols[symbol as usize].namespace = Some(shared);
                if kind != ScopeKind::Enum {
                    self.scopes.scope_mut(shared).namespace = Some(symbol);
                }
                shared
            }
        };
        if kind == ScopeKind::Enum {
            return shared;
        }
        let per = self.scopes.push(ScopeKind::Namespace, Some(shared));
        self.scopes.scope_mut(per).export = Some(shared);
        per
    }

    /// `export default <expression>` or `export = <expression>`.
    pub(super) fn export_expression(&mut self, expression: &Expression<'a>) {
        self.visit_expression(expression);
        if module_syntax::is_access_path(expression) {
            return;
        }
        let unwrapped = module_syntax::unwrap_expression(expression);
        let mut decl = self.blank(DeclKind::Expression, None, range(unwrapped.span()));
        decl.inline = self.inline_decls.get(&unwrapped.address()).copied();
        self.add(decl, Some(ModuleKey::Expression));
    }

    /// The import alias declared by `local`, and what it imports.
    pub(super) fn import_alias(&mut self, local: &BindingIdentifier<'a>, exported: bool, alias: AliasRef) {
        let name = local.name.as_str();
        let decl = self.blank(DeclKind::Import, Some(name), range(local.span));
        let index = self.add(decl, None);
        let symbol = self.declare(self.scope, name, index, Meanings::BOTH, exported);
        self.alias_refs.insert(symbol, alias);
    }

    pub(super) fn unsupported_alias(&mut self) -> AliasRef {
        self.aliases.push(AliasDraft::Unsupported);
        AliasRef::Local((self.aliases.len() - 1) as u32)
    }

    // --- Object literals ------------------------------------------------------------------

    pub(super) fn object(&mut self, it: &ObjectExpression<'a>) {
        let tracked = self.pending_objects.remove(&it.unstable_address());
        let container = std::mem::replace(&mut self.container, self.next_container);
        self.next_container += 1;
        let mut members = Vec::new();
        let mut closed = true;
        for property in &it.properties {
            let property = match property {
                ObjectPropertyKind::SpreadProperty(spread) => {
                    closed = false;
                    self.visit_expression(&spread.argument);
                    continue;
                }
                ObjectPropertyKind::ObjectProperty(property) => property,
            };
            let name = key_name(&property.key, property.computed);
            if property.computed {
                closed &= name.is_some();
                if let Some(key) = property.key.as_expression() {
                    self.visit_expression(key);
                }
            }
            let method = match &property.value {
                Expression::FunctionExpression(function)
                    if property.method || property.kind != PropertyKind::Init =>
                {
                    Some(function)
                }
                _ => None,
            };
            if let Some(function) = method {
                let kind = match property.kind {
                    PropertyKind::Get => DeclKind::Getter,
                    PropertyKind::Set => DeclKind::Setter,
                    PropertyKind::Init => DeclKind::Method,
                };
                let has_body = function.body.is_some();
                let mut decl = self.blank(kind, name.as_deref(), range(property.span));
                decl.flags = decl.flags.with(DeclFlags::HAS_BODY, has_body);
                decl.node = self.node(property.unstable_address()).filter(|_| has_body);
                let index = self.add(decl, None);
                members.push(index);
                self.callable(index, function, Frame::Function, None);
                continue;
            }
            self.visit_expression(&property.value);
            if tracked.is_none() {
                continue;
            }
            let kind = if property.shorthand {
                DeclKind::Shorthand
            } else {
                DeclKind::PropertyAssignment
            };
            let mut decl = self.blank(kind, name.as_deref(), range(property.span));
            decl.initializer = Some(self.converted(&property.value));
            if !property.shorthand {
                let value = unwrap(&property.value);
                if matches!(
                    value,
                    Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_)
                ) {
                    decl.inline = self.inline_decls.get(&value.address()).copied();
                }
            }
            members.push(self.add(decl, None));
        }
        self.container = container;
        if let Some((variable, plain)) = tracked {
            let object = self.objects.len() as u32;
            for &member in &members {
                self.decl_mut(member).object = Some(object);
            }
            self.objects.push(ObjectFacts {
                variable,
                members,
                closed,
                plain,
            });
        }
    }

    /// Records a write to a member of `object` (`name`, or any member for a computed key).
    pub(super) fn write(&mut self, object: &Expression<'a>, name: Option<Box<str>>) {
        let object = self.converted(object);
        self.writes.push(MemberWrite { object, name });
    }

    pub(super) fn member_target(&mut self, target: &SimpleAssignmentTarget<'a>) {
        match target {
            SimpleAssignmentTarget::StaticMemberExpression(it) => {
                self.write(&it.object, Some(it.property.name.as_str().into()));
            }
            SimpleAssignmentTarget::ComputedMemberExpression(it) => {
                let name = literal_name(&it.expression);
                self.write(&it.object, name);
            }
            _ => {}
        }
    }
}
