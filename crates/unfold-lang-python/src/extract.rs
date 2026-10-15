//! Extraction of one Python file: parse, the pre-pass ([`crate::analysis`]), lowering to nodes
//! ([`crate::lower`]), the value table and call facts ([`crate::values`]), the binder pass
//! ([`crate::bind`]), the written type names, and the payload.

use ruff_python_ast::Expr;
use rustc_hash::FxHashMap;
use unfold_core::Snapshot;
use unfold_core::text::ByteRange;
use unfold_core::vocab::{Form, NodeKind};
use unfold_engine::SourceFailure;
use unfold_engine::facts::{CallFact, FileFacts, GraphFacts, TypeFacts, TypeRefFact, TypeUse};
use unfold_engine::settings::PythonVersion;

use crate::analysis::{self, Analysis};
use crate::bind::ir::{BindFacts, ExprId};
use crate::bind::{self, TypeName, TypeSite, alias_address};
use crate::census;
use crate::lower::{self, Lowered, Truncated, address};
use crate::parse::{self, Module, failure};
use crate::payload::{ImportSyntax, PAYLOAD_VERSION, Payload};
use crate::tokens::Tokens;
use crate::values::Values;

/// Parses `file` with grammar `version` and extracts its facts. Never panics: every problem is
/// a [`SourceFailure`].
pub fn extract(file: &Snapshot, version: PythonVersion) -> Result<FileFacts, SourceFailure> {
    let text = file.text();
    parse::check_encoding(text)?;
    let module = parse::parse_module(text, version)
        .ok_or_else(|| failure("the parser did not produce a module", None))?;
    let syntax_error = parse::syntax_error(&module);
    let analysis = analysis::analyze(module.syntax());
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
    let tokens = Tokens::new(module.tokens());
    let lowered = lower::lower(file.path(), text, module.syntax(), tokens, &analysis.proofs)?;
    let mut values = Values::new(text);
    // Each call expression's callee in the value table, for the binder's factories.
    let mut callees: FxHashMap<usize, u32> = FxHashMap::default();
    let calls: Vec<CallFact> = lowered
        .calls
        .iter()
        .map(|called| {
            let fact = values.call(called);
            if let Expr::Call(_) = called.expr {
                callees.insert(address(called.expr), fact.callee);
            }
            fact
        })
        .collect();
    let (values, exprs) = values.finish();
    let inputs = bind::Inputs {
        text,
        definitions: &lowered.definitions,
        values: &exprs,
        callees: &callees,
    };
    let built = bind::build(module.syntax(), &inputs);
    let mut bind = built.facts;
    let (type_refs, types) = type_refs(text, &lowered, built.type_names, &bind.types);
    bind.types = types;
    let Lowered {
        nodes,
        imports: lowered_imports,
        truncated,
        stubs,
        signatures,
        parameters,
        members,
        ..
    } = lowered;
    let mut imports = Vec::with_capacity(lowered_imports.len());
    let mut written = Vec::with_capacity(lowered_imports.len());
    for import in lowered_imports {
        let mut syntax = import.syntax;
        if let (ImportSyntax::Child { node, .. }, Some(alias)) = (&mut syntax, import.child) {
            *node = built
                .aliases
                .get(&alias_address(alias))
                .copied()
                .unwrap_or_default();
        }
        imports.push(import.fact);
        written.push(syntax);
    }
    let payload = payload(text, &truncated, &stubs, written, bind)
        .to_bytes()
        .map_err(|error| failure(error.to_string(), None))?;
    let types = TypeFacts::new(signatures, parameters, members);
    let graph = GraphFacts::new(nodes, values, calls, imports, type_refs, types)
        .map_err(|error| failure(format!("inconsistent facts: {error}"), None))?;
    Ok(FileFacts::new(graph, payload))
}

/// The file's written type names as `types` and `extends` links before linking, with the
/// binder's expression of each (aligned). A name in an annotation starts at the nearest
/// definition around it, a base at its class, and a handler's class at its `except` part; a
/// name inside a truncated block or an overload stub, or whose node wasn't lowered, has no
/// link.
fn type_refs(
    text: &str,
    lowered: &Lowered<'_>,
    names: Vec<TypeName>,
    exprs: &[ExprId],
) -> (Vec<TypeRefFact>, Vec<ExprId>) {
    let nodes = &lowered.nodes;
    let candidates = |keep: &dyn Fn(u32) -> Option<ByteRange>| -> Vec<(u32, ByteRange)> {
        (0..u32::try_from(nodes.len()).unwrap_or(u32::MAX))
            .filter_map(|id| keep(id).map(|range| (id, range)))
            .collect()
    };
    let definitions = candidates(&|id| {
        let node = &nodes[id as usize];
        node.kind().is_definition().then_some(node.range)
    });
    let handlers = candidates(&|id| {
        let node = &nodes[id as usize];
        (node.form == Form::Except).then_some(node.head).flatten()
    });
    let mut references = Vec::new();
    let mut kept = Vec::new();
    for (name, &expr) in names.iter().zip(exprs) {
        if lowered
            .truncated
            .iter()
            .any(|truncated| truncated.range.contains(name.range))
            || lowered.stubs.iter().any(|stub| stub.contains(name.range))
        {
            continue;
        }
        let from = match name.site {
            TypeSite::Annotation => innermost(&definitions, name.range),
            TypeSite::Base => innermost(&definitions, name.range)
                .filter(|&class| nodes[class as usize].kind() == NodeKind::Class),
            TypeSite::Handler => innermost(&handlers, name.range),
        };
        let Some(from) = from else {
            continue;
        };
        let usage = match name.site {
            TypeSite::Base => TypeUse::Extends,
            TypeSite::Annotation | TypeSite::Handler => TypeUse::Annotation,
        };
        let written = text.get(name.range.as_usize()).unwrap_or_default();
        references.push(TypeRefFact::new(from, name.range, written, name.qualified, usage));
        kept.push(expr);
    }
    (references, kept)
}

/// The node with the smallest of `candidates`' ranges that contains `range`.
fn innermost(candidates: &[(u32, ByteRange)], range: ByteRange) -> Option<u32> {
    candidates
        .iter()
        .filter(|(_, candidate)| candidate.contains(range))
        .min_by_key(|(id, candidate)| (candidate.len(), std::cmp::Reverse(*id)))
        .map(|&(id, _)| id)
}

/// The payload: the call sites from a fresh lexer pass (without those truncated blocks hide or
/// overload stubs hold), the written imports, and the binder facts.
fn payload(
    text: &str,
    truncated: &[Truncated],
    stubs: &[ByteRange],
    imports: Vec<ImportSyntax>,
    bind: BindFacts,
) -> Payload {
    let calls = census::calls(text)
        .into_iter()
        .filter(|range| {
            !truncated.iter().any(|truncated| truncated.hides(*range))
                && !stubs.iter().any(|stub| stub.contains(*range))
        })
        .collect();
    Payload {
        version: PAYLOAD_VERSION,
        calls,
        imports,
        bind,
    }
}
