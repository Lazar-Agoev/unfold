# Unfold architecture

Status: V1, with the studio window · 2026-10-09; agents · 2026-10-11

This document describes what we are building in the new `unfold` repository and how it fits together. It replaces the two earlier proposals, `desktop-app-design.md` and `unfold-desktop-from-scratch.md`, which were compared, merged, and removed. A companion document completes it:

- [Decisions and trade-offs](decisions-and-tradeoffs.md) explains *why*: every decision (U-1 to U-63), the options we weighed, and the pros and cons of each.

The proof of concept is the `graph-compiler` repository, whose product was called "Codegraph". It stays as a frozen reference. No code moves over. Its semantics, test data, measurements, and reader design do.

## Summary

- **One desktop app.** You add a repository as a project, press **Compile**, and read the code as a graph. You see entrances (routes, workers, schedules, commands, scripts), the functions they reach, and the calls between them, as cards on a board. There is no separate compiler, CLI, server, or web app.
- **A Rust engine in an Electron app, with a React UI.** TypeScript is parsed with oxc and Python with Ruff's parser. The engine is a native Node addon that runs in its own helper process, the engine host, so a crash in it never closes a window. Chromium renders the UI the same way on every platform.
- **A studio over a workspace.** Every project you connect joins one workspace. Its Home answers how it starts; boards open in tabs, hold pages, and can mix cards from several projects. One **Add** box finds code in every project. Projects nobody is looking at rest and hold no watcher.
- **As many windows as you want.** Every window shows the whole workspace, and all of them share one engine and one graph per project. A board is open in one window at a time.
- **macOS, Windows, and Linux.** V1 ships on all three at once.
- **The graph keeps itself current.** After the first Compile, Unfold watches the folder and updates the graph in the background when files are saved. Only changed files are re-read and re-parsed, and only changed rows are written. V2 adds finer tracking, so that a typical edit re-resolves only its own file.
- **Every repository gets a `.unfold/` folder.** Shared things, the settings and published boards, sit at its top level and can be committed. Machine-only things, the graph cache and your drafts, sit in `.unfold/local/`, which Git ignores on its own.
- **The proof of concept's rules stay.** A call is linked only when there is proof of exactly one target. The graph records what the code declares, never what might happen at runtime. Code is shown only after its file's hash is checked. The same input produces the same graph. Nothing is ever run.
- **The proof of concept becomes the test oracle.** Its outputs are frozen, and Unfold must match them or record why it differs.

## 1. What Unfold is

### 1.1 The idea

A repository starts work through boundaries that a file tree doesn't show. A package script launches a process, `app.post("/orders", …)` registers handlers, a worker consumes a queue, a cron string schedules a job, and a console script names a Python callable. Unfold finds those boundaries without running anything, records what each one reaches, and lets a person read the path on a board.

The result is a graph of graphs. A repository is a graph of definitions connected by calls, constructions, imports, and passed callbacks. Each definition is itself a small structural graph of conditions, loops, and calls. A card shows one definition as its exact source (**Code**) or as a Mermaid-style diagram of its structure (**Flow**). An arrow leaves from the line that makes each call.

Unfold answers three questions, in this order:

| Question | Where in the app | Facts behind it |
| --- | --- | --- |
| What can start this project? | Home's How it starts, and Add's map | Declared entrances, their evidence, and their targets |
| Where is the implementation? | Add, the peek sheet, and cards | Definitions and their source ranges |
| What happens inside, and what does it depend on? | Code and Flow, arrows, and the sidebar's Selected section | Containment, slots, and the relationship recorded at each call |

### 1.2 A typical session

1. Add a repository with ⌘O, or drop it onto the window. It joins the workspace as a project and its Home opens. Nothing is written to the folder yet.
2. Press **Compile**. Unfold reads the TypeScript and Python files, builds the graph, and creates `.unfold/`. Home fills with how the project starts: its applications and their entrances.
3. On Home, trace `POST /orders`. A new draft board opens in a tab, with the entrance card and one card per handler.
4. Click the call to `orderService.place` inside `createOrder`. The `OrderService.place` card opens beside it, with an arrow from the calling line. Switch a card to Flow to see its structure.
5. Write a one-line description in the sidebar, label a Flow node, and group a few lines. The board saves itself as a draft.
6. Press `A` and type `checkout`. Add finds `POST /api/cart/checkout` in the web app, another project; Enter puts it on the page. Draw a link from it to `POST /orders` and label it "calls over HTTP".
7. Edit `order-service.ts` in your editor and save. The card updates within a second, in whichever window shows it.
8. Press **Publish**. The board is written to `.unfold/boards/order-lifecycle.json`, ready to commit. Later edits wait on this machine as unpublished changes until you publish again.

### 1.3 Non-goals

- Editing, refactoring, running, or debugging code, and language-server features such as completion. Unfold opens a file in your editor instead.
- Runtime traces, and control-flow or data-flow graphs.
- Accounts, hosted sharing, and real-time collaboration.
- Links between languages without explicit evidence, such as API contracts. Sharing one graph doesn't make a TypeScript client call a Python route.
- Entrances inferred from names, exports, or functions without callers.

## 2. Guarantees

