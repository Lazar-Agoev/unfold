//! Python typed records: signatures, parameters, members, arguments with their pairings, call
//! results, and type links, through full compiles that pass validation against the snapshots
//! and the Python census.
//!
//! The fixture snapshot, `testdata/fixtures/typed-lines/expected/python.jsonl`, holds the
//! canonical export's typed records (`signature`, `parameter`, `member`, `argument`, `result`,
//! and `type_link` lines, in export order) of the fixture's Python files, compiled with the
//! engine on a temporary copy of the whole fixture folder. `UNFOLD_BLESS=1` rewrites it; every
//! change to it is reviewed by hand.

#![allow(clippy::unwrap_used)]

mod common;

use common::{Compiled, canonical_records, compile_files, compile_folder, testdata};
use unfold_core::typed::{Argument, CallResult, Member, Pairing, Parameter, TypeLink};
use unfold_core::vocab::{
    Expansion, MemberKind, PairingReason, ParameterKind, Reason, Resolution, ResultBinding,
};
use unfold_core::{FileGraph, Key};
use unfold_engine::settings::Settings;

const TYPED: &[&str] = &[
    "signature",
    "parameter",
    "member",
    "argument",
    "result",
    "type_link",
];

fn compile(sources: &[(&str, &str)]) -> Compiled {
    compile_files(sources, &Settings::default()).valid()
}

/// Helpers over one compiled file.
struct File<'a> {
    compiled: &'a Compiled,
    path: &'a str,
}

impl<'a> File<'a> {
    fn new(compiled: &'a Compiled, path: &'a str) -> Self {
        Self { compiled, path }
    }

    fn graph(&self) -> &'a FileGraph {
        self.compiled.file(self.path)
    }

    fn slice(&self, range: unfold_core::ByteRange) -> &'a str {
        &self.compiled.text(self.path)[range.start as usize..range.end as usize]
    }

    /// The local ID of the block keyed `key` (without the path).
    fn local(&self, key: &str) -> u32 {
        let full = format!("{}#{key}", self.path);
        self.graph()
            .blocks
            .iter()
            .find(|block| block.key.as_str() == full)
            .unwrap_or_else(|| {
                panic!(
                    "no block {key} in {:#?}",
                    self.graph()
                        .blocks
                        .iter()
                        .map(|block| block.key.as_str())
                        .collect::<Vec<_>>()
                )
            })
            .local_id
    }

    /// The key of the block `local`, without the path.
    fn key(&self, local: u32) -> &'a str {
        let key = self.graph().blocks[local as usize].key.as_str();
        &key[key.find('#').map_or(0, |at| at + 1)..]
    }

    /// The parameters of the block keyed `key`: (kind, name, default, written type).
    fn parameters(&self, key: &str) -> Vec<(ParameterKind, &'a str, bool, Option<&'a str>)> {
        let local = self.local(key);
        self.graph()
            .types
            .parameters
            .iter()
            .filter(|parameter| parameter.block == local)
            .map(|parameter: &'a Parameter| {
                (
                    parameter.kind,
                    parameter.name.as_deref().unwrap_or_default(),
                    parameter.default,
                    parameter.ty.as_ref().map(|ty| self.slice(ty.range)),
                )
            })
            .collect()
    }

    /// The written return type of the block keyed `key`.
    fn returns(&self, key: &str) -> Option<&'a str> {
        let local = self.local(key);
        let signature = self
            .graph()
            .types
            .signatures
            .iter()
            .find(|signature| signature.block == local)
            .unwrap_or_else(|| panic!("no signature for {key}"));
        signature.returns.as_ref().map(|returns| {
            assert_eq!(&*returns.text, self.slice(returns.range));
            &*returns.text
        })
    }

    /// The members of the class keyed `key`: (name, default, written type).
    fn members(&self, key: &str) -> Vec<(&'a str, bool, &'a str)> {
        let local = self.local(key);
        self.graph()
            .types
            .members
            .iter()
            .filter(|member| member.block == local)
            .map(|member: &'a Member| {
                assert_eq!(member.kind, MemberKind::Property);
                (
                    member.name.as_deref().unwrap_or_default(),
                    member.default,
                    member.ty.as_ref().map_or("", |ty| self.slice(ty.range)),
                )
            })
            .collect()
    }

    /// The result of the call whose occurrence edge reads `text` (the `nth` such call).
    fn result(&self, text: &str, nth: usize) -> &'a CallResult {
        let call = self.call(text, nth);
        self.graph()
            .types
            .results
            .iter()
            .find(|result| result.call == call)
            .unwrap_or_else(|| panic!("no result for {text}"))
    }

    /// The block of the `nth` call whose occurrence edge reads `text`.
    fn call(&self, text: &str, nth: usize) -> u32 {
        self.graph()
            .edges
            .iter()
            .filter(|edge| edge.seq == 0 && &*edge.text == text)
            .nth(nth)
            .unwrap_or_else(|| panic!("no call `{text}` #{nth}"))
            .source
    }

    /// The pairings of the `nth` call whose occurrence edge reads `text`, with each argument's
    /// expansion and keyword.
    fn arguments(&self, text: &str, nth: usize) -> Vec<&'a Argument> {
        let call = self.call(text, nth);
        self.graph()
            .types
            .arguments
            .iter()
            .filter(|argument| argument.call == call)
            .collect()
    }

    fn pairings(&self, text: &str, nth: usize) -> Vec<Option<Pairing>> {
        self.arguments(text, nth)
            .into_iter()
            .map(|argument| argument.pairing)
            .collect()
    }

    /// Every type link as (written text, resolution, target key or module or reason).
    fn links(&self) -> Vec<(&'a str, Resolution, String)> {
        self.graph()
            .types
            .type_links
            .iter()
            .map(|link: &'a TypeLink| {
                assert_eq!(&*link.text, self.slice(link.range));
                let outcome = match link.resolution {
                    Resolution::Resolved => link.target.as_ref().map(Key::to_string).unwrap_or_default(),
                    Resolution::External => link
                        .module
                        .as_ref()
                        .map(|module| module.as_str().to_owned())
                        .unwrap_or_default(),
                    Resolution::Unresolved => {
                        link.reason.map(|reason| reason.to_string()).unwrap_or_default()
                    }
                };
                (self.slice(link.range), link.resolution, outcome)
            })
            .collect()
    }

    /// The links whose text is `text`.
    fn link(&self, text: &str) -> Vec<(Resolution, String)> {
        self.links()
            .into_iter()
            .filter(|(written, ..)| *written == text)
            .map(|(_, resolution, outcome)| (resolution, outcome))
            .collect()
    }
}

