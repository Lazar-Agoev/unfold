//! Edges: one occurrence edge for every call, construction, and runtime import, followed by the
//! references of named callable arguments, in block order.
//!
//! [`program_edges`] resolves every occurrence of one language in parallel and reassembles the
//! results by input index, so the edges never depend on thread scheduling. [`text`] holds the
//! shared label grammar that languages use to spell value labels.

use rayon::prelude::*;
use smallvec::SmallVec;
use unfold_core::text::short_label;
use unfold_core::vocab::{EdgeKind, Reason};
use unfold_core::{Edge, ModuleLabel};

use crate::error::{BackendError, path_of};
use crate::evidence::{ExternalLocation, ModuleTarget, ValueContext};
use crate::program::{BlockRef, CallRef, FileId, ValueRef};
use crate::resolve::{Outcome, Resolver};

/// One occurrence to resolve: a call (by index in its file's call table) or an import.
#[derive(Debug, Clone, Copy)]
enum Occurrence {
    Call(u32),
    Import(u32),
}

type OccurrenceEdges = SmallVec<[Edge; 2]>;

/// The edges of every file of the resolver's language, in file order, each file's edges in
/// source then sequence order. Occurrences are resolved in parallel on the current rayon pool.
pub fn program_edges(resolver: &Resolver<'_>) -> Result<Vec<(FileId, Vec<Edge>)>, BackendError> {
    program_edges_checked(resolver, || Ok(()))
}

/// [`program_edges`], calling `check` before each occurrence so a caller can stop the work
/// early (for example, on cancellation). The first error `check` returns stops resolution and
/// is returned; backend errors convert into `E`.
pub fn program_edges_checked<E>(
    resolver: &Resolver<'_>,
    check: impl Fn() -> Result<(), E> + Sync,
) -> Result<Vec<(FileId, Vec<Edge>)>, E>
where
    E: From<BackendError> + Send,
{
    let program = resolver.program();
    let mut work: Vec<(FileId, Occurrence)> = Vec::new();
    let mut files: Vec<(FileId, usize)> = Vec::new();
    for (file, entry) in program.iter() {
        let Some(facts) = entry.facts.as_deref().filter(|_| resolver.owns(file)) else {
            continue;
        };
        check_dependencies(resolver, file)?;
        let before = work.len();
        work.extend(occurrences(facts).map(|occurrence| (file, occurrence)));
        files.push((file, work.len() - before));
    }
    let resolved: Vec<OccurrenceEdges> = work
        .par_iter()
        .map(|&(file, occurrence)| {
            check()?;
            occurrence_edges(resolver, file, occurrence).map_err(E::from)
        })
        .collect::<Result<_, _>>()?;
    let mut resolved = resolved.into_iter();
    Ok(files
        .into_iter()
        .map(|(file, count)| (file, resolved.by_ref().take(count).flatten().collect()))
        .collect())
}

/// The edges of one file, resolved on the calling thread.
pub fn file_edges(resolver: &Resolver<'_>, file: FileId) -> Result<Vec<Edge>, BackendError> {
    let Some(facts) = resolver.program().facts(file) else {
        return Ok(Vec::new());
    };
    check_dependencies(resolver, file)?;
    let mut edges = Vec::new();
    for occurrence in occurrences(facts) {
        edges.extend(occurrence_edges(resolver, file, occurrence)?);
    }
    Ok(edges)
}

/// A file's calls and imports merged into block order.
fn occurrences(facts: &crate::facts::FileFacts) -> impl Iterator<Item = Occurrence> + '_ {
    let calls = facts.graph.calls();
    let imports = facts.graph.imports();
    let (mut call, mut import) = (0usize, 0usize);
    std::iter::from_fn(move || {
        let next_call = calls.get(call).map(|fact| fact.block);
        let next_import = imports.get(import).map(|fact| fact.block);
        match (next_call, next_import) {
            (Some(a), Some(b)) if b < a => {
                import += 1;
                Some(Occurrence::Import(import as u32 - 1))
            }
            (Some(_), _) => {
                call += 1;
                Some(Occurrence::Call(call as u32 - 1))
            }
            (None, Some(_)) => {
                import += 1;
                Some(Occurrence::Import(import as u32 - 1))
            }
            (None, None) => None,
        }
    })
}

