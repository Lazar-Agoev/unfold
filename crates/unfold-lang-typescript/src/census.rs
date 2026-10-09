//! The independent census that validation compares extraction with.
//!
//! It is computed during extraction but apart from block selection: token boundaries come from
//! oxc's token stream, and call sites and module dependencies come from a separate walk of oxc's
//! AST that skips type syntax (as the proof of concept's `callSites()` and
//! `moduleDependencies()`). Dynamic `import()` is a module dependency, never a call site. The
//! record is cached in the payload; [`TypeScriptCensus`] answers validation's questions from the
//! records of every TypeScript file after linking.

use oxc_ast::ast::{
    Argument, CallExpression, ExportAllDeclaration, ExportFromDeclaration, Expression, ImportDeclaration,
    ImportExpression, NewExpression, Program, Statement, TSClassImplements, TSImportEqualsDeclaration,
    TSIndexSignature, TSInterfaceDeclaration, TSType, TSTypeAliasDeclaration, TaggedTemplateExpression,
};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::{Kind, Token};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use unfold_core::RepoPath;
use unfold_core::text::ByteRange;
use unfold_core::validate::{CallSite, Census};
use unfold_core::vocab::{BlockKind, EdgeKind, Language};

use crate::build::{MAX_DEPTH, TooDeep, range};
use crate::imports;

/// One file's census.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CensusRecord {
    /// A bitset of the byte offsets where a token starts.
    starts: Vec<u8>,
    /// A bitset of the byte offsets where a token ends.
    ends: Vec<u8>,
    /// Every call, tagged template, and construction, in source order.
    call_sites: Vec<CallSiteRecord>,
    /// The range of every runtime module dependency, in source order.
    dependencies: Vec<ByteRange>,
}

/// One call site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSiteRecord {
    range: ByteRange,
    /// A `new` expression (a `construct` block with a `constructs` edge).
    construction: bool,
    /// The positions of spread arguments, which can never supply a named reference.
    spreads: Vec<u32>,
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
    fn site(&mut self, span: oxc_span::Span, construction: bool, arguments: &[Argument<'_>]) {
        let spreads = arguments
            .iter()
            .enumerate()
            .filter(|(_, argument)| matches!(argument, Argument::SpreadElement(_)))
            .map(|(position, _)| position as u32)
            .collect();
        self.call_sites.push(CallSiteRecord {
            range: range(span),
            construction,
            spreads,
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
        self.site(it.span, false, &it.arguments);
        walk::walk_call_expression(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        self.site(it.span, true, &it.arguments);
        walk::walk_new_expression(self, it);
    }

    fn visit_tagged_template_expression(&mut self, it: &TaggedTemplateExpression<'a>) {
        self.site(it.span, false, &[]);
        walk::walk_tagged_template_expression(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        self.dependencies.push(range(it.span));
        walk::walk_import_expression(self, it);
    }

    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        if imports::import_declaration(it, self.verbatim) {
            self.dependencies.push(range(it.span));
        }
    }

    fn visit_export_from_declaration(&mut self, it: &ExportFromDeclaration<'a>) {
        if imports::export_from(it, self.verbatim) {
            self.dependencies.push(range(it.span));
        }
    }

    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        if imports::export_all(it) {
            self.dependencies.push(range(it.span));
        }
    }

    fn visit_ts_import_equals_declaration(&mut self, it: &TSImportEqualsDeclaration<'a>) {
        if imports::import_equals(it) {
            self.dependencies.push(range(it.span));
        }
    }

    // Type syntax has no runtime calls or dependencies.
    fn visit_ts_type(&mut self, _it: &TSType<'a>) {}

    fn visit_ts_interface_declaration(&mut self, _it: &TSInterfaceDeclaration<'a>) {}

    fn visit_ts_type_alias_declaration(&mut self, _it: &TSTypeAliasDeclaration<'a>) {}

    fn visit_ts_index_signature(&mut self, _it: &TSIndexSignature<'a>) {}

    fn visit_ts_class_implements(&mut self, _it: &TSClassImplements<'a>) {}
}

/// The census of every TypeScript file of a linked program.
#[derive(Debug, Default)]
pub struct TypeScriptCensus {
    files: FxHashMap<RepoPath, CensusRecord>,
}

impl TypeScriptCensus {
    /// A census over `files`. A file without a record (for example, one whose cached payload
    /// could not be decoded) has no token boundaries, so validation reports its blocks.
    pub fn new(files: impl IntoIterator<Item = (RepoPath, CensusRecord)>) -> Self {
        Self {
            files: files.into_iter().collect(),
        }
    }
}

impl Census for TypeScriptCensus {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn is_token_range(&self, path: &RepoPath, range: ByteRange) -> bool {
        self.files
            .get(path)
            .is_some_and(|record| record.is_token_range(range))
    }

    fn call_sites(&self, path: &RepoPath) -> Option<Vec<CallSite>> {
        let record = self.files.get(path)?;
        Some(
            record
                .call_sites
                .iter()
                .map(|site| CallSite {
                    range: site.range,
                    block_kind: if site.construction {
                        BlockKind::New
                    } else {
                        BlockKind::Call
                    },
                    edge_kind: Some(if site.construction {
                        EdgeKind::Constructs
                    } else {
                        EdgeKind::Calls
                    }),
                    ineligible_references: site.spreads.clone(),
                })
                .collect(),
        )
    }

    fn module_dependencies(&self, path: &RepoPath) -> Option<Vec<ByteRange>> {
        Some(self.files.get(path)?.dependencies.clone())
    }
}
