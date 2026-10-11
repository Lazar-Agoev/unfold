//! Extraction of one Python file: parse, the pre-pass ([`crate::analysis`]), block selection
//! through the engine's shared selector, the value table and occurrences, the typed facts
//! ([`crate::typed`]), and the payload.

use ruff_python_ast::{Expr, Stmt};
use ruff_text_size::TextRange;
use rustc_hash::FxHashMap;
use unfold_core::Snapshot;
use unfold_engine::SourceFailure;
use unfold_engine::facts::{FileFacts, GraphFacts, ImportFact, TypeFacts};
use unfold_engine::select;
use unfold_engine::settings::PythonVersion;

use crate::analysis::{self, Analysis};
use crate::bind::ir::BindFacts;
use crate::bind::{self, alias_address};
use crate::census;
use crate::mapping::bytes;
use crate::parse::{self, Module, failure};
use crate::payload::{CensusData, ImportSyntax, PAYLOAD_VERSION, Payload};
use crate::syntax::{Node, Syntax};
use crate::tokens::Tokens;
use crate::typed::Typed;
use crate::values::{Values, address};

/// Parses `file` with grammar `version` and extracts its facts. Never panics: every problem is
/// a [`SourceFailure`].
pub fn extract(file: &Snapshot, version: PythonVersion) -> Result<FileFacts, SourceFailure> {
    let text = file.text();
    parse::check_encoding(text)?;
    let module = parse::parse_module(text, version)
        .ok_or_else(|| failure("the parser did not produce a module", None))?;
    let syntax_error = parse::syntax_error(&module);
    let analysis = analysis::analyze(module.syntax(), version >= PythonVersion::V3_14);
    let result = match (syntax_error, &analysis.failure) {
        (Some(problem), _) => Err(problem),
        (None, Some(problem)) => Err(problem.clone()),
        (None, None) => facts(file, &module, &analysis),
    };
    parse::release(module, analysis.depth);
    result
}

