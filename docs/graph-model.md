# Graph model

Read this for what the compiler records: blocks, links, typed records, keys, ranges, validation, and the
canonical export. Exact fields are owned by [model.rs](../crates/unfold-core/src/model.rs) and
[typed.rs](../crates/unfold-core/src/typed.rs); vocabularies and which values combine by
[vocab.rs](../crates/unfold-core/src/vocab.rs). How the reader draws these records is in [Reader](reader.md).

A diagram of code shows two relationships: what is **inside** what, and what **calls** what. The compiler records
the first as one tree of blocks per file, and the second as links. Everything else is code, which the code panel
shows.

## Blocks

| Block | What it is | Node kinds |
| --- | --- | --- |
| Definition | Something that can be a card | `module`, `class`, `function`, `type` |
| Control block | Code that chooses, repeats, or protects other code | `decision`, `loop`, `try`, `with` |
| Part | One lane of a control block | `part` |
| Call | A place where this code runs other code | `call` |
| Exit | A place where a path leaves | `exit` |

A node stores its written **form**, and the form decides its kind:

| Kind | Forms |
| --- | --- |
| `module`, `class`, `try` | the same as the kind |
| `function` | `function`, `method`, `constructor`, `getter`, `setter`, `lambda` |
| `type` | `interface`, `type_alias`, `enum` |
| `decision` | `if`, `match`, `switch`, `ternary` |
| `loop` | `for`, `while`, `do`, `comprehension` |
| `with` | `with`, `using` |
| `part` | `then`, `elif`, `else`, `case`, `default`, `body`, `except`, `catch`, `finally` |
| `call` | `call`, `new` |
| `exit` | `return`, `raise`, `throw`, `break`, `continue`, `yield` |

