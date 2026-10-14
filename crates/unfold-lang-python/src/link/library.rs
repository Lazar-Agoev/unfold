//! The standard library's written types ([`crate::stubs`], from typeshed): what an external
//! origin in one of the covered modules denotes, member lookup through the stub classes' C3
//! linearizations, the class an instance of which a call gives, and the names `builtins` binds.
//!
//! A call gives an instance of a class it constructs, or of the class a function's written
//! return type names: through `await` for an `async def` and only without it otherwise, with
//! `Self` read as the instance the function was looked up on. A construction is an instance of
//! exactly its class; a written type is an instance of the class or of any class that derives
//! from it. An instance of `builtins.type` (or of a class deriving from it) is itself a class,
//! and which one the stubs don't say: it is unknown. Members are looked up the way Python looks
//! them up, along the class's linearization (`builtins.object` last). Anything the stubs don't
//! write is unknown; nothing is inferred.

use std::sync::{Arc, LazyLock};

use rustc_hash::FxHashMap;

use super::classes::merge;
use super::value::External;
use crate::bind::ir::Literal;
use crate::stubs::{CLASSES, MODULES, Member, Returns, StubClass, StubModule};

const OBJECT: &str = "builtins.object";
const TYPE: &str = "builtins.type";
const BASE_EXCEPTION: &str = "builtins.BaseException";
/// Builtin exception classes whose `__new__` decides which class an instance is.
const EXCEPTION_GROUPS: &[&str] = &["builtins.BaseExceptionGroup", "builtins.ExceptionGroup"];

/// An instance of a stub class: `exact` when it is an instance of the class itself (a literal, a
/// display, a construction, or what `Self` reads as on one); otherwise of the class or of any
/// class that derives from it (a written type).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Instance {
    pub(crate) class: &'static StubClass,
    pub(crate) exact: bool,
}

/// What the stubs prove a value is.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Stub {
    /// A covered module.
    Module(&'static StubModule),
    /// A class itself.
    Class(&'static StubClass),
    /// An instance of a class.
    Instance(Instance),
    /// A function or method, with the instance it was looked up on (what `Self` reads as).
    Function {
        returns: Returns,
        receiver: Option<Instance>,
    },
}

/// How much the stubs know about an external origin.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Known {
    /// What they prove it is.
    Stub(Stub),
    /// Nothing: a value they write no type for, or a member a class or instance doesn't have.
    Untyped,
    /// It is outside them: a module they don't cover, or a member a covered module doesn't bind
    /// (such as a submodule another import loads).
    Uncovered,
}

/// The C3 linearization of every stub class, by qualified name; `None` for a class whose
/// bases have no consistent order.
static LINEARIZATIONS: LazyLock<FxHashMap<&'static str, Option<Arc<[&'static str]>>>> = LazyLock::new(|| {
    let mut done = FxHashMap::default();
    for class in CLASSES {
        linearize(class, &mut done, &mut Vec::new());
    }
    done
});

fn linearize(
    class: &'static StubClass,
    done: &mut FxHashMap<&'static str, Option<Arc<[&'static str]>>>,
    path: &mut Vec<&'static str>,
) -> Option<Arc<[&'static str]>> {
    if let Some(found) = done.get(class.name) {
        return found.clone();
    }
    if path.contains(&class.name) {
        return None;
    }
    path.push(class.name);
    let mut bases = class.bases.to_vec();
    if bases.is_empty() && class.name != OBJECT {
        bases.push(OBJECT);
    }
    let lists: Option<Vec<Arc<[&'static str]>>> = bases
        .iter()
        .map(|&base| linearize(self::class(base)?, done, path))
        .collect();
    path.pop();
    let found = lists
        .and_then(|lists| merge(class.name, lists, bases))
        .map(Arc::from);
    done.insert(class.name, found.clone());
    found
}

fn module(name: &str) -> Option<&'static StubModule> {
    let index = MODULES.binary_search_by(|module| module.name.cmp(name)).ok()?;
    MODULES.get(index)
}