use Pairing::{Parameter as P, Unpaired as U};
use PairingReason as R;

fn external(module: &str) -> (Resolution, String) {
    (Resolution::External, module.to_owned())
}

fn unresolved(reason: Reason) -> (Resolution, String) {
    (Resolution::Unresolved, reason.to_string())
}

fn resolved(key: &str) -> (Resolution, String) {
    (Resolution::Resolved, key.to_owned())
}

/// The fixture's typed records, compiled with the engine and validated with the census, equal
/// the reviewed snapshot.
#[test]
fn the_typed_lines_fixture_matches_its_snapshot() {
    let compiled = compile_folder(&testdata().join("fixtures/typed-lines")).valid();
    assert!(compiled.failures.is_empty(), "{:?}", compiled.failures);
    let export = unfold_core::canonical::to_string(&compiled.graph);
    let python: Vec<String> = canonical_records(&export, TYPED)
        .into_iter()
        .filter(|record| record.contains("\"py/"))
        .collect();
    assert!(!python.is_empty());
    let mut snapshot = python.join("\n");
    snapshot.push('\n');
    unfold_core::testkit::assert_golden("fixtures/typed-lines/expected/python.jsonl", &snapshot);
}

#[test]
fn parameters_follow_ruffs_groups_in_order() {
    let source = "def f(a, b=1, /, c: int = 2, *args: str, d, e: \"Cart\" = None, **kwargs: int) -> None:\n    pass\n\n\ndef g(a, *, b):\n    pass\n\n\nh = lambda x, *ys, z=1, **kw: x\n";
    let compiled = compile(&[("main.py", source)]);
    let file = File::new(&compiled, "main.py");
    use ParameterKind::{KeywordOnly, Keywords, Positional, PositionalOnly, Rest};
    assert_eq!(
        file.parameters("function:f"),
        [
            (PositionalOnly, "a", false, None),
            (PositionalOnly, "b", true, None),
            (Positional, "c", true, Some("int")),
            (Rest, "args", false, Some("str")),
            (KeywordOnly, "d", false, None),
            (KeywordOnly, "e", true, Some("\"Cart\"")),
            (Keywords, "kwargs", false, Some("int")),
        ]
    );
    assert_eq!(file.returns("function:f"), Some("None"));
    assert_eq!(
        file.parameters("function:g"),
        [(Positional, "a", false, None), (KeywordOnly, "b", false, None)]
    );
    assert_eq!(file.returns("function:g"), None);
    let lambda = "variable:h/function[0]";
    assert_eq!(
        file.parameters(lambda),
        [
            (Positional, "x", false, None),
            (Rest, "ys", false, None),
            (KeywordOnly, "z", true, None),
            (Keywords, "kw", false, None),
        ]
    );
    assert_eq!(file.returns(lambda), None);
}

