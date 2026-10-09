# Reader

How Unfold turns the stored graph into what people read: derived views (owners, the definition
catalog, activation closures, applications, relations), the Code and Flow views' render models,
boards, and Mermaid export. This document is authoritative for the rules `unfold-reader` implements; [architecture §10](architecture.md#10-the-reader)
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
- Ownership never crosses files. Because blocks are stored in preorder, a block's owner is its
  parent when the parent owns it and otherwise the parent's owner, so one pass over a file computes
  every owner.

### Definition catalog

The catalog holds every function-like or `class` block that has a name, or that is the target of a
resolved edge or a resolved entrance target. Untargeted anonymous callbacks stay out of it; they
remain readable inside their owners.

| Field | Rule |
| --- | --- |
| Label | The block's name. An unnamed callable that is (inside) an argument of a `call` or `new`, before any enclosing definition or file, is `callback to <callee>`, where the callee is the written text of that occurrence's first non-reference edge (`call` when there is none). Any other unnamed callable is `anonymous function`; an unnamed class is `class` |
| Qualified name | The names of enclosing classes and callables, and of a `variable` whose initializer contains the definition, outermost first, then the label, joined with `.`: `OrderService.place`, `handlers.onCreate`, `handler.anonymous function` |

Getters and setters keep their own identities. The store keeps the catalog as the `definitions`
table and indexes the label and qualified name for search.

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
- **Outgoing** occurrences are the edges it owns, in source order, grouped as already on the board
  (the caller supplies the board's keys), resolved and openable, and external or unresolved. Each
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

The Code view shows the exact verified slice of a block (a card) or a file (a source tab, through
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
| `dedent` | The indentation common to every nonblank line after the first, which the view may hide |
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
`return`, `throw`, `break`, `continue`, `import`, `class`, `with`, `match`) with no role or a `body`,
`then`, `else`, `catch`, or `finally` role. Its `parent` is the index of the nearest enclosing item.

- **Folds.** A compound item (`if`, loops, `switch`, `try`, `catch`, callables, classes, Python
  `with` and `match`) that spans several lines folds. The fold's `headerEnd` is where the hidden
  part begins: in Python, the end of the written header line, which stays visible; otherwise the
  start of the body slot (`then` for `if`), or the end of the first line when there is none.
  `braces` says the folded row shows the body as `{ … }`. The `summary` reads `N lines`, plus
  `· returns` and `· throws` when the block contains a `return` or `throw` outside nested callables.
- **Default folds.** In a card longer than 40 lines, callbacks (nested callables and classes) and
  an `if` that heads an `else if` chain of three or more checks (`chain`) start folded; the `else
  if` links of a chain never do. A source tab starts with nothing folded. Folds are view state; the
  reader never folds a card by itself.
- **Groups.** An item is `groupable` when it is a statement (not a nested callable or class) in a
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

### Differences from the proof of concept

- **Every occurrence in the slice is a link.** A PoC TypeScript card linked only the occurrences its
  definition owned, so calls inside an unfolded nested callback were plain text; Python cards also
  linked header calls. Unfold links every occurrence the slice shows and says who owns it.
- **Import links** cover the specifier instead of the whole import statement.
- **Lines** also end at U+2028 and U+2029, matching stored line numbers, and a final terminator adds
  no empty line.
- **Highlighting** runs on the whole file instead of the slice.
- **Source tabs** are new: their fold policy starts nothing folded.

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

A node's type comes from the compiled structure; its label is the board's authored label for that
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
[U-22](decisions-and-tradeoffs.md#u-22-a-fresh-board-format)): structural references, positions,
each card's saved view and fold, groups, descriptions, Flow labels, and guards. It holds neither
copied source nor compiled arrows. Its JSON Schema is
[`schemas/unfold-board.schema.json`](schemas/unfold-board.schema.json) (the crate's copy,
`crates/unfold-reader/schema/`, is the source; `tools/oracles/check-board-schema.mjs` checks the
two are identical and validates the sample board with an independent validator).

### Format

| Field | Rule |
| --- | --- |
| `format`, `version` | `"unfold-board"` and `1`. Anything else is refused |
| `title` | Not blank |
| `defaultView` | `"code"` or `"flow"`: the view of cards without their own |
| `cards[]` | `id` (not blank, unique among cards and groups); `ref`, exactly one of `block` (a key with `#`) or `entrance` (an `entrance:` key); `at`, two finite numbers; optional `view`; `folded`, written only when true; optional `description` |
| `groups[]` | `id`, `card` (an existing card's id), `title`, `members` (one or more distinct block keys), `collapsed`, optional `description` |
| `labels` | Flow-node labels by block key (optional on input, always written) |
| `guards` | By referenced key: `kind` (a block kind or `entrance:<kind>`), `owner` (the owner's key; empty for an entrance), `signature` (optional on input, always written) |

A **description** or **label** has `text` (not blank), `author` (`person:<username>` or
`agent:<name>`), and `signature`: `sha256:` and 64 lowercase hex digits, or `""` (or absent) for
text not yet signed, which the next save signs from verified source.

- **Strict parsing.** Unknown fields, duplicate JSON keys, wrong types, and malformed values are
  refused with the JSON pointer of the offending value ("invalid board at /cards/2/ref: must have
  exactly one of block or entrance"). Parsing checks shape only; whether references exist is
  checked against the graph.
- **Deterministic serialization.** Object keys sorted by UTF-16 code units, cards and groups sorted
  by id, two-space indentation, numbers and string escapes exactly as JavaScript's
  `JSON.stringify(value, null, 2)` writes them, and a final newline. Loading and saving a board
  written this way reproduces it byte for byte; the sample board does.

### Signatures

A signature is the SHA-256 of the exact selected source bytes, computed only from bytes that
verified against the facts' hash, so text can never be signed against source the graph has not
seen:

| What | Selected bytes |
| --- | --- |
| A card (and its guard) | The block's range, or an entrance's evidence range (in its source file or configuration input) |
| A group | From the start of its first member to the end of its last |
| A label | The labeled block's range |

When a signature no longer matches the verified bytes (or the bytes do not verify), the description
or label is **outdated**.

### Guards and reconnecting

Every save records a guard for each referenced key (cards, group members, labels): its kind, owner,
and signature. Guards for keys no longer referenced are dropped. After a graph update, references
reconnect by key:

- A **surviving key** reconnects. If its text no longer matches, the text is outdated.
- A **missing key** stays on the board as a placeholder, reported as missing.
- A guarded key whose **kind or owner changed**, or a **position-based** key (one with an ordinal
  step such as `if[0]`) whose source changed, is **held for review** instead of attaching its text
  to different code. Held references cannot be described, and Mermaid draws them as "needs review".
- **Renames and moves** leave the old key missing; the Reconnect repair points it at the new key.

### Groups

A group is a run of consecutive sibling statements with the same parent and the same body role
(`body`, `then`, `else`, `catch`, `finally`), inside its block card and not the card's block
itself. Groups in one card never overlap or nest. `group_problem` names the first rule a group
breaks.

### Health and repairs

| Kind | When | Repairs offered |
| --- | --- | --- |
| `missing` | A card's or label's key is not in the graph | Reconnect, remove the card or label |
| `outdated` | A card, group, or label signature no longer matches | Re-sign (keep the text, accept the code), remove the description or label |
| `brokenGroup` | A group breaks the group rules, or overlaps or nests with another | Ungroup |
| `review` | A guarded reference (a card, a group member, a label) is held for review | Accept (save a fresh guard for the current code), Reconnect, remove or ungroup |

`apply_repair` performs a repair. Saving (`prepare_save`) restores guards an edit dropped (from the
version being replaced), signs unsigned text, saves guards, and refuses a board that introduces a
missing reference, a broken group, or a held reference that the previous version did not already
have with the same reference; problems carried over unchanged are kept, so a board with a
placeholder can still be moved or retitled.

### Outline

The Boards view's outline lists the cards in follow order: roots are cards no other card links to
(entrance cards first, then left to right and top to bottom), each followed depth first through its
links (an entrance to its resolved targets' cards in registration order; a block card to the cards
of the resolved targets of the occurrences it owns, in source order), indented by call depth. It
also lists the Flow nodes, in that order, that still show a fallback name ("N Flow nodes still show
code names").

### Files

| Board | Path |
| --- | --- |
| Draft | `.unfold/local/drafts/<id>.json` (the `local/` folder ignores itself for Git) |
| Published | `.unfold/boards/<slug-of-title>.json` |

- **Names** are plain stems (letters, digits, `.`, `_`, `-`, starting with a letter or digit). The
  slug of a title drops accents and turns every other run of non-alphanumeric characters into `-`
  (`Order placement` → `order-placement`, `Café Crème` → `cafe-creme`; nothing left → `board`).
- **Saving** replaces the file atomically: a temporary file beside it is written, synced, and
  renamed over it.
- **Publish** writes the published file without ever overwriting one (the synced temporary file is
  hard-linked to the name, which fails if it exists), then removes the draft. A collision reports
  the existing path and a free name (`order-placement-2`) so the UI can let the person edit it.
- **Move back to drafts**, **Rename** (never overwriting), and **Delete** are file operations;
  board directories must be real directories, not links.
- **External edits.** Each load or save records the bytes' revision (their SHA-256); `files::change`
  reports whether a file is unchanged, changed by someone else, or gone, so the app can reload a board
  an agent or teammate wrote, or ask when it has unsaved edits of its own.

### Differences from the proof of concept

- **The format** is `unfold-board` v1: a card's `state` became an optional `view`, `blocks` became
  `labels`, and old formats are not read. Signatures hash bytes, not decoded text (equal for the
  playgrounds, which are ASCII).
- **Guards an edit sets win.** The PoC let the previous version's guards override the new ones;
  Unfold only restores guards the new version dropped, so the Accept repair can save a fresh guard.
- **Health** separates held references (`review`) from broken groups, and every item names its
  repairs; group and label messages say "label" instead of "block description".
- **Files** live under `.unfold/` with a `.json` extension, and publishing never overwrites even when
  two writers race.

## Mermaid export

`mermaid::board`, `mermaid::entrance`, and `mermaid::flow` export a board, an entrance with its
handlers, and one Flow as Mermaid flowchart text ([architecture §10.5](architecture.md#105-boards)).

- **Cards.** An entrance card is a stadium node with its label (or method and trigger); a Code-view
  card is a box with its qualified name, location, and description; a Flow-view card is a subgraph
  with its Flow, and its groups as nested subgraphs. An entrance export draws the entrance card and a
  Flow card for each distinct resolved target.
- **Shapes.** Start and End are stadiums, conditions rhombi, loops hexagons, calls subroutines,
  returns and transfers circles, throws flags, everything else boxes. Opaque nodes get the `opaque`
  class.
- **Arrows.** Inside a card, every route with its label (`on error` dashed). Between cards, solid
  arrows for calls and constructions and dashed arrows for imports and passed references, labeled
  with the edge kind and leaving from the Flow node that shows the occurrence; an entrance points at
  each handler (`handler`), with a dashed arrow to a placeholder for a target not on the board.
- **Labels are the app's** ([U-27](decisions-and-tradeoffs.md#u-27-the-reader-keeps-the-v71-rules)):
  the authored label or the fallback name, never exact source, plus a line per listed callback. An
  outdated description or label is marked `(description outdated)`; references held for review,
  missing references, and broken groups are drawn as "needs review" placeholders and never applied to
  changed code. Health items and Flow warnings become `%%` comments.
- **Escaping.** The characters `&`, `<`, `>`, `"`, `#`, `|`, `\`, the backtick, `[`, `]`, `{`, and `}`
  become entity codes (`#34;`), and line breaks become `<br/>`.
- **Deterministic.** The text depends only on the board, the graph, and the verified sources.

`tests/mermaid_reference.rs` compares every entrance diagram of both playgrounds with the PoC's
structurally: comment lines are dropped; node ids and shapes, subgraphs, classes, and every arrow
must be equal line for line; node labels are compared wherever the PoC did not show source text
(entrances, Code-view cards, placeholders, `End`). `tests/sample_board.rs` compares the sample
board's diagram with the PoC's exactly. Snapshots of every Unfold diagram (`testdata/boards/*.mmd`,
`testdata/mermaid/*.json`) are parsed by the real Mermaid parser in `tools/oracles/check-mermaid.mjs`.

### Differences from the proof of concept

- **Labels** instead of exact source (U-27).
- **No handler numbers**: entrance arrows read `handler`, not `handler 1` (no step numbers in any
  view), and a placeholder for an unresolved target shows its display reason.
