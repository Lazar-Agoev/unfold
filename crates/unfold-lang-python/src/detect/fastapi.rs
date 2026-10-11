//! `fastapi.route`: `get`, `post`, `put`, `patch`, `delete`, `options`, and `head` registrations
//! on a proven `FastAPI` application or `APIRouter`, as a function decorator
//! (`@app.get("/x")`) or an immediately applied route factory (`app.get("/x")(handler)`).
//!
//! The trigger is the literal path, prefixed by every concrete `include_router` mount up to
//! five deep (each mount branch is its own entrance); a router with unknown, absent, or
//! annotation-only mounts keeps a `partial` local path, and a dynamic or bytes path is
//! `unknown`. Literal `dependencies=[Depends(...)]` lists of the application, mounts, router,
//! and route come first as handler targets, in declaration order, then the endpoint. Each
//! branch that reaches an application records its binding: the block of the module-level
//! assignment the application is (`app = FastAPI()`), which an ASGI launch target
//! `module:app` resolves to.

use rustc_hash::FxHashMap;
use unfold_core::vocab::{DetectorId, EntranceKind, HttpMethod, Reason};
use unfold_engine::BlockRef;
use unfold_engine::entrances::{CandidateEvidence, EntranceCandidate, TargetCandidate, Trigger};

use super::origins::{Flow, Identity, Origins, Receiver, Spec};
use super::{Source, failure};
use crate::bind::ir::{ExprKind, NodeKind};
use crate::link::binder::Ctx;
use crate::link::program::{ExprRef, NodeRef};

const SPECS: &[Spec] = &[
    Spec {
        family: "fastapi",
        kind: "application",
        imports: &["fastapi.FastAPI", "fastapi.applications.FastAPI"],
        safe_updates: &[],
    },
    Spec {
        family: "fastapi",
        kind: "router",
        imports: &["fastapi.APIRouter", "fastapi.routing.APIRouter"],
        safe_updates: &[],
    },
    Spec {
        family: "fastapi-dependency",
        kind: "dependency",
        imports: &[
            "fastapi.Depends",
            "fastapi.param_functions.Depends",
            "fastapi.params.Depends",
        ],
        safe_updates: &[],
    },
];

const METHODS: &[(&str, HttpMethod)] = &[
    ("get", HttpMethod::Get),
    ("post", HttpMethod::Post),
    ("put", HttpMethod::Put),
    ("patch", HttpMethod::Patch),
    ("delete", HttpMethod::Delete),
    ("options", HttpMethod::Options),
    ("head", HttpMethod::Head),
];

/// Decorators that keep a registered body (with independently proven identity).
pub(super) const TRANSPARENT: &[&str] = &[
    "builtins.staticmethod",
    "builtins.classmethod",
    "functools.cache",
    "functools.lru_cache",
    "typing.final",
    "typing.override",
];

const DEPENDS: &[&str] = &[
    "fastapi.Depends",
    "fastapi.param_functions.Depends",
    "fastapi.params.Depends",
];

/// The deepest chain of mounts a route path follows.
const MAX_MOUNTS: usize = 5;

/// The most mount branches one route follows. Each level of `include_router` mounts multiplies
/// the branches, so a route past this many gets one `partial` entrance with its local path, as
/// a router mounted nowhere does, instead of exponentially many.
const MAX_MOUNT_BRANCHES: usize = 256;

struct Registration {
    receiver: Receiver,
    method: HttpMethod,
    factory: ExprRef,
}

struct Mount {
    parent: Receiver,
    /// The literal prefix (`""` when none is written); `None` when it isn't a literal.
    prefix: Option<String>,
    dependencies: Vec<ExprRef>,
}

struct RoutePath {
    trigger: Trigger,
    dependencies: Vec<ExprRef>,
    /// The block binding the application the mount chain reaches.
    binding: Option<BlockRef>,
}

/// A target before it becomes a candidate target, keeping the expression it was written as.
#[derive(Clone)]
enum Target {
    Value(ExprRef),
    Block(BlockRef, Box<str>),
    Failure(Reason, Box<str>),
}

/// The FastAPI routes of the program.
pub(super) fn detect(flow: &Flow<'_, '_, '_>) -> Vec<EntranceCandidate> {
    let origins = Origins::new(flow, SPECS);
    let mut routes = Routes {
        origins,
        mounts: FxHashMap::default(),
    };
    routes.collect_mounts();
    routes.candidates()
}