#[test]
fn members_are_annotated_names_written_directly_in_class_bodies() {
    let source = r#"from dataclasses import dataclass
from typing import ClassVar, NamedTuple, TypedDict

from pydantic import BaseModel


@dataclass
class Point:
    x: int
    y: int = 0
    label = "p"

    def norm(self) -> float:
        return 0.0

    class Inner:
        z: int

    if True:
        w: int
    (v): int = 0
    other: "Point | None" = None


class Movie(TypedDict):
    title: str


class User(BaseModel):
    name: str = "x"


class Pair(NamedTuple):
    left: int
    right: int


class Plain:
    count: ClassVar[int] = 0

    def __init__(self) -> None:
        self.value: int = 1
"#;
    let compiled = compile(&[("main.py", source)]);
    let file = File::new(&compiled, "main.py");
    assert_eq!(
        file.members("class:Point"),
        [
            ("x", false, "int"),
            ("y", true, "int"),
            ("other", true, "\"Point | None\"")
        ],
        "unannotated assignments, methods, nested classes, nested statements, and parenthesized \
         targets are not members"
    );
    assert_eq!(file.members("class:Point/class:Inner"), [("z", false, "int")]);
    assert_eq!(file.members("class:Movie"), [("title", false, "str")]);
    assert_eq!(file.members("class:User"), [("name", true, "str")]);
    assert_eq!(
        file.members("class:Pair"),
        [("left", false, "int"), ("right", false, "int")]
    );
    assert_eq!(file.members("class:Plain"), [("count", true, "ClassVar[int]")]);
    // A string that is not a dotted name records no link; an attribute's annotation anywhere
    // records its links.
    let links: Vec<&str> = file.links().into_iter().map(|(text, ..)| text).collect();
    assert!(!links.iter().any(|text| text.contains("Point")), "{links:?}");
    let value = file.local("class:Plain/constructor:__init__/variable:self.value");
    assert!(
        file.graph()
            .types
            .type_links
            .iter()
            .any(|link| link.block == value && &*link.text == "int")
    );
}