/// The position of a builtin exception class the binder proves as a base: `BaseException`, or
/// a `builtins` class that derives from it through single bases, but not `BaseExceptionGroup`
/// or `ExceptionGroup` (their `__new__` decides which class an instance is). Each has metaclass
/// `type` and takes its arguments through `BaseException`'s standard construction, so a project
/// class that derives from one constructs its instances like any concrete class.
pub(crate) fn builtin_exception(class: &StubClass) -> Option<u32> {
    let chain = linearization(class);
    let [.., BASE_EXCEPTION, OBJECT] = chain else {
        return None;
    };
    let proven = chain.iter().all(|&name| {
        name.starts_with("builtins.")
            && !EXCEPTION_GROUPS.contains(&name)
            && self::class(name).is_some_and(|found| found.bases.len() <= 1)
    });
    if !proven {
        return None;
    }
    let index = CLASSES
        .binary_search_by(|found| found.name.cmp(class.name))
        .ok()?;
    u32::try_from(index).ok()
}

/// The builtin exception class at a position [`builtin_exception`] gave, and the positions of
/// its chain of bases up to `BaseException` (itself first, `object` left out).
pub(crate) fn exception_chain(index: u32) -> Option<Vec<u32>> {
    let class = CLASSES.get(index as usize)?;
    linearization(class)
        .iter()
        .filter(|&&name| name != OBJECT)
        .map(|&name| builtin_exception(self::class(name)?))
        .collect()
}

/// Whether `builtins` binds `name`: a name nothing in scope binds is a builtin exactly then.
pub(crate) fn builtin(name: &str) -> bool {
    module("builtins").is_some_and(|builtins| member(builtins.members, name).is_some())
}

/// The stub class of a qualified name (`builtins.str`, `logging.Logger`).
pub(crate) fn class(name: &str) -> Option<&'static StubClass> {
    let index = CLASSES.binary_search_by(|class| class.name.cmp(name)).ok()?;
    CLASSES.get(index)
}

fn member(members: &'static [(&'static str, Member)], name: &str) -> Option<Member> {
    let index = members.binary_search_by(|(found, _)| (*found).cmp(name)).ok()?;
    members.get(index).map(|&(_, member)| member)
}

/// The C3 linearization of a stub class (itself first); empty when it has none.
pub(crate) fn linearization(class: &StubClass) -> &'static [&'static str] {
    LINEARIZATIONS
        .get(class.name)
        .and_then(|found| found.as_deref())
        .unwrap_or_default()
}

/// Member `name` of a class or its instances: the first declaration along its linearization.
pub(crate) fn lookup(class: &StubClass, name: &str) -> Option<Member> {
    linearization(class)
        .iter()
        .find_map(|&found| member(self::class(found)?.members, name))
}

/// An instance of `class` (`exact` as for [`Instance`]); `None` when `class` is `builtins.type`
/// or derives from it, whose instances are classes the stubs don't name.
pub(crate) fn instance(class: &'static StubClass, exact: bool) -> Option<Instance> {
    (!linearization(class).contains(&TYPE)).then_some(Instance { class, exact })
}

/// Whether any class may implement the members of an instance of a written `class`, whatever
/// its bases: `builtins.object`, or a protocol.
pub(crate) fn open(class: &StubClass) -> bool {
    class.name == OBJECT || class.protocol
}

/// What a member found on `instance` is.
pub(crate) fn on_instance(instance: Instance, found: Member) -> Option<Stub> {
    match found {
        Member::Function(returns) => Some(Stub::Function {
            returns,
            receiver: Some(instance),
        }),
        Member::Class(name) => class(name).map(Stub::Class),
        Member::Variable(written) => instance_of(written?).map(Stub::Instance),
        Member::SelfVariable => Some(Stub::Instance(instance)),
        Member::Module(_) => None,
    }
}

