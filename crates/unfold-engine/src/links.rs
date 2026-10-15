//! Links: every call's `calls` link, then the `passes` links of the definitions it passes, and
//! every kept `imports` link and every `types` and `extends` link, each with its outcome; with
//! each call's argument pairings ([`crate::pairing`]).
//!
//! [`program_links`] resolves every call, import, and type name of one language in parallel
//! and reassembles the results by input index, so nothing depends on thread scheduling. A
//! node's links are numbered from 0: its `calls` link first, then the others in written order.

use rayon::prelude::*;
use unfold_core::text::short_label;
use unfold_core::typed::{Argument, Pairing};
use unfold_core::vocab::{LinkKind, NodeKind, PairingReason, Reason};
use unfold_core::{ByteRange, Link, LinkTarget, ModuleLabel};

use crate::error::{BackendError, path_of};
use crate::evidence::{ExternalLocation, ModuleTarget, ValueContext};
use crate::facts::{CallContext, CallFact, FileFacts, TypeUse};
use crate::pairing::{Takers, pair};
use crate::program::{CallRef, FileId, ImportRef, NodeRef, TypeRef, ValueRef};
use crate::resolve::{Outcome, Resolver};

/// What one file's resolution produces: its links, its arguments with their pairings, and the
/// call nodes that are constructions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileLinks {
    /// Links in `from`, then `seq`, order.
    pub links: Vec<Link>,
    /// One per argument of each call, in call, then index, order; paired exactly when the
    /// call's `calls` link reaches a project definition.
    pub arguments: Vec<Argument>,
    /// The call nodes linking proved to be constructions ([`crate::Linked::constructs`]), in
    /// node order: their form becomes `new`.
    pub constructions: Vec<u32>,
}

/// One unit of resolution work, by index in its file's call, import, or type-name table.
#[derive(Debug, Clone, Copy)]
enum Work {
    Call(u32),
    Import(u32),
    Type(u32),
}

/// What one unit of work resolved to.
enum Resolved {
    Call(ResolvedCall),
    /// An unkept conditional import has no link.
    Import(Option<Draft>),
    Type(Draft),
}

/// A link before it is numbered.
struct Draft {
    from: u32,
    kind: LinkKind,
    range: ByteRange,
    text: Box<str>,
    to: LinkTarget,
    implements: bool,
    bound: Option<u32>,
}

/// What resolving one call produces.
struct ResolvedCall {
    /// Its `calls` link, then its `passes` links in argument order.
    links: Vec<Draft>,
    arguments: Vec<Argument>,
    /// The call node, when linking proved it a construction.
    construction: Option<u32>,
}

/// The label of an expression without a static shape, such as a dynamic import's specifier.
pub(crate) const OTHER: &str = "(…)";

/// Every file's [`FileLinks`] for the resolver's language, in file order, calling `check` before
/// each unit of work so a caller can stop early (for example, on cancellation). The first error
/// `check` returns stops resolution and is returned; backend errors convert into `E`.
pub fn program_links<E>(
    resolver: &Resolver<'_>,
    check: impl Fn() -> Result<(), E> + Sync,
) -> Result<Vec<(FileId, FileLinks)>, E>
where
    E: From<BackendError> + Send,
{
    let program = resolver.program();
    let mut work: Vec<(FileId, &FileFacts, Work)> = Vec::new();
    let mut files: Vec<(FileId, usize)> = Vec::new();
    for (file, entry) in program.iter() {
        let Some(facts) = entry.facts.as_deref().filter(|_| resolver.owns(file)) else {
            continue;
        };
        check_dependencies(resolver, file)?;
        let before = work.len();
        work.extend(file_work(facts).map(|unit| (file, facts, unit)));
        files.push((file, work.len() - before));
    }
    let resolved: Vec<Resolved> = work
        .par_iter()
        .map(|&(file, facts, unit)| {
            check()?;
            resolve_work(resolver, file, facts, unit).map_err(E::from)
        })
        .collect::<Result<_, _>>()?;
    let mut resolved = resolved.into_iter();
    Ok(files
        .into_iter()
        .map(|(file, count)| (file, assemble(resolved.by_ref().take(count))))
        .collect())
}

