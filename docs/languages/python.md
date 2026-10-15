# Python front end

Read this for Python parsing, lowering to nodes, imports and source roots, binding, receivers, or typed records.
Start at [lib.rs](../../crates/unfold-lang-python/src/lib.rs). The block model, keys, outcomes, and the shared
resolution policy are in [Graph model](../graph-model.md).

## Code map

| Concern | Owning module |
| --- | --- |
| Extraction of one file | `extract.rs` |
| Ruff parsing and compile-time checks | `parse.rs`, `analysis/` |
| Lowering to nodes | `lower/` (`statements.rs`, `expressions.rs`), `ast.rs`, `tokens.rs` |
| Value table and call facts | `values.rs` |
| Per-file binder facts and payload | `bind/`, `payload.rs` |
| Source roots, imports, whole-program binding | `link/` |
| Standard-library stubs | `stubs.rs` (generated), `link/library.rs`, `stdlib.rs` |
| Written types | `typed.rs`, `bind/types.rs` |
| Census, and highlighting (the code panel's, and the comments gap ticks skip) | `census.rs`, `highlight.rs` |

Paths are relative to `crates/unfold-lang-python/src/`. Read a module's comment and the relevant fixture first.

## Files and parsing

`.py` files are sources; `.pyi` stubs are not. Each file is parsed with pinned Ruff under its
`[[python.project]]` grammar version (3.12, 3.13, or 3.14, the default). Strict UTF-8 is required: a declared
non-UTF-8 encoding fails the file even when its bytes decode. A file also fails on the compile-time errors CPython
reports that Ruff's parser leaves to later passes ([analysis/checks.rs](../../crates/unfold-lang-python/src/analysis/checks.rs)):
`return` outside a function, `break`/`continue` outside a loop or leaving an `except*` handler, duplicate parameters,
a bare `except:` that isn't last, and the `match` pattern errors. Leaving a `finally` block with `return`, `break`, or
`continue` compiles under every grammar: CPython 3.14 only warns about it (PEP 765).

Lines end at CR, LF, and CRLF, as CPython counts them: a U+2028 or U+2029 in a string or a comment doesn't end a line
([Graph model › Text, ranges, and lines](../graph-model.md#text-ranges-and-lines)).

A file either parses completely or fails with a message and the offset of its first problem; a failure keeps the
file's last good facts ([Engine](../engine.md#facts-cache-and-failures)). Python is never run: no imports, no
annotations evaluated, no decorators executed. Cached facts hold binder state, not a Ruff tree.

A per-file pre-pass proves the identities that decide structure, before lowering: `builtins.property` (getter and
setter forms), `typing.TYPE_CHECKING` (its imports get no link), `typing.overload` (its stubs are left out), and
`importlib.import_module` (a literal call also has an `imports` link). A star import is assumed not to rebind them;
only a same-file write invalidates one.

## Lowering

`lower/` turns Ruff's tree straight into nodes, one rule per syntax form, keyed in preorder with `KeyBuilder`.

| Written | Node |
| --- | --- |
| The module | `module`; its docstring is `doc` |
| `def`, `async def` | `function`; in a class body `method`, `constructor` for `__init__`, `getter`/`setter` for a proven `@property` and its setter |
| `class` | `class`; a base written as a name or member chain is an `extends` link, any call in its header a `base` call |
| Decorators (plain names included); a `def`'s defaults and annotations | Calls in the enclosing body before the definition, role `decorator` or `default`, `for` the definition |
| `lambda` | `function` (`lambda`): named when it is exactly the value of a simple assignment, `callback[n]` in a call's arguments, `lambda[n]` anywhere else |
| `if` / `elif` / `else` | `decision` (`if`): `then`, `elif[n]`, `else`; each test is its part's `head`, its calls `test` calls |
| `match` / `case` / `case _:` | `decision` (`match`), headed by its subject: `case[n]` (pattern and guard are its head), and `default` for the guardless `case _:`, with no head |
| `a if c else b` with a call | `decision` (`ternary`) |
| `for`, `async for`, `while`, loop `else` | `loop` (`for`, `while`): header calls, `body`, `else` |
| Comprehension or generator with a call | `loop` (`comprehension`): `for` clauses are the header, the element the body, `if` clauses the body's test |
| `try` / `except` / `except*` / `else` / `finally` | `try`: `body`, `except[n]` (its clause is its head, calls in it are `test` calls; `group` for `except*`), `else`, `finally` |
| `with`, `async with` | `with` (flag `async`): header calls, `body` |
| `return`, `raise`, `break`, `continue`, `yield`, `yield from` | `exit`, as [Graph model › Exits](../graph-model.md#exits) describes |
| Calls | `call`, with its normalized callee; form `new` once linking proves a construction |
| Imports, `importlib.import_module("x")` | `imports` links from the module, class, or function the import is written in |
| Assignments, expression statements, `pass`, `assert`, `del`, `global`, `nonlocal` | Code: only their calls are blocks |

Choices the lowering makes:

- Bytes and f-strings normalize as `""` in callees, like string literals.
- `conditional` holds inside ternaries and comprehensions, and a walrus keeps `exit_value`.
- Past the depth limit a node is marked `truncated` and nothing below it is lowered. Lowering recurses on a stack
  that grows as needed, so deep syntax can't overflow it.
- A `def` with a decorator proven to be `typing.overload` is a stub. It is left out with everything written in it
  (its decorators, defaults, and annotations), as TypeScript leaves out overload signatures, so the implementation
  takes the plain key: `pkg/parse.py#parse`. A decorator named `overload` that isn't `typing`'s keeps the `def`.

## Imports and source roots

Every file resolves its imports through its own source roots, the same however the code is run:

- **Source roots** come from the deepest `[[python.project]]` containing the file (`source_roots`). Without them the
  project folder is that project's root, or else the nearest folder above the file with a `pyproject.toml`, or else
  the open folder; its root is `src/` for a src layout, and the folder itself otherwise.
- **Absolute imports** `a.b` are looked for under every source root as `a/b/__init__.py`, then `a/b.py`, then a
  namespace folder `a/b`. More than one match is `ambiguous`, listed as a problem. Nothing found is `not_found` when
  the first part may name project code: a module or package a source root reaches, or, unless the standard library
  has a module of that name, a Python module or a folder of them anywhere in the project (code the source roots
  don't reach can still be imported after a `sys.path` change). Any other miss is external, labeled by its first
  part. Calls and type names through a module that isn't found are `not_found` too.
- **Relative imports** need the importing file to lie in a source root deep enough for its dots; nothing found is
  `not_found`.
- A module that exists outside the index (a source root outside `[scope]`) is `not_indexed`, never `not_found`.
  Existence comes from the program, discovery's unindexed and context files, and, for paths discovery never walked,
  the disk.

A `from package import name` loads a child module only when the attribute is proven absent: the package is indexed,
its `__init__.py` binds nothing of that name and defines no hook that could (`__getattr__`, `__path__`, …), and no
code anywhere may write the attribute. Only then is the child import an `imports` link of its own; linking keeps or
drops it, and nothing is renumbered. Dynamic import calls other than a literal `importlib.import_module` and
arbitrary `__getattr__` exports are not static module proof.

Implementation: [link/modules.rs](../../crates/unfold-lang-python/src/link/modules.rs),
[link/children.rs](../../crates/unfold-lang-python/src/link/children.rs).

## Binder and runtime uncertainty

Linking builds whole-program scopes, declarations, writes, module and member indexes, and class tables from the
per-file payloads, in three steps: the program (names, writes, bindings, star imports expanded), two mutation passes
in program order, then freezing, after which answers are cached and read from any thread. It follows Python's scope
rules for functions, lambdas, comprehensions, class bodies, `global`, `nonlocal`, parameters, and imports.

- A name with competing writes is never followed to one binding.
- Attribute writes from any file invalidate module members and class members; `self.attr = …` in a method records
  an instance field; a proven `exec` or `eval` makes every identity in its module `unsupported`; a proven
  `setattr`/`delattr` invalidates the member it names, or every member.
- The same writes replace standard-library members: on `builtins` (`builtins.print = …`), on a stub class
  (`logging.Logger.info = …`), or on an instance the stubs type (`logger.info = …`, which may be any instance of its
  class). Calls that may reach a replaced member are `dynamic`: through the class, the classes that inherit the
  member, and written types that may hold one of them.
- C3 linearization governs member lookup. A builtin exception base sits in the linearization after every user
  class. Construction and instance access bind receivers differently from access through the class; `super()` must
  agree across every linearization that can reach it.
- A body stays concrete only when every decorator is proven transparent (`staticmethod`, `classmethod`,
  `functools.cache`, `functools.lru_cache`, `typing.final`, `typing.override`) or marks something other than a
  callable body (`typing.overload`, `abc.abstractmethod`, `property` and its setter). Anything else, `@dataclass`
  included, makes calls through it `wrapper`.

Implementation: [link/binder.rs](../../crates/unfold-lang-python/src/link/binder.rs).

## What a call's value is

A call gives an instance of a class only when it is proven ([link/value.rs](../../crates/unfold-lang-python/src/link/value.rs)):

- **Construction:** the call constructs a proven, concrete project class.
- **Written return type:** the callee is exactly one proven, concrete `def` that isn't overridden or wrapped, and its
  annotation is a class (a name, member chain, or quoted dotted name), `Repo | None` or `None | Repo`, or
  `Optional[Repo]` with `Optional` proven to be `typing.Optional` (or `typing_extensions.Optional`). A generic class
  is read by its class, its type arguments dropped, as written variable types are: `-> Repo[str]` gives `Repo`,
  `-> list[int]` gives `list`. An `async def`'s type is read only through `await`, any other's only without it.
  Unions of classes, TypeVars, `Self`, `type[Repo]`, quoted generics, and `Union[Repo, None]` give nothing, and so
  does a class that derives from `Generic[T]`, whose base Unfold doesn't read.
- **Written annotations are read as their `types` links are:** in the type meaning, where a name an assignment binds
  (`Alias = Repo`) is its own declaration, never followed. So `-> Alias` and `param: Alias` give nothing, as the
  `types` link says `not_concrete`. A class with a decorator that isn't proven transparent is still its own type,
  as for its `types` link, and calls to its members are `wrapper`.
- **Standard-library stubs:** eight modules (`builtins`, `logging`, `os` with `os.path`, `json`, `re`, `datetime`,
  `pathlib`, `asyncio`) and every class their members reach, generated into `stubs.rs` by
  `cargo xtask gen-py-stubs <typeshed-dir>` from typeshed at the commit pinned in `stdlib.rs`. Nothing is read at run
  time. The same run writes the builtin exception classes and their bases into the reader (`python_exceptions.rs`),
  which tells which `except` catches a raise ([Reader › Contracts](../reader.md#contracts)).
  - Type arguments are dropped (`list[str]` is `list`); a class or `None`, `Optional`, quoted names, and
    `async` through `await` are kept.
  - `Self` in a stub reads as the receiver's class (`datetime.datetime.now()`); `LiteralString` reads as `str`.
  - Overloads and platform branches count only when they agree. The stubs are read for Python 3.12, 3.13, and 3.14
    and intersected: a member is kept only when every version has it, with a type only when every version agrees.
  - The generator refuses a typeshed whose SHA-256 isn't the pinned one.

Values resolve through the stubs along C3 order to `builtins.object`: `logging.getLogger(…)` then `logger.info`
(through a module or function variable, `self.log` set in `__init__`, or a project `-> logging.Logger`) is library
`python:logging`, and so are chains such as `re.compile(…).match(…).group`. Builtin methods resolve on written
types (`row: dict[str, Any]` gives `row.get`; parameter and variable annotations drop type arguments) and on
literals, including a variable bound once to a literal. Builtin values and type names (`ord`, `id`, `ExceptionGroup`)
come from the stubs' `builtins` module.

A written type may hold a project subclass, so a member of a written stub class is `overridden` when any project
class deriving from it defines that member; a written `object` or protocol (`os.PathLike`) gives `overridden` for
every member. Exact values (literals, displays, constructions) aren't affected. A member the stub class doesn't have,
or a hop on a value the stubs don't type, stays `unknown_receiver`; `type(x)` and other metaclass instances are never
instances; a class with `__getattr__` gives `dynamic`.

## Module labels

| Origin | Label |
| --- | --- |
| Python's builtins | `python:builtins` (outcome `builtin`) |
| A standard-library module (its first part is a typeshed `stdlib/` root) | `python:<root>`: `python:logging` |
| A stub class's member | The class's module: typeshed's `_asyncio.Task` gives `python:_asyncio` |
| Any other external module | Its first part: `fastapi` |
| An unindexed repository file | Its repository-relative path |

## Typed records

| Record | From |
| --- | --- |
| Signature | `def` (async too) and `lambda`: the annotation after `->` |
| Parameter | Ruff's parameters in order: positional-only, positional, `*args` (`rest`), keyword-only, `**kwargs` (`keywords`); the whole parameter as written, its name, whether a default is written, its annotation |
| Member | Every annotated assignment to a plain name written directly in a class body, kind `property`, whatever the class's decorators or bases. No generated fields for dataclasses, `TypedDict`, Pydantic, or `NamedTuple` |

Type-name links come from parameter, return, and annotated-assignment annotations, class bases, and `except`
clauses ([bind/types.rs](../../crates/unfold-lang-python/src/bind/types.rs)). A subscript links its value and every
element of its slice; both sides of `|` link. Not links: PEP 695 type parameters, `None` and literals, the arguments
of `Literal[…]`, metadata after the first `Annotated[…]` argument, and any string whose whole content isn't a dotted
name (a quoted name's range is inside the quotes). A base written as a name, member chain, or subscript value is an
`extends` link; keyword bases (`metaclass=`), starred bases, and calls are not static heritage.

An assignment or other write (`Money = Decimal`, `T = TypeVar("T")`) is its own declaration in the type meaning,
never followed to its value. Pairing binds `self` for an instance call and `cls` for a `classmethod`, nothing for a
`staticmethod` or access through the class ([link/types.rs](../../crates/unfold-lang-python/src/link/types.rs)); see
[Pairing](../graph-model.md#pairing).

## Known limits and checks

No interpreter or type checker participates. Dynamic imports, generated members, arbitrary decorators, runtime
reflection, and uncertain receiver dispatch stay explicit unknowns. Equal argument and parameter ordinals are never
binding proof.

Tests are one binary, `crates/unfold-lang-python/tests/python/` (lowering, binder, typed records, failures,
robustness, the engine, and the playground goldens). Fixtures: `testdata/fixtures/python-lowering/`,
`python-ranges/`, `python/policy/`, and `typed-lines/`. Syntax changes need grammar, encoding, census, and range
cases; binder changes need writes, imports, classes, decorators, reflection, and receiver cases.

The precision gate compares resolved calls with Pyright's answers ([Testing](../testing.md)); keep precision exact
rather than replacing an uncertain answer.
