//! Typed records, read from oxc's tree as the lowering creates each node
//! (`docs/languages/typescript.md`, "Typed records"): a signature and parameters per function,
//! and the written members of interfaces, object type literals that are a type alias's whole
//! aliased type, enums, and classes. Everything is read from what is written; nothing is
//! inferred.
//!
//! It also lists the written types whose names become `types` links ([`Typed::linked`]): the
//! return and parameter types recorded here, class property types, variable annotations, catch
//! clause annotations, a type alias's aliased type, and an interface's body. Binding walks
//! exactly those types for names. Each is recorded with the node its names link from
//! ([`Owners`]), as is each class's and interface's head, whose heritage clauses binding reads.

use std::cmp::Reverse;

use oxc_ast::ast::{
    BindingPattern, ClassElement, FormalParameter, FormalParameters, MethodDefinitionKind, PropertyKey,
    TSEnumDeclaration, TSEnumMemberName, TSInterfaceDeclaration, TSSignature, TSThisParameter, TSType,
    TSTypeAliasDeclaration, TSTypeAnnotation, TSTypeParameterDeclaration,
};
use oxc_parser::Kind;
use oxc_span::{GetSpan, Span};
use rustc_hash::FxHashSet;
use unfold_core::text::ByteRange;
use unfold_core::typed::{Member, Parameter, Signature, WrittenType};
use unfold_core::vocab::{MemberKind, ParameterKind};
use unfold_engine::facts::TypeFacts;

use crate::spans::{Tokens, range};

/// What a function's typed records are read from.
pub(crate) struct Callable<'s, 'a> {
    pub(crate) this: Option<&'s TSThisParameter<'a>>,
    pub(crate) parameters: &'s FormalParameters<'a>,
    pub(crate) returns: Option<&'s TSTypeAnnotation<'a>>,
}

/// One file's typed records, and the written types whose names are linked.
pub(crate) struct Typed<'s> {
    source: &'s str,
    tokens: Tokens<'s>,
    signatures: Vec<Signature>,
    parameters: Vec<Parameter>,
    members: Vec<Member>,
    /// The ranges of the written types whose type names get links.
    linked: FxHashSet<ByteRange>,
    /// Each linked type's range, and each class's and interface's head, with the node the type
    /// names written there link from.
    owners: Vec<(ByteRange, u32)>,
}

impl<'s> Typed<'s> {
    pub(crate) fn new(source: &'s str, tokens: Tokens<'s>) -> Self {
        Self {
            source,
            tokens,
            signatures: Vec::new(),
            parameters: Vec::new(),
            members: Vec::new(),
            linked: FxHashSet::default(),
            owners: Vec::new(),
        }
    }

    /// The finished records, the linked types, and the owners of the type names in them.
    pub(crate) fn finish(self) -> (TypeFacts, FxHashSet<ByteRange>, Owners) {
        let types = TypeFacts::new(self.signatures, self.parameters, self.members);
        (types, self.linked, Owners::new(self.owners))
    }

    fn written(&self, range: ByteRange) -> Option<WrittenType> {
        let text = self.source.get(range.start as usize..range.end as usize)?;
        (!text.is_empty()).then(|| WrittenType {
            text: text.into(),
            range,
        })
    }

    fn text(&self, range: ByteRange) -> Option<Box<str>> {
        self.source
            .get(range.start as usize..range.end as usize)
            .filter(|text| !text.is_empty())
            .map(Box::from)
    }

    /// Links the written type at `range` from `owner`.
    fn link(&mut self, range: ByteRange, owner: u32) {
        self.linked.insert(range);
        self.owners.push((range, owner));
    }

    /// A written annotation's type (without its colon), linked from `owner`.
    fn linked_type(&mut self, annotation: Option<&TSTypeAnnotation<'_>>, owner: u32) -> Option<WrittenType> {
        let ty = self.written(range(annotation?.type_annotation.span()))?;
        self.link(ty.range, owner);
        Some(ty)
    }