Python has no `switch`, `do`, `using`, `catch`, `throw`, or type declarations; TypeScript has no `match`,
comprehensions, `with`, `except`, or `raise`. Each language's mapping from syntax to forms is in
[TypeScript](languages/typescript.md#lowering) and [Python](languages/python.md#lowering).

### Definitions

A definition records its name, its signature (a [typed record](#typed-records)), the flags `async` and
`generator`, and its **doc**: the range of its docstring or leading JSDoc. The doc is the card's description from
code, never a step. A module's top-level code is its body, drawn like any function's, so
`if __name__ == "__main__": main()` is a decision with a call to `main`.

### Control blocks and parts

Every lane of a control block is a part with its own key, so "the `else` of this `try`" can be named, combined, or
given a proposal.

| Form | Parts, in written order | Header, test, clause |
| --- | --- | --- |
| `if` | `then`, `elif[n]`, `else` | each part's test (`else` has none) |
| `match` | `case[n]`, `default` (Python's guardless `case _:`) | the subject is the header; a case's pattern and guard are its test |
| `switch` | `case[n]`, `default` | the discriminant is the header; a case flagged `falls_through` runs into the next |
| `ternary` | `then`, `else` | the condition is `then`'s test |
| `for`, `while`, `do` | `body`, and Python's loop `else` | the loop header: `for item in order["items"]` |
| `comprehension` | `body` | its `for` clauses are the header; its `if` clauses are the body's test |
| `try` | `body`, `except[n]` or `catch`, Python's `else`, `finally` | each handler's clause: `except PaymentDeclined as error`; `except*` is flagged `group` |
| `with`, `using` | `body` | the header: `with open(path) as f`; flag `async` |

`elif` is a part of the same decision, not an `if` nested in an `else`. Validation checks that each control block
has its required parts (`then` for `if`, `then` and `else` for a ternary, `body` for loops, `try`, and `with`) in
written order.

### Calls

A call block is one written call: a function or method call, or a construction (`new X()`, or a Python call whose
class identity linking proves), whose form is then `new`. Its `name` is the written callee, normalized as
[keys](#identity-and-structural-keys) describe. It records:

| Field | Meaning |
| --- | --- |
| `binds` | The written target its value is assigned to, when it is exactly that value: `receipt`, `order.receipt`, `email_row_id, inserted`. `a = b = f()` binds `a = b`. Ternaries and comprehensions record it too |
| `exit_value` | Its value is exactly what its exit returns, raises, or yields |
| `awaited` | Written under `await` |
| `conditional` | It runs only sometimes inside its expression: the right side of `and`, `or`, or `??`, or after `?.` |
| `role`, `for` | `test`: written in a part's test. `decorator`, `base`, `default`: it runs when a definition is created, in the enclosing body, and `for` names that definition. A TypeScript default runs each time its function is called, so it is a call at the start of the function's own body, with role `default` and no `for` |
| `stmt` | See [Statements](#statements) |

Every decorator is a call, plain names included: `@property` is a call to `property`. Every call is a block,
including builtins, calls no one could resolve, and calls inside `assert` and `del`. So every resolved call can be
opened where it is written, and an unresolved call is never dropped. Which calls are drawn is the reader's choice.

A call **contains** what is written in its receiver chain and arguments: calls, ternaries, comprehensions, and
inline definitions. `save_all(map(convert, rows))` is a `save_all` block containing a `map` block. A function or
TypeScript class expression written in a call's arguments is a definition inside the call, `callback[n]` in written
order, object properties included; one written as a property (`useMutation({ onSuccess: () => … })`) also records
the property as its `name`. A function called where it is written, `(() => …)()`, is `lambda[0]` inside its call.

### Statements

Statements are not blocks, but the code panel shows a call's whole statement, Combine spans whole statements, a
proposal goes after a block's statement, and gap ticks count the lines no drawn statement covers. So the **first
block of each statement** records the statement's range as `stmt`, and every exit records it too. One rule holds in
both languages:

- A definition is the first block of its statement, which runs from its first decorator to its end.
- A decorator call has no `stmt`: its definition's statement covers it.
- `receipt = charge_card(order["card"], total)` gives `call:charge_card` the whole line as `stmt`.

### Exits

| Form | Leaves to |
| --- | --- |
| `return` | End, through every enclosing `finally` |
| `raise`, `throw` | The caller, through every enclosing `finally`; an enclosing `try`'s handlers may catch it first |
| `break`, `continue` | Its loop, recorded as `target`. TypeScript labels are resolved by the front end. A TypeScript `break` that leaves a `switch` before its case ends targets the `switch`; a `break` that ends a case is no node |
| `yield`, `yield from` | Continues when the consumer resumes it. A `yield` is an exit only as an expression statement or as an assignment's (or declarator's) whole value; any other `yield` is code |

An exit contains what its value contains (`return mark_failed(order)` holds the `mark_failed` call, flagged
`exit_value`) and records its value's range as `value`, so `return order` is labeled by `order` though `order` is
not a block.

### Expressions that decide or repeat

A conditional expression is a `decision` with form `ternary`, and a comprehension or generator expression is a
`loop` with form `comprehension`, **when it contains a call**. It can sit anywhere an expression can: a statement's
value, a call's arguments, an exit's value, a test, or a header. In `[convert(r) for r in fetch() if keep(r)]`,
`fetch` is a header call, `keep` a test call in the body, and `convert` is in the body.

Short circuits (`and`, `or`, `??`) and optional chaining (`?.`) are not decisions; they mark their calls
`conditional`.

### What is not a block

| Written | Becomes |
| --- | --- |
| Assignments and expression statements | Code; their calls are blocks |
| A statement that calls nothing (`total += price`, `pass`, `global`) | Code |
| Imports | `imports` links from the module, class, or function they are written in |
| A docstring or leading JSDoc | The definition's `doc` |
| `assert`, `del` | Code; calls inside them are blocks |
| A ternary or comprehension with no call | Code |
| JSX elements | Code; calls inside their expressions are blocks |
| Type syntax and annotations | Typed records, and `types` and `extends` links |

### Containment and ownership

```text
module, class, function   →  calls, exits, control blocks, definitions
control block             →  parts, and the expression blocks in its header
part                      →  calls, exits, control blocks, definitions, and the expression blocks in its test
call                      →  the expression blocks in its receiver and arguments
exit                      →  the expression blocks in its value
type                      →  nothing

expression blocks         =  calls, ternaries, comprehensions, and the functions and classes written in an expression
```

A definition inside a part (a `def` under `except ImportError:`) sits in that part and is still keyed by its name.
So does one written in an expression: an inline definition, or a TypeScript object literal's method in a
`return` (`return { run() {…} }` holds `#make.run`).

**A block belongs to the nearest definition above it.** That is the only ownership rule, implemented once as
`unfold_core::owner` and used by the engine, validation, and the reader. Decorators, bases, and Python defaults sit
in the enclosing body, so they belong to the enclosing definition; calls inside a callback belong to the callback.

### Uncertainty

A fact is known, or it is `unknown` with a reason. Besides a link's [outcome](#links), there are two other unknowns,
each drawn dashed and labeled, never omitted and never drawn as an end:

- **A truncated block.** What a front end can't place is marked `truncated` on the nearest node above it: nesting
  past the depth limit, or a `break` out of a TypeScript labeled plain block.
- **An entrance target that names nothing,** with its [state](engine.md#resolving-code).

Routes are not stored. The reader derives them from the tree, so code after an early `return` is a route state
("never runs"), never a compile error.

## Links

| Kind | From | To | Example |
| --- | --- | --- | --- |
| `calls` | a call | a definition, a library or builtin label, or unknown with a reason | `charge_card(…)` → `shop/payments.py#charge_card` |
| `passes` | a call | a definition passed as a value | `map(convert, rows)` → `#convert` |
| `extends` | a class or type | a class or type; flagged `implements` for a TypeScript interface | `class PaymentDeclined(Exception)` → `python:builtins` |
| `imports` | a module, class, function, or dynamic-import call | a module | `from shop.store import …` → `shop/store.py#` |
| `types` | a definition or an `except` part | a class or type named in an annotation or a handler clause | `except PaymentDeclined` → `#PaymentDeclined` |

Every call has exactly one `calls` link, numbered first; a node's other links follow in written order. Each link
keeps the range and text of the name it resolves, so a parameter's or field's type is found by its range. A
`calls` link to a project function may record `bound`: how many leading parameters the call binds itself. A
TypeScript link's text follows the callee normalization, so a call reads the same on its node and its link
(`"".trim`, `a[]`).

An annotation's `types` links come from the definition it types: a parameter's and return type's from its
function, a heritage clause's from its class, and a TypeScript declaration's or class field's from the function or
class it names, when that definition begins the statement (`export const handle: Handler = (r) => …` links
`Handler` from `#handle`); any other from the definition it is written in. A link's range lies inside its node, or
inside the statement a definition begins.

| Outcome | Target |
| --- | --- |
| `project` | The key of exactly one proven definition (for `imports`, a module) in this project |
| `library` | A proven outside module: `fastapi`, `node:fs`, `python:logging`, `global` |
| `builtin` | The language's own builtins: `python:builtins`, `ecmascript` (ECMAScript's standard objects) |
| `unknown` | One of 14 reasons: `unknown_name`, `unknown_receiver`, `parameter`, `mutable_binding`, `not_concrete`, `accessor`, `ambiguous`, `overridden`, `unbound_this`, `not_indexed`, `not_found`, `dynamic`, `wrapper`, `unsupported` |

Host APIs (`console`, `fetch`, timers, `process`) are `library global`, not builtins. A link reaches a project
definition only with proof of exactly one permitted target ([value policy](#value-policy-and-reason-order)).

## What a call's value is

Most unknown calls are method calls on a value nobody typed. A call's value is known only from written facts, in
exactly these forms:

- **Written return types on project definitions.** When a call resolves to one concrete project function that isn't
  overridden or wrapped, its value is an instance of the class written as that function's return type, if the
  annotation is one of: a class (`-> Repo`); a generic class, read by its class with its type arguments dropped as
  written variable types are (`-> Repo[str]` and `Repo<string>` are `Repo`, `-> list[int]` is `list`, and
  TypeScript's `number[]` is `Array`); a class or nothing
  (`Repo | None`, `Optional[Repo]` from `typing`, TypeScript `Repo | null | undefined`), read as `Repo`; a quoted
  name (`-> "Repo"`); or any of these on an `async` function, read through `await` (TypeScript: `Promise<Repo>`,
  with the library `Promise`). Unions of classes, type variables, `Self`, `Union[Repo, None]`, and an `async` call
  without `await` leave the value unknown. A Python annotation is read as its `types` link is, so an alias
  (`Alias = Repo`) gives nothing, and a class that derives from `Generic[T]` stays unresolved.
- **Stubs for eight Python standard modules:** `builtins`, `logging`, `os` (with `os.path`), `json`, `re`,
  `datetime`, `pathlib`, `asyncio`, generated from pinned typeshed stubs and read without generics. So
  `logging.getLogger(…)` gives a `logging.Logger`, and `logger.info` is a library call. Builtin methods resolve on
  written types (`row: dict[str, Any]` → `row.get`) and on literals. A written type may hold a project subclass: when
  one defines the member, the call is `overridden`. A project write that replaces a member of a stub class, a
  builtin, or a member of an instance the stubs type (`logging.Logger.info = …`, `builtins.print = …`,
  `logger.info = …`) makes the calls that may reach it `dynamic`.
- **ECMAScript globals by name:** `JSON.parse`, `Object.keys`, `String(…)`, `new Error(…)`, `Intl.Collator(…)`. A
  built-in method resolves only on a literal or a written class type no project class may override (`[1, 2].map`,
  or `rows.map` where `rows: Row[]`), or as a member a project class inherits from an ECMAScript base (`super.map(…)`
  in `class Rows extends Array<Row>`) that no subclass declares. A member's own value isn't read, so a call through
  it is unknown: `s[0].toLowerCase()`, `e.message.trim()`, `process.argv.slice(2)`. A project write to a global,
  its prototype, or an instance of one of its types (`Array.prototype.map = …`, `rows.map = …`) makes the calls that
  may reach it `dynamic`, and so does replacing a global (`globalThis.Map = …`) for every member of what `new` of it
  constructs.
- **No third-party stubs.** A call into FastAPI or Express is `library` when an import proves the module, and
  `unknown` when it is a method on a value whose type isn't written.

Nothing is inferred from function bodies. The language guides list the exact rules:
[TypeScript](languages/typescript.md#what-a-calls-value-is), [Python](languages/python.md#what-a-calls-value-is).

## Records

| Record | One per |
| --- | --- |
| `file` | Indexed source file: path, language, state, hash, failure |
| `input` | Fingerprinted configuration or evidence file: path, role, hash. Never also a source file |
| `node` | Block, in preorder |
| `link` | Relationship from a block |
| `signature`, `parameter`, `member`, `argument` | [Typed record](#typed-records) |
| `app`, `entrance`, `entrance_target` | `[[app]]` table, entrance, and `code` key of `unfold.toml`, with its state ([Engine](engine.md#entrances)) |

A node's fields:

| Field | Meaning |
| --- | --- |
| `key`, `parent` | Identity and containment |
| `form` | The written form, which decides `kind` |
| `name` | A definition's name, a call's normalized callee, or a callback's property name |
| `range` | The whole block; the module spans the whole file |
| `head` | A definition's signature, a control block's header, a part's test or clause, a call's callee |
| `body` | The suite of a definition or a part |
| `doc` | A definition's docstring or JSDoc |
| `stmt`, `value` | See [Statements](#statements) and [Exits](#exits) |
| `binds`, `role`, `for`, flags | See [Calls](#calls). Flags: `async`, `generator`, `awaited`, `conditional`, `exit_value`, `falls_through`, `group`, `truncated` |
| `target` | For `break` and `continue`, their loop (or a TypeScript `switch`) |

A file is `current`, `last_good`, or `failed`. Current and last-good files carry nodes, links, and typed records;
last-good facts refer to their own hash, not the bytes on disk. A failed file carries none.

### Typed records

Only written types are recorded: there is no inferred type and no exception analysis. Ranges are token-exact bytes;
written text is cut to 120 UTF-16 units.

| Record | Rule |
| --- | --- |
| Signature | One per function; its written return type, if any |
| Parameter | One per written parameter, in order: kind, name, `optional`, `default`, written annotation |
| Member | Written fields and signatures of a type or class, in source order, each with its range |
| Argument | One per argument of a call: range, expansion, keyword, pairing, and `bound` for a passed function |

Parameter kinds are `positional`, `this` (TypeScript), `rest`, `positional_only`, `keyword_only`, and `keywords`
(Python). An argument is paired exactly when its call's `calls` link reaches a project definition
([Pairing](#pairing)). Where a call's value lands is the call's `binds` and `exit_value`.

`types` and `extends` links resolve only to same-language classes, interfaces, type aliases, and enums; generic
arguments get their own links. Dynamic bases are not static heritage. The language guides list what each language
excludes.

## Text, ranges, and lines

- Snapshots keep bytes, BOM, line endings, and normalization. Text decodes strictly as UTF-8; files over `u32::MAX`
  bytes fail. Python rejects a declared non-UTF-8 encoding.
- Offsets are zero-based UTF-8 bytes; ranges are half-open `[start, end)` on character boundaries. Ranges exclude
  leading comments, indentation, and trailing whitespace, and keep formatting inside them.
- Lines are one-based. CR, LF, and CRLF end lines, and CRLF counts once. In TypeScript U+2028 and U+2029 also end
  lines, as in ECMAScript; in Python, as in CPython, they don't. Files that aren't sources, such as `unfold.toml`, end
  lines at CR, LF, and CRLF. An empty file spans `[0, 0)` on line 1.
- Rendered offsets are UTF-16, converted through core's text utilities. An offset inside a UTF-8 sequence or between
  surrogate halves is an error, never rounded.
- Labels are cut without splitting surrogate pairs. Python identity uses NFKC; text keeps the written spelling.

Implementation: [text/](../crates/unfold-core/src/text/mod.rs).

## Paths

Project paths are nonempty, relative, case-preserving, `/`-separated UTF-8, ordered by bytes. Empty, `.` or `..`
components, backslashes, NUL, drive prefixes, and absolute paths are rejected. Syntactic validity doesn't prove
confinement; runtime access also checks symlinks. See [RepoPath](../crates/unfold-core/src/path.rs) and
[workspace confinement](../crates/unfold-workspace/src/confine.rs).

## Identity and structural keys

Within a file, a node is its local ID: its position in preorder (the module is 0). Links and typed records refer to
nodes of their own file that way. Across files, in boards, settings, agent text, and the export, a node is its
**key**: readable, structural, and derived from the tree, never chosen.

```text
key    = path "#"                               a module
       | path "#" names [ "/" steps ]           a named definition, and the blocks inside it
       | path "#/" steps                        the blocks of a module's own code
       | "entrance:" app [ "/" entrance ]       an app's start, or one of its entrances
names  = name [ "[" n "]" ] { "." name [ "[" n "]" ] }
steps  = step { "/" step }
step   = control "[" n "]" | part | exit "[" n "]"
       | "call:" callee [ "(" literal ")" ] [ "[" n "]" ]
       | inline "[" n "]" [ "." names ]
```

```text
shop/service.py#place_order                                             the function
shop/service.py#place_order/try[0]/except[0]                            a part
shop/service.py#place_order/try[0]/except[0]/return[0]/call:mark_failed the call it returns
shop/cli.py#/if[0]/then/call:main                                       a module's own code
src/routes/orders.ts#/call:ordersRouter.post("/orders/:id")/callback[0] an inline handler
```

- **Named definitions** are keyed by qualified name wherever they sit, so a guard added around a `def` doesn't move
  its key: `src/repo.ts#Repo.save`, `#place_order.helper`. A repeat of the same key gets `[n]`. A function bound by a
  declaration or a simple assignment takes that name (`const handler = …` is `#handler`); an object-literal method
  or class property takes its property name, after the names of the declaration that names its object
  (`#handlers.onCreate`); an anonymous default export is `#default`.
- **Inline definitions** (functions, and TypeScript class expressions with the form `class`) are `callback[n]` in a
  call's arguments and `lambda[n]` anywhere else, numbered in written order. Named definitions inside them follow
  their key and `.`: `#/call:define("x")/callback[0].connectedCallback`.
- **Control blocks and exits** carry their form and an ordinal among siblings of that form: `if[0]`, `return[1]`.
  Parts are `then`, `else`, `default`, `body`, `catch`, `finally`, or `elif[n]`, `case[n]`, `except[n]`.
- **Calls** are keyed by their normalized callee, with `[n]` only for a repeat under the same parent
  (`call:logger.info[1]`). Arguments become `()`, subscripts `[]` (escaped), a string literal `""`, any other
  expression `(…)`; `await`, parentheses, `!`, and type casts are dropped; `?.` is written `.`; a callee is cut at 80
  characters. So `res.status(500).json(…)` is `call:res.status().json`.
- **A call that receives an inline definition** is also keyed by its first argument when that is a string literal:
  `call:ordersRouter.post("/orders/:id")`. Adding a route above it doesn't move its key, and editing its path makes
  the key missing instead of wrong. The literal is written double-quoted with only `"` and `\` escaped.
- **Steps always follow `/` and names follow `#` or `.`,** so a module's own steps start with `#/`, and a function
  named `match` is never read as a `match` statement.
- Names and callees percent-encode `%`, `/`, `#`, `[`, and `]` in uppercase, and names also `.`, which separates
  them: `{ "order.created": … }` in `const handlers` is `#handlers.order%2Ecreated`.

App and entrance keys come from names ([Engine › Keys](engine.md#app-and-entrance-keys)).

**Only `unfold-core`'s key module reads keys** ([key.rs](../crates/unfold-core/src/key.rs)): `KeyBuilder` makes
them, `Key::parse` reads them, and `Key::rebase` moves the keys inside a renamed block. Every other crate, the UI, and
the MCP server treat keys as opaque text and get kinds, parents, and files from records. Validation re-derives every
key and compares.

**Short labels.** Agent text pairs each full key with the label agents read and write: the key's steps after its
card's definition (`try[0]/except[0]` in `a.py#f`), from `Key::steps_after`. The MCP server maps a label back to a
key by looking it up in the response, never by joining strings.

Ordinals can shift when code is inserted above a block; [board guards](reader.md) catch that, so an explanation
never silently attaches to different code. SQL interns keys for the database's lifetime.

## Order and canonical export

Files and inputs sort by path, nodes are preorder, links sort by `from` then `seq`, typed records by node then index,
and apps and entrances keep their order in `unfold.toml`. Output is independent of thread scheduling.

The canonical export is deterministic JSON Lines, one record per line, named by `t`: `header`, then each `file`
followed by its `node`, `link`, `signature`, `parameter`, `member`, and `argument` records, then each `input`, then
each `app` followed by its `entrance` and `entrance_target` records. Records refer to nodes by key and leave out
absent fields and false flags. The export has one version; reading and writing it again gives identical bytes. An
export in another format fails like any invalid file. The reader and writer own exact fields:
[canonical/](../crates/unfold-core/src/canonical/mod.rs).

```json
{"t":"node","key":"src/orders_playground/cli.py#report_command/return[0]","parent":"src/orders_playground/cli.py#report_command","kind":"exit","form":"return","range":[240,268],"lines":[10,10],"stmt":[240,268],"value":[247,268]}
```

The export is the golden format (`testdata/goldens/`). Write one with `cargo xtask export <folder>`; never hand-edit
an export to make a test pass.

## Validation invariants

Every compile validates before committing; a failure keeps the previous graph. Validation checks the graph's own
consistency, never routes, reachability, or runtime dispatch, so legal code never fails a compile. The checks live
in [validate/](../crates/unfold-core/src/validate/mod.rs):

- Files and inputs are uniquely ordered, exclusive by path, and consistent with their hashes, states, and
  failures; current files are checked against their bytes (lines, character boundaries, trivia).
- Nodes are in preorder with the module at 0, follow the [containment grammar](#containment-and-ownership), and have
  each control block's parts in written order. Forms fit the language, flags fit the form, and each kind carries
  only its fields (an exit always has `stmt`; only `break` and `continue` have `target`, naming an enclosing loop,
  or for `break` a `switch`, in the same definition).
- Ranges lie inside their parents; `head`, `body`, and `value` inside the node; `stmt` around it.
- Keys are unique and equal to the keys `KeyBuilder` derives from the tree.
- Every call has exactly one `calls` link, first. Link sources, targets, outcomes, and module labels fit the link
  kind; a project target exists and has a permitted kind.
- Typed records have the right cardinality (exactly one signature per function), contiguous indexes, and ranges
  inside their nodes; pairings agree with resolution.
- App and entrance keys are re-derived from their names and unique. An entrance target's state is data, not an
  error: a `missing` target is valid.
- **The census:** one check, each language's own walk of the parse tree: every call in it is exactly one call node.
  Recall is never improved by dropping hard occurrences.

## Value policy and reason order

Languages supply evidence (bindings, receivers, declarations, imports); the shared backend decides outcomes in
[resolve/policy.rs](../crates/unfold-engine/src/resolve/policy.rs). The first test that matches decides, so the order
is part of the contract and candidate order never changes the answer:

1. A language refusal (`wrapper`, `dynamic`, `unsupported`, `not_concrete`), then ambiguous evidence, then unbound
   `this`, then a failed import binding (`not_found`, `not_indexed`, `ambiguous`).
2. An unindexed repository candidate is `not_indexed`.
3. A proven package-import root is external, unless an indexed candidate contradicts it. Here and in 4 and 5, an
   external outcome is `overridden` when the evidence says the receiver may dispatch to a project override.
4. No candidate that can supply a body: external when every augmenting candidate is external, otherwise
   `unknown_name`, `unknown_receiver`, or `dynamic` by the value's shape.
5. Only external candidates are external; mixed origins are `ambiguous`; an indexed candidate the language can't
   name is `unknown_name`.
6. Several distinct concrete targets are `ambiguous`.
7. No target: `parameter`, then `mutable_binding`, then `accessor`, then `not_concrete`.
8. Non-concrete declarations beside the target, other than its compatible overloads, are `ambiguous`.
9. A target an override may replace is `overridden`.
10. A passed value must reach a function or class, a type name a class or type; otherwise `unsupported`. Else the
    one permitted target.

The override census covers descendants through every declared base, including unknown computed member names;
static, private, and `super` accesses have their own exemptions. Change precedence in the shared policy, never in a
front end.

### Module labels

A label names an outcome the policy already found external; it never establishes a target. The first rule that
applies wins ([labels.rs](../crates/unfold-engine/src/labels.rs)):

1. The package an external import binding of the access root names.
2. The package the receiver's bounded import provenance comes from, so `app.get` on `const app = express()` names
   `express` ([provenance.rs](../crates/unfold-engine/src/provenance.rs)). Provenance never decides what is
   callable.
3. An undeclared, unimported root such as `console` (TypeScript `global`), unless every candidate is a builtin.
4. The labels of the candidates' locations: one label, or a sorted JSON array of several.

Each language spells its labels: [TypeScript](languages/typescript.md#module-labels),
[Python](languages/python.md#module-labels).

## Pairing

Arguments pair only for a call resolved to a project definition, in
[pairing.rs](../crates/unfold-engine/src/pairing.rs). The one language question is how many leading parameters the
call binds itself.

1. Proven bound parameters and every `this` parameter are dropped; the rest are the takers. An unproven binding
   leaves every argument unpaired with `receiver`.
2. Positional arguments fill positional takers left to right, then a `rest` taker; any further one is
   `no_parameter`.
3. Once an argument is spread, it and every later positional argument are `spread`. Python `**` is itself `spread`;
   later keywords still pair by name.
4. A keyword fills the named taker that accepts keywords unless a positional argument already did, else `**kwargs`,
   else `no_parameter`.
5. A class with no direct constructor takes nothing only when its language proves it (no base, no decorator);
   otherwise its arguments are `no_signature`.

Equal ordinals never prove a binding, and defaults don't fill missing parameters. `passes` links aren't paired; a
passed function's own bound count is recorded on its argument.
