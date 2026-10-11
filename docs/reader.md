# Reader

How Unfold turns the stored graph into what people read: derived views (owners, the definition
catalog, activation closures, applications, relations), the Code and Flow views' render models,
typed lines, boards, and Mermaid export. This document is authoritative for the rules `unfold-reader` implements; [architecture §10](architecture.md#10-the-reader)
gives the product view, and [graph-model.md](graph-model.md) defines the records these rules read.

The rules come from the proof of concept's reader (`reader/core`, its `docs/reader.md`), ported
without change except where this document says otherwise. `testdata/reference/poc-reader/` holds
what the PoC derived for both playgrounds, and the tests in `crates/unfold-reader/tests/`
(`reference.rs`, `flow_reference.rs`, `mermaid_reference.rs`, `sample_board.rs`) check that Unfold
derives the same from the same facts, apart from the differences listed in each section.

## Derived views

Nothing in this section adds facts. Every view is a function of saved blocks, roles, edges, and
entrances; none reads source.

### Owner

The owner of a block is the function, class, or file whose activation it belongs to. Function-like
kinds are `function`, `method`, `constructor`, `getter`, and `setter`. Walk up from the block:

```text
owner(block):
  current := block
  while current has a parent:
    parent := current.parent
    if parent is file: return parent
    if parent is class and current.role is not header: return parent
    if parent is function-like and current.role is body or parameter: return parent
    current := parent
  return current
```

- Nested callbacks own their own calls.
- Decorators, default values outside a parameter, and Python header expressions (`header` role)
  belong to the enclosing scope, not to the definition they decorate.
- TypeScript type declarations (`interface`, `type_alias`, `enum`) are neither callable nor
  classes: the walk passes through them, so a call in an enum member's initializer belongs to the
  enum's owner.
- Ownership never crosses files. Because blocks are stored in preorder, a block's owner is its
  parent when the parent owns it and otherwise the parent's owner, so one pass over a file computes
  every owner.

### Definition catalog

The catalog holds every function-like, `class`, or type declaration (`interface`, `type_alias`,
`enum`) block that has a name, or that is the target of a resolved edge or a resolved entrance
target. Untargeted anonymous callbacks stay out of it; they remain readable inside their owners.

| Field | Rule |
| --- | --- |
| Label | The block's name. An unnamed callable that is (inside) an argument of a `call` or `new`, before any enclosing definition or file, is `callback to <callee>`, where the callee is the written text of that occurrence's first non-reference edge (`call` when there is none). Any other unnamed callable is `anonymous function`; an unnamed class is `class` |
| Qualified name | The names of enclosing classes and callables, and of a `variable` whose initializer contains the definition, outermost first, then the label, joined with `.`: `OrderService.place`, `handlers.onCreate`, `handler.anonymous function` |

Getters and setters keep their own identities. The store keeps the catalog as the `definitions`
table and indexes the label, qualified name, and block kind for search, so search finds type
declarations with their kind and can narrow to kinds (`search`'s `definitionKinds`). A type
declaration is named after the classes and callables around it (`local.Shape` for an interface in
`function local`), but its own name doesn't qualify a definition written inside it (a callback in an
enum member's initializer is `callback to register`, not `Mode.callback to register`).

### Activation closure

The closure of a set of seeds is what is statically reached at least from them. It is a
conservative static set, shown as "reached at least from", never a claim about what runs.

```text
closure(seeds):
  visit each unit once, starting from the seeds
  a branch seed (if, case, default) considers its owner's occurrences that lie inside the branch;
  any other unit considers the occurrences it owns
  for each resolved edge among them:
    calls, imports, references -> add the target
    constructs                 -> add the target and its nearest enclosing class, if any
  a file or callable also adds the classes it owns
```

- External and unresolved edges stop the walk; every branch of the code contributes.
- A resolved edge whose target is not indexed (a damaged cache) stops the walk too.
- The closure of a seed set is the union of its seeds' closures, so closures are cached per seed.

### Applications and membership

- An **application** is a distinct resolved startup among process entrances: a file root for every
  launch detector, or the callable a Python console script (`pyproject.script`) names. Launches
  that start the same target share an application; two console scripts naming two callables in one
  file are two applications. External and unresolved startups create none (they stay launches into
  code that is not indexed).
- Its **seeds** are the startup, plus the callable's module root for a console script, so
  import-time registrations count without activating every function in the module.
- Its **name** is the startup file's name without a `py` or `[cm]?[jt]sx?` extension (`server`),
  followed by `#` and the callable's qualified name for a console script (`cli#main`).
- A non-process entrance's **anchor** is the owner of its evidence block; without source evidence,
  its first resolved target (a branch target anchors to its owner); otherwise it has none.
- A non-process entrance **belongs** to every application whose closure contains its anchor.
  Entrances that belong to none are listed as other entrances.

The store keeps applications (sorted by startup key) and memberships (sorted by entrance, then
application) as tiny tables replaced whole when they change.

### Relations

For one definition:

- **Callers** reverse resolved edges into it and map each occurrence to its owner. Each caller
  carries the owner, the occurrence, its file, the edge's sequence and kind, and a passed
  reference's argument position. They come in graph order (path, occurrence, sequence).
- **Outgoing** occurrences are the edges it owns, in source order, grouped as already on the page
  (the caller supplies the keys of that project's cards on the page), resolved and openable, and external or unresolved. Each
  carries its written text, line, and note.
- **Reached from** lists the non-process entrances whose closures contain it, in canonical order.
  A non-process entrance's closure seeds are all its resolved targets.

### External and unresolved text

External occurrences show their module; unresolved ones show a short reason. A guess never
replaces either, and an unresolved occurrence is never upgraded because its text resembles a known
function. A module label with several possible origins reads `client-a or client-b`.

| Reason | Text |
| --- | --- |
| `unknown_name` | unknown name |
| `unknown_receiver` | receiver type unknown |
| `parameter` | parameter |
| `mutable_binding` | mutable binding |
| `not_concrete` | not a concrete function |
| `accessor` | accessor |
| `ambiguous` | ambiguous target |
| `overridden` | overridden by a subclass |
| `unbound_this` | unbound this |
| `not_indexed` | not indexed |
| `not_found` | not found |
| `dynamic` | dynamic call |
| `wrapper` | wrapper |
| `unsupported` | unsupported form |

### Where each view is computed

| View | When | From |
| --- | --- | --- |
| Definitions, applications, memberships | After every update, before the commit | `project(&graph)` over the whole in-memory graph: one index pass, one catalog pass, one closure per application |
| Owner, callers, closures, reached-from | Per request | A `ReaderIndex`: a compact index of the committed graph (block numbers, parents, kinds, roles, owners, subtrees, edges grouped by owner and by target, owned classes, entrances), loaded once per graph version and dropped on the next commit. Closures are cached per seed |
| Outgoing occurrences, the Code view | Per request | One file's stored facts (`file_graph`) and, for Code, its verified bytes |
| Typed lines | Per request | The caller's and the callee's stored facts, the store's reverse index of type links, and verified bytes only where written text is shown ([typed lines](#typed-lines)) |

The index holds no names, ranges, or slots, so keeping one per graph version is cheap; it is
built either by streaming files from the store or directly from the graph the workspace just
committed.

### Differences from the proof of concept

- **Freshness and membership.** The PoC traversed only files whose source still verified when it
  computed memberships and reached-from, and it required a current launch and current entrance
  evidence. Unfold derives these views from the graph alone: the engine relinks whenever a source or
  configuration input changes, and a file that no longer parses keeps its last good facts, whose
  relationships stay navigable ([architecture §10.6](architecture.md#106-freshness)). Withholding
  an entrance's evidence range, and the membership claims that rest on it, while its configuration
  file is changed and not yet relinked is the workspace's job at request time.
- **Stored names.** The PoC's sidebar computed application names; Unfold stores them.

## Code view

The Code view shows the exact verified slice of a block (a card) or a file (the peek sheet, through
its root), with syntax colors, folds from the compiled structure, and calls as links. The reader
builds a render model that crosses IPC as JSON; the UI only lays it out.

### Verified source

Before any stored range is shown, the file's bytes are read and hashed, and the hash is compared
with the hash the facts came from ([U-17](decisions-and-tradeoffs.md#u-17-freshness-verify-before-showing-cheap-checks-first)).
Only when they match are those same bytes decoded strictly as UTF-8 and sliced.

| The check finds | The model carries |
| --- | --- |
| The bytes match | `state: "current"` with the text and everything below |
| The bytes differ (pending, or a `last_good` file) | `state: "changed"`, no text: old ranges are never applied to new bytes |
| The file is missing or unreadable, or has no facts | `state: "missing"`, no text |

The block's key, path, language, kind, and stored lines are present in every state, so names and
relationships stay navigable. A stored range that does not fit the verified text is a damaged cache
and is reported as an error, never sliced.

### The render model

Every offset is a UTF-16 code-unit offset relative to the start of the shown text
([U-12](decisions-and-tradeoffs.md#u-12-utf-8-offsets-in-the-engine-utf-16-at-the-ui)); fields are
camelCase; absent optional fields are omitted.

| Part | Contents |
| --- | --- |
| `text` | The exact slice, line endings and byte-order mark included |
| `dedent` | The indentation every nonblank line shares, the first counted at its line's indentation in the file (a method's class indentation; nothing for a top-level function); the view may hide it from the lines after the first |
| `lines` | `number` (one-based, in the file), `start`, `end` (before the terminator), `indent` (leading spaces and tabs) |
| `spans` | `[start, end, class]`, sorted and non-overlapping; text outside them is plain |
| `outline` | The statement outline in preorder (below) |
| `links` | Every occurrence in the slice (below) |

```json
{"key":"src/routes/orders.ts#function:createOrder","path":"src/routes/orders.ts","language":"typescript",
 "kind":"function","startLine":32,"endLine":48,
 "source":{"state":"current","text":"export async function createOrder(…","dedent":0,
   "lines":[{"number":32,"start":0,"end":61,"indent":0}],
   "spans":[[0,6,"keyword"]],
   "outline":[{"key":"…/if[0]","kind":"if","role":"body","startLine":34,"endLine":36,"start":70,"end":131,
               "groupable":true,"fold":{"folded":false,"headerEnd":84,"braces":true,"summary":"2 lines",
               "returns":true,"throws":false,"chain":1}}],
   "links":[{"source":"…/call[0]","owner":"src/routes/orders.ts#function:createOrder","kind":"calls",
             "resolution":"resolved","target":"src/services/order-service.ts#class:OrderService/method:place",
             "text":"service.place","span":[210,215],"line":40}]}}
```

- **Lines** end at CR, LF, CRLF, U+2028, and U+2029, exactly as stored line numbers count them, so
  line `n` of the view is line `n` of the file. A terminator at the very end of the slice (a whole
  file ending in a newline) does not start an extra empty line.
- **Highlighting** comes from the language crate's `Highlighter`, which the workspace injects; the
  reader never parses source. The whole file is highlighted, because a slice taken alone can begin
  inside a string or template, and the spans are clipped to the slice. Spans that break the
  highlighter contract (empty, overlapping, out of range, inside a UTF-8 sequence) are dropped.

### Outline, folds, and groups

The outline is derived from stored blocks only. An item is a nested callable, or a statement
(`variable`, `expression_statement`, `if`, loops, `switch`, `case`, `default`, `try`, `catch`,
`return`, `throw`, `break`, `continue`, `import`, `class`, a type declaration (`interface`,
`type_alias`, `enum`), `with`, `match`) with no role or a `body`, `then`, `else`, `catch`, or
`finally` role. Its `parent` is the index of the nearest enclosing item. Type declarations are items
so the peek sheet's `⊕` and folds reach them, as they reach the catalog's other definitions.

- **Folds.** A compound item (`if`, loops, `switch`, `try`, `catch`, callables, classes, type
  declarations, Python `with` and `match`) that spans several lines folds. The fold's `headerEnd` is
  where the hidden part begins: in Python, the end of the written header line, which stays visible;
  otherwise the start of the body slot (`then` for `if`, the braces for an interface or enum), or
  the end of the first line when there is none.
  `braces` says the folded row shows the body as `{ … }`. The `summary` reads `N lines`, plus
  `· returns` and `· throws` when the block contains a `return` or `throw` outside nested callables.
- **Default folds.** In a card longer than 40 lines, nested definitions (callables, classes, and
  type declarations) and
  an `if` that heads an `else if` chain of three or more checks (`chain`) start folded; the `else
  if` links of a chain never do. The peek sheet starts with nothing folded. Folds are view state; the
  reader never folds a card by itself.
- **Groups.** An item is `groupable` when it is a statement (not a nested callable, class, or type
  declaration) in a
  `body`, `then`, `else`, `catch`, or `finally` role. A gutter selection of lines makes a group of
  the consecutive siblings it covers: it descends into a compound item only when the selection lies
  strictly inside it, and keeps the first covered item's role. `group_for_lines` returns those members,
  or none when the selection makes no valid group.

### Call links

Every call, construction, and import in the slice is a link, with the occurrence's key (`source`;
an arrow for an opened call leaves from its `line`), its `owner`, the edge's kind, resolution,
target, written text, and, when external or unresolved, its module or reason and a display `note`.

- The underline covers the written callee (an import's specifier; the whole occurrence when neither
  slot was recorded).
- A nested call that begins the callee keeps its own link, and the outer link starts after it:
  in `res.status(202).json(…)` the inner call links `res.status` and the outer links `json`.
- Shorter links claim characters first, so links never overlap; a link left without characters has
  no `span` but keeps its line. Leading `.`, `?`, and whitespace are trimmed from the underline.
- Links are occurrence edges, so an arrow from a Code card is the edge `(source, 0)`. Passed
  references (`seq` > 0) appear as Flow chips, which carry their `seq`.

### Differences from the proof of concept

- **Every occurrence in the slice is a link.** A PoC TypeScript card linked only the occurrences its
  definition owned, so calls inside an unfolded nested callback were plain text; Python cards also
  linked header calls. Unfold links every occurrence the slice shows and says who owns it.
- **Import links** cover the specifier instead of the whole import statement.
- **Lines** also end at U+2028 and U+2029, matching stored line numbers, and a final terminator adds
  no empty line.
- **Highlighting** runs on the whole file instead of the slice.
- **The peek sheet** is new: its fold policy starts nothing folded.

## Typed lines

A line on a board can say what flows through the call it draws: what goes in, what comes back, and
what can be thrown. `unfold_reader::contracts` computes it per request from the typed records
([graph model](graph-model.md)) of one or two files (the caller's and the callee's), read once each
through a request's `Facts`, plus the verified source only where written text is shown. Nothing is
inferred: a type is what the code wrote, a pairing is what the engine proved, and a missing record
reads as unknown, never as a guess. The proof of concept had no typed lines.

### Type text

A written type crosses IPC as a `TypeText`: its text (the stored written text, cut to 120 UTF-16
units) and one span per type link inside its range, in order.

| `style` | When | Extra fields |
| --- | --- | --- |
| `declared` | The link resolved | `target`: the declaration's key |
| `external` | External, other than the built-in labels | `module`, and `note`: the module in words (`client-a or client-b`) |
| `builtin` | External `global`, `python:builtins`, or `python:typing` | `module` |
| `unresolved` | Unresolved | `note`: the [reason in words](#external-and-unresolved-text) |

- Offsets are UTF-16 code units relative to the text. The text is the written source of the type's
  range, so a link's byte offset from the range's start is its byte offset in the text; no source is
  read.
- A link the cut text no longer holds whole is left out, as is one that doesn't lie inside the range
  or on character boundaries (only a damaged cache has those).
- A name that stands alone (a constructed or thrown class) is a `TypeText` whose one span covers all
  of it.

### Line labels

The at-rest label of one arrow, addressed by its occurrence and edge `(source, seq)`:

| Arrow | `inputs` | `output` |
| --- | --- | --- |
| A resolved call or construction (`seq` 0) | One per argument, in written order: the written type of the parameter it fills, else that parameter's `name`, else nothing (an argument that fills no proven parameter, or a pattern parameter; the UI shows `…`). A Python keyword argument keeps its `keyword` | The callee's written return type; for a construction (a class target, a constructor, or the method a Python construction reaches) the class's name as a `declared` span; absent when nothing is written |
| A resolved passed reference (`seq` > 0) | The passed callable's own signature: one per parameter that takes an argument (every kind but TypeScript's `this`), its type, else its name, after the parameters the passed value binds itself (its argument's `bound` count: `obj.method` binds `self`). When a function's count isn't proven (a Python `classmethod` passed by its bare name), `inputs` is empty and `inputsUnknown` is true: what it takes is unknown, not nothing, and the UI shows `(…)` | Its return type (a passed class: its name) |
| Anything else | No label (`null`): external and unresolved edges, imports, unknown occurrences and edges | |

### Call contracts

The contract of one selected line: a resolved call or construction (`seq` 0). Any other edge has
none.

| Part | Contents |
| --- | --- |
| `sends` | Each argument in written order: `argument` (`index`; `text`, sliced from the caller's verified source and cut to 120 UTF-16 units, absent when the caller's file changed; `expand`; `keyword`) and the `parameter` it fills (`index`, `name`, `kind`, `optional`, `default`, `type`) or why it fills none (`unpaired`: `reason` and `note`). Then each parameter no argument filled, with only `parameter` |
| `returns` | `binding` (`variable`, `return`, `argument`, `discard`, `other`), `text` (the variable's name, or the outer call's written callee from its occurrence edge), `awaited`, and `type`: the label's output |
| `throws` | The callee's [direct throws](#direct-throws), each a `type` over the thrown class's name and the `throw` statement's first `line` |
| `caught` | The call sits in the `body` of a `try` that has a `catch` handler, below the caller's own function (its owner) |
| `calleeLines` | The callee lines the contract read, ascending: every `parameter` and `return_type` slot's lines (from the callee's verified source; none when it changed, since the card shows no text to mark) and each listed throw's line |
| `callerLine` | The calling line |

**Unfilled parameters.** They are listed only when the pairings say which parameters were filled:
every argument fills a parameter or is `no_parameter`. An expanded argument (`spread`), an unknown
receiver binding (`receiver`), or a class without its own constructor (`no_signature`) could fill
any of them, so then none are listed. Parameters that take no argument (`this`) are left out, and so
are the first `bound` parameters, which the call binds itself (its result's `bound` count, proven by
linking: a Python method's `self` reached through an instance). When the count isn't proven, no
unfilled parameter is listed. So `gateway.ping()` lists `ping`'s parameters after `self`, while
`Gateway.ping()` lists `self` too, and `Gateway.ping(gateway)` lists only those after it.

| Pairing reason | Note |
| --- | --- |
| `spread` | spread argument |
| `no_parameter` | no parameter takes it |
| `receiver` | receiver binding unknown |
| `no_signature` | no constructor signature |

### Direct throws

The direct throws of a function-like block are every `throw` block it owns (not one inside a nested
callable) whose expression is exactly a `new` (TypeScript) or `call` (Python) block with a
`constructs` edge, and which isn't inside the `body` of a `try` that has a `catch` handler in the
same function (it may be caught). Statements in a `catch`, `finally`, or Python `else` are not in
the `try`'s body.

- A resolved edge names a class: the target itself, or a constructor's parent class (or the class
  of the method a Python construction reaches). Its type is the class's name as a `declared` span.
- An external or unresolved edge keeps its written text, with its module or reason.
- A bare `throw error`, `raise`, or `raise Name`, a re-throw, and a call that isn't a resolved
  construction (a Python call without proven class identity is a `calls` edge) construct nothing
  provable and are not listed. Neither are the exceptions the callee's own callees throw.

### Type definitions and uses

A **type definition** is opened for an `interface`, `type_alias`, `enum`, or `class` block; any
other key has none. It is what a line's contract opens for a written type, and what a **type card**
shows: a card of one of those blocks lists, in Diagram view (and in Flow for a type declaration),
its aliased type, its members, and its inherited groups, and a `declared` span in a field's type that
names another card of the same project draws a type arrow from that field to the card
([UI shell](ui-shell.md#screens)).

| Field | Contents |
| --- | --- |
| `key`, `kind`, `name`, `path`, `line`, `language` | The declaration; `name` is its catalog label when it has no name |
| `members` | Its member records in source order: `name`, `kind`, `optional`, `readonly`, `default`, and the written `type` (with its spans) or an enum member's `value` |
| `inherited` | The members it inherits, one group per base ([inherited members](#inherited-members)): `from`, the base's name as written with one span (`declared` with its key, `external` or `builtin` with its module, `unresolved` with the reason in words), and `members`, the base's own members as above (none unless `declared`) |
| `aliased` | For a type alias with no members (its aliased type isn't an object type): the text of its `type` slot from the verified source (no record stores it) with the type links inside it, cut to 120 UTF-16 units; absent when the file changed |
| `uses` | How many type links resolve to it, from the store's reverse index ("Used in N places") |

**Type uses** are one page of those type links, by path, then block, then range. Each place has its
`owner`, the definition the annotation belongs to: the link's block when it is function-like or a
type declaration (a parameter's type belongs to its function, a member's to its interface, a base
to the class or interface whose heritage clause names it), else that block's owner (a variable
annotation belongs to its function or file); the owner's catalog `label`; the `path`; the `line` of
the written name, from the verified source (the block's first line when the file changed); and, for
a heritage clause, `heritage`: whether the owner `extends` or `implements` the type.

#### Inherited members

A definition's `inherited` groups follow the heritage links of its block that are `extends` (an
interface's or a class's `extends`, a Python class's bases); `implements` names an interface whose
members the class must write itself, so it contributes none.

- **Order.** Depth first in written order: a base's group, then the groups of that base's own bases,
  then the next base. For `interface Order extends OrderDraft, Timestamped<Date>, Tracked` where
  `Tracked extends Timestamped<Date>`: `OrderDraft`, `Timestamped`, `Tracked`.
- **Each type once.** A declared base already listed is skipped, and so is the definition itself,
  so a diamond lists its shared base once and a cycle (`Loop extends Knot`, `Knot extends Loop`)
  stops. An external or unresolved base reads the same wherever it is written, so it is listed once
  too.
- **Depth.** Bases more than `MAX_INHERITED_DEPTH` (8) levels down are not listed.
- **No declaration.** An external, builtin, or unresolved base is a group without members (Python's
  `BaseModel`, `Generic`; a merged TypeScript interface, which is `ambiguous`), and the walk doesn't
  go past it.
- **As written.** Members are the base's member records: a generic base's members keep their written
  types (`createdAt: T`, not `Date`), and a member a type writes again isn't hidden.

## Flow view

Flow is labeled **Paths from written structure**. The reader derives it from saved blocks, roles,
slots, and source order ([architecture §10.4](architecture.md#104-flow-view)); it never re-parses the
source to build a program model, and the graph has no control-flow graph. Flow is not a runtime
trace: short-circuit expressions stay as written, labeled transfers get no guessed destination, and
exceptions and asynchronous behavior are not modeled. `flow::flow_view` builds a card's Flow from one
file's stored facts and its verified bytes; a changed or missing file gets only Start and one
explanatory node, never old ranges applied to new bytes.

### What is drawn

| Saved structure | Drawn as |
| --- | --- |
| Callable body | Start, the written statements, and End where a route reaches it |
| Statement, variable, call, construction | One node with its call chips. A main call (a `calls` or `constructs` occurrence ending where the statement's `expression`, or a variable's `initializer`, ends) makes it a `call` node |
| `if` and `else if` | A `condition` with yes and no lanes. The head of a chain of three or more checks carries `checks` and `outcome` (`throw`, `return`, or `mixed`), so the UI can fold it into one node with a "▸ N checks" tag |
| Loop (`for`, `for_in`, `for_of`, `while`, `do_while`) | A `loop` header and its body: `each` into the body, `next` back from its end, `done` out. A do-while is entered at its body, routes `again` into it and `test` back |
| `return`, `throw` | A terminal node. A return also routes `return` to End |
| Unlabeled `break` or `continue` | A transfer: `break` leaves the enclosing loop like `done`, `continue` returns to its header |
| TypeScript `try`/`catch` without `finally` | A `try` region beside its `catch` region, with a dashed `on error` route from the try body's entry to the handler's |
| Nested callback or class | A separate definition: a statement lists the definitions written in it (`callbacks`, with their catalog label, line count, and whether they throw), and their calls belong to them |
| Nested `function` or `class` statement | A `definition` node labeled with its qualified name |
| TypeScript `switch` (and a `case`, `default`, or `catch` shown alone), `try` with `finally` or without a handler; Python `with`, `match`, `case`, loops with `else`, and every `try` and `except` | An `opaque` node with a warning and no outgoing route |
| A class | Its members as `definition` nodes, with no routes |
| A file | One explanatory node: file initialization is not drawn |
| TypeScript `interface`, `type_alias`, `enum` | Nothing: type declarations are not steps of a path (the app shows a type card's fields instead) |

- Statements after a terminal or opaque node stay visible without an incoming route.
- A labeled `break` or `continue` is opaque (the exact block text tells it from a plain one); one
  outside any loop of the card gets no route and a warning.
- Statements nested more than 100 levels deep become one opaque node, so deep nesting cannot
  exhaust the stack.

### The model

`FlowModel` crosses IPC as camelCase JSON. Rust computes structure; the UI computes geometry, so the
model has no coordinates.

| Part | Contents |
| --- | --- |
| `nodes` | `id` (its index), `key`, `kind` (`start`, `end`, `variable`, `call`, `statement`, `condition`, `loop`, `return`, `throw`, `break`, `continue`, `definition`, `opaque`), `blockKind`, `label`, `fallback`, `startLine`/`endLine`, `lane`, `chips`, `call` (the main chip's index), `checks`/`outcome`, `callbacks`, `warning` |
| `routes` | `from`, `to`, `kind`: `sequence`, `yes`, `no`, `elseIf`, `each`, `again`, `next`, `test`, `continue`, `done`, `break`, `return`, `error` (dashed) |
| `tree` | The lane structure top to bottom: `node`, `branch` (`yes`, `no`, `yesLane`, `elseIf`), `loop` (`body`, `testAfter`), and `try` (`body`, `handler`, `catchLane`, `binding`) items, each with `ends` and `lanes` |
| `start`, `end`, `lanes`, `source`, `warnings` | Start and End node ids, the width in lanes, the source state, and warnings (`code` and `message`) without duplicates |

A branch keeps `no` in its own lane and puts `yes` in the lane `yesLane` to the right; when the yes
lane ends and an `else` exists, the else continues below in the main lane instead. A catch region
sits `catchLane` lanes to the right of its try region. Node ids follow derivation order (a sequence
derives all its statements before connecting them, a condition precedes its branches, a loop header
follows its body), the same order the proof of concept used.

### Labels

A node's type comes from the compiled structure; its label is the page's authored label for that
block. Without one it falls back to a name from the code, and `fallback` is true so the UI draws a
dotted underline:

| Node | Fallback |
| --- | --- |
| `call` | The target's qualified name when resolved (the workspace supplies names from other files through `FlowContext`), otherwise the written callee, shortened to 48 UTF-16 units |
| `variable` | Its name, or the names its bindings declare, or the names written in its name slot |
| `condition` | The names written in the condition, then `?` (`req.header?`); a folded chain shows the names of all its checks |
| `loop` | The names written in the initializer, iterable, and condition (a do-while: its condition) |
| `statement` | The first two names written in it |
| `return`, `throw`, `break`, `continue` | `Return`, `Throw`, `Break`, `Continue` |
| `opaque` | What it is: `switch command`, `try … finally`, `with`, `Loop with else`, … |

Start shows the block's name and never takes a label; End shows `End`. Names come from a lexical
scan of the verified text of the slot or block, never from parsing: at most four (two for a
statement) dotted identifier paths, in order, without duplicates, dropping a path that extends an
earlier one and reserved words, then `…` when there are more. TypeScript blanks comments, strings,
templates, and regular expressions first and drops a leading `this.`; Python skips comments,
strings (with their prefixes), numbers, and keywords.

### Step groups

A card's [groups](#groups) are drawn in Flow too, where the app calls them **step groups**. The model
carries nothing for them: the UI finds each group in the lane tree, where the group rule makes its
members a run of consecutive items of one list (an item's key is its node's, or a `try`'s own). A
group whose statements Flow doesn't show (inside an opaque node, or inside a folded "▸ N checks" chain
until it is opened) isn't drawn.

- **Expanded**, a step group is a titled region around its steps, laid out like a loop's region; its
  steps keep their labels, and routes into it turn above the region and cross its title band.
- **Collapsed**, it is one step showing the group's title, its number of steps, and a `returns` or
  `throws` mark when a statement inside can return or throw (the reader's fold summary for a compound
  member, else the member's own kind and the statements written in it, outside nested callables).
  Routes enter at its first step and leave from its last. A route that leaves from inside keeps its
  kind when it is a `return` (to End), a `break`, `continue`, `next`, `test`, or `on error`; any
  other (a condition's `yes` or `no`, a loop's `done`) leaves as plain sequence. A throw ends there,
  as a throw node does. Arrows for calls inside leave from the step, as from a collapsed group in Code.
- **One collapse state.** The group's `collapsed` field folds it in Code and in Flow.

### Differences from the proof of concept

- **Labels instead of code in exports.** Mermaid uses the same labels as the app (below).
- **Start** shows the block's name, not its name slot's text; a variable with neither a name nor
  named bindings falls back to the names in its name slot instead of the clipped slot text; listed
  callbacks use their catalog label (`callback to items.map`); the catch binding is its first name.
- **No raw code** in the model: the PoC's `code` field is gone.
- Everything else (node order, kinds, lanes, chips, the main call, routes and their labels, the lane
  tree, chain folding, opaque regions, warnings) matches the PoC for every definition of both
  playgrounds (`tests/flow_reference.rs`).

## Boards

A board is a file of authored choices ([architecture §9.1](architecture.md#91-board-files),
[U-22](decisions-and-tradeoffs.md#u-22-a-fresh-board-format),
[U-42](decisions-and-tradeoffs.md#u-42-the-board-format-pages-folders-and-other-projects)): titled pages of
structural references, positions, each card's saved view and fold, groups, descriptions, Flow labels, guards,
links (drawn links, and connections when they have a kind), frames, resource cards, and notes. It holds neither
copied source nor compiled arrows. Its JSON Schema is
[`schemas/unfold-board.schema.json`](schemas/unfold-board.schema.json) (the crate's copy,
`crates/unfold-reader/schema/`, is the source; `tools/oracles/check-board-schema.mjs` checks the two are identical
and validates the boards in `testdata/boards/` with an independent validator). The code is
`crates/unfold-reader/src/board/`; the
workspace's board commands ([IPC](ipc.md#boards)) drive it.

### Format

| Field | Rule |
| --- | --- |
| `format`, `version` | `"unfold-board"` and `1`. Anything else is refused; there is no other version and no reader for an older shape |
| `title` | Not blank |
| `folder` | Optional: the board's folder ([below](#pages-cards-and-folders)); absent at the project's top level |
| `projects` | Exactly the sorted set of other projects any item of any page names: cards' `ref.project`, resources' `project`, anchors' `project`, and links' `carriesProject`; `[]` when every item is the board's own project's. Anything else is refused |
| `pages` | One or more, in the board's page order; ids unique in the board |
| page `id`, `title`, `defaultView` | An id, a title that isn't blank, and `"code"`, `"flow"`, or `"diagram"`: the view of cards without their own |
| page `cards[]` | `id`; `ref`, exactly one of `block` (a key with `#`) or `entrance` (an `entrance:` key), plus an optional `project`; `at`, two finite numbers; optional `view` (`"code"`, `"flow"`, or `"diagram"`); `folded`, written only when true; optional `description` |
| page `groups[]` | `id`, `card` (a block card of this page), `title`, `members` (one or more distinct block keys of that card's project), `collapsed`, optional `description`, optional `author` (who wrote the title; absent on older groups) |
| page `labels[]` | Flow-node labels: `{key (a block key), project?, text, author, signature}`, one per (project, key); optional on input, always written |
| page `guards[]` | `{key, project?, kind (a block kind or entrance:<kind>), owner (the owner's key; empty for an entrance), signature}`, one per referenced (project, key); optional on input, always written |
| page `links[]` | `{id, from, to, label?}` and, for a **connection**, the fields below; `from` and `to` are two different cards, resources, or frames of the page; `label` is a description. Optional on input, always written |
| link `kind` | `http`, `rpc`, `queue`, `event`, `writes`, `reads`, `calls`, `relates`, or `other`; absent for a drawn link (drawn as `other`). With a kind, `author` is required |
| link `step`, `channel`, `carries`, `carriesProject` | `step`: a block key inside the `from` card's block, so only when `from` is a block card. `channel`: what travels, not blank. `carries`: a type declaration or class key; `carriesProject` only with it |
| link `fromColumn`, `toColumn`, `cardinality` | Only on `relates`, whose ends are two tables. A column of a table that lists its own columns must be one of them. `cardinality` is `one-to-one`, `one-to-many`, `many-to-one`, or `many-to-many` |
| link `anchors`, `description`, `author`, `signature` | Optional; `anchors` written only when there are some |
| page `frames[]` | `{id, title, description?, members, look, icon?, direction?, collapsed?, author}`. `members`: one or more distinct ids of cards, resources, notes, or frames of the page; an item is in at most one frame; frames never hold each other in a cycle and nest at most 3 deep. `look` is `box` or `zone`; `icon` one of `api`, `worker`, `schedule`, `cli`, `web`, `library`, `system`; `direction` `right` or `down`; `collapsed` written only when true |
| page `resources[]` | `{id, kind, name, project?, type?, columns?, anchors, at, description?, author, signature?}`. `kind` is `table`, `queue`, `cache`, `storage`, `external`, or `actor`; `name` not blank. Only a table has a `type` (a block key; `project` is its project, and only with it) and `columns` (`{name, type?, key?, references?, optional?}`, names distinct; `key` is `primary`, `foreign`, or `unique`; only a foreign key has `references`). A table backed by a type lists marks only: `name`, `key`, `references` |
| page `notes[]` | `{id, text, at, attach?, author}`: plain text of 1 to 400 characters, not blank; `attach` names a card, frame, resource, or link of the page, and then `at` is an offset from it |
| anchor | `{key, project?}` (a block or entrance key) or `{file, lines: [first, last], project?}` (lines of an [evidence file](settings-and-scope.md#evidence-files), or of another input the graph records: a path relative to the project's folder, lines 1-based and inclusive) |
| `publishes`, `baseRevision` | Only in an unpublished-changes copy ([below](#files-drafts-publish-and-unpublished-changes)): `"boards/<name>.json"`, matching the copy's file name, and the 64 lowercase hex digits of the revision it started from. Refused anywhere else |

A **description** or **label** has `text` (not blank), `author` (`person:<username>` or `agent:<name>`), and
`signature`: `sha256:` and 64 lowercase hex digits, or `""` (or absent) for text not yet signed, which the next
save signs from verified source.

- **Strict parsing.** Unknown fields, duplicate JSON keys, wrong types, and malformed values are refused with the
  JSON pointer of the offending value ("invalid board at /pages/0/cards/2/ref: must have exactly one of block or
  entrance"). A file over 16 MiB is refused. Parsing checks the shape and the references between the items of one
  page (a frame's members, a link's ends, a note's item: "invalid board at /pages/0/frames/1/members/0: `run` is
  already in frame `server`; an item is in at most one frame"); whether keys exist is checked against the graph.
  Removing an item is the window's job: a page that leaves a reference to it is refused. Rust's own repairs remove
  what would dangle (see [Health and repairs](#health-and-repairs)), and every save checks the page's references
  again before writing.
- **Deterministic serialization.** Object keys sorted by UTF-16 code units; `pages` in board order; cards, groups,
  links, frames, resources, and notes sorted by id; labels and guards by project (the board's own first) and key;
  two-space indentation; numbers and string escapes exactly as JavaScript's `JSON.stringify(value, null, 2)` writes
  them; optional fields omitted when absent, and `frames`, `resources`, and `notes` when empty; and a final newline.
  Loading and saving a board written this way reproduces it byte for byte; the sample board does, and so does
  `testdata/boards/order-system.json`, which holds every item. The window receives pages in the same shape.
- **What a window sends.** A save carries only `{title, pages}` (`BoardEdit`). Rust keeps the folder from the open
  board, derives `projects`, and writes the file's framing, so moving a board between folders never races its
  autosave.

### Pages, cards, and folders

- **Ids.** Page and item ids are a letter or digit followed by up to 99 letters, digits, `_`, `.`, or `-`. A page's
  cards, groups, links, frames, resources, and notes share one id space, so an id is unique within its page and the
  same card id can appear on two pages. A new board's first page is `page-1`.
- **Folders.** A folder path is 1 to 8 names joined by `/`; each name is 1 to 64 characters, with no leading or
  trailing whitespace, no control characters, no `/` or `\`, and isn't `.` or `..`. Names compare exactly. A
  folder exists while a board is in it or under it; an empty one is kept in the project's `.unfold/local/state.json`
  until a board goes into it ([U-47](decisions-and-tradeoffs.md#u-47-folders-are-a-field-on-the-board)). A published
  board's folder is shared with the team; a draft's is local.

### Cards from other projects

A card's `ref.project` is the **shared name** of the project its key is in, and is absent for the board's own
project ([U-44](decisions-and-tradeoffs.md#u-44-boards-that-span-projects-with-drawn-links)); so are a resource's and
an anchor's `project` and a link's `carriesProject`. A save that writes the board's own project's name in any of
them is refused ("omit project for the board's own project").

- **Resolving.** A name resolves to the first connected project with that shared name, in workspace order, the
  board's own included. A name that resolves to no project is **not connected**.
- **Checking.** Each page is checked per project: the references of each connected project against that project's
  graph (`PageGraphs`, one `BoardGraph` per project, loaded for the keys the page references in it). Loading another
  project's graph never wakes it: a resting project is read through its read-only pool.
- **Not connected.** A card from a project that isn't connected is a placeholder ("From storefront-web · not
  connected"), never an error, and is never dropped. That project's labels, groups, guards, and the claims citing
  it are kept as they are and not checked.
- **Arrows** (opened calls) join only cards of one project, because the graph has no edges between projects. Cards
  of different projects are joined by drawn links.

### Signatures

A signature is the SHA-256 of the exact selected source bytes, computed only from bytes that verified against the
facts' hash of the reference's project, so text can never be signed against source the graph has not seen:

| What | Selected bytes |
| --- | --- |
| A card (and its guard) | The block's range, or an entrance's evidence range (in its source file or configuration input) |
| A group | From the start of its first member to the end of its last |
| A label | The labeled block's range |
| A drawn link's label | `"<from card's guard signature>\n<to card's guard signature>"` (each the hex digits) |
| A resource card, and its description | Its type's block range (a table backed by a type), then each anchor's bytes, each followed by `\n` |
| A connection (a link with a `kind` that cites code), and its description and label | Its step's block range, then each anchor's bytes, each followed by `\n` |
| A frame's description | Its member cards' signatures (direct and in nested frames, sorted by card id), hex, joined with `\n` |

An anchor's bytes are a block's range, an entrance's evidence range, or an evidence file's lines (from the start of
the first through the end of the last, its line break included), all verified against the hash the graph recorded.
A save signs unsigned claims, and signs a resource card or connection again when what it cites changed.

When a signature no longer matches the verified bytes (or the bytes do not verify), the description, label, or claim
is **outdated**. A drawn link's label is signed when it is saved but never reported outdated: a drawn link is a
person's claim, and health checks only that both ends still exist. A label on a drawn link with a resource or frame
end, and a claim that cites no code (an `actor`, a `writes` connection without a step), aren't signed.

### Guards and reconnecting

Every save records a guard for each referenced (project, key) of a connected project (cards, group members, labels,
and what claims cite: tables' types, key anchors, connections' steps and carried types): its kind, owner, and
signature. Guards for references no longer used are dropped; guards of a project that isn't
connected are kept unchanged. After a graph update, references reconnect by key:

- A **surviving key** reconnects. If its text no longer matches, the text is outdated.
- A **missing key** stays on the page as a placeholder, reported as missing.
- A guarded key whose **kind or owner changed**, or a **position-based** key (one with an ordinal step such as
  `if[0]`) whose source changed, is **held for review** instead of attaching its text to different code. Held
  references cannot be described, and Mermaid draws them as "needs review".
- **Renames and moves** leave the old key missing; the Reconnect repair points it at the new key, in the card's own
  project, everywhere on the page: cards, group members, labels, tables' types, anchors, and carried types.
  **Reconnecting a card carries its connections' steps along**: a step under the old key moves to the same path
  under the new key (`…#function:place/expression_statement[1]/call[0]` → `…#function:placeOrder/…`), with a fresh
  guard.
- **Evidence ranges** have no guard: they are missing when the graph no longer records the file or the file has
  fewer lines, and outdated when the lines' bytes change. The Reconnect repair points one anchor, by its position,
  at other lines or a key.

### Grounding

A save checks every claim that is new or changed in what it says or cites (not in its position or its texts), after
its references were checked, so what it cites exists:

- a table backed by a type marks only the type's fields (its properties, inherited ones included), and a `relates`
  connection joins only columns its tables have (the fields of a table backed by a type);
- every resource card an **agent** wrote (`agent:` author), except an `actor`, has an anchor, and so does an agent's
  `http`, `rpc`, `queue`, `event`, or `calls` connection: a person's may lack them;
- an agent's resource card's `name`, and the columns a table lists itself, appear in the verified bytes it cites
  (its type's and its anchors'): a plain text search;
- an agent's connection's `channel` appears in its step's or anchors' bytes, unless its `to` end is an entrance,
  whose channel the save fills in for every connection: an `http` connection gets the route's `METHOD path`, a
  `queue` connection the worker's queue;
- an agent's `carries` is used, by a type link, inside the step's owner (the function it is in) or inside an
  anchor's block (an entrance anchor's evidence block);
- an agent's `calls` connection leaves from a call Unfold can't follow: its `step` is a `call` or `new` block whose
  occurrence edge is unresolved or external.

An agent's claim citing a project that isn't connected can't be grounded and is refused. A refusal is
`board_problem` and says what failed, why, and what to do instead ("Can't add the table "orders": the column
"customer_id" doesn't appear in its anchors (OrderRepository in src/db/repository.ts). Cite where the column is
defined, or leave it out"). Claims carried over unchanged aren't checked again, so code that changes under a grounded
claim makes it outdated, not refused.

### Groups

A group is a run of consecutive sibling statements with the same parent and the same body role (`body`, `then`,
`else`, `catch`, `finally`), inside its block card and not the card's block itself. Groups in one card never overlap
or nest. `group_problem` names the first rule a group breaks. Code draws a group as a magenta rail and Flow as a
[step group](#step-groups); its one `collapsed` field folds it in both.

### Health and repairs

Health is computed per page, and every item names its page and, for labels and keys, its project:

| Kind | When | Repairs offered |
| --- | --- | --- |
| `missing` | A card's or label's key is not in its project's graph; a resource card's type or anchor is gone (an evidence range: the file isn't recorded, or has fewer lines) | Reconnect, remove the card, label, or resource card |
| `outdated` | A card, group, or label signature no longer matches; a resource card's, connection's, or their texts' signature, or a frame description's, no longer matches | Re-sign (keep the text, accept the code), remove the description or label (a connection's description and label), remove the resource card or connection |
| `brokenGroup` | A group breaks the group rules, or overlaps or nests with another | Ungroup |
| `review` | A guarded reference (a card, a group member, a label, a key a claim cites) is held for review | Accept (save a fresh guard for the current code), Reconnect, remove or ungroup |
| `notConnected` | A card's `ref.project` resolves to no connected project | Remove the card |
| `brokenLink` | A link's end is a card that is missing from its connected project (a resource or frame end never is); a connection's step is gone or not inside its card's block, its carried type is gone or isn't a type, or an anchor is gone | Reconnect (the step, type, or anchor), remove the link |

Each item names its page and its target (a card, group, label, link, resource card, or frame), and, for a reference
that isn't the target itself, its key and project, and an anchor's position (`anchor`). Claims citing a project that
isn't connected are kept unchecked, like its cards' labels and groups.

A board's list label sums its items, claims' included ("1 missing · 2 outdated · 1 not connected · 2 broken links");
a healthy board has none. `apply_repair` performs a repair on one page: `reconnect` (a key, page-wide),
`reconnect_anchor` (one anchor of a resource card or connection, by position, to a key or an evidence range),
`remove_card`, `remove_label`, `ungroup`, `remove_link`, `remove_resource`, `remove_description`, `resign`, and
`accept`. Removing a card, a link, or a resource card removes what would dangle: a
card's groups, the links that end at it, its place in a frame (a frame left with no members goes too, with the links
to it), and the attachment of a note, which stays where it was, unattached (at the item's position plus its offset:
a card's or resource's position, a link's ends' midpoint, a frame's members' top-left). Saving (`prepare_save`) re-checks only the pages that
changed, restores guards an edit dropped (from the version being replaced), signs unsigned descriptions, labels,
link labels, and claims, saves guards, and refuses a board that introduces a missing reference, a broken group, a
held reference, or a broken link that the previous version did not already have, or a new or changed claim that
isn't [grounded](#grounding); problems carried over unchanged are
kept, and a card that isn't connected is never a save error, so a board with a placeholder can still be moved or
retitled.

The workspace computes health for every board of a project, cached per board by its file's revision and the graph
generations of the projects its cards resolve to, and computes it again after a graph update of any of them. It
feeds the project's Problems list and Home's Health line.

### Outline

A page's outline ("On this page") lists its cards in follow order. A *card link* is an opened call between two cards
of one project: an entrance card to the cards of its resolved targets, in registration order, or a block card to
the cards of the resolved targets of the occurrences it owns, in source order. Roots are the cards nothing on the
page links to (entrance cards first, then left to right and top to bottom); each is followed depth first through its
card links and then the page's links between two cards (in id order; links to resources and frames aren't followed),
indented by call depth. Cards reachable only through a
cycle come last. The outline also lists the Flow nodes, in that order, that still show a fallback name ("N Flow steps
unlabeled").

### Files, drafts, Publish, and unpublished changes

| Board | File | Its id |
| --- | --- | --- |
| Draft | `.unfold/local/drafts/<name>.json` (the `local/` folder ignores itself for Git) | That path, also when `local/` is in the fallback location |
| Published | `.unfold/boards/<name>.json` | That path |
| Unpublished changes of a published board | `.unfold/local/drafts/<name>.changes.json`, a board document with `publishes` and `baseRevision` | The published board's id: a board keeps its id while it has unpublished changes |

- **Names** are plain stems (letters, digits, `.`, `_`, `-`, starting with a letter or digit). A new draft is
  `board-<milliseconds>`, then `-2`, `-3`, … when taken; a draft's name never ends in `.changes`. A published name is
  the slug of the title at its first Publish: the slug drops accents and turns every other run of non-alphanumeric
  characters into `-` (`Order placement` → `order-placement`, `Café Crème` → `cafe-creme`; nothing left → `board`).
  It never changes afterwards: renaming a board changes its title only.
- **Saving** replaces the file atomically: a temporary file beside it is written, synced, and renamed over it.
  Board directories must be real directories, not links. Before a project's first Compile there is no `local/`, so
  there are no drafts.
- **The working file** of an open board is the file its saves write: a draft's file; a published board's changes
  copy when it has one; else its published file.

| Action | Draft | Published, no copy | Published, with a copy |
| --- | --- | --- | --- |
| Save | Writes the draft | Writes a new copy whose `baseRevision` is the published revision the board was loaded from | Writes the copy; a save that makes the board equal to the published one deletes the copy |
| Publish | Writes `boards/<name>.json`, never overwriting (a taken name reports a free one, `order-placement-2`); deletes the draft; the id changes | Nothing to do | If the published file is still the copy's `baseRevision`: writes the copy over it atomically and deletes the copy. Otherwise a **conflict**, unless the person keeps their version (then it is written anyway). A deleted published file is a conflict too; keeping mine writes it again |
| Discard changes | — | Nothing to do | Deletes the copy and returns to the published file (also Publish's "take theirs") |
| Move back to drafts | — | The published content becomes a new draft; the id changes | The copy's content becomes a new draft; the copy and the published file are deleted; the id changes |
| Delete | Deletes the draft | Deletes the published file | Deletes both |
| Duplicate | A new draft, "<title> copy", in the same folder, from the working version | Same | Same |

- **Unpublished changes** count the differences between the working and the published version
  ([U-43](decisions-and-tradeoffs.md#u-43-edits-to-a-published-board-wait-for-publish)): the title (1), the folder
  (1), the page order (1), each page added or removed (1), and for each page in both its title (1) and default view
  (1), each card, group, link, frame, resource, and note added, removed, or changed (by id; a link's change includes
  its connection fields), and each label added, removed, or changed (by project and key). Guards, and so a
  reference's signature alone, are not counted. Zero means the copy is deleted, so a board never
  has "0 unpublished changes". **`baseChanged`** says the published file is no longer the copy's base (a pull changed
  it): Publish will ask.
- **Moving a published board to another folder**, or renaming a folder it is in, is an unpublished change.
- **External edits.** Each load or save records the bytes' revision (their SHA-256); `BoardDirs::change` reports
  whether a working file is unchanged, changed by someone else, or gone, so the app can reload a board an agent or
  teammate wrote, or ask when it has unsaved edits of its own. The watcher reports a change to `x.changes.json` under
  the published id `.unfold/boards/x.json`.

### Differences from the proof of concept

- **The format** is `unfold-board` v1: a board holds pages, a card's `state` became an optional `view`, `blocks`
  became `labels`, labels and guards are lists keyed by project and key, and cards can come from other projects.
  Old formats are not read. Signatures hash bytes, not decoded text (equal for the playgrounds, which are ASCII).
- **Guards an edit sets win.** The PoC let the previous version's guards override the new ones; Unfold only
  restores guards the new version dropped, so the Accept repair can save a fresh guard.
- **Health** separates held references (`review`) from broken groups, adds cards that aren't connected and broken
  links, and every item names its repairs; group and label messages say "label" instead of "block description".
- **Files** live under `.unfold/` with a `.json` extension, publishing never overwrites even when two writers race,
  and edits to a published board wait in a local copy until the next Publish.

## Mermaid export

`mermaid::page`, `mermaid::entrance`, and `mermaid::flow` export one page of a board, an entrance with its
handlers, and one Flow as Mermaid flowchart text ([architecture §10.5](architecture.md#105-boards)).

- **Cards.** An entrance card is a stadium node with its label (or method and trigger); a Code-view
  or Diagram-view card is a box with its qualified name, location, and description, and so is a type
  declaration (`interface`, `type_alias`, `enum`) in Flow view, which has no paths; a Flow-view card
  is a subgraph with its Flow, and its groups as nested subgraphs. A card from a project that isn't
  connected is a "From <name>, not connected" placeholder. An entrance export draws the entrance card
  and a Flow card for each distinct resolved target.
- **Resource cards** are nodes labeled with their kind and name, then a line per column (its name,
  written type, and marks: `PK`, `FK → <references>`, `unique`, `optional`) and their description. A
  table is a cylinder, a queue a parallelogram, a cache the other parallelogram, storage a trapezoid,
  an outside service a hexagon, and an actor a double circle.
- **Frames** are subgraphs (`f0`, `f1`, …) titled with their title and description, with
  `direction LR` or `direction TB` from their `direction`, around their members in member order and
  nested as the frames are; every other item is drawn at the top level. A collapsed frame is drawn
  expanded.
- **Notes** are `%%` comments where they sit ("Note on create-order: …", or "Note: …" when free).
- **Shapes.** Start and End are stadiums, conditions rhombi, loops hexagons, calls subroutines,
  returns and transfers circles, throws flags, everything else boxes. Opaque nodes get the `opaque`
  class.
- **Arrows.** Inside a card, every route with its label (`on error` dashed). Between cards, solid
  arrows for calls and constructions and dashed arrows for imports and passed references, labeled
  with the edge kind and leaving from the Flow node that shows the occurrence. A Diagram-view card
  draws one arrow to each card it calls, as the app does: labeled with its first call's or
  construction's kind (else its first edge's), or "N calls" for several, and dashed only when every
  edge is; an entrance points at
  each handler (`handler`), with a dashed arrow to a placeholder for a target not on the page.
  Arrows from the graph join only cards of one project; links drawn on the page are dashed arrows with
  their label. A connection is labeled with its kind's word (`HTTP`, `RPC`, `queue`, `event`,
  `writes`, `reads`, `runtime call`, `relates`), a relationship's cardinality in words and its
  columns, its channel, and its label, and drawn with its kind's line: solid for `http`, `rpc`, and
  `writes`, thick for `queue`, dotted for `event`, `calls`, and `other`, and a plain line for
  `relates`; a `reads` arrow points from the resource back at the code. A connection with a step
  leaves from the Flow node that shows it; one that ends at a frame points at its subgraph.
- **Labels are the app's** ([U-27](decisions-and-tradeoffs.md#u-27-the-reader-keeps-the-v71-rules)):
  the authored label or the fallback name, never exact source, plus a line per listed callback. An
  outdated description, label, or frame description is marked `(description outdated)`, and an outdated
  resource card or connection `(outdated)`; references held for review,
  missing references, and broken groups are drawn as "needs review" placeholders and never applied to
  changed code. Health items and Flow warnings become `%%` comments.
- **Escaping.** The characters `&`, `<`, `>`, `"`, `#`, `|`, `\`, the backtick, `[`, `]`, `{`, and `}`
  become entity codes (`#34;`), and line breaks become `<br/>`.
- **Deterministic.** The text depends only on the page, the graphs, and the verified sources.

`tests/mermaid_reference.rs` compares every entrance diagram of both playgrounds with the PoC's
structurally: comment lines are dropped; node ids and shapes, subgraphs, classes, and every arrow
must be equal line for line; node labels are compared wherever the PoC did not show source text
(entrances, Code-view cards, placeholders, `End`). `tests/sample_board.rs` compares the sample
board's diagram with the PoC's exactly; `unfold-workspace`'s `tests/commands/claims.rs` saves
`testdata/boards/order-system.json`, which has every frame, resource, connection, and note shape, on a compiled
`ts-orders` (its typed records ground the claims) and snapshots the board and its diagram. Snapshots of every Unfold diagram (`testdata/boards/*.mmd`,
`testdata/mermaid/*.json`) are parsed by the real Mermaid parser in `tools/oracles/check-mermaid.mjs`.

### Differences from the proof of concept

- **Labels** instead of exact source (U-27).
- **No handler numbers**: entrance arrows read `handler`, not `handler 1` (no step numbers in any
  view), and a placeholder for an unresolved target shows its display reason.
