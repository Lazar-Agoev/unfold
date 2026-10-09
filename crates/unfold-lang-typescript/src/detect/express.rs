//! `express.route`: `get`, `head`, `post`, `put`, `patch`, `delete`, `options`, and `all` called
//! on a proven Express application or router, with mount prefixes composed through `use`.
//!
//! | Receiver provenance | Receiver |
//! | --- | --- |
//! | `express` default, `()` | Application |
//! | `Express` or `Application`, `:type` | Application |
//! | `Router`, `()` or `:type` | Router |
//! | `express` default, `Router` `()` | Router |
//!
//! **Mounts.** `parent.use(router)` and `parent.use(path, router)` on a recognized parent, with
//! exactly one or two arguments and no leading spread, mount a router that is one indexed,
//! non-ambient `const` initialized by a proven router factory (across files too; an annotation
//! alone proves no identity).
//!
//! **Triggers.** An application route's literal path is exact. A router route follows each mount
//! branch (in registration order, duplicates kept) to an application, at most five mounts deep,
//! joining literal prefixes and the local path, collapsing repeated slashes, and dropping a
//! trailing slash (`/` stays). An unmounted router, a cycle, a deeper chain, a dynamic prefix, or
//! a branch without an application keeps the local path as `partial`; a route whose mounts fan
//! out into more than [`MAX_MOUNT_BRANCHES`] branches keeps it as one `partial` trigger. A path
//! that isn't a literal, or is empty, is unknown.
//!
//! **Targets.** The handlers after the path, in written order, with array literals flattened: a
//! spread is `unsupported`, a hole is `dynamic`, anything else goes through the shared handler
//! policy (a wrapper call is `wrapper`). After a leading spread every handler is `unsupported`.
//! A registration with no handler produces no entrance. Evidence is the route call.

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use unfold_core::vocab::{BlockKind, DetectorId, EntranceKind, HttpMethod, Reason};
use unfold_engine::entrances::{CandidateEvidence, EntranceCandidate, TargetCandidate, Trigger};
use unfold_engine::evidence::Origin;
use unfold_engine::facts::ValueShape;
use unfold_engine::provenance::Provenance;
use unfold_engine::{DeclRef, Linked, ValueRef};

use super::{Call, Source, failure, proves};
use crate::payload::{ArgumentSyntax, Element};

/// How many mounts a router route may follow to its application.
const MAX_MOUNTS: usize = 5;

/// How many mount branches one router route may follow. Each branch is its own entrance, so
/// wide mount fan-out multiplies them; past this bound the route keeps its local path as one
/// `partial` trigger instead.
const MAX_MOUNT_BRANCHES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Application,
    Router,
}

/// A recognized application or router expression.
#[derive(Debug, Clone, Copy)]
struct Receiver {
    kind: Kind,
    value: ValueRef,
}

/// One `use` of a router: its parent and its literal prefix (`None` when dynamic).
#[derive(Debug, Clone)]
struct Mount<'l> {
    parent: Receiver,
    path: Option<&'l str>,
}

struct Route<'l> {
    call: Call<'l>,
    receiver: Receiver,
    method: HttpMethod,
}

/// The routes of the program.
pub(super) fn detect(source: &Source<'_, '_, '_>) -> Vec<EntranceCandidate> {
    let importers = source.importers("express");
    if !importers.contains(&true) {
        return Vec::new();
    }
    let everywhere = vec![true; importers.len()];
    let mut routes = Vec::new();
    let mut mounts: FxHashMap<DeclRef, Vec<Mount<'_>>> = FxHashMap::default();
    for call in source.calls(&everywhere, BlockKind::Call) {
        let Some(callee) = call.callee() else {
            continue;
        };
        let Some(name) = source.member(callee) else {
            continue;
        };
        let method = method(name);
        if method.is_none() && name != "use" {
            continue;
        }
        let Some(receiver) = source.receiver(callee) else {
            continue;
        };
        let Some(kind) = receiver_kind(source.provenance(receiver).as_ref()) else {
            continue;
        };
        let parent = Receiver {
            kind,
            value: receiver,
        };
        if let Some(method) = method {
            routes.push(Route {
                call,
                receiver: parent,
                method,
            });
            continue;
        }
        if !matches!(call.len(), 1 | 2) {
            continue;
        }
        let (Some(first), Some(last)) = (call.argument(0), call.argument(call.len() - 1)) else {
            continue;
        };
        if first.spread {
            continue;
        }
        let Some(router) = mounted_router(source, last.value) else {
            continue;
        };
        let path = if call.len() == 1 {
            Some("")
        } else {
            first.literal()
        };
        mounts.entry(router).or_default().push(Mount { parent, path });
    }
    let paths = Paths { source, mounts };
    let mut candidates = Vec::new();
    for route in routes {
        let leading_spread = route.call.argument(0).is_some_and(|argument| argument.spread);
        let targets = handlers(source, &route.call, leading_spread);
        if targets.is_empty() {
            continue;
        }
        let local = route.call.argument(0).and_then(|argument| argument.literal());
        for trigger in paths.triggers(route.receiver, local) {
            candidates.push(EntranceCandidate {
                detector: DetectorId::ExpressRoute,
                kind: EntranceKind::Http,
                method: Some(route.method),
                trigger,
                label: None,
                evidence: CandidateEvidence::Block(route.call.block()),
                targets: targets.clone(),
            });
        }
    }
    candidates
}