/// A file's links, numbered, with its arguments and constructions.
fn assemble(resolved: impl Iterator<Item = Resolved>) -> FileLinks {
    let mut drafts = Vec::new();
    let mut links = FileLinks::default();
    for unit in resolved {
        match unit {
            Resolved::Call(call) => {
                drafts.extend(call.links);
                links.arguments.extend(call.arguments);
                links.constructions.extend(call.construction);
            }
            Resolved::Import(draft) => drafts.extend(draft),
            Resolved::Type(draft) => drafts.push(draft),
        }
    }
    // A node's `calls` link first, then its other links in written order. The sort is stable,
    // and the drafts arrive in input order, so equal keys keep a deterministic order.
    drafts.sort_by_key(|draft| {
        (
            draft.from,
            draft.kind != LinkKind::Calls,
            draft.range.start,
            draft.range.end,
            draft.kind,
        )
    });
    let mut previous = None;
    let mut seq = 0;
    links.links = drafts
        .into_iter()
        .map(|draft| {
            seq = if previous == Some(draft.from) { seq + 1 } else { 0 };
            previous = Some(draft.from);
            Link {
                from: draft.from,
                seq,
                kind: draft.kind,
                range: draft.range,
                text: draft.text,
                to: draft.to,
                implements: draft.implements,
                bound: draft.bound,
            }
        })
        .collect();
    links
}

/// A file's units of work: its calls in node order, then its imports, then its type names.
fn file_work(facts: &FileFacts) -> impl Iterator<Item = Work> + '_ {
    let graph = &facts.graph;
    let count = |len: usize| 0..u32::try_from(len).unwrap_or(u32::MAX);
    count(graph.calls().len())
        .map(Work::Call)
        .chain(count(graph.imports().len()).map(Work::Import))
        .chain(count(graph.type_refs().len()).map(Work::Type))
}