/// An instance of a written type, by its class's qualified name.
fn instance_of(name: &str) -> Option<Instance> {
    instance(class(name)?, false)
}

/// What an external origin denotes, from its covered module through each member ([`known`]).
pub(crate) fn external(external: &External) -> Option<Stub> {
    match known(external) {
        Known::Stub(stub) => Some(stub),
        Known::Untyped | Known::Uncovered => None,
    }
}

/// How much the stubs know about an external origin, from its module through each member. A
/// class's attribute read on the class itself is untyped: the stubs write what its instances
/// hold. A method looked up on the class reads `Self` as an instance of the class or of any
/// class that derives from it, since its first argument can be either.
pub(crate) fn known(external: &External) -> Known {
    let Some(module) = module(&external.module) else {
        return Known::Uncovered;
    };
    let mut current = Stub::Module(module);
    if &*external.member == "*" {
        return Known::Stub(current);
    }
    for name in external.member.split('.') {
        let next = match current {
            Stub::Module(module) => match member(module.members, name) {
                None => return Known::Uncovered,
                Some(Member::Module(name)) => match self::module(name) {
                    Some(module) => Some(Stub::Module(module)),
                    None => return Known::Uncovered,
                },
                Some(Member::Function(returns)) => Some(Stub::Function {
                    returns,
                    receiver: None,
                }),
                Some(Member::Class(name)) => class(name).map(Stub::Class),
                Some(Member::Variable(written)) => written.and_then(instance_of).map(Stub::Instance),
                Some(Member::SelfVariable) => None,
            },
            Stub::Class(owner) => match lookup(owner, name) {
                Some(Member::Function(returns)) => Some(Stub::Function {
                    returns,
                    receiver: instance(owner, false),
                }),
                Some(Member::Class(name)) => class(name).map(Stub::Class),
                Some(Member::Module(_) | Member::Variable(_) | Member::SelfVariable) | None => None,
            },
            Stub::Instance(owner) => lookup(owner.class, name).and_then(|found| on_instance(owner, found)),
            Stub::Function { .. } => None,
        };
        match next {
            Some(next) => current = next,
            None => return Known::Untyped,
        }
    }
    Known::Stub(current)
}

/// The instance calling `callee` gives (`awaited` when the call is the operand of `await`): an
/// exact instance of a class it constructs, or what the function's written return type says.
pub(crate) fn called(callee: Stub, awaited: bool) -> Option<Instance> {
    match callee {
        Stub::Class(constructed) if !awaited => instance(constructed, true),
        Stub::Function { returns, receiver } => match (returns, awaited) {
            (Returns::Instance(name), false) | (Returns::AwaitInstance(name), true) => instance_of(name),
            (Returns::Receiver, false) | (Returns::AwaitReceiver, true) => receiver,
            _ => None,
        },
        _ => None,
    }
}

/// The builtin class a literal or display makes.
pub(crate) fn literal(literal: Literal) -> Option<&'static StubClass> {
    class(match literal {
        Literal::Str => "builtins.str",
        Literal::Bytes => "builtins.bytes",
        Literal::Int => "builtins.int",
        Literal::Float => "builtins.float",
        Literal::Complex => "builtins.complex",
        Literal::List => "builtins.list",
        Literal::Set => "builtins.set",
        Literal::Dict => "builtins.dict",
    })
}

