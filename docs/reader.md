# Reader and boards

Read this for what people and agents see of a compiled graph: owners and relations, the drawn flow, agent text, the
code panel's model, contracts, and boards with their signatures, guards, health, and files. Start at
[reader lib.rs](../crates/unfold-reader/src/lib.rs). Exact board shapes belong to
[board/model.rs](../crates/unfold-reader/src/board/model.rs) and the
[board JSON Schema](schemas/unfold-board.schema.json); command shapes belong to [IPC](ipc.md).

The reader computes everything from stored records and verified source. It never parses source and never reads key
text: kinds, parents, and files come from the records, and only `unfold-core`'s key module takes a key apart.

## Derived views

| View | Rule | Code |
| --- | --- | --- |
| Tree | One file's nodes with their structure; blocks found by key across files | `tree.rs` |
| Owner | A block belongs to the nearest definition above it (`unfold_core::owner`, the one implementation) | `tree.rs`, `index.rs` |
| Catalog | Every module, class, function (inline ones too), and type can be a card. Labels: a definition's name, a module's path, an inline function's or class's property name (`onSuccess`), else `callback to <callee>`, else `anonymous function` or `anonymous class`. Qualified names join enclosing named definitions with `.` (`OrderService.place`) | `catalog.rs` |
| Closure | What a definition reaches: its `calls`, `passes`, and `imports` links, plus the classes and inline functions written in it. A lower bound, never a runtime order | `closure.rs` |
| Reached from | The entrances whose closures hold a definition, seeded from `code` targets that are `found` or `stale`. An app's own start is left out: it reaches nearly everything | `reader_index.rs` |
| Relations | Callers (the `calls` and `passes` links that reach a definition, a page at a time from the store's reverse index) and a definition's outgoing calls in source order | `relations.rs` |

The catalog is computed per file after each commit and stored for search. Graph-wide answers (owners across files,
closures, Reached from) come from one `ReaderIndex` per graph version, shared through `ReaderCache` until the next
commit; closures are cached per seed. Library and unknown calls stay visible in every view.

## Verified source

[source.rs](../crates/unfold-reader/src/source.rs): before any stored range is shown or signed, the file's bytes are
hashed and compared with the hash its facts came from. Only matching bytes are decoded (strict UTF-8) and sliced.
Otherwise the view says the source changed or is missing, without text: old ranges are never applied to new bytes.
Stored ranges are UTF-8 bytes; everything rendered uses UTF-16 offsets.

Each project keeps a `SourceCache`: the sources the drawings verified most recently, by path and hash, within 32 MiB
of text. While a file changes on disk and the compile hasn't caught up, or while it doesn't parse (its last good
facts keep the hash they came from), a drawing's words come from the text kept for the facts' hash, which is exactly
the text the facts came from. Nothing is kept across a restart: a file that changed while the app was closed has no
kept text until the reopen check recompiles it.

## The drawn flow

[drawing/](../crates/unfold-reader/src/drawing/mod.rs) holds the one drawing function. `draw` turns four inputs into a
definition's `DrawnFlow`, the same way for every language:

- the definition's tree and links;
- the page's overlay: names, combined steps, and proposed steps, applied page-wide by key (inside insets too);
- the card's open paths: calls and callbacks opened in place, up to 3 levels;
- whether Show all calls is on.

Routes aren't stored; they are drawn each time. The canvas only lays the flow out, and agent text prints the same
flow. Nothing else decides what is drawn.

| Part | Carries |
| --- | --- |
| Header | Key, kind, form, label, the page's name, signature, the doc as a description from code, path and line, its file's `state` (`current`, `changed` since the last compile, or `doesNotParse`), `async`, generator, what runs when it is defined (decorators, class bases, Python defaults), imports, and its overlay actions |
| Steps | In the order they run: call boxes, thin rows, decision/loop/try/with frames whose parts are lanes, exits, combined steps, proposed steps. Each has an id and a route state |
| Call box | Key, written callee, the page's name, form (`call`/`new`), where it goes, `binds`, AWAIT, SOMETIMES, role, the call it feeds, the library and unknown calls written inside it, its callbacks (each with the property it is written as and its signature), its inset when opened, its actions |
| Frame | Key, form, the page's name, its header as written, the boxes written in the header and its unknown calls outside them (`headerUnknown`), `binds`, its lanes, its actions |
| Lane | Key, form, the label the code panel's path uses (`case "refund"`, `except PaymentDeclined as error`), its lines, its `test` (text as written, the boxes written in it, and its unknown calls outside them), the classes a handler catches, `fallsThrough`, its combinable steps, its body. An `except` or `catch` lane's test is its clause, keyword included (`except errors()`) |
| Exit | Form, the call that is its value or its value as written (`RETURN mark_failed`, `RETURN order`), a `break`/`continue` target |
| Combined step | The page's id, title, members, collapsed, the blocks it `holds` while collapsed, its lines, counts of conditions, loops, library and unknown calls, and its project calls with their targets (arrows leave a collapsed step from its box) |
| Gap tick | On the route into a step, or after a body's last step: how many lines, which lines, and the hidden control blocks that start in it |
| Routes | `from` an id `to` a step, End, or the caller, `through` any `finally` lanes, and what a `break`/`continue` leaves (nothing when a `finally` that always exits ends it there) |

### What is drawn

1. **Project calls are boxes, in the order they run,** with `→ binds` and a chevron to open in place. A project call
   written inside another call is its own box, drawn before the call it feeds; so is a ternary or comprehension
   written inside a call or an exit. A named library or unknown call, or one that takes a callback, is a box too.
2. **Thin rows.** Consecutive library and unknown calls of one body fold into one row that lists them and counts its
   unknowns; each unknown keeps its `?`.
3. **Builtin calls are hidden.** With Show all calls they join the thin rows.
4. **Callbacks** are rows under the call they are passed to, labeled by the property they are written as
   (`onSuccess`) and their signature, and open in place like a call.
5. **Frames.** A decision draws a diamond per test, its parts as lanes; a loop is a frame titled by its header; a try
   is TRY with EXCEPT (or CATCH) sections titled by their clause, then ELSE and FINALLY; a with is a frame titled by
   its header. Project calls written in a test, a header, or a handler's clause are drawn on it as chips (a clause's
   in its section title), not as steps, each with the library and unknown calls written inside it. The test's or
   header's other unknown calls are listed after them, each with its `?`; its library and builtin calls stay its
   written text, with Show all calls too.
