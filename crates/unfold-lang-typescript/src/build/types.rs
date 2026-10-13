//! Type declarations (interfaces, type aliases, enums) and the written members of type
//! declarations and classes.
//!
//! An interface's members, and those of an object type literal that is a type alias's whole
//! aliased type, are type syntax: they never become nodes, only [`MemberDraft`]s beside the tree.
//! An enum's members are nodes (their initializers can hold calls) and [`MemberDraft`]s too.

use oxc_allocator::UnstableAddress;
use oxc_ast::ast::{
    FormalParameter, FormalParameters, PropertyKey, TSEnumDeclaration, TSEnumMemberName,
    TSInterfaceDeclaration, TSSignature, TSType, TSTypeAliasDeclaration, TSTypeAnnotation,
    TSTypeParameterDeclaration,
};
use oxc_parser::Kind;
use oxc_span::{GetSpan, Span};
use unfold_core::text::ByteRange;
use unfold_core::vocab::MemberKind;

use super::{Builder, Ctx, Head, range};
use crate::syntax::{Cat, Flags, Label, MemberDraft};

impl<'a> Builder<'a, '_> {
    /// A type declaration's node: its range starts at a preceding `export` keyword, and it
    /// records its oxc origin so binding can name its block.
    fn declaration_node(&mut self, parent: u32, label: Label, head: Head, span: Span, ctx: Ctx) -> u32 {
        let mut span = range(span);
        if let Some(start) = head.export_start {
            span.start = span.start.min(start);
        }
        let flags = ctx.flags().with(Flags::DEFAULT, head.default);
        self.push(parent, label, head.cat, span, flags)
    }

    /// `interface X<…> extends … { … }`: its name, type parameters, `extends` clause (from
    /// `extends` through its last type), and body, with every member recorded.
    pub(super) fn interface(
        &mut self,
        parent: u32,
        label: Label,
        it: &TSInterfaceDeclaration<'a>,
        head: Head,
        ctx: Ctx,
    ) {
        let ctx = ctx.ambient(it.declare);
        let node = self.declaration_node(parent, label, head, it.span, ctx);
        self.syntax.record_origin(it.unstable_address(), node);
        let flags = ctx.flags();
        self.push_text(
            node,
            Label::Name,
            Cat::Identifier,
            it.id.span,
            it.id.name.as_str(),
        );
        self.type_parameter_node(node, it.type_parameters.as_deref(), ctx);
        if let (Some(first), Some(last)) = (it.extends.first(), it.extends.last()) {
            let start = self
                .token_before(first.span.start)
                .filter(|token| token.kind() == Kind::Extends)
                .map_or(first.span.start, |token| token.start());
            let clause = ByteRange {
                start,
                end: last.span.end,
            };
            self.push(node, Label::HeritageClauses, Cat::Type, clause, flags);
        }
        self.push(node, Label::Body, Cat::Type, range(it.body.span), flags);
        for member in &it.body.body {
            self.signature_member(node, member);
        }
    }

    /// `type X<…> = T`: its name, type parameters, and aliased type; the members of an object
    /// type literal that is the whole aliased type are recorded.
    pub(super) fn type_alias(
        &mut self,
        parent: u32,
        label: Label,
        it: &TSTypeAliasDeclaration<'a>,
        head: Head,
        ctx: Ctx,
    ) {
        let ctx = ctx.ambient(it.declare);
        let node = self.declaration_node(parent, label, head, it.span, ctx);
        self.syntax.record_origin(it.unstable_address(), node);
        self.push_text(
            node,
            Label::Name,
            Cat::Identifier,
            it.id.span,
            it.id.name.as_str(),
        );
        self.type_parameter_node(node, it.type_parameters.as_deref(), ctx);
        let aliased = range(it.type_annotation.span());
        self.push(node, Label::Type, Cat::Type, aliased, ctx.flags());
        if let TSType::TSTypeLiteral(literal) = &it.type_annotation {
            for member in &literal.members {
                self.signature_member(node, member);
            }
        }
    }

    /// `enum X { … }` or `const enum X { … }`: its name and body, whose members keep their
    /// initializers (which may call) and are recorded as `value` members.
    pub(super) fn enumeration(
        &mut self,
        parent: u32,
        label: Label,
        it: &TSEnumDeclaration<'a>,
        head: Head,
        ctx: Ctx,
    ) {
        let ctx = ctx.ambient(it.declare);
        let node = self.declaration_node(parent, label, head, it.span, ctx);
        self.syntax.record_origin(it.unstable_address(), node);
        let flags = ctx.flags();
        self.push_text(
            node,
            Label::Name,
            Cat::Identifier,
            it.id.span,
            it.id.name.as_str(),
        );
        let body = self.push(node, Label::Body, Cat::Other, range(it.body.span), flags);
        for member in &it.body.members {
            let item = self.push(body, Label::Other, Cat::Other, range(member.span), flags);
            if let Some(initializer) = &member.initializer {
                self.expression(item, Label::Initializer, initializer, ctx);
            }
            let name = match &member.id {
                TSEnumMemberName::ComputedString(_) | TSEnumMemberName::ComputedTemplateString(_) => {
                    self.bracketed(member.id.span())
                }
                _ => range(member.id.span()),
            };
            self.syntax.record_member(MemberDraft {
                value: member
                    .initializer
                    .as_ref()
                    .map(|initializer| range(initializer.span())),
                ..MemberDraft::new(node, MemberKind::Value, Some(name))
            });
        }
    }

    /// A declaration's type parameter list: one type node, never a slot.
    fn type_parameter_node(
        &mut self,
        node: u32,
        parameters: Option<&TSTypeParameterDeclaration<'a>>,
        ctx: Ctx,
    ) {
        if let Some(parameters) = parameters {
            self.push(node, Label::Other, Cat::Type, range(parameters.span), ctx.flags());
        }
    }

    /// Records one member of an interface or object type literal owned by `owner`.
    fn signature_member(&mut self, owner: u32, member: &TSSignature<'a>) {
        let draft = match member {
            TSSignature::TSPropertySignature(it) => MemberDraft {
                optional: it.optional,
                readonly: it.readonly,
                ty: annotation_range(it.type_annotation.as_deref()),
                ..MemberDraft::new(
                    owner,
                    MemberKind::Property,
                    Some(self.key_range(&it.key, it.computed)),
                )
            },
            TSSignature::TSMethodSignature(it) => MemberDraft {
                optional: it.optional,
                ty: Some(signature_range(
                    it.type_parameters.as_deref(),
                    &it.params,
                    it.return_type.as_deref(),
                )),
                ..MemberDraft::new(
                    owner,
                    MemberKind::Method,
                    Some(self.key_range(&it.key, it.computed)),
                )
            },
            TSSignature::TSIndexSignature(it) => {
                let parameter = it.parameter.span;
                let start = self
                    .token_before(parameter.start)
                    .filter(|token| token.kind() == Kind::LBrack)
                    .map_or(parameter.start, |token| token.start());
                let end = self
                    .token_from(parameter.end, Kind::RBrack)
                    .map_or(parameter.end, |token| token.end());
                MemberDraft {
                    readonly: it.readonly,
                    ty: Some(range(it.type_annotation.type_annotation.span())),
                    ..MemberDraft::new(owner, MemberKind::Index, Some(ByteRange { start, end }))
                }
            }
            TSSignature::TSCallSignatureDeclaration(it) => MemberDraft {
                ty: Some(signature_range(
                    it.type_parameters.as_deref(),
                    &it.params,
                    it.return_type.as_deref(),
                )),
                ..MemberDraft::new(owner, MemberKind::Call, None)
            },
            TSSignature::TSConstructSignatureDeclaration(it) => MemberDraft {
                ty: Some(signature_range(
                    it.type_parameters.as_deref(),
                    &it.params,
                    it.return_type.as_deref(),
                )),
                ..MemberDraft::new(owner, MemberKind::Construct, None)
            },
        };
        self.syntax.record_member(draft);
    }

    /// Records a class's instance property (a field or `accessor`) owned by class node `owner`.
    pub(super) fn property_member(
        &mut self,
        owner: u32,
        key: (&PropertyKey<'a>, bool),
        flags: (bool, bool),
        annotation: Option<&TSTypeAnnotation<'a>>,
        initialized: bool,
    ) {
        let (optional, readonly) = flags;
        let name = self.key_range(key.0, key.1);
        self.syntax.record_member(MemberDraft {
            optional,
            readonly,
            default: initialized,
            ty: annotation_range(annotation),
            ..MemberDraft::new(owner, MemberKind::Property, Some(name))
        });
    }

    /// Records the parameter properties (`constructor(public x: T)`) of a constructor of class
    /// node `owner`.
    pub(super) fn parameter_properties(&mut self, owner: u32, parameters: &FormalParameters<'a>) {
        for parameter in &parameters.items {
            if !is_parameter_property(parameter) {
                continue;
            }
            let (pattern, default) = match &parameter.pattern {
                oxc_ast::ast::BindingPattern::AssignmentPattern(it) => (&it.left, true),
                pattern => (pattern, parameter.initializer.is_some()),
            };
            let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = pattern else {
                continue;
            };
            self.syntax.record_member(MemberDraft {
                optional: parameter.optional,
                readonly: parameter.readonly,
                default,
                ty: annotation_range(parameter.type_annotation.as_deref()),
                ..MemberDraft::new(owner, MemberKind::Property, Some(range(id.span)))
            });
        }
    }

    /// The written range of a member or property name: a computed name with its brackets.
    fn key_range(&self, key: &PropertyKey<'a>, computed: bool) -> ByteRange {
        if computed {
            self.bracketed(key.span())
        } else {
            range(key.span())
        }
    }

    /// `inner` widened to the brackets around it (a computed name).
    pub(super) fn bracketed(&self, inner: Span) -> ByteRange {
        let start = self
            .token_before(inner.start)
            .filter(|token| token.kind() == Kind::LBrack)
            .map_or(inner.start, |token| token.start());
        let end = self
            .token_from(inner.end, Kind::RBrack)
            .map_or(inner.end, |token| token.end());
        ByteRange { start, end }
    }
}

