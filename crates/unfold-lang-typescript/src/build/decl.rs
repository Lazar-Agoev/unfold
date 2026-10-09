//! Declarations and module syntax: TypeScript declarations (enums, namespaces, import aliases),
//! imports, re-exports, and exports.

use oxc_ast::ast::{
    Declaration, ExportDefaultDeclarationKind, ModuleDeclaration, StringLiteral, TSModuleReference,
    TSNamespaceDeclaration, TSNamespaceDeclarationBody,
};
use oxc_span::GetSpan;
use unfold_core::text::ByteRange;

use super::{Builder, Ctx, Head, range};
use crate::imports;
use crate::syntax::{Cat, Flags, Label};

impl<'a> Builder<'a, '_> {
    /// A declaration; `export_start` is the offset of a preceding `export` keyword.
    pub(super) fn declaration(
        &mut self,
        parent: u32,
        label: Label,
        declaration: &Declaration<'a>,
        export_start: Option<u32>,
        ctx: Ctx,
    ) {
        match declaration {
            Declaration::VariableDeclaration(list) => {
                self.variable_list(parent, label, list, export_start, ctx);
            }
            Declaration::FunctionDeclaration(function) => {
                let head = Head::new(Cat::FunctionDeclaration).exported(export_start);
                self.function(parent, label, function, head, ctx);
            }
            Declaration::ClassDeclaration(class) => {
                let head = Head::new(Cat::ClassDeclaration).exported(export_start);
                self.class(parent, label, class, head, ctx);
            }
            Declaration::TSTypeAliasDeclaration(it) => {
                self.push(parent, label, Cat::Type, range(it.span), ctx.flags());
            }
            Declaration::TSInterfaceDeclaration(it) => {
                self.push(parent, label, Cat::Type, range(it.span), ctx.flags());
            }
            Declaration::TSEnumDeclaration(it) => {
                let ctx = ctx.ambient(it.declare);
                let node = self.push(parent, label, Cat::Other, range(it.span), ctx.flags());
                for member in &it.body.members {
                    let item = self.push(node, Label::Other, Cat::Other, range(member.span), ctx.flags());
                    if let Some(initializer) = &member.initializer {
                        self.expression(item, Label::Initializer, initializer, ctx);
                    }
                }
            }
            Declaration::TSExternalModuleDeclaration(it) => {
                let ctx = ctx.ambient(it.declare);
                let node = self.push(parent, label, Cat::Other, range(it.span), ctx.flags());
                if let Some(body) = &it.body {
                    let block = self.push(node, Label::Body, Cat::Other, range(body.span), ctx.flags());
                    self.directives(block, &body.directives);
                    self.statements(block, &body.body, ctx);
                }
            }
            Declaration::TSNamespaceDeclaration(it) => self.namespace(parent, label, it, ctx),
            Declaration::TSGlobalDeclaration(it) => {
                let ctx = ctx.ambient(it.declare);
                let node = self.push(parent, label, Cat::Other, range(it.span), ctx.flags());
                let block = self.push(node, Label::Body, Cat::Other, range(it.body.span), ctx.flags());
                self.directives(block, &it.body.directives);
                self.statements(block, &it.body.body, ctx);
            }
            Declaration::TSImportEqualsDeclaration(it) => {
                let span = range(it.span);
                match &it.module_reference {
                    TSModuleReference::ExternalModuleReference(reference) => {
                        let flags = ctx.flags().with(Flags::RUNTIME, imports::import_equals(it));
                        let node = self.push(parent, label, Cat::ImportEquals, span, flags);
                        self.specifier(node, &reference.expression);
                    }
                    _ => {
                        self.push(parent, label, Cat::Other, span, ctx.flags());
                    }
                }
            }
        }
    }

    fn namespace(&mut self, parent: u32, label: Label, it: &TSNamespaceDeclaration<'a>, ctx: Ctx) {
        let ctx = ctx.ambient(it.declare);
        let node = self.push(parent, label, Cat::Other, range(it.span), ctx.flags());
        match &it.body {
            TSNamespaceDeclarationBody::TSNamespaceDeclaration(inner) => {
                if self.enter() {
                    self.namespace(node, Label::Body, inner, ctx);
                }
                self.leave();
            }
            TSNamespaceDeclarationBody::TSModuleBlock(body) => {
                let block = self.push(node, Label::Body, Cat::Other, range(body.span), ctx.flags());
                self.directives(block, &body.directives);
                self.statements(block, &body.body, ctx);
            }
        }
    }

    pub(super) fn module_declaration(
        &mut self,
        parent: u32,
        label: Label,
        module: &ModuleDeclaration<'a>,
        ctx: Ctx,
    ) {
        let span = range(module.span());
        let verbatim = self.options.verbatim_module_syntax;
        match module {
            ModuleDeclaration::ImportDeclaration(it) => {
                let runtime = imports::import_declaration(it, verbatim);
                let node = self.module_node(parent, label, Cat::ImportDeclaration, span, runtime, ctx);
                self.specifier(node, &it.source);
            }
            ModuleDeclaration::ExportAllDeclaration(it) => {
                let node = self.module_node(parent, label, Cat::Reexport, span, imports::export_all(it), ctx);
                self.specifier(node, &it.source);
            }
            ModuleDeclaration::ExportFromDeclaration(it) => {
                let runtime = imports::export_from(it, verbatim);
                let node = self.module_node(parent, label, Cat::Reexport, span, runtime, ctx);
                self.specifier(node, &it.source);
            }
            ModuleDeclaration::ExportDeclaration(it) => {
                self.declaration(parent, label, &it.declaration, Some(it.span.start), ctx);
            }
            ModuleDeclaration::ExportDefaultDeclaration(it) => {
                let start = Some(it.span.start);
                match &it.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                        let head = Head::new(Cat::FunctionDeclaration)
                            .exported(start)
                            .default_export();
                        self.function(parent, label, function, head, ctx);
                    }
                    ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                        let head = Head::new(Cat::ClassDeclaration).exported(start).default_export();
                        self.class(parent, label, class, head, ctx);
                    }
                    ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                        self.push(parent, label, Cat::Type, range(interface.span), ctx.flags());
                    }
                    _ => {
                        if let Some(expression) = it.declaration.as_expression() {
                            let flags = ctx.flags().with(Flags::DEFAULT, true);
                            let node = self.push(parent, label, Cat::ExportAssignment, span, flags);
                            self.expression(node, Label::Expression, expression, ctx);
                        }
                    }
                }
            }
            ModuleDeclaration::TSExportAssignment(it) => {
                let node = self.push(parent, label, Cat::ExportAssignment, span, ctx.flags());
                self.expression(node, Label::Expression, &it.expression, ctx);
            }
            ModuleDeclaration::ExportNamedDeclaration(_)
            | ModuleDeclaration::TSNamespaceExportDeclaration(_) => {
                self.push(parent, label, Cat::Other, span, ctx.flags());
            }
        }
    }

    fn module_node(
        &mut self,
        parent: u32,
        label: Label,
        cat: Cat,
        span: ByteRange,
        runtime: bool,
        ctx: Ctx,
    ) -> u32 {
        let flags = ctx.flags().with(Flags::RUNTIME, runtime);
        self.push(parent, label, cat, span, flags)
    }

    fn specifier(&mut self, parent: u32, source: &StringLiteral<'a>) {
        self.push_text(
            parent,
            Label::ModuleSpecifier,
            Cat::StringLiteral,
            source.span,
            source.value.as_str(),
        );
    }
}