/// The facts of a file that parsed and passed the pre-pass.
fn facts(file: &Snapshot, module: &Module, analysis: &Analysis) -> Result<FileFacts, SourceFailure> {
    let text = file.text();
    let syntax = Syntax {
        text,
        module: module.syntax(),
        tokens: Tokens::new(module.tokens()),
        proofs: &analysis.proofs,
    };
    let lines = file.line_index();
    let mut occurrences: Vec<(Node<'_>, u32)> = Vec::new();
    let mut blocks: FxHashMap<usize, u32> = FxHashMap::default();
    let mut typed = Typed::new(&syntax);
    let selection = select::walk(&syntax, file.path(), &lines, Node::Module, |node, local| {
        typed.selected(node, local);
        match node {
            Node::Stmt(stmt @ (Stmt::FunctionDef(_) | Stmt::ClassDef(_))) => {
                blocks.insert(std::ptr::from_ref(stmt) as usize, local);
            }
            Node::Expr(expr @ Expr::Lambda(_), _) => {
                blocks.insert(address(expr), local);
                occurrences.push((node, local));
            }
            Node::Expr(Expr::Call(_), _)
            | Node::Alias(_)
            | Node::Child(_)
            | Node::Stmt(Stmt::ImportFrom(_)) => {
                occurrences.push((node, local));
            }
            _ => {}
        }
    })
    .map_err(|error| failure(format!("cannot select blocks: {error}"), None))?;

    let mut values = Values::new(&syntax, &blocks);
    let mut calls = Vec::new();
    // Each call block's call, by address, for its result.
    let mut call_blocks: Vec<(usize, u32)> = Vec::new();
    let mut callees: FxHashMap<usize, u32> = FxHashMap::default();
    let mut imports = Vec::new();
    let mut written = Vec::new();
    let mut conditional = Vec::new();
    // Each child candidate's entry in `written`, with its alias, for the binder's declaration.
    let mut children: Vec<(usize, usize)> = Vec::new();
    // The import index and normalized specifier of the latest `from` import, which its child
    // candidates (its next blocks in preorder) extend.
    let mut from: Option<(u32, String)> = None;
    for &(node, block) in &occurrences {
        match node {
            Node::Expr(expr @ Expr::Call(call), _) => match analysis.proofs.dynamic_import(call) {
                Some(name) => {
                    imports.push(ImportFact {
                        block,
                        specifier: Some(name.into()),
                    });
                    written.push(ImportSyntax::Dynamic);
                }
                None => {
                    let fact = values.call(call, block);
                    if let Some(callee) = fact.callee {
                        callees.insert(address(expr), callee);
                    }
                    call_blocks.push((address(expr), block));
                    calls.push(fact);
                }
            },
            Node::Alias(alias) => {
                imports.push(ImportFact {
                    block,
                    specifier: Some(dotted(syntax.slice(alias.name.range)).into()),
                });
                written.push(ImportSyntax::Module {
                    module: alias.name.id.as_str().into(),
                    alias: alias.asname.as_ref().map(|name| name.id.as_str().into()),
                });
            }
            Node::Child(alias) => {
                let (index, parent) = from.as_ref().map_or((0, ""), |(index, parent)| (*index, parent));
                let member = syntax.slice(alias.name.range);
                let separator = if parent.ends_with('.') { "" } else { "." };
                imports.push(ImportFact {
                    block,
                    specifier: Some(format!("{parent}{separator}{member}").into()),
                });
                conditional.push(block);
                children.push((written.len(), alias_address(alias)));
                written.push(ImportSyntax::Child {
                    from: index,
                    name: alias.name.id.as_str().into(),
                    // Filled in once the binder has declared the name.
                    node: 0,
                });
            }
            Node::Stmt(Stmt::ImportFrom(import)) => {
                let specifier = syntax
                    .module_range(import)
                    .map_or("", |range| syntax.slice(range));
                let specifier = dotted(specifier);
                imports.push(ImportFact {
                    block,
                    specifier: Some(specifier.as_str().into()),
                });
                from = Some((imports.len() as u32 - 1, specifier));
                written.push(ImportSyntax::From {
                    level: import.level,
                    module: import.module.as_ref().map(|name| name.id.as_str().into()),
                    names: import
                        .names
                        .iter()
                        .map(|alias| {
                            let name: Box<str> = alias.name.id.as_str().into();
                            (
                                name,
                                alias.asname.as_ref().map(|asname| asname.id.as_str().into()),
                            )
                        })
                        .collect(),
                });
            }
            _ => {}
        }
    }
    let (values, exprs) = values.finish();
    let inputs = bind::Inputs {
        text,
        blocks: &blocks,
        values: &exprs,
        callees: &callees,
    };
    let built = bind::build(module.syntax(), &inputs);
    let (signatures, parameters, members, results) = typed.finish(&call_blocks, &built.awaited);
    for (index, alias) in children {
        if let Some(ImportSyntax::Child { node, .. }) = written.get_mut(index) {
            *node = built.aliases.get(&alias).copied().unwrap_or_default();
        }
    }
    let payload = payload(text, analysis, written, built.facts)
        .to_bytes()
        .map_err(|error| failure(error.to_string(), None))?;
    let graph = GraphFacts::new(selection, values, calls, imports)
        .and_then(|graph| graph.with_conditional(conditional))
        .and_then(|graph| {
            let types = TypeFacts::new(
                graph.blocks(),
                signatures,
                parameters,
                members,
                results,
                built.type_refs,
            );
            graph.with_types(types)
        })
        .map_err(|error| failure(format!("inconsistent facts: {error}"), None))?;
    Ok(FileFacts::new(graph, payload))
}

/// A dotted module path as written, without the whitespace and line continuations Python
/// allows between its parts.
fn dotted(written: &str) -> String {
    written
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\\')
        .collect()
}

/// The payload: the census from a fresh lexer pass, the written imports, and the binder facts.
fn payload(text: &str, analysis: &Analysis, imports: Vec<ImportSyntax>, bind: BindFacts) -> Payload {
    let scan = census::scan(text);
    let dynamic: Vec<TextRange> = analysis.proofs.dynamic_ranges().collect();
    let calls = scan
        .calls
        .into_iter()
        .filter(|call| !dynamic.iter().any(|range| bytes(*range) == call.range))
        .collect();
    let dependencies = analysis
        .proofs
        .dependencies()
        .iter()
        .map(|range| bytes(*range))
        .collect();
    Payload {
        version: PAYLOAD_VERSION,
        census: CensusData::new(&scan.starts, &scan.ends, calls, dependencies),
        imports,
        bind,
    }
}
