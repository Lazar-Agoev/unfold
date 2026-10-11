//! Edges: one occurrence edge for every call, construction, and runtime import, followed by the
//! references of named callable arguments, in block order; with each call's argument pairings
//! ([`crate::pairing`]) and each written type name's type link.
//!
//! [`program_links`] resolves every occurrence and type name of one language in parallel and
//! reassembles the results by input index, so nothing depends on thread scheduling. [`text`]
//! holds the shared label grammar that languages use to spell value labels.

use rayon::prelude::*;
use smallvec::SmallVec;
use unfold_core::text::short_label;
use unfold_core::typed::{Argument, Pairing, TypeLink};
use unfold_core::vocab::{EdgeKind, PairingReason, Reason, SlotName};
use unfold_core::{Edge, ModuleLabel};

use crate::error::{BackendError, path_of};
use crate::evidence::{ExternalLocation, ModuleTarget, ValueContext};
use crate::facts::ArgumentFact;
use crate::pairing::{Takers, pair};
use crate::program::{BlockRef, CallRef, FileId, TypeRef, ValueRef};
use crate::resolve::{Outcome, Resolver};

/// What one file's resolution produces: its edges, its arguments with their pairings, and its
/// type links.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileLinks {
    /// Edges in source, then sequence, order.
    pub edges: Vec<Edge>,
    /// One per argument of each call or construction, in call, then index, order; paired
    /// exactly when the call's occurrence edge resolved.
    pub arguments: Vec<Argument>,
    /// One per written type name, in the order the language recorded them.
    pub type_links: Vec<TypeLink>,
    /// For each call resolved to a function-like block whose bound count is proven: the call's
    /// block and how many leading parameters it binds itself, in call order.
    pub bounds: Vec<(u32, u32)>,
}

/// One unit of resolution work: a call (by index in its file's call table), an import, or a
/// written type name.
#[derive(Debug, Clone, Copy)]
enum Work {
    Call(u32),
    Import(u32),
    Type(u32),
}

/// What one unit of work resolved to.
enum Resolved {
    Call(ResolvedCall),
    Import(Edge),
    Type(TypeLink),
}

type OccurrenceEdges = SmallVec<[Edge; 2]>;

/// The edges of every file of the resolver's language, in file order, each file's edges in
/// source then sequence order. Occurrences are resolved in parallel on the current rayon pool.
pub fn program_edges(resolver: &Resolver<'_>) -> Result<Vec<(FileId, Vec<Edge>)>, BackendError> {
    Ok(program_links(resolver)?
        .into_iter()
        .map(|(file, links)| (file, links.edges))
        .collect())
}

/// Every file's [`FileLinks`] for the resolver's language, in file order.
pub fn program_links(resolver: &Resolver<'_>) -> Result<Vec<(FileId, FileLinks)>, BackendError> {
    program_links_checked(resolver, || Ok(()))
}

/// [`program_links`], calling `check` before each unit of work so a caller can stop early
/// (for example, on cancellation). The first error `check` returns stops resolution and is
/// returned; backend errors convert into `E`.
pub fn program_links_checked<E>(
    resolver: &Resolver<'_>,
    check: impl Fn() -> Result<(), E> + Sync,
) -> Result<Vec<(FileId, FileLinks)>, E>
where
    E: From<BackendError> + Send,
{
    let program = resolver.program();
    let mut work: Vec<(FileId, Work)> = Vec::new();
    let mut files: Vec<(FileId, usize)> = Vec::new();
    for (file, entry) in program.iter() {
        let Some(facts) = entry.facts.as_deref().filter(|_| resolver.owns(file)) else {
            continue;
        };
        check_dependencies(resolver, file)?;
        let before = work.len();
        work.extend(file_work(facts).map(|unit| (file, unit)));
        files.push((file, work.len() - before));
    }
    let resolved: Vec<Resolved> = work
        .par_iter()
        .map(|&(file, unit)| {
            check()?;
            resolve_work(resolver, file, unit).map_err(E::from)
        })
        .collect::<Result<_, _>>()?;
    let mut resolved = resolved.into_iter();
    Ok(files
        .into_iter()
        .map(|(file, count)| {
            let mut links = FileLinks::default();
            for unit in resolved.by_ref().take(count) {
                links.add(unit);
            }
            (file, links)
        })
        .collect())
}

/// The edges of one file, resolved on the calling thread.
pub fn file_edges(resolver: &Resolver<'_>, file: FileId) -> Result<Vec<Edge>, BackendError> {
    Ok(file_links(resolver, file)?.edges)
}

/// The [`FileLinks`] of one file, resolved on the calling thread.
pub fn file_links(resolver: &Resolver<'_>, file: FileId) -> Result<FileLinks, BackendError> {
    let mut links = FileLinks::default();
    let Some(facts) = resolver.program().facts(file) else {
        return Ok(links);
    };
    check_dependencies(resolver, file)?;
    for unit in file_work(facts) {
        links.add(resolve_work(resolver, file, unit)?);
    }
    Ok(links)
}

impl FileLinks {
    fn add(&mut self, resolved: Resolved) {
        match resolved {
            Resolved::Call(call) => {
                self.edges.extend(call.edges);
                self.arguments.extend(call.arguments);
                self.bounds.extend(call.bound);
            }
            Resolved::Import(edge) => self.edges.push(edge),
            Resolved::Type(link) => self.type_links.push(link),
        }
    }
}

/// A file's units of work: its calls and imports merged into block order, then its type names.
fn file_work(facts: &crate::facts::FileFacts) -> impl Iterator<Item = Work> + '_ {
    let types = facts.graph.types().type_refs.len() as u32;
    occurrences(facts)
        .map(|occurrence| match occurrence {
            Occurrence::Call(index) => Work::Call(index),
            Occurrence::Import(index) => Work::Import(index),
        })
        .chain((0..types).map(Work::Type))
}