/// The external origin of member `name` found on an instance of `class`: the class's module,
/// and the member under the class's name (`logging` and `Logger.info`).
pub(crate) fn member_origin(class: &StubClass, name: &str) -> External {
    let (module, short) = class.name.rsplit_once('.').unwrap_or(("builtins", class.name));
    External {
        module: module.into(),
        member: format!("{short}.{name}").into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linearization_of(name: &str) -> Vec<&'static str> {
        LINEARIZATIONS
            .get(name)
            .cloned()
            .flatten()
            .map(|found| found.to_vec())
            .unwrap_or_default()
    }

    #[test]
    fn the_tables_are_sorted_and_every_class_is_linearized() {
        assert!(MODULES.windows(2).all(|pair| pair[0].name < pair[1].name));
        assert!(CLASSES.windows(2).all(|pair| pair[0].name < pair[1].name));
        for class in CLASSES {
            assert!(
                class.members.windows(2).all(|pair| pair[0].0 < pair[1].0),
                "{}",
                class.name
            );
            let found = linearization_of(class.name);
            assert_eq!(found.first(), Some(&class.name));
            assert_eq!(found.last(), Some(&OBJECT), "{}", class.name);
        }
    }

    #[test]
    fn lookup_follows_the_linearization() {
        assert_eq!(
            linearization_of("logging.Logger"),
            ["logging.Logger", "logging.Filterer", OBJECT]
        );
        let logger = class("logging.Logger").unwrap();
        assert_eq!(lookup(logger, "info"), Some(Member::Function(Returns::Unknown)));
        assert!(lookup(logger, "filter").is_some(), "inherited from Filterer");
        assert!(lookup(logger, "__repr__").is_some(), "inherited from object");
        assert_eq!(lookup(logger, "no_such_member"), None);
        let datetime = class("datetime.datetime").unwrap();
        assert!(matches!(lookup(datetime, "isoformat"), Some(Member::Function(_))));
        assert!(
            matches!(lookup(datetime, "weekday"), Some(Member::Function(_))),
            "from date"
        );
    }

    #[test]
    fn external_origins_walk_the_covered_modules() {
        let origin = |module: &str, member: &str| External {
            module: module.into(),
            member: member.into(),
        };
        let returned = |module, member| {
            external(&origin(module, member))
                .and_then(|stub| called(stub, false))
                .map(|instance| (instance.class.name, instance.exact))
        };
        // A construction is exact; a written return type is not.
        assert_eq!(returned("logging", "getLogger"), Some(("logging.Logger", false)));
        assert_eq!(returned("re", "compile"), Some(("re.Pattern", false)));
        assert_eq!(returned("pathlib", "Path"), Some(("pathlib.Path", true)));
        assert_eq!(
            returned("datetime", "datetime.now"),
            Some(("datetime.datetime", false))
        );
        assert_eq!(returned("builtins", "str"), Some(("builtins.str", true)));
        assert_eq!(
            returned("os", "path.basename"),
            None,
            "str and bytes overloads disagree"
        );
        assert!(
            matches!(external(&origin("os", "path")), Some(Stub::Module(found)) if found.name == "os.path")
        );
        assert!(matches!(external(&origin("os", "*")), Some(Stub::Module(_))));
        // A class's attribute read on the class is unknown; a module's variable is an instance.
        assert!(external(&origin("datetime", "datetime.year")).is_none());
        assert!(
            matches!(external(&origin("datetime", "UTC")), Some(Stub::Instance(found)) if found.class.name == "datetime.timezone")
        );
        assert!(
            external(&origin("logging.handlers", "RotatingFileHandler")).is_none(),
            "not covered"
        );
        assert!(external(&origin("fastapi", "FastAPI")).is_none());
        assert!(external(&origin("logging", "no_such_member")).is_none());
    }

    #[test]
    fn calls_read_await_only_on_async_functions() {
        let task = class("_asyncio.Task").unwrap();
        let receiver = Instance {
            class: task,
            exact: false,
        };
        let sync = Stub::Function {
            returns: Returns::Instance("builtins.str"),
            receiver: None,
        };
        let asynchronous = Stub::Function {
            returns: Returns::AwaitReceiver,
            receiver: Some(receiver),
        };
        assert_eq!(
            called(sync, false).map(|instance| instance.class.name),
            Some("builtins.str")
        );
        assert!(called(sync, true).is_none());
        assert_eq!(
            called(asynchronous, true).map(|instance| instance.class.name),
            Some("_asyncio.Task")
        );
        assert!(called(asynchronous, false).is_none());
        assert!(called(Stub::Class(task), true).is_none());
    }

    #[test]
    fn instance_members_take_their_class_module() {
        let logger = class("logging.Logger").unwrap();
        let origin = member_origin(logger, "info");
        assert_eq!((&*origin.module, &*origin.member), ("logging", "Logger.info"));
        let pattern = class("asyncio.events.AbstractEventLoop").unwrap();
        let origin = member_origin(pattern, "run_until_complete");
        assert_eq!(&*origin.module, "asyncio.events");
    }

    #[test]
    fn instances_of_metaclasses_are_unknown() {
        let metaclass = class("builtins.type").unwrap();
        assert!(instance(metaclass, false).is_none());
        assert!(
            called(Stub::Class(metaclass), false).is_none(),
            "`type(x)` is a class"
        );
        assert!(instance(class("builtins.str").unwrap(), false).is_some());
    }

    #[test]
    fn known_tells_untyped_values_from_uncovered_modules() {
        let known_at = |module: &str, member: &str| {
            known(&External {
                module: module.into(),
                member: member.into(),
            })
        };
        // `Match.string` is written `AnyStr`; `Path` has no `no_such`.
        assert!(matches!(known_at("re", "Match.string"), Known::Untyped));
        assert!(matches!(known_at("pathlib", "Path.no_such"), Known::Untyped));
        assert!(matches!(known_at("os", "getcwd.no_such"), Known::Untyped));
        // A covered module's unbound member may be a submodule loaded elsewhere.
        assert!(matches!(known_at("asyncio", "no_such.x"), Known::Uncovered));
        assert!(matches!(known_at("fastapi", "FastAPI"), Known::Uncovered));
        assert!(
            matches!(known_at("os", "environ"), Known::Stub(Stub::Instance(found)) if found.class.name == "os._Environ")
        );
    }

    #[test]
    fn object_and_protocols_are_open() {
        assert!(open(class(OBJECT).unwrap()));
        assert!(open(class("os.PathLike").unwrap()));
        assert!(!open(class("logging.Logger").unwrap()));
        assert!(!open(class("builtins.dict").unwrap()));
    }

    #[test]
    fn builtins_are_the_names_the_builtins_stub_binds() {
        for name in [
            "ord",
            "id",
            "hash",
            "divmod",
            "frozenset",
            "chr",
            "vars",
            "bytearray",
            "__import__",
        ] {
            assert!(builtin(name), "{name}");
        }
        // `function` is a stub-only class; `reveal_type` is a checker's.
        for name in ["function", "reveal_type", "no_such_name"] {
            assert!(!builtin(name), "{name}");
        }
    }

    #[test]
    fn builtin_exceptions_reach_base_exception_through_single_bases() {
        let index = |name: &str| builtin_exception(class(name).unwrap());
        let names = |index: u32| -> Vec<&str> {
            exception_chain(index)
                .unwrap()
                .into_iter()
                .map(|found| CLASSES[found as usize].name)
                .collect()
        };
        let unicode = index("builtins.UnicodeDecodeError").unwrap();
        assert_eq!(
            names(unicode),
            [
                "builtins.UnicodeDecodeError",
                "builtins.UnicodeError",
                "builtins.ValueError",
                "builtins.Exception",
                "builtins.BaseException",
            ]
        );
        assert!(index("builtins.ExceptionGroup").is_none());
        assert!(index("builtins.BaseExceptionGroup").is_none());
        assert!(index("builtins.str").is_none());
        assert!(index("json.decoder.JSONDecodeError").is_none(), "not in builtins");
        let count = CLASSES
            .iter()
            .filter(|found| builtin_exception(found).is_some())
            .count();
        assert_eq!(count, 65, "every builtin exception of Python 3.12");
    }
}