    /// A written annotation's type (without its colon), not linked.
    fn written_type(&self, annotation: Option<&TSTypeAnnotation<'_>>) -> Option<WrittenType> {
        self.written(range(annotation?.type_annotation.span()))
    }

    /// Links a variable's, class field's, or catch clause's annotation from `owner`: the
    /// definition it types, the definition it is written in, or the `catch` part.
    pub(crate) fn annotation(&mut self, annotation: Option<&TSTypeAnnotation<'_>>, owner: u32) {
        self.linked_type(annotation, owner);
    }

    /// The signature and parameters of function `node`: an explicit `this` first, then each
    /// written parameter, then a rest parameter.
    pub(crate) fn function(&mut self, node: u32, callable: &Callable<'_, '_>) {
        let mut index = 0;
        if let Some(this) = callable.this {
            let ty = self.linked_type(this.type_annotation.as_deref(), node);
            self.parameters.push(Parameter {
                node,
                index,
                range: range(this.span),
                kind: ParameterKind::This,
                name: Some("this".into()),
                optional: false,
                default: false,
                ty,
            });
            index += 1;
        }
        for parameter in &callable.parameters.items {
            let record = self.parameter(node, index, parameter);
            self.parameters.push(record);
            index += 1;
        }
        if let Some(rest) = &callable.parameters.rest {
            let ty = self.linked_type(rest.type_annotation.as_deref(), node);
            self.parameters.push(Parameter {
                node,
                index,
                range: range(rest.span),
                kind: ParameterKind::Rest,
                name: identifier(&rest.rest.argument),
                optional: false,
                default: false,
                ty,
            });
        }
        let returns = self.linked_type(callable.returns, node);
        self.signatures.push(Signature { node, returns });
    }

    fn parameter(&mut self, node: u32, index: u32, parameter: &FormalParameter<'_>) -> Parameter {
        let (pattern, default) = match &parameter.pattern {
            BindingPattern::AssignmentPattern(it) => (&it.left, true),
            pattern => (pattern, parameter.initializer.is_some()),
        };
        let ty = self.linked_type(parameter.type_annotation.as_deref(), node);
        Parameter {
            node,
            index,
            range: range(parameter.span),
            kind: ParameterKind::Positional,
            name: identifier(pattern),
            optional: parameter.optional,
            default,
            ty,
        }
    }

    /// The members of interface `node`, whose body is linked; its head's heritage links from
    /// it too.
    pub(crate) fn interface(&mut self, node: u32, it: &TSInterfaceDeclaration<'_>) {
        let body = range(it.body.span);
        let head = ByteRange {
            start: it.span.start,
            end: body.start,
        };
        self.owners.push((head, node));
        self.link(body, node);
        for member in &it.body.body {
            self.signature_member(node, member);
        }
    }

    /// The members of type alias `node` (those of an object type literal that is its whole
    /// aliased type), whose aliased type is linked.
    pub(crate) fn type_alias(&mut self, node: u32, it: &TSTypeAliasDeclaration<'_>) {
        self.link(range(it.type_annotation.span()), node);
        if let TSType::TSTypeLiteral(literal) = &it.type_annotation {
            for member in &literal.members {
                self.signature_member(node, member);
            }
        }
    }

    /// The `value` members of enum `node`.
    pub(crate) fn enumeration(&mut self, node: u32, it: &TSEnumDeclaration<'_>) {
        for member in &it.body.members {
            let name = match &member.id {
                TSEnumMemberName::ComputedString(_) | TSEnumMemberName::ComputedTemplateString(_) => {
                    self.bracketed(member.id.span())
                }
                _ => range(member.id.span()),
            };
            let value = member
                .initializer
                .as_ref()
                .and_then(|initializer| self.text(range(initializer.span())));
            self.member(
                node,
                range(member.span),
                MemberKind::Value,
                Some(name),
                |member| {
                    member.value = value;
                },
            );
        }
    }

