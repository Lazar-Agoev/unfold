use std::collections::BTreeMap;

use super::stubs::{Loader, Truth, absolute_module, condition};
use super::table::{self, Member, Returns, Table, TableClass};
use super::{Header, TYPESHED_COMMIT, render};

/// A typeshed folder with `files` (paths under `stdlib/` and their text) and the table of `roots`.
fn table(files: &[(&str, &str)], roots: &[&str]) -> Table {
    let dir = tempfile::tempdir().expect("temp dir");
    let stdlib = dir.path().join("stdlib");
    for (path, text) in files {
        let path = stdlib.join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
        std::fs::write(path, text).expect("written");
    }
    let mut loader = Loader::new(stdlib, (3, 14));
    table::build(&mut loader, roots).expect("builds")
}

fn members<'t>(table: &'t Table, module: &str) -> &'t BTreeMap<String, Member> {
    &table
        .modules
        .iter()
        .find(|it| it.name == module)
        .unwrap_or_else(|| panic!("module {module}"))
        .members
}

fn class<'t>(table: &'t Table, name: &str) -> &'t TableClass {
    table
        .classes
        .iter()
        .find(|it| it.name == name)
        .unwrap_or_else(|| panic!("class {name}"))
}

fn instance(class: &str) -> Member {
    Member::Function(Returns::Instance(class.to_owned()))
}

const BUILTINS: &str = "\
from typing import Any, Generic, TypeVar, overload, Self, Protocol
_T = TypeVar('_T')
class object:
    def __eq__(self, other: object, /) -> bool: ...
class bool: ...
class int: ...
class bytes: ...
class type: ...
class str:
    @overload
    def strip(self: LiteralString, chars: LiteralString | None = None) -> LiteralString: ...
    @overload
    def strip(self, chars: str | None = None) -> str: ...
class list(Generic[_T]):
    def copy(self) -> list[_T]: ...
    def pop(self) -> _T: ...
class dict(MutableMapping[_T, _T]):
    @overload
    def get(self, key: _T, default: None = None, /) -> _T | None: ...
    @overload
    def get(self, key: _T, default: _T, /) -> _T: ...
class _NotImplementedType(Any): ...
NotImplemented: _NotImplementedType
def len(obj: object, /) -> int: ...
from typing import LiteralString, MutableMapping
";

const TYPING: &str = "\
class Any: ...
Generic: object
Protocol: object
def overload(f): ...
class MutableMapping(Generic[_KT, _VT]):
    def setdefault(self, key, default) -> _VT: ...
";

#[test]
fn reads_the_written_forms_without_generics() {
    let module = "\
import sys
from typing import Optional, Self, TypeVar, overload
from typing_extensions import deprecated
_T = TypeVar('_T')
class Repo:
    @classmethod
    def open(cls) -> Self: ...
    def maybe(self) -> Repo | None: ...
    def optional(self) -> Optional[Repo]: ...
    def quoted(self) -> 'Repo': ...
    def generic(self) -> list[str]: ...
    def either(self) -> Repo | int: ...
    def variable(self) -> _T: ...
    def nothing(self) -> None: ...
    def unwritten(self): ...
    def klass(self) -> type[Repo]: ...
    async def fetch(self) -> Repo: ...
    async def __aenter__(self) -> Self: ...
    @property
    def name(self) -> str: ...
    @name.setter
    def name(self, value: str) -> None: ...
    size: int
    @overload
    def agree(self, x: int) -> Repo: ...
    @overload
    def agree(self, x: str) -> 'Repo': ...
    @overload
    def differ(self, x: int) -> Repo: ...
    @overload
    def differ(self, x: str) -> str: ...
    @deprecated('old')
    def old(self) -> Repo: ...
def get_repo() -> Repo: ...
def stripped() -> LiteralString: ...
from typing_extensions import LiteralString
";
    let table = table(
        &[
            ("builtins.pyi", BUILTINS),
            ("typing.pyi", TYPING),
            ("typing_extensions.pyi", "from typing import *\n"),
            ("m.pyi", module),
        ],
        &["m"],
    );
    let m = members(&table, "m");
    assert_eq!(m["Repo"], Member::Class("m.Repo".into()));
    assert_eq!(m["get_repo"], instance("m.Repo"));
    assert_eq!(m["stripped"], instance("builtins.str"));
    // Type variables are stub-only.
    assert!(!m.contains_key("_T"));
    let repo = &class(&table, "m.Repo").members;
    let expect = [
        ("open", Member::Function(Returns::Receiver)),
        ("maybe", instance("m.Repo")),
        ("optional", instance("m.Repo")),
        ("quoted", instance("m.Repo")),
        ("generic", instance("builtins.list")),
        ("either", Member::Function(Returns::Unknown)),
        ("variable", Member::Function(Returns::Unknown)),
        ("nothing", Member::Function(Returns::Unknown)),
        ("unwritten", Member::Function(Returns::Unknown)),
        ("klass", Member::Function(Returns::Unknown)),
        ("fetch", Member::Function(Returns::AwaitInstance("m.Repo".into()))),
        ("__aenter__", Member::Function(Returns::AwaitReceiver)),
        ("name", Member::Variable(Some("builtins.str".into()))),
        ("size", Member::Variable(Some("builtins.int".into()))),
        ("agree", instance("m.Repo")),
        ("differ", Member::Function(Returns::Unknown)),
        ("old", instance("m.Repo")),
    ];
    assert_eq!(repo.len(), expect.len());
    for (name, member) in expect {
        assert_eq!(repo[name], member, "{name}");
    }
    // Overloads on `LiteralString` and `str` agree; `dict.get`'s don't.
    assert_eq!(
        class(&table, "builtins.str").members["strip"],
        instance("builtins.str")
    );
}

