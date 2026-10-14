//! The independent census that validation compares extraction with.
//!
//! It is computed during extraction but apart from the lowering: token boundaries come from
//! oxc's token stream, and call sites and module dependencies come from a separate walk of oxc's
//! AST that skips type syntax and ambient declarations. Every call, construction, tagged
//! template, dynamic `import()`, and decorator is a call site; a dynamic `import()` is a module
//! dependency too, at its specifier, like every runtime import. The only thing the census takes
//! from the lowering is what it left out past the depth limit ([`CensusRecord::cut`]). The
//! record is cached in the payload; [`TypeScriptCensus`] answers validation's questions from the
//! records of every TypeScript file after linking.
//!
//! The walk is also the file's depth check: syntax nested deeper than [`MAX_DEPTH`] fails the
//! file before anything recurses into it.

use oxc_ast::ast::{
    CallExpression, Class, Decorator, ExportAllDeclaration, ExportFromDeclaration, Expression, Function,
    ImportDeclaration, ImportExpression, NewExpression, Program, Statement, TSClassImplements,
    TSEnumDeclaration, TSExternalModuleDeclaration, TSGlobalDeclaration, TSImportEqualsDeclaration,
    TSIndexSignature, TSInterfaceDeclaration, TSModuleReference, TSNamespaceDeclaration, TSType,
    TSTypeAliasDeclaration, TaggedTemplateExpression, VariableDeclaration,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::{Kind, Token};
use oxc_span::{GetSpan, Span};
use oxc_syntax::scope::ScopeFlags;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use unfold_core::RepoPath;
use unfold_core::text::ByteRange;
use unfold_core::validate::{CallSite, Census};
use unfold_core::vocab::{Form, Language};

use crate::imports;
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
    /// A bitset of the byte offsets where a token starts.
    starts: Vec<u8>,
    /// A bitset of the byte offsets where a token ends.
    ends: Vec<u8>,
    /// Every call, in source order.
    call_sites: Vec<CallSiteRecord>,
    /// The range of every runtime module dependency's specifier, in source order.
    dependencies: Vec<ByteRange>,
}

/// One call site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CallSiteRecord {
    range: ByteRange,
    /// A `new` expression, whose node has the form `new`.
    construction: bool,
}

impl CensusRecord {
    /// Takes the census of one parsed file of `len` bytes.
    pub(crate) fn take(
        program: &Program<'_>,
        tokens: &[Token],
        len: u32,
        verbatim: bool,
    ) -> Result<Self, TooDeep> {
        let mut starts = Bits::new(len);
        let mut ends = Bits::new(len);
        for token in tokens.iter().filter(|token| token.kind() != Kind::Eof) {
            if token.start() < token.end() {
                starts.set(token.start());
                ends.set(token.end());
            }
        }
        let mut walker = Walker {
            verbatim,
            depth: 0,
            too_deep: false,
            call_sites: Vec::new(),
            dependencies: Vec::new(),
        };
        walker.visit_program(program);
        if walker.too_deep {
            return Err(TooDeep);
        }
        Ok(Self {
            starts: starts.0,
            ends: ends.0,
            call_sites: walker.call_sites,
            dependencies: walker.dependencies,
        })
    }

    /// Leaves out the call sites and dependencies inside `cuts` (sorted, disjoint ranges): the
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
        self.dependencies.retain(|&dependency| !inside(dependency));
    }

    /// Whether `range` starts at a token's first byte and ends right after a token's last byte.
    pub fn is_token_range(&self, range: ByteRange) -> bool {
        Bits::get(&self.starts, range.start) && Bits::get(&self.ends, range.end)
    }
}

/// A bitset over byte offsets `0..=len`.
struct Bits(Vec<u8>);

impl Bits {
    fn new(len: u32) -> Self {
        Self(vec![0; len as usize / 8 + 1])
    }

    fn set(&mut self, offset: u32) {
        if let Some(byte) = self.0.get_mut(offset as usize / 8) {
            *byte |= 1 << (offset % 8);
        }
    }

    fn get(bits: &[u8], offset: u32) -> bool {
        bits.get(offset as usize / 8)
            .is_some_and(|byte| byte & (1 << (offset % 8)) != 0)
    }
}

struct Walker {
    verbatim: bool,
    depth: u32,
    too_deep: bool,
    call_sites: Vec<CallSiteRecord>,
    dependencies: Vec<ByteRange>,
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
        self.dependencies.push(range(it.source.span()));
        walk::walk_import_expression(self, it);
    }

    fn visit_decorator(&mut self, it: &Decorator<'a>) {
        if !decorator_is_call(&it.expression) {
            self.site(it.expression.span(), false);
        }
        walk::walk_decorator(self, it);
    }

    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        if imports::import_declaration(it, self.verbatim) {
            self.dependencies.push(range(it.source.span));
        }
    }

    fn visit_export_from_declaration(&mut self, it: &ExportFromDeclaration<'a>) {
        if imports::export_from(it, self.verbatim) {
            self.dependencies.push(range(it.source.span));
        }
    }

    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        if imports::export_all(it) {
            self.dependencies.push(range(it.source.span));
        }
    }

    fn visit_ts_import_equals_declaration(&mut self, it: &TSImportEqualsDeclaration<'a>) {
        if imports::import_equals(it)
            && let TSModuleReference::ExternalModuleReference(reference) = &it.module_reference
        {
            self.dependencies.push(range(reference.expression.span));
        }
    }

    // Type syntax has no runtime calls or dependencies.
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

/// One file's census, as validation reads it.
#[derive(Debug, Default)]
struct CensusFile {
    record: CensusRecord,
    call_sites: Vec<CallSite>,
}

/// The census of every TypeScript file of a linked program.
#[derive(Debug, Default)]
pub struct TypeScriptCensus {
    files: FxHashMap<RepoPath, CensusFile>,
}

impl TypeScriptCensus {
    /// A census over `files`. A file without a record (for example, one whose cached payload
    /// could not be decoded) has no token boundaries, so validation reports its nodes.
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
                (path, CensusFile { record, call_sites })
            })
            .collect();
        Self { files }
    }
}

impl Census for TypeScriptCensus {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn is_token_range(&self, path: &RepoPath, range: ByteRange) -> bool {
        self.files
            .get(path)
            .is_some_and(|file| file.record.is_token_range(range))
    }

    fn call_sites(&self, path: &RepoPath) -> Option<&[CallSite]> {
        Some(&self.files.get(path)?.call_sites)
    }

    fn module_dependencies(&self, path: &RepoPath) -> Option<&[ByteRange]> {
        Some(&self.files.get(path)?.record.dependencies)
    }
}