fn check_dependencies(resolver: &Resolver<'_>, file: FileId) -> Result<(), BackendError> {
    let program = resolver.program();
    let expected = program.imports(file).len();
    let found = resolver.linked().module_dependencies(file).len();
    if expected == found {
        return Ok(());
    }
    Err(BackendError::DependencyCount {
        path: path_of(program, file),
        expected,
        found,
    })
}

fn occurrence_edges(
    resolver: &Resolver<'_>,
    file: FileId,
    occurrence: Occurrence,
) -> Result<OccurrenceEdges, BackendError> {
    match occurrence {
        Occurrence::Call(index) => call_edges(resolver, CallRef { file, call: index }),
        Occurrence::Import(index) => {
            import_edge(resolver, file, index).map(|edge| SmallVec::from_iter([edge]))
        }
    }
}

fn edge(
    resolver: &Resolver<'_>,
    source: u32,
    seq: u32,
    kind: EdgeKind,
    text: &str,
    outcome: Outcome,
) -> Edge {
    let fields = outcome.into_fields(resolver.program());
    Edge {
        source,
        seq,
        kind,
        target: fields.target,
        text: short_label(text).into(),
        resolution: fields.resolution,
        reason: fields.reason,
        module: fields.module,
        position: fields.position,
    }
}

fn call_edges(resolver: &Resolver<'_>, call: CallRef) -> Result<OccurrenceEdges, BackendError> {
    let program = resolver.program();
    let mut edges = OccurrenceEdges::new();
    let Some(fact) = program.call(call) else {
        return Ok(edges);
    };
    let label = |value: u32| {
        program
            .value(ValueRef {
                file: call.file,
                value,
            })
            .map_or(text::OTHER, |fact| &*fact.label)
    };
    let invocation = resolver.invocation(call);
    let outcome = resolver.resolve_invoked(call, invocation)?;
    let kind = invocation.map_or(fact.edge, |invocation| invocation.edge);
    let callee = fact.callee.map_or(text::OTHER, label);
    edges.push(edge(resolver, fact.block, 0, kind, callee, outcome));
    for argument in fact.arguments.iter().filter(|argument| argument.reference) {
        let value = ValueRef {
            file: call.file,
            value: argument.value,
        };
        if !program.value(value).is_some_and(|fact| fact.shape.is_reference()) {
            continue;
        }
        let outcome = resolver.resolve_value(value, ValueContext::Reference)?;
        if !matches!(outcome, Outcome::Resolved(_)) {
            continue;
        }
        let seq = edges.len() as u32;
        let mut reference = edge(
            resolver,
            fact.block,
            seq,
            EdgeKind::References,
            label(argument.value),
            outcome,
        );
        reference.position = Some(argument.position);
        edges.push(reference);
    }
    Ok(edges)
}