    /// The instance properties of class `node`, whose head (`head`, its heritage) links from it:
    /// its fields and `accessor` properties, and the parameter properties of its constructor, in
    /// written order. The lowering links each field's type from the definition it types, and
    /// the constructor's parameters' types from the constructor.
    pub(crate) fn class(&mut self, node: u32, head: ByteRange, elements: &[ClassElement<'_>]) {
        self.owners.push((head, node));
        for element in elements {
            match element {
                ClassElement::PropertyDefinition(it) if !it.r#static => {
                    let name = self.key_range(&it.key, it.computed);
                    let ty = self.written_type(it.type_annotation.as_deref());
                    self.member(node, range(it.span), MemberKind::Property, Some(name), |member| {
                        member.optional = it.optional;
                        member.readonly = it.readonly;
                        member.default = it.value.is_some();
                        member.ty = ty;
                    });
                }
                ClassElement::AccessorProperty(it) if !it.r#static => {
                    let name = self.key_range(&it.key, it.computed);
                    let ty = self.written_type(it.type_annotation.as_deref());
                    self.member(node, range(it.span), MemberKind::Property, Some(name), |member| {
                        member.default = it.value.is_some();
                        member.ty = ty;
                    });
                }
                ClassElement::MethodDefinition(it)
                    if it.kind == MethodDefinitionKind::Constructor && it.value.body.is_some() =>
                {
                    for parameter in it.value.params.items.iter().filter(|parameter| {
                        parameter.accessibility.is_some() || parameter.readonly || parameter.r#override
                    }) {
                        self.parameter_property(node, parameter);
                    }
                }
                _ => {}
            }
        }
    }

    fn parameter_property(&mut self, node: u32, parameter: &FormalParameter<'_>) {
        let (pattern, default) = match &parameter.pattern {
            BindingPattern::AssignmentPattern(it) => (&it.left, true),
            pattern => (pattern, parameter.initializer.is_some()),
        };
        let BindingPattern::BindingIdentifier(id) = pattern else {
            return;
        };
        let ty = self.written_type(parameter.type_annotation.as_deref());
        self.member(
            node,
            range(parameter.span),
            MemberKind::Property,
            Some(range(id.span)),
            |member| {
                member.optional = parameter.optional;
                member.readonly = parameter.readonly;
                member.default = default;
                member.ty = ty;
            },
        );
    }

    /// Records one member of an interface or object type literal of `node`.
    fn signature_member(&mut self, node: u32, member: &TSSignature<'_>) {
        match member {
            TSSignature::TSPropertySignature(it) => {
                let name = self.key_range(&it.key, it.computed);
                let ty = it
                    .type_annotation
                    .as_ref()
                    .and_then(|annotation| self.written(range(annotation.type_annotation.span())));
                self.member(node, range(it.span), MemberKind::Property, Some(name), |member| {
                    member.optional = it.optional;
                    member.readonly = it.readonly;
                    member.ty = ty;
                });
            }
            TSSignature::TSMethodSignature(it) => {
                let name = self.key_range(&it.key, it.computed);
                let ty = self.written(signature_range(
                    it.type_parameters.as_deref(),
                    &it.params,
                    it.return_type.as_deref(),
                ));
                self.member(node, range(it.span), MemberKind::Method, Some(name), |member| {
                    member.optional = it.optional;
                    member.ty = ty;
                });
            }
            TSSignature::TSIndexSignature(it) => {
                let parameter = it.parameter.span;
                let start = self
                    .tokens
                    .before(parameter.start)
                    .filter(|token| token.kind() == Kind::LBrack)
                    .map_or(parameter.start, |token| token.start());
                let end = self
                    .tokens
                    .find(parameter.end, Kind::RBrack)
                    .map_or(parameter.end, |token| token.end());
                let ty = self.written(range(it.type_annotation.type_annotation.span()));
                let name = ByteRange { start, end };
                self.member(node, range(it.span), MemberKind::Index, Some(name), |member| {
                    member.readonly = it.readonly;
                    member.ty = ty;
                });
            }
            TSSignature::TSCallSignatureDeclaration(it) => {
                let ty = self.written(signature_range(
                    it.type_parameters.as_deref(),
                    &it.params,
                    it.return_type.as_deref(),
                ));
                self.member(node, range(it.span), MemberKind::Call, None, |member| {
                    member.ty = ty;
                });
            }
            TSSignature::TSConstructSignatureDeclaration(it) => {
                let ty = self.written(signature_range(
                    it.type_parameters.as_deref(),
                    &it.params,
                    it.return_type.as_deref(),
                ));
                self.member(node, range(it.span), MemberKind::Construct, None, |member| {
                    member.ty = ty;
                });
            }
        }
    }

    /// Records a member of `node` written at `range` and named at `name`, filled by `fill`, at
    /// the next index.
    fn member(
        &mut self,
        node: u32,
        range: ByteRange,
        kind: MemberKind,
        name: Option<ByteRange>,
        fill: impl FnOnce(&mut Member),
    ) {
        let index = self
            .members
            .iter()
            .rev()
            .take_while(|member| member.node == node)
            .count() as u32;
        let mut member = Member {
            node,
            index,
            range,
            kind,
            name: name.and_then(|name| self.text(name)),
            optional: false,
            readonly: false,
            default: false,
            ty: None,
            value: None,
        };
        fill(&mut member);
        self.members.push(member);
    }

    /// The written range of a member or property name: a computed name with its brackets.
    fn key_range(&self, key: &PropertyKey<'_>, computed: bool) -> ByteRange {
        if computed {
            self.bracketed(key.span())
        } else {
            range(key.span())
        }
    }

    /// `inner` widened to the brackets around it (a computed name).
    fn bracketed(&self, inner: Span) -> ByteRange {
        let start = self
            .tokens
            .before(inner.start)
            .filter(|token| token.kind() == Kind::LBrack)
            .map_or(inner.start, |token| token.start());
        let end = self
            .tokens
            .find(inner.end, Kind::RBrack)
            .map_or(inner.end, |token| token.end());
        ByteRange { start, end }
    }
}