#[test]
fn results_say_where_each_call_lands() {
    let source = r#"async def run(x):
    a = f()
    b = c = g()
    d: int = h()
    if (e := k()):
        pass
    x += m()
    outer(p(), key=q(), *r(), **s())
    t()
    (u())
    v().w
    lam = lambda: y()
    o = await aw()
    await (await aw2())
    print((await aw3()).value)
    raise Problem()
    return await n()
"#;
    let compiled = compile(&[("main.py", source)]);
    let file = File::new(&compiled, "main.py");
    let landed = |text: &str| {
        let result = file.result(text, 0);
        (
            result.binding,
            result.target.map(|target| file.key(target)),
            result.position,
            result.awaited,
        )
    };
    use ResultBinding::{Argument, Discard, Other, Return, Variable};
    assert_eq!(
        landed("f"),
        (Variable, Some("function:run/variable:a"), None, false)
    );
    assert_eq!(
        landed("g"),
        (Variable, Some("function:run/variable:c/variable:b"), None, false),
        "a chain's innermost level is initialized by the value"
    );
    assert_eq!(
        landed("h"),
        (Variable, Some("function:run/variable:d"), None, false)
    );
    assert_eq!(
        landed("k"),
        (Variable, Some("function:run/if[0]/variable:e"), None, false),
        "an assignment expression"
    );
    assert_eq!(landed("m"), (Other, None, None, false), "an augmented assignment");
    let outer = Some("function:run/expression_statement[0]/call[0]");
    assert_eq!(landed("p"), (Argument, outer, Some(0), false));
    assert_eq!(
        landed("q"),
        (Argument, outer, Some(1), false),
        "a keyword's value"
    );
    assert_eq!(landed("r"), (Other, None, None, false), "`*f()`");
    assert_eq!(landed("s"), (Other, None, None, false), "`**f()`");
    assert_eq!(landed("outer"), (Discard, None, None, false));
    assert_eq!(landed("t"), (Discard, None, None, false));
    assert_eq!(
        landed("u"),
        (Discard, None, None, false),
        "parentheses are transparent"
    );
    assert_eq!(landed("v"), (Other, None, None, false), "a receiver");
    assert_eq!(landed("y"), (Return, None, None, false), "a lambda's body");
    assert_eq!(
        landed("aw"),
        (Variable, Some("function:run/variable:o"), None, true)
    );
    assert_eq!(landed("aw2"), (Discard, None, None, true), "nested awaits");
    assert_eq!(landed("aw3"), (Other, None, None, true), "an awaited receiver");
    assert_eq!(landed("print"), (Discard, None, None, false));
    assert_eq!(landed("Problem"), (Other, None, None, false), "a raise");
    assert_eq!(landed("n"), (Return, None, None, true));
}

#[test]
fn pairing_follows_pythons_calling_rules() {
    let source = r#"def f(a, b, /, c, *, d, **rest):
    pass


def g(a, *args):
    pass


def h(a, b=2):
    pass


def p(a, /, b):
    pass


class Gateway:
    def __init__(self, key, *, retries=3):
        self.key = key

    def charge(self, amount, currency="usd"):
        return amount

    @staticmethod
    def fee(amount):
        return amount

    @classmethod
    def build(cls, key):
        return key

    def use(self):
        self.charge(1)
        self.build("k")

    @classmethod
    def make(cls):
        return cls.build("k")


class Base:
    def pay(self, amount):
        return amount


class Child(Base):
    def pay(self, amount):
        return super().pay(amount)


class Registry:
    @classmethod
    def create(cls, key):
        return key

    default = create("k")


def main(items, options):
    f(1, 2, 3, d=4, e=5)
    f(1, 2, c=3, d=4, b=5)
    p(a=1, b=2)
    g(1, *items, 3)
    h(1, **options, b=3)
    h(1, 2, 3)
    h(1, a=2)
    gateway = Gateway("key", retries=1)
    gateway.charge(10, currency="eur")
    Gateway.charge(gateway, 10)
    Gateway.fee(5)
    gateway.fee(5)
    Gateway.build("k")
    gateway.build("k")
    callback = lambda x, y=1: x
    callback(1, y=2)
"#;
    let compiled = compile(&[("main.py", source)]);
    let file = File::new(&compiled, "main.py");
    let pairs = |text: &str, nth: usize| file.pairings(text, nth);
    // Positional arguments fill positions, then `**rest` takes unknown keywords.
    assert_eq!(
        pairs("f", 0),
        [Some(P(0)), Some(P(1)), Some(P(2)), Some(P(3)), Some(P(4))]
    );
    // A keyword for a positional-only parameter falls to `**rest`.
    assert_eq!(
        pairs("f", 1),
        [Some(P(0)), Some(P(1)), Some(P(2)), Some(P(3)), Some(P(4))]
    );
    // Without `**kwargs`, a positional-only parameter named by keyword takes nothing.
    assert_eq!(pairs("p", 0), [Some(U(R::NoParameter)), Some(P(1))]);
    // `*args` takes the rest; a spread makes itself and later positions spread.
    assert_eq!(
        pairs("g", 0),
        [Some(P(0)), Some(U(R::Spread)), Some(U(R::Spread))]
    );
    // A `**` expansion is spread itself; keywords after it still pair by name.
    assert_eq!(pairs("h", 0), [Some(P(0)), Some(U(R::Spread)), Some(P(1))]);
    assert_eq!(pairs("h", 1), [Some(P(0)), Some(P(1)), Some(U(R::NoParameter))]);
    assert_eq!(pairs("h", 2), [Some(P(0)), Some(U(R::NoParameter))]);
    // `__init__` reached by a construction binds the instance.
    assert_eq!(pairs("Gateway", 0), [Some(P(1)), Some(P(2))]);
    // Through an instance (a variable or `self`), the receiver is bound; through the class, not.
    assert_eq!(pairs("gateway.charge", 0), [Some(P(1)), Some(P(2))]);
    assert_eq!(pairs("self.charge", 0), [Some(P(1))]);
    assert_eq!(pairs("Gateway.charge", 0), [Some(P(0)), Some(P(1))]);
    // A static method binds nothing, however it is reached.
    assert_eq!(pairs("Gateway.fee", 0), [Some(P(0))]);
    assert_eq!(pairs("gateway.fee", 0), [Some(P(0))]);
    // A class method binds the class through the class, `cls`, or an instance.
    assert_eq!(pairs("Gateway.build", 0), [Some(P(1))]);
    assert_eq!(pairs("gateway.build", 0), [Some(P(1))]);
    assert_eq!(pairs("self.build", 0), [Some(P(1))]);
    assert_eq!(pairs("cls.build", 0), [Some(P(1))]);
    // `super().m()` binds the instance.
    assert_eq!(pairs("super().pay", 0), [Some(P(1))]);
    // A class method called by its bare name: whether it binds anything is not proven.
    assert_eq!(pairs("create", 0), [Some(U(R::Receiver))]);
    // A lambda is a plain function.
    assert_eq!(pairs("callback", 0), [Some(P(0)), Some(P(1))]);
    // Expansions and keywords are recorded as written.
    let g = file.arguments("g", 0);
    assert_eq!(g[1].expand, Some(Expansion::Spread));
    let h = file.arguments("h", 0);
    assert_eq!(h[1].expand, Some(Expansion::Keywords));
    assert_eq!(h[2].keyword.as_deref(), Some("b"));
}