6. **A control block with nothing drawn inside is hidden,** unless the overlay puts something in it (a proposal, a
   name, or a combined step on a block written in it), which draws it as a frame. A hidden block's code stays in the
   code panel and in any combined step that spans it, and its gap tick names it (`⋯ 3 lines · for loop`). A proposal
   after it sits after that tick.
7. **Gap ticks** count the lines between steps that no drawn step's statement covers, skipping blank and comment
   lines. Comments are the language's highlighter's comment tokens, so a comment mark inside a string is code, and a
   line is one of the file's lines as its language ends them. A tick sits on a route and takes no layout of its own.
8. **Exits** are terminals labeled by their value, with the library and unknown calls written inside it.

The words (tests, headers, values, signatures, docs) and gap ticks come from the text the facts came from: the file's
own, or the text the [source cache](#verified-source) kept while the file changed or doesn't parse, and the header's
`state` says which. With no such text, its steps have no words and no gap ticks.

### Routes

| Block | Routes |
| --- | --- |
| A body or part | Each step goes `on` to the next; the last goes wherever the body goes |
| `if`, ternary | Each test in order: `yes` into its part, `no` to the next test, then `else`, then after the decision |
| `switch`, `match` | `case` from the header to each case in order; a case ends after the decision, or falls into the next. `default` when no case matches |
| Loop | `each` into the body (through a comprehension's filter); `next` back to the header from the body's end and `continue`; `break` after the loop; `done` to `else`, then after |
| `try` | Dashed `onError` routes from the body to each handler, a possibility rather than a claim. The body's end goes to `else`, then `finally`, then after |
| `with` | Header, body, then after |
| Exits | `return` to End; `raise` to the first handler proven to catch it ([Contracts](#contracts)) of the nearest enclosing `try` that has one, else the caller; `break`/`continue` to their target; each through every `finally` between. A `finally` that never reaches its own end (it always returns, raises, breaks, or continues) replaces the transfer: an exit through it ends in it, and its own exit goes on |
| Combined step | Wherever its blocks went: a collapsed step takes over the routes that leave it, so combining never makes a dead end |

Every step and lane has a **route state**: `reached`; `unreachable`, drawn dimmed and marked "never runs" (code after
an early `return`, or after an always-exiting `finally`); or `truncated`, drawn with a dashed "may continue" route
(nesting past the depth limit, a TypeScript labeled plain block). A truncated step, part, or definition keeps its
other routes and may continue to where its end goes: the route leaves the step, the part, or the definition's start
(into End). Both are facts about the code: validation never checks routes, so legal code never fails a compile.

## Overlay rules

[overlay/](../crates/unfold-reader/src/overlay/mod.rs) holds the only copy of these rules. The drawing reports what
each block allows (its `actions`), and saving a board checks them again.

| Overlay | Rule |
| --- | --- |
| Name | Any block. A name replaces the code name, which stays beside it. Naming a library or unknown call lifts it out of its thin row; an unknown call keeps its `?` |
| Combine | Consecutive steps of one body, or exactly one whole control block. A page's combined steps never overlap or nest. A combined step covers its blocks' statements and the code between them: a step written between its blocks later joins it, is drawn inside it, and sends it to review |
| Propose | After a step of a body, or at the start of a definition's or part's body |
| Open in place | A project call or a callback; a path of 1 to 3 such blocks, each inside what the previous one opens; at most 64 paths per card |

A **body** is a module's, class's, or function's code, or one part's. Its **steps** are the calls, exits, control
blocks, and definitions that run in it, except what is written in a part's test. Decorators, class bases, and Python
defaults are steps of the body they are written in.

## Agent text

[drawing/text.rs](../crates/unfold-reader/src/drawing/text.rs) prints what agents read, so goldens can check it and the
MCP server never lays out flows itself:

- `code_text` prints a definition's compiled tree, for `read_code`;
- `flow_text` prints a drawn flow, for `shape_card` results and `read_page`.

Every line ends with the full keys of its blocks after `key=`, the calls listed on a box, a chip, an exit's value,
or in a thin row included, in the order they are printed. Each result also carries a label → key map, so the
server turns an agent's short label (`try[0]/except[0]`, `call:slack.post`) into a key by lookup, never by joining or
splitting key text. Marks: `▸` a project call, `□` another box, `·` a thin row, `?` an unknown call, `+` the calls listed inside a call, `⊞`/`⊟` a
collapsed or open combined step, `⋯` a gap tick, `↳` a callback (`↳ callback onSuccess () =>`: the property it is written as, then its
signature), `+ PROPOSED` a proposal, `· never runs` and
`· truncated` for route states (a truncated definition's on its first line). A handler's line is its clause, then
the clause's project calls and unknown calls: `ON ERROR → except errors() · ▸ errors`. An exit says where it goes, after a
`finally →` for each `finally` it passes: `End`, `the caller`, the handler that catches it (`RAISE ValueError →
except ValueError`), `after the loop`, `next`, or `finally` when a `finally` that always exits ends it. From [ts-orders.flows.txt](../testdata/goldens/ts-orders.flows.txt), keys
shortened:

```text
( createOrder )   key=src/routes/orders.ts#createOrder
  ⋯ 1 line
  IF req.header("prefer") === "respond-async"   key=…#createOrder/if[0] …#createOrder/if[0]/then
      · ordersQueue.add → job, res.status().json ?   2 calls, 1 unknown   key=…
      RETURN → End   key=…#createOrder/if[0]/then/return[0]
  ▸ orderService.place → order   key=…#createOrder/call:orderService.place
  ▸ slack.post   key=…#createOrder/call:slack.post
  · res.status().json ?   key=…#createOrder/call:res.status().json
( End )
```

A flow whose file isn't the one its facts came from says so on its first line, in the code panel's words:
`( place_order ) · Changed since the last compile`, or `· Doesn't parse`; `code_text` says the same after the
definition's lines. `code_text` aligns its labels up to 40 characters, and a longer one is followed by one space.

## The code panel's model

[code_view/](../crates/unfold-reader/src/code_view/mod.rs) answers what the panel beside the canvas shows for one
selection. A request names the **selected** target and, when the path widens the view, a target to **show**
instead; the selection stays marked inside the wider view.

| Target | Used for |
| --- | --- |
| `Block` | Any block: a module, definition, control block, part, call, or exit |
| `Lines` | A combined step, a thin row, a gap tick, and the lines a resource card or connection cites (evidence files included), with an optional mark and a combined step's title |
| `Field` | A written member of a class or type card, shown as its line |
| `File` | A whole file: a source file, an evidence file as plain text, or any file in the folder |
| `Start` | An app or entrance key: its lines in `unfold.toml` |

What a block shows:

| Selected | Shows | Marked |
| --- | --- | --- |
| A definition | From its first decorator to its last line | nothing |
| A module | The whole file | nothing |
| A control block | Its header to the end of its last part | nothing |
| A part | Its whole control block: the `switch` for a `case`, the `try` for an `except` or `finally`, the `if` with every `elif`/`else`, the loop for its body or `else` | the part's opening line and body |
| A call | The statement it is written in (its own `stmt` or the nearest one above it); a call in a test or header shows that line | the call's expression |
| An exit | Its statement | nothing |
| A combined step, thin row, gap tick | Their lines | nothing |
| An app or entrance | Its lines in `unfold.toml` | nothing |
| A citation | Its lines | the cited lines |

Library, builtin, and unknown calls open their call site; nothing opens a callee Unfold has no source for.

The model carries the file and its **state**, the shown lines (dedented to the view, real line numbers,
highlighted by the language's highlighter, each with the block hovering it outlines and the calls named on it), the
mark, the **path** (the file, then each block above the selection, then the selection, each with a label and key;
`body` parts are left out), and each shown definition's first line for `⊕`. Path labels: a definition by name, a
control block by its header or keyword (`for item in order["items"]`, `switch (command)`, `try`), a part by the text
that opens it, a call by its written callee, an exit by its keyword, a combined step by `⊞` and its title, a gap tick
by its lines.

| State | The panel shows |
| --- | --- |
| `current` | The view, cropped and marked |
| `changed` | The whole file as it is now: "Changed since the last compile · showing the whole file" |
| `doesNotParse` | The whole file as it is now: "Doesn't parse · showing the whole file". `unfold.toml` gets it too when it doesn't parse |
| `notAnalyzed` | The whole file: it is outside the graph |
| `notFound` | Nothing: the target no longer resolves, or its file is gone |

**The panel never marks lines from ranges that may have moved.** Only `current` views are cropped, marked, or carry
blocks; every other state shows whole files with nothing marked.

## Contracts

[contracts/](../crates/unfold-reader/src/contracts/mod.rs) shows written facts only: a type is what the code wrote, a
pairing is what the engine proved, and a missing annotation shows missing.

- **A call's contract**, by its block key: what it sends (each argument beside the parameter it fills, or why it
  fills none; unfilled parameters only when the pairings prove them), what returns (the written return type, or the
  class a construction makes) and where the value lands (`binds`, an exit, an outer call, or nowhere), and what the
  callee raises, with the caller's enclosing handler that catches each. A TypeScript `catch` or a bare `except`
  catches everything; otherwise a handler catches a raise whose class derives from a class its `types` links name:
  through project `extends` chains (at most 8 levels), and through the bases typeshed writes for Python's builtin
  exception classes, so `except Exception` catches `ValueError` and `except LookupError` a project class deriving from
  `KeyError`. A library class matches by its module and name; a raise of no proven class is caught only by a handler
  that catches everything. The table of builtin exceptions is generated with the stubs (`cargo xtask
  gen-py-stubs`). The drawing routes each `raise` and `throw` by the same rule, reading the drawn file's classes.
- **A class or type**: its written members, a type alias's aliased type, its use count, and inherited members. These
  follow `extends` only (never `implements`), depth first in written order, each base once, at most 8 levels; a
  library or unknown base is a group without members. Generic members stay as written.
- **Uses** of a class or type: one page at a time from the store's reverse index, each with its owner.

## Boards

A board is an `unfold-board` version 1 JSON file: a title, an optional folder (one name; folders are one level
deep), and pages. A board shows one project. A page holds only authored choices; it never holds copied source or
compiled arrows.

| Page field | Holds |
| --- | --- |
| `defaultView` | `compact` or `expanded`, for cards without their own |
| `cards` | `ref` (`{block}` for a module, definition, or type; `{entrance}` for an app or entrance), `at`, `view`, `description`, and `open` paths |
| `names` | `{key, text, author, signature}` on any block, page-wide |
| `combined` | `{id, title, members, collapsed, description, author, signature}`, page-wide |
| `proposals` | `{id, after \| start, title, reason, author}`: an intended change, never signed, its place guarded |
| `guards` | What each referenced key was when saved |
| `links` | Connections: `kind` is `http`, `queue`, `reads`, `writes`, `calls`, `relates`, or `other` (which needs a label); `step` is a call's block key |
| `frames`, `resources`, `notes` | Boxes around items; tables, queues, caches, storage, outside services, and actors; short plain text |

Ids are unique together within a page; page ids are unique within the board. Text records its author
(`person:<username>` or `agent:<name>`). Parsing is strict and serialization deterministic. Files that break the
format fail to parse like any invalid file.

### Files

[board/files.rs](../crates/unfold-reader/src/board/files.rs) and the workspace's
[board commands](../crates/unfold-workspace/src/commands/boards/mod.rs) own the files:

- Every board is `.unfold/boards/<name>.json`, its id the path in the project. A new board's name is its title's slug
  (`Order placement` → `order-placement`), with `-2`, `-3` when taken; a new board never overwrites a file. A file
  whose name a board file can't have (1 to 100 ASCII letters, digits, `.`, `_`, or `-`) is listed with an error that
  says so.
- Files are written atomically as the board is edited, and shared through git. A new board's file is claimed
  exclusively and then replaced atomically, never hard-linked: FSEvents doesn't report the deletion of a file that
  began as a hard link. A board file is at most 16 MiB: a larger one isn't read, and no board that large is written.
- An open board has a **session**: the version last loaded or saved. A save validates the window's version against
  the graph, signs what is new, and writes it over the file. It is refused (`board_changed`) when the file no longer
  holds the version the window's edits build on (changed, or not a valid board), unless the person chose their version;
  a deleted file is written again. Repairs and folder moves never write over a file that isn't a valid board either.
- An outside change to an open board's file (an agent, an editor, a `git pull`) sends `board_reloaded` to the window:
  a valid board reloads the session; a deleted file, or one that isn't a valid board (with why), leaves it. A window
  with edits of its own asks the person which version to keep; the person's is checked against every version it could
  be built on.
- A board's folder is metadata the folder commands change; a window's save never changes it. Empty folders live in
  `.unfold/local/state.json`.

### Signatures

| Signed | Over |
| --- | --- |
| A card's description | Its block's source, or the source of each block an app's or entrance's `code` names (none for a heading-only app or for code that isn't found, so its description goes to review once the code is found) |
| A name | Its block's source |
| A combined step and its description | The source from its first block's start (a definition's first decorator) to its last block's end |
| A resource card | Its table type's range, then each anchor's bytes |
| A connection, its label and description | Its step's range, then each anchor's bytes |
| A frame's description | Its member cards' signatures |

Signatures come only from verified bytes; text can never be signed against source the graph hasn't seen. Notes,
proposals, and open paths are never signed. A save never marks old text reviewed just because new bytes appeared.

### Guards and reconnecting

A guard remembers each referenced key's kind (one of the 11 node kinds, or `entrance`), owner, and signature. An app
or entrance guard hashes its `code` keys. After an update, keys are looked up again:

- a surviving key reconnects; changed source makes its text **outdated**;
- a missing key stays on the page as **missing**;
- a key whose kind or owner changed, or a position-based key whose code changed, is **held for review**.

Nothing is reattached by a similar name. A rename leaves the old key missing, and Reconnect moves the reference to the
new key, carrying what is keyed inside it (cards, names, combined steps, proposals, open paths, connection steps,
resource anchors) to the rebased keys that exist; a combined step or open path moves whole or not at all, and the
rest stays missing at its old key. For a missing app or entrance, health offers the one with the same `code`.
An evidence-file anchor is gone when its file isn't an evidence file of the project or no longer has those lines; its
bytes verify against the input's recorded hash, and Reconnect points it at new lines or a key.

### Grounding

[board/claims.rs](../crates/unfold-reader/src/board/claims.rs) checks every claim that is new or changed (in what it
says or cites, not its position), whoever wrote it:

- a table backed by a type marks only that type's fields (inherited ones included), and `relates` joins only columns
  its tables have;
- a connection's `step` is a call, and a `calls` connection starts at an unknown call.

A claim an agent wrote must also cite what it names; people's claims need no citations:

- a resource card other than an actor has an anchor; an `http`, `queue`, or `calls` connection has an anchor, and a
  `reads` or `writes` connection a step or an anchor;
- a resource card's name, and the columns a table lists itself, appear in the cited bytes;
- a connection's channel appears in what it cites, unless its `to` end is an entrance, whose `on` the save fills in;
- a carried type is used inside the step's function or an anchor's block.

A refusal (`board_problem`) says what failed and what to do instead, and records nothing. (An agent's `connect` is
also refused when it repeats a compiled call; the window checks that, [agents in the window](ui-shell.md#agents-in-the-window).)

### Health and repairs

[board/check.rs](../crates/unfold-reader/src/board/check.rs) reports health per page, each item with the repairs it
allows:

| Health | About |
| --- | --- |
| Missing | A card's, name's, or proposal's key, or a resource card's anchor or type, isn't in the graph |
| Outdated | The code under a description, name, combined step, resource card, or connection changed since it was signed, or a step written between a combined step's blocks joined it |
| Review | A guarded reference changed kind or owner, or a position-based one changed code |
| Broken combined step | A combined step no longer follows the overlay rules: one of its blocks is gone, they are out of written order or in different bodies, or it overlaps another combined step |
| Broken link | A connection's end, step, carried type, or anchor is gone |

Repairs are explicit: Reconnect, Resign, Accept, Remove, and Remove description. Resigning a combined step also records
the steps that joined it as its blocks. A save refuses a new missing
reference, broken combined step, held reference, or broken link, and anything that breaks the overlay rules. Health
is cached per board, keyed by the board file's revision, the graph's compile, and a hash of its apps and entrances,
and refreshed after the graph or the entrances change (an `[[app]]` edit compiles nothing); it feeds Home's Health
line, the Problems list, the boards list, and open boards, which windows refresh on `graph_updated` and
`entrances_changed`.

## Checks

Start with the reader's tests: `drawing/tests.rs` (with the [drawn-flow goldens](../testdata/goldens/)),
`code_view/tests.rs`, `overlay/tests.rs`, `board/tests.rs`, and `board/claims_tests.rs`. A board persistence change
also needs the workspace's `tests/commands/boards.rs`; a schema change updates both copies
(`crates/unfold-reader/schema/` and `docs/schemas/`), which `tools/oracles/check-board-schema.mjs` keeps identical. Bless goldens
only after reading the diff ([testing](testing.md)).