impl MemberDraft {
    /// A member of `owner` with every flag unset and no type or value.
    const fn new(owner: u32, kind: MemberKind, name: Option<ByteRange>) -> Self {
        Self {
            owner,
            kind,
            name,
            optional: false,
            readonly: false,
            default: false,
            ty: None,
            value: None,
        }
    }
}

/// Whether a constructor parameter declares a property (`public`, `private`, `protected`,
/// `readonly`, or `override`).
fn is_parameter_property(parameter: &FormalParameter<'_>) -> bool {
    parameter.accessibility.is_some() || parameter.readonly || parameter.r#override
}

/// The written type of an annotation, without its colon.
fn annotation_range(annotation: Option<&TSTypeAnnotation<'_>>) -> Option<ByteRange> {
    annotation.map(|annotation| range(annotation.type_annotation.span()))
}

/// A signature's text after its name: from its type parameters (or its parameter list) through
/// its return type (or its parameter list).
fn signature_range(
    type_parameters: Option<&TSTypeParameterDeclaration<'_>>,
    parameters: &FormalParameters<'_>,
    returns: Option<&TSTypeAnnotation<'_>>,
) -> ByteRange {
    let start = type_parameters.map_or(parameters.span.start, |parameters| parameters.span.start);
    let end = returns.map_or(parameters.span.end, |returns| returns.type_annotation.span().end);
    ByteRange {
        start: start.min(end),
        end,
    }
}