fn method(name: &str) -> Option<HttpMethod> {
    Some(match name {
        "get" => HttpMethod::Get,
        "head" => HttpMethod::Head,
        "post" => HttpMethod::Post,
        "put" => HttpMethod::Put,
        "patch" => HttpMethod::Patch,
        "delete" => HttpMethod::Delete,
        "options" => HttpMethod::Options,
        "all" => HttpMethod::All,
        _ => return None,
    })
}

/// What a receiver's provenance proves it is.
fn receiver_kind(provenance: Option<&Provenance>) -> Option<Kind> {
    let application = proves(provenance, "express", "default", &["()"])
        || proves(provenance, "express", "Express", &[":type"])
        || proves(provenance, "express", "Application", &[":type"]);
    if application {
        return Some(Kind::Application);
    }
    let router = router_factory(provenance) || proves(provenance, "express", "Router", &[":type"]);
    router.then_some(Kind::Router)
}

/// Whether provenance proves a router made by a factory call.
fn router_factory(provenance: Option<&Provenance>) -> bool {
    proves(provenance, "express", "Router", &["()"])
        || proves(provenance, "express", "default", &["Router", "()"])
}

/// The one indexed declaration a name denotes.
fn declaration(source: &Source<'_, '_, '_>, value: ValueRef) -> Option<DeclRef> {
    let linked = source.linked();
    if linked.program().value(value)?.shape != ValueShape::Name {
        return None;
    }
    let evidence = linked.value_evidence(value);
    if evidence.ambiguous || evidence.unbound_this {
        return None;
    }
    let mut declarations: SmallVec<[DeclRef; 2]> = SmallVec::new();
    for candidate in evidence
        .candidates
        .iter()
        .filter(|candidate| !candidate.augmentation)
    {
        let declaration = candidate
            .declaration
            .filter(|_| candidate.origin == Origin::Indexed)?;
        if !declarations.contains(&declaration) {
            declarations.push(declaration);
        }
    }
    match declarations.as_slice() {
        [declaration] => Some(*declaration),
        _ => None,
    }
}

/// The declaration of a mounted router: a non-ambient `const` initialized by a call or
/// construction whose provenance proves a router factory.
fn mounted_router(source: &Source<'_, '_, '_>, value: ValueRef) -> Option<DeclRef> {
    let declaration = declaration(source, value)?;
    source.linked().origin_declaration(declaration).factory_callee?;
    router_factory(source.provenance(value).as_ref()).then_some(declaration)
}

/// The handler targets after the path, with array literals flattened.
fn handlers(source: &Source<'_, '_, '_>, call: &Call<'_>, uncertain: bool) -> Vec<TargetCandidate> {
    let mut targets = Vec::new();
    for argument in call.arguments_from(1) {
        if let ArgumentSyntax::Array(elements) = argument.syntax {
            for element in elements {
                targets.push(match *element {
                    Element::Hole if uncertain => failure(Reason::Unsupported, text_of_hole()),
                    Element::Hole => failure(Reason::Dynamic, text_of_hole()),
                    Element::Spread(value) => source.failure(Reason::Unsupported, at(argument.value, value)),
                    Element::Value(value) if uncertain => {
                        source.failure(Reason::Unsupported, at(argument.value, value))
                    }
                    Element::Value(value) => TargetCandidate::Value(at(argument.value, value)),
                });
            }
        } else if uncertain || argument.spread {
            targets.push(source.failure(Reason::Unsupported, argument.value));
        } else {
            targets.push(TargetCandidate::Value(argument.value));
        }
    }
    targets
}