/// Resolves one unit of `file`'s work; `facts` are the file's facts, whose tables the unit
/// indexes.
fn resolve_work(
    resolver: &Resolver<'_>,
    file: FileId,
    facts: &FileFacts,
    unit: Work,
) -> Result<Resolved, BackendError> {
    Ok(match unit {
        Work::Call(call) => Resolved::Call(call_links(resolver, facts, CallRef { file, call })?),
        Work::Import(import) => Resolved::Import(import_link(resolver, facts, ImportRef { file, import })?),
        Work::Type(index) => Resolved::Type(type_link(resolver, facts, TypeRef { file, index })?),
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

/// The link target of an outcome: a resolved node's key, a library or the language's builtins,
/// or a reason.
fn link_target(resolver: &Resolver<'_>, outcome: Outcome) -> LinkTarget {
    match outcome {
        Outcome::Resolved(node) => match resolver.program().node(node) {
            Some(node) => LinkTarget::Project(node.key.clone()),
            // The policy resolves only to nodes that exist.
            None => LinkTarget::Unknown(Reason::NotFound),
        },
        Outcome::External(label) if resolver.linked().is_builtin(&label) => LinkTarget::Builtin(label),
        Outcome::External(label) => LinkTarget::Library(label),
        Outcome::Unresolved(reason) => LinkTarget::Unknown(reason),
    }
}

/// Whether `outcome` resolved to a function node.
fn function(resolver: &Resolver<'_>, outcome: &Outcome) -> Option<NodeRef> {
    match outcome {
        Outcome::Resolved(node)
            if resolver
                .program()
                .node(*node)
                .is_some_and(|found| found.kind() == NodeKind::Function) =>
        {
            Some(*node)
        }
        _ => None,
    }
}

/// A call's `calls` link (with its bound count when it reaches a project function), the
/// `passes` links of the project definitions its arguments name (each argument with the passed
/// function's bound count), and its arguments, paired when the call reaches a project
/// definition.
fn call_links(
    resolver: &Resolver<'_>,
    facts: &FileFacts,
    call: CallRef,
) -> Result<ResolvedCall, BackendError> {
    let linked = resolver.linked();
    let graph = &facts.graph;
    // Call and value indexes are checked when the facts are built or decoded.
    let fact = &graph.calls()[call.call as usize];
    let callee = &graph.values()[fact.callee as usize];
    let constructs = fact.context == CallContext::Call && linked.constructs(call);
    let context = if constructs {
        CallContext::Construct
    } else {
        fact.context
    };
    let outcome = resolver.resolve_call(call, context)?;
    let bound = function(resolver, &outcome).and_then(|target| linked.bound_parameters(call, target));
    let mut arguments = arguments(resolver, fact, &outcome, bound);
    let mut links = vec![Draft {
        from: fact.node,
        kind: LinkKind::Calls,
        range: callee.range,
        text: callee.label.clone(),
        to: link_target(resolver, outcome),
        implements: false,
        bound,
    }];
    for (argument, record) in fact.arguments.iter().zip(&mut arguments) {
        let Some(passed) = argument.value else {
            continue;
        };
        let reference = ValueRef {
            file: call.file,
            value: passed,
        };
        let outcome = resolver.resolve_value(reference, ValueContext::Reference)?;
        let Outcome::Resolved(_) = outcome else {
            continue;
        };
        if let Some(target) = function(resolver, &outcome) {
            record.bound = linked.reference_bound_parameters(reference, target);
        }
        let passed = &graph.values()[passed as usize];
        links.push(Draft {
            from: fact.node,
            kind: LinkKind::Passes,
            range: passed.range,
            text: passed.label.clone(),
            to: link_target(resolver, outcome),
            implements: false,
            bound: None,
        });
    }
    Ok(ResolvedCall {
        links,
        arguments,
        construction: constructs.then_some(fact.node),
    })
}

/// A call's argument records, in written order: expansions and keywords as written, paired with
/// the target's parameters when the call reaches a project definition (`bound` is how many of a
/// function target's leading parameters the call binds, when proven).
fn arguments(
    resolver: &Resolver<'_>,
    fact: &CallFact,
    outcome: &Outcome,
    bound: Option<u32>,
) -> Vec<Argument> {
    let program = resolver.program();
    let pairings: Vec<Option<Pairing>> = match outcome {
        Outcome::Resolved(target) => {
            let takers = Takers::of(program, *target, bound, || {
                resolver.linked().class_takes_no_arguments(*target)
            });
            match takers {
                Some(takers) => pair(&fact.arguments, takers).into_iter().map(Some).collect(),
                None => vec![Some(Pairing::Unpaired(PairingReason::NoSignature)); fact.arguments.len()],
            }
        }
        _ => vec![None; fact.arguments.len()],
    };
    fact.arguments
        .iter()
        .zip(pairings)
        .enumerate()
        .map(|(index, (argument, pairing))| Argument {
            call: fact.node,
            index: u32::try_from(index).unwrap_or(u32::MAX),
            range: argument.range,
            expand: argument.expand,
            keyword: argument.keyword.clone(),
            pairing,
            bound: None,
        })
        .collect()
}

/// A written type name's `types` or `extends` link, resolved in the `type` context.
fn type_link(resolver: &Resolver<'_>, facts: &FileFacts, reference: TypeRef) -> Result<Draft, BackendError> {
    let fact = &facts.graph.type_refs()[reference.index as usize];
    let outcome = resolver.resolve_type(reference)?;
    Ok(Draft {
        from: fact.from,
        kind: match fact.usage {
            TypeUse::Annotation => LinkKind::Types,
            TypeUse::Extends | TypeUse::Implements => LinkKind::Extends,
        },
        range: fact.range,
        text: fact.text.clone(),
        to: link_target(resolver, outcome),
        implements: fact.usage == TypeUse::Implements,
        bound: None,
    })
}

/// A written module dependency's `imports` link, or `None` for a conditional import linking
/// doesn't keep.
fn import_link(
    resolver: &Resolver<'_>,
    facts: &FileFacts,
    import: ImportRef,
) -> Result<Option<Draft>, BackendError> {
    let program = resolver.program();
    let linked = resolver.linked();
    let file = import.file;
    let fact = &facts.graph.imports()[import.import as usize];
    // The dependency count is checked before any of the file's work.
    let Some(target) = linked.module_dependencies(file).get(import.import as usize) else {
        return Err(BackendError::DependencyCount {
            path: path_of(program, file),
            expected: facts.graph.imports().len(),
            found: linked.module_dependencies(file).len(),
        });
    };
    if fact.conditional && !linked.keeps_import(import) {
        return Ok(None);
    }
    let specifier = fact.specifier.as_deref();
    let text = match (target, specifier) {
        (ModuleTarget::Dynamic, _) | (_, None) => OTHER,
        (_, Some(specifier)) => specifier,
    };
    let outcome = match target {
        ModuleTarget::Indexed(target) => {
            if program.facts(*target).is_none() {
                return Err(BackendError::MissingModule {
                    path: path_of(program, file),
                    target: path_of(program, *target).as_str().into(),
                });
            }
            Outcome::Resolved(NodeRef {
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
        ModuleTarget::NotFound => Outcome::Unresolved(Reason::NotFound),
        ModuleTarget::NotIndexed => Outcome::Unresolved(Reason::NotIndexed),
        ModuleTarget::Ambiguous => Outcome::Unresolved(Reason::Ambiguous),
        ModuleTarget::Dynamic => Outcome::Unresolved(Reason::Dynamic),
        ModuleTarget::Unsupported => Outcome::Unresolved(Reason::Unsupported),
    };
    Ok(Some(Draft {
        from: fact.from,
        kind: LinkKind::Imports,
        range: fact.range,
        text: short_label(text).into(),
        to: link_target(resolver, outcome),
        implements: false,
        bound: None,
    }))
}