#[test]
fn type_links_resolve_in_the_type_meaning() {
    let models = r#"from dataclasses import dataclass
from decimal import Decimal
from typing import TypeVar

from pydantic import BaseModel

Money = Decimal
T = TypeVar("T")


@dataclass
class Cart:
    total: Money


class Outer:
    class Inner:
        pass


def helper():
    pass
"#;
    let main = r#"import typing

import models
from typing import Annotated, Literal, Optional
from pydantic import BaseModel
from models import Cart, Money, Outer, T, helper


def f(a: Cart, b: int, c: list[dict[str, Cart]], d: typing.Optional[Cart], e: Optional[int]) -> BaseModel:
    pass


def g(a: Money, b: T, c: "Cart", d: "models.Cart", e: "not a name", f: Literal["x", Cart], g: Annotated[Cart, Cart]) -> models.Cart:
    pass


def h[K](a: K, b: Unknown, c: helper, d: Outer.Inner, e: models.Missing) -> None:
    x: "Cart | None" = None
    y: K | None = None
"#;
    let compiled = compile(&[("models.py", models), ("main.py", main)]);
    let file = File::new(&compiled, "main.py");
    let cart = resolved("models.py#class:Cart");
    let builtins = external("python:builtins");
    let typing = external("python:typing");
    // `a`, inside `c` and `d`, the string `"Cart"`, and `Annotated`'s first argument; not
    // `Literal`'s.
    assert_eq!(file.link("Cart").len(), 5, "{:#?}", file.links());
    assert!(
        file.link("Cart").iter().all(|link| *link == cart),
        "a dataclass is its type"
    );
    assert_eq!(file.link("int"), [builtins.clone(), builtins.clone()]);
    for name in ["list", "dict", "str"] {
        assert_eq!(file.link(name), std::slice::from_ref(&builtins), "{name}");
    }
    assert_eq!(file.link("typing.Optional"), std::slice::from_ref(&typing));
    assert_eq!(file.link("Optional"), std::slice::from_ref(&typing));
    assert_eq!(file.link("Literal"), std::slice::from_ref(&typing));
    assert_eq!(file.link("Annotated"), [typing]);
    assert_eq!(file.link("BaseModel"), [external("pydantic")]);
    assert_eq!(file.link("Money"), [unresolved(Reason::NotConcrete)]);
    assert_eq!(file.link("T"), [unresolved(Reason::NotConcrete)]);
    assert_eq!(
        file.link("helper"),
        [unresolved(Reason::NotConcrete)],
        "a function is no type"
    );
    assert_eq!(file.link("models.Cart"), [cart.clone(), cart]);
    assert_eq!(
        file.link("Outer.Inner"),
        [resolved("models.py#class:Outer/class:Inner")]
    );
    assert_eq!(file.link("Unknown"), [unresolved(Reason::UnknownName)]);
    assert_eq!(file.link("models.Missing"), [unresolved(Reason::UnknownReceiver)]);
    assert!(
        file.link("K").is_empty(),
        "a PEP 695 type parameter is never linked"
    );
    assert!(
        !file
            .links()
            .iter()
            .any(|(text, ..)| text.contains(' ') || text.contains('"')),
        "strings other than dotted names record nothing: {:#?}",
        file.links()
    );
    // A string forward reference's range lies inside its quotes.
    let text = compiled.text("main.py");
    let quoted = text.find("\"Cart\"").unwrap() as u32;
    assert!(
        file.graph()
            .types
            .type_links
            .iter()
            .any(|link| link.range.start == quoted + 1 && link.range.end == quoted + 5)
    );
    // Nothing links in `Literal[...]`'s arguments or after `Annotated[...]`'s first.
    let annotated = text.find("Annotated[Cart, Cart]").unwrap() as u32;
    let second = annotated + "Annotated[Cart, ".len() as u32;
    assert!(
        !file
            .graph()
            .types
            .type_links
            .iter()
            .any(|link| link.range.start == second)
    );
}