struct Routes<'f, 's, 'b, 'a> {
    origins: Origins<'f, 's, 'b, 'a>,
    mounts: FxHashMap<Identity, Vec<Mount>>,
}

impl Routes<'_, '_, '_, '_> {
    fn source(&self) -> &Source<'_, '_> {
        self.origins.source()
    }

    /// `receiver.<method>(...)` on a proven FastAPI object.
    fn registration(&self, call: Option<ExprRef>) -> Option<Registration> {
        let call = call?;
        let source = self.source();
        let (base, name) = source.member(source.callee(call)?)?;
        let method = METHODS.iter().find(|(written, _)| *written == name)?.1;
        let receiver = self.origins.receiver(Some(base))?;
        (receiver.spec.family == "fastapi").then_some(Registration {
            receiver,
            method,
            factory: call,
        })
    }

    /// The `dependencies=` declaration and expansions of a constructor, mount, or route call.
    fn dependency_declarations(&self, call: Option<ExprRef>) -> Vec<ExprRef> {
        let Some(call) = call else {
            return Vec::new();
        };
        let args = self.source().args(call);
        args.get("dependencies", None)
            .into_iter()
            .chain(args.expansions())
            .collect()
    }

    fn collect_mounts(&mut self) {
        let source = self.origins.source();
        for site in source.sites.all() {
            let Some((base, "include_router")) =
                source.callee(site.call).and_then(|callee| source.member(callee))
            else {
                continue;
            };
            let parent = self.origins.receiver(Some(base));
            let args = source.args(site.call);
            let Some(parent) = parent.filter(|parent| !parent.annotation_only && !args.uncertain()) else {
                continue;
            };
            let child = self.origins.receiver(args.get("router", Some(0)));
            let Some(child) = child.filter(|child| !child.annotation_only && child.spec.kind == "router")
            else {
                continue;
            };
            let prefix = match args.get("prefix", None) {
                Some(prefix) => source.string(Some(prefix)).map(str::to_owned),
                None => Some(String::new()),
            };
            let dependencies = self.dependency_declarations(Some(site.call));
            self.mounts.entry(child.identity).or_default().push(Mount {
                parent,
                prefix,
                dependencies,
            });
        }
    }

    /// A receiver's own prefix: none for an application, the literal `prefix=` of a router's
    /// constructor; `None` when it isn't known.
    fn receiver_prefix(&self, receiver: &Receiver) -> Option<String> {
        if receiver.spec.kind == "application" {
            return Some(String::new());
        }
        let args = self.source().args(receiver.factory?);
        if args.uncertain() {
            return None;
        }
        match args.get("prefix", None) {
            Some(prefix) => self.source().string(Some(prefix)).map(str::to_owned),
            None => Some(String::new()),
        }
    }

    /// Whether every decorator of the declaration and of the classes around it keeps the body
    /// (a route registration on the declaration itself is the registration, not a wrapper).
    fn stack(&self, node: NodeRef) -> bool {
        let source = self.source();
        let mut containers = vec![node];
        containers.extend(source.enclosing_classes(node));
        containers.iter().all(|&container| {
            source.decorators(container).into_iter().all(|decorator| {
                if container == node && self.registration(Some(decorator)).is_some() {
                    return true;
                }
                let target = match source.kind(decorator) {
                    Some(ExprKind::Call { .. }) => source.callee(decorator),
                    _ => Some(decorator),
                };
                self.origins
                    .imported(target)
                    .is_some_and(|identity| TRANSPARENT.contains(&&*identity))
            })
        })
    }

    /// A registered `def` as its own handler.
    fn declared_handler(&self, node: NodeRef, display: Box<str>) -> Target {
        if !self.stack(node) {
            return Target::Failure(Reason::Wrapper, display);
        }
        let binder = self.source().binder;
        let mut ctx = Ctx::new();
        let stable = binder
            .program
            .binding_of(node)
            .is_some_and(|binding| binder.stable(binding, &mut ctx));
        if !stable
            || binder.is_abstract(node, &mut ctx)
            || binder.overload(node, &mut ctx)
            || binder.accessor(node, &mut ctx)
        {
            return Target::Failure(Reason::Unsupported, display);
        }
        match self.source().block_of(node) {
            Some(block) => Target::Block(block, display),
            None => Target::Failure(Reason::Unsupported, display),
        }
    }

