# Typed lines

Status: compiler built (records, store, reader, commands); UI not yet · 2026-10-10

On a board, the line between two cards should say what flows through the call it draws: what goes
in, what comes back, and what can be thrown. Clicking a type opens its definition in place. This
document describes the design (Paper file "Lazar's Playground", page "Upfold App 2"), what the
compiler records today, and the facts, records, commands, and UI pieces that make the design work.

## Contents

1. [The design](#1-the-design)
2. [Today](#2-today)
3. [Proposed compiler facts](#3-proposed-compiler-facts)
4. [Storage, export, and validation](#4-storage-export-and-validation)
5. [Reader](#5-reader)
6. [Commands](#6-commands)
7. [UI](#7-ui)
8. [Tests and fixtures](#8-tests-and-fixtures)
9. [Decisions made here](#9-decisions-made-here)
10. [Not in the initial scope](#10-not-in-the-initial-scope)

## 1. The design

Three artboards. Exact values come from the Paper export; the UI maps them onto the theme tokens in
`ui/src/theme/tokens.css` (right column), so dark mode follows.

**"Typed lines · board".** Three Code cards: `createOrder` calls `reserveInventory` and
`chargePayment`. The arrows are unchanged (1.5px `--color-arrow` bezier with a filled head). The
`reserveInventory` line carries a closed label at its midpoint: `(CartItem[]) → Promise<Hold>`. The
`chargePayment` line is selected, and its label has opened into the contract.

**"Typed lines · type opened".** The same contract with `Cart` expanded under its row: `interface
Cart`, `cart.ts:4`, its fields with their own types, and "Peek source", "Used in 6 places".

**"Typed lines · states and sources" ("Types on a line").** A table of every part of the line and the
compiler fact it needs; section 2 answers it.

| Element | Paper value | Token |
| --- | --- | --- |
| Closed label | JetBrains Mono 11/16, padding 2px 6px, background `#F7F8FA`, no border | `--color-canvas` |
| Label type names | `#202126` | `--color-text` |
| Label punctuation `(`, `) → `, `[]`, `<>` | `#8B93A1` | `--color-contour` |
| Contract box | `#FFFFFF`, 1px `#E0E2E7`, radius 8, shadow `0 4px 14px #20212612`, padding 6px 0, 316px wide | `--color-surface`, `--color-border`, the floating shadow token |
| Contract sections | Sends, then a 1px `#EEF0F3` rule, Returns and Throws; section padding 4–6px 12px | a new `--color-rule-soft` (light `#EEF0F3`) |
| Contract rows | 22px high, gap 12px: a 52px label column (Inter 11/14 `#656974`), a 76px caller column (Mono 12/16 `#202126`), then the callee side | `--color-muted`, `--color-text` |
| Callee side | `cart: ` in `#8B93A1`, then the type | `--color-contour` |
| A clickable type | Mono 12, `#202126`, underline 1px `#C9CDD6`, offset 2px | a new `--color-type-underline` |
| The opened type | `#5862C0` with an accent underline, padding 0 2px, radius 3 | `--color-accent` |
| "catch" in the caller column of Throws | `#8B93A1` | `--color-contour` |
| Type definition inset | indented 64px; `#F7F8FA`, 1px `#EEF0F3`, radius 6, padding 8px 10px 6px; 20px rows | `--color-canvas`, `--color-rule-soft` |
| Definition header | `interface ` in `#8B93A1`, the name Mono 12 weight 600, `cart.ts:4` Inter 11 `#656974` at the right | |
| Definition fields | name column ≥84px in `#8B93A1` (indented two spaces), the type as above; `coupon?` keeps its `?` | |
| Definition footer | "Peek source" and "Used in 6 places", Inter 11 `#5862C0`, gap 14, 4px above | `--color-accent-text` |
| Callee card marks | parameter lines, the return-type line, and the throw line tinted `#F4F5F8` | `--color-step` |
| Caller line | the calling line tinted `#EDF0FC` | `--color-selection` |

Two new tokens (`--color-type-underline`, `--color-rule-soft`) get light and dark values in all three
blocks of `tokens.css`; the dark values are designed, like the rest of the dark set
(`--color-type-underline` dark ≈ `#3d414b`, `--color-rule-soft` dark ≈ `#2a2c32`). Punctuation in
`--color-contour` is a mark repeated by the text beside it; if `contrast.test.ts` flags it, it joins
that test's listed exceptions with that reason.

## 2. Today

Verified in the code on 2026-10-10.

| Part of the line | What the compiler records | Enough? |
| --- | --- | --- |
| Arguments as written | `argument[i]` slots on `call`/`new` blocks: one byte range each. Spreads, keywords, `*args`, and `**kwargs` keep a slot but nothing says which they are | Partly |
| Parameter names and types | `parameter[i]` slots on function-like blocks: one range over the name, annotation, and default together | Partly |
| Return type | The `return_type` slot | Range only |
| Which argument fills which parameter | Nothing. `Edge.position` exists only for references (passed callables) and unresolved `parameter` calls; Python's docs say equal ordinals never prove a binding | No |
| Where the result lands | The block tree has the enclosing `variable`, `return`, or `expression_statement`, but nothing ties a call to it, and `await` and parentheses aren't recorded | No |
| What it can throw | `throw` blocks and the `constructs` edge of `throw new X()` / `raise X(...)` | Yes, gathered per function in the reader |
| What a type is defined as | `class` blocks in both languages. The TypeScript builder pushes interfaces and type aliases as skipped type syntax (`build/decl.rs`); enums are traversed only for calls in member initializers. No class records its fields | Partly |
| From a type name to its definition | The TypeScript binder reads written annotations to type receivers (`Linked::annotation_evidence`), and Python's binder does the same for receivers, but no type reference becomes a fact | No |
| Types the code never wrote | No type checker (U-7, U-8) | No, and not proposed |

Lines: `ui/src/canvas/arrows.ts` builds each arrow from the occurrence key, target card, edge kind,
and line; `ArrowEdge.tsx` draws the bezier and head; `canvas.css` sets `pointer-events: none` on
arrow edges. Drawn links (`LinkEdge.tsx`) already put labels on the curve's midpoint through React
Flow's `EdgeLabelRenderer`, with `pointer-events: all` on the label.

Versions today: store `SCHEMA_VERSION` 1, `ENGINE_VERSION` `…+facts.2`, both language payloads at
`PAYLOAD_VERSION` 4, canonical export header `version` 1.

## 3. Proposed compiler facts

The precision policy is unchanged and extends to every new fact: **written types only, nothing
inferred.** A type is the annotation text as written; a pairing exists only where the language's
calling rules prove it; a type link resolves only with proof of exactly one declaration, through the
same value policy and reason order as calls.

### 3.1 Type declarations become blocks (TypeScript)

| Syntax | Block | Slots |
| --- | --- | --- |
| `interface X<…> extends … { … }` | `interface` | `name`, `heritage` (from `extends` through its last type), `body` (the braces) |
| `type X<…> = T` | `type_alias` | `name`, `type` (the aliased type) |
| `enum X { … }`, `const enum X { … }` | `enum` | `name`, `body` (the braces) |

- New block kinds `interface`, `type_alias`, `enum` (TypeScript only) and one new slot name `type`
  ("a type alias's aliased type"). They aren't function-like, occurrence, or statement-outline kinds.
- Ambient declarations (`declare interface`, anything inside `declare module`, `declare global`, or
  `declare namespace`) stay skipped, as ambient classes are. Declarations inside an ordinary
  `namespace` become blocks under the namespace's saved ancestor, as classes do.
- Keys follow the existing rules: `src/cart.ts#interface:Cart`; merged interfaces are
  `interface:Cart` and `interface:Cart[1]`.
- Role: `body`, like any statement. Calls in enum member initializers now have the `enum` block as
  their parent (role `body`, the first slot their path leaves through), so their keys gain the
  `enum:X/` step.
- The reader's owner walk passes through these kinds (they are neither callable nor class), the
  Flow and outline ignore them, and they stay out of the definition catalog and search (not yet).

Python classes are already blocks; no Python kind is added.

### 3.2 Records

Six new records. Each belongs to one file, refers to blocks of that file by key in the export (by
local ID in memory), and lists in block order. Text fields are written source, cut like edge labels
to 120 UTF-16 units; ranges are UTF-8 byte ranges, token-exact.

**`signature`**: exactly one per function-like block.

| Field | Meaning |
| --- | --- |
| `block` | The function-like block |
| `returns`? | The written return type (TypeScript: the annotation after `:`, without the colon; Python: the expression after `->`) |
| `returns_range`? | Its range; present exactly with `returns`, inside the `return_type` slot |

**`parameter`**: exactly one per `parameter[i]` slot, same index.

| Field | Meaning |
| --- | --- |
| `block`, `index` | The function-like block and the slot index |
| `kind` | From *parameter kinds* below |
| `name`? | The identifier; absent for a destructuring pattern |
| `optional` | TypeScript `x?` (written only when true) |
| `default` | A default value is written (only when true) |
| `type`?, `type_range`? | The written annotation and its range, inside the parameter slot |

Parameter kinds: `positional` (an ordinary parameter), `this` (TypeScript's explicit `this`
parameter, which takes no argument), `rest` (TypeScript `...x`, Python `*args`), `positional_only`
(Python, before `/`), `keyword_only` (Python, after `*` or `*args`), `keywords` (Python `**kwargs`).
`this` is TypeScript only; `positional_only`, `keyword_only`, and `keywords` are Python only.

**`member`**: the written shape of a type declaration or class, in source order.

| Field | Meaning |
| --- | --- |
| `block`, `index` | An `interface`, `type_alias`, `enum`, or `class` block, and a contiguous index from 0 |
| `kind` | `property`, `method`, `index`, `call`, `construct`, or `value` (an enum member) |
| `name`? | The member name as written (`coupon`, `"x-id"`, `[Symbol.iterator]`); absent for `call`/`construct`; an index signature's parameter text (`[key: string]`) for `index` |
| `optional`, `readonly`, `default` | TypeScript `?` and `readonly`; a written initializer (class field) or default (Python); each written only when true |
| `type`?, `type_range`? | The written type: a property's annotation, a method or call signature's text after its name (`(a: A): B`), an index signature's value type |
| `value`? | An enum member's written initializer |

Which members are recorded:

- **TypeScript** interfaces and object type literals that are the whole aliased type of a
  `type_alias` (`type Cart = { … }`): every property, method, index, call, and construct signature.
  Enums: every member (kind `value`). Classes: instance property declarations (with or without a
  type, `accessor` fields included) and constructor parameter properties, not static members and
  not methods (methods are blocks already).
- **Python** classes: every annotated assignment whose target is a plain name written directly in
  the class body (`id: CartId`, `total: Money = Money(0)`), kind `property`, with `default` when a
  value is written. This covers dataclasses, `TypedDict`, pydantic models, `NamedTuple`, attrs, and
  plain annotated classes alike: the record states what is written, not what a decorator or base
  makes of it. Unannotated assignments, methods, and nested classes are not members.

**`argument`**: exactly one per `argument[i]` slot of a `call` or `new` block, same index.

| Field | Meaning |
| --- | --- |
| `call`, `index` | The occurrence block and the slot index |
| `expand`? | `spread` (TypeScript `...x`, Python `*x`) or `keywords` (Python `**x`; Python only) |
| `keyword`? | A Python keyword argument's name (`currency` in `currency="usd"`) |
| `parameter`? | The index of the parameter this argument fills, in the resolved target's `parameter` records |
| `unpaired`? | Why it fills no proven parameter, from *pairing reasons* below |

`parameter` and `unpaired` are pairing ([3.4](#34-pairing-arguments-with-parameters)): exactly one
of them is present when the occurrence edge (`seq` 0) is `resolved`, and neither otherwise.

Pairing reasons: `spread` (this argument, or an earlier one, is expanded), `no_parameter` (no
parameter of the target takes it), `receiver` (it can't be proven whether the call binds the
target's first parameter itself), `no_signature` (the target declares no parameters of its own: a
class without a direct constructor that has a base).

**`result`**: exactly one per `call` and `new` block. Where the call's value lands, after
transparent wrappers (parentheses, `await`, TypeScript `!`, `as`, `satisfies`, `<T>`):

| `binding` | When | `target` | `position` |
| --- | --- | --- | --- |
| `variable` | It is exactly the initializer of a `variable` block (`const charge = await f()`, Python `x = f()`, `x: T = f()`) | The variable block | — |
| `return` | It is exactly a `return`'s expression (Python: also a lambda body) | — | — |
| `argument` | It is exactly an argument of another `call`/`new` (`json(toOrder(…))`) | The outer call | The outer argument's slot index |
| `discard` | It is exactly the expression of an `expression_statement` | — | — |
| `other` | Anything else: a receiver (`res.status(201).json`), an operand, a condition, an assignment expression, a `throw`, a `yield`, … | — | — |

`awaited` (written only when true) says an `await` was one of the unwrapped layers. A chained
assignment (`a = b = f()`) is `variable` with the innermost variable level.

**`type_link`**: an edge from a type name written in an annotation to its declaration.

| Field | Meaning |
| --- | --- |
| `block` | The innermost block whose range contains the name: the function-like block for parameter and return annotations, the variable, the type declaration or class for members, the `type_alias` for its aliased type |
| `range` | The written name, qualified names whole (`Stripe.PaymentIntent`), without type arguments |
| `text` | The name as written |
| `resolution`, `target`?, `reason`?, `module`? | As for edges: a `class`, `interface`, `type_alias`, or `enum` block of the same language when resolved; a module label when external; a reason when unresolved |

Every type reference in a recorded annotation gets one link, generic arguments included
(`Promise<Charge>` links `Promise` and `Charge`; `CartItem[]` links `CartItem`; `Record<string,
Cart>` links `Record` and `Cart`). The annotations are exactly the ones the records above carry:
parameter and return types, member types, a type alias's aliased type, and variable annotations
(TypeScript `const x: T`; Python `x: T` anywhere).

What is not a link:

- TypeScript keyword types (`string`, `number`, `boolean`, `void`, `any`, `unknown`, `never`,
  `null`, `undefined`, `object`, `bigint`, `symbol`), literal types, `this`, `typeof x` queries,
  and `import("m").T` types.
- Names that bind to a type parameter of an enclosing declaration (TypeScript `<T>`, `infer U`,
  mapped `K`; Python PEP 695 `[T]`). The binder sees these lexically, so extraction never records
  them.
- Python: `None`, literals, and the arguments of a subscript spelled `Literal[...]`, and every
  argument after the first of one spelled `Annotated[...]` (values, not types; the spelling only
  avoids recording links, it never makes one). A string annotation is linked only when its whole
  content is a dotted name (`"Cart"`, `"models.Cart"`), with the range inside the quotes; any other
  string annotation records nothing.

### 3.3 How each language extracts them

**TypeScript.** The builder already visits every function, parameter, call, variable, and class;
it records signatures, parameters, arguments, and results in the same walk, and stops skipping
interfaces, type aliases, and enums (each now pushes a block with its slots and members instead of
`Cat::Type`/`Cat::Other`). Annotations are walked by a small type-syntax visitor that records each
`TSTypeReference`'s name (and recurses into its type arguments, unions, intersections, arrays,
tuples, function types, object type literals, indexed accesses, conditional and mapped types) as a
type reference fact. The census gains nothing new: type links and new blocks are checked against
its token boundaries like every range.

**Python.** The mapping already selects `def`, `lambda`, and class bodies and walks annotations (role
`header`); it records parameter kinds from Ruff's `Parameters` (`posonlyargs`, `args`, `vararg`,
`kwonlyargs`, `kwarg`), keywords and expansions from `Arguments`, results from the statement and
expression context, members from annotated assignments in class bodies, and type references from
annotation expressions (`Name`, `Attribute` chains, subscripts and their slices, `|`, tuples in
slices, and dotted-name strings).

Both: the per-file part goes into `GraphFacts` (signatures, parameters, members, argument shapes,
results, and type-reference facts, each with its owner block), so the facts cache carries it. Each
type reference also carries what its language needs to resolve it (a value-table entry or a
payload index); `ENGINE_VERSION` and both `PAYLOAD_VERSION`s change.

### 3.4 Pairing arguments with parameters

Pairing runs in `unfold-engine` after an occurrence edge resolves, from the target's `parameter`
records (in `FactsSet`) and the call's `argument` facts. It is language neutral; the only language
question is how many of the target's leading parameters the call binds itself:

```rust
/// How many of `target`'s leading parameters `call` binds without an argument: 1 for a Python
/// method called through an instance, `cls`, or `super()`, or an `__init__` reached by a
/// construction; 0 for a static method or a plain function. `None` when it can't be proven.
fn bound_parameters(&self, call: CallRef, target: BlockRef) -> Option<u32>;
```

TypeScript answers `Some(0)` always (an explicit `this` parameter has kind `this` and is skipped by
the rules below). Python answers from the binder's receiver facts; `None` makes every argument
`unpaired: receiver`.

The rules, applied to the arguments in written order:

1. Drop the bound parameters, and every `this` parameter. The remaining parameters are the
   *takers*, in order.
2. **Positional** arguments fill the takers that accept positions (`positional`, `positional_only`;
   TypeScript `positional` also covers destructured and optional parameters) left to right. Past
   them, a `rest` parameter takes every further positional argument; without one, a further
   argument is `no_parameter`.
3. Once an argument is expanded (`expand: spread`), it and every later positional argument are
   `spread`. In Python, a `keywords` expansion makes itself `spread`; keyword arguments after it
   still pair by name.
4. **Python keyword** arguments fill the taker of that name that accepts keywords (`positional`,
   `keyword_only`), else the `keywords` parameter, else they are `no_parameter`. A keyword that
   names a parameter a positional argument already filled is `no_parameter` (the call is an error).
5. A resolved target that is a `class` (no direct constructor): when its language proves it takes
   nothing (no base, no decorator, no Python `__new__` or `metaclass=`), every argument is
   `no_parameter`; otherwise every argument is `no_signature` (decision 23).
6. Parameters left unfilled are simply unfilled; defaults are never "paired".

Equal ordinals never prove anything on their own: pairing uses the language's calling rules above
and nothing else. References (`seq` > 0) are not paired; their label shows the passed callable's own
signature.

### 3.5 Resolving type links

Type links resolve through the shared value policy (`unfold_engine::resolve`) in a new context:

| Context | Used for | Permitted target |
| --- | --- | --- |
| `type` | A type name in an annotation | A `class`, `interface`, `type_alias`, or `enum` declaration |

- A new `Linked::type_evidence(TypeRef) -> Evidence` supplies candidates the way `value_evidence`
  does, but looked up in the **type** meaning: TypeScript follows type-only imports and exports
  (the export tables already follow names "as types"), namespace members, and qualified names
  through imports; Python follows the binder's identities (an import, a module attribute, a class
  in scope). `Linked::concrete_target(declaration, ValueContext::Type)` answers the declaration's
  block when it is one of the permitted kinds.
- The reason order is the existing one. Examples: two merged `interface Cart` declarations are
  `ambiguous` (step 10); a name nothing declares is `unknown_name`; a Python name bound to a
  variable (`Money = Decimal`, `T = TypeVar("T")`) has no type declaration and gets step 11's
  `not_concrete`; an excluded file's type is `not_indexed`.
- **External types** follow the module-label rules. `Stripe.PaymentIntent` with `import Stripe from
  "stripe"` is external `stripe` (rule 1). `Promise`, `Array`, `Map`, `Record` are undeclared,
  unimported library types: external `global` (rule 3). Python `int`, `list`, `dict` are
  `python:builtins`; `typing.Optional` / `from typing import Optional` is `python:typing`.
- TypeScript library types: `is_library_type` covers global classes and interfaces; the global
  utility type aliases (`Partial`, `Record`, `Pick`, `Omit`, `ReturnType`, `Awaited`, …) need a
  table too. Regenerate with `cargo xtask gen-lib-tables` (adding the lib's global type aliases)
  when TypeScript 6.0.3's lib files are available in `node_modules`; otherwise add them as a
  separate checked-in table with its source noted. Python: the type context treats every name
  `builtins` defines in the pinned typeshed inventory as a builtin, not only the proof of concept's
  call list.
- The override census and `this`/`super` steps don't apply to types.

### 3.6 Direct throws (reader)

The reader gathers, for a function-like block, its **direct throws**: every `throw` block it owns
(not inside a nested callable) whose expression is exactly a `new` (TypeScript) or `call` (Python)
block with a `constructs` edge, and which is not inside the `body` of a `try` that has a `catch`
handler in the same function (it may be caught). A resolved edge names a class (or a constructor,
whose class is its parent); external and unresolved ones keep their text and module or reason. A
bare `throw err` / `raise` / `raise ValueError` records no construction and is not listed.

## 4. Storage, export, and validation

### 4.1 In memory

`FileGraph` gains one field, `types: FileTypes` (`#[derive(Default)]`), holding six vectors in
canonical order: `signatures`, `parameters`, `members`, `arguments`, `results`, `type_links`. Tests
that build `FileGraph` literals use `types: FileTypes::default()`. The validator, canonical writer and
reader, store mirror, and row diff read and write them like slots and edges.

### 4.2 Canonical export

Per file, after its edges: `signature`, `parameter`, `member`, `argument`, `result`, `type_link`
records, each ordered by block (preorder), then index (or range for type links). The header's
`version` becomes `2`; the reader still accepts `version: 1` exports, which have none of the new
records (the frozen proof-of-concept references are version 1).

| `t` | Fields |
| --- | --- |
| `signature` | `block`, `returns`?, `returns_range`? |
| `parameter` | `block`, `index`, `kind`, `name`?, `optional`?, `default`?, `type`?, `type_range`? |
| `member` | `block`, `index`, `kind`, `name`?, `optional`?, `readonly`?, `default`?, `type`?, `type_range`?, `value`? |
| `argument` | `call`, `index`, `expand`?, `keyword`?, `parameter`?, `unpaired`?, `bound`? |
| `result` | `call`, `binding`, `awaited`?, `target`?, `position`?, `bound`? |
| `type_link` | `block`, `range`, `text`, `resolution`, `target`?, `reason`?, `module`? |

Booleans are written only when true. Parity with the proof of concept (`cargo xtask parity`,
`tests/structure/parity.rs`, the Python parity test) compares only what the reference has: the new
records, and the new TypeScript block kinds with their slots, are left out of the comparison, and the
keys of calls under enums are compared after removing the `enum:X/` step.

### 4.3 Validation invariants

- **Signatures and parameters.** Exactly one `signature` per function-like block and one
  `parameter` per `parameter` slot, with the slot's index; `returns_range` lies in the `return_type`
  slot and `type_range` in its parameter slot; kinds are permitted for the language; Python's
  parameters are in the language's order (positional-only, positional, `*args`, keyword-only,
  `**kwargs`); at most one `rest` and one `keywords`; `name` is nonempty when present.
- **Members.** On permitted block kinds only (`interface`, `type_alias`, `enum`, `class`); indices
  contiguous from 0; ranges inside the block; `value` only on `value` members, which exist only under
  an `enum`.
- **Arguments.** Exactly one per `argument` slot, with its index; `expand: keywords` and `keyword`
  are Python only; pairing fields present exactly when the occurrence edge is resolved, exactly one
  of them, and `parameter` is an index of the target's parameters (a function-like target) whose kind
  can take an argument (never `this`); a `class` target has only `unpaired`.
- **Results.** Exactly one per `call`/`new` block; `target` present exactly for `variable` and
  `argument` and names a block of that kind (`variable`; `call`/`new`) that is an ancestor of the
  call; `position` present exactly for `argument`, below the outer call's argument count, and that
  argument slot contains the call.
- **Type links.** Ordered by block, then range; each range nonempty, inside its block, and not
  overlapping another link; outcome fields as for edges (`target` exactly when resolved, and a
  permitted kind in the same language; `reason` exactly when unresolved; a valid `module` exactly
  when external); `text` at most 120 UTF-16 units.
- **With snapshots and a census:** every new range lies on character and token boundaries.

### 4.4 Store

`SCHEMA_VERSION` becomes 2; the store is a cache and rebuilds. No migration is written or run.

```sql
CREATE TABLE signatures (file_id INTEGER NOT NULL, block_local_id INTEGER NOT NULL,
  returns TEXT, returns_start INTEGER, returns_end INTEGER,
  PRIMARY KEY (file_id, block_local_id)) WITHOUT ROWID;
CREATE TABLE parameters (file_id INTEGER NOT NULL, block_local_id INTEGER NOT NULL, idx INTEGER NOT NULL,
  kind TEXT NOT NULL, name TEXT, flags INTEGER NOT NULL,          -- optional | default
  type TEXT, type_start INTEGER, type_end INTEGER,
  PRIMARY KEY (file_id, block_local_id, idx)) WITHOUT ROWID;
CREATE TABLE members (file_id INTEGER NOT NULL, block_local_id INTEGER NOT NULL, idx INTEGER NOT NULL,
  kind TEXT NOT NULL, name TEXT, flags INTEGER NOT NULL,          -- optional | readonly | default
  type TEXT, type_start INTEGER, type_end INTEGER, value TEXT,
  PRIMARY KEY (file_id, block_local_id, idx)) WITHOUT ROWID;
CREATE TABLE arguments (file_id INTEGER NOT NULL, call_local_id INTEGER NOT NULL, idx INTEGER NOT NULL,
  expand TEXT, keyword TEXT, parameter INTEGER, unpaired TEXT, bound INTEGER,
  PRIMARY KEY (file_id, call_local_id, idx)) WITHOUT ROWID;
CREATE TABLE results (file_id INTEGER NOT NULL, call_local_id INTEGER NOT NULL,
  binding TEXT NOT NULL, awaited INTEGER NOT NULL, target_local_id INTEGER, position INTEGER,
  bound INTEGER, PRIMARY KEY (file_id, call_local_id)) WITHOUT ROWID;
CREATE TABLE type_links (file_id INTEGER NOT NULL, block_local_id INTEGER NOT NULL, seq INTEGER NOT NULL,
  start_byte INTEGER NOT NULL, end_byte INTEGER NOT NULL, text TEXT NOT NULL,
  resolution TEXT NOT NULL, target_key_id INTEGER, reason TEXT, module TEXT,
  PRIMARY KEY (file_id, block_local_id, seq)) WITHOUT ROWID;
CREATE INDEX type_links_by_target ON type_links (target_key_id);
```

`type_links_by_target` is the reverse index: "Used in N places" is the number of type links whose
target is the declaration, and the list of places reads the same index. Rows are diffed and written
per file like slots and edges.

## 5. Reader

`unfold-reader` gains `contracts` (or similar), computed per request from one or two files' stored
facts, like the Code view. Rust computes everything; the UI only lays it out.

**Type text.** A written type becomes a `TypeText`: its text, and one span per type link inside its
range, with UTF-16 offsets relative to the text and a `style`:

| `style` | When | The UI |
| --- | --- | --- |
| `declared` | Resolved | Underlined, clickable: opens the definition |
| `external` | External, other than the built-in labels | Plain text with a dotted contour underline; hovering or clicking shows the package ("From stripe"); never opens |
| `builtin` | External `global`, `python:builtins`, or `python:typing` | Plain text, like punctuation |
| `unresolved` | Unresolved | A dashed underline whose tooltip gives the reason in words (the reader's reason text) |

**Line label** (at rest), for one arrow (`source` occurrence, `seq`):

- A call or construction (`seq` 0, resolved): one input per **argument** in written order, each the
  type of the parameter it fills (its `TypeText`), else that parameter's name (`gray`), else `…` for
  an unpaired argument; a Python keyword argument keeps its keyword (`currency=str`). The output is
  the target's `returns`, or the class's name (as a `declared` span) for a construction, or absent
  (`?`) when nothing is written.
- A reference (`seq` > 0): the passed callable's own signature, one input per parameter that takes
  an argument.

**Call contract**, for one selected line:

| Part | Contents |
| --- | --- |
| `sends` | Each argument in order: its written text (sliced from the caller's verified source; absent when the caller's file changed), `expand`, `keyword`, and the parameter it fills (name, kind, optional, default, `TypeText`) or its `unpaired` reason in words. Then each taker parameter no argument filled, marked `default` or `optional` |
| `returns` | The result binding (`variable` with its name, `return`, `argument` with the outer call's written callee, `discard`, `other`), `awaited`, and the callee's return `TypeText` |
| `throws` | The callee's direct throws ([3.6](#36-direct-throws-reader)): each as a `TypeText` over the thrown class's name, with its line; and `caught`: whether the call sits in the `body` of a `try` with a `catch` handler in the caller's function |
| `calleeLines` | The lines the contract read from in the callee: every parameter slot's lines, the `return_type` slot's lines, and each listed throw's line |
| `callerLine` | The calling line |

**Type definition**, for a declaration key: `kind` (`interface`, `type_alias`, `enum`, `class`),
`name`, `path`, `line`, `language`, its `members` (each with name, kind, flags, `TypeText` or value),
for a `type_alias` that isn't an object type its aliased `TypeText`, and `uses` (the count from the
reverse index). **Type uses**: one page of the places a declaration is used: the type link's owner
(its catalog label or name), path, and line.

## 6. Commands

Four commands in `unfold-workspace` (`src/commands/types.rs`), added to the command table and
`docs/ipc.md`, then `cargo xtask gen-ipc`.

| Command | L | Args → result |
| --- | --- | --- |
| `line_labels` | wakes | `{project, lines: [{source, seq}]}` (≤ 500) → `[{source, seq, label: LineLabel \| null}]`: the at-rest labels of every arrow on a page in one call |
| `call_contract` | wakes | `{project, source, seq}` → `CallContract` |
| `type_definition` | wakes | `{project, key}` → `TypeDefinition`; `not_found` when the key isn't a type declaration or class |
| `type_uses` | wakes | `{project, key, offset?, limit?}` (default 50, ≤ 200) → `{total, places: [{owner, label, path, line}]}` |

```ts
interface TypeText { text: string; spans: TypeSpan[] }
interface TypeSpan { start: number; end: number; style: "declared" | "external" | "builtin" | "unresolved";
                     target?: Key; module?: string; note?: string }
interface LineLabel { inputs: { keyword?: string; name?: string; type?: TypeText }[]; output?: TypeText }
interface CallContract {
  source: Key; seq: number; target: Key; callerLine: number;
  sends: { argument?: { index: number; text?: string; expand?: "spread" | "keywords"; keyword?: string };
           parameter?: { index: number; name?: string; kind: ParameterKind; optional: boolean; default: boolean; type?: TypeText };
           unpaired?: { reason: PairingReason; note: string } }[];
  returns: { binding: ResultBinding; text?: string; awaited: boolean; type?: TypeText };
  throws: { type: TypeText; line: number }[];
  caught: boolean;
  calleeLines: number[];
}
interface TypeDefinition { key: Key; kind: "interface" | "type_alias" | "enum" | "class"; name: string;
  path: string; line: number; language: Language;
  members: { name?: string; kind: MemberKind; optional: boolean; readonly: boolean; default: boolean;
             type?: TypeText; value?: string }[];
  aliased?: TypeText; uses: number }
```

The generated types are the source of truth; the shapes above are the intent. Invalidation
(`ui/src/ipc/queries.ts`): `graph_updated` of P invalidates P's `line_labels`, `call_contract`,
`type_definition`, and `type_uses`; `project_reset` removes them with everything else.

If the Code model's call links don't say which edge a references arrow came from, the link gains its
`seq`, so an arrow knows its `(source, seq)`.

## 7. UI

Lines don't change: same stroke, color, curve, and head, no return track, and arrow edges keep
`pointer-events: none`. Everything new is in the edge-label layer.

| Piece | Where | What |
| --- | --- | --- |
| Arrow model | `canvas/arrows.ts` | Each arrow carries its `seq` and its project, so labels and contracts can be asked for |
| Labels for a page | `canvas/lines/useLineLabels.ts` | One `line_labels` query per project per page for every call and reference arrow (handler arrows have none) |
| Label text | `canvas/lines/labelText.ts` (pure, tested) | `(CartItem[]) → Promise<Hold>`; three or more inputs fold to `(Cart, +2) → Receipt`; a name without a type is gray; a missing output is `?`; keywords keep their names (`currency=str`) |
| Label placement | `canvas/lines/placeLabels.ts` (pure, tested) | At the curve's midpoint (`getBezierPath`'s label point). Labels that would overlap each other are nudged apart along the curve's vertical, so text stays readable where lines cross; the label's canvas-colored background hides the lines under it |
| Closed label | `canvas/lines/LineLabel.tsx` | Rendered from `ArrowEdge` through `EdgeLabelRenderer`, `nodrag nopan`, `pointer-events: all`; a click selects the line |
| Contract | `canvas/lines/LineContract.tsx` | The selected line's label opens in place into the contract, centered where the label was and above the cards (`--z-*`). Sends, Returns, Throws; caller on the left, callee on the right; Escape, a click on the canvas, or selecting anything else closes it |
| Type text | `code/TypeText.tsx` | Renders a `TypeText` with the four span styles; a `declared` span toggles its definition |
| Type definition | `code/TypeDefinition.tsx` | The inset under the row: kind and name, `file:line`, members with their own clickable types (which open nested, up to four deep), "Peek source" (the peek sheet at that line) and "Used in N places" (a popover listing `type_uses`, each opening the peek sheet at its line) |
| Selection | `state/boards/` | A new page selection kind, `line` (arrow id, source, seq); which types are open is view state per page, never saved to the board and never an undo step. The sidebar's Selected section shows the call as written and its two cards |
| Card marks | `code/CodeView.tsx`, block cards | While a line is selected, the callee card tints `calleeLines` with `--color-step`; the caller's calling line keeps the normal `--color-selection` tint |
| Tokens | `theme/tokens.css` | `--color-type-underline` and `--color-rule-soft`, light and dark |

The label and contract use only tokens, so dark mode follows the theme. Folded cards: a label still
sits on the line; card marks apply only to unfolded Code cards (Flow cards get none in the initial
scope).

## 8. Tests and fixtures

**Fixture repository.** `testdata/fixtures/typed-lines/` mirrors the design in both languages, and is
the project for `just dev`, the e2e journey, and screenshots:

- TypeScript: `orders/create.ts` (`createOrder` calling `reserveInventory(cart.items)` and
  `chargePayment(cart, req.user)` in a `try`/`catch`), `inventory.ts`, `payments.ts` (with `throw new
  CardDeclinedError(intent)` and `import Stripe from "stripe"`), `cart.ts` (`interface Cart`,
  `CartItem`, `type CartId`, `Money`), `user.ts`, `errors.ts`, plus an unannotated module, a
  three-argument call, a passed callback, an enum, merged interfaces, generics, and a type parameter.
- Python: the same flow with a `@dataclass`, a `TypedDict`, a pydantic `BaseModel`, a `NamedTuple`,
  keyword arguments, `*args`/`**kwargs`, positional-only and keyword-only parameters, a method called
  through `self` and through the class, `raise CardDeclined(...)`, and a string forward reference.

**Compiler.** Unit tests in `unfold-core` (canonical round trips including version 1 input, every new
validation invariant with a failing case, vocabularies); `unfold-engine` (pairing rules and the
`type` policy context through the alien test language); snapshot tests of the new records for the
fixture in each language crate (`tests/structure/` for TypeScript, `tests/structure.rs` for Python),
reviewed by hand; parity, determinism, and incremental-equals-full keep passing with the new records
included in the canonical export they compare; `unfold-store` round trips and the reverse index;
`unfold-reader` line labels, contracts, throws, type definitions, and type text from hand-written
graphs; `unfold-workspace` command tests through `Host`. If the TypeScript checker oracle can resolve
type references on the fixture, every resolved type link is checked against it.

**UI.** Vitest for label text and folding, placement, `TypeText`, the contract, the definition inset,
and invalidation; one e2e journey on the fixture (the closed label's text, open the contract, open
`Cart`, "Used in", Escape), added to the coverage table in `e2e/src/coverage.ts` if that table lists
journeys.

## 9. Decisions made here

These were made without asking, to keep the build moving.

1. **Six records next to edges, not new edge kinds.** Edges stay sourced at occurrence blocks; type
   links are sourced at the block that holds the annotation and carry a range. Edge invariants and
   every consumer of edges stay as they are.
2. **Written type text is stored**, cut like edge labels, so labels on many lines don't need every
   callee file read and verified. Ranges are stored too, so spans and card marks come from facts.
3. **Interfaces, type aliases, and enums become blocks** (TypeScript only), keyed like classes. They
   stay out of the catalog and search for now.
4. **Python members are every annotated name in a class body**, whatever the decorator or base: the
   record says what is written.
5. **The at-rest label is per call**: the types of the parameters the call's arguments fill, in
   argument order, so Python keywords keep their names and folding counts what is sent. A reference
   line shows the passed callable's signature.
6. **Pairing is language-neutral** in the engine, with one language question (bound leading
   parameters); anything unproven is an `unpaired` reason, never a guess.
7. **Type links reuse the value policy and reason vocabulary** in a new `type` context; no new
   resolution reason is added, and pairing has its own four reasons.
8. **Throws are syntactic and direct**: constructions thrown by the function itself, outside a `try`
   with a handler. Re-throws and bare `raise Name` are not listed.
9. **Built-ins** are external types labeled `global`, `python:builtins`, or `python:typing`, shown
   as plain text; TypeScript keyword types never become links.
10. **The canonical export's version becomes 2**, and the reader still reads version 1 for the frozen
    references.

Decisions made while building the compiler phase:

11. **A TypeScript arrow function's expression body is a `return`**, like a Python lambda's body: the
    value is what the arrow returns, the same fact either way.
12. **A Python augmented assignment (`x += f()`) is `other`**, not `variable`: the call's value is
    an operand of the update, not the variable's initializer.
13. **Version 1 stays readable and writable.** `canonical::write_version` writes a graph at version 1
    (without typed records) so a version-1 export round-trips (`cargo xtask canonical-check` writes
    back at the export's own version), `cargo xtask reference import` keeps writing the references
    at version 1, and `Validator::without_typed_records` validates such graphs without requiring
    typed records. Parity compares `canonical::version_one` of the compiled graph (no typed
    records, no type declaration blocks, enum-nested calls moved up and keyed again) with the
    references, which is exactly "compare only what the reference has".
14. **Facts check shape, the validator checks completeness.** `GraphFacts` refuses typed facts that
    name the wrong kind of block or no slot; that every function-like block has a signature, every
    call a result, and every slot its record is a graph invariant the validator checks.
15. **Argument records follow the argument slots**, not the language's argument facts: a slot with
    no recorded argument value is a plain positional argument for pairing.
16. **A type link's range is checked against characters, not tokens**: a Python string annotation's
    name lies inside its quotes. Written-type ranges are checked against both. Checks against bytes
    for typed records name the file, since only the files read in an update are checked against
    their bytes.
17. **TypeScript type names are looked up in the type meaning only.** A value that shares a library
    type's name (`const Promise = 1`) doesn't hide it; only a type declaration does.
18. **TypeScript details:** a call that is a destructuring or parameter default is `other` (its value
    lands only sometimes); only an object type literal that is directly the aliased type has
    members (not one in a union or parentheses); `get`/`set` signatures in an interface are
    `method` members; `declare` and `abstract` class fields are members; parameter-property
    accessibility isn't recorded (no field for it). `@types/node`'s global type aliases aren't in
    the alias table (`@types/node` 25.3.3 isn't installed to generate from), so an undeclared one is
    `unknown_name`.
19. **Python details:** a decorated class (`@dataclass`) is still its type (type resolution doesn't
    refuse wrappers); a class whose name is rebound is `mutable_binding`; a string annotation is
    linked only when it is a single-part string without escapes; list items (`Callable[[A], R]`)
    and starred values in annotations link; an ordinary method called through `cls` binds no
    parameter, as Python does.
20. **Reader details:** a contract lists a taker no argument filled only when every argument filled
    a parameter or is `no_parameter`; an alias's aliased type text is sliced from the verified
    source (no record stores it); `type_uses` refuses a `limit` over 200 (`invalid_args`); Code
    links get no `seq`, since a Code link is always the occurrence itself (`seq` 0) and passed
    callables are Flow chips, which carry theirs.
21. **Bound counts are recorded, never inferred.** `result.bound` is the count pairing used (the
    language's `bound_parameters` answer), present when the occurrence resolved to a
    function-like block and the count is proven; a contract drops exactly the first `bound`
    parameters from the unfilled list and lists none when the count is unknown (it used to infer
    the count from which parameter the first positional argument filled, which listed `self` for
    `gateway.ping()`).
22. **A passed callable records its own bound count.** `Linked::reference_bound_parameters`
    answers the same question for an argument value (`obj.method` binds the instance,
    `Class.method` nothing, `Class.classmethod` the class; TypeScript 0), recorded as `bound` on
    the argument that passes a function-like callable. The label drops that many parameters;
    when the count isn't proven the label has no inputs and `inputsUnknown: true`, so the UI
    shows `(…)` rather than "takes nothing".
23. **A class is proven to take nothing only by its language** (`Linked::class_takes_no_arguments`):
    no base, no decorator (TypeScript or Python), and in Python no `__new__` in its body and no
    `metaclass=`. Only then are a construction's arguments `no_parameter`; otherwise
    `no_signature`. (Rule 5 of 3.4 used to look only at the base.)
24. **Python proves builtin exception bases.** `BaseException` and its typeshed subclasses (66,
    without the two exception groups, whose `__new__` picks the class) are proven, concrete bases
    standing in linearizations; a linearization is kept only when every builtin follows every
    user class, a lookup reaching a builtin finds nothing, and `super()` past one is
    `unsupported`. So `raise CardDeclined(…)` is a resolved construction and a direct throw.
    Parity and the oracle check are unchanged on the in-repository corpora (their exceptions are
    builtins raised directly).
25. **Code links don't gain a `seq`.** A Code link is always its occurrence's own edge (`seq` 0):
    the Code view lists occurrences, and references aren't occurrences, so a reference arrow only
    ever comes from a Flow chip, which already carries its `seq`. The arrow model takes `seq` from
    there (0 for a Code link) and the reader is unchanged.
26. **Labels sit over the cards, sized by measurement.** The closed label is in the edge-label
    layer, like a drawn link's label, so it stays readable and clickable when cards are close; its
    width comes from the bundled font's measurement (as card widths do), so placement needs no DOM
    reads. Overlapping labels are placed from the top and moved down below the label they meet.
27. **Paper's colors where the table and the artboards differ.** A label's types read in
    `--color-text` whole (`CartItem[]`, `Promise<Hold>`), as the artboard draws them; only the
    label's own punctuation (`(`, `, `, `) → `) is `--color-contour`. Inside the contract, text
    between a type's names is split: words read as text (`string`), punctuation and `builtin` names
    as contour (`Promise<`, `[]`), matching "type opened". A parameter name without a type is
    `--color-muted`; `?` and `…` are contour.
28. **The contract opens where the label was and grows downward.** It is centered on the label's
    point when it loads; opening a type keeps its top still. It takes focus, so Escape reaches it;
    Escape is a fixed key (`board.closeLine`, registered after the peek sheet's so it wins while a
    line is selected), not a rebindable action. Removing the selection does nothing for a line.
29. **Open types are keyed by row path.** `PageView.openTypes` maps a row (`send:0`, `returns`,
    `throw:0`, then `<row>/<member index>` or `<row>/=` inside a definition) to its opened span; a
    row shows one opened type, and closing or changing it closes everything opened inside it.
    The caller column shows a parameter no argument filled as `default`, `optional`, or `—`; the
    result as the variable, `return`, `outer(…)`, or `—` for a discarded value.
30. **The shadow is the floating one.** The contract uses `--shadow-float` rather than a new token
    for Paper's `0 4px 14px #20212612`, as section 1 maps it; members show `readonly` before the
    name (Paper's fixture has none), and a long name keeps 8px before its type.

## 10. Not in the initial scope

- Inferred types of any kind (no checker), generic substitution (`Promise<T>` stays as written),
  overload selection (pairing and labels use the implementation's signature), and contextual types
  of untyped callbacks.
- Type links from expressions (`as T`, `satisfies T`, `<T>`, call type arguments `f<T>()`, `extends`
  and `implements` clauses, decorators), TypeScript `import("m").T` types, and Python string
  annotations other than a dotted name.
- Type declarations in the definition catalog, in Add's search, as cards of their own, or with `⊕`
  in the peek sheet.
- Flow-view card marks, labels on entrance-to-handler arrows, and lines across projects (there are
  none).
- Re-thrown or bare-name exceptions, and exceptions thrown by the callee's own callees.