#[test]
fn bases_close_the_reached_classes_and_open_classes_are_left_out() {
    let module = "\
from typing import Any
from typing_extensions import TypeAlias
_Base: TypeAlias = dict[str, int]
class Row(_Base): ...
class Loose(Any):
    def f(self) -> int: ...
def row() -> Row: ...
def loose() -> Loose: ...
nothing = NotImplemented
";
    let table = table(
        &[
            ("builtins.pyi", BUILTINS),
            ("typing.pyi", TYPING),
            ("typing_extensions.pyi", "from typing import *\n"),
            ("m.pyi", module),
        ],
        &["m"],
    );
    assert_eq!(class(&table, "m.Row").bases, ["builtins.dict"]);
    assert_eq!(class(&table, "builtins.dict").bases, ["typing.MutableMapping"]);
    assert_eq!(class(&table, "typing.MutableMapping").bases, Vec::<String>::new());
    assert_eq!(
        class(&table, "builtins.dict").members["get"],
        Member::Function(Returns::Unknown)
    );
    let m = members(&table, "m");
    assert_eq!(m["Loose"], Member::Variable(None));
    assert_eq!(m["loose"], Member::Function(Returns::Unknown));
    assert_eq!(m["nothing"], Member::Variable(None));
    assert!(table.classes.iter().all(|class| class.name != "m.Loose"));
    assert!(
        table
            .classes
            .iter()
            .all(|class| !class.name.contains("NotImplemented"))
    );
    // Only reached classes are listed, sorted.
    let names: Vec<&str> = table.classes.iter().map(|class| class.name.as_str()).collect();
    assert_eq!(names, ["builtins.dict", "m.Row", "typing.MutableMapping"]);
}

#[test]
fn versions_platforms_reexports_and_submodules() {
    let package = "\
import sys
from . import path as _path
from ._impl import *
from ._impl import Shown as Shown, Hidden
from ._impl import Listed
from typing import Any
path = _path
if sys.version_info >= (3, 13):
    def new() -> int: ...
else:
    def old() -> int: ...
if sys.version_info >= (3, 15):
    def future() -> int: ...
elif sys.version_info >= (3, 14):
    def current() -> int: ...
else:
    def past() -> int: ...
if sys.platform == 'win32':
    def both() -> int: ...
    def windows() -> int: ...
else:
    def both() -> int: ...
if sys.platform == 'win32':
    def split() -> int: ...
else:
    def split() -> str: ...
if sys.platform != 'win32' and sys.version_info < (3, 10):
    def gone() -> int: ...
if not sys.version_info >= (3, 14):
    def gone_too() -> int: ...
__all__ = ['Listed', 'new']
__all__ += ['star']
";
    let implementation = "\
from typing import type_check_only
__all__ = ['star', 'Proto']
class Shown: ...
class Hidden: ...
class Listed: ...
def star() -> Shown: ...
def unlisted() -> int: ...
@type_check_only
class Proto:
    def method(self) -> int: ...
def proto() -> Proto: ...
";
    let table = table(
        &[
            ("builtins.pyi", BUILTINS),
            ("typing.pyi", TYPING),
            ("pkg/__init__.pyi", package),
            ("pkg/_impl.pyi", implementation),
            ("pkg/path.pyi", "def join(a: str) -> str: ...\n"),
        ],
        &["pkg"],
    );
    let names: Vec<&str> = table.modules.iter().map(|module| module.name.as_str()).collect();
    assert_eq!(names, ["pkg", "pkg.path"]);
    let pkg = members(&table, "pkg");
    let int = instance("builtins.int");
    assert_eq!(pkg["new"], int);
    assert_eq!(pkg["current"], int);
    assert_eq!(pkg["both"], int);
    assert_eq!(pkg["windows"], int);
    assert_eq!(pkg["split"], Member::Function(Returns::Unknown));
    assert_eq!(pkg["star"], instance("pkg._impl.Shown"));
    assert_eq!(pkg["Shown"], Member::Class("pkg._impl.Shown".into()));
    // Listed in `__all__`, though imported without `as`.
    assert_eq!(pkg["Listed"], Member::Class("pkg._impl.Listed".into()));
    assert_eq!(pkg["path"], Member::Module("pkg.path".into()));
    for absent in [
        "old", "future", "past", "gone", "gone_too", "Hidden", "unlisted", "Any", "sys", "_path", "Proto",
        "proto",
    ] {
        assert!(!pkg.contains_key(absent), "{absent}");
    }
    assert_eq!(members(&table, "pkg.path")["join"], instance("builtins.str"));
}