    /// A handler expression: a proven reference to a route-decorated `def` keeps that body;
    /// anything else goes through the shared handler policy.
    fn handler(&self, expr: ExprRef) -> Target {
        let source = self.source();
        let binder = source.binder;
        if matches!(
            source.kind(expr),
            Some(ExprKind::Name(_) | ExprKind::Member { .. })
        ) {
            let mut ctx = Ctx::new();
            let value = binder.resolve(expr, &mut ctx);
            let proven = !value.ambiguous
                && !value.overridden
                && value.excluded.is_none()
                && value
                    .refusal
                    .is_none_or(|refusal| refusal == unfold_engine::evidence::Refusal::Wrapper)
                && value.declarations.len() == 1
                && value
                    .binding
                    .is_none_or(|binding| binder.stable(binding, &mut ctx))
                && value
                    .receiver_class
                    .is_none_or(|class| binder.class_concrete(class, &mut ctx));
            if proven {
                let declaration = value.declarations[0];
                let registered = matches!(
                    binder.program.node(declaration).map(|fact| &fact.kind),
                    Some(NodeKind::Function(function)) if function.name.is_some()
                ) && source
                    .decorators(declaration)
                    .into_iter()
                    .any(|decorator| self.registration(Some(decorator)).is_some());
                if registered {
                    return self.declared_handler(declaration, source.text(expr));
                }
            }
        }
        Target::Value(expr)
    }

    /// The `Depends(...)` targets of dependency declarations, in order.
    fn dependencies(&self, declarations: &[ExprRef]) -> Vec<Target> {
        let source = self.source();
        let mut targets = Vec::new();
        for &declaration in declarations {
            let items: Vec<ExprRef> = match source.kind(declaration) {
                Some(ExprKind::Sequence(items)) => items
                    .iter()
                    .map(|&item| ExprRef {
                        file: declaration.file,
                        expr: item,
                    })
                    .collect(),
                _ => vec![declaration],
            };
            for item in items {
                let factory = match source.kind(item) {
                    Some(ExprKind::Call { .. }) => self.origins.trusted_import(source.callee(item)),
                    _ => None,
                };
                if !factory.is_some_and(|factory| DEPENDS.contains(&&*factory)) {
                    targets.push(Target::Failure(Reason::Unsupported, source.text(item)));
                    continue;
                }
                let args = source.args(item);
                let dependency = args.get("dependency", Some(0));
                match dependency {
                    Some(dependency) if !args.uncertain() => targets.push(self.handler(dependency)),
                    _ => targets.push(Target::Failure(Reason::Unsupported, source.text(item))),
                }
            }
        }
        targets
    }

    /// Every path a route is reachable at, through the mounts of its router.
    fn route_paths(&self, route: &Registration, local: Option<&str>) -> Vec<RoutePath> {
        let start = Walk {
            current: route.receiver,
            prefixes: Vec::new(),
            declarations: self.dependency_declarations(Some(route.factory)),
            literal: true,
            seen: Vec::new(),
        };
        let mut paths = Vec::new();
        let mut branches = 0;
        if self.follow(start, local, &mut paths, &mut branches) {
            return paths;
        }
        let mut dependencies = self.dependency_declarations(route.receiver.factory);
        dependencies.extend(self.dependency_declarations(Some(route.factory)));
        vec![RoutePath {
            trigger: local.map_or(Trigger::Unknown, |local| Trigger::Partial(local.into())),
            dependencies,
            binding: None,
        }]
    }