#[test]
fn type_links_stay_conservative() {
    let source = r#"from shapes import *


class Cart:
    pass


class Cart:
    pass


class Order:
    pass


def f(a: Cart, b: Shape, c: Order) -> "Order":
    pass
"#;
    let compiled = compile(&[("main.py", source)]);
    let file = File::new(&compiled, "main.py");
    // A star import whose names are unknown makes every name of its scope ambiguous, as it
    // does for values: bound here or not.
    for name in ["Cart", "Shape"] {
        assert_eq!(file.link(name), [unresolved(Reason::Ambiguous)], "{name}");
    }
    // `c: Order` and the string `"Order"`.
    assert_eq!(
        file.link("Order"),
        [unresolved(Reason::Ambiguous), unresolved(Reason::Ambiguous)]
    );
    let rebound = compile(&[(
        "main.py",
        "class Cart:\n    pass\n\n\nclass Cart:\n    pass\n\n\ndef f(a: Cart) -> None:\n    pass\n",
    )]);
    assert_eq!(
        File::new(&rebound, "main.py").link("Cart"),
        [unresolved(Reason::MutableBinding)]
    );
}

/// The bound count of a call (linking's proven answer) and of each passed callable.
#[test]
fn bound_counts_are_recorded_where_they_are_proven() {
    let source = r#"class Gateway:
    def ping(self):
        return True

    def apply(self, value):
        return value

    @classmethod
    def build(cls, key):
        return cls()

    @staticmethod
    def fee(amount):
        return amount


def run(fn, *args):
    return fn(*args)


gateway = Gateway()
gateway.ping()
Gateway.ping(gateway)
Gateway.ping()
run(gateway.apply)
run(Gateway.apply)
run(Gateway.build)
run(Gateway.fee)
run(run)
"#;
    let compiled = compile(&[("src/app.py", source)]);
    let file = File::new(&compiled, "src/app.py");
    // Through an instance the call binds `self`; through the class it binds nothing.
    assert_eq!(file.result("gateway.ping", 0).bound, Some(1));
    assert_eq!(file.result("Gateway.ping", 0).bound, Some(0));
    assert_eq!(file.result("Gateway.ping", 1).bound, Some(0));
    // A construction of a class without `__init__` targets the class: no count.
    assert_eq!(file.result("Gateway", 0).bound, None);
    // Passed callables carry their own count on the argument.
    let passed: Vec<Option<u32>> = (0..5).map(|nth| file.arguments("run", nth)[0].bound).collect();
    assert_eq!(passed, [Some(1), Some(0), Some(1), Some(0), Some(0)]);
}

