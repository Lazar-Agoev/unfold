use std::collections::BTreeMap;

use super::stubs::{Loader, Truth, absolute_module, condition};
use super::table::{self, Member, Returns, Table, TableClass};
use super::{Header, MODULES, TYPESHED_COMMIT, generate, render, render_exceptions};

/// A `stdlib/` folder with `files` (paths under it and their text).
fn stdlib(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
        std::fs::write(path, text).expect("written");
    }
    dir
}

/// The table of `roots` in a typeshed folder with `files`, read for Python `version`.
fn table_for(files: &[(&str, &str)], roots: &[&str], version: (u64, u64)) -> Table {
    let dir = stdlib(files);
    let mut loader = Loader::new(dir.path().to_owned(), version);
    table::build(&mut loader, roots).expect("builds")
}

/// The table of `roots` in a typeshed folder with `files`, read for Python 3.14.
fn table(files: &[(&str, &str)], roots: &[&str]) -> Table {
    table_for(files, roots, (3, 14))
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
    @property
    def parent(self) -> Self: ...
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
        ("parent", Member::SelfVariable),
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
            python_versions: "3.12–3.14",
            files: 3,
            digest: "def",
        },
        &table,
    );
    assert!(text.contains(
        "typeshed abc (`stdlib/`, 3 files),\n//! read without generics for each of Python 3.12–3.14."
    ));
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
fn builtin_exceptions_are_written_for_the_reader() {
    let builtins = "\
class object: ...
class str: ...
class BaseException: ...
class Exception(BaseException): ...
class LookupError(Exception): ...
class KeyError(LookupError): ...
class OSError(Exception): ...
IOError = OSError
class Mixed(Exception, str): ...
";
    let table = table(
        &[("builtins.pyi", builtins), ("typing.pyi", TYPING)],
        &["builtins"],
    );
    let text = render_exceptions(
        &Header {
            commit: "abc",
            python_versions: "3.12–3.14",
            files: 2,
            digest: "def",
        },
        &table,
    );
    assert!(
        text.starts_with(
            "//! Generated by `cargo xtask gen-py-stubs <typeshed-dir>`; do not edit by hand.\n"
        )
    );
    assert!(text.contains(
        "pub(crate) static BUILTIN_EXCEPTIONS: &[(&str, &str, &[&str])] = &[
    (\"BaseException\", \"BaseException\", &[]),
    (\"Exception\", \"Exception\", &[\"BaseException\"]),
    (\"IOError\", \"OSError\", &[\"Exception\"]),
    (\"KeyError\", \"KeyError\", &[\"LookupError\"]),
    (\"LookupError\", \"LookupError\", &[\"Exception\"]),
    (\"OSError\", \"OSError\", &[\"Exception\"]),
];
"
    ));
}

#[test]
fn the_commit_is_the_one_stdlib_pins() {
    let stdlib = std::fs::read_to_string(crate::repo::root().join("crates/unfold-lang-python/src/stdlib.rs"))
        .expect("stdlib.rs");
    assert!(stdlib.contains(TYPESHED_COMMIT));
}

#[test]
fn protocols_are_marked() {
    let module = "\
from typing import Generic, Protocol, TypeVar
_T = TypeVar('_T')
class Plain(Protocol):
    def a(self) -> int: ...
class Generic_(Protocol[_T]):
    def b(self) -> int: ...
class Nominal(Plain):
    def c(self) -> int: ...
class Box(Generic[_T]): ...
def plain() -> Plain: ...
def generic() -> Generic_: ...
def nominal() -> Nominal: ...
def box() -> Box: ...
";
    let table = table(
        &[
            ("builtins.pyi", BUILTINS),
            ("typing.pyi", TYPING),
            ("m.pyi", module),
        ],
        &["m"],
    );
    assert!(class(&table, "m.Plain").protocol);
    assert!(class(&table, "m.Generic_").protocol);
    // Deriving from a protocol makes an ordinary class.
    assert!(!class(&table, "m.Nominal").protocol);
    assert_eq!(class(&table, "m.Nominal").bases, ["m.Plain"]);
    assert!(!class(&table, "m.Box").protocol);
}

#[test]
fn versions_intersect_to_what_every_version_has() {
    let module = "\
import sys
from n import Old
class Path:
    def stem(self) -> str: ...
    if sys.version_info >= (3, 14):
        def copy(self) -> Path: ...
        def changed(self) -> Path: ...
        size: int
    else:
        def changed(self) -> str: ...
        size: str
if sys.version_info >= (3, 13):
    def new() -> Path: ...
    def kind() -> int: ...
else:
    def old() -> Path: ...
    kind: int
if sys.version_info >= (3, 14):
    held: Old
else:
    def held() -> Old: ...
def path() -> Path: ...
";
    let files = [
        ("builtins.pyi", BUILTINS),
        ("typing.pyi", TYPING),
        ("m.pyi", module),
        ("n.pyi", "class Old: ...\n"),
    ];
    let tables = [(3, 12), (3, 13), (3, 14)]
        .into_iter()
        .map(|version| table_for(&files, &["m"], version))
        .collect();
    let table = table::intersect(tables).expect("intersects");
    let m = members(&table, "m");
    assert_eq!(m["path"], instance("m.Path"));
    for gated in ["new", "old"] {
        assert!(!m.contains_key(gated), "{gated}");
    }
    // A function in some versions and a variable in others is a variable of unknown type.
    assert_eq!(m["kind"], Member::Variable(None));
    assert_eq!(m["held"], Member::Variable(None));
    let path = &class(&table, "m.Path").members;
    assert_eq!(path["stem"], instance("builtins.str"));
    assert!(!path.contains_key("copy"), "3.14 only");
    assert_eq!(path["changed"], Member::Function(Returns::Unknown));
    assert_eq!(path["size"], Member::Variable(None));
    // Every version reaches `Old`, but only through a member whose versions disagree.
    assert!(table.classes.iter().all(|class| class.name != "n.Old"));
}

#[test]
fn versions_with_different_bases_fail() {
    let module = "\
import sys
class A: ...
class B: ...
if sys.version_info >= (3, 14):
    class C(A): ...
else:
    class C(B): ...
";
    let files = [
        ("builtins.pyi", BUILTINS),
        ("typing.pyi", TYPING),
        ("m.pyi", module),
    ];
    let tables = [(3, 13), (3, 14)]
        .into_iter()
        .map(|version| table_for(&files, &["m"], version))
        .collect();
    let error = table::intersect(tables).expect_err("bases differ");
    assert!(error.to_string().contains("`m.C`"), "{error}");
}

#[test]
fn another_typeshed_is_refused() {
    let mut files = vec![("builtins.pyi", BUILTINS), ("typing.pyi", TYPING)];
    let modules: Vec<String> = MODULES
        .iter()
        .filter(|module| **module != "builtins")
        .map(|module| format!("{module}.pyi"))
        .collect();
    files.extend(modules.iter().map(|module| (module.as_str(), "")));
    let dir = stdlib(&files);
    let error = generate(dir.path()).expect_err("not the pinned stub set");
    assert!(error.to_string().contains(TYPESHED_COMMIT), "{error}");
}
