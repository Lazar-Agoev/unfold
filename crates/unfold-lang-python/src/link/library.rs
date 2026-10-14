//! The standard library's written types ([`crate::stubs`], from typeshed): what an external
//! origin in one of the covered modules denotes, member lookup through the stub classes' C3
//! linearizations, and the class an instance of which a call gives.
//!
//! A call gives an instance of a class it constructs, or of the class a function's written
//! return type names: through `await` for an `async def` and only without it otherwise, with
//! `Self` read as the class the function was looked up on. Members are looked up the way Python
//! looks them up, along the class's linearization (`builtins.object` last). Anything the stubs
//! don't write is unknown; nothing is inferred.

use std::sync::{Arc, LazyLock};

use rustc_hash::FxHashMap;

use super::classes::merge;
use super::value::External;
use crate::bind::ir::Literal;
use crate::stubs::{CLASSES, MODULES, Member, Returns, StubClass, StubModule};

const OBJECT: &str = "builtins.object";

/// What the stubs prove a value is.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Stub {
    /// A covered module.
    Module(&'static StubModule),
    /// A class itself.
    Class(&'static StubClass),
    /// An instance of a class.
    Instance(&'static StubClass),
    /// A function or method, with the class it was looked up on (what `Self` reads as).
    Function {
        returns: Returns,
        receiver: Option<&'static StubClass>,
    },
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

/// The stub class of a qualified name (`builtins.str`, `logging.Logger`).
pub(crate) fn class(name: &str) -> Option<&'static StubClass> {
    let index = CLASSES.binary_search_by(|class| class.name.cmp(name)).ok()?;
    CLASSES.get(index)
}

fn member(members: &'static [(&'static str, Member)], name: &str) -> Option<Member> {
    let index = members.binary_search_by(|(found, _)| (*found).cmp(name)).ok()?;
    members.get(index).map(|&(_, member)| member)
}

/// Member `name` of a class or its instances: the first declaration along its linearization.
pub(crate) fn lookup(class: &StubClass, name: &str) -> Option<Member> {
    LINEARIZATIONS
        .get(class.name)?
        .as_deref()?
        .iter()
        .find_map(|&found| member(self::class(found)?.members, name))
}

/// What a member found on an instance of `class` is.
pub(crate) fn on_instance(class: &'static StubClass, found: Member) -> Option<Stub> {
    match found {
        Member::Function(returns) => Some(Stub::Function {
            returns,
            receiver: Some(class),
        }),
        Member::Class(name) => self::class(name).map(Stub::Class),
        Member::Variable(written) => self::class(written?).map(Stub::Instance),
        Member::Module(_) => None,
    }
}

/// What an external origin denotes, from its covered module through each member. A class's
/// attribute read on the class itself is unknown: the stubs write what its instances hold.
pub(crate) fn external(external: &External) -> Option<Stub> {
    let mut current = Stub::Module(module(&external.module)?);
    if &*external.member == "*" {
        return Some(current);
    }
    for name in external.member.split('.') {
        current = match current {
            Stub::Module(module) => match member(module.members, name)? {
                Member::Function(returns) => Stub::Function {
                    returns,
                    receiver: None,
                },
                Member::Class(name) => Stub::Class(class(name)?),
                Member::Module(name) => Stub::Module(self::module(name)?),
                Member::Variable(written) => Stub::Instance(class(written?)?),
            },
            Stub::Class(owner) => match lookup(owner, name)? {
                Member::Function(returns) => Stub::Function {
                    returns,
                    receiver: Some(owner),
                },
                Member::Class(name) => Stub::Class(class(name)?),
                Member::Module(_) | Member::Variable(_) => return None,
            },
            Stub::Instance(owner) => on_instance(owner, lookup(owner, name)?)?,
            Stub::Function { .. } => return None,
        };
    }
    Some(current)
}

/// The class an instance of which calling `callee` gives (`awaited` when the call is the
/// operand of `await`).
pub(crate) fn called(callee: Stub, awaited: bool) -> Option<&'static StubClass> {
    match callee {
        Stub::Class(constructed) if !awaited => Some(constructed),
        Stub::Function { returns, receiver } => match (returns, awaited) {
            (Returns::Instance(name), false) | (Returns::AwaitInstance(name), true) => class(name),
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

    fn linearization(name: &str) -> Vec<&'static str> {
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
            let found = linearization(class.name);
            assert_eq!(found.first(), Some(&class.name));
            assert_eq!(found.last(), Some(&OBJECT), "{}", class.name);
        }
    }

    #[test]
    fn lookup_follows_the_linearization() {
        assert_eq!(
            linearization("logging.Logger"),
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
                .map(|class| class.name)
        };
        assert_eq!(returned("logging", "getLogger"), Some("logging.Logger"));
        assert_eq!(returned("re", "compile"), Some("re.Pattern"));
        assert_eq!(returned("pathlib", "Path"), Some("pathlib.Path"));
        assert_eq!(returned("datetime", "datetime.now"), Some("datetime.datetime"));
        assert_eq!(returned("builtins", "str"), Some("builtins.str"));
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
            matches!(external(&origin("datetime", "UTC")), Some(Stub::Instance(found)) if found.name == "datetime.timezone")
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
        let sync = Stub::Function {
            returns: Returns::Instance("builtins.str"),
            receiver: None,
        };
        let asynchronous = Stub::Function {
            returns: Returns::AwaitReceiver,
            receiver: Some(task),
        };
        assert_eq!(called(sync, false).map(|class| class.name), Some("builtins.str"));
        assert!(called(sync, true).is_none());
        assert_eq!(
            called(asynchronous, true).map(|class| class.name),
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
}