/// A class without a direct constructor takes no arguments only when nothing could supply one.
#[test]
fn class_constructions_take_nothing_only_when_proven() {
    let source = r#"from dataclasses import dataclass


class Plain:
    pass


class Shaped:
    def __new__(cls, *args):
        return super().__new__(cls)


class Base:
    pass


class Derived(Base):
    pass


def wrap(cls):
    return cls


@wrap
class Wrapped:
    pass


Plain(1)
Shaped(1)
Derived(1)
"#;
    let compiled = compile(&[("src/app.py", source)]);
    let file = File::new(&compiled, "src/app.py");
    assert_eq!(file.pairings("Plain", 0), [Some(U(R::NoParameter))]);
    assert_eq!(file.pairings("Shaped", 0), [Some(U(R::NoSignature))]);
    assert_eq!(file.pairings("Derived", 0), [Some(U(R::NoSignature))]);
}

/// A class whose bases reach the builtin exception hierarchy constructs its instances, as long
/// as every user class comes before the builtins in its linearization.
#[test]
fn exception_classes_construct_through_builtin_bases() {
    let source = r#"class PaymentError(Exception):
    def describe(self):
        return "payment"


class CardDeclined(PaymentError):
    def __init__(self, intent):
        super().__init__(intent)
        self.intent = intent


class Lookup(LookupError):
    pass


class Mixin:
    def describe(self):
        return "mixin"


class Mixed(ValueError, Mixin):
    pass


def fail(error: CardDeclined):
    error.describe()
    error.with_traceback(None)
    raise CardDeclined("x")


def mixed(error: Mixed):
    error.describe()


raise PaymentError("y")
raise Lookup("z")
raise Mixed()
"#;
    let compiled = compile(&[("src/app.py", source)]);
    let file = File::new(&compiled, "src/app.py");
    let edge = |text: &str, nth: usize| {
        let call = file.call(text, nth);
        file.graph()
            .edges
            .iter()
            .find(|edge| edge.source == call && edge.seq == 0)
            .unwrap()
            .clone()
    };
    let declined = edge("CardDeclined", 0);
    assert_eq!(declined.kind, unfold_core::vocab::EdgeKind::Constructs);
    assert_eq!(
        declined.target.as_ref().map(Key::as_str),
        Some("src/app.py#class:CardDeclined/constructor:__init__")
    );
    assert_eq!(
        edge("PaymentError", 0).target.as_ref().map(Key::as_str),
        Some("src/app.py#class:PaymentError")
    );
    assert_eq!(
        edge("Lookup", 0).target.as_ref().map(Key::as_str),
        Some("src/app.py#class:Lookup")
    );
    // A user method is found before the builtins; a builtin's own member is not followed.
    assert_eq!(
        edge("error.describe", 0).target.as_ref().map(Key::as_str),
        Some("src/app.py#class:PaymentError/method:describe")
    );
    assert_eq!(edge("error.with_traceback", 0).resolution, Resolution::Unresolved);
    // `super().__init__` reaches the builtin `Exception.__init__`: not followed.
    assert_eq!(edge("super().__init__", 0).reason, Some(Reason::Unsupported));
    // A user class after a builtin in the linearization: the class still constructs (a class
    // without `__init__` is its own target), but no member is looked up through it, since a
    // builtin before the user's `Mixin` could define the name.
    assert_eq!(
        edge("Mixed", 0).target.as_ref().map(Key::as_str),
        Some("src/app.py#class:Mixed")
    );
    assert_eq!(edge("error.describe", 1).resolution, Resolution::Unresolved);
    // A construction of a class with a base takes its arguments through something unseen.
    assert_eq!(file.pairings("PaymentError", 0), [Some(U(R::NoSignature))]);
}