/// The node each written type name links from: the innermost recorded range that holds it, or
/// else the module. Linked types and class and interface heads lie inside one another or apart,
/// never across.
pub(crate) struct Owners {
    /// The ranges with their nodes, by start, each before the ranges inside it.
    ranges: Vec<(ByteRange, u32)>,
    /// The index of the range around each one.
    around: Vec<Option<usize>>,
}

impl Owners {
    fn new(mut ranges: Vec<(ByteRange, u32)>) -> Self {
        ranges.sort_unstable_by_key(|&(range, _)| (range.start, Reverse(range.end)));
        let mut around = Vec::with_capacity(ranges.len());
        let mut open: Vec<usize> = Vec::new();
        for (index, &(range, _)) in ranges.iter().enumerate() {
            while open.last().is_some_and(|&outer| !ranges[outer].0.contains(range)) {
                open.pop();
            }
            around.push(open.last().copied());
            open.push(index);
        }
        Self { ranges, around }
    }

    /// The node the type name written at `name` links from.
    pub(crate) fn owner(&self, name: ByteRange) -> u32 {
        let mut at = self
            .ranges
            .partition_point(|(range, _)| range.start <= name.start)
            .checked_sub(1);
        while let Some(index) = at {
            let (range, node) = self.ranges[index];
            if range.contains(name) {
                return node;
            }
            at = self.around[index];
        }
        0
    }
}

/// The identifier a binding pattern is, if it is one.
fn identifier(pattern: &BindingPattern<'_>) -> Option<Box<str>> {
    match pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str().into()),
        _ => None,
    }
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