fn import_edge(resolver: &Resolver<'_>, file: FileId, index: u32) -> Result<Edge, BackendError> {
    let program = resolver.program();
    let linked = resolver.linked();
    let fact = program.imports(file).get(index as usize);
    let target = linked.module_dependencies(file).get(index as usize);
    let (Some(fact), Some(target)) = (fact, target) else {
        return Err(BackendError::DependencyCount {
            path: path_of(program, file),
            expected: program.imports(file).len(),
            found: linked.module_dependencies(file).len(),
        });
    };
    let specifier = fact.specifier.as_deref();
    let text = match (target, specifier) {
        (ModuleTarget::Dynamic, _) | (_, None) => text::OTHER,
        (_, Some(specifier)) => specifier,
    };
    let outcome = match target {
        ModuleTarget::Indexed(target) => {
            if program.facts(*target).is_none() {
                return Err(BackendError::MissingFileBlock {
                    path: path_of(program, file),
                    target: path_of(program, *target).as_str().into(),
                });
            }
            Outcome::Resolved(BlockRef {
                file: *target,
                local: 0,
            })
        }
        ModuleTarget::External(location) => {
            let label = match (location, specifier) {
                (ExternalLocation::File(_), _) => linked.label_for_location(file, location),
                (_, Some(specifier)) if linked.is_package_specifier(file, specifier) => {
                    linked.label_for_specifier(file, specifier)
                }
                _ => linked.label_for_location(file, location),
            };
            let module = ModuleLabel::single(&label).ok_or_else(|| BackendError::InvalidModuleLabel {
                path: path_of(program, file),
                label: label.as_str().into(),
            })?;
            Outcome::External(module)
        }
        ModuleTarget::NotFound => Outcome::unresolved(Reason::NotFound),
        ModuleTarget::NotIndexed => Outcome::unresolved(Reason::NotIndexed),
        ModuleTarget::Ambiguous => Outcome::unresolved(Reason::Ambiguous),
        ModuleTarget::Dynamic => Outcome::unresolved(Reason::Dynamic),
        ModuleTarget::Unsupported => Outcome::unresolved(Reason::Unsupported),
    };
    Ok(edge(resolver, fact.block, 0, EdgeKind::Imports, text, outcome))
}

/// The shared grammar of value labels. Languages spell the parts; these helpers join them and
/// keep every intermediate label within 120 UTF-16 units, so labels of long chains stay cheap.
///
/// | Expression | Label |
/// | --- | --- |
/// | Identifier, private name, `this`, `super` | Its text |
/// | Property access | [`member`]: `receiver.name` |
/// | Call result | [`call_result`]: `callee()`, never the arguments |
/// | Literal element access | [`literal_key`]: `receiver["key"]`, keeping the written spelling |
/// | Computed element access | [`computed_key`]: `receiver[…]` |
/// | Construction, tagged template | The constructed expression's or tag's label |
/// | Transparent wrapper | The inner label |
/// | Anything else | [`OTHER`]: `(…)` |
pub mod text {
    use unfold_core::text::{LABEL_MAX_UTF16, short_label};

    /// The label of an expression without a static shape.
    pub const OTHER: &str = "(…)";

    fn join(parts: &[&str]) -> String {
        let mut label = String::new();
        for part in parts {
            label.push_str(part);
            // A label of at most 120 bytes is at most 120 UTF-16 units.
            if label.len() > LABEL_MAX_UTF16 && short_label(&label).len() < label.len() {
                break;
            }
        }
        let keep = short_label(&label).len();
        label.truncate(keep);
        label
    }

    /// `receiver.name`.
    pub fn member(receiver: &str, name: &str) -> String {
        join(&[receiver, ".", name])
    }

    /// `callee()`.
    pub fn call_result(callee: &str) -> String {
        join(&[callee, "()"])
    }

    /// `receiver[key]`, where `key` is the literal as written (quotes included).
    pub fn literal_key(receiver: &str, key: &str) -> String {
        join(&[receiver, "[", key, "]"])
    }

    /// `receiver[…]`.
    pub fn computed_key(receiver: &str) -> String {
        join(&[receiver, "[…]"])
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use unfold_core::text::utf16_len;

        #[test]
        fn joins_parts_like_the_proof_of_concept() {
            assert_eq!(member("orderService", "place"), "orderService.place");
            assert_eq!(member(&call_result("adults"), "map"), "adults().map");
            assert_eq!(literal_key("handlers", "\"save\""), "handlers[\"save\"]");
            assert_eq!(computed_key("handlers"), "handlers[…]");
        }

        #[test]
        fn long_chains_stay_within_the_limit() {
            let mut label = String::from("root");
            for _ in 0..200 {
                label = member(&label, "next");
                assert!(utf16_len(&label) <= LABEL_MAX_UTF16);
            }
            assert!(label.starts_with("root.next.next"));
            let wide = "😀".repeat(70);
            assert_eq!(utf16_len(&member(&wide, "x")), LABEL_MAX_UTF16);
        }
    }
}