| Guarantee | What it means | How we keep it |
| --- | --- | --- |
| Precision over recall | A call is `resolved` only with proof of exactly one target in the graph. Otherwise it is `external`, with its module, or `unresolved`, with one of 14 reasons. Nothing ever upgrades a guess. | The shared value policy ([§6.5](#65-the-shared-backend)). 100% precision gates against independent oracles ([§16](#16-testing)) |
| Static claims only | The graph records declarations, containment, and occurrences. An entrance is a declaration, not proof that it runs. Flow draws paths from written structure and marks what it can't route as opaque. | The graph model ([§5.5](#55-what-the-graph-does-not-claim)) and the Flow rules ([§10.4](#104-flow-view)) |
| Verified source | Code is shown only when the file's current bytes match the hash its facts came from. The graph stores ranges and hashes, never code. | Hash, then slice ([§10.6](#106-freshness)) |
| Deterministic | The same effective inputs give the same graph, whatever the thread count, the input order, or whether it was built at once or by many updates. | Canonical-export comparisons and incremental-equals-full property tests ([§16](#16-testing)) |
| Nothing runs | Unfold never executes the indexed code, installs dependencies, runs Git or package scripts, or needs Node or Python installed. The Node runtime inside Electron runs only Unfold's own code. | Execution-sentinel tests, packaged-app checks, and Electron fuses that stop the bundled runtime from running anything else ([§13](#13-security-and-privacy), [§16](#16-testing)) |
| One place in your folder | Unfold writes only inside `.unfold/` at the root of each project, and in its own app data. | Path confinement per project ([§13](#13-security-and-privacy)) |

## 3. Decisions at a glance

Each row links to its full reasoning in [Decisions and trade-offs](decisions-and-tradeoffs.md).

| Area | Decision | IDs |
| --- | --- | --- |
| Product | One desktop app, a reader rather than an editor, started fresh in a new repository | U-1, U-2, U-4 |
| Words | Unfold, Compile, Entrances, Workspace, Project, Board, Page, Draft, Publish, Export, Diagram, Flow, Code | U-3, U-38 |
| Engine | Rust | U-5 |
| Shell | Electron, with the Rust engine as a native addon (napi-rs) | U-6 |
| TypeScript | oxc's parser, semantic analysis, and resolver, with the proof of concept's native evidence rules and no type checker in V1 | U-7, U-8 |
| Python | Ruff's parser, pinned, with Unfold's own binder | U-9 |
| Engine shape | One shared backend behind narrow language adapters; the proof of concept's semantics, with its known defects fixed | U-10, U-11 |
| Offsets | UTF-8 bytes in the engine and UTF-16 at the UI boundary | U-12 |
| Compiling | Compile once, then update in the background. V1 caches per-file facts and relinks in memory; V2 adds early cutoff. | U-13, U-14 |
| Broken files | A file that stops parsing keeps its last good facts, and its code is withheld | U-15 |
| Scope and freshness | Discovery without running Git, scope in settings, hash before showing, cheap checks first | U-16, U-17 |
| Repository footprint | `.unfold/` in every repository: shared `config.toml` and `boards/`, plus a self-ignored `local/` | U-18 |
| Graph storage | SQLite with write-ahead logging in `.unfold/local/cache/`, a cache rather than a contract | U-19, U-21 |
| Identity | Block IDs scoped to their file; structural keys for targets and boards | U-20 |
| Boards | `unfold-board` version 1: a board holds pages, sits in a folder, and can hold cards from other projects, joined by drawn links. Drafts stay local, Publish writes to `.unfold/boards/`, edits to a published board wait as unpublished changes, and Export writes anywhere | U-22, U-38, U-42, U-43, U-44, U-47 |
| Settings | TOML in `.unfold/config.toml`, personal state in `.unfold/local/`, and the preferences, the workspace, and the windows in app data | U-23, U-46 |
| UI | React. Rust computes, TypeScript lays out. IPC types are generated from Rust | U-24, U-25 |
| Workspace | Every connected project in one workspace; commands name their project; projects rest when nothing needs them; branches and worktrees read from Git's files | U-39, U-40, U-41, U-45, U-49 |
| Window | A studio: window tabs, a sidebar with two modes (Home and the open board), a canvas top bar and a floating dock, one Add box for finding and commands, and a peek sheet for whole files | U-26, U-28, U-29, U-30, U-48, U-50, U-51 |
| Several windows | Any number of windows, each showing the whole workspace and sharing one engine; a board is open in one window at a time | U-37 |
| Reader | The v7.1 reader rules carry over unchanged | U-27 |
| Processes | The main process, one engine host that runs the engine, and one sandboxed renderer per window | U-31 |
| Repository | A Cargo workspace of seven engine crates and the addon crate, and a pnpm workspace for the Electron app and the UI | U-32 |
| Testing | A frozen PoC reference with a divergence ledger, independent oracles, and incremental-equals-full tests | U-33, U-34 |
| Platforms | macOS, Windows, and Linux, all released in V1 and tested in CI from the first commit | U-35 |
| Build order | A vertical slice first, then the proof of concept migrated one part per step, behind spikes and gates | U-36 |
| Agents | One MCP server in the main process, on `127.0.0.1`, off until turned on, with no token and fixed tools; agents' edits applied by the window that holds the board, one undo step each; the agent mark; the current view and links; teaching through instructions, a guide, prompts, and descriptions | U-52, U-53, U-54, U-55, U-62, U-63 |
| Pages for agents and people | The Diagram view and type cards; frames; resource cards and typed connections as anchored, grounded claims; evidence files; page images and checks; the board format stays version 1 | U-56, U-57, U-58, U-59, U-60, U-61 |

## 4. System overview

### 4.1 Processes

```mermaid
flowchart LR
    Folder["Your projects<br/>(read only)"]
    subgraph App["Unfold"]
        Main["Main process<br/>windows, menus,<br/>dialogs, updates,<br/>agents' MCP server"]
        subgraph Windows["One renderer per window"]
            UI1["Window<br/>React, React Flow"]
            UI2["Window<br/>React, React Flow"]
        end
        subgraph Host["Engine host"]
            Addon["unfold-node<br/>the engine as a Node addon"]
            Workspace["Projects<br/>a workspace per live project"]
            Watcher["File watcher"]
            Engine["Engine<br/>extract, link, resolve,<br/>detect, validate"]
            Languages["TypeScript: oxc<br/>Python: Ruff parser"]
            Reader["Reader<br/>cards, Flow, relations,<br/>boards, Mermaid"]
        end
    end
    Store[(".unfold/local/cache/<br/>graph.sqlite")]
    Shared[".unfold/config.toml<br/>.unfold/boards/"]
    Drafts[".unfold/local/drafts/"]
    Main -->|creates, hands out ports| Windows
    Main -->|starts, restarts| Host
    UI1 <-->|commands, events| Addon
    UI2 <-->|commands, events| Addon
    Addon <--> Workspace
    Folder --> Watcher
    Watcher --> Workspace
    Workspace --> Engine
    Engine --> Languages
    Folder --> Engine
    Workspace --> Store
    Reader --> Store
    Workspace --> Reader
    Workspace <--> Shared
    Workspace <--> Drafts
```

Unfold is an Electron app with three kinds of process ([U-31](decisions-and-tradeoffs.md#u-31-an-engine-host-and-locked-down-windows)):

- **The main process**, in TypeScript, owns the app's lifecycle: windows, menus, native dialogs, the single-instance lock, and updates. While Preferences › Agents has it on, it also runs the MCP server that agents connect to, which reaches the engine with untrusted requests and boards through the windows that hold them, and it creates the hidden render window that draws page images ([§13](#13-security-and-privacy)). It holds no graph logic and never blocks on the engine: it awaits the engine host's answers like a window.
- **The engine host** is an Electron utility process. It loads `unfold-node`, the Rust engine compiled as a Node-API addon, and holds the workspace's projects, with an open engine for each live one ([§11.2](#112-live-and-resting-projects)). Its JavaScript only routes messages between windows and the addon. If it stops, the main process restarts it and every window reconnects ([§11.11](#1111-background-updates-health-and-the-engine)).
- **Each window** is a sandboxed Chromium renderer running the React UI. It reaches the engine host over its own message port, and it can call only Unfold's commands ([§12](#12-commands-and-events), [§13](#13-security-and-privacy)).

### 4.2 Processes and threads

| Process | Thread | Work |
| --- | --- | --- |
| Each window | UI thread | React rendering, card layout, and input |
| Main process | Main thread | Windows, menus, dialogs, updates, and the agents' MCP server, which awaits the engine host's answers without blocking |
| Engine host | JavaScript thread | Routes commands, results, and events between windows and the addon. It never blocks on long work: commands run on Rust threads and resolve as promises |
| Engine host | Engine thread, one per live project | Owns the in-memory program for that project and runs one compile or update at a time |
| Engine host | Rayon pool, shared | Parallel per-file extraction and resolution, with fewer threads and low priority for background updates |
| Engine host | Watcher thread, one per live project | Debounces and batches file-system events |
| Engine host | Liveness thread | Rests projects nothing needs, and wakes the ones that gained interest ([§11.2](#112-live-and-resting-projects)) |
| Engine host | SQLite | One writer per live project (its engine thread) and a pool of readers; a resting project has one read-only reader. Readers always see the last committed graph |

### 4.3 What happens during a compile

1. The workspace collects the changed paths. On the first Compile, that's every file in scope.
2. The engine reads and hashes those files. It takes their per-file facts from the cache, or extracts them in parallel.
3. It rebuilds the cross-file indexes in memory, resolves call sites, runs the entrance detectors, and validates the result.
4. It writes only the rows that changed, in one SQLite transaction.
5. It emits `graph_updated` with the project and its changed files, keys, and entrances to every window. Each window refreshes only what it shows of that project that depends on them.

[Section 7](#7-compiling-and-keeping-the-graph-current) gives the details.

## 5. The graph model

### 5.1 Records

| Record | What it is |
| --- | --- |
| File | An indexed source file, with its language, its state, and the hash its facts came from |
| Block | A selected syntax entity: a function, class, call, `if`, loop, and so on. It has a byte range, a parent in the same file, a role relative to that parent, and a structural key |
| Slot | The range of a written part of a block, such as its condition, body, parameter *n*, or argument *n*, whether or not that part has a block of its own |
| Edge | A relationship sourced at one occurrence: a call, a construction, an import, or a named callable passed as an argument |
| Entrance | A declared way that a process or unit of work starts, with its evidence and ordered targets |
| Entrance target | One independently resolved handler or startup target |

Occurrences are the sources of edges, not functions. Function-level views, such as callers and owners, are derived ([§10.1](#101-derived-views)).

### 5.2 Vocabularies

These vocabularies carry over from the proof of concept unchanged.

**Block kinds**

| Kind | Meaning |
| --- | --- |
| `file` | An indexed source file's root |
| `function` | A function declaration or expression with a body, an arrow function, or a Python lambda |
| `class` | A non-ambient class declaration or expression |
| `method`, `constructor`, `getter`, `setter` | A class or object member with a body |
| `variable` | A non-ambient variable declaration, or one name bound by destructuring |
| `call`, `new` | A call or tagged template, and a construction (`new` is TypeScript only) |
| `import` | A runtime module dependency |
| `if` | A condition and its written branches. An `else if` is a nested `if` with role `else` |
| `for`, `for_in`, `for_of`, `while`, `do_while` | Loops. Python `for` maps to `for_of` |
| `switch`, `case`, `default` | A switch and its clauses. Python `match` cases are `case` blocks |
| `try`, `catch` | A try statement and its handlers. `finally` is a slot |
| `return`, `throw`, `break`, `continue` | Transfers. Python `raise` maps to `throw` |
| `expression_statement` | A standalone expression |
| `with`, `match` | Python only |

**Roles**: a block's relationship to its saved parent. The roles are `body`, `parameter`, `condition`, `then`, `else`, `initializer`, `update`, `iterable`, `catch`, `finally`, `expression`, `binding`, `callee`, `argument`, `heritage`, `decorator` (TypeScript), `header` (Python decorators, defaults, annotations, and class bases), `name`, or empty.

**Slots**

| Block kind | Slots |
| --- | --- |
| `function`, `method`, `constructor`, `getter`, `setter` | `name`, repeated `parameter`, `return_type`, `body` |
| `class` | `name`, `heritage` |
| `variable` | `name`, `initializer` |
| `call`, `new` | `callee`, repeated `argument` |
| `if` | `condition`, `then`, `else` |
| `for` | `initializer`, `condition`, `update`, `body` |
| `for_in`, `for_of` | `initializer`, `iterable`, `body`, and `else` in Python |
| `while`, `do_while` | `condition`, `body`, and `else` in Python |
| `switch` | `expression`, `body` |
| `case` | `condition`, `body` |
| `default` | `body` |
| `try` | `body`, `catch` (repeated and indexed in Python), `finally`, and `else` in Python |
| `catch` | `binding`, `body` |
| `with`, `match` | `expression`, `body` |
| `return`, `throw`, `expression_statement` | `expression` |
| `import` | `specifier` |

**Edges**

| Edge kind | Source | Resolved target kinds |
| --- | --- | --- |
| `calls` | `call` | `function`, `method`, `constructor`, `getter`, `setter`, `class` |
| `constructs` | TypeScript `new`, or a Python call with proven class identity | `function`, `method`, `constructor`, or a `class` with no direct constructor |
| `imports` | `import` | `file` |
| `references` | `call` or `new`, at an argument position | `function`, `method`, `class` |

Every call, construction, and runtime import has exactly one occurrence edge. References follow their occurrence and are emitted only for arguments that resolve to a specific callable or class.

**Resolutions**: `resolved` (one proven target in the graph), `external` (a proven origin outside the graph, with a module label), and `unresolved` (with a reason).

**Reasons**

| Reason | Meaning |
| --- | --- |
| `unknown_name` | No declaration exists for the name, and it isn't a known global |
| `unknown_receiver` | The type of the member's receiver is unknown |
| `parameter` | The called value comes from a parameter |
| `mutable_binding` | Writes to the binding or member prevent a stable target |
| `not_concrete` | There is no proven concrete body: an interface or type member, an abstract or ambient declaration, an overload with no implementation, or an opaque value |
| `accessor` | The call invokes an accessor's returned value, not the accessor itself |
| `ambiguous` | More than one body could be the target |
| `overridden` | Subclasses in the graph can supply different implementations |
| `unbound_this` | `this` inside a non-arrow function that isn't a member |
| `not_indexed` | The target exists in the folder but outside the indexed files, because it was excluded or failed to parse |
| `not_found` | A relative import, launch path, or locator names nothing |
| `dynamic` | There is no static shape, as with a computed key, a call result, or a non-literal path |
| `wrapper` | A call result, decorator, or partial supplies the callable without proving which body it is |
| `unsupported` | A recognized form that the supported syntax doesn't cover, such as a spread or a sandbox processor |

**Entrances**

| Kind | Trigger | Targets |
| --- | --- | --- |
| `process` | Script or bin name, or a declared trigger | Exactly one startup: a file root, or the callable named by a Python console script |
| `command` | Subcommand | One or more handlers |
| `http` | Route path plus method (`GET`, `HEAD`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`, `ALL`) | One or more handlers |
| `worker` | Queue or topic | One or more handlers |
| `schedule` | Schedule expression | One or more handlers |
| `event` | Event name | One or more handlers |

A trigger's status is `exact`, `partial`, or `unknown`. Targets keep their declaration order, which is not an execution order. An unresolved handler never removes its entrance. One definition can serve several entrances, and one entrance can name several handlers. [Section 6.6](#66-entrance-detectors) lists the detectors.

**Module labels.** A label names a package (`express`, `@scope/name`), a Node built-in (`node:fs`), `global`, or a repository-relative path for a TypeScript import of an unindexed file. Python labels use the top-level package name, `python:<stdlib-root>`, or `python:builtins`. When several origins are possible, the label is a sorted JSON array.

### 5.3 Text, ranges, and lines

- Each update reads a file's bytes once, hashes them with SHA-256, and decodes them strictly as UTF-8. Invalid UTF-8 fails the file. A byte-order mark is kept as U+FEFF, and line endings and Unicode normalization are preserved.
- Ranges inside the engine are UTF-8 byte offsets. They are zero-based, half-open, and token-exact: they exclude leading comments, indentation, and trailing whitespace. A file root spans the whole text.
- Lines are one-based. CR, LF, CRLF, U+2028, and U+2029 end a line, and CRLF counts once.
- The UI only ever receives UTF-16 offsets. One module in `unfold-core` converts between the two using a per-file index ([U-12](decisions-and-tradeoffs.md#u-12-utf-8-offsets-in-the-engine-utf-16-at-the-ui)).
- Python name identity uses NFKC normalization, but names and ranges keep the written spelling. A Python file that declares a non-UTF-8 encoding fails.
- Paths are relative to the folder, separated by `/`, case-preserving, and never escape the root.

### 5.4 Identity and keys

- Within one graph database, a block is identified by `(file_id, local_id)`. Local IDs are numbered in preorder within each file, so changing a file renumbers only that file.
- A **structural key** names a block in a way that survives most edits, and it is what boards and resolved targets refer to ([U-20](decisions-and-tradeoffs.md#u-20-identity-file-scoped-ids-and-structural-keys)). Keys carry over from the proof of concept:

| Key part | Derivation |
| --- | --- |
| File root | `path#` |
| Named step | `kind:name`, with reserved characters escaped and `[n]` for duplicate siblings |
| Unnamed step | `kind[n]`, the zero-based position among siblings of that kind |
| Nested block | The parent key, then `/step` |
| Entrance | `entrance:` plus a JSON array of detector, kind, method, trigger, and first target text, with `#n` for duplicates |

For example, `ts-playground/src/routes/orders.ts#function:createOrder/if[0]` is the first `if` in `createOrder`. Ordinal steps such as `if[0]` shift when code is inserted before them, so boards protect them with guards and signatures ([§9.1](#91-board-files)).

### 5.5 What the graph does not claim

- It contains no control-flow graph, data-flow graph, or runtime trace. Source order is an outline, not an execution order. In `save(convert(value))`, the outer call is written first, but `convert` runs first.
- Activation closures follow resolved relationships and include every written branch. They over-approximate, so the UI says "reached at least from".
- Fingerprints cover sources and the configuration that shaped the graph, not installed packages or runtime state.
- An export, a name, or a function without callers never becomes an entrance.

### 5.6 Where the complete semantics live

The proof of concept's `docs/format.md`, `docs/compiler.md`, and `docs/reader.md` hold every invariant, syntax mapping, detector bound, and derivation rule. In Step 1 of the migration they are rewritten for the new repository as `docs/graph-model.md`, `docs/languages/typescript.md`, `docs/languages/python.md`, `docs/entrances.md`, and `docs/reader.md`, using Unfold's names and byte offsets. Where this document summarizes a rule, those files are authoritative.

## 6. The engine

### 6.1 Pipeline

| Stage | Runs over | Cached | Produces |
| --- | --- | --- | --- |
| Discover | Changed paths (the whole folder on a first Compile) | No | The set of files in scope |
| Snapshot | Changed files | No | Bytes, hash, decoded text, and line index |
| Extract | Changed files, in parallel | Yes, by path, content hash, engine version, and settings hash | Per-file facts: blocks, slots, names, roles, local bindings, import and export declarations, class shapes, call sites, and registration candidates |
| Link | The whole program, in memory | Rebuilt from cached facts | Module graph, export tables, class families, override and mutation censuses, globals, the emit map, and project inputs |
| Resolve | Every call site (V1), or only affected files (V2) | No | Edges with resolution, reason, module, and position |
| Detect | All detectors | No | Entrances and their ordered targets |
| Validate | The new graph state | No | Invariants hold, or the update fails |
| Commit | Only the rows that changed | No | One SQLite transaction and a `graph_updated` event |

### 6.2 Language adapters

Language crates answer narrow questions. One shared backend owns the policy.

```rust
/// One source language. Extraction is per file and cached; linking sees the whole program.
pub trait Language: Send + Sync {
    fn id(&self) -> LanguageId;
    fn producer(&self) -> Producer;                         // name and version, stored with the graph
    fn owns(&self, path: &RepoPath, settings: &Settings) -> bool;
    fn settings_hash(&self, path: &RepoPath, settings: &Settings) -> SettingsHash;
    fn extract(&self, file: &Snapshot, settings: &Settings) -> Result<FileFacts, SourceFailure>;
    fn link<'a>(&'a self, files: &'a FactsSet, inputs: &'a ProjectInputs) -> Box<dyn Linked + 'a>;
    fn highlighter(&self) -> &dyn Highlighter;
}

/// What the shared backend asks about. Evidence can withhold a target, never invent one.
pub trait Linked {
    fn callee_evidence(&self, call: CallSite) -> CalleeEvidence;
    fn value_evidence(&self, value: ValueRef) -> ValueEvidence;
    fn module_dependencies(&self, file: FileId) -> &[ModuleDependency];
    fn module_exports(&self, file: FileId) -> &[Export];
    fn classes(&self) -> &ClassTable;
    fn detectors(&self) -> &[&dyn EntranceDetector];
}
```

`FileFacts` has two parts. One is a language-neutral graph mapping: blocks, slots, call sites, and their ranges. The other is a private payload that only the language crate reads and serializes. The backend never interprets language syntax. A tiny in-memory "alien" test language proves this in `unfold-engine`'s tests, as it did in the proof of concept.

### 6.3 TypeScript

- **Files.** `.ts`, `.tsx`, `.mts`, and `.cts`. Declaration files (`.d.ts`) never get blocks. JavaScript comes in V2.
- **Parsing.** `oxc_parser`, pinned to an exact version behind Unfold's own syntax adapter. A file with syntax errors is a failed file with a message. There is no editor-style error recovery.
- **Project configuration.** Each file uses its nearest `tsconfig.json`. That means JSONC, an `extends` chain that may reach into installed packages, `paths` and `baseUrl`, module detection, JSX mode, and target-dependent semantics for parameters and class fields. A configuration's file selectors never change what is graphed.
- **Binding.** This uses `oxc_semantic`'s scopes, symbols, and references, with Unfold's binding rules on top. Those rules cover `var`, `let`, and `const` scoping, parameter visibility, destructuring, merging namespaces and enums, static and instance member tables, private names, `this` (arrow functions inherit it), and merged overloads. If `oxc_semantic` disagrees with these rules, the proof of concept's binder is ported onto oxc's syntax tree instead ([U-7](decisions-and-tradeoffs.md#u-7-typescript-through-oxc-without-a-type-checker-in-v1)).
- **Evidence levels.** These carry over from the proof of concept's native front end:
  1. lexical functions, constant function, arrow, and class initializers, and immediately invoked functions;
  2. named, default, and namespace imports, re-exports, packages, and known globals;
  3. classes, `new`, `super`, `this`, static members, and inherited members;
  4. written class and library annotations, parameter properties, property chains, and declared return types.

  New in Unfold: members of constant object literals (`logger.info`), which fixes PoC defect C-2.
- **Override census.** A method on an instance is withheld as `overridden` when a subclass in the graph can override it.
- **Globals and library receivers.** Tables of known ECMAScript, DOM, and Node globals and library receiver types are generated from TypeScript's `lib` files at a pinned version. `cargo xtask gen-lib-tables` parses those files with oxc and checks in the output, along with its source version.
- **Modules.** `oxc_resolver` with each file's `tsconfig`, covering `paths`, `baseUrl`, package `exports`, and `.js`-to-`.ts` extension aliasing. A missing relative import is `not_found`. A bare package import is `external` without needing installed packages. Existing files outside scope, and files that failed, are `not_indexed`.
- **Runtime imports.** Import declarations, `export … from`, `import x = require(…)`, and dynamic `import()` with a literal argument. Type-only imports are erased. Empty named forms such as `import {} from "x"` are erased unless `verbatimModuleSyntax` is on, which fixes C-12.
- **Emit map.** This maps compiled output paths, such as `node dist/server.js`, back to their sources, using each configuration's `rootDir`, `outDir`, and extension rules. When `rootDir` isn't given, it defaults to the configuration's directory. An output that two sources can produce stays ambiguous. Emitted files never need to exist.

### 6.4 Python

- **Parsing.** `ruff_python_parser`, pinned to a Ruff release tag behind Unfold's adapter. The grammar version comes from project settings: 3.12, 3.13, or 3.14, defaulting to 3.14. Syntax that the selected version doesn't support fails the file.
- **Structure.** Calls stay `call` blocks; only proven class identity turns their edge into `constructs`. Lambdas are `function`, `raise` is `throw`, `except` and `except*` are `catch`, and both `for` and `async for` are `for_of`. Decorators, defaults, annotations, and class bases use the `header` role and belong to the enclosing scope. Comprehensions create scopes but not blocks.
- **Binding.** These rules are ported from the proof of concept and are not borrowed from another checker:
  - **Write census.** Definitions, imports, assignments (including augmented and deleting ones), `global` and `nonlocal`, loop, `with`, and `except` targets, destructuring, assignment expressions, comprehensions, and type-parameter scopes are all counted. Any competing write anywhere in the indexed scope invalidates a name's identity.
  - **Reflection** needs proven identity. A non-literal `getattr` is `dynamic` and `functools.partial` is `wrapper`. An unknown `setattr` or `delattr` invalidates the members it might affect. Proven `exec` and `eval` make dependent module identities `unsupported`.
  - **Classes.** C3 linearization requires known bases. A virtual call needs a single implementation across every possible receiver. `super()` must agree across linearizations, and class attribute lookup needs a proven metaclass. A direct concrete `__init__` is the construction target; otherwise the class itself is. A callable instance needs a proven `__call__`.
  - **Receivers.** The first parameter of an ordinary method or a proven `classmethod` is the receiver; a `staticmethod` has none. An attribute's receiver comes from a written annotation or from a single proven assignment in the constructor.
  - **Decorators** must all be on the allowlist for a body to stay concrete: `staticmethod`, `classmethod`, `functools.cache`, `functools.lru_cache`, `typing.final`, and `typing.override`. Each must be proven by identity, not by spelling.
  - **Imports.** These cover absolute and relative imports, package initializers, namespace packages, a literal `__all__`, aliases, and conditional alternatives, all resolved only inside the configured source roots. A runtime import is suppressed only by a proven `TYPE_CHECKING` guard. A named child import (`from pkg import child`) adds a dependency only when the attribute is proven absent. Local modules outside scope are `not_indexed`. External labels come from a pinned standard-library inventory or the top-level package name.
  - **Positions** are written ordinals, including receivers, keywords, and expansions. Matching ordinals never bind a caller's argument to a parameter.
- **Source roots.** These come from `[[python.project]]` in `.unfold/config.toml`. Without one, Unfold uses `src/` when the project uses a src layout, and otherwise the folder that holds `pyproject.toml`.
- **Isolation.** No Python interpreter, virtual environment, or installed package is involved, ever.

### 6.5 The shared backend

The backend owns block selection, roles, names, slots, the value policy, the order in which reasons are assigned, module labels, provenance, edges, entrance assembly, locators, and the launch-command grammar.

**Block selection** (the TypeScript mapping; Python has its own): a function, method, constructor, or accessor becomes a block only when it has a body. A class becomes a block only when it is non-ambient. Variables become blocks, including one binding block for each destructured name. Calls, `new` expressions, runtime imports, and control statements also become blocks. Types, overload signatures, ambient declarations, namespaces, and brace-only blocks never become blocks, but their runtime descendants are still visited.

**Roles** use the first child slot on the path that leaves the nearest saved ancestor. A path through a decorator gets `decorator`.

**Names** come from syntax, never from matching targets. An anonymous function gets the name of the variable, parameter, field, or property it initializes, or of the identifier it is assigned to, or `default`.

**Value policy**

| Context | Target |
| --- | --- |
| Call | A concrete callable body |
| Construction | A callable function or method, a class's explicit constructor with a body, or otherwise the class |
| Reference (a callable passed as an argument) | A concrete function, method, or class |
| Handler (an entrance target) | A concrete function or method, including an inline function. A class handler is `unsupported` |

**Reason order.** The shared evidence path applies these tests in this order, and the first one that matches decides the outcome:

| Order | Test | Outcome |
| --- | --- | --- |
| 1 | The language withholds (wrapper, dynamic, unsupported, not concrete) | That reason |
| 2 | The evidence is ambiguous | `ambiguous` |
| 3 | `this` is unbound | `unbound_this` |
| 4 | The import binding target is `not_found`, `not_indexed`, or ambiguous | That reason |
| 5 | The effective candidate is excluded, or is a repository file outside scope | `not_indexed` |
| 6 | A proven package-import root with no local contradiction | `external` |
| 7 | No effective candidates | `external` for external augmentations only; otherwise `unknown_name` for an identifier, `unknown_receiver` for a property access, or `dynamic` |
| 8 | Every candidate is external, or they mix indexed and external | `external`, or `ambiguous` if mixed |
| 9 | An indexed candidate without a declaration | `unknown_name` |
| 10 | More than one distinct concrete target | `ambiguous` |
| 11 | No target | `parameter`, then `mutable_binding`, then `accessor`, then `not_concrete` |
| 12 | Non-concrete evidence beside one target, other than compatible overloads | `ambiguous` |
| 13 | Override evidence, or a subclass override found by the census | `overridden` |
| 14 | A class handler, or an unsupported target kind | `unsupported` |
| 15 | One permitted target | `resolved` |

**Module labels** apply four rules; the first that matches wins:

1. The package an import is rooted in.
2. The origin of the receiver: `app.get` on `const app = express()` names `express`.
3. `global`, for an undeclared, unimported root such as `console` or `process`.
4. Declaration locations, normalized and sorted.

**Provenance** proves a narrow import lineage within three steps. Those steps can be a property access, an imported identifier from an external package, a constant initialized by a call or `new`, or a written annotation. Detectors use it to recognize framework objects, but it never makes a callee callable.

**Edges and labels.** The edges for each occurrence are written in block order. Named-argument references follow their call in argument order. Labels are short syntax: an identifier, `receiver.member`, `callee()` with no arguments, `[…]` for computed keys, or `(…)` for anything else. A label is at most 120 UTF-16 units and never splits a surrogate pair.

**Manual locators** follow the grammar `path`, `path#name`, `path#Owner.member`, `path#default`, or `path:line`. They are resolved against current declarations and exports on every compile.

**Launch-command grammar.** Package scripts are interpreted from their JSON text and never executed. The grammar covers:

- a POSIX shell subset: `&&` and `;` steps, quoting, `env` and `cross-env` assignments, and `cd`;
- script references: `npm run`, `npm --prefix … run`, `yarn`, `pnpm`, and `bun run`, nested up to eight levels with cycle detection;
- file launchers, each with the exact accepted and rejected options carried over from the proof of concept: `node`, `tsx`, `ts-node`, `ts-node-esm`, `nodemon`, and `bun`.

The last step decides the startup. `npm run build && node dist/server.js` starts `src/server.ts` through the emit map.

### 6.6 Entrance detectors

| ID | Language | Recognizes | Trigger | Evidence |
| --- | --- | --- | --- | --- |
| `manual` | Both | `[[entrance]]` declarations in `.unfold/config.toml` | Declared | The TOML table, with a pointer |
| `package.script` | TypeScript | A `package.json` script whose final step launches a module | Script name | The JSON member |
| `package.bin` | TypeScript | `package.json` `bin` entries | Command name | The JSON member |
| `express.route` | TypeScript | `get`, `head`, `post`, `put`, `patch`, `delete`, `options`, and `all` on a proven Express application or router, with mount prefixes composed up to five levels | Method and path | The route call |
| `bullmq.worker` | TypeScript | `new Worker(queue, processor)` from `bullmq` | Queue name | The construction |
| `node-cron.schedule` | TypeScript | `schedule(expression, handler)` from `node-cron` | Cron expression | The call |
| `pyproject.script` | Python | `[project.scripts]` entries of the form `module:callable` | Script name | The TOML assignment |
| `fastapi.route` | Python | `get`, `post`, `put`, `patch`, `delete`, `options`, and `head` on a proven `FastAPI` or `APIRouter`, with `include_router` prefixes up to five mounts and `Depends` lists | Method and path | The decorator or registration call |
| `celery.task` | Python | Parameterized `@app.task(...)` on a proven `Celery` instance | The literal `queue` | The factory or registration call |
| `apscheduler.job` | Python | APScheduler 3 `BlockingScheduler.add_job` with `CronTrigger.from_crontab` | The crontab string | The `add_job` call |

Every detector recognizes only what its provenance proves. Lookalike local names, wrappers, spreads, and dynamic values become explicit `unsupported`, `wrapper`, `dynamic`, or `unknown` outcomes, never guesses. New detectors need positive cases, decoys, alias and rebinding cases, evidence-range checks, and refusal cases ([U-34](decisions-and-tradeoffs.md#u-34-testing-at-every-layer)).

### 6.7 Validation

Every compile and update validates the new graph state before committing it. Validation checks:

- the vocabularies, and that ranges are inside their file and on token boundaries;
- that parents contain their children and the tree is in preorder;
- slot cardinality, and that call, `new`, and import blocks pair one-to-one with their occurrence edges;
- that references are in argument order, and that resolved targets exist with a permitted kind;
- that reasons and modules appear exactly where they should;
- entrance ordering, evidence ranges and pointers, and target cardinality;
- that no resolved edge crosses languages;
- that input fingerprints match.

Language crates also supply an independent census of call sites and token boundaries, so validation doesn't just trust the extractor. A failed validation fails the update, and the previous graph stays.

### 6.8 Determinism

Files are processed in path order. Blocks are in preorder within each file, edges follow their source block, and entrances sort by evidence path, start, end, detector, kind, method, and trigger. Parallel work is reassembled by input index, never by completion order. The canonical export ([§8.6](#86-canonical-export)) must be identical under shuffled input order, any thread count, and any sequence of updates that ends with the same file contents.

### 6.9 Defects from the proof of concept

| ID | Defect in the proof of concept | In Unfold |
| --- | --- | --- |
| C-1, C-3 | The checker-backed producer can use a computed key's symbol as a target, or lose a signature's placeholder | Gone, along with that producer |
| C-2 | The native producer misses members of constant object literals (25 playground calls) | A bounded rule for constant object literals |
| C-4 | An explicit `this` parameter shifts parameter positions | Positions skip it |
| C-5 | A launch into an existing unindexed file can report `not_found` | Extensions are probed before choosing between `not_found` and `not_indexed` |
| C-6 | A local `NodeJS.Process` parameter is labeled `global` | The root must be undeclared |
| C-12 | Empty `import {} from "x"` forms create a dependency | They are erased, as TypeScript's emit does |
| C-8, C-9 | Open questions: destructured bindings from literal dynamic imports, and property locators compared with calls | Decided with fixtures during the TypeScript migration (C-8 in Step 4, C-9 in Step 5), then recorded as decisions |

## 7. Compiling and keeping the graph current

### 7.1 Three actions

| Action | When | What it does |
| --- | --- | --- |
| **Compile** (the button on a new project's Home, then the project menu, or ⇧⌘B) | Once after a project is added, and whenever you want to be sure | Checks every file against the graph and brings everything up to date. The first Compile creates `.unfold/` and builds the graph from nothing |
| **Background updates** | Automatically after the first Compile, while the project is live. You can pause them per project | Watches the folder and applies changes a moment after files are saved |
| **Rebuild from scratch** | Rarely, from the project menu or Add's commands | Compiles everything again without the cache, then replaces the stored graph |

### 7.2 One code path

The engine has one entry point, `update(changes)`:

- A first Compile is an update in which every file is new.
- A background update contains the files the watcher reported.
- Rebuild from scratch is an update over every file with an empty cache.

Because they all share this code path, a graph built by many updates is the graph a fresh compile would build from the same effective inputs, and the tests check exactly that ([§16](#16-testing)).

### 7.3 What "recompile only what changed" means

The graph has two kinds of facts:

- **Per-file facts** depend only on one file's bytes and its language settings: blocks, slots, names, roles, local bindings, import and export declarations, class shapes, and call sites. They are cached by path, content hash, engine version, and settings hash. A file whose bytes haven't changed is never parsed again.
- **Cross-file facts** depend on many files at once. They include which file an import names, which declaration an export chain ends at, which subclasses override a method, whether any module reassigns a function, and how a router's mount prefix combines with a route path. Edges and entrances are cross-file facts.

So editing one file can change edges in files that weren't edited:

| An edit in one file | Changes elsewhere |
| --- | --- |
| Renaming `export function place` | Every call to `place` in other files |
| Adding `class PriorityOrders extends OrderService` with its own `place()`, in a new file | `orderService.place()` calls in other files become `overridden`, although those files never import the new one |
| In Python, writing `orders.place = traced(orders.place)` | `place` loses its proven identity everywhere |
| Changing a router's mount prefix in `server.ts` | The trigger path of routes declared in `routes/orders.ts` |
| Creating `src/lib/slack.ts` | Imports in other files that were `not_found` now resolve |

**V1.** Every update does this:

1. Re-read, hash, and extract only the changed files, using the cache for bytes it has seen before.
2. Rebuild the cross-file indexes in memory from every file's cached facts.
3. Re-resolve every call site and re-run the detectors, in memory and in parallel.
4. Compare the result with the previous state, and write only the rows that changed, in one transaction.
5. Report which files, keys, and entrances changed, so that only those cards refresh.

Steps 2 and 3 cover the whole program, but they run on compact in-memory facts, with no parsing and no I/O. We expect well under a second for repositories with a few thousand files; Step 5 of the migration measures this and Step 9 sets a hard limit. Re-resolving everything makes V1 correct by construction, because there is no dependency bookkeeping to get wrong ([U-14](decisions-and-tradeoffs.md#u-14-what-gets-recompiled-per-file-facts-in-v1-early-cutoff-in-v2)).

### 7.4 V2: early cutoff

For very large repositories, V2 skips re-resolving files that an edit can't affect:

- Each file's facts split into two parts, each with its own hash. The **interface** is what other files can observe: exports, top-level declarations and whether they are concrete and stable, class shapes, module-level writes, and registrations that detectors can trace. The **body** is everything else.
- While resolving, the engine records what each file consulted: other files' interfaces, census entries (a class family's overrides, the writes to a module member), and lookups that found nothing.
- An update re-resolves three sets of files: the changed files, the files that consulted a changed interface or census entry, and the files whose failed lookups match a created or deleted path. Detectors re-run only where their evidence or provenance changed.
- Most edits are inside function bodies. Then only the edited file is re-resolved.

V2's early cutoff is turned on only when the incremental-equals-full property tests pass with it ([§16](#16-testing)). A query-based framework of the kind rust-analyzer uses (Salsa) is the step after that, and only if V2 isn't enough.

### 7.5 Triggers, batching, and cancellation

- `notify`, with a debouncer, watches the folder through the same ignore rules as discovery. It watches `.unfold/config.toml`, `.unfold/boards/`, and `.unfold/local/drafts/` for external edits, and never watches `.git/` or `.unfold/local/cache/`.
- Saves are batched. An update starts about 300 ms after the last change, so a branch switch that touches 500 files becomes one update.
- A newer change cancels an update that hasn't started committing. The changes merge and the update starts again. Commits are atomic: the UI sees the old graph or the new one, never half of either.
- Background updates run at low priority on a limited number of threads. An explicit Compile uses every core but one.
- If the watcher can't keep up, because of an event overflow, Linux inotify limits, or a network volume, Unfold falls back to a periodic scan and says so on the project's Home.
- While background updates are paused, the project's row and Home count the files that changed since the last Compile.
- A resting project holds no watcher. When it wakes, the reopen check below catches up with what changed while it rested.

### 7.6 Files that don't parse

| State | Meaning | In the graph | On its cards |
| --- | --- | --- | --- |
| Current | Its facts come from the bytes on disk | Normal | Code and Flow shown |
| Pending | It changed on disk and the update hasn't committed yet | The previous facts | The last verified render, dimmed, labeled "updating" |
| Last good | It changed and no longer parses | The facts from the last version that parsed | The last verified render, dimmed, labeled "doesn't parse right now", while the app stays open; after a restart, code is withheld |
| Failed | No version of it has parsed | Absent; calls into it are `not_indexed` | Withheld; Problems shows the error |

The first Compile never aborts because some files don't parse. It builds the graph from the files that do, marks it incomplete, and lists the rest in Problems. Read errors and invalid settings are reported the same way. An engine or validation error fails the update and keeps the previous graph.

The UI keeps a card's last verified render only in memory. It never applies old ranges to new bytes, and it never persists source ([U-15](decisions-and-tradeoffs.md#u-15-a-file-that-stops-parsing-keeps-its-last-good-facts)). Rebuild from scratch drops last-good facts along with the cache, so a file that doesn't parse at that moment becomes failed.

### 7.7 Settings and inputs

The inputs that shape the graph are fingerprinted:

- `.unfold/config.toml`;
- each `tsconfig.json` and its `extends` chain;
- the `package.json` files that the package detectors read;
- the `pyproject.toml` files that console scripts and Python project settings use.

When one changes, the files whose settings hash changed are extracted again, and linking runs again. For example, changing the Python grammar version touches every Python file.

The files `[evidence] files` lists are fingerprinted too, as inputs with the role `evidence`, but they shape nothing: boards cite them. A changed evidence file only replaces its record in the graph; nothing is extracted or linked again ([settings and scope](settings-and-scope.md#evidence-files), [U-58](decisions-and-tradeoffs.md#u-58-evidence-files-are-inputs)).

### 7.8 Opening a project

A project opens when it wakes ([§11.2](#112-live-and-resting-projects)). The graph in SQLite is usable at once. In the background, Unfold compares each file's size and modification time with the stored values, hashes only the files that differ, and runs one update. It also loads the cached per-file facts to rebuild the in-memory program, so the next save updates quickly. When nothing changed while the project rested (no file added, removed, or changed, and the same settings, configuration inputs, and engine), the stored graph is kept as it is: nothing is linked again or written. When an app update has changed the engine version, the cache no longer matches and Unfold recompiles in the background. Boards are unaffected.

### 7.9 What a project's state says

Each project has one state, shown as one glyph on its row in the sidebar and in words on its Home. The first that applies wins:

| State | Row | Home |
| --- | --- | --- |
| Failed | Red triangle | Compile failed · the previous graph stays · See Health |
| Compiling | Progress ring | Compiling · 12 of 84 files · linking · Cancel |
| Updating | Progress ring | Updating · 2 files changed |
| Paused | Pause glyph | Paused · 3 files changed since · Resume |
| Not compiled | Hollow circle | Not compiled yet · branch main |
| Problems | Amber dot, with the count in its tooltip | 1 file doesn't parse · showing its last version that parsed · See Health |
| Current | Nothing | Graph current · 84 files · TypeScript + Python · updated 2 min ago |
| Resting | Moon | Resting · last checked 6 min ago (Home shows the last known state) |
| Unavailable | Red triangle, with the reason in its tooltip | Unavailable · the folder is gone, or another copy of Unfold has it open |

Problems here are graph and settings problems only: files that don't parse or can't be read, settings errors, discovery, input, and cache problems, and a failed Compile. Boards that need review show in Home's Health line instead. "Using fallback location" and "watching by scanning" are notes on Home's state line and in Project settings → General.

## 8. Storage

### 8.1 `.unfold/` in every repository

```text
<folder>/.unfold/
├── config.toml              # shared settings; written only when a setting differs from the defaults
├── boards/                  # published boards; commit them like code
│   └── order-placement.json
└── local/                   # this machine only; Git ignores it
    ├── .gitignore           # contains "*", so Git ignores this folder and this file
    ├── cache/               # disposable: safe to delete at any time
    │   ├── graph.sqlite     # the graph, the per-file facts, and the search index
    │   └── lock             # one copy of Unfold per folder
    ├── drafts/              # your drafts, and published boards' unpublished changes; not disposable
    │   ├── board-1760000000000.json
    │   └── order-lifecycle.changes.json
    └── state.json           # background updates on or off, empty board folders, each board's view state
```

- The first Compile of a project creates `.unfold/`. Adding a project and reading its preview before then write nothing.
- After a first Compile with default settings, Git sees nothing new, because everything is under the self-ignored `local/`. `config.toml` and `boards/` appear as new files only when you change a setting or publish a board, which is exactly when you'd want to commit them.
- `.unfold/` is never indexed or searched.
- `local/cache/` can be deleted at any time; the next Compile rebuilds it. Drafts and state survive.
- A subfolder of a repository added as a project gets its own `.unfold/`.
- A team that doesn't want shared files can add `.unfold/` to its own `.gitignore`. Published boards then simply stay on each machine.

### 8.2 Outside the folder

| What | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Preferences: theme, keyboard, editor command, background defaults, thread limit, agents | `~/Library/Application Support/Unfold/preferences.json` | `%APPDATA%\Unfold\preferences.json` | `~/.config/Unfold/preferences.json` |
| The workspace (`workspace.json`: projects, the default Home selection, recent boards, folders opened before) and the windows that come back at launch (`windows.json`) | `~/Library/Application Support/Unfold/` | `%APPDATA%\Unfold\` | `~/.config/Unfold/` |
| Logs | `~/Library/Logs/Unfold/` | `%APPDATA%\Unfold\logs\` | `~/.config/Unfold/logs/` |
| Fallback for folders Unfold can't write to | `~/Library/Caches/Unfold/folders/<hash of the path>/` | `%LOCALAPPDATA%\Unfold\folders\<hash>\` | `~/.cache/Unfold/folders/<hash>/` |

Electron keeps Chromium's own caches in the same app-data folder. The UI never uses browser storage: everything it remembers goes through Rust into the files above or into `.unfold/local/` ([U-46](decisions-and-tradeoffs.md#u-46-workspace-state-lives-in-app-data)). Rust writes each of these JSON files atomically and reads a missing or broken one as the defaults, because remembered state never blocks the app:

| File | Holds | Written when |
| --- | --- | --- |
| `workspace.json` | Each project's id, path, cached shared name, row expanded or collapsed, repository identity, when it was added, and its last known state; the last project selected in any window (the default for new windows); recent boards (≤ 20); folders removed from the workspace (≤ 5) | Projects are added, removed, reordered, expanded, selected, renamed, or switched; a board is created, opened, renamed by Publish, or deleted; a project rests; the app quits |
| `windows.json` | Each window's key, bounds, whether maximized, Home selection, board tabs (each with its page), active tab, and layout (sidebar width and visibility, the Selected section, the peek pin, Home's view and board filters, expanded sidebar folders); at most 16 windows | The window saves its state, and the main process its bounds |
| `.unfold/local/state.json` | Background updates on or off; empty board folders; each board's view state: its active page and each page's camera and lens (≤ 500 boards, ≤ 200 pages each) | A setting or a view changes. Before the first Compile, changes are kept in memory |


If a folder is read-only, or sits on a network volume where SQLite's write-ahead log isn't safe, `local/` moves to the fallback location and the project's Home says so. Publishing then needs a writable folder.

### 8.3 The graph database

The executable schema and its current version live in
[`crates/unfold-store/src/schema.rs`](../crates/unfold-store/src/schema.rs). For a storage
change, start there and follow the store's `writer/` and `read/` modules. The SQL below explains
the design; connection settings and query-specific indexes are maintained in the implementation.

```sql
-- .unfold/local/cache/graph.sqlite. A cache: rebuilt whenever the engine version changes.
PRAGMA journal_mode = WAL;

CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
-- schema_version, engine_version, producers (JSON), settings_hash, compiled_at, complete,
-- link_basis (JSON: what the graph was linked from, and the problems linking found)

CREATE TABLE files (
  id          INTEGER PRIMARY KEY,
  path        TEXT NOT NULL UNIQUE,     -- folder-relative, '/'-separated, case preserved
  language    TEXT NOT NULL,            -- 'typescript' | 'python'
  state       TEXT NOT NULL,            -- 'current' | 'last_good' | 'failed'
  facts_hash  BLOB,                     -- SHA-256 of the bytes the facts came from
  disk_hash   BLOB,                     -- SHA-256 of the bytes at the last check
  disk_mtime  INTEGER,
  disk_size   INTEGER,                  -- cheap change check before hashing
  failure     TEXT                      -- syntax or read error, with its position
);

CREATE TABLE inputs (                   -- configuration that shaped the graph
  path TEXT PRIMARY KEY,
  role TEXT NOT NULL,                   -- 'unfold-config' | 'tsconfig' | 'package' | 'pyproject' | 'evidence'
  hash BLOB NOT NULL
);

CREATE TABLE keys (id INTEGER PRIMARY KEY, key TEXT NOT NULL UNIQUE);   -- interned structural keys

CREATE TABLE blocks (
  file_id INTEGER NOT NULL, local_id INTEGER NOT NULL,
  key_id INTEGER NOT NULL UNIQUE,
  parent_local_id INTEGER,
  kind TEXT NOT NULL, role TEXT, name TEXT,
  start_byte INTEGER NOT NULL, end_byte INTEGER NOT NULL,
  start_line INTEGER NOT NULL, end_line INTEGER NOT NULL,
  PRIMARY KEY (file_id, local_id)
) WITHOUT ROWID;

CREATE TABLE slots (
  file_id INTEGER NOT NULL, block_local_id INTEGER NOT NULL,
  slot TEXT NOT NULL, idx INTEGER NOT NULL DEFAULT -1,
  start_byte INTEGER NOT NULL, end_byte INTEGER NOT NULL,
  PRIMARY KEY (file_id, block_local_id, slot, idx)
) WITHOUT ROWID;

CREATE TABLE edges (
  file_id INTEGER NOT NULL, source_local_id INTEGER NOT NULL, seq INTEGER NOT NULL,
  kind TEXT NOT NULL,                   -- 'calls' | 'constructs' | 'imports' | 'references'
  target_key_id INTEGER,                -- only when resolved
  target_text TEXT NOT NULL,
  resolution TEXT NOT NULL,             -- 'resolved' | 'external' | 'unresolved'
  reason TEXT, module TEXT, position INTEGER,
  PRIMARY KEY (file_id, source_local_id, seq)
) WITHOUT ROWID;
CREATE INDEX edges_by_target ON edges (target_key_id);

-- Typed records (docs/graph-model.md, "Typed records"), written and replaced per file like edges.
CREATE TABLE signatures (file_id INTEGER NOT NULL, block_local_id INTEGER NOT NULL,
  returns TEXT, returns_start INTEGER, returns_end INTEGER,
  PRIMARY KEY (file_id, block_local_id)) WITHOUT ROWID;
CREATE TABLE parameters (file_id INTEGER NOT NULL, block_local_id INTEGER NOT NULL, idx INTEGER NOT NULL,
  kind TEXT NOT NULL, name TEXT, flags INTEGER NOT NULL,   -- optional (1) | default (2)
  type TEXT, type_start INTEGER, type_end INTEGER,
  PRIMARY KEY (file_id, block_local_id, idx)) WITHOUT ROWID;
CREATE TABLE members (file_id INTEGER NOT NULL, block_local_id INTEGER NOT NULL, idx INTEGER NOT NULL,
  kind TEXT NOT NULL, name TEXT, flags INTEGER NOT NULL,   -- optional (1) | readonly (2) | default (4)
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
  resolution TEXT NOT NULL, target_key_id INTEGER, reason TEXT, module TEXT, heritage TEXT,
  PRIMARY KEY (file_id, block_local_id, seq)) WITHOUT ROWID;
CREATE INDEX type_links_by_target ON type_links (target_key_id);   -- "used in N places"

CREATE TABLE entrances (
  id INTEGER PRIMARY KEY,
  key_id INTEGER NOT NULL UNIQUE,
  kind TEXT NOT NULL, label TEXT, method TEXT, trigger TEXT, trigger_status TEXT NOT NULL,
  detector TEXT NOT NULL,
  evidence_path TEXT NOT NULL,
  evidence_start_byte INTEGER NOT NULL, evidence_end_byte INTEGER NOT NULL,
  evidence_block_key_id INTEGER,        -- source evidence
  evidence_pointer TEXT                 -- configuration evidence: JSON or TOML pointer
);

CREATE TABLE entrance_targets (
  entrance_id INTEGER NOT NULL, position INTEGER NOT NULL,
  role TEXT NOT NULL,                   -- 'startup' | 'handler'
  target_key_id INTEGER, target_text TEXT NOT NULL,
  resolution TEXT NOT NULL, reason TEXT, module TEXT,
  PRIMARY KEY (entrance_id, position)
);

-- Reader projections, recomputed after each update.
CREATE TABLE definitions (key_id INTEGER PRIMARY KEY, file_id INTEGER, local_id INTEGER,
                          kind TEXT, qualified_name TEXT);
CREATE TABLE applications (id INTEGER PRIMARY KEY, startup_key_id INTEGER, name TEXT, language TEXT);
CREATE TABLE memberships (entrance_id INTEGER, application_id INTEGER,
                          PRIMARY KEY (entrance_id, application_id));

-- The per-file facts cache.
CREATE TABLE file_facts (
  path TEXT NOT NULL, hash BLOB NOT NULL, engine TEXT NOT NULL, settings_hash BLOB NOT NULL,
  facts BLOB NOT NULL,                  -- serialized FileFacts
  PRIMARY KEY (path, hash, engine, settings_hash)
) WITHOUT ROWID;

CREATE VIRTUAL TABLE search USING fts5(name, qualified_name, path, kind, key UNINDEXED);
```

- **Keys are interned.** A structural key keeps its ID for the life of the database, so edges into an unchanged definition stay valid when other code moves.
- **Byte offsets are stored**, and `unfold-reader` converts them to UTF-16 when it builds what the UI renders.
- **Reached-from is computed on demand**, from the activation closures, and cached in memory.

### 8.4 The per-file facts cache

The cache holds the serialized `FileFacts`, in a compact serde format (postcard), keyed as above. For each path it keeps the current entry and the last good one. Deserializing facts is much cheaper than parsing, so reopening a folder rebuilds the in-memory program quickly.

### 8.5 Search index

FTS5 finds candidate definitions, entrances, and files, and a fuzzy matcher (`nucleo`) ranks them. Text search reads files with ripgrep's libraries (`grep-searcher`, `grep-regex`) through the same ignore rules. It streams results and can be cancelled.

### 8.6 Canonical export

The canonical export is a deterministic JSON Lines form of the whole graph. Records are sorted by path, range, and kind, and refer to structural keys instead of numeric IDs. For example (the values are illustrative):

```text
{"t":"block","key":"src/routes/orders.ts#function:createOrder","kind":"function","role":"body","name":"createOrder","range":[512,1044],"lines":[25,35]}
{"t":"edge","source":"src/routes/orders.ts#function:createOrder/variable:order/call[0]","seq":0,"kind":"calls","target":"src/services/order-service.ts#class:OrderService/method:place","text":"orderService.place","resolution":"resolved"}
```

It is used for snapshots, parity with the proof of concept, determinism checks, and debugging. It keeps what the proof of concept's CSV files were good at, reviewable diffs, without making them the storage format.

### 8.7 Concurrency

- Write-ahead logging lets UI queries read the last committed graph while an update runs.
- Only a live project's engine thread writes its graph. A resting project's graph is opened read-only.
- Unfold runs as a single copy per user. Launching it again, or opening a folder with it from another program, hands the request to the running copy, which adds the folder to the workspace.
- Inside that copy, every window shows every project. A live project has one engine thread, one SQLite writer, and one in-memory program, whichever windows read it ([§11.2](#112-live-and-resting-projects)).
- A board is open in one window at a time, so two windows never edit the same board.
- A lock file in `local/cache/` keeps a second copy of Unfold, such as a development build or another version, from opening the same folder. That copy shows the project as unavailable instead.
- Board and settings files edited outside the app, by a person, an agent, a pull, or a branch switch, are reloaded when the open board has no unsaved changes. Otherwise Unfold asks which version to keep.

## 9. Files we treat as contracts

| Contract | Who reads it | How it's versioned |
| --- | --- | --- |
| Board file (`unfold-board`) | Teammates' apps, agents, and code review | `format` and `version` fields, plus a JSON Schema. Unknown fields are rejected |
| `.unfold/config.toml` | Teammates' apps | A `version` key. Unknown keys are rejected, with line and column |
| Canonical export | Tests and parity tools | Changes only with an explained snapshot update |
| IPC types | The UI | Generated from Rust and checked at build time |
| Graph database | Nobody else | Not a contract. Rebuilt whenever the engine version changes |

### 9.1 Board files

```json
{
  "folder": "Checkout",
  "format": "unfold-board",
  "pages": [
    {
      "cards": [
        {
          "at": [0, 0],
          "id": "checkout",
          "ref": { "entrance": "entrance:[\"express.route\",\"http\",\"POST\",\"/api/cart/checkout\",\"checkout\"]", "project": "storefront-web" }
        },
        {
          "at": [320, 0],
          "id": "post-orders",
          "ref": { "entrance": "entrance:[\"express.route\",\"http\",\"POST\",\"/orders\",\"requireUser\"]" }
        },
        {
          "at": [640, 0],
          "description": {
            "author": "person:you",
            "signature": "sha256:…",
            "text": "Places the order now, or queues it when the client asks for an async reply."
          },
          "folded": true,
          "id": "create-order",
          "ref": { "block": "src/routes/orders.ts#function:createOrder" },
          "view": "flow"
        }
      ],
      "defaultView": "flow",
      "groups": [],
      "guards": [],
      "id": "page-1",
      "labels": [
        {
          "author": "person:you",
          "key": "src/routes/orders.ts#function:createOrder/if[0]",
          "signature": "sha256:…",
          "text": "Did the client ask for an async reply?"
        }
      ],
      "links": [
        {
          "from": "checkout",
          "id": "checkout-calls-orders",
          "label": { "author": "person:you", "signature": "sha256:…", "text": "calls over HTTP" },
          "to": "post-orders"
        }
      ],
      "title": "Placing an order"
    }
  ],
  "projects": ["storefront-web"],
  "title": "Order lifecycle",
  "version": 1
}
```

(The example leaves its guards out for brevity; a saved board has one for every reference.)

- **A board holds pages** ([U-38](decisions-and-tradeoffs.md#u-38-a-board-holds-pages),
  [U-42](decisions-and-tradeoffs.md#u-42-the-board-format-pages-folders-and-other-projects)). Each page has its
  own cards, groups, Flow labels, guards, links (drawn links, and connections when they have a kind), frames,
  resource cards, and notes, which share one id space. A board may sit in a `folder`.
- **A card's `ref`** is exactly one of `block` or `entrance`, holding a structural key, and an optional
  `project`: the shared name of another project, absent for the board's own. `projects` lists exactly the other
  projects the page items name, sorted.
- **A description, label, or link label** has `text`, `author` (`person:<username>` or `agent:<name>`), and
  `signature`, which is `sha256:` followed by the hash of the exact selected source bytes. When the signature no
  longer matches, the description is marked outdated. A drawn link's label is signed over both ends, but never
  reported outdated; a connection that cites code has its label signed over that code, and it goes outdated with
  it.
- **Claims** (resource cards and connections) cite code: anchors (block or entrance keys, or lines of an evidence
  file), a table's type, a connection's step and carried type. They are signed over their type's, step's, and
  anchors' bytes like descriptions (a carried type is guarded, not signed), go outdated or missing when the code
  changes, and a claim an agent writes is grounded at save: what it names must appear in, or be used by, the code it
  cites ([reader: grounding](reader.md#grounding),
  [U-57](decisions-and-tradeoffs.md#u-57-resource-cards-and-connections-are-anchored-signed-grounded-claims)). A
  frame's description is signed over its member cards. New fields are optional and left out when empty, so the
  format stays version 1 ([U-61](decisions-and-tradeoffs.md#u-61-the-board-format-stays-version-1)).
- **A group** is a run of consecutive sibling statements with the same parent and body role, inside one card.
  Groups never cross a body boundary, overlap, or nest.
- **Writing** validates the whole board, serializes it deterministically (sorted keys, two-space indentation, final
  newline), and replaces the file atomically. Unknown fields are refused.
- **Reconnecting** after an update reuses surviving keys. A missing reference stays as a placeholder. An ordinal
  reference whose code changed is held for review. Renames and moves need a manual repair, which board health offers.
  A card from a project that isn't connected stays as a placeholder, unchecked.
- **The top Diagram/Flow/Code lens**, code folds, peeks, and each page's camera are view state and never go into the
  file.
- **Unpublished changes** of a published board live in a copy of it in `.unfold/local/drafts/<name>.changes.json`,
  which also names the file it publishes and the revision it started from.

[The reader](reader.md#boards) holds every rule of the format, and
[`schemas/unfold-board.schema.json`](schemas/unfold-board.schema.json) is its JSON Schema.

### 9.2 Project settings

```toml
# Unfold settings for this repository. Commit this file to share them.
version = 1

[project]
name = "storefront-api"                   # optional; the shared name other projects' boards use
languages = ["typescript", "python"]     # optional; default: every supported language found

[scope]
include = []                              # optional; when set, only these folders or globs are graphed
exclude = ["**/generated/**"]

[detectors]
disabled = []                             # for example ["node-cron.schedule"]

[[python.project]]
root = "py-playground"
source_roots = ["src"]
version = "3.12"                          # "3.12", "3.13", or "3.14" (the default)

# Entrances the detectors can't see, such as CLI dispatch or a signal handler.
[[entrance]]
kind = "command"
trigger = "import"
handler = "ts-playground/src/jobs/import-orders.ts#importOrders"

[[entrance]]
kind = "command"
trigger = "report"
handler = "ts-playground/src/cli.ts:12"

[[entrance]]
kind = "event"
trigger = "SIGTERM"
handler = "ts-playground/src/server.ts#shutdown"
```

| Key | Meaning |
| --- | --- |
| `version` | Settings format version, currently `1` |
| `project.name` | The project's shared name, which boards of other projects use for it. By default, the folder's name (for a repository's root, its main worktree's folder name). Changing it re-extracts nothing |
| `languages` | Limits the languages graphed |
| `scope.include`, `scope.exclude` | Glob patterns relative to the folder. The defaults always apply as well ([U-16](decisions-and-tradeoffs.md#u-16-discovery-and-scope-without-running-git)) |
| `detectors.disabled` | Detector IDs to turn off. `manual` can't be turned off |
| `python.project` | `root`, `source_roots`, and the grammar `version` of each Python project |
| `entrance` | `kind`; an optional `trigger` (an empty trigger is invalid); `method` for `http` only; an optional `label`; and either `handler` (one locator or a list) or, for `process`, `startup` |
| `evidence.files` | Glob patterns relative to the folder: files boards may cite that Unfold hashes but never compiles |

The app edits this file only through validated changes from Project settings (the name, scope, detectors, Python projects, and evidence files), the first Compile's scope, the manual entrance editor, and an agent's `update_project_settings`, and it preserves comments and formatting.

## 10. The reader

### 10.1 Derived views

These are computed by `unfold-reader` from the stored graph. Nothing here adds facts.

- **Owner.** The function, class, or file whose activation an occurrence belongs to. Walk up from the occurrence:

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

  Nested callbacks own their own calls. Decorators and Python header expressions belong to the enclosing scope.
- **Definition catalog.** Every function-like, class, or type declaration (interface, type alias, enum) block that has a name, or is the target of a resolved edge or entrance. Qualified names prepend the enclosing classes and callables. An unnamed callback is labeled after its callee.
- **Activation closure.** Starting from the seed targets, it adds the targets of resolved calls, constructions, imports, and references, plus the classes a file or callable owns. A branch seed considers only occurrences inside that branch. External and unresolved edges stop it. It is shown as "reached at least from".
- **Applications and membership.** An application is a distinct resolved startup among process entrances: a file, or the callable named by a Python console script. A non-process entrance belongs to every application whose closure contains its anchor, which is the owner of its evidence, or otherwise its first resolved target.
- **Relations.** Callers are found by reversing resolved edges and mapping each occurrence to its owner. Outgoing calls are grouped as already on the board, resolved and openable, or external or unresolved. Reached-from lists the non-process entrances whose closures contain the selected definition.
- **External and unresolved text.** External calls show their module. Unresolved calls show a short reason: "receiver type unknown", "parameter", "mutable binding", "not a concrete function", and so on. A guess never replaces either.

### 10.2 Cards

- An **entrance card** shows the declared entrance, its evidence, and its targets in registration order, without numbers.
- A **block card** shows one definition, or another indexed block, in one of three views: **Diagram**, one compact node (the kind's mark, the name, `file:line`, and the description), **Flow**, or **Code**. A card keeps its own view, else follows the page's lens, else the page's default view. An entrance card in Diagram view is a node too, marked with its kind (`HTTP`, `WORKER`, …) ([U-59](decisions-and-tradeoffs.md#u-59-the-diagram-view-and-type-cards)).
- A **type card** is a card of a type declaration (an interface, type alias, or enum) or a class. In Diagram view it lists its fields: each name as written, `readonly`, `?`, its written type (declared names open their definitions in place), a default (` = …`) or an enum member's value; an alias that isn't an object shows its aliased type, and inherited fields follow under the type they come from. A type declaration shows its fields in Flow too, since it has no paths; a class keeps its Flow. Code is the source.
- Cards open full and size themselves to their content: 360–880 px wide (a Diagram node 240–560, a type card 280–560), with longer lines soft-wrapping. A type card of twelve fields or more lists the first eight and "N more". Nothing scrolls inside a card; you pan and zoom the canvas instead.
- Opening an entrance adds its card and a full card for each resolved handler. Following a call opens the target 80 px to the right, with its header level with the calling line, or focuses the card if it is already open.
- Each opened call has its own arrow, leaving from a port at the end of the calling line, even when several calls go to one card. A Diagram node has no lines: its arrows leave from its edge, and its calls to one card merge into one arrow, labeled with the call's signature, or "N calls", which lists them in Selected, each opening its contract.
- A type card's field whose written type names another type card of the same project has a **type arrow** from its row to that card, labeled with the written type. Type arrows are compiled, drawn like call arrows, and Arrange places a type after the types that link to it.
- A call or passed-reference line carries its signature at its midpoint (`(CartItem[]) → Promise<Hold>`). Clicking the label opens the call's contract in its place: what the call sends (each argument beside the parameter it fills), what comes back and where it lands, and what the callee throws. A written type in it opens its definition inline, with "Peek source" and "Used in N places". While a line is selected, the callee card marks the lines the contract reads ([proposal](proposals/typed-lines.md)).
- A card folds to its title only when you fold it. Its arrows then attach to the header. Following a call into a folded card unfolds it.
- A **frame** is a named, described box around cards, resource cards, notes, and other frames, nested up to three deep: a **box** (outlined, with a title bar) for a unit of a few functions or a service, or a **zone** (a tinted area with a large title) for an area that holds services, with an icon from a fixed set (`api`, `worker`, `schedule`, `cli`, `web`, `library`, `system`). A frame has no position of its own: its box is what it holds plus room for its title, so dragging it moves them. Collapsed, it is one node that says what it holds ("3 functions · 1 table"), and the arrows and links of what it hides attach to it, merged ("3 calls"). Nothing collapses by itself. Agents draw frames; people collapse, rename, describe, move, and remove them ([U-56](decisions-and-tradeoffs.md#u-56-frames-are-a-page-layer-of-boxes-and-zones)).
- A **resource card** stands for what the code talks to that isn't a function: a table, a queue, a cache, storage, a service outside the workspace, or an actor. It is authored (the authored color, its kind's glyph and word, its name, its description, and the agent mark when an agent drew it) and looks the same in every view. A table backed by a type lists the type's fields as its columns, compiled, with the board's primary-key, foreign-key, and unique marks laid on; a table that lists its own columns shows them as written; up to 12 show, then "N more". Its anchors (block or entrance keys, or lines of an evidence file) are what it cites ([U-57](decisions-and-tradeoffs.md#u-57-resource-cards-and-connections-are-anchored-signed-grounded-claims)).
- A drawn link between two items becomes a **connection** when it has a kind: `http`, `rpc`, `queue`, `event`, `writes`, `reads`, `calls` (where a call the graph can't follow goes at runtime), `relates` (two tables' columns, with a cardinality), or `other`. Each kind has its own line in the authored color (solid, long dashes, dots, short dashes, thin with crow's-foot ends, dashed), a head where the data goes, and a chip with its kind's word and what travels (`queue · orders · OrderJob`), placed with the typed lines' labels so it never covers a card or another label. A connection leaves from its step's line in Code and its step in Flow, and from the card's edge in Diagram; clicking the carried type opens its definition. A connection is never compiled: it changes no Called by, Reached from, Flow, or dashed call. A link without a kind keeps the drawn link's dashed gray look.
- A **note** is up to 400 characters of plain text on a page, free or attached to a card, frame, resource card, or link (a drawn link or a connection), which it moves with. A note whose item is removed stays where it was.
- **Arrange** lays a page out by call depth from the inside out: each frame first, deepest first, to the right or downward as its direction says, then the page with each frame as one node, an arrow that crosses a frame's border counting at the cards it joins. Connections order their ends as data flows (a table after the code that writes it, before the code that reads it), each note attached to a card, resource card, or frame goes under its item, and each note on a link beside its line's middle, clear of the cards. It is deterministic: arranging again changes nothing.

### 10.3 Code view

- The exact verified slice of the source, with syntax colors.
- Each language crate computes highlighting in Rust. Python's comes from the parser's token stream. TypeScript's comes from oxc's lexer, or, if its token API isn't usable, from AST spans and comments plus a small keyword pass. The UI receives UTF-16 spans.
- Compound blocks (`if`/`else` chains, loops, `try`/`catch`, `switch`, callbacks, and Python `with` and `match`) are tinted, get an indent guide, and fold from the gutter. Python folds keep their written header. In a card longer than about 40 lines, callbacks and `else if` chains of three or more checks start folded.
- Calls are links at their written ranges:
  - hover to see the signature and location;
  - click to follow;
  - ⌥-click to expand the callee inline.

  External calls show their module, and unresolved calls have a dashed underline that explains why.
- Drag down the line-number gutter to make a group, then type its title on the magenta rail.

### 10.4 Flow view

Flow is labeled **Paths from written structure**. It is derived from saved blocks, roles, slots, and source order, never by re-parsing the source.

| Saved structure | Drawn as |
| --- | --- |
| Callable body | Start, the written statements, and End where a route reaches it |
| Statement, variable, call, construction | One node with its call chips. A main call can set the node's type |
| `if` and `else if` | A condition with yes and no lanes. A chain of three or more checks folds into one node with a "▸ N checks" tag |
| Loop | A header and body, with each, next, and done routes. A do-while tests after its body |
| `return`, `throw` | A terminal node. A return connects to End |
| Unlabeled `break` or `continue` | A transfer to the enclosing loop |
| TypeScript `try`/`catch` without `finally` | Separate regions with an on-error connection |
| Nested callback or class | A separate definition. The node lists it and it opens as its own card |
| TypeScript `switch` and `try`/`finally`; Python `with`, `match`, loops with `else`, and every `try`/`except` | An opaque region with a warning and no outgoing execution route |

Each node shows its compiler-derived type and a plain-language label, never raw code. The label is the board's authored label for that block. Without one, it falls back to a name taken from the code (a callee, a binding, or the names in a condition), drawn with a dotted underline, so you can see what still needs describing. Mermaid export uses the same labels as the app ([U-27](decisions-and-tradeoffs.md#u-27-the-reader-keeps-the-v71-rules)).

Flow is not a runtime trace or proof that a branch runs. Short-circuit expressions stay as written, labeled transfers get no guessed destination, and exceptions and asynchronous behavior aren't modeled.

### 10.5 Boards

- **Drafts.** Every board saves itself, a moment after each change, to its project's `.unfold/local/drafts/`. Each window's open boards come back with the window ([§11.10](#1110-several-windows-and-restore)).
- **Publish.** A draft's first Publish shows the file it will write, `.unfold/boards/<slug-of-title>.json`, and never overwrites an existing one; the name is editable only when a file already has it. The board is validated, written, and its draft removed. Later edits are kept as unpublished changes on this machine until the next Publish writes them over the published file, so `git diff` shows finished edits only. Unfold never runs Git.
- **Discard changes** returns to the published version. **Move back to drafts** reverses Publish. **Delete** asks first.
- **Export** writes one page as Mermaid to a location you choose, or to the clipboard, or as a PNG image of the page as the board draws it (unsaved changes included), drawn by a hidden render window at twice its size. SVG and a Markdown walkthrough come in V2. Mermaid uses solid arrows for calls and constructions, dashed arrows for imports and passed references, and dashed labeled arrows for drawn links. A Diagram-view card is a box with one arrow to each card it calls, and a type declaration in Flow view is a box.
- **Health** lists, per page, missing references, outdated descriptions, broken groups, guarded references that need review, cards from projects that aren't connected, drawn links whose end is missing, resource cards and connections whose cited code changed or is gone, and outdated frame descriptions, and offers repairs. A card or resource card shows its problems in a banner, a connection's chip a review dot, and Selected the repairs.
- **Agents** work through Unfold's local MCP server ([§13](#13-security-and-privacy), [desktop/README.md](../desktop/README.md#the-mcp-server)): they read the graph and boards through its tools, and the window that holds a board applies their edits, as one undo step each, saved at once and checked like any save ([U-54](decisions-and-tradeoffs.md#u-54-the-window-that-holds-a-board-applies-an-agents-edits)). What an agent writes is attributed `agent:<name>` and carries the agent mark; agents re-sign only what an agent wrote ([U-55](decisions-and-tradeoffs.md#u-55-the-agent-mark-agents-never-write-as-people-and-re-sign-only-agents-work)). Agents can still read and write board files directly, just like people: the app validates a board file when it loads it and reloads it when the file changes. The graph database is not an interface for agents.

### 10.6 Freshness

| What the file check finds | What the reader does |
| --- | --- |
| The source matches the facts' hash | Code and supported Flow are shown |
| The source changed (pending or last good) | Names and relationships stay navigable. The last verified render stays dimmed in memory, and old ranges are never applied to new bytes |
| The source is missing or unreadable | The metadata is kept and the card shows that the source is unavailable |
| Configuration evidence changed | The old evidence range and the membership claims that depend on it are withheld |
| Another file is unchanged | That file stays readable on its own |

Before any range is shown, the file's bytes are read, hashed, and compared with the stored hash. Only then are those same bytes decoded and sliced.

### 10.7 Reader rules

These come from the v7.1 reader and from review comments on the Paper designs. They hold in every view:

- Cards size to their content and never scroll inside. Every opened function is a full card with its own Diagram/Flow/Code switch.
- Calls in the code are the links, and an arrow leaves from the calling line. There is no per-card footer and no list of call chips.
- Code shows exact verified source with IDE syntax colors and folds from the compiled structure.
- There are no step numbers (1, 2, 2.1…) in any view.
- Flow looks like a Mermaid diagram. Each node shows its block type and a plain-language label, never raw code.
- A description is one quiet line with a **more** link. There are no heavy panels.
- A card folds to its title only when you fold it. Unfold never collapses anything by itself.
- Zoom is a camera and never changes what a card shows. Fit always shows the whole board.
- Nothing covers cards for long: there is no overview screen and no minimap, and floating chrome follows the overlay rule ([§11.3](#113-the-window)).
- The top Diagram/Flow/Code switch is a lens and never edits the board. A card's own switch is saved with the board.
- The sidebar's Selected section never shows code.
- The reader's colors are the Paper v7 tokens: an indigo accent (`#5862C0`), magenta for authored text, and the v7 syntax palette, in light and dark sets. The chrome around them has its own tokens ([U-51](decisions-and-tradeoffs.md#u-51-the-studios-visual-system)).

## 11. The desktop app

The window is a studio, laid out like the Paper desktop app with repositories in place of design files
([U-26](decisions-and-tradeoffs.md#u-26-a-studio-window)). The canvas gets most of the window,
projects are first-class, and every job sits on the thing it describes. [The UI shell](ui-shell.md)
describes the window as built, part by part, with the keyboard map; the [user guide](user-guide.md)
says how to use it.

### 11.1 Workspace, projects, and boards

| Thing | What it is | Where it's stored |
| --- | --- | --- |
| **Workspace** | Everything connected on this machine, one per person. Home lives here | App data: `workspace.json` |
| **Project** | A folder with its own graph, usually a repository. Shows its branch and switches worktrees in place | The folder itself, with its `.unfold/` |
| **Folder** | Yours, to organize a project's boards, like folders in Paper. Not the disk layout | A field on each board |
| **Board** | A story about the code, such as "Order lifecycle": a draft or published. Can hold cards from other projects | `.unfold/boards/` (published) or `.unfold/local/drafts/` |
| **Page** | One canvas inside a board: one trace, such as "Placing an order" | Inside the board file |
| **Card** | An entrance, a definition, or a type, in Diagram, Flow, or Code | Inside a page |

Projects hold code. Boards hold stories ([U-38](decisions-and-tradeoffs.md#u-38-a-board-holds-pages)).

- **A project** is a folder Unfold has connected, with its own `.unfold/`
  ([U-40](decisions-and-tradeoffs.md#u-40-a-project-is-a-folder-with-a-shared-name)). A subfolder of a
  monorepo can be a project of its own. The workspace knows each project by a random id (`p-` and ten
  base-32 characters) that survives restarts and worktree switches, and holds at most 64 of them.
  Adding a folder writes nothing into it; removing one from the workspace never touches its `.unfold/`.
- **A project's shared name** is how boards of other projects name it: `[project] name` in its
  `.unfold/config.toml`, by default the folder's name (for a repository's root, the name of its main
  worktree's folder, so every worktree shares it). A name resolves to the first connected project with
  that name, in workspace order.
- **Branches and worktrees.** Unfold reads Git's files (`.git/HEAD` and the worktree list), never runs
  Git, and shows one row per repository with a switcher of its worktrees. Each worktree is its own
  folder with its own `.unfold/`, so switching changes the graph, the published boards, and the drafts
  ([U-41](decisions-and-tradeoffs.md#u-41-branches-and-worktrees-from-gits-files)).

The window follows seven principles:

1. **One sidebar, and it follows you.** On Home it holds projects and their boards. Inside a board it
   holds pages, the cards on the page, and the selected card. Never two at once, and no right-hand
   inspector.
2. **State lives on the thing it describes.** Graph state on the project, health on the card, publish
   state on the board. There is no status bar.
3. **Finding code is adding it.** One **Add** box finds entrances, functions, files, and text in every
   connected project and puts what you pick on a page. The same box runs commands.
4. **Home answers "how does it start?"** A project opens on its entrances, grouped by application, and
   then the boards people made about it.
5. **A trace doesn't stop at a repository.** A page can show the web app's checkout handler next to the
   API route it calls.
6. **Nothing covers cards for long** ([§11.3](#113-the-window)).
7. **The engine guarantees don't change** ([§2](#2-guarantees)).

### 11.2 Live and resting projects

Watching every connected project all the time would break the memory budget, so a project is **live**
only while something needs it, and **rests** otherwise
([U-45](decisions-and-tradeoffs.md#u-45-live-and-resting-projects)). Rust owns this; the engine host
knows nothing about projects.

- **Interest.** A project is needed while a window holds one of its boards, while a window has it
  selected on Home, while an open board of any project has cards from it, and while a Compile of it
  runs.
- **Waking.** A command that uses a project's engine wakes it, and so does gaining interest. Waking
  opens its workspace ([§7.8](#78-opening-a-project)): the stored graph is readable at once, and the
  check against the files on disk runs in the background. A wake that fails (the folder is gone, or
  another copy of Unfold has it open) marks the project **unavailable**.
- **Resting.** Every wake and every loss of interest extends the project's lease to five minutes from
  then. Once nothing needs it, its lease has run out, and its engine is idle, it rests: its last state
  is written to `workspace.json`, and its engine thread, watcher, lock, and in-memory program are
  closed. One thread checks every project each 30 seconds.
- **What a resting project answers.** Lists and searches never wake a project: boards, drafts,
  entrances, search, and text search read its files and its stored graph read-only, and its row shows
  its last known state. Using a result (opening a card, a preview, a file, or a board) wakes it
  ([IPC](ipc.md#how-a-command-travels) marks each command `wakes` or `rests`).

### 11.3 The window

`⌂` is the Home tab, `S` a project's monogram, and the dock floats at the bottom of the canvas.

```text
┌────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ⌂ Home │ S Order lifecycle × │ S Worker retries │                                              │
├─────────────────────────────┬──────────────────────────────────────────────────────────────────┤
│ ‹ storefront-api / Checkout │ Placing an order · Published · 2 unpublished changes             │
│ Order lifecycle         ⋯   │                  [ Diagram | Flow | Code ]     Share ▾   Publish │
│                             ├──────────────────────────────────────────────────────────────────┤
│ PAGES                    +  │                                                                  │
│   ▣ Placing an order        │   ┌──────────────┐    ┌───────────────────┐                      │
│   ▣ Refunds              ●  │   │ POST /orders │───▶│ ƒ createOrder     │                      │
│ ON THIS PAGE    follow order│   │ Handlers,    │    │  const draft = …  │                      │
│   ↳ POST /orders            │   │ in order     │    │  if (req.header(  │                      │
│     ƒ createOrder           │   └──────────────┘    └───────────────────┘                      │
│       ƒ OrderService.place  │                                                                  │
│ SELECTED                    │                                                                  │
│   createOrder               │                                                                  │
│   routes/orders.ts:25       │  ┌─────────────────────────────────────────────────────────────┐ │
│   Places the order, or      │  │ Add A  ‹ ›  POST /orders › createOrder  Arrange − 92% + Fit │ │
│   queues it.                │  └─────────────────────────────────────────────────────────────┘ │
└─────────────────────────────┴──────────────────────────────────────────────────────────────────┘
```

- **Tabs.** The top of every window is a tab strip: Home, pinned first and never closed, then the open
  boards, each with its project's monogram and title.
- **The sidebar has two modes.** On Home it holds the workspace: Recents, Drafts, and the projects,
  each with its state indicator and its tree of folders and boards. In a board tab it holds that board:
  its header (back to Home, the project and folder, the title, and the board menu), its **Pages**,
  **On this page** (the cards in follow order, indented by call depth, with their groups and drawn
  links, and frames as headings over what they hold), and **Selected** (the selected card, Flow node,
  group, link, or frame: its description, Called by, Reached from, and calls not opened yet; a frame's
  members and the links in and out of it). It is 264px wide by default, resizes from 220 to 360px,
  and ⌘B hides it.
- **The canvas top bar** shows the page's title and the board's publish state ("Draft", "Published",
  "Published · 2 unpublished changes"), the window's **Diagram | Flow | Code** lens in the middle, and **Share**
  (Export Mermaid, Copy Mermaid, Export PNG) and **Publish** on the right.
- **The dock** floats at the bottom of the canvas: **Add**, Back and Forward, the follow trail,
  **Arrange**, and the zoom (−, percentage, +, Fit).
- **Sheets and dialogs.** The peek sheet slides over the right of the canvas
  ([§11.7](#117-the-peek-sheet)). Board dialogs (Publish, Discard changes, Delete) center on the canvas
  with a scrim over it; Add, project settings, and Preferences center on the window.
- **Toasts** appear at the bottom right: a project that finished compiling, an action that failed, the
  engine reconnecting.

**The overlay rule** ([U-50](decisions-and-tradeoffs.md#u-50-the-overlay-rule)): the dock and the lens
float at the canvas's edges and never cover a card after Fit; sheets and dialogs cover it only while
they are open, and Escape closes them. While the peek sheet floats over the canvas, the lens, the dock,
and banners center on the part of the canvas it leaves uncovered.

### 11.4 Home

Home is the first tab of every window. It shows the project selected in its sidebar, and each window
has its own selection.

- **First launch.** With no projects, Home shows a line about what Unfold does, **Add project…** (⌘O),
  a drop zone (a folder dropped anywhere on the window is added too), and the folders removed from the
  workspace recently ("Opened before"), each one click from coming back.
- **A project that hasn't been compiled** shows what a Compile would do: how many TypeScript and Python
  files it would graph, what it skips and why, any settings or discovery problems, where the graph will
  be kept, the promise ("Reads files only. Runs nothing and installs nothing. Creates `.unfold/` in this
  folder for the graph and your boards."), and the **scope tree**: the top-level folders with check
  boxes, so a large monorepo can graph part of itself. Nothing is written until **Compile** (⇧⌘B), which
  first saves the chosen scope to `.unfold/config.toml`. Home fills in as the Compile finds entrances.
- **A compiled project** shows:
  - **the header:** the path, the name, the project menu, Settings, **New board**, the branch and
    worktree switcher, and the state line ("Graph current · 1,284 files · TypeScript + Python · updated
    2 min ago", [§7.9](#79-what-a-projects-state-says));
  - **How it starts:** the applications, named after how they start, each with its first eight
    entrances, a count, and a filter. Routes are ordered by their first path segment. **Trace** on an
    entrance opens it on a new draft board. Below them, collapsed: "Not reached from a launch" and
    "Launches into code that isn't indexed";
  - **Boards:** a thumbnail of each board's first page, drawn from card positions (not a screenshot),
    with All, Published, and Drafts filters, sorted by last edited or by title;
  - **Health:** one line summing up what needs a look (files that don't parse, a settings error,
    boards that need review), each with its action. **Health and activity** opens the full Problems
    list and the index log of the last updates.
- **Recents and Drafts**, in the sidebar, show boards across every project as the same grid:
  recently opened boards, and every project's drafts and published boards with unpublished changes
  ([U-49](decisions-and-tradeoffs.md#u-49-drafts-are-stored-per-project-and-listed-together)).

After the first Compile there is no Compile button in the chrome: the graph keeps itself current
([§7](#7-compiling-and-keeping-the-graph-current)). Compile again, Rebuild from scratch, Cancel, Pause or
Resume background updates, Settings, Add entrance, Reveal in Finder, Health and activity, Copy
diagnostics, and Remove from workspace are in the **project menu** (`⋯` on the project's row and on its
Home).

### 11.5 Reading a board

Opening a board opens it in a tab and switches the sidebar to it. A board starts with one page; **+**
under Pages adds another. Following calls, ⌥-click inline expansion, Diagram, Flow, and Code, folds, groups,
frames, descriptions, and Flow labels work as [§10](#10-the-reader) describes. Descriptions and labels are
written in **Selected** or on the card itself; "N Flow steps unlabeled · Label them", under On this
page, steps through the nodes that still show code names. **Show callers**, in a card's menu, adds the
cards of the definitions that call it to its left. A frame's chevron collapses and expands it; its title
renames it in place (double-click), drags it with everything in it, and opens its menu (Rename…,
Describe…, Collapse or Expand, Copy link, Remove frame (keep cards)). A card that needs review shows a banner ("Changed
since it was described · Review") that lists its problems and their repairs, and its page and its row
on the page carry a mark.

Every edit is an undo step of the board, whichever page it changed, and undo shows that page again.
Each page remembers its own camera and lens.

### 11.6 Add

**Add** (`A`, ⌘K, or `/`) opens one box over the window
([U-30](decisions-and-tradeoffs.md#u-30-add-finds-and-runs-everything-across-projects)):

- **Before you type** it shows a map: every connected project's applications and their entrances, with
  the ones already on the page marked and, on hover, where else they are on boards. **Look in** limits
  it to one project.
- **When you type** it searches entrances, functions, files, and text in every connected project, with
  a tab and a count for each kind; text search has match case, whole word, and regular expressions.
  The right side previews the selection: its location, its first lines, Reached from, Calls, and Called
  by.
- **Enter** adds the selection to the current page, **⌘Enter** adds it on a new page, and **Tab**
  switches the kind. From Home, Enter starts a new draft board in the result's project. A file opens in
  the peek sheet, and a text match opens it at its line.
- **Typing `>`** lists commands: every action of the window, by group, with its shortcut. ⇧⌘P opens
  Add in this mode, ⌘P on Files, and ⌘T on Functions.

### 11.7 The peek sheet

A source file opens in a **peek sheet**, 640px wide, that slides in from the right over the canvas
([U-48](decisions-and-tradeoffs.md#u-48-whole-files-in-a-peek-sheet)): the whole file, read-only, with
syntax colors, folds, calls as links, and a `⊕` beside each definition that adds it to the page (on
Home, to a new draft). A click on a resolved call adds the callee. A file that changed since the graph
was built is shown without `⊕`, folds, or links. A pin keeps the sheet open while you work, and the
canvas narrows instead of being covered; Escape closes it, pinned or not. Its menu has Open in editor
(⇧⌘O) and Reveal in Finder. A window has one sheet, kept across tab switches.

### 11.8 Drafts, publishing, and unpublished changes

- **A new board is a draft.** It lives only on this machine, in its project's `.unfold/local/drafts/`,
  and saves itself a moment after each change.
- **Publish** (⇧⌘E) writes it to `.unfold/boards/<name>.json`, where it is committed like code. The
  name is the slug of the title, editable when a file already has it, and never changes afterwards:
  Rename changes the title only.
- **Edits to a published board** are kept on this machine as **unpublished changes** until you press
  **Publish** again, so teammates never see half-finished edits in `git diff`
  ([U-43](decisions-and-tradeoffs.md#u-43-edits-to-a-published-board-wait-for-publish)). **Discard
  changes** returns to the published version. If the published file changed since you started (a pull),
  Publish asks whether to keep your version or take theirs and drop your changes.
- **The board menu** has Rename, Move to folder, Duplicate, Open in new window, Share, Fold all and
  Unfold all cards, Discard changes, Move back to drafts, and Delete. Folders, made and renamed from the
  sidebar's tree, organize a project's boards
  ([U-47](decisions-and-tradeoffs.md#u-47-folders-are-a-field-on-the-board)).

The files and the conflict rule are in [§9.1](#91-board-files) and [the reader](reader.md#files-drafts-publish-and-unpublished-changes).

### 11.9 Boards across projects

A page can hold cards from any connected project, for example `POST /api/cart/checkout` in
`storefront-web` next to `POST /orders` in `storefront-api`
([U-44](decisions-and-tradeoffs.md#u-44-boards-that-span-projects-with-drawn-links)). The board lives
in the project it was created in, and its file names the other projects by their shared names. A
teammate who hasn't connected one of them sees its cards as placeholders ("From storefront-web · not
connected", with **Connect project**).

The graph doesn't link code across projects: a `fetch("/api/orders")` in the web app is not an edge to
the API's route. So a link between cards of two projects is **drawn by you** (Draw link… in a card's
menu): a dashed arrow, with an optional label such as "calls over HTTP" that is signed like a
description. Health checks only that both ends still exist. Any two cards, resource cards, or frames of a page can be
linked, and Kind ▸ makes a link a connection (`http`, say), which cites code on both sides and is checked like a
description. Detecting such links automatically is a later step.

### 11.10 Several windows and restore

You can open as many windows as you want ([U-37](decisions-and-tradeoffs.md#u-37-several-windows-one-window-per-board)).

- **Every window shows the whole workspace**, with its own tabs, sidebar, Home selection, and peek
  sheet. ⇧⌘N opens another; **Open in new window**, on a tab or in the board menu, moves a board, with
  its unsaved edits and undo history, into a window of its own.
- **A board is open in one window at a time.** Opening it elsewhere focuses the window and the tab that
  have it.
- **Windows share the engine.** Every window talks to the one engine host, so a save updates the cards in
  every window that shows them, and every project row shows the same state everywhere.
- **Windows come back.** `windows.json` remembers each window's position, size, Home selection, tabs
  (with each board's page), and layout. Every window open when the app quits comes back on launch, moved
  onto a visible screen if a display has gone; a window you close is forgotten, unless it was the last.
- **Closing windows.** A closing window's last saves land before its boards are released
  ([IPC](ipc.md#closing-writes)). On macOS, Unfold keeps running with no windows, as Mac apps do. On
  Windows and Linux, closing the last window quits.
- **What a window costs.** Each window is its own Chromium renderer, so each adds memory
  ([§17](#17-performance-targets)). The graph is never copied into a window: each one fetches only the
  cards and pages it shows.

### 11.11 Background updates, health, and the engine

- **One indicator per project row** ([§7.9](#79-what-a-projects-state-says)): nothing when the graph is
  current, a progress ring while compiling or updating, an amber dot for problems, a pause glyph, a
  hollow circle before the first Compile, a red triangle when the last Compile failed, and a moon for a
  resting project. Home's state line says the same in words.
- **Health on the card:** a banner on the card, a mark on its row under On this page, and a mark on
  its page. Boards that need review are listed in Home's Health line, never as a project problem.
- **If the engine host stops**, every window shows a toast ("Reconnecting to the engine…") and keeps
  its boards. The main process restarts the engine host, every project starts resting, and each window,
  once reconnected, selects its project again, claims its board tabs again, saves its unsaved edits, and
  refetches what it shows.

### 11.12 Settings and preferences

- **Project settings** (the project menu, or Settings on Home) open as a dialog with five sections:
  **General** (the shared name, the folder, background updates, and where the graph is stored),
  **Scope** (a check-box tree of the project's folders, with skipped folders shown and why, and the
  scope rules), **Detectors** (one switch per detector, grouped by language), **Python projects**, and
  **Evidence files** (the files boards may cite, each with Remove, and a field that adds one).
  Changes are staged and saved together into `.unfold/config.toml`, all or nothing, through Rust, which
  keeps comments and formatting ([settings and scope](settings-and-scope.md)).
- **Manual entrances** are added with **Add entrance…** (the project menu, or How it starts on Home) or
  **Add as entrance…** in a function card's menu), and edited or removed from their row on Home.
- **Preferences** (⌘,, or the gear at the bottom of Home's sidebar) are app-wide: theme, editor command,
  background updates for newly compiled projects, the compile thread limit, update checks, whether agents
  may connect (and on which port), and key bindings.

### 11.13 Keyboard

The keyboard map and the dispatcher's rules are in [the UI shell](ui-shell.md#keyboard). Every
action in the map can be rebound in Preferences, and the application menu runs the same actions as the
keys. Windows and Linux use Ctrl in place of ⌘.

### 11.14 UI technology, and what runs where

- Chromium, which Electron bundles, renders the UI on every platform, so cards, Code, and Flow lay out the
  same everywhere. The UI and code fonts (Inter and JetBrains Mono) are bundled too, so text measures the
  same on every machine.
- React 19 and strict TypeScript, built with Vite.
- React Flow (`@xyflow/react`) for the canvas. It renders only visible cards on large boards, and edges
  and ports are memoized.
- TanStack Query for data from Rust, invalidated in each window by the events of the project they concern.
- Zustand with Immer for each window's state: its tabs, layout, Home selection, overlays, and its open
  boards with their pages, view state, and undo and redo history.
- TanStack Virtual for long lists: the sidebar's tree, Add's results, the peek sheet's lines, and Health.
- Design tokens as CSS variables, in a light set and a designed dark set
  ([U-51](decisions-and-tradeoffs.md#u-51-the-studios-visual-system)), with styles kept beside each
  component.
- **Rust computes everything that needs the graph or the source bytes**: the code render model in UTF-16,
  highlighting, folds, links, the Flow model, relations, closures, health, thumbnails' contents, search,
  and Mermaid.
- **TypeScript does layout**, because layout needs measured sizes: card sizing, reflow, frames' boxes,
  Arrange by call depth (frames from the inside out), arrow and link routing, thumbnails' rectangles, and
  the placement of nodes inside Flow.
- **The UI uses no Node APIs and no browser storage.** It reaches the rest of the app only through
  `window.unfold` ([§12](#12-commands-and-events)), and everything it remembers goes through Rust.

## 12. Commands and events

Commands, events, and their payload types are defined once in Rust, in `unfold-workspace`'s command table. `cargo xtask gen-ipc` generates their TypeScript types with ts-rs, and `tsc` checks the UI against them at build time ([U-25](decisions-and-tradeoffs.md#u-25-typed-commands-generated-from-rust)). Queries filter in Rust and return pages, so the UI never loads every definition at once. [IPC](ipc.md) is the catalogue: every command and event, their rules, error codes, and limits.

**How a command travels.** The UI calls `window.unfold.invoke(command, args)`. The preload script sends it over the window's port to the engine host, which hands it to the addon's `dispatch` function; the command runs on a Rust thread, and its result comes back the same way, as a promise. Window commands (new windows, dialogs, the clipboard) go to the main process instead. Streams, such as compile progress and text search, arrive as messages tagged with their request, and can be cancelled.

**Commands name their project.** Every window shows the whole workspace, so a window is attached to nothing: every command about a project takes its `project` id, and every path it names is relative to that project's folder ([U-39](decisions-and-tradeoffs.md#u-39-one-window-shows-the-workspace-commands-name-their-project)). Searching across projects is one command that fans out in Rust. Each command either wakes its project or is answered from disk while the project rests ([§11.2](#112-live-and-resting-projects)).

**Events go to every window.** Each project event names its project, and each window refreshes only what it shows of that project. Only `board_conflict` goes to one window, the one holding the board.

**The addon's surface** is small and rarely changes. Adding a command touches only Rust and the generated types:

| Function | Does |
| --- | --- |
| `init` | Starts the engine with the app's folders; every project starts resting |
| `subscribe` | Delivers events and stream messages from Rust threads to the engine host, in order with the results they precede |
| `dispatch` | Runs one command for one window, and resolves with its result |
| `cancel` | Cancels a running command or stream |
| `detachWindow` | A window closed: waits for its last board and window-state writes, then releases its boards and its Home selection |
| `shutdown` | The app is quitting: closes every live project |

**Commands**, by area ([IPC](ipc.md#commands) lists each one):

| Area | What the UI asks for | Handled by |
| --- | --- | --- |
| [Workspace](ipc.md#workspace) | The projects and their states, adding and removing projects, the Home selection, branches and worktrees, drafts across projects | Rust (adding a project from a dialog goes through the main process) |
| [Project, Home, settings](ipc.md#project-home-and-settings) | Home's summary, the not-compiled preview and scope tree, entrances by application, Problems and the index log, Compile, background updates, project settings, preferences, diagnostics | Rust |
| [Files and graph](ipc.md#files-and-graph) | A file for the peek sheet, cards, call peeks, relations, callers, previews, where a key is on boards | Rust |
| [Search](ipc.md#search) | Names and text across projects | Rust |
| [Boards](ipc.md#boards) | Listing, thumbnails, opening and claiming, saving, health and repairs, Publish, unpublished changes, folders (and, for agents' tools, reading a board without claiming it) | Rust |
| [Export](ipc.md#export) | Mermaid to a file or the clipboard | The main process shows the save dialog, then Rust writes the file |
| [Windows](ipc.md#windows) | What each window remembers | Rust |
| [Window commands](ipc.md#window-commands-and-shell-events) | New windows, focusing, dialogs, moving a board to a new window, the clipboard, Export PNG and the render window's answers, view reports, answers to agents' requests, and the agents' server state | The main process |

Each board command names its board, and Rust checks that the calling window holds it. Commands that take an absolute path, or manage windows, run only when the main process sends them ([§13](#13-security-and-privacy)).

## 13. Security and privacy

- **Windows get no file, shell, process, or network access.** Every renderer runs with context isolation and Chromium's sandbox, with Node integration off. Its preload script exposes one object, `window.unfold`, which can only send Unfold's commands and receive its events.
- **The main process trusts only what it created.** It loads the UI only from Unfold's own `app://` protocol, inside the app's signed archive. It blocks navigation, new windows, embedded webviews, and every permission request, such as the camera or notifications, and it checks the sender of every message it handles.
- **The render window is a window like the others, with less it may do.** Page images (Share › Export PNG, an agent's screenshot) and page checks are drawn by a hidden window the main process creates on demand with every window's hardening, offscreen, loading the same `app://` UI under the same policy. It is attached to the engine for the cards' data like any window, but never opens or claims a board, and the one message it may send the main process is its answer to a render request. The main process writes the exported image to the path the person chose in the save dialog ([U-60](decisions-and-tradeoffs.md#u-60-page-images-from-a-hidden-render-window-and-page-checks)).
- **Paths outside a project come only from the main process.** A folder picked in the folder picker, dropped on a window, or passed on the command line, and a file chosen in a save dialog, reach Rust only as trusted commands, which only the main process can send ([IPC](ipc.md#how-a-command-travels)). A window can never name a path outside a project; it can add back a folder the workspace removed only by its listed path.
- **One local listener, for agents, off by default.** While Preferences › Agents has it on, the main process serves MCP at `http://127.0.0.1:<port>/mcp` (port 29180 unless the person picks another) for agents such as Claude Code and Codex. It listens on the loopback interface only; it refuses, with a bare 403, any request that carries an `Origin` header (a web page) and any whose `Host` isn't `127.0.0.1` or `localhost` at its port (DNS rebinding); it caps request bodies at 1 MiB and serves nothing but `/mcp`. It has no token: while it is on, any program on the machine can use it, and such programs can already read the repositories. Its engine requests are untrusted, like a window's, and its tools are fixed actions with no pass-through to the engine; the one that names a path outside a project, adding a project, runs only after the person confirms it in a native dialog. Turning it off closes every connection ([U-52](decisions-and-tradeoffs.md#u-52-one-mcp-server-in-the-main-process-on-a-fixed-local-port-with-no-token), [U-53](decisions-and-tradeoffs.md#u-53-the-mcp-sdks-v2-server-stateless-with-no-sessions)).
- **Every path a command receives is checked, per project.** It must be relative to the folder of the project the command names, and Rust resolves it and rejects `..`, absolute paths, and symlinks that escape that folder. Discovery doesn't follow symlinks out of the folder.
- **Strict content security policy.** All assets are local, and nothing is loaded from the network. Source and descriptions are rendered as text, never as HTML.
- **The bundled runtime runs only Unfold.** Electron's fuses are set when the app is packaged: it can't be started as plain Node, it ignores `NODE_OPTIONS` and inspector flags, and it loads its code only from its own archive, whose integrity is checked on macOS and Windows.
- **Unfold never runs the indexed code**, installs dependencies, runs Git or package scripts, or writes outside `.unfold/`, apart from its own app folders and files you export.
- **Electron stays current.** Unfold ships a current stable Electron and never falls outside the three major versions the Electron project supports, because each one carries Chromium's security fixes.
- **No telemetry in V1.** The only network call is the update check, which can be turned off. Chromium's spellchecker is off, because on Windows and Linux it downloads dictionaries. Logs stay on the machine, and **Copy diagnostics** puts a redacted summary on the clipboard for bug reports.

## 14. Packaging, platforms, and updates

| Platform | Architectures | Package | Signing | Release |
| --- | --- | --- | --- | --- |
| macOS | Apple silicon and Intel, as separate builds | `.dmg`, plus a `.zip` that the updater uses | Developer ID, notarized | V1 |
| Windows 10 and 11 | x64 | NSIS installer | Authenticode | V1 |
| Linux | x64 | AppImage, `.deb`, and `.rpm` | Checksums published with each release | V1 |
| Windows and Linux on Arm | arm64 | As above | As above | V2 |

- **All three platforms ship in V1** ([U-35](decisions-and-tradeoffs.md#u-35-platforms-macos-windows-and-linux-in-v1)). They are built and tested in CI from the first commit, and a release goes out only when the journeys pass on all three.
- **Builds** use electron-builder, which also sets the Electron fuses ([§13](#13-security-and-privacy)). napi-rs builds `unfold-node` for each platform and architecture. The addon is unpacked beside the app's archive, as native modules must be, and ships inside the signed app.
- **Updates** use electron-updater with GitHub Releases. Every download must match the SHA-512 hash in the release's manifest, and on macOS and Windows an update installs only if it carries our code signature. The AppImage updates itself; `.deb` and `.rpm` installs show a notice that links to the new package. Updates can be turned off.
- **Electron upgrades** arrive as ordinary pull requests that must pass every gate. The addon uses Node-API, which is ABI-stable, so it doesn't need rebuilding for each Electron version.
- **The Mac App Store and the Microsoft Store are not planned.** The Mac App Store's sandbox would need security-scoped bookmarks for folder access.
- **The bundle identifier and the Windows app ID** are chosen together with the trademark check of the name Unfold ([U-3](decisions-and-tradeoffs.md#u-3-names-and-words)).

## 15. Repository layout

### 15.1 The tree

```text
unfold/
├── README.md                       # what Unfold is, the docs, and the proof of concept it rebuilds
├── Cargo.toml                      # Rust workspace: crates/*, tools/xtask
├── rust-toolchain.toml             # pinned stable Rust
├── package.json                    # pnpm workspace root: desktop, ui, e2e, tools/oracles
├── pnpm-workspace.yaml
├── justfile                        # just dev | test | check | e2e | bench | bundle | reference | oracles
├── deny.toml                       # cargo-deny: licenses and advisories
├── .github/workflows/              # pr.yml (all three platforms), nightly.yml, release.yml
├── docs/
│   ├── README.md                   # map of the docs
│   ├── architecture.md             # this document
│   ├── decisions-and-tradeoffs.md  # every decision, with the options weighed
│   ├── graph-model.md              # blocks, slots, edges, entrances, keys, invariants
│   ├── compiling.md                # the update entry point, the facts cache, the workspace lifecycle
│   ├── settings-and-scope.md       # .unfold/config.toml, discovery, scope
│   ├── entrances.md                # detectors, launch grammar, manual entrances, locators
│   ├── languages/
│   │   ├── typescript.md           # the TypeScript front end: mapping, evidence, modules, known limits
│   │   └── python.md               # the Python front end: mapping, binder rules, known limits
│   ├── reader.md                   # derived views, Code, Flow, boards, Mermaid
│   ├── ipc.md                      # every command and event
│   ├── ui-shell.md                 # the window as built: modules, state, keyboard, tokens
│   ├── user-guide.md               # using the app
│   ├── testing.md                  # test kinds, gates, snapshot policy, budgets
│   └── schemas/                    # unfold-board.schema.json
├── crates/
│   ├── unfold-core/                # graph model and vocabularies, structural keys, text (snapshots,
│   │                               #   SHA-256, line index, UTF-8/UTF-16 mapping), validator,
│   │                               #   canonical export, test helpers behind the `testkit` feature
│   ├── unfold-engine/              # Language and Linked traits; discovery and scope; the shared
│   │                               #   backend (selection, roles, names, slots, value policy,
│   │                               #   reason order, module labels, provenance, edges, entrance
│   │                               #   assembly, locators, launch grammar); update(), facts cache,
│   │                               #   relink, diff
│   ├── unfold-lang-typescript/     # oxc adapter, binding and evidence, tsconfig groups, emit map,
│   │                               #   generated lib tables, package scripts and bins, Express,
│   │                               #   BullMQ, node-cron, highlighting
│   ├── unfold-lang-python/         # Ruff parser adapter, binder, imports and source roots, C3 and
│   │                               #   receivers, decorators, pyproject scripts, FastAPI, Celery,
│   │                               #   APScheduler, highlighting
│   ├── unfold-store/               # SQLite schema, row diffs, queries, FTS5, the facts cache table
│   ├── unfold-reader/              # owner, catalog, closure, applications, relations, Flow model,
│   │                               #   code render model, Mermaid; boards: format, JSON Schema,
│   │                               #   validation, signatures, guards, health, drafts, publishing
│   ├── unfold-workspace/           # the projects registry (workspace.json, windows.json, live and
│   │                               #   resting projects, Git's files); one live project's workspace
│   │                               #   (.unfold/ layout and fallback, settings, watcher, scheduler,
│   │                               #   cancellation, events, lock); search and text search across
│   │                               #   projects; the command table; board sessions and claims
│   └── unfold-node/                # the napi-rs addon: init, subscribe, dispatch, cancel,
│                                   #   detachWindow, shutdown; no logic of its own
├── desktop/                        # the Electron app, in TypeScript, with no graph logic of its own
│   ├── package.json
│   ├── electron.vite.config.ts     # builds the main process, the preload script, and the engine host
│   ├── electron-builder.yml        # packages, signing, notarization, fuses, updater
│   ├── build/                      # icons and macOS entitlements
│   └── src/
│       ├── main/                   # index.ts, windows.ts, workspace.ts, window-placement.ts, menu.ts,
│       │                           #   dialogs.ts, ipc.ts, engine-host.ts, protocol.ts, security.ts,
│       │                           #   updater.ts; agents/ (the agent bridge); mcp/ (the MCP server)
│       ├── preload/                # index.ts: window.unfold, and nothing else
│       ├── engine-host/            # index.ts, router.ts, binding.ts: loads unfold-node, routes each
│       │                           #   window's port, broadcasts events
│       └── shared/                 # transport.ts, unfold-api.ts: the wire types and window.unfold;
│                                   #   agent-api.ts, render-api.ts: the agent and render protocols
├── ui/                             # React app, built with Vite
│   ├── package.json
│   ├── vite.config.ts
│   └── src/
│       ├── main.tsx
│       ├── app/                    # the window frame: tab strip, sidebar container, content area,
│       │                           #   window-level layers, wiring, folder drops
│       ├── home/                   # Home: its sidebar, first launch, not compiled, overview, boards
│       │                           #   and thumbnails, health and activity
│       ├── board/                  # a board tab: its sidebar (Pages, On this page, Selected), the
│       │                           #   canvas top bar, the dock
│       ├── canvas/                 # React Flow canvas, cards, frames, resource cards, notes, arrows,
│       │                           #   drawn links and connections, placeholders, Arrange, Show callers
│       ├── code/, flow/            # the Code and Flow renderers
│       ├── add/                    # Add: the map, search across projects, preview, commands
│       ├── peek/                   # the source peek sheet
│       ├── settings/               # project settings, the entrance editor, diagnostics
│       ├── preferences/            # Preferences and key bindings
│       ├── dialogs/, menus/        # Publish and the other dialogs; board, project, and worktree menus
│       ├── ipc/                    # generated command types (never edited), the client, queries,
│       │                           #   invalidation by project, streams
│       ├── keyboard/               # the action registry, keymap, dispatcher, bindings, menu commands
│       ├── state/                  # per-window stores: workspace, window, tabs, layout, overlays,
│       │                           #   compile, session, open boards with undo
│       ├── components/, icons/     # the shared kit and the icon set
│       ├── lib/                    # pure helpers
│       └── theme/                  # tokens (light and dark), base styles, bundled fonts
├── testdata/
│   ├── fixtures/                   # small exact cases: worked, classes, entrances, unicode, failures,
│   │                               #   Python policy cases
│   ├── detectors/                  # <detector>/<case>/ with sources and expected entrances
│   ├── playgrounds/                # ts-orders/ and py-orders/, copied from graph-compiler
│   ├── boards/                     # the "Order placement" sample board, and order-system.json with
│   │                               #   every frame, resource card, connection, and note, in unfold-board v1
│   ├── reference/                  # frozen PoC generations, their canonical form, the reader
│   │                               #   reference, divergences.toml
│   └── corpora.lock.toml           # larger corpora pinned by URL and hash
├── tools/
│   ├── xtask/                      # developer CLI: compile headless, export, reference import,
│   │                               #   snapshots, parity, oracles, budgets, gen-lib-tables, gen-ipc
│   └── oracles/                    # Node, developer-only: TypeScript checker and Pyright oracles,
│                                   #   and the reader-reference export
├── e2e/                            # Playwright driving the built Electron app: journeys/, budgets/,
│                                   #   and their helpers in src/
└── fuzz/                           # cargo-fuzz targets for both language adapters and the backend
```

### 15.2 Crates

| Crate | Owns | Depends on |
| --- | --- | --- |
| `unfold-core` | The data model, vocabularies, keys, text and offsets, validator, canonical export | No other Unfold crate |
| `unfold-engine` | Language traits, discovery, the shared backend, `update()` | `unfold-core` |
| `unfold-lang-typescript` | Everything TypeScript-specific | `unfold-core`, `unfold-engine`, oxc |
| `unfold-lang-python` | Everything Python-specific | `unfold-core`, `unfold-engine`, Ruff's parser |
| `unfold-store` | SQLite | `unfold-core`, rusqlite |
| `unfold-reader` | Derived views and boards | `unfold-core`, `unfold-store` |
| `unfold-workspace` | The workspace and its projects, each live project end to end, and the command table | All of the above, `notify`, `ignore`, the ripgrep libraries, ts-rs |
| `unfold-node` | The Node-API addon that the engine host loads | `unfold-workspace`, napi-rs |

| Package | Owns | Depends on |
| --- | --- | --- |
| `desktop` | The Electron main process, the preload script, and the engine host | Electron, electron-updater, the MCP SDK's server (`@modelcontextprotocol/server`) and zod, the `unfold-node` addon, the generated IPC types |
| `ui` | The React app | React and the libraries in [§11.14](#1114-ui-technology-and-what-runs-where), the generated IPC types |
| `e2e` | The UI and app journeys | Playwright |
| `tools/oracles` | Developer-only oracles and the reader-reference export | TypeScript 6 and Pyright, as published packages |

### 15.3 Dependency rules

1. No engine crate depends on Electron, Node, or the UI. Everything outside `unfold-node`, `desktop/`, and `ui/` can be tested headless.
2. Language crates depend only on `unfold-core` and `unfold-engine`. They never touch the store.
3. `unfold-engine` produces in-memory results. `unfold-workspace` persists them through `unfold-store`.
4. `unfold-reader` never parses source. It reads the store and verified bytes, and gets highlighting through a `Highlighter` trait that the language crates implement and the workspace wires in.
5. `unfold-node` and `desktop/` hold no graph logic. Rust validates every command and `unfold-workspace` handles it. The engine host only routes messages.
6. The UI never touches files or Node. It only calls typed commands through `window.unfold`.
7. Test data lives in `testdata/`, never inside crates, so Rust tests, UI tests, and tools share one copy.
8. The main process holds no graph logic. It manages windows and the operating system's dialogs, and sends the engine host lifecycle requests, paths chosen in dialogs, and, for agents' tools, untrusted commands whose results it formats as text.
9. The UI depends on `window.unfold`, not on Electron, and the engine knows nothing about Electron. Either could sit behind another host, such as an editor extension, without changes ([U-6](decisions-and-tradeoffs.md#u-6-electron-for-the-shell-with-the-rust-engine-as-a-native-addon)).

### 15.4 Everyday commands

| Command | Does |
| --- | --- |
| `just dev` | Builds the addon and runs the Electron app, with hot reload for the UI |
| `just test` | Runs `cargo nextest` across the workspace, the addon's Node tests, and Vitest for the UI and the main process |
| `just check` | Runs rustfmt, clippy, `tsc`, ESLint, and cargo-deny |
| `just e2e` | Runs the Playwright UI journeys and the app journeys |
| `just bench` | Runs the criterion benchmarks and the corpus budgets |
| `just bundle` | Builds a signed app for the current platform with electron-builder |
| `cargo xtask gen-ipc` | Regenerates the TypeScript types for commands, events, and payloads |
| `cargo xtask compile <folder>` | Compiles a folder headless and prints its statistics |
| `cargo xtask export <folder>` | Writes the canonical export |
| `cargo xtask reference import <graph-compiler path>` | Imports and converts the frozen proof-of-concept outputs |
| `cargo xtask parity` | Compares against the reference and checks `divergences.toml` |
| `cargo xtask oracles` | Refreshes the oracle outputs (needs Node) |

`xtask` is for developers and CI. It is never shipped, and users never see it.

### 15.5 Continuous integration

| Workflow | Runs |
| --- | --- |
| `pr.yml`, on macOS, Windows, and Linux | Formatting, clippy, every Rust test, the addon's Node tests, snapshot and parity tests on fixtures and playgrounds, UI component tests, an unsigned app build, and the Playwright journeys on the UI and on that build |
| `nightly.yml`, on macOS, Windows, and Linux | Full-corpus parity and oracles, budgets, a fixed-length fuzzing run, the incremental-equals-full property tests with long edit sequences, and the full app journeys, including several windows and engine-host restarts |
| `release.yml` | On release tags: signed builds for all three platforms and both Mac architectures, macOS notarization, and the updater's manifests |

## 16. Testing

| Layer | What we check | How |
| --- | --- | --- |
| `unfold-core` | Vocabularies, keys and their escaping, validator invariants, canonical-export round trips, UTF-8/UTF-16 mapping, line breaks | Unit tests, and property tests against Rust's `encode_utf16` |
| `unfold-engine` | The precision policy, reason order, roles, slots, entrance assembly, and `update()` | The alien in-memory language, and property tests on temporary folders: shuffled order, different thread counts, incremental equals full |
| `unfold-lang-typescript` | Blocks, slots, imports, evidence, reasons, detectors, launch grammar | Fixture snapshots, parity with the reference, the checker oracle, fuzzing |
| `unfold-lang-python` | The same, plus the binder's refusal cases | Snapshots, parity, the Pyright oracle, the policy fixtures, fuzzing |
| Detectors | Every supported form, and decoys that must not match | Data-driven cases in `testdata/detectors/<id>/<case>/` |
| `unfold-store` | The schema, row diffs, single-transaction commits, reads during writes, rebuilds on a version change, a crash mid-commit | Unit tests with injected faults |
| `unfold-reader` | Owner, catalog, closure, applications, relations, Flow lanes, the code render model, Mermaid, and boards (schema, signatures, guards, health, publishing) | Snapshots on both playgrounds; hand-written graph fixtures that don't depend on extraction; Mermaid checked by the real Mermaid parser |
| `unfold-workspace` | Watcher batching, debounce, cancellation, last-good files, settings changes, reopening, the lock, the fallback location, the workspace and windows files, live and resting projects, Git's files and worktrees, command contracts, path confinement per project, board claims, unpublished changes, and cross-project boards | Integration tests on temporary folders |
| `unfold-node` | The addon's surface: dispatch round trips, events from Rust threads, cancellation, detaching windows, shutdown, and panics returned as errors | Node tests against the built addon |
| `desktop` | Window management and restore, engine-host restarts, sender checks, the `app://` protocol, each window's security settings, and agents: the MCP listener's refusals, every tool through a real MCP client, the agent bridge, page revisions, links, and the render window | Vitest for the main process, and generated TypeScript types that must compile against the preload script, the engine host, and the UI |
| UI | Components and journeys | Vitest and Testing Library, using fixtures the Rust views generate; Playwright in Chromium against recorded command responses |
| App | End-to-end journeys on built apps, including several windows, several projects, and a restart of the engine host | Playwright's Electron support, on macOS, Windows, and Linux |
| Packaging | No Node or Python on `PATH`, paths with spaces and Unicode, read-only folders, execution sentinels, and the fuses and window settings read back from the packaged app | Packaged smoke tests on clean machines of each platform |

**Gates**:

- Every resolved call and named reference agrees with an independent oracle: 100% precision.
- Recall floors per corpus can only go up.
- The reference shows no unlisted divergences, and no listed divergence has stopped occurring.
- The canonical export is deterministic.
- Incremental updates equal a full compile.
- Budgets hold.
- The journeys pass.

[U-33](decisions-and-tradeoffs.md#u-33-correctness-evidence-frozen-poc-reference-divergence-ledger-independent-oracles) and [U-34](decisions-and-tradeoffs.md#u-34-testing-at-every-layer) explain these gates.

## 17. Performance targets

These targets are set from the first measurements (Steps 5 and 9 of the migration, and the Step 0 window spike) and ratified on a fixed reference machine for each platform before V1 ships. Memory counts every Unfold process: the main process, the engine host, and the renderers.

| Interaction | Target | Proof-of-concept baseline |
| --- | --- | --- |
| Open a compiled project to a usable graph | Under 1 s | Manual re-index |
| First Compile of TypeScript's 602-file compiler source (262,508 blocks) | Under 5 s, under 1 GB peak | 11.3 s, 3.7 GB, serial native |
| First Compile of a 2,000-file application repository | Under 5 s | Not measured |
| Save to updated card, 2,000-file repository (V1) | Under 1 s | Manual re-index |
| Save to updated card after a body-only edit (V2) | Under 300 ms | |
| Search, 95th percentile | Under 100 ms | |
| Open a cached card, 95th percentile | Under 150 ms | |
| Canvas | 60 fps with 50 cards; usable with 100 | |
| Cancel acknowledged | Under 250 ms | |
| Launch to Home | Under 1.5 s | |
| Idle memory, one window, with a 2,000-file repository open | Under 600 MB | |
| Each further window | Under 150 MB more | |
| Idle memory with three projects connected | Under 600 MB (as one window); measured in [testing](testing.md#budgets-measured) | |
| Engine-host restart, until every window is usable again | Under 2 s | |

A budget never passes by dropping files, call sites, or external or unresolved records.

## 18. Risks

| Risk | Mitigation |
| --- | --- |
| The rewrite introduces wrong targets | Reference parity, 100%-precision oracle gates, and every divergence recorded before release |
| Background updates leave a stale or wrong graph | One code path for every compile; full relinking in V1; incremental-equals-full property tests; Compile and Rebuild from scratch are always available |
| Recall is lower without a type checker | The Step 0 recall check, the C-2 rule, written-type evidence, visible reasons, and a TypeScript 7 checker provider ([U-8](decisions-and-tradeoffs.md#u-8-type-checker-evidence-later-through-typescript-7s-api)) |
| oxc changes its syntax tree often | Exact pins, our own adapter, and upgrades only with full corpus parity |
| `oxc_semantic`'s scoping doesn't match our binding rules | Keep oxc's parser and port the proof of concept's binder onto its tree |
| Ruff's parser is internal to Ruff | Pin a release tag behind an adapter; fall back to a vendored copy |
| Electron uses more memory, especially with several windows | A per-window budget, measured from the Step 0 spike. The graph lives once, in the engine host, and each window fetches only what it shows |
| Many connected projects use memory and watchers | Projects rest when nothing needs them: no watcher, engine thread, or in-memory program, and searches read their stored graphs ([§11.2](#112-live-and-resting-projects)) |
| A native crash in the engine | The engine runs in its own process. The main process restarts it and the windows reconnect, keeping their boards. Fuzzing, depth limits, and larger stacks for engine threads make crashes rare |
| An Electron security setting is missed | One hardening module in the main process, and a packaged-app test that reads back the fuses and every window's settings |
| Electron's eight-week release cycle | Upgrades as routine pull requests behind every gate. Node-API means the addon needs no rebuild for each Electron version |
| Native builds for three platforms and two Mac architectures | One napi-rs build matrix in CI, and packaged smoke tests on every target |
| Signing certificates take time to obtain, especially on Windows | Ordered in Step 0, and signed builds on every platform from the Step 0 packaging spike onward |
| `.unfold/local/` on read-only folders or network volumes | The fallback location in the OS cache folder |
| Other tools react to writes in `.unfold/` | Writes are confined to `.unfold/local/cache/` and documented; dev servers rarely watch dot-folders |
| Linux watcher limits on huge folders | Detect the inotify limit, fall back to scanning, and say so |
| Mistakes converting between bytes and UTF-16 | One conversion module, property tests, and Unicode fixtures |
| The app grows into an editor | The non-goals, stated up front |
| "Unfold" is already a consumer app's name | A trademark check in Step 0, before the name reaches folders and formats |

## 19. Glossary

| Term | Meaning |
| --- | --- |
| Workspace | Every project connected on this machine, one per person. Every window shows it |
| Project | A folder connected to the workspace, usually a repository, with its own `.unfold/` and graph |
| Shared name | The name boards of other projects use for a project: `[project] name`, by default its folder's name |
| Live, resting | A live project has its engine, watcher, and in-memory program open; a resting one is closed, answers lists and searches from its stored graph, and wakes when used |
| Window | One Unfold window: the whole workspace, with its own tabs, sidebar, Home selection, and open boards |
| Home | The first tab of every window: the selected project's entrances, boards, and health, or the first launch |
| Main process | Electron's process that owns windows, menus, dialogs, and updates, and runs the agents' MCP server |
| Engine host | The helper process that runs the Rust engine for every window |
| Add | The one box that finds entrances, functions, files, and text in every project, and runs commands |
| Peek sheet | The sheet over the canvas that shows a whole source file |
| Compile | Bring the graph up to date with the files on disk. The first Compile builds it |
| Background update | A compile the watcher triggers after files change |
| Per-file facts | Everything the engine extracts from one file alone; cached by content hash |
| Linking | Building cross-file indexes such as modules, exports, and class families from all files' facts |
| Block | A selected syntax entity with a range, a parent, a role, and a structural key |
| Slot | The range of a written part of a block |
| Occurrence | One written call, construction, import, or passed callable |
| Edge | The relationship recorded at an occurrence, with its resolution |
| Resolution | `resolved`, `external`, or `unresolved` |
| Reason | Why an occurrence is unresolved |
| Entrance | A declared way that a process or unit of work starts |
| Trigger | The declared activating value: a command, route, queue, schedule, or event |
| Evidence | The source or configuration range that declares an entrance |
| Evidence file | A file a project lists so boards may cite its lines: hashed as an input, never compiled |
| Detector | A rule that recognizes one kind of declaration |
| Locator | A manual entrance's address: `path`, `path#name`, `path#Owner.member`, or `path:line` |
| Owner | The function, class, or file whose activation an occurrence belongs to |
| Application | A distinct startup target shared by one or more process entrances |
| Closure | Everything statically reached from some targets, "at least" |
| Structural key | A path-like name for a block that survives most edits |
| Last good | The facts from the last version of a file that parsed |
| Board | A story about the code: titled pages of cards, groups, descriptions, labels, drawn links, frames, resource cards, connections, and notes over the graph |
| Page | One canvas inside a board: one trace |
| Card | An entrance, a definition, or a type on a page, in Diagram, Flow, or Code |
| Folder | A name that organizes a project's boards, stored on each board; not the disk layout |
| Draft | A board that lives only on this machine, in `.unfold/local/drafts/` |
| Publish | Write a board to `.unfold/boards/`, so it can be committed |
| Unpublished changes | Edits to a published board, kept on this machine until the next Publish |
| Drawn link | An arrow a person draws between two items, usually cards from different projects; with a kind it is a connection |
| Frame | A named box or zone around items of one page; frames nest and collapse |
| Resource card | A table, queue, cache, storage, outside service, or actor on a page, citing the code it stands on |
| Connection | A link with a kind (`http`, `queue`, `writes`, …), leaving from a step and citing code |
| Note | Up to 400 characters of plain text on a page, free or attached to an item |
| Agent | A coding agent, such as Claude Code or Codex, connected through the MCP server; what it writes carries the agent mark |
| Lens | The top Diagram/Flow/Code switch, which changes the view without editing the board |
