# Python

This document describes how `crates/unfold-lang-python` turns Python files into blocks, slots,
values, occurrences, and resolved edges. It carries over the proof of concept's Python front end
(`frontend/python/{parser,mapping,token-census,binder,modules,index}.ts`, `source/python-projects.ts`,
and the "Python-specific schema 3 rules" of its `docs/format.md`) onto Ruff's parser, with Unfold's
byte offsets. The shared rules (block
selection, roles, keys, slots, validation) are in [the graph model](../graph-model.md); this file
covers what is specific to Python.

Status: structure, the binder, and the entrance detectors are complete; the `py-orders` playground's whole
graph (files, blocks, slots, edges, entrances, targets, and inputs) equals the proof of concept's
([Parity](#parity-with-the-proof-of-concept)). The detectors (`pyproject.script`, `fastapi.route`,
`celery.task`, `apscheduler.job`) are described in [entrances](../entrances.md#python-detectors).

## Files and parsing

- A file is Python when its extension is `.py` and Python is graphed (`languages` in
  `.unfold/config.toml`). Stub files (`.pyi`) are not indexed.
- The parser is Ruff's (`ruff_python_parser`, `ruff_python_ast`, `ruff_text_size`), pinned to the
  Ruff release tag **0.16.10** in the root `Cargo.toml` (U-9). The crates are internal to Ruff and
  not published, so they come from Git.
- Each file is parsed on its own. Extraction never reads another file, the file system, or an
  interpreter; nothing is executed.
- Extraction runs in four steps: the encoding check, parsing, a pre-pass over the whole tree
  (compile-time checks, tree depth, and the [structural proofs](#structural-proofs)), and block
  selection through the engine's shared selector, which also records values and occurrences.
- Every pass over the tree is iterative or depth-guarded. Ruff's parser grows its own stack; a deep
  tree is dropped on a stack segment sized from its measured depth. Selected blocks may nest at
  most 1,024 deep (the engine's limit); a deeper file fails.
- Ranges are UTF-8 byte offsets. A byte-order mark is kept in the text (the file root covers it;
  the first token starts after it).

## Grammar versions

The grammar version comes from the file's `[[python.project]]` (the deepest project root that
contains it), or is 3.14 when no project covers the file. It is part of the file's facts-cache key
(the project's settings hash, mixed with the front end's version).

| Syntax | First version | Rejected under |
| --- | --- | --- |
| PEP 695 type parameters, `type` aliases; PEP 701 f-strings | 3.12 | — |
| PEP 696 type parameter defaults (`def f[T = int]()`) | 3.13 | 3.12 |
| PEP 758 unparenthesized `except A, B:`; PEP 750 template strings (`t"..."`) | 3.14 | 3.12, 3.13 |
| Python 3.15 syntax (`lazy import`, unpacking in comprehensions, ...) | 3.15 | every supported version |

Version-gated syntax comes from Ruff's unsupported-syntax detection, which covers every version
check the proof of concept's grammar (Pyright 1.1.407) made between 3.12 and 3.14. One check is
added on top: under 3.14, the proof of concept rejected `return`, `break`, or `continue` that
leaves a `finally` block (PEP 765). CPython 3.14 only warns about these; Unfold keeps the proof of
concept's behavior.

## Failures and encodings

A file fails as a whole, with a message and the byte offset of its first problem; there is no
partial graph from recovered syntax. It fails when:

- **It declares an encoding other than UTF-8.** A comment matching `coding[:=]\s*([-\w.]+)` on line
  1, or on line 2 when line 1 is blank or a comment (as CPython reads it), must name UTF-8 under one
  of the spellings CPython normalizes to it (`utf-8`, `utf8`, `utf_8`, `utf-8-sig`, any case).
  Invalid UTF-8 bytes fail earlier, when the file is read.
- **It has a syntax error,** or syntax its grammar version does not support. When there are several,
  the message names the first by offset and counts the rest.
- **It has a compile-time error the proof of concept's grammar reported** (CPython rejects all of
  them too): `return` outside a function (a class body is not one); `break` or `continue` outside a
  loop body (a nested function or class, or a loop's `else`, is outside); `return`, `break`, or
  `continue` leaving an `except*` handler; a parameter name written twice (functions and lambdas);
  a bare `except:` before the last handler; and in `match`, an irrefutable case before the last
  case, an irrefutable alternative before the last one in an or-pattern, a name bound twice in a
  pattern, alternatives binding different names, or two starred names in a sequence pattern.

Unlike the proof of concept, a `break` or `continue` in a function nested inside a loop fails (the
proof of concept's grammar kept loop context across function boundaries; CPython does not).

## Syntax mapping

| Syntax | Block | Slots |
| --- | --- | --- |
| Module | `file` | |
| `def`, `async def` | `function`; directly in a class body: `constructor` (`__init__`), `getter` (a proven `property` decorator), `setter` (`@x.setter` where `x` is a proven property), else `method` | `name`, `parameter` (each named parameter, `*args` and `**kwargs` included; not the bare `*` or `/`), `return_type`, `body` |
| `lambda` | `function` | `parameter`, `body` (the expression) |
| `class` | `class` | `name`, `heritage` (from the first base or keyword to the last, metaclass included) |
| `a = value` | `variable` | `name`, `initializer` |
| `a = b = value` | `variable` per target, nested: the last target outermost, each level's `initializer` the next level (which starts at the first target) | |
| `a += value`, `a: T = value`, `a: T`, `type A = T`, `(a := value)` | `variable` | `name`, `initializer` (none for `a: T`) |
| Call | `call` (never `new`), or `import` for a proven literal `importlib.import_module` | `callee`, `argument` (indexed, in written order); `specifier` for an import |
| `import a.b as c, d` | one `import` per item | `specifier` (the dotted module) |
| `from .m import x` | `import` | `specifier` (dots included) |
| `if` / `elif` / `else` | `if`; an `elif` is a nested `if` with role `else` that runs to the end of the chain | `condition`, `then`, `else` |
| `for`, `async for` | `for_of` | `initializer`, `iterable`, `body`, `else` |
| `while` | `while` | `condition`, `body`, `else` |
| `try` (with `except` or `except*`) | `try` | `body`, `catch` (indexed: one per handler), `else`, `finally` |
| `except`, `except*` | `catch` | `binding` (the `as` name, else the exception type) |
| `with`, `async with` | `with` | `expression` (from the first item to the last), `body` |
| `match` | `match` | `expression`, `body` (from the first case to the last) |
| `case` | `case` | `condition` (the pattern and guard), `body` |
| `return` | `return` | `expression` |
| `raise` | `throw` | `expression` (the exception, not the cause) |
| `break`, `continue` | `break`, `continue` | |
| A standalone expression (docstrings, `...`, `await x`, calls) | `expression_statement` | `expression` |

No block: `pass`, `global`, `nonlocal`, `del`, `assert` (their expressions are still walked),
comprehensions and generator expressions (their calls belong to the enclosing block; calls in a
lambda's body belong to the lambda), patterns, and type annotations as such (calls in them are
still calls).

**Roles.** A block's role is the part of its saved parent that the path to it leaves through, as in
the shared rules. Decorators, parameter annotations and defaults (lambda defaults included), return
annotations, type parameters, and class bases and keywords have role `header`: they belong to the
enclosing scope and say nothing about when they are evaluated. Statements take the role of the
suite they are in (`body`, `then`, `else`, `catch`, `finally`); a handler is a `catch`. An annotated
assignment's annotation takes the role `name` (it belongs to the target), a bare annotation `a: T`
takes `header`, a `raise`'s cause and an `assert`'s message take `body`.

**Ranges.** Ranges are token-exact and follow the proof of concept's grammar where it differed
from Ruff's:

- A decorated definition starts at its first `@`.
- A suite (`body`, `then`, `else`, `finally`, ...) runs from its colon to the end of its last
  statement.
- An expression's range includes the parentheses around it (`(n := len(items))`, a condition
  `(a) > (b)`), except calls and lambdas, which never include them. Parentheses that belong to the
  parent stay out: a call's argument list, a class's base list, and the parentheses of a
  parenthesized `with` item list (`with (a as b, c):`).
- An unparenthesized tuple ends at its last element, not a trailing comma, and so does the
  statement it ends (`x = 1, 2,` is `x = 1, 2`).
- An `elif` runs from `elif` to the end of the whole chain.

## Names

- Functions and classes are named by their identifiers; lambdas are unnamed.
- A variable is named by the written text of its target, cut to 120 UTF-16 units: `count`,
  `self.total`, `names["key"]`, `quantity, price`.
- Names keep their written spelling (`ｃｏｕｎｔ` stays `ｃｏｕｎｔ`); keys escape them like any name.
  Identity (the binder's concern) uses the NFKC form Ruff's lexer gives identifiers.

## Value shapes and labels

Every callee and argument is recorded in the file's value table, and so is every item of a list or
tuple argument (which only the entrance detectors read, to name a `dependencies=[...]` item); a
member's receiver chain is recorded first, so receivers always precede the values that use them.

| Expression | Shape |
| --- | --- |
| `name` | `Name` |
| `receiver.member` | `Member`, with its receiver and member name (NFKC) |
| A call | `CallResult` |
| `lambda` | `InlineFunction`, naming the lambda's block |
| Anything else: subscripts, literals, operators, ... | `Other` |

**Labels** are the written source text of the expression's range (parentheses included except
around calls and lambdas), cut to 120 UTF-16 units. Unlike TypeScript's composed labels, a call
result keeps its arguments: `datetime.now(timezone.utc).isoformat`, `celery_app.task(name="x")`.

**Calls.** Every call is a `call` block with one `CallFact`: edge kind `calls`, context `call`.
Only the binder can prove class identity, which turns the occurrence into `constructs`.
**Argument positions** are written ordinals: positional, `*`-expanded, keyword, and `**`-expanded
arguments all count. For `*args` and `**kwargs` the value is the expanded expression, recorded with
`reference: false`, so it never supplies a named reference. Equal argument and parameter ordinals
never prove a binding.

## Import blocks

Every runtime import is an `import` block with one `ImportFact`, and the census lists its range:

| Syntax | Block range | Specifier |
| --- | --- | --- |
| `import a.b as c` | The item `a.b as c` (one block per item) | `a.b` |
| `from ..m import x, y` | The whole statement | `..m`; `.` for `from . import x` |
| `importlib.import_module("m")`, proven, literal | The call | The decoded string `m` |

- A dynamic import counts only when its callee is proven to be `importlib.import_module` (imported
  under any alias, never by spelling alone), and its one argument (positional or `name=`) is a plain
  string literal that is not relative. It then replaces the call occurrence; anything else stays a
  call.
- An import in the then-branch of an `if` (or `elif`) whose test is proven to be
  `typing.TYPE_CHECKING` is erased: no block, no occurrence. A local `TYPE_CHECKING = True` proves
  nothing.
- Specifiers keep the written spelling, without the whitespace and line continuations Python
  allows between parts.
- **Named child imports.** Each name of a runtime `from m import a, b` other than `*` and dunder
  names (`__x__`) is a *candidate* child import: a conditional `import` block over the written item
  (`first as loaded`), nested in the statement's block, with a `specifier` slot over the name and the
  specifier `m.a` (relative dots kept: `from . import a` is `.a`). Whether it is an occurrence
  depends on other files, so linking decides ([Named child imports](#named-child-imports)); unkept
  candidates are dropped from the graph with their slots and edges, and later sibling keys are
  renumbered. The facts cache keeps every candidate.

### Structural proofs

Three structural decisions depend on what a name refers to: getter and setter kinds
(`builtins.property`), erased imports (`typing.TYPE_CHECKING`), and dynamic imports
(`importlib.import_module`). Extraction proves them with a per-file write census before selecting
blocks: a name's identity is proven when its only binding is a single import (followed through
module attributes, `typing.TYPE_CHECKING`), or when nothing in the file binds it (a builtin). Python
scoping applies: LEGB lookup, class scopes invisible to nested functions, comprehension scopes,
`global` and `nonlocal`. Any competing write anywhere in the file (an assignment, a parameter, a
`def`, a `del`, a loop target, ...) disproves it, as does an attribute write to the imported
identity in the same file (`typing.TYPE_CHECKING = True`) or a proven builtin `exec`/`eval`.

These three proofs stay per file, because they decide which blocks exist and extraction is per
file; everything else the binder decides over the whole program. See [Known limits](#known-limits)
for the cross-file effects they can't see.

## Census

Validation compares every Python file with an independent census, computed from a fresh run of
Ruff's lexer (not the syntax tree or block selection) and stored in the file's payload:

- **Token boundaries**: every block and slot must start where a token starts and end where one ends
  (comments, newlines, and indentation are not tokens).
- **Call sites**: the proof of concept's primary-chain scanner. It tracks where the primary
  expression ending at each token starts (names, literals, `.member` steps, closing brackets,
  implicitly concatenated strings, whole f-strings) and reports each `(` after one as a call up to
  its matching `)`; the parameter list of a `def`, the base list of a `class`, a parenthesized
  `match` subject, and `case` patterns are not calls. `*` and `**` arguments are recorded as
  positions that never supply references. Proven dynamic imports are removed from the call sites.
- **Module dependencies**: the ranges of the import blocks above, taken from the import statements,
  plus the named child imports linking keeps.

## Highlighting

Colors come from Ruff's lexer, so text that does not parse is still colored. The classes are the
proof of concept reader's: keywords (including `True`, `False`, `None`), comments, strings (bytes
and the literal parts of f-strings and t-strings), and numbers by token; a name after `class` or
starting with an uppercase letter is a `type`, a name after `def` or `@` or before `(` is a
`function`, and a name after `.` is a `property`. Other names, operators, and punctuation stay
plain. Two refinements over the proof of concept: soft keywords (`match`, `case`, `type`, `lazy`)
are keywords only where they start such a statement, and expressions inside f-strings are colored
as code.

## Payload

The private per-file payload (postcard, versioned by `PAYLOAD_VERSION`, which the producer version and
so the facts-cache key include) holds the census, the written form of every runtime import (module,
`as` names, level; for a child candidate its `from` import, name, and declaration), aligned with the
file's import facts, and the binder's facts (`bind::ir::BindFacts`):

| Table | Holds |
| --- | --- |
| Names | NFKC identifiers, member names, decoded string literals |
| Scopes | Module, function, class, comprehension, and type-parameter scopes, with `global` and `nonlocal` declarations (a `nonlocal` read already points at its function scope) |
| Declarations | `def` and `lambda` (block, name, decorators, named parameters, first parameter, body scope), `class` (block, name, decorators, bases and keywords, body scope), parameters (owner, written position, annotation), assignments (value, annotation, factory callee), imports, and every other write |
| Expressions | What binding evaluates, in a closed vocabulary: names, member accesses, calls (callee, written arguments, enclosing function), subscripts (with a literal key), `await`, ternaries, string literals, list/tuple and dict displays, `*x` items (read only by the detectors' negative census), lambdas, chained-assignment levels; anything else is opaque |
| Writes and bindings | The complete write census, already redirected by `global`/`nonlocal` |
| Value table | The expression of each value-table entry |
| Star imports, effects | Star imports to expand, and the mutation passes' inputs in preorder: attribute and item targets, augmented targets, calls |

Extraction never reads another file; linking reads only these facts (it never parses).

## Source roots and imports

**Source roots** come from the file's `[[python.project]]` (`source_roots`, joined onto its `root`).
Without them, the project folder is the configured project's root, or else the nearest folder above
the file holding a `pyproject.toml`, or else the project's folder; its source root is `src/` when that
folder has a src layout (a `src` folder with Python files), and the folder itself otherwise. A source
root may lie outside `[scope]` (py-orders' `legacy/`): modules found there exist but are not indexed.

**Resolution** (no interpreter, environment, or installed package is ever involved):

- `a.b` is looked for under every source root as `a/b.py`, then the package `a/b/__init__.py`, then
  the namespace package folder `a/b`. More than one file or folder is `ambiguous`; nothing at all is
  external, labeled by the first part (`a`).
- A relative specifier (`..m`) needs the importing file to lie in a source root at least as many
  folders deep as its dots; otherwise, or when nothing is found, it is `not_found`.
- Existence comes from the program's files, discovery's unindexed and context files and inputs, and,
  for paths discovery never walks (ignored or always-excluded folders), the folder on disk.
- An import's outcome: an indexed module file is `resolved` to its file block; a module outside the
  repository is `external`; an existing module that is not indexed (excluded, failed, or a namespace
  folder) is `not_indexed`; `ambiguous` and `not_found` as above.
- A member of an imported module is a top-level binding of the module's file (or files, when
  ambiguous), or else a child module; a module file that exists but isn't indexed makes the member
  `not_indexed`. `import a.b` binds `a`; `import a.b as c` binds `c` to `a.b`.
- `from m import *` binds the names of `m`'s literal `__all__` (a list or tuple of string literals,
  written once); any other star import makes every unresolved name in its scope (and the scope's own
  bindings) `ambiguous`.
- A literal `importlib.import_module("m")` (see [Import blocks](#import-blocks)) resolves `m` like an
  absolute import; its result is a module of unknown shape, so members of it are `not_concrete`.
- Imports a proven `TYPE_CHECKING` guard erases still bind their names; only their occurrences go.

`PythonLinked::ambiguous_imports` lists the imports whose module several roots (or a module and a
package) could supply, for the Problems list.

### Named child imports

A child candidate is kept, with one `imports` edge to the child module, only when the attribute is
proven absent from the package, so the import machinery must load the submodule:

- the module is an indexed package (`__init__.py`) or a namespace folder, not external, missing, or
  ambiguous, and the child exists (`pkg/child.py`, `pkg/child/__init__.py`, or a folder);
- the package's `__init__.py` binds nothing of that name (except this very import, for a package's
  own `from . import child`) and defines no `__getattr__`, `__path__`, `__name__`, `__class__`, or
  `__dict__`, holds no unknown star import, and no proven `exec`/`eval`; the importing scope holds
  none either;
- no code anywhere may write the attribute: `pkg.child = …`, `setattr(pkg, …)` (a non-literal name
  counts as every name), writes through `pkg.__dict__`, `vars(pkg)`, `globals()`/`locals()` (in the
  package), `__path__` and `__spec__` (which redirect loading), mutating methods of any of these
  (`update`, `insert`, `append`, …, through aliases too), and augmented assignments to them;
- for a relative import, the importer's own `__package__`, `__spec__`, `__name__`, and `__path__` are
  unchanged (not bound at its top level, not written through its `globals()` or from another module).

The pass follows values to the module objects they may be, through aliases, assignments, imports,
tuple and dict displays, and subscripts, and records these negative facts apart from the first
mutation pass, so they only ever withhold child imports.

## Binder

The binder ports the proof of concept's rules; evidence can withhold a target, never invent one.
Linking decodes every payload once, interns names program-wide, builds hash-map indexes (bindings by
scope and name, the class tables), runs the two mutation passes in program order, then freezes and
answers from any thread. Answers that are expensive (linearizations, class concreteness, decorator
identities) are memoized; cycles are cut where the proof of concept cut them, and an answer is kept
only when it doesn't depend on where it was first asked, so the graph never depends on scheduling.

### Write census

Every write anywhere in the file counts, before or after the use (flow-insensitive): `def` and
`class`, imports, assignments (each level of a chain, annotated, augmented, unpacking through tuples,
lists, and starred targets), `del`, `for`/`async for` and comprehension targets, `with` targets,
`except` names, assignment expressions (which bind outside comprehensions), parameters, PEP 695 type
parameters, `type` aliases, and pattern captures (`case x`, `*rest`, `**rest`, `as x`, and `_`).
`global` writes bind in the module; `nonlocal` writes in the nearest enclosing function that writes
the name. Lookup is LEGB: the scope, then its ancestors, where a function or comprehension skips the
class scopes around it; a `global` or `nonlocal` declaration redirects reads too.

A name's identity is followed only when its binding has one effective write: a single write, a
property getter with its setter, or `typing.overload` signatures beside one implementation. Any
competing write makes a call `mutable_binding`; competing imports, or a conditional import among the
writes (inside `if`, `try`, `match`, `for`, or `while`), make it `ambiguous`. Names are compared in
their NFKC form (Ruff's lexer normalizes identifiers); blocks keep their written spelling.

The first mutation pass makes a binding mutated (so never followed) when another module or a
non-receiver write assigns it as an attribute: `module.attr = …` from any file invalidates `attr` of
every module the receiver may be (and every attribute for a star write); a write to a class's
attribute from outside its own receiver invalidates that member.

### Reflection

Reflection needs proven identity, never spelling (a local `def getattr` is an ordinary function):
`builtins.getattr` makes a call result `dynamic`; `functools.partial` makes it `wrapper`; a proven
`setattr`/`delattr` invalidates the literal member it names on the class or module of its first
argument, or every member for a non-literal name; a proven `exec` or `eval` anywhere in a module
makes every identity in that module `unsupported`. A subscript is `dynamic`, a conditional expression
`ambiguous`, and `await x` is `x`. Only the proof of concept's list of builtins (`print`, `len`,
`super`, `property`, `getattr`, `Exception`, …) is external when nothing binds the name; any other
unbound name is `unknown_name`.

### Classes and C3

- **Bases.** A class's bases must all be proven classes (`object` is skipped); then its C3
  linearization is computed (iteratively, so deep hierarchies can't exhaust the stack). Unknown or
  inconsistent bases leave no linearization, and member lookup through it finds nothing (an
  abstract-only own member is `not_concrete`).
- **Concreteness.** A class is concrete when no decorator wraps it, its header has no expansion and
  no keyword but a `metaclass` proven to be `builtins.type`, and its bases are concrete. Calling a
  proven concrete class constructs an instance: the occurrence edge becomes `constructs`, resolved in
  the construct context to a direct concrete `__init__`, otherwise the class. A class that isn't
  concrete is `wrapper` (when a decorator wraps it) or `unsupported`.
- **Class attribute lookup** (`Class.member`, `cls.member`) goes through the metaclass, so it needs a
  concrete class; through a `cls` parameter, every indexed descendant must be concrete too.
- **Members** are looked up through the linearization: a member invalidated anywhere in it is
  `dynamic`, as is any instance lookup when a class in it defines `__getattribute__`.
- **Virtual calls.** A call through an instance or `cls` receiver is `overridden` unless every indexed
  descendant of the receiver's class has a linearization in which the same binding wins and no class
  changes the member's lookup (an instance field, an invalidated member, `__getattribute__`).
- **`super()`** only without arguments, in a method with a named first parameter that isn't a
  `staticmethod`. The member after the method's class must be the same in the linearization of every
  indexed descendant that can reach it; disagreement is `ambiguous`, a descendant without a
  linearization `unsupported`.
- **Callable instances** are called through a proven `__call__`.

The engine's override census gets every class's bases and its own methods (with their `staticmethod`
flags); a member of a `typing.final` class or a `typing.final` method is never checked for
overrides.

### Receivers

The first named parameter of a method (a `def` directly in a class body) is an instance of the class,
or the class itself for a proven `classmethod`; a `staticmethod` has none. A parameter or variable
annotated with a proven class is an instance of it. An instance attribute `self.x` is an instance of a
class when its written annotations and its single assignment in `__init__` (the receiver being the
method's first parameter) all name that class, and no descendant may change it (an own field, an
invalidated member, `__getattribute__`, or no linearization makes it `dynamic`); more than one write
to the field, or one outside the constructor, proves nothing.

### Decorators

A decorator's identity is the external origin its expression (or, for `@d(...)`, `d`) resolves to;
`@x.setter` is a setter when `x` is bound by a declaration with a proven `builtins.property`
decorator. A body stays concrete only when every decorator is on the allowlist, proven by identity:
`builtins.staticmethod`, `builtins.classmethod`, `functools.cache`, `functools.lru_cache`,
`typing.final`, and `typing.override`. `typing.overload` marks a signature, `abc.abstractmethod` an
abstract method (`not_concrete`), and `property` and its setter accessors (`accessor`); any other
decorator (or one on the method's class) makes it a `wrapper`. Reassigning a decorator's module
attribute (`functools.cache = replacement`) disproves its identity.

### Positions

Argument positions are written ordinals counting positional, starred, keyword, and `**` arguments;
parameter positions count every named parameter, receivers included. An unresolved `parameter` call
records the owner's parameter position; matching ordinals never bind a caller's argument to a
parameter.

### Provenance and locators

For detectors and module labels the binder answers: a stable assignment initialized by a call names
its callee as the factory (`origin_declaration`), a parameter or variable with a written annotation
has annotation evidence, and receivers come from the evidence above. Manual locators look names up
NFKC-normalized: `path#name` among a module's top-level writes (exports first), `path#Owner.member`
among a class body's writes, with the handler context's concrete body.

## Module labels

An external module is labeled `python:<root>` when its first part is in the pinned standard-library
inventory (`unfold_lang_python::stdlib`, the top-level names of typeshed's `stdlib/` at commit
`a205439338a4ad3debec1eeae7d300e3781c066d`, bundled with Pyright 1.1.407, which the proof of concept
pinned) or is `builtins`; otherwise it is labeled with its first part (`fastapi`, `celery`). Builtin
names resolve as `python:builtins`, an unindexed repository file by its path. Updating the inventory
means listing a newer typeshed's `stdlib/` folder and recording its commit.

## Known limits

- **Structural proofs are per file.** Getter and setter kinds, `TYPE_CHECKING` erasure, and dynamic
  import blocks are decided at extraction, which can't see that a star import rebinds `property`,
  `TYPE_CHECKING`, or `importlib`, that another module writes `typing.TYPE_CHECKING`, or that a local
  `importlib.py` or `typing.py` shadows the standard module. The binder itself sees all of these, so
  only block kinds and occurrences can differ, never a target: a shadowed `importlib.import_module`
  call keeps its `import` block and resolves the literal as a module.
- **Namespace folders** are found from known files and the disk; a folder discovery never walked
  counts only if it exists on disk.
- **Order-dependent mutation state.** As in the proof of concept, the mutation passes resolve values
  with the state earlier steps left. Class descendants used during those passes come from a snapshot
  taken before them (and are rebuilt after), and linearizations, concreteness, and decorator identities
  computed during them are kept until the next change to invalidated members or modules; only programs
  that monkeypatch a class's base through another module could see a difference.
- **Recursion limits.** Resolution follows at most 256 nested steps (aliases of aliases, attribute
  chains); a longer chain withholds its answer.
- **Pattern captures** (`case P() as x`, `case {**rest}`) are writes, where the proof of concept
  missed them (a conservative fix: they can only withhold targets).

## Entrance detectors and problems

`PythonLinked` returns the four detectors from `Linked::detectors` ([entrances](../entrances.md#python-detectors)
has their rules) and reports the `pyproject.toml` files `pyproject.script` reads through `Linked::inputs_read`, so
they are fingerprinted even without scripts. The source detectors share one value flow per link
(`detect/origins/flow.rs`), computed only when an enabled detector's framework is imported somewhere.

`Linked::problems` lists every import whose module more than one source root (or a module and a package)
supplies, at the importing file, without making the graph incomplete (its edges are already `ambiguous`), and
every owning `pyproject.toml` the console-script detector can't read, which does make it incomplete. The engine
lists them in Problems with the configuration-input problems.

## Parity with the proof of concept

On `testdata/playgrounds/py-orders` (scoped by its `.unfold/config.toml`), the whole canonical export (files,
blocks, slots, edges, entrances, targets, and inputs) equals
`testdata/reference/canonical/python-playground.jsonl` record for record, in order, including the six named
child-import blocks; `cargo xtask parity python-playground` reports no difference and the divergence ledger has no
entry for this corpus. The 25 entrances and 28 targets match `testdata/playgrounds/py-orders/expected-entrances.txt`,
and the Python half of the mixed folder matches `testdata/playgrounds/expected-entrances.txt`. Against the proof
of concept's Pyright reference, 91 of its 96 resolved calls resolve to the same target (the PoC's locked recall
floor), and every one of the 91 resolved calls is correct. Other known differences, none of which occur in the
reference corpora (the detectors' own are listed in [entrances](../entrances.md#differences-from-the-proof-of-concept-1)):

- An expression statement in parentheses around a call or lambda (`(f(x))`) spans the parentheses,
  so its `expression` slot lies inside it (the proof of concept's block was the call's range).
- A `break`/`continue` in a function nested in a loop fails (see [Failures](#failures-and-encodings)).
- An encoding declaration on line 2 counts only when line 1 is blank or a comment, as in CPython;
  `utf-8-sig` is accepted.
- A dynamic import's argument must be a string, not bytes.
- Pattern captures and the per-file structural proofs, as listed under [Known limits](#known-limits).
