//! The independent census that validation compares extraction with.
//!
//! It is computed during extraction but apart from the lowering: call sites come from a separate
//! walk of oxc's AST that skips type syntax and ambient declarations. Every call, construction,
//! tagged template, dynamic `import()`, and decorator is a call site. The only thing the census
//! takes from the lowering is what it left out past the depth limit ([`CensusRecord::cut`]).
//! The record is cached in the payload; [`TypeScriptCensus`] answers validation's question from
//! the records of every TypeScript file after linking.
//!
//! The walk is also the file's depth check: syntax nested deeper than [`MAX_DEPTH`] fails the
//! file before anything recurses into it.

use oxc_ast::ast::{
    CallExpression, Class, Decorator, Expression, Function, ImportExpression, NewExpression, Program,
    Statement, TSClassImplements, TSEnumDeclaration, TSExternalModuleDeclaration, TSGlobalDeclaration,
    TSIndexSignature, TSInterfaceDeclaration, TSNamespaceDeclaration, TSType, TSTypeAliasDeclaration,
    TaggedTemplateExpression, VariableDeclaration,
};
use oxc_ast_visit::{Visit, walk};
use oxc_span::{GetSpan, Span};
use oxc_syntax::scope::ScopeFlags;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use unfold_core::RepoPath;
use unfold_core::text::ByteRange;
use unfold_core::validate::{CallSite, Census};
use unfold_core::vocab::{Form, Language};

use crate::lower::decorator_is_call;
use crate::spans::range;

/// The deepest syntax nesting extracted. Real code stays far below it; deeper files fail.
pub(crate) const MAX_DEPTH: u32 = 4_000;

/// The syntax nests deeper than [`MAX_DEPTH`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TooDeep;

/// One file's census.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CensusRecord {
    /// Every call, in source order.
    call_sites: Vec<CallSiteRecord>,
}

/// One call site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CallSiteRecord {
    range: ByteRange,
    /// A `new` expression, whose node has the form `new`.
    construction: bool,
}

impl CensusRecord {
    /// Takes the census of one parsed file.
    pub(crate) fn take(program: &Program<'_>) -> Result<Self, TooDeep> {
        let mut walker = Walker {
            depth: 0,
            too_deep: false,
            call_sites: Vec::new(),
        };
        walker.visit_program(program);
        if walker.too_deep {
            return Err(TooDeep);
        }
        Ok(Self {
            call_sites: walker.call_sites,
        })
    }

    /// Leaves out the call sites inside `cuts` (sorted, disjoint ranges): the
    /// syntax the lowering left out past its depth limit.
    pub(crate) fn cut(&mut self, cuts: &[ByteRange]) {
        if cuts.is_empty() {
            return;
        }
        let inside = |range: ByteRange| {
            let index = cuts.partition_point(|cut| cut.start <= range.start);
            index
                .checked_sub(1)
                .is_some_and(|index| cuts[index].contains(range))
        };
        self.call_sites.retain(|site| !inside(site.range));
    }
}

struct Walker {
    depth: u32,
    too_deep: bool,
    call_sites: Vec<CallSiteRecord>,
}

impl Walker {
    fn site(&mut self, span: Span, construction: bool) {
        self.call_sites.push(CallSiteRecord {
            range: range(span),
            construction,
        });
    }

    /// Enters one level of nesting; `false` once the depth limit is reached.
    fn enter(&mut self) -> bool {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.too_deep = true;
        }
        !self.too_deep
    }
}

impl<'a> Visit<'a> for Walker {
    fn visit_expression(&mut self, it: &Expression<'a>) {
        if self.enter() {
            walk::walk_expression(self, it);
        }
        self.depth -= 1;
    }

    fn visit_statement(&mut self, it: &Statement<'a>) {
        if self.enter() {
            walk::walk_statement(self, it);
        }
        self.depth -= 1;
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        self.site(it.span, false);
        walk::walk_call_expression(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        self.site(it.span, true);
        walk::walk_new_expression(self, it);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        self.site(it.span, false);
        walk::walk_tagged_template_expression(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        self.site(it.span, false);
        walk::walk_import_expression(self, it);
    }

    fn visit_decorator(&mut self, it: &Decorator<'a>) {
        if !decorator_is_call(&it.expression) {
            self.site(it.expression.span(), false);
        }
        walk::walk_decorator(self, it);
    }

    // Type syntax has no runtime calls.
    fn visit_ts_type(&mut self, _it: &TSType<'a>) {}

    fn visit_ts_interface_declaration(&mut self, _it: &TSInterfaceDeclaration<'a>) {}

    fn visit_ts_type_alias_declaration(&mut self, _it: &TSTypeAliasDeclaration<'a>) {}

    fn visit_ts_index_signature(&mut self, _it: &TSIndexSignature<'a>) {}

    fn visit_ts_class_implements(&mut self, _it: &TSClassImplements<'a>) {}

    // Neither do ambient declarations.
    fn visit_ts_global_declaration(&mut self, _it: &TSGlobalDeclaration<'a>) {}

    fn visit_ts_external_module_declaration(&mut self, _it: &TSExternalModuleDeclaration<'a>) {}

    fn visit_ts_namespace_declaration(&mut self, it: &TSNamespaceDeclaration<'a>) {
        if !it.declare {
            walk::walk_ts_namespace_declaration(self, it);
        }
    }

    fn visit_ts_enum_declaration(&mut self, it: &TSEnumDeclaration<'a>) {
        if !it.declare {
            walk::walk_ts_enum_declaration(self, it);
        }
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        if !it.declare {
            walk::walk_class(self, it);
        }
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        if it.body.is_some() {
            walk::walk_function(self, it, flags);
        }
    }

    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        if !it.declare {
            walk::walk_variable_declaration(self, it);
        }
    }
}

/// The census of every TypeScript file of a linked program.
#[derive(Debug, Default)]
pub struct TypeScriptCensus {
    files: FxHashMap<RepoPath, Vec<CallSite>>,
}

impl TypeScriptCensus {
    /// A census over `files`.
    pub fn new(files: impl IntoIterator<Item = (RepoPath, CensusRecord)>) -> Self {
        let files = files
            .into_iter()
            .map(|(path, record)| {
                let call_sites = record
                    .call_sites
                    .iter()
                    .map(|site| CallSite {
                        range: site.range,
                        form: Some(if site.construction { Form::New } else { Form::Call }),
                    })
                    .collect();
                (path, call_sites)
            })
            .collect();
        Self { files }
    }
}

impl Census for TypeScriptCensus {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn call_sites(&self, path: &RepoPath) -> Option<&[CallSite]> {
        self.files.get(path).map(Vec::as_slice)
    }
}