/// A value of the same file as `near`.
fn at(near: ValueRef, value: u32) -> ValueRef {
    ValueRef {
        file: near.file,
        value,
    }
}

/// A hole has no expression; its text is the label of anything without a static shape.
fn text_of_hole() -> Box<str> {
    unfold_engine::edges::text::OTHER.into()
}

/// Mount chains, for router routes.
struct Paths<'s, 'c, 'l, 'a> {
    source: &'s Source<'c, 'l, 'a>,
    mounts: FxHashMap<DeclRef, Vec<Mount<'l>>>,
}

/// One branch being followed: the prefixes so far (outermost first) and whether all are literal.
struct Branch<'l> {
    receiver: Receiver,
    prefixes: Vec<&'l str>,
    literal: bool,
    seen: Vec<DeclRef>,
}

impl Paths<'_, '_, '_, '_> {
    /// The triggers of a route on `receiver` with the local path `local`: one per mount branch.
    fn triggers(&self, receiver: Receiver, local: Option<&str>) -> Vec<Trigger> {
        let exact_or_unknown = |path: &str| {
            if path.is_empty() {
                Trigger::Unknown
            } else {
                Trigger::Exact(path.into())
            }
        };
        if receiver.kind == Kind::Application {
            return vec![local.map_or(Trigger::Unknown, exact_or_unknown)];
        }
        let fallback = match local {
            Some(path) if !path.is_empty() => Trigger::Partial(path.into()),
            _ => Trigger::Unknown,
        };
        let mut triggers = Vec::new();
        // Depth-first, in mount registration order; a branch is at most five mounts deep.
        let mut pending = vec![Branch {
            receiver,
            prefixes: Vec::new(),
            literal: true,
            seen: Vec::new(),
        }];
        let mut branches = 0;
        while let Some(branch) = pending.pop() {
            branches += 1;
            if branches > MAX_MOUNT_BRANCHES {
                return vec![fallback];
            }
            if branch.receiver.kind == Kind::Application {
                triggers.push(match local {
                    None => Trigger::Unknown,
                    Some(_) if !branch.literal => fallback.clone(),
                    Some(local) => {
                        let mut parts = branch.prefixes.clone();
                        parts.push(local);
                        Trigger::Exact(join(&parts).into())
                    }
                });
                continue;
            }
            let identity = declaration(self.source, branch.receiver.value)
                .filter(|identity| !branch.seen.contains(identity) && branch.seen.len() < MAX_MOUNTS);
            let parents = identity.and_then(|identity| Some((identity, self.mounts.get(&identity)?)));
            let Some((identity, parents)) = parents.filter(|(_, parents)| !parents.is_empty()) else {
                triggers.push(fallback.clone());
                continue;
            };
            for mount in parents.iter().rev() {
                let mut prefixes = branch.prefixes.clone();
                if let Some(path) = mount.path {
                    prefixes.insert(0, path);
                }
                let mut seen = branch.seen.clone();
                seen.push(identity);
                pending.push(Branch {
                    receiver: mount.parent,
                    prefixes,
                    literal: branch.literal && mount.path.is_some(),
                    seen,
                });
            }
        }
        triggers
    }
}

/// Literal path parts joined: empty parts dropped, repeated slashes collapsed, and one trailing
/// slash removed; an empty result is `/`.
fn join(parts: &[&str]) -> String {
    let joined = parts
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("/");
    let mut collapsed = String::with_capacity(joined.len());
    for character in joined.chars() {
        if character == '/' && collapsed.ends_with('/') {
            continue;
        }
        collapsed.push(character);
    }
    if collapsed.ends_with('/') {
        collapsed.pop();
    }
    if collapsed.is_empty() {
        "/".to_owned()
    } else {
        collapsed
    }
}

#[cfg(test)]
mod tests {
    use super::join;

    #[test]
    fn joins_literal_prefixes_like_the_proof_of_concept() {
        assert_eq!(join(&["/v1/", "/nested/", "/item//"]), "/v1/nested/item");
        assert_eq!(join(&["", ""]), "/");
        assert_eq!(join(&["/known", "/local//"]), "/known/local");
        assert_eq!(join(&["/direct", "/"]), "/direct");
        assert_eq!(join(&["/top", "/m4", "/five"]), "/top/m4/five");
        assert_eq!(join(&["a", "b"]), "a/b");
    }
}
