# Graph model

This document is the authoritative description of Unfold's graph: its records, vocabularies, text
coordinates, paths, structural keys, canonical export, and validation invariants. It replaces the
proof of concept's `docs/format.md` and the "Stable keys" section of its `docs/reader.md` for these
topics ([architecture §5.6](architecture.md#56-where-the-complete-semantics-live)). How each language
selects blocks and resolves edges lives in `docs/languages/`; detector rules live in
`docs/entrances.md`.

`crates/unfold-core` encodes everything here: `vocab` (vocabularies and per-kind rules), `path`,
`text` (snapshots, hashes, ranges, lines, UTF-16), `key`, `model`, `typed` (typed records),
`canonical`, and `validate`. The sections on the value policy, module labels, provenance, pairing,
and edges describe `crates/unfold-engine`'s shared backend.

## Records

| Record | What it is |
| --- | --- |
| File | An indexed source file: path, language, state, the hash its facts came from, and its failure if it doesn't parse |
| Input | A fingerprinted configuration file that shaped the graph: path, role, hash |
| Block | A selected syntax entity with a byte range, a parent in the same file, a role, an optional name, lines, and a structural key |
| Slot | The range of a written part of a block, whether or not that part has a block of its own |
| Edge | A relationship sourced at one occurrence block: a call, a construction, an import, or a named callable passed as an argument |
| Entrance | A declared way a process or unit of work starts, with its evidence and ordered targets |
| Entrance target | One independently resolved handler or startup target |
| Typed records | Per file: signatures, parameters, members, arguments, call results, and type links ([Typed records](#typed-records)) |

Occurrences are the sources of edges, not functions. Callers, owners, and other function-level
views are derived by the reader.

### File

| Field | Meaning |
| --- | --- |
| `path` | Folder-relative path ([Paths](#paths)) |
| `language` | `typescript` or `python` |
| `state` | `current`, `last_good`, or `failed` ([File states](#file-states-and-input-roles)) |
| `hash` | SHA-256 of the bytes the facts came from; absent only for a `failed` file |
| `failure` | The syntax or read error with its position; present exactly for `last_good` and `failed` |

A `current` or `last_good` file has blocks, starting with its root. A `failed` file has no hash,
blocks, slots, or edges; calls into it are `not_indexed`.

### Input

| Field | Meaning |
| --- | --- |
| `path` | Folder-relative path; never also an indexed file |
| `role` | `unfold-config`, `tsconfig`, `package`, or `pyproject` |
| `hash` | SHA-256 of its bytes |

### Block

| Field | Meaning |
| --- | --- |
| `local_id` | Position in the file's preorder; the root is 0. File-scoped and never persisted outside the store |
| `parent` | The nearest recorded syntax ancestor in the same file; absent only for the root |
| `kind` | From [block kinds](#block-kinds) |
| `role` | The relationship to the parent ([roles](#roles)); absent for the root or an unmapped relationship |
| `name` | Declared or syntactically inferred label; never empty and never identity; absent for imports |
| `range` | Token-exact byte range ([Text](#text-ranges-and-lines)); the root spans the whole file |
| `lines` | The one-based lines the range starts and ends on |
| `key` | The structural key ([Keys](#identity-and-structural-keys)) |

Blocks nest; they don't partition a file. Omitted brace and expression wrappers don't interrupt
containment: the saved ancestor stays the parent. Non-root ranges are nonempty. Call relationships
may form cycles; the parent tree never does.

### Slot

| Field | Meaning |
| --- | --- |
| `block` | The owning block |
| `slot` | From [slot names](#slot-names) |
| `index` | Contiguous zero-based index of a repeated slot; absent for single slots |
| `range` | Nonempty token-exact range inside the block's range |

A slot exists only when the part is written. It is independent of child blocks (a condition with no
call still has a range) and of roles on descendants. `(block, slot, index)` is unique. Each parameter
range includes its modifiers, decorators, binding, annotation, and default. A body is the block
statement, Python suite (including its opening colon), or concise function expression. A
`case`/`default` body spans its first through last statement; an empty clause has no body slot. A
tagged template has a callee slot and no argument slots. The TypeScript class `heritage` slot
records the first `extends` clause; Python `heritage` spans the written base/metaclass list.

### Edge

| Field | Meaning |
| --- | --- |
| `source` | The occurrence block: `call`, `new`, or `import` |
| `seq` | Position among the source's edges, contiguous from 0: the occurrence first, then references in argument order |
| `kind` | From [edge kinds](#edge-kinds-and-resolutions) |
| `target` | The target block's key, exactly when resolved |
| `text` | Short written callee, argument, or import specifier without quotes (`(…)` for a non-literal specifier); at most 120 UTF-16 code units |
| `resolution` | `resolved`, `external`, or `unresolved` |
| `reason` | Exactly when unresolved; from [reasons](#reasons) |
| `module` | Exactly when external; a [module label](#module-labels) |
| `position` | For a reference, its zero-based written argument position. For an unresolved `parameter` call or construction, the owner's parameter position (TypeScript runtime index, skipping an explicit `this`; Python written ordinal). Absent otherwise |

Every call, tagged template, construction, and runtime import has exactly one occurrence edge;
repeated occurrences have separate source blocks. References are emitted only for arguments that
resolve to a specific indexed callable or class, so they are always `resolved`, at most one per
argument position, and their positions need not be contiguous. Tagged templates produce no
references. Python keyword arguments keep their written ordinal and can supply references; `*args`
and `**kwargs` keep an argument slot but never produce references. An outer closure parameter leaves
`position` absent.

### Entrance

| Field | Meaning |
| --- | --- |
| `key` | The entrance key ([Keys](#identity-and-structural-keys)) |
| `kind` | From [entrance kinds](#entrances) |
| `label` | Display label; manual declarations only, never empty |
| `method` | HTTP method; `http` entrances only |
| `trigger` | The activating value; absent exactly when `trigger_status` is `unknown`, never empty |
| `trigger_status` | `exact`, `partial`, or `unknown` |
| `detector` | From [detector IDs](#entrances) |
| `evidence_path` | The source file or input that declares it |
| `evidence_range` | The declaration's byte range in that file |
| `evidence_block` | Source evidence: the key of the block with exactly that file and range |
| `evidence_pointer` | Configuration evidence: an RFC 6901 pointer into the parsed JSON or TOML value |

Source evidence is a registration call, construction, or decorated declaration. Configuration
evidence is an exact JSON member or array element, or a TOML table or assignment: a JSON property's
range runs from its key's opening quote through the end of its value, a TOML assignment's from its
key through its value. The pointer names the parsed value.

### Entrance target

| Field | Meaning |
| --- | --- |
| `position` | Contiguous zero-based declaration order within the entrance (not execution order) |
| `role` | `startup` for a process, `handler` for every other kind |
| `target` | The target block's key, exactly when resolved |
| `text` | The handler label, locator, or launch path as written; at most 120 UTF-16 code units |
| `resolution`, `reason`, `module` | As for edges |

Every entrance has at least one target; a process has exactly one. An unresolved handler never
removes its entrance. Several entrances can share a target.

### Typed records

What each call sends, returns, and where its value lands, and what written types say. Every typed
record belongs to one file and refers to blocks of that file by local ID (by key in the export).
Only what is written is recorded: a type is the annotation text as written, a pairing exists only
where the language's calling rules prove it, and a type link resolves only with proof of exactly one
declaration. Text fields are written source cut like edge labels to 120 UTF-16 units; ranges are
UTF-8 byte ranges, token-exact (a type link's range lies on character boundaries, since a Python
string annotation's name lies inside its quotes).

**Signature**: exactly one per function-like block.

| Field | Meaning |
| --- | --- |
| `block` | The function-like block |
| `returns` | The written return type (TypeScript: the annotation after `:`, without the colon; Python: the expression after `->`); present exactly when the block has a `return_type` slot |
| `returns_range` | Its range, the `return_type` slot's |

**Parameter**: exactly one per `parameter` slot, with the slot's index.

| Field | Meaning |
| --- | --- |
| `block`, `index` | The function-like block and the slot index |
| `kind` | From [parameter kinds](#parameter-kinds-member-kinds-and-pairing-reasons) |
| `name` | The identifier; absent for a destructuring pattern |
| `optional` | TypeScript `x?` (only when true) |
| `default` | A default value is written (only when true) |
| `type`, `type_range` | The written annotation and its range, inside the parameter slot |

**Member**: the written shape of a type declaration or class, in source order.

| Field | Meaning |
| --- | --- |
| `block`, `index` | An `interface`, `type_alias`, `enum`, or `class` block, and a contiguous index from 0 |
| `kind` | `property`, `method`, `index`, `call`, `construct`, or `value` (an enum member) |
| `name` | The name as written (`coupon`, `"x-id"`, `[Symbol.iterator]`); an index signature's parameter text (`[key: string]`); absent for call and construct signatures |
| `optional`, `readonly`, `default` | TypeScript `?` and `readonly`; a written initializer (class field) or default (Python); each only when true |
| `type`, `type_range` | The written type: a property's annotation, a method or call signature's text after its name (`(a: A): B`), an index signature's value type |
| `value` | An enum member's written initializer |

TypeScript records the members of interfaces, of an object type literal that is a type alias's whole
aliased type, and of enums, and a class's instance properties (with or without a type, `accessor`
fields included) and constructor parameter properties, not static members or methods (methods are
blocks). Python records every annotated assignment whose target is a plain name written directly in
a class body, as a `property`: dataclasses, `TypedDict`, pydantic models, `NamedTuple`, and plain
classes alike, since the record states what is written, not what a decorator or base makes of it.

**Argument**: exactly one per `argument` slot of a `call` or `new` block, with the slot's index.

| Field | Meaning |
| --- | --- |
| `call`, `index` | The occurrence block and the slot index |
| `expand` | `spread` (TypeScript `...x`, Python `*x`) or `keywords` (Python `**x`) |
| `keyword` | A Python keyword argument's name |
| `parameter` | The index of the parameter it fills, among the resolved target's `parameter` records |
| `unpaired` | Why it fills no proven parameter, from [pairing reasons](#parameter-kinds-member-kinds-and-pairing-reasons) |
| `bound` | For an argument that passes a callable (a resolved `references` edge at its position to a function-like block): how many of that callable's leading parameters the passed value binds itself (`obj.method` binds the instance, `Class.method` nothing), when proven |

Exactly one of `parameter` and `unpaired` is present when the occurrence edge (`seq` 0) is
`resolved`, and neither otherwise ([Pairing](#pairing)). `bound` is absent on every other argument,
and on a passed callable whose binding isn't proven.

**Result**: exactly one per `call` and `new` block: where its value lands after transparent layers
(parentheses and `await`; TypeScript also `!`, `as`, `satisfies`, and `<T>`).

| `binding` | When | `target` | `position` |
| --- | --- | --- | --- |
| `variable` | It is exactly the initializer of a `variable` block (`const charge = await f()`, Python `x = f()`, `x: T = f()`, `(x := f())`; for `a = b = f()` the innermost level) | The `variable` block | — |
| `return` | It is exactly a returned expression: a `return`'s, a Python lambda's body, or a TypeScript arrow function's expression body | — | — |
| `argument` | It is exactly an argument of another `call` or `new` (a Python keyword argument's value counts; an expanded one doesn't) | The outer call | The outer argument's slot index |
| `discard` | It is exactly the expression of an `expression_statement` | — | — |
| `other` | Anything else: a receiver, an operand, a condition, an assignment expression, an augmented assignment, a `throw`, a `yield`, a spread, … | — | — |

`awaited` (only when true) says an `await` was one of the transparent layers. `bound` is present
when the occurrence edge resolved to a function-like block and the language proved how many of its
leading parameters the call binds itself (a Python method called through an instance binds `self`,
an `__init__` reached by a construction binds the new instance); it is the same count
[pairing](#pairing) used.

**Type link**: a type name written in a recorded annotation, and the declaration it resolves to.

| Field | Meaning |
| --- | --- |
| `block` | The innermost block whose range contains the name: the function-like block for parameter and return types, the variable, the type declaration or class for members, the `type_alias` for its aliased type |
| `range` | The written name, a qualified name whole (`Stripe.PaymentIntent`), without type arguments |
| `text` | The name as written |
| `resolution`, `target`, `reason`, `module` | As for edges: a `class`, `interface`, `type_alias`, or `enum` block of the same language when resolved; a module label when external; a reason when unresolved |

The recorded annotations are exactly the ones typed records carry: parameter and return types,
member types, a type alias's aliased type, and variable annotations (TypeScript `const x: T`; Python
`x: T` anywhere). Every type reference in them gets one link, generic arguments included
(`Promise<Charge>` links `Promise` and `Charge`; `CartItem[]` links `CartItem`). Not links:
TypeScript keyword types, literal types, `this`, `typeof x` queries, and `import("m").T` types;
names that bind to a type parameter of an enclosing declaration (TypeScript `<T>`, `infer U`, a
mapped type's key; Python PEP 695 `[T]`), which extraction sees lexically; Python `None`, literals,
the arguments of a subscript spelled `Literal[...]`, and every argument after the first of one
spelled `Annotated[...]`. A Python string annotation is linked only when its whole content is a
dotted name (`"Cart"`, `"models.Cart"`), with the range inside the quotes. How each language
extracts and resolves them is in `docs/languages/`.

## Vocabularies

Every value below is serialized exactly as written.

### Block kinds

| Kind | Meaning |
| --- | --- |
| `file` | An indexed source file's root |
| `function` | A function declaration or expression with a body, an arrow function, or a Python lambda |
| `class` | A non-ambient class declaration or expression |
| `method`, `constructor`, `getter`, `setter` | A class or object member with a body |
| `variable` | A non-ambient variable declaration, or one name bound by destructuring |
| `call`, `new` | A call or tagged template, and a construction |
| `import` | A runtime module dependency (including dynamic `import()` and a proven literal `importlib.import_module`) |
| `if` | A condition and its written branches; an `else if` is a nested `if` with role `else` |
| `for`, `for_in`, `for_of`, `while`, `do_while` | Loops |
| `switch`, `case`, `default` | A switch and its clauses; Python `match` cases are `case` blocks |
| `try`, `catch` | A try statement and its handlers; `finally` is a slot |
| `return`, `throw`, `break`, `continue` | Transfers |
| `expression_statement` | A standalone expression |
| `with`, `match` | Python context managers and pattern matches |
| `interface`, `type_alias`, `enum` | A non-ambient TypeScript interface, type alias, and enum |

The *function-like* kinds are `function`, `method`, `constructor`, `getter`, and `setter`. The
*occurrence* kinds are `call`, `new`, and `import`. The *type declaration* kinds are `class`,
`interface`, `type_alias`, and `enum`: what a type name can resolve to, and the blocks that carry
members.

Python never has `new`, `for`, `for_in`, `do_while`, `switch`, `default`, `interface`,
`type_alias`, or `enum`: its `for`/`async for` is `for_of`, `raise` is `throw`,
`except`/`except*` is `catch`, and wildcard match clauses are `case`. TypeScript never has `with`
or `match`.

### Roles

A role is a block's relationship to its saved parent. The root has no role, and neither does a
block in an unmapped structural relationship.

| Role | Meaning relative to the parent |
| --- | --- |
| `body` | File statements, a callable, class, loop, or clause body, or a try body |
| `parameter` | Inside a callable's written parameter |
| `condition` | An if, loop, or case condition |
| `then`, `else` | A written if branch; in Python, `else` also applies to loops and try statements |
| `initializer`, `update`, `iterable` | A loop or variable initializer, a counted-loop update, a loop iterable |
| `catch`, `finally` | A try handler or cleanup region |
| `expression` | A returned, thrown, standalone, or switch expression |
| `binding` | A destructuring or catch binding |
| `callee`, `argument` | An invocation's target or argument |
| `heritage` | Class base syntax (TypeScript) |
| `decorator` | Reached through a TypeScript decorator |
| `header` | Python only: decorators, defaults, annotations, and class base/metaclass expressions, owned by the enclosing scope |
| `name` | Inside a computed callable or class name |

`header` is Python only. `else` under a loop or `try` is Python only.

### Slot names

| Slot | Meaning as written |
| --- | --- |
| `name` | Identifier, computed name, or binding pattern |
| `parameter` | One complete parameter |
| `return_type` | Callable return annotation |
| `body` | Block, suite, or expression body, or a clause's statement span |
| `heritage` | First TypeScript `extends` clause, or the Python base/metaclass list |
| `initializer` | Variable or loop initializer |
| `condition` | If, loop, or case condition (a Python case's pattern and guard) |
| `update` | Counted-loop increment |
| `iterable` | For-in/of expression |
| `then`, `else` | Written branches; in Python also loop and try `else` suites |
| `expression` | Switch, return, throw, or standalone expression; Python `match` subject or `with` item list |
| `callee` | Call or construction target, or template tag |
| `argument` | One complete argument |
| `catch`, `finally` | Try handler or cleanup |
| `binding` | Catch declaration |
| `specifier` | Import or re-export specifier, or a dynamic import's first argument |
| `type` | A type alias's aliased type |

Permitted slots per kind (repeated slots carry an index):

| Block kind | Slots |
| --- | --- |
| `function`, `method`, `constructor`, `getter`, `setter` | `name`, repeated `parameter`, `return_type`, `body` |
| `class` | `name`, `heritage` |
| `interface` | `name`, `heritage` (from `extends` through its last type), `body` (the braces) |
| `type_alias` | `name`, `type` |
| `enum` | `name`, `body` (the braces) |
| `variable` | `name`, `initializer` |
| `call`, `new` | `callee`, repeated `argument` |
| `if` | `condition`, `then`, `else` |
| `for` | `initializer`, `condition`, `update`, `body` |
| `for_in`, `for_of` | `initializer`, `iterable`, `body`; Python `for_of` also `else` |
| `while`, `do_while` | `condition`, `body`; Python `while` also `else` |
| `switch` | `expression`, `body` |
| `case` | `condition`, `body` |
| `default` | `body` |
| `try` | `body`, `catch`, `finally`; Python also `else`, and its `catch` is repeated and indexed |
| `catch` | `binding`, `body` |
| `with`, `match` | `expression`, `body` |
| `return`, `throw`, `expression_statement` | `expression` |
| `import` | `specifier` |
| `file`, `break`, `continue` | None |

### Edge kinds and resolutions

| Edge kind | Source | Resolved target kinds |
| --- | --- | --- |
| `calls` | `call` | `function`, `method`, `constructor`, `getter`, `setter`, `class` |
| `constructs` | TypeScript `new`, or a Python `call` with proven class identity | `function`, `method`, `constructor`, or a `class` that declares no direct constructor |
| `imports` | `import` | `file` |
| `references` | `call` or `new`, at an argument position | `function`, `method`, `class` |

A construction of a class that declares a constructor targets that constructor block. Inherited
constructors are not promoted. Resolved edges never cross languages.

| Resolution | Meaning |
| --- | --- |
| `resolved` | One proven target in the graph |
| `external` | A proven origin outside the graph, with a module label |
| `unresolved` | No reliable target under the precision policy, with a reason |

### Reasons

| Reason | Meaning |
| --- | --- |
| `unknown_name` | No declaration exists for the name, and it isn't a known global |
| `unknown_receiver` | The type of the member's receiver is unknown |
| `parameter` | The called value comes from a parameter |
| `mutable_binding` | Writes to the binding or member prevent a stable target |
| `not_concrete` | No proven concrete body: an interface or type member, an abstract or ambient declaration, an overload with no implementation, or an opaque value |
| `accessor` | The call invokes an accessor's returned value, not the accessor itself |
| `ambiguous` | More than one body could be the target |
| `overridden` | Subclasses in the graph can supply different implementations |
| `unbound_this` | `this` inside a non-arrow function that isn't a member |
| `not_indexed` | The target exists in the folder but outside the indexed files (excluded, or failed to parse) |
| `not_found` | A relative import, launch path, or locator names nothing |
| `dynamic` | No static shape: a computed key, a call result, a non-literal path |
| `wrapper` | A call result, decorator, or partial supplies the callable without proving which body it is |
| `unsupported` | A recognized form that the supported syntax doesn't cover, such as a spread |

### Parameter kinds, member kinds, and pairing reasons

| Parameter kind | Meaning |
| --- | --- |
| `positional` | An ordinary parameter: it takes a positional argument (in Python also a keyword argument of its name) |
| `this` | TypeScript's explicit `this` parameter, which takes no argument |
| `rest` | TypeScript `...x`, Python `*args`: every further positional argument |
| `positional_only` | Python, before `/` |
| `keyword_only` | Python, after `*` or `*args` |
| `keywords` | Python `**kwargs`: every further keyword argument |

`this` is TypeScript only; `positional_only`, `keyword_only`, and `keywords` are Python only, and
the argument expansion `keywords` (`**x`) is Python only.

| Member kind | Meaning |
| --- | --- |
| `property` | A property or field |
| `method` | A method signature |
| `index` | An index signature |
| `call`, `construct` | A call or construct signature |
| `value` | An enum member |

| Pairing reason | Meaning |
| --- | --- |
| `spread` | This argument, or an earlier positional one, is expanded |
| `no_parameter` | No parameter of the target takes it |
| `receiver` | It can't be proven whether the call binds the target's first parameter itself |
| `no_signature` | The target declares no parameters of its own: a class without a direct constructor that has a base |

Result bindings are `variable`, `return`, `argument`, `discard`, and `other` ([Typed records](#typed-records)).

### Module labels

A module label is one nonempty plain label, or, when several origins are possible, a compact JSON
array of two or more distinct nonempty labels sorted by UTF-16 code units, for example
`["client-a","client-b"]`. A plain label never starts with `[`. Labels name a package (`express`,
`@scope/name`), a Node built-in (`node:fs`), `global`, or a repository-relative path for a TypeScript
import of an unindexed file. Python labels are a top-level package name, `python:<stdlib-root>`, or
`python:builtins`. Unindexed repository callables are `not_indexed`, never path-labeled.

### Entrances

| Kind | Trigger | Targets |
| --- | --- | --- |
| `process` | Script or bin name, or a declared trigger | Exactly one `startup` |
| `command` | Subcommand | One or more `handler`s |
| `http` | Route path, with a method | One or more `handler`s |
| `worker` | Queue or topic | One or more `handler`s |
| `schedule` | Schedule expression | One or more `handler`s |
| `event` | Event name | One or more `handler`s |

HTTP methods are `GET`, `HEAD`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`, and `ALL`. Trigger
statuses are `exact` (the whole trigger is known), `partial` (only part of it), and `unknown`
(exactly when the trigger is absent).

| Detector | Language | Entrance kinds | Evidence |
| --- | --- | --- | --- |
| `manual` | Either | Any | An `unfold-config` input (`.unfold/config.toml`), with a pointer |
| `package.script` | TypeScript | `process` | A `package` input, with a pointer |
| `package.bin` | TypeScript | `process` | A `package` input, with a pointer |
| `express.route` | TypeScript | `http` | A source block |
| `bullmq.worker` | TypeScript | `worker` | A source block |
| `node-cron.schedule` | TypeScript | `schedule` | A source block |
| `pyproject.script` | Python | `process` | A `pyproject` input, pointer `/project/scripts/<name>` |
| `fastapi.route` | Python | `http` | A source block |
| `celery.task` | Python | `worker` | A source block |
| `apscheduler.job` | Python | `schedule` | A source block |

| Target role | Detector | Resolved target kinds |
| --- | --- | --- |
| `startup` | All except `pyproject.script` | `file`, standing for its module initialization |
| `startup` | `pyproject.script` | `function`, `method`: the declared console callable |
| `handler` | Framework detectors | `function`, `method` |
| `handler` | `manual` | `function`, `method`, `if`, `case`, `default` (branches support explicit CLI dispatch) |

A detector's resolved targets are in its language. One entrance never mixes resolved targets of
different languages.

### File states and input roles

| State | Meaning |
| --- | --- |
| `current` | The facts come from the bytes on disk |
| `last_good` | The file no longer parses; the facts come from the last version that did |
| `failed` | No version has parsed; the file has no facts |

| Input role | File |
| --- | --- |
| `unfold-config` | `.unfold/config.toml` |
| `tsconfig` | A `tsconfig.json` or a file in its `extends` chain |
| `package` | A `package.json` the package detectors read |
| `pyproject` | A `pyproject.toml` used by console scripts or Python project settings |

## Text, ranges, and lines

- Each update reads a file's bytes once, hashes them with SHA-256, and decodes them strictly as
  UTF-8. Invalid UTF-8 fails the file. A byte-order mark is kept as U+FEFF; line endings and
  Unicode normalization are preserved, so the decoded text and the bytes are the same sequence.
  Files larger than 4 GiB (`u32::MAX` bytes) fail. A Python file that declares a non-UTF-8 encoding
  fails even if its bytes are valid UTF-8.
- Offsets are zero-based UTF-8 byte offsets, and ranges are half-open `[start, end)`. Every offset
  is on a character boundary.
- A non-root block and every slot are token-exact: they begin at a token and end immediately after
  the final token, excluding leading comments, indentation, and trailing whitespace; internal
  formatting and comments stay. A file root spans the whole text, including its BOM, comments, and
  final newline. Configuration evidence uses the same conventions.
- Lines are one-based. CR, LF, CRLF, U+2028, and U+2029 end a line, and CRLF counts once. A line
  terminator belongs to the line it ends. For a nonempty range, the start line contains `start` and
  the end line contains `end - 1`; an empty file has range `[0, 0)` and lines `[1, 1]`.
- The UI only receives UTF-16 offsets. `unfold-core`'s `Utf16Index` converts a file's byte offsets
  to UTF-16 and back in O(log n); a byte offset inside a UTF-8 sequence, or a UTF-16 offset between
  the halves of a surrogate pair, is an error, never silently rounded.
- Labels (`text` fields) are at most 120 UTF-16 code units. Truncation keeps the longest prefix that
  fits and never splits a surrogate pair.
- Python name identity uses NFKC; names and ranges keep the written spelling.

## Paths

Paths are relative to the project's folder, nonempty, `/`-separated, and case-preserving. They contain no
empty, `.`, or `..` component, no backslash, no NUL, and no drive or absolute prefix, so they never
escape the root. Paths order by their UTF-8 bytes.

## Identity and structural keys

Within one graph database a block is identified by `(file_id, local_id)`; local IDs are numbered in
preorder within each file, so changing a file renumbers only that file. Everything persisted outside
the store (boards, the canonical export, resolved targets across files) uses **structural keys**,
which survive most edits.

| Key part | Derivation |
| --- | --- |
| File root | `path#` |
| Named step | `kind:name`, with reserved characters escaped, and `[n]` when n ≥ 1 earlier siblings have the same kind and name |
| Unnamed step | `kind[n]`: n is the number of earlier siblings of that kind, named or not |
| Nested block | The parent key, then `/step`; a root's children follow `#` directly, with no `/` |
| Entrance | `entrance:` and a compact JSON array of detector, kind, method, trigger, and first target text (absent values as `""`), then `#n` when n ≥ 1 earlier entrances have the same array |

- Siblings are counted in preorder, which orders them by start offset. An empty name counts as
  unnamed.
- In names, `%`, `/`, `#`, `[`, and `]` are percent-encoded in uppercase hex (`%25`, `%2F`, `%23`,
  `%5B`, `%5D`), as JavaScript's `encodeURIComponent` writes them. Nothing else is escaped, so a
  step never contains `/` or `#`, and the parent of a key is everything before its last `/` after
  the file's `path#`.
- The entrance array is escaped exactly as JavaScript's `JSON.stringify` does: `"` and `\` with a
  backslash, `\b`, `\f`, `\n`, `\r`, `\t`, other control characters as lowercase `\u00xx`, and
  everything else as written. Duplicate numbering follows the canonical entrance order.
- Keys are derived, never chosen: the validator re-derives every block and entrance key.

Examples, from the proof of concept's playgrounds:

| Key | Names |
| --- | --- |
| `ts-playground/src/lib/validate.ts#` | The file root |
| `ts-playground/src/lib/validate.ts#function:validateOrder/if[1]` | The second `if` directly in `validateOrder` |
| `ts-playground/src/lib/validate.ts#function:validateOrder/variable[0]/variable:email` | `email`, bound by the function's first (unnamed, destructuring) variable |
| `py-playground/src/orders_playground/features/decorators.py#function:stringify[2]` | The third top-level function named `stringify` |
| `py-playground/src/orders_playground/db/repository.py#class:OrderRepository/method:insert/variable:self.orders%5Border%5B"id"%5D%5D` | The assignment to `self.orders[order["id"]]` |
| `entrance:["express.route","http","POST","/orders","requireUser"]` | The first `POST /orders` route whose first handler is `requireUser` |
| `entrance:["bullmq.worker","worker","","","(…)"]` | A worker with an unknown queue and a non-literal processor |

Ordinal steps such as `if[0]` shift when code is inserted before them, so boards protect them with
guards and signatures.

## Order

- Files are in path order. Blocks are in preorder within each file, with siblings in start order.
- Slots are ordered by block, then start. Edges are ordered by source, then `seq`: an occurrence
  precedes its references, which follow in argument order.
- Signatures, parameters, members, arguments, and results are ordered by block (preorder), then
  index; type links by block, then range.
- Entrances sort by evidence path, evidence start, evidence end, then detector, kind, method, and
  trigger compared as their vocabulary strings (absent values as empty strings). Ties keep detector
  order. Targets follow their entrance in position order.
- Source order is an outline, not an execution order.

## Canonical export

The canonical export is a deterministic JSON Lines form of the whole graph, used for snapshots,
parity with the proof of concept, determinism checks, and debugging. Records refer to structural
keys, never local IDs. Reading an export and writing it again gives identical bytes, and the output
depends only on the graph's contents.

**Encoding.** UTF-8, one JSON object per line, each line ending in LF, including the last. Objects
are compact (no spaces), with fields in the order listed below. Optional fields are omitted when
absent, never written as `null` or `""`. Strings are escaped as `serde_json` writes them (`"`, `\`,
and control characters; non-ASCII is written as is). Ranges and lines are two-element arrays.

**Sections**, in order:

1. One `header` record.
2. For each file in path order: its `file` record, then its `block`s in preorder, its `slot`s (by
   block, then range, slot name, index), its `edge`s (by source, then `seq`), and its typed
   records: `signature`, `parameter`, `member`, `argument`, and `result` records (each by block,
   then index), then `type_link`s (by block, then range).
3. Every `input` in path order.
4. Every `entrance` in canonical order, each followed by its `target`s in position order.

**Records** (fields in order; `?` marks an optional field):

| `t` | Fields |
| --- | --- |
| `header` | `format` (`"unfold-graph"`), `version` (`2`) |
| `file` | `path`, `language`, `state`, `hash`?, `failure`? |
| `block` | `key`, `kind`, `role`?, `name`?, `range`, `lines` |
| `slot` | `block` (key), `slot`, `index`?, `range` |
| `edge` | `source` (key), `seq`, `kind`, `target`? (key), `text`, `resolution`, `reason`?, `module`?, `position`? |
| `signature` | `block` (key), `returns`?, `returns_range`? |
| `parameter` | `block` (key), `index`, `kind`, `name`?, `optional`?, `default`?, `type`?, `type_range`? |
| `member` | `block` (key), `index`, `kind`, `name`?, `optional`?, `readonly`?, `default`?, `type`?, `type_range`?, `value`? |
| `argument` | `call` (key), `index`, `expand`?, `keyword`?, `parameter`?, `unpaired`?, `bound`? |
| `result` | `call` (key), `binding`, `awaited`?, `target`? (key), `position`?, `bound`? |
| `type_link` | `block` (key), `range`, `text`, `resolution`, `target`? (key), `reason`?, `module`? |
| `input` | `path`, `role`, `hash` |
| `entrance` | `key`, `kind`, `label`?, `method`?, `trigger`?, `trigger_status`, `detector`, `evidence_path`, `evidence_range`, `evidence_block`? (key), `evidence_pointer`? |
| `target` | `entrance` (key), `position`, `role`, `target`? (key), `text`, `resolution`, `reason`?, `module`? |

Hashes are 64 lowercase hex digits. Flags (`optional`, `readonly`, `default`, `awaited`) are
written only when true, and a written type's text and range appear together. A block's local ID is
its position among its file's block records, and its parent follows from its key. A reader rejects
unknown record types, unknown fields, fields that belong to another record type, a flag written
`false`, an argument with both `parameter` and `unpaired`, records out of section order, and slots,
edges, typed records, or blocks that name a block outside the current file (a `type_link`'s and an
edge's `target` may name any block).

**Version 1.** Exports written before typed records existed have `version` `1` and no typed records
(the frozen proof-of-concept references are version 1). The reader accepts them and refuses typed
records in them; `canonical::write_version` writes a graph at version 1 (without its typed records)
so such an export round-trips, and the validator checks it without requiring typed records
(`Validator::without_typed_records`). `canonical::version_one` is a graph as a version-1 export
describes it: no typed records and no `interface`, `type_alias`, or `enum` blocks, with the blocks
nested in a removed enum (calls in member initializers) moved up to its parent and every key derived
again. Parity with the proof of concept compares that view with the references.

An excerpt of the export of `unfold_core::testkit::sample` (hashes shortened), and of
`unfold_core::testkit::sample::typed`'s typed records:

```text
{"t":"header","format":"unfold-graph","version":2}
{"t":"file","path":"src/orders.ts","language":"typescript","state":"current","hash":"131293a2…"}
{"t":"block","key":"src/orders.ts#","kind":"file","range":[0,230],"lines":[1,12]}
{"t":"block","key":"src/orders.ts#function:createOrder","kind":"function","role":"body","name":"createOrder","range":[43,159],"lines":[3,8]}
{"t":"slot","block":"src/orders.ts#function:createOrder","slot":"parameter","index":0,"range":[64,76]}
{"t":"edge","source":"src/orders.ts#function:createOrder/return[0]/call[0]","seq":0,"kind":"calls","target":"src/orders.ts#function:save","text":"save","resolution":"resolved"}
{"t":"edge","source":"src/orders.ts#function:createOrder/return[0]/call[0]","seq":1,"kind":"references","target":"src/helper.ts#function:helper","text":"helper","resolution":"resolved","position":1}
{"t":"signature","block":"src/orders.ts#function:createOrder"}
{"t":"parameter","block":"src/orders.ts#function:createOrder","index":0,"kind":"positional","name":"req","type":"unknown","type_range":[69,76]}
{"t":"argument","call":"src/orders.ts#function:createOrder/return[0]/call[0]","index":0,"parameter":0}
{"t":"result","call":"src/orders.ts#function:createOrder/return[0]/call[0]","binding":"return","bound":0}
{"t":"input","path":"package.json","role":"package","hash":"811bd953…"}
{"t":"entrance","key":"entrance:[\"express.route\",\"http\",\"POST\",\"/orders\",\"createOrder\"]","kind":"http","method":"POST","trigger":"/orders","trigger_status":"exact","detector":"express.route","evidence_path":"src/server.ts","evidence_range":[0,32],"evidence_block":"src/server.ts#expression_statement[0]/call[0]"}
{"t":"target","entrance":"entrance:[\"express.route\",\"http\",\"POST\",\"/orders\",\"createOrder\"]","position":0,"role":"handler","target":"src/orders.ts#function:createOrder","text":"createOrder","resolution":"resolved"}

{"t":"member","block":"src/cart.ts#interface:Cart","index":1,"kind":"property","name":"coupon","optional":true,"type":"string","type_range":[49,55]}
{"t":"type_link","block":"src/cart.ts#interface:Cart","range":[30,36],"text":"CartId","resolution":"resolved","target":"src/cart.ts#type_alias:CartId"}
{"t":"argument","call":"jobs/settle.py#expression_statement[0]/call[0]","index":1,"keyword":"currency","parameter":1}
{"t":"type_link","block":"src/charge.ts#function:charge","range":[86,93],"text":"Promise","resolution":"external","module":"global"}
```

`cargo xtask canonical-check <file>` reads an export, validates it, writes it again at its own
version, and fails unless the bytes are identical.

## Validation invariants

Every compile and update validates the new graph before committing it; a failed validation fails
the update and the previous graph stays. The validator reports every violation, each naming its
record, and never panics. A successful validation doesn't prove correct dispatch, inferred types,
or runtime reachability.

**Files and inputs**

- File paths and input paths are each strictly increasing; no path is both a file and an input.
- File state, hash, failure, and facts agree as described under [File](#file); a failed file has
  no typed records either.
- Every block key is unique in the graph.

**Blocks**

- `local_id` equals the preorder position. Block 0 is the only `file` block, with no parent, role,
  or name, starting at byte 0 on line 1.
- Every other block names an open ancestor as its parent (preorder), and the parent's range and
  lines contain the child's. Siblings are in start order. Non-root ranges are nonempty.
- Kinds and roles are permitted for the file's language. Names are never empty; imports have none.
- Lines are one-based and ordered. Every key equals its derivation.

**Slots**

- The slot is permitted for the block's kind and language. Repeated slots have indices contiguous
  from zero in order; single slots have no index and don't repeat. Slots are in block, then start,
  order, and each range is nonempty and inside its block.

**Edges**

- Edges are in source, then `seq`, order, with `seq` contiguous from zero per source. Sources are
  occurrence blocks.
- The occurrence kind matches the source: `calls` for a call (or `constructs` for a Python call),
  `constructs` for `new`, `imports` for `import`. Every occurrence block has exactly one
  occurrence edge, which precedes its references.
- References come from a `call` or `new`, are resolved, and have strictly increasing positions
  within the source's argument count.
- An occurrence's `position` appears only on an unresolved `calls` or `constructs` edge with reason
  `parameter`, within its owner's parameter count. The owner is the nearest ancestor that is the
  file, a class (unless reached through a Python `header`), or a function-like block reached
  through its body or a parameter.
- `reason` appears exactly when unresolved, `module` exactly when external (and is a valid label),
  `target` exactly when resolved. A resolved target exists, has a permitted kind, is in the same
  language, and, for `constructs` of a class, the class declares no direct constructor.
- `text` is at most 120 UTF-16 code units.

**Typed records**

- Signatures and parameters: exactly one `signature` per function-like block (and none on other
  blocks) and one `parameter` per `parameter` slot, with the slot's index, in order. `returns` is
  present exactly when the block has a `return_type` slot, and `returns_range` lies in it;
  `type_range` lies in its parameter slot. Kinds are permitted for the language; a `this` parameter
  is the first; Python's are in the language's order (positional-only, positional, `*args`,
  keyword-only, `**kwargs`), with at most one `rest` and one `keywords`; only TypeScript parameters
  are optional, and never a rest one. Names are nonempty when present.
- Members: only on `interface`, `type_alias`, `enum`, and `class` blocks; indices contiguous from 0
  per block, in order; type ranges inside the block; `value` members are exactly the members of an
  enum, and only they have a `value`; only TypeScript members are optional or readonly.
- Arguments: exactly one per `argument` slot of a `call` or `new`, with its index, in order;
  `expand: keywords` and `keyword` are Python only, and an expanded argument has no keyword.
  Pairing fields are present exactly when the occurrence edge is resolved, exactly one of them; a
  `parameter` names a parameter of the target (a function-like target) whose kind takes an argument
  (never `this`); a `class` target's arguments are only unpaired. A `bound` count appears only on an
  argument whose position has a resolved `references` edge to a function-like block.
- Results: exactly one per `call` and `new` block; `target` present exactly for `variable` and
  `argument` and names a block of that kind (`variable`; `call` or `new`) that is an ancestor of the
  call; `position` present exactly for `argument`, below the outer call's argument count, and that
  argument slot contains the call. A `bound` count appears only when the occurrence edge resolved
  to a function-like block.
- Type links: ordered by block, then range; each range nonempty, inside its block, not inside a
  nearer nested block, and not overlapping another link; outcome fields as for edges (`target`
  exactly when resolved, naming a `class`, `interface`, `type_alias`, or `enum` block of the same
  language; `reason` exactly when unresolved; a valid `module` exactly when external); `text`
  nonempty and at most 120 UTF-16 code units.
- A graph read from a version-1 export is validated without requiring typed records
  ([Canonical export](#canonical-export)); any it has are still checked.

**Entrances**

- Entrances are in canonical order, and every key equals its derivation.
- The detector produces that kind; only `http` has a method; the trigger and its status agree;
  only `manual` has a label.
- Evidence is where the detector's table says: configuration evidence is an input of the right
  role with a valid pointer (`~` only as `~0` or `~1`) and no block; source evidence is an indexed
  file of the detector's language with no pointer, and its block exists with exactly the evidence's
  file and range, inside the file.
- Targets have contiguous positions and the role their kind requires; outcome fields as for edges;
  resolved targets exist with a permitted kind, in the detector's language; resolved targets don't
  mix languages; every entrance has a target and a process exactly one.

**With snapshots** (the bytes the facts came from, supplied by the caller):

- Every current file and every input has a snapshot whose hash matches.
- The root spans the whole text. Every block's lines match the text. Block and slot ranges lie on
  character boundaries and don't start with whitespace or a comment (`//` or `/*` in TypeScript,
  `#` in Python), or end with whitespace.
- Evidence ranges lie inside their file, on character boundaries.
- Written-type ranges (`returns_range`, `type_range`) and type-link ranges lie on character
  boundaries. These checks name the file, since only the files read in an update are checked
  against their bytes.

**With a language census** (computed independently by the front end from the same bytes):

- Block and slot ranges start and end on token boundaries.
- Call sites and `call`/`new` blocks correspond one to one by range and kind, and the occurrence edge
  kind agrees with the front end's (for example Python `constructs`). No reference uses an argument
  the front end marks ineligible, such as an expansion.
- Runtime module dependencies and `import` blocks correspond one to one by range.
- Written-type ranges start and end on token boundaries. (Type-link ranges are not checked against
  tokens: a Python string annotation's name lies inside its string token.)

Pointer identity for configuration evidence (that the pointer names the member at exactly that
range) needs a JSON or TOML parser, and is checked by the detectors' crates rather than
`unfold-core`.

## Python-specific rules

- Python calls stay `call` blocks; only stable class evidence turns their occurrence edge into
  `constructs`, which targets a direct concrete `__init__`, otherwise the class. The front end answers this
  at link time (`Linked::invocation`; see [Compiling](compiling.md), "Link-time decisions").
- Decorated declaration ranges run from the first `@` through the final body token. Decorators,
  defaults, annotations, and class base/metaclass expressions have role `header` and belong to the
  enclosing scope, including lambda defaults; calls in annotations are potential relationships, not
  claims of eager evaluation.
- Argument and parameter positions are written ordinals, including keyword and expanded arguments
  and receiver parameters. Equal argument and parameter ordinals don't prove caller binding; only
  [pairing](#pairing) does, by the calling rules.
- Comprehensions and generator expressions create binding scopes without a graph kind, so their
  calls keep the enclosing owner; calls in a lambda's body belong to the lambda.
- `from x import a, b` is one parent module occurrence; each proven child-module import adds an
  `import` block over its written item, a `specifier` slot over the imported name, and one `imports`
  edge whose text joins the parent specifier and the name (keeping relative dots). Whether a name is a
  child import depends on other files, so these blocks are conditional: extraction selects every
  candidate and linking keeps only the proven ones (`Linked::keeps_import`). `import a, b`
  has separate item occurrences. Proven `TYPE_CHECKING` imports are erased.

## Value policy and reason order

One shared policy (`unfold_engine::resolve`) turns language evidence into outcomes. Languages answer
narrow questions over program IDs (`Linked`); they can withhold a target, never invent one, and they
never choose a reason themselves.

| Context | Used for | Permitted target |
| --- | --- | --- |
| `call` | A callee | A concrete callable body |
| `construct` | A `new` callee, or a TypeScript `super(...)` (which keeps its `call` block and `calls` edge) | A callable function or method, a class's direct constructor with a body, or otherwise the class |
| `reference` | A named argument | A concrete `function`, `method`, or `class` |
| `handler` | An entrance target expression | A concrete `function` or `method`, including an inline function; a class is `unsupported` |
| `type` | A type name in an annotation | A `class`, `interface`, `type_alias`, or `enum` declaration |

Before any evidence is consulted, reference and handler values are checked by shape: a call result
as a handler is `wrapper`; an inline function or class is its own block (subject to steps 14–15); any
shape other than an identifier or member access is `dynamic`. A call site uses its authoritative
callee evidence; a type name uses type evidence (`Linked::type_evidence`: the name looked up in the
type meaning, through type-only imports, namespace members, and qualified names); every other value
uses value evidence. A type name is shaped like an identifier, or like a member access when it is
qualified, for step 7's reasons.

*Effective candidates* are the candidates that are not augmentations (namespace or type declarations
merged into a value), or every candidate when all of them are augmentations. An *unindexed repository
candidate* is an excluded one or an external one located at a repository file. The tests below run in
order, and the first that matches decides:

| Step | Test | Outcome |
| --- | --- | --- |
| 1 | The language refuses (`wrapper`, `dynamic`, `unsupported`, `not_concrete`) | That reason |
| 2 | The evidence is ambiguous | `ambiguous` |
| 3 | `this` is unbound (a non-arrow function that isn't a member) | `unbound_this` |
| 4 | The access root's import binding resolved to `not_found`, `not_indexed`, or `ambiguous` | That reason |
| 5 | An effective candidate is an unindexed repository candidate | `not_indexed` |
| 6 | The import binding is an external package (not at a repository file, written with a package specifier) and no effective candidate is indexed | `external` |
| 7 | No non-augmentation candidates | `external` when only external augmentations exist; otherwise `unknown_name` for an identifier, `unknown_receiver` for a member access, `dynamic` for anything else |
| 8 | Every candidate is external / origins are mixed | `external` / `ambiguous` |
| 9 | An indexed candidate has no declaration the language can name | `unknown_name` |
| 10 | The declarations have more than one distinct concrete target block | `ambiguous` |
| 11 | No concrete target | `parameter` if any declaration is a parameter, else `mutable_binding` if any binding is rebindable, else `accessor` if any is an accessor, else `not_concrete`, whatever the candidates' order |
| 12 | A non-concrete declaration sits beside the one target and is not a compatible overload signature of its implementation | `ambiguous` |
| 13 | The language reports a dispatching receiver family, or the override census finds an override | `overridden` |
| 14 | A class handler, a reference or handler target whose kind isn't `function`, `method`, or `class`, or a type target that isn't a type declaration | `unsupported` |
| 15 | Otherwise | `resolved` to the one target |

**Parameter positions.** An unresolved `parameter` call records `position` only in the `call` context
of a call site (never for constructions), only when exactly one containing parameter is involved
(destructured bindings report their whole parameter), and only when that parameter belongs to the
call's owner: the nearest ancestor that is the file, a class (unless reached through a Python
`header`), or a function-like block reached through its body or a parameter. An outer closure's
parameter leaves it absent. The position is the language's runtime position: TypeScript skips an
explicit `this` parameter (fixing C-4), and Python counts written ordinals including receivers. The
shared policy never adjusts it.

**Overload compatibility.** A signature is compatible with an implementation when the language says it
has the same owner, name, and declaration category and no body. Several compatible signatures beside
one body still resolve.

**Override census.** Built once per link from the language's class shapes: for every class, the names
of the overriding members (non-static members that can override: methods, callable fields, parameter
properties; not constructors or ambient declarations) declared by any proper descendant, through every
declared base, plus whether any of them has an unknown computed name. A call to a member that the
language marks for override checks (instance methods and callable fields) is `overridden` when the
receiver's class (from evidence, or else the member's owner) has a descendant overrider with the same
name, or one with an unknown name, or when the member's own name is unknown and any overrider exists.
`super` and static accesses, static members, private names, and members not listed in their owner's
shape are exact.

## Module labels

A module label names an outcome the policy already classified `external`; it never establishes a
target. Languages spell labels (`Linked::label_for_specifier`, `label_for_location`, `global_label`);
the shared order of rules is fixed, and the first rule that applies wins:

1. **Import-rooted.** An external package import binding of the access root names its package
   (`express/router` → `express`, `@scope/name/sub` → `@scope/name`, `fs` → `node:fs`). Relative
   imports, path aliases, and boundaries at repository files don't qualify.
2. **Origin-rooted.** The bounded [provenance](#provenance) of the callee's receiver names its package,
   ahead of any declaration location: `app.get` on `const app = express()` names `express`. Type
   names have no provenance.
3. **Ambient global.** An identifier root with no import binding and no declaration in indexed source
   names `global` (Python: `python:builtins`), as for `console` or `Math`, or the type names
   `Promise` and `Partial`. A declared root, such as a local parameter, never does (fixing C-6).
4. **Declaration locations.** The labels of the effective candidates' locations (a library is
   `global`; a package is its public name, so `@types/a__b` is `@a/b`), deduplicated: one plain label,
   or a sorted JSON array for several.

An import occurrence that resolves outside the repository is labeled from its package specifier when
it has one, and from its location otherwise; an existing unindexed repository file is labeled with its
repository-relative path. An external outcome without module evidence, or a label that is empty or
starts with `[`, is a broken contract between the language and the backend: the update fails rather
than guessing a package.

## Provenance

Provenance (`unfold_engine::provenance`) proves a narrow import lineage: an external module, the
imported export (`default`, `*`, or a name), and a path of steps. Detectors use it to recognize
framework objects, and module-label rule 2 uses it; it never makes anything callable. The walk spends
at most three steps, including the terminal import or annotation; transparent wrappers are already
unwrapped in the facts and spend nothing.

| Value or declaration | Step |
| --- | --- |
| Property access | Trace the receiver with one fewer step, then append the member name |
| Identifier bound by an external package import | The binding's export and an empty path, unless an indexed, excluded, or repository-file candidate contradicts it |
| Identifier with exactly one indexed declaration that is a non-ambient constant initialized by a call or construction | Trace the initializer's callee with one fewer step, then append `()` |
| Identifier with exactly one indexed declaration that carries a written annotation | The annotation's external package import, with path `[":type"]` |
| Anything else (element access, `let`/`var` factories, return values, general data flow) | No provenance |

Ambiguous evidence, unbound `this`, no candidates, and mixed origins stop the walk; visited values and
declarations stop cycles. A namespace import keeps export `*`. For a callee, the walk starts at the
callee's receiver (or at the callee itself when it isn't an access), so the invoked member doesn't
spend the budget: an imported `express` called once gives `default` + `["()"]`; a constant
initialized from `express.Router()` gives `default` + `["Router", "()"]`, which spends all three steps.

The walk is language-neutral: each language answers its questions through `Linked` (for Python, the binder
supplies factories and annotations; see [Python](languages/python.md#provenance-and-locators)), and module-label
rule 2 uses it in both languages. The TypeScript detectors recognize framework objects through it
([entrances](entrances.md#framework-provenance)). The Python detectors don't: they prove framework identity
through the binder's external origin of an expression, with a negative census that withholds the proof when
anything could replace the API, which is not bounded to three steps
([entrances](entrances.md#framework-provenance-1)).

## Edges and labels

Each file's edges follow its occurrence blocks in block order. A `call` or `new` block's occurrence
edge (`seq` 0) is followed by the references of its named arguments in argument order; an `import`
block has exactly its `imports` edge. Occurrences are resolved in parallel and reassembled by input
index, so edge order never depends on scheduling.

- **References.** Only arguments whose unwrapped shape is an identifier or a member access are
  resolved, in the `reference` context, and only a `resolved` outcome emits an edge. Every written
  argument advances the position, including inline functions (child blocks, found through argument
  slots), spreads, literals, and arguments the language marks ineligible (Python `*args` and
  `**kwargs`), none of which produce references. Tagged templates have no arguments. Override guards
  apply to references as to calls.
- **Imports.** An indexed module targets its file root. A module outside the repository is `external`;
  a missing relative or configured path is `not_found`; an existing module that is excluded or failed
  is `not_indexed` (or `external` with its path, for a TypeScript import of an unindexed file); a
  non-literal specifier is `dynamic`.
- **Type links.** Every written type name a front end records is resolved in the `type` context in
  parallel with the occurrences and reassembled by input index like edges; each call's arguments are
  [paired](#pairing) as its occurrence resolves.

Labels are short syntax, never arguments or bodies. Languages spell the parts, and
`unfold_engine::edges::text` joins them:

| Expression | Label |
| --- | --- |
| Identifier, private name, `this`, `super` | Its text |
| Property access | `receiver.member` |
| Call result | `callee()`, without the arguments |
| Element access with a literal key | `receiver["key"]`, keeping the written literal |
| Computed element access | `receiver[…]` |
| Construction or tagged template | The constructed expression's or tag's label |
| Transparent wrapper | The inner label |
| Anything else (including inline functions and literals) | `(…)` |
| Import | The decoded specifier without quotes, or `(…)` for a non-literal one |

Every label (edges and entrance targets alike) is at most 120 UTF-16 code units: the longest prefix
that fits, never splitting a surrogate pair. The helpers keep intermediate labels within the limit, so
long access chains stay cheap to label.

## Pairing

Pairing runs in `unfold_engine::pairing` when a call's or construction's occurrence edge resolves,
from the target's `parameter` records and the call's written arguments (their expansions and
keywords). It is language neutral; the one language question is how many of the target's leading
parameters the call binds itself (`Linked::bound_parameters`): TypeScript always answers 0 (an
explicit `this` parameter has its own kind); Python answers 1 for a method called through an
instance, `cls`, or `super()`, or an `__init__` reached by a construction, 0 for a static method, a
method called through its class, or a plain function, and nothing when it can't be proven. The rules,
applied to the arguments in written order:

1. The bound parameters and every `this` parameter are dropped; the rest are the *takers*, in order.
   Without a proven bound count, every argument is `receiver`.
2. Positional arguments fill the takers that accept positions (`positional`, `positional_only`;
   TypeScript `positional` also covers destructured and optional parameters) left to right. Past
   them, a `rest` taker takes every further positional argument; without one, a further argument is
   `no_parameter`.
3. Once an argument is spread, it and every later positional argument are `spread`. A Python `**`
   expansion is `spread` itself; keyword arguments after it still pair by name.
4. A Python keyword argument fills the taker of that name that accepts keywords (`positional`,
   `keyword_only`), unless a positional argument already filled it (then it is `no_parameter`, the
   call is an error); otherwise the `keywords` taker; otherwise it is `no_parameter`.
5. A `class` target (no direct constructor) takes nothing only when its language proves it
   (`Linked::class_takes_no_arguments`: no base, no decorator that could replace the class, and,
   in Python, no `__new__` in its body and no `metaclass=`); every argument is then
   `no_parameter`. Any other class target's arguments are `no_signature`.
6. Parameters left unfilled stay unfilled; defaults are never paired.

Equal ordinals never prove anything on their own. References (`seq` > 0) are not paired; a passed
callable's argument records the passed value's own bound count instead
(`Linked::reference_bound_parameters`, by the same rules without construction), when proven. The
count a call or a passed callable binds is recorded on its record (`bound`), so readers never
infer it from which parameters the arguments filled.

## Differences from the proof of concept

- Offsets are UTF-8 bytes instead of UTF-16 code units, and the graph is stored in SQLite instead
  of six CSV files. Block and entrance IDs (`b1`, `e1`) are replaced by file-scoped local IDs and
  structural keys; keys are derived exactly as before.
- Paths, and entrance evidence paths, sort by UTF-8 bytes instead of UTF-16 code units. The two
  orders differ only between astral characters and U+E000–U+FFFF.
- Files carry a state (`current`, `last_good`, `failed`), and inputs carry a role. There is no
  manifest: producers, detectors, and settings fingerprints live in the store's metadata.
- Manual entrances come from `.unfold/config.toml` (role `unfold-config`) instead of
  `codegraph.json`.
- An absent name, role, method, trigger, label, reason, module, position, target, or index is
  absent, not an empty string; empty strings are invalid where a value is present.
- TypeScript interfaces, type aliases, and enums are blocks, and calls in enum member initializers
  are nested in their enum; the proof of concept skipped the declarations (its keys are what
  `canonical::version_one` derives). Typed records are new: the references, version-1 exports,
  have none.
