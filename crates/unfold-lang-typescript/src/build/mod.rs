//! Converts oxc's AST into the normalized [`Syntax`] tree: statements in [`stmt`], declarations
//! and module syntax in [`decl`], expressions and patterns in [`expr`], callables and classes in
//! [`class`], and type declarations and the written members of types and classes in [`types`].
//!
//! The conversion follows the proof of concept's normalized TypeScript syntax: which nodes
//! exist, the child slot each one fills, and their token-exact ranges. Where oxc's spans differ
//! (a declaration under `export` starts at `export`, a computed name includes its brackets, a
//! heritage clause starts at `extends`), the range is rebuilt from the token stream.
//!
//! Conversion recurses over the AST with a depth guard: a file nested deeper than
//! [`MAX_DEPTH`] stops converting and is reported as a failure instead of overflowing the stack.

mod class;
mod decl;
mod expr;
mod stmt;
mod types;

use oxc_allocator::Allocator;
use oxc_ast::ast::{Directive, Program};
use oxc_parser::{Kind, Token};
use oxc_span::{GetSpan, Span};
use unfold_core::text::ByteRange;

use crate::syntax::{Cat, Flags, Label, NONE, Syntax};

/// The deepest syntax nesting converted. Real code stays far below it; deeper files fail.
pub(crate) const MAX_DEPTH: u32 = 4_000;

/// Which declaration binds the identifiers of a binding pattern being converted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Binding {
    None,
    Variable,
    Parameter,
}

/// What a subtree inherits from its ancestors.
#[derive(Debug, Clone, Copy)]
struct Ctx {
    /// Inside an ambient (`declare`) declaration.
    ambient: bool,
    /// The nearest enclosing variable declaration or parameter.
    binding: Binding,
}

impl Ctx {
    const ROOT: Self = Self {
        ambient: false,
        binding: Binding::None,
    };

    fn ambient(self, declare: bool) -> Self {
        Self {
            ambient: self.ambient || declare,
            ..self
        }
    }

    fn binding(self, binding: Binding) -> Self {
        Self { binding, ..self }
    }

    fn flags(self) -> Flags {
        Flags::default().with(Flags::AMBIENT, self.ambient)
    }
}

/// How a function, class, or type declaration is written: its category, the offset of a
/// preceding `export` keyword (where its range starts), and whether it is `export default`.
#[derive(Debug, Clone, Copy)]
struct Head {
    cat: Cat,
    export_start: Option<u32>,
    default: bool,
}

impl Head {
    fn new(cat: Cat) -> Self {
        Self {
            cat,
            export_start: None,
            default: false,
        }
    }

    fn exported(self, start: Option<u32>) -> Self {
        Self {
            export_start: start,
            ..self
        }
    }

    fn default_export(self) -> Self {
        Self {
            default: true,
            ..self
        }
    }
}

/// Options that shape which syntax is a runtime dependency.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BuildOptions {
    /// TypeScript's `verbatimModuleSyntax`: keep named imports and re-exports whose
    /// specifiers are all type-only (or empty) as runtime dependencies.
    pub(crate) verbatim_module_syntax: bool,
}

/// The converter's state for one file.
pub(crate) struct Builder<'a, 't> {
    allocator: &'a Allocator,
    tokens: &'t [Token],
    options: BuildOptions,
    syntax: Syntax<'a>,
    depth: u32,
    too_deep: bool,
}

/// The conversion stopped because the syntax nests deeper than [`MAX_DEPTH`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TooDeep;

/// Converts one parsed program.
pub(crate) fn build<'a>(
    allocator: &'a Allocator,
    program: &Program<'a>,
    tokens: &[Token],
    options: BuildOptions,
) -> Result<Syntax<'a>, TooDeep> {
    let mut builder = Builder {
        allocator,
        tokens,
        options,
        syntax: Syntax::with_capacity(tokens.len()),
        depth: 0,
        too_deep: false,
    };
    let root = builder.syntax.push(
        NONE,
        Label::Root,
        Cat::SourceFile,
        range(program.span),
        Flags::default(),
        "",
    );
    builder.directives(root, &program.directives);
    for statement in &program.body {
        builder.statement(root, Label::Statements, statement, Ctx::ROOT);
    }
    if builder.too_deep {
        return Err(TooDeep);
    }
    Ok(builder.syntax)
}

/// A span as a byte range.
pub(crate) fn range(span: Span) -> ByteRange {
    ByteRange {
        start: span.start,
        end: span.end,
    }
}

impl<'a> Builder<'a, '_> {
    fn push(&mut self, parent: u32, label: Label, cat: Cat, range: ByteRange, flags: Flags) -> u32 {
        self.syntax.push(parent, label, cat, range, flags, "")
    }

    fn push_text(&mut self, parent: u32, label: Label, cat: Cat, span: Span, text: &'a str) -> u32 {
        self.syntax
            .push(parent, label, cat, range(span), Flags::default(), text)
    }

    /// Enters one level of nesting; `false` once the depth limit is reached.
    fn enter(&mut self) -> bool {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.too_deep = true;
        }
        !self.too_deep
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// The last token ending at or before `offset`.
    fn token_before(&self, offset: u32) -> Option<&Token> {
        let index = self.tokens.partition_point(|token| token.end() <= offset);
        index.checked_sub(1).and_then(|index| self.tokens.get(index))
    }

    /// The first token of `kind` starting at or after `offset`.
    fn token_from(&self, offset: u32, kind: Kind) -> Option<&Token> {
        let index = self.tokens.partition_point(|token| token.start() < offset);
        self.tokens[index..].iter().find(|token| token.kind() == kind)
    }

    /// Directive prologues (`"use strict";`) are expression statements in the normalized tree.
    fn directives(&mut self, parent: u32, directives: &[Directive<'a>]) {
        for directive in directives {
            let statement = self.push(
                parent,
                Label::Statements,
                Cat::ExpressionStatement,
                range(directive.span),
                Flags::default(),
            );
            let literal = &directive.expression;
            self.push_text(
                statement,
                Label::Expression,
                Cat::StringLiteral,
                literal.span,
                literal.value.as_str(),
            );
        }
    }

    /// A type annotation: one childless type node over the type itself (without the colon).
    fn type_annotation(
        &mut self,
        parent: u32,
        label: Label,
        annotation: &oxc_ast::ast::TSTypeAnnotation<'a>,
    ) {
        let span = annotation.type_annotation.span();
        self.push(parent, label, Cat::Type, range(span), Flags::default());
    }
}