#[test]
fn star_imports_without_all_take_public_exported_names() {
    let table = table(
        &[
            ("builtins.pyi", BUILTINS),
            ("typing.pyi", TYPING),
            ("a.pyi", "from b import *\n"),
            (
                "b.pyi",
                "import sys\nfrom typing import Any\nfrom c import kept as kept\ndef public() -> int: ...\ndef _private() -> int: ...\n",
            ),
            ("c.pyi", "def kept() -> int: ...\n"),
        ],
        &["a"],
    );
    let names: Vec<&String> = members(&table, "a").keys().collect();
    assert_eq!(names, ["kept", "public"]);
}

#[test]
fn conditions_decide_versions_and_leave_the_rest_open() {
    let truth = |text: &str| {
        let parsed = ruff_python_parser::parse_expression(text).expect("parses");
        condition(parsed.expr(), (3, 14))
    };
    assert_eq!(truth("sys.version_info >= (3, 14)"), Truth::True);
    assert_eq!(truth("sys.version_info >= (3, 15)"), Truth::False);
    assert_eq!(truth("sys.version_info < (3, 14)"), Truth::False);
    assert_eq!(truth("sys.version_info > (3, 14)"), Truth::True);
    assert_eq!(truth("sys.version_info >= (3,)"), Truth::True);
    assert_eq!(truth("sys.version_info >= (4,)"), Truth::False);
    assert_eq!(truth("sys.version_info >= (3, 14, 1)"), Truth::Either);
    assert_eq!(truth("sys.platform == 'win32'"), Truth::Either);
    assert_eq!(truth("sys.platform.startswith('linux')"), Truth::Either);
    assert_eq!(
        truth("sys.platform == 'win32' and sys.version_info < (3, 10)"),
        Truth::False
    );
    assert_eq!(
        truth("sys.platform == 'win32' or sys.version_info >= (3, 10)"),
        Truth::True
    );
    assert_eq!(
        truth("sys.platform == 'win32' and sys.version_info >= (3, 10)"),
        Truth::Either
    );
    assert_eq!(truth("not sys.version_info >= (3, 14)"), Truth::False);
}

#[test]
fn relative_imports_resolve_from_the_module_or_package() {
    assert_eq!(
        absolute_module("asyncio", true, 1, Some("tasks")).expect("ok"),
        "asyncio.tasks"
    );
    assert_eq!(
        absolute_module("asyncio.tasks", false, 1, Some("events")).expect("ok"),
        "asyncio.events"
    );
    assert_eq!(absolute_module("os", true, 1, None).expect("ok"), "os");
    assert_eq!(absolute_module("a.b.c", false, 2, Some("d")).expect("ok"), "a.d");
    assert_eq!(
        absolute_module("x", false, 0, Some("typing")).expect("ok"),
        "typing"
    );
    assert!(absolute_module("top", false, 1, Some("x")).is_err());
}

#[test]
fn rendering_is_sorted_compact_and_versioned() {
    let table = table(
        &[
            ("builtins.pyi", BUILTINS),
            ("typing.pyi", TYPING),
            (
                "m.pyi",
                "class B:\n    def m(self) -> B: ...\ndef f() -> B: ...\nclass A: ...\n",
            ),
        ],
        &["m"],
    );
    let text = render(
        &Header {
            commit: "abc",
            python_version: "3.14",
            files: 3,
            digest: "def",
        },
        &table,
    );
    assert!(text.contains("typeshed abc (`stdlib/`, 3 files),\n//! read without generics for Python 3.14."));
    assert!(text.contains("use Member::{Class, Function};\nuse Returns::Instance;\n"));
    assert!(text.contains(
        "    StubModule { name: \"m\", members: &[\n        (\"A\", Class(\"m.A\")), (\"B\", Class(\"m.B\")), (\"f\", Function(Instance(\"m.B\"))),\n    ] },\n"
    ));
    let a = text.find("StubClass { name: \"m.A\"").expect("A");
    let b = text.find("StubClass { name: \"m.B\"").expect("B");
    assert!(a < b);
    assert!(text.lines().all(|line| line.len() <= 110));
}

#[test]
fn the_commit_is_the_one_stdlib_pins() {
    let stdlib = std::fs::read_to_string(crate::repo::root().join("crates/unfold-lang-python/src/stdlib.rs"))
        .expect("stdlib.rs");
    assert!(stdlib.contains(TYPESHED_COMMIT));
}