    /// Follows the mounts of `walk`'s router into `paths`; `false` when the route has more than
    /// [`MAX_MOUNT_BRANCHES`] branches (`branches` counts them).
    fn follow(
        &self,
        walk: Walk,
        local: Option<&str>,
        paths: &mut Vec<RoutePath>,
        branches: &mut usize,
    ) -> bool {
        *branches += 1;
        if *branches > MAX_MOUNT_BRANCHES {
            return false;
        }
        let fallback = |declarations: Vec<ExprRef>| RoutePath {
            trigger: local.map_or(Trigger::Unknown, |local| Trigger::Partial(local.into())),
            dependencies: declarations,
            binding: None,
        };
        let Walk {
            current,
            prefixes,
            declarations,
            literal,
            seen,
        } = walk;
        if seen.contains(&current.identity) || seen.len() > MAX_MOUNTS {
            paths.push(fallback(declarations));
            return true;
        }
        let mut all = self.dependency_declarations(current.factory);
        all.extend(declarations);
        if current.spec.kind == "application" {
            let binding = match current.identity {
                Identity::Node(node) => self.source().binder.binding_block(node),
                Identity::Call(_) => None,
            };
            let path = match local {
                None => RoutePath {
                    trigger: Trigger::Unknown,
                    dependencies: all,
                    binding,
                },
                Some(_) if !literal => RoutePath {
                    binding,
                    ..fallback(all)
                },
                Some(local) => {
                    let mut trigger: String = prefixes.concat();
                    trigger.push_str(local);
                    RoutePath {
                        trigger: Trigger::Exact(trigger.into()),
                        dependencies: all,
                        binding,
                    }
                }
            };
            paths.push(path);
            return true;
        }
        let own = self.receiver_prefix(&current);
        let mut next = prefixes;
        if let Some(own) = &own {
            next.insert(0, own.clone());
        }
        let parents = self.mounts.get(&current.identity).map_or(&[][..], Vec::as_slice);
        if current.annotation_only || parents.is_empty() {
            paths.push(fallback(all));
            return true;
        }
        let mut seen = seen;
        seen.push(current.identity);
        for mount in parents {
            let mut prefixes = next.clone();
            if let Some(prefix) = &mount.prefix {
                prefixes.insert(0, prefix.clone());
            }
            let mut declarations = mount.dependencies.clone();
            declarations.extend(all.iter().copied());
            let walk = Walk {
                current: mount.parent,
                prefixes,
                declarations,
                literal: literal && own.is_some() && mount.prefix.is_some(),
                seen: seen.clone(),
            };
            if !self.follow(walk, local, paths, branches) {
                return false;
            }
        }
        true
    }

    fn candidates(&self) -> Vec<EntranceCandidate> {
        let source = self.source();
        let mut candidates = Vec::new();
        for site in source.sites.all() {
            let call = site.call;
            let (route, mut targets) = match source.sites.decorated(call) {
                Some(node) => {
                    let Some(route) = self.registration(Some(call)) else {
                        continue;
                    };
                    let display = source
                        .block_of(node)
                        .and_then(|block| source.binder.program.facts.block(block))
                        .and_then(|block| block.name.clone())
                        .unwrap_or_default();
                    (route, vec![self.declared_handler(node, display)])
                }
                None => {
                    let Some(route) = self.registration(source.callee(call)) else {
                        continue;
                    };
                    let args = source.args(call);
                    let endpoint = args.get("func", Some(0));
                    let targets = if args.uncertain() || args.len() != 1 {
                        args.all()
                            .iter()
                            .map(|argument| Target::Failure(Reason::Unsupported, source.text(argument.expr)))
                            .collect()
                    } else if let Some(endpoint) = endpoint {
                        vec![self.handler(endpoint)]
                    } else {
                        continue;
                    };
                    (route, targets)
                }
            };
            let args = source.args(route.factory);
            let path = args.get("path", Some(0));
            let local = if args.uncertain() {
                None
            } else {
                source.string(path).map(str::to_owned)
            };
            if args.uncertain() {
                targets = targets
                    .into_iter()
                    .map(|target| match target {
                        Target::Block(_, text) => Target::Failure(Reason::Unsupported, text),
                        Target::Value(expr) => Target::Failure(Reason::Unsupported, source.text(expr)),
                        failed => failed,
                    })
                    .collect();
            }
            for path in self.route_paths(&route, local.as_deref()) {
                let mut all = self.dependencies(&path.dependencies);
                all.extend(targets.iter().cloned());
                if all.is_empty() {
                    continue;
                }
                candidates.push(EntranceCandidate {
                    detector: DetectorId::FastapiRoute,
                    kind: EntranceKind::Http,
                    method: Some(route.method),
                    trigger: path.trigger,
                    label: None,
                    evidence: CandidateEvidence::Block(site.block),
                    binding: path.binding,
                    targets: all.into_iter().map(|target| self.candidate(target)).collect(),
                });
            }
        }
        candidates
    }

    fn candidate(&self, target: Target) -> TargetCandidate {
        match target {
            Target::Value(expr) => self.source().handler(expr),
            Target::Block(block, text) => TargetCandidate::Block {
                block,
                text: Some(text),
            },
            Target::Failure(reason, text) => failure(reason, text),
        }
    }
}

/// One step of following a router's mounts.
struct Walk {
    current: Receiver,
    prefixes: Vec<String>,
    declarations: Vec<ExprRef>,
    literal: bool,
    seen: Vec<Identity>,
}