fn resolve_work(resolver: &Resolver<'_>, file: FileId, unit: Work) -> Result<Resolved, BackendError> {
    Ok(match unit {
        Work::Call(index) => Resolved::Call(call_edges(resolver, CallRef { file, call: index })?),
        Work::Import(index) => Resolved::Import(import_edge(resolver, file, index)?),
        Work::Type(index) => Resolved::Type(type_link(resolver, TypeRef { file, index })?),
    })
}

/// One occurrence: a call (by index in its file's call table) or an import.
#[derive(Debug, Clone, Copy)]
enum Occurrence {
    Call(u32),
    Import(u32),
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

/// A call's edges (its occurrence, then its references), its arguments (paired when the
/// occurrence resolved; a passed callable's with its bound count), and, when the occurrence
/// resolved to a function-like block, how many of its parameters the call binds itself.
fn call_edges(resolver: &Resolver<'_>, call: CallRef) -> Result<ResolvedCall, BackendError> {
    let program = resolver.program();
    let mut resolved = ResolvedCall::default();
    let Some(fact) = program.call(call) else {
        return Ok(resolved);
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
    let callable = match outcome {
        Outcome::Resolved(target) if is_function_like(resolver, target) => Some(target),
        _ => None,
    };
    let bound = callable.and_then(|target| resolver.linked().bound_parameters(call, target));
    resolved.bound = bound.map(|bound| (fact.block, bound));
    resolved.arguments = arguments(resolver, call, &outcome, bound);
    resolved
        .edges
        .push(edge(resolver, fact.block, 0, kind, callee, outcome));
    for argument in fact.arguments.iter().filter(|argument| argument.reference) {
        let value = ValueRef {
            file: call.file,
            value: argument.value,
        };
        if !program.value(value).is_some_and(|fact| fact.shape.is_reference()) {
            continue;
        }
        let outcome = resolver.resolve_value(value, ValueContext::Reference)?;
        let Outcome::Resolved(target) = outcome else {
            continue;
        };
        if is_function_like(resolver, target) {
            let bound = resolver.linked().reference_bound_parameters(value, target);
            if let Some(record) = resolved
                .arguments
                .iter_mut()
                .find(|record| record.index == argument.position)
            {
                record.bound = bound;
            }
        }
        let seq = resolved.edges.len() as u32;
        let mut reference = edge(
            resolver,
            fact.block,
            seq,
            EdgeKind::References,
            label(argument.value),
            outcome,
        );
        reference.position = Some(argument.position);
        resolved.edges.push(reference);
    }
    Ok(resolved)
}

/// What resolving one call produces.
#[derive(Default)]
struct ResolvedCall {
    edges: OccurrenceEdges,
    arguments: Vec<Argument>,
    /// The call's block and its proven bound count.
    bound: Option<(u32, u32)>,
}

/// Whether `target` is a function-like block.
fn is_function_like(resolver: &Resolver<'_>, target: BlockRef) -> bool {
    resolver
        .program()
        .block(target)
        .is_some_and(|block| block.kind.is_function_like())
}

/// A call's argument records, one per `argument` slot: expansions and keywords as written,
/// paired with the target's parameters when the occurrence resolved (`bound` is how many of a
/// callable target's leading parameters the call binds, when proven). A slot without an argument
/// fact is a plain positional argument.
fn arguments(resolver: &Resolver<'_>, call: CallRef, outcome: &Outcome, bound: Option<u32>) -> Vec<Argument> {
    let program = resolver.program();
    let (Some(fact), Some(facts)) = (program.call(call), program.facts(call.file)) else {
        return Vec::new();
    };
    let slots = facts.graph.slots();
    let start = slots.partition_point(|slot| slot.block < fact.block);
    let written: Vec<ArgumentFact> = slots[start..]
        .iter()
        .take_while(|slot| slot.block == fact.block)
        .filter(|slot| slot.slot == SlotName::Argument)
        .filter_map(|slot| slot.index)
        .map(|index| {
            fact.arguments
                .iter()
                .find(|argument| argument.position == index)
                .cloned()
                .unwrap_or_else(|| ArgumentFact::positional(0, index, false))
        })
        .collect();
    let pairings: Vec<Option<Pairing>> = match outcome {
        Outcome::Resolved(target) => {
            let takers = Takers::of(program, *target, bound, || {
                resolver.linked().class_takes_no_arguments(*target)
            });
            match takers {
                Some(takers) => pair(&written, takers).into_iter().map(Some).collect(),
                None => vec![Some(Pairing::Unpaired(PairingReason::NoSignature)); written.len()],
            }
        }
        _ => vec![None; written.len()],
    };
    written
        .into_iter()
        .zip(pairings)
        .map(|(argument, pairing)| Argument {
            call: fact.block,
            index: argument.position,
            expand: argument.expand,
            keyword: argument.keyword,
            pairing,
            bound: None,
        })
        .collect()
}

/// A written type name's link: resolved in the `type` context.
fn type_link(resolver: &Resolver<'_>, reference: TypeRef) -> Result<TypeLink, BackendError> {
    let program = resolver.program();
    let Some(fact) = program.type_ref(reference) else {
        return Err(BackendError::Facts {
            path: path_of(program, reference.file),
            message: format!("type reference {} does not exist", reference.index).into(),
        });
    };
    let fields = resolver.resolve_type(reference)?.into_fields(program);
    Ok(TypeLink {
        block: fact.block,
        range: fact.range,
        text: fact.text.clone(),
        resolution: fields.resolution,
        target: fields.target,
        reason: fields.reason,
        module: fields.module,
    })
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
