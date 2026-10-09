# Unfold decisions and trade-offs

Status: decided · 2026-10-08

This document records every decision behind Unfold. For each one it gives the question, the options we weighed, what we chose, why, and the pros and cons. It also preserves the discussion that led there. The two earlier proposals were removed after they were merged, so their arguments live on here.

- [Architecture](architecture.md) describes what we are building.
- [Migration plan](migration-plan.md) describes the order we build it in, and what V1 and V2 contain.

Decision IDs are U-1 to U-36. In the new repository each one becomes its own file under `docs/decisions/`, and later changes are added as new decisions rather than edits to old ones.

## How we got here

### The proof of concept

`graph-compiler` (product name "Codegraph") proved the idea. A compiler reads TypeScript and Python without running anything. It writes a graph that contains no source code: blocks, slots, per-call edges, and declared entrances. A local browser reader lets a person start at `POST /orders` and follow calls card by card, reading each function as Code or Flow. At the final recorded run:

- **The graph.** The mixed playground held 84 sources (34 TypeScript, 50 Python), 50 entrances, 56 targets, 1,564 blocks, 2,766 slots, and 696 edges.
- **Tests.** 854 compiler tests, 57 reader tests, and 3 browser tests passed.
- **Precision.** Every resolved target agreed with an independent oracle: the TypeScript checker, and Pyright's evaluator for Python.
- **Recall floors.** Python's were frozen at 91 of 96 calls on the playground, 185 of 232 on the FastAPI full-stack template, and 3 of 3 on the range corpus.
- **Re-indexing.** Boards survived it. Structural keys reconnected their cards, and signatures marked descriptions outdated when their code changed.

These parts of it hurt:

| What hurt | Why it matters for a product |
| --- | --- |
| Two TypeScript producers, a pinned TypeScript 6.0.3 fork (818 MB checked out), and vendored Pyright 1.1.407 (114 MB) | A heavy toolchain, and two producers to keep in parity |
| Compiler work in a JavaScript runtime | A serial build of TypeScript's own 602 source files took 11.3 s and 3.7 GB at peak. Four parsing workers were slower, at 12.8 s |
| A compiler and a reader as two programs joined by files | Inode-checked consistent reads, directory-swap publication with rollback, and producer replay in a re-index subprocess existed only because the halves were separate |
| A browser app over a local server | Loopback and origin checks, server-sent events, a 750 ms polling loop, and Re-index as a manual step |
| Artifacts in the repository | `.codecodegraph/`, `.codecodegraph-drafts/`, and `codegraph/boards/` were all written at the repository root, and 113 MB of reports were committed |
| Block IDs numbered across the whole generation (`b1`, `b2`, …) | Any change renumbered every block, so updates couldn't be incremental |
| A sidebar that stacked boards, entrances, and a flat Definitions list (capped at 400 items) | It doesn't scale to real repositories, and you can't browse the way you do in an IDE |
| The name "Codegraph" | You didn't like it |

### Two proposals

You asked for a clean rebuild as one desktop app. Two agents each wrote a proposal:

| | Proposal A: `desktop-app-design.md` | Proposal B: `unfold-desktop-from-scratch.md` |
| --- | --- | --- |
| Written by | An earlier Claude session | Another agent |
| Shell and engine | Tauri 2 and a Rust engine | Electron and a TypeScript engine; Rust only for profiled hot spots |
| TypeScript | oxc plus the PoC's native evidence rules, with no checker in V1 | The upstream TypeScript 6 compiler API, with the checker as the only producer |
| Python | Ruff's parser plus a ported binder | The vendored Pyright parser, with the binder kept in TypeScript |
| Graph storage | One SQLite cache per project, in app data | One immutable SQLite file per generation, in the OS cache, swapped in through a pointer |
| Repository footprint | An opt-in `.unfold/` for settings and published boards | Nothing in the repository; Save As and Export instead of Publish |
| Keeping current | A file watcher with live updates; a broken file keeps its last good graph | Manual Compile; the watcher only marks things stale |
| Scope | Exclusions in settings; scope from the tree later | A persisted scope recipe, "Compile this folder", and context provenance |
| Old boards | Imports the PoC's boards | No compatibility promise |
| Unresolved calls | Listed in Problems | Not problems |
| Build order | Layer by layer: the reader in Phase 3, Python in Phase 4 | An end-to-end slice first |
| Repository | A Cargo workspace with 11 crates, plus a pnpm UI | One TypeScript package |

**They agreed on most of the product:**

- one desktop app with an IDE frame and no user-facing CLI, server, or web app;
- the PoC's precision policy, static-only claims, the detector families, and one shared backend behind narrow language adapters;
- the v7.1 reader interaction;
- an activity bar with Files, Entrances, Search, and Boards, plus source tabs, an inspector, Problems, a status bar, and a command palette;
- a SQLite graph that is a rebuildable cache, not a public format;
- discovery that reads ignore files without running Git;
- a single TypeScript analyzer;
- the PoC as a behavioral reference, compared by file, range, and kind;
- macOS first.

Both also said that the shell follows the engine language, and both rejected Tauri plus a bundled Node process.

### The evidence that settled the biggest question

The two proposals each dropped a different TypeScript producer. That made the type checker the deciding question, and the proof of concept had already measured it (`graph-compiler/docs/testing.md`):

- **Recall without a checker was high where it was measured.** Without a checker, the native producer resolved **96.0%** of the calls the checker resolved on TypeScript's own 602-file compiler. On the codegraph source it resolved **100%** (4,440 of 4,440). On the playground it resolved 65% (46 of 71), and **all 25 missing calls come from one bounded defect, C-2**: members of constant object literals, such as `logger.info`.
- **Memory was the real problem.** The JavaScript engine peaked at 3.7 GB, and adding workers didn't help.
- **TypeScript 6 is the line being retired.** TypeScript 7.0, the Go port, has no programmatic API. Its API is planned for 7.1 and runs out of process, so a future checker doesn't need a JavaScript host.
- **The checker can't be its own judge.** If it is the production analyzer, it can't also be the independent oracle for precision. The proof of concept made exactly this point for Python in its decision D-85.
- **The PoC's own archived rewrite plan** had already recommended Rust, with oxc for the native front end.

### The review

The review preferred A's destination, meaning its engine, shell, repository-local sharing, IDE detail, and parity ledger. It preferred B's discipline:

- the end-to-end slice first;
- a persisted scope;
- staleness when settings change;
- a job lifecycle you can cancel;
- a separate cache;
- no unresolved calls in Problems;
- no promises about old formats;
- packaged-app journeys.

It also pointed out that the checker question could be settled cheaply, by running the PoC's two producers on real application repositories.

### Your decisions on 2026-10-08

1. **Rust and Tauri.**
2. **A `.unfold/` folder in every repository**, so each repository keeps its own information there.
3. **The button is called "Compile", but compilation should run in the background** and recompile only what an edit changed, rather than the whole graph. This needed design thought, which is recorded in U-13, U-14, and U-15.
4. Delete the two proposals and replace them with three documents: this one, the [architecture](architecture.md), and the [migration plan](migration-plan.md).

Everything else below follows the review's merged recommendations, adjusted for these four decisions.

## Decision index

| ID | Decision | Area |
| --- | --- | --- |
| [U-1](#u-1-one-desktop-app) | One desktop app | Product |
| [U-2](#u-2-start-fresh-carry-specifications-and-data-not-code) | Start fresh: carry specifications and data, not code | Product |
| [U-3](#u-3-names-and-words) | Names and words | Product |
| [U-4](#u-4-a-reader-not-an-editor) | A reader, not an editor | Product |
| [U-5](#u-5-rust-for-the-engine) | Rust for the engine | Engine and platform |
| [U-6](#u-6-tauri-2-for-the-shell) | Tauri 2 for the shell | Engine and platform |
| [U-7](#u-7-typescript-through-oxc-without-a-type-checker-in-v1) | TypeScript through oxc, without a type checker in V1 | Engine and platform |
| [U-8](#u-8-type-checker-evidence-later-through-typescript-7s-api) | Type-checker evidence later, through TypeScript 7's API | Engine and platform |
| [U-9](#u-9-python-through-ruffs-parser) | Python through Ruff's parser | Engine and platform |
| [U-10](#u-10-one-shared-backend-behind-narrow-language-adapters) | One shared backend behind narrow language adapters | Engine and platform |
| [U-11](#u-11-the-pocs-semantics-carry-over-with-its-known-defects-fixed) | The PoC's semantics carry over, with its known defects fixed | Engine and platform |
| [U-12](#u-12-utf-8-offsets-in-the-engine-utf-16-at-the-ui) | UTF-8 offsets in the engine, UTF-16 at the UI | Engine and platform |
| [U-13](#u-13-compile-once-then-update-in-the-background) | Compile once, then update in the background | Compiling |
| [U-14](#u-14-what-gets-recompiled-per-file-facts-in-v1-early-cutoff-in-v2) | What gets recompiled: per-file facts in V1, early cutoff in V2 | Compiling |
| [U-15](#u-15-a-file-that-stops-parsing-keeps-its-last-good-facts) | A file that stops parsing keeps its last good facts | Compiling |
| [U-16](#u-16-discovery-and-scope-without-git) | Discovery and scope without Git | Compiling |
| [U-17](#u-17-freshness-verify-before-showing-cheap-checks-first) | Freshness: verify before showing, cheap checks first | Compiling |
| [U-18](#u-18-a-unfold-folder-in-every-repository) | A `.unfold/` folder in every repository | Storage and files |
| [U-19](#u-19-the-graph-is-a-sqlite-cache) | The graph is a SQLite cache | Storage and files |
| [U-20](#u-20-identity-file-scoped-ids-and-structural-keys) | Identity: file-scoped IDs and structural keys | Storage and files |
| [U-21](#u-21-only-files-people-commit-are-contracts) | Only files people commit are contracts | Storage and files |
| [U-22](#u-22-a-fresh-board-format) | A fresh board format | Storage and files |
| [U-23](#u-23-settings-in-toml-in-three-places) | Settings in TOML, in three places | Storage and files |
| [U-24](#u-24-react-ui-rust-computes-typescript-lays-out) | React UI: Rust computes, TypeScript lays out | The app |
| [U-25](#u-25-typed-commands-generated-from-rust) | Typed commands generated from Rust | The app |
| [U-26](#u-26-an-ide-shaped-window) | An IDE-shaped window | The app |
| [U-27](#u-27-the-reader-keeps-the-v71-rules) | The reader keeps the v7.1 rules | The app |
| [U-28](#u-28-problems-lists-only-real-problems) | Problems lists only real problems | The app |
| [U-29](#u-29-inspector-and-board-outline) | Inspector and board outline | The app |
| [U-30](#u-30-search-covers-definitions-entrances-files-and-text) | Search covers definitions, entrances, files, and text | The app |
| [U-31](#u-31-one-process-with-a-locked-down-webview) | One process with a locked-down webview | The app |
| [U-32](#u-32-repository-layout-a-cargo-workspace-and-a-pnpm-ui) | Repository layout: a Cargo workspace and a pnpm UI | Engineering |
| [U-33](#u-33-correctness-evidence-frozen-poc-reference-divergence-ledger-independent-oracles) | Correctness evidence: frozen PoC reference, divergence ledger, independent oracles | Engineering |
| [U-34](#u-34-testing-at-every-layer) | Testing at every layer | Engineering |
| [U-35](#u-35-platforms-macos-first-all-three-in-ci) | Platforms: macOS first, all three in CI | Engineering |
| [U-36](#u-36-build-order-a-vertical-slice-first) | Build order: a vertical slice first | Engineering |

## Product

### U-1: One desktop app

**Decided.** Unfold is one installable desktop app. The analysis engine is a library inside it. There is no separate compiler, CLI, local server, browser app, or editor extension.

**Question.** What shape should the product take after the PoC's compiler, CSV package, local server, browser reader, and export CLI?

**Options**

| Option | For | Against |
| --- | --- | --- |
| Keep a compiler and a reader as two programs joined by files | Each half can be used alone; a CLI suits CI | It is exactly where the PoC's protocol machinery came from: consistent reads, directory swaps, producer replay, polling |
| A web app, hosted or on a local server | No install; easy to share | Needs a server or a local daemon; a browser can't watch folders or read arbitrary files without one |
| A VS Code or JetBrains extension | It lives where developers already are, with the editor's tree, tabs, and search for free | The canvas reader would be squeezed into a webview panel. We'd be bound to each host's APIs and need one extension per editor |
| **One desktop app** | One install and one lifecycle; we own the whole reading experience | We build the IDE basics and own packaging and updates |

**Why.** It's what you asked for: open a directory, press a button, get the graph, in something that feels like an IDE. The PoC also showed that much of its complexity existed only because its halves were separate programs.

**Pros**
- One install, one lifecycle, and no protocol between halves.
- We control the whole experience, including the IDE frame around the canvas.
- Offline and private by default.
- The app can keep its own graph current (U-13).

**Cons**
- We own windows, menus, packaging, signing, and updates on every platform.
- We rebuild basics an editor extension would get free: the file tree, tabs, search.
- There is no headless command for CI, such as checking board health. If that's ever needed, it has to come without becoming a second product.

**Revisit if** people mainly want Unfold inside their editor. The engine knows nothing about its shell, so an extension could host it later.

### U-2: Start fresh: carry specifications and data, not code

**Decided.** We start a new repository, `unfold`. From the PoC we carry over the semantics, fixtures, playgrounds, measurements, frozen outputs, and reader designs. No PoC code goes into the product. Developer-only oracle tools may be adapted, with attribution. Unfold doesn't read `.codecodegraph` packages or `codegraph-board` files.

**Options**

| Option | For | Against |
| --- | --- | --- |
| Refactor the PoC in place | Keeps the tests running throughout | You said not to fix this repository. It carries the mess, the names, and the TypeScript fork |
| Fork it and clean it up | Some reuse | The same mess, renamed, and a Rust engine can't reuse TypeScript code anyway |
| **A fresh repository** | Clean structure and names, and an architecture designed for background updates | Every guarantee has to be earned again |

**Why.** You said the repository is messy and shouldn't be fixed, and that we start fresh. The PoC's lasting value is its specification and its evidence, not its code.

**Pros**
- Clean names and layout, with no 818 MB TypeScript fork and no 114 MB vendored Pyright.
- The architecture can assume background updates from the first commit.
- There are no legacy formats to support.

**Cons**
- Every guarantee has to be earned again: precision, recall, determinism, and the detectors' bounds.
- Subtle rules can be lost in translation. The frozen reference, the divergence ledger, and the oracles (U-33) exist to catch that.
- Reaching parity takes time.

### U-3: Names and words

**Decided**

| Thing | Word |
| --- | --- |
| The product | **Unfold**, subject to a trademark check in Phase 0, before the name reaches folders and formats |
| The button that builds or refreshes the graph | **Compile** |
| Declared ways work starts | **Entrances** |
| An authored canvas | **Board**. One that lives only on this machine is a **Draft** |
| Making a board shareable through the repository | **Publish**. Writing it to a file elsewhere is **Export** |
| The two ways to see a card | **Code** and **Flow** |
| The top Code/Flow switch | The **lens** |

Nothing is named Codegraph anywhere: not folders, formats, IDs, or bundle names.

**Options considered**
- For the button: "Unfold", "Compile", "Index", and "Build graph".
- For entrances: "Entrances" or "Entry points".
- For sharing: "Publish" or "Save As".

**Why**
- **"Compile"** is your word, and it matches the idea of compiling code into a graph.
- **"Entrances"** is precise. One entrance can name several handlers, and one handler can serve several entrances, whereas "entry point" suggests a single function.
- **"Publish"** was chosen in the v7.1 review. Here it means "put it in `.unfold/boards/` so it can be committed".

**Pros**
- The verbs are familiar.
- The distinction between Publish and Export is clear.
- The words carry over what the PoC's users (you) already know.

**Cons**
- "Compile" can suggest building binaries. The first-compile screen says plainly that it reads files only and runs nothing.
- "Entrances" is a new term for newcomers. A tooltip explains it.
- "Unfold" is also the name of a consumer app for designing social media stories.

**Revisit if** the trademark check fails.

### U-4: A reader, not an editor

**Decided.** These are non-goals:
- editing, refactoring, running, or debugging code;
- language-server features such as completion;
- runtime traces;
- control-flow and data-flow graphs;
- accounts, hosted sharing, and real-time collaboration;
- links between languages without explicit evidence;
- entrances inferred from names.

"Open in editor" hands a file and line to your own editor.

**Why.** The value is in reading and explaining code. Editor features would multiply the scope without improving that.

**Pros**
- A smaller surface and no language server.
- Clear positioning beside your editor.
- Focus on the graph and boards.

**Cons**
- You switch apps to change code.
- Some IDE expectations, such as rename or find-all-references inside a file, aren't met.

## Engine and platform

### U-5: Rust for the engine

**Decided.** The analysis engine is written in Rust: discovery, both language front ends, the shared backend, the store, the reader derivations, and boards. Only layout and presentation stay in TypeScript (U-24).

**Question.** Should we keep the engine in TypeScript, the PoC's language, as Proposal B argued, or rewrite it?

**Options**

| Option | For | Against |
| --- | --- | --- |
| TypeScript (Proposal B) | The PoC's logic and test knowledge are already in TypeScript. Pyright's parser and the TypeScript compiler API are JavaScript. It's the shortest path to a working app, with one language for the UI and the engine | The PoC's engine peaked at 3.7 GB and took 11.3 s for 602 files, and parallel workers didn't help. The checker API it needs is TypeScript 6's, the line being retired. A JavaScript host for the engine means Electron |
| **Rust (Proposal A)** | The best TypeScript and Python parsers are written in Rust (oxc, and Ruff's). It brings real parallelism, low memory, one binary, and a strict type system for a precision engine. The PoC's archived rewrite plan already recommended Rust and oxc | Everything is ported, and every guarantee is earned again. Builds are slower. The parser crates change often. There are two languages in the repository |
| Go | TypeScript 7 is written in Go | TypeScript 7's packages are internal, and its API runs out of process anyway. Python parser options are weak, and Go's desktop shells (Wails) are less mature |
| C# | A mature language with native UI toolkits (Avalonia, MAUI) | No first-class TypeScript or Python parser, and a full UI rebuild |
| C++ | Maximum control | tree-sitter is the only realistic parser, memory safety is manual, and we'd still need a UI toolkit |

**Why**
- **The type checker isn't needed for recall on the code measured so far.** Native recall was 96% and 100% on real corpora, and the playground gap is one bounded rule, C-2. That removes B's main reason to keep a JavaScript host.
- **Memory and speed were the PoC's real problems.** A desktop app opened on arbitrary repositories has to fix them by design.
- **Background compilation (U-13, U-14) needs a fast in-memory relink** over compact facts, which Rust makes natural.
- **The future TypeScript checker is a Go binary** with an out-of-process API, so a Rust host can use it as easily as Node can (U-8).
- **A desktop app ships as one binary.** That was the "single binary" trigger in the PoC's own rewrite plan (D-76).
- **You chose Rust and Tauri.**

**Pros**
- Speed and low memory, with parallel extraction and resolution across every core.
- One binary, with nothing to install.
- Maintained parsers with large user bases: oxc is used by Rolldown and Oxlint, and Ruff's parser by Ruff and ty.
- A compiler-checked type system for policy code where mistakes become wrong links.
- No TypeScript fork, and no TypeScript 6 dependency.

**Cons**
- A full port of roughly 19,000 lines of dense TypeScript logic, plus the reader's derivations.
- Every rule has to be earned again against the oracles.
- Rust compiles more slowly, and fewer people can contribute.
- oxc is on 0.x versions and changes its syntax tree often. Ruff's parser is an internal crate.
- The repository has two languages: Rust for the engine and TypeScript for the UI.
- There is no type checker in V1, so recall on inference-heavy code is a risk. The Phase 0 recall check measures it (U-8).

**Revisit if** a Phase 0 spike shows that oxc or Ruff's parser can't carry the rules. Even then the fallback stays in Rust: port the PoC's binder onto oxc's tree, or vendor Ruff's parser.

### U-6: Tauri 2 for the shell

**Decided.** Tauri 2 hosts the React UI in the system webview, and the Rust engine runs in the same process.

**Options**

| Option | For | Against |
| --- | --- | --- |
| Electron with a TypeScript engine (Proposal B) | One Chromium everywhere and a huge ecosystem. Playwright drives it | It bundles Chromium and Node, so the app is large. It only makes sense with a TypeScript engine, which U-5 rejects |
| Electron with a Rust native addon (napi-rs) | Chromium everywhere and the Rust engine | Bundled Chromium and Node anyway, plus an addon boundary to maintain |
| **Tauri 2** | The Rust engine runs in-process. Small download and memory. A capability-based security model. A built-in bundler, updater, and signing | Three rendering engines; less mature end-to-end tooling on macOS |
| A native toolkit (GPUI, Slint, egui, Qt, SwiftUI) | Native performance and look | Rebuilds the cards, code layout, and canvas from nothing, and loses the HTML and CSS workflow used to design the reader in Paper |
| Wails (Go) | A small shell | Tied to Go (U-5) |

**Why.** Both proposals agreed that the shell follows the engine. With a Rust engine, Tauri adds no second runtime and no transport, and Tauri 2 has been stable since October 2024.

**Pros**
- A small app with low idle memory.
- No sidecar processes and no serialization between the engine and the store.
- The webview can only call the commands we expose (U-31).
- The updater, signing, and bundling are built in.

**Cons**
- **Three webviews.** Tauri renders with WKWebView on macOS, WebView2 (Chromium) on Windows, and WebKitGTK on Linux. A canvas that sizes cards by measuring text is sensitive to their differences, and WebKitGTK is the weakest.
- **End-to-end testing on macOS** uses an embedded WebDriver plugin rather than a standard driver.
- **tauri-specta**, which generates our IPC types, is still a release candidate (U-25).
- The ecosystem is smaller than Electron's.

**Fallback.** If the Phase 0 canvas spike fails on WebKit, we move to Electron with a native addon. The engine crates don't change, because no engine crate depends on Tauri.

### U-7: TypeScript through oxc, without a type checker in V1

**Decided.** TypeScript is parsed with `oxc_parser`, bound with `oxc_semantic`, and resolved with `oxc_resolver`, each pinned exactly and kept behind Unfold's adapter. The PoC's native evidence rules (levels 1–4) are ported on top, and a rule for constant object literals fixes C-2. The TypeScript checker is used only as a developer-only oracle.

**Options**

| Option | For | Against |
| --- | --- | --- |
| Port the PoC's own scanner, parser, and binder to Rust | Exact parity | Thousands of lines of grammar to maintain, which a library already provides |
| **oxc** | A fast, complete TypeScript, TSX, and JSX grammar, plus scopes, symbols, references, and a resolver that understands `tsconfig` `paths` and package `exports` | 0.x API churn. Its scoping must match our binding rules |
| SWC | A mature parser | Focused on transforms. No semantic analysis or resolver comparable to oxc's |
| tree-sitter-typescript | Fast and tolerant of broken code | Syntax only, with a different tree shape. Binding, modules, and classes would all be extra work (PoC D-5) |
| The TypeScript 6 checker in production (Proposal B) | The highest recall on inference-heavy code | Needs Node. Ties Unfold to the line being retired. Large memory. It was the source of defects C-1 and C-3. It can't be its own precision oracle |
| TypeScript 7's API | The future checker | Not available in 7.0 |

**Why**
- The recall data (U-5).
- Keeping the oracle independent of the analyzer.
- oxc supplies the parser, the scopes, and the resolver in one family, which together replace the PoC's scanner, parser, binder, and its dependency on the fork.

**Pros**
- Fast, maintained parsing of all TypeScript syntax.
- A path to JavaScript support (V2).
- No 818 MB fork.
- The checker stays an independent judge.

**Cons**
- oxc's syntax tree changes between releases, so we pin it and adapt.
- `oxc_semantic` may not match the PoC's binding rules. The fallback is to port the PoC's binder onto oxc's tree.
- The tables of known globals and library receivers must be generated from TypeScript's `lib` files.
- The emit map has to be reimplemented for the bounded rules the PoC used.
- Recall may be lower on inference-heavy application code (U-8).
- oxc's spans are UTF-8 (U-12).

**Revisit if** the Phase 0 recall check fails. The checker provider then moves into V1 (U-8).

### U-8: Type-checker evidence later, through TypeScript 7's API

**Decided.** An optional checker provider will use TypeScript 7's programmatic API, planned for 7.1. It runs as a bundled `tsgo` process and feeds evidence through the same shared policy and the same oracle gates. It is planned for V2 and moves into V1 only if the Phase 0 recall check fails.

**Decision rule (Phase 0).** We pick two real-world TypeScript backend repositories, pinned by commit, and run both of the PoC's TypeScript producers on them:

- If the native producer resolves at least **90%** of the calls the checker resolves on each repository, V1 ships without a checker.
- If it doesn't, the provider joins V1. If TypeScript 7's API isn't stable at that point, we choose between shipping V1 with lower recall, with the reasons visible, and waiting for the API.

**Pros**
- Higher recall when it's needed.
- It is host-agnostic, because it's a separate process.
- The precision policy stays shared.

**Cons**
- It bundles a Go binary, which adds size.
- There is a cost per query between processes.
- Two sources of evidence have to stay consistent.
- The API's timing is uncertain.
- A provider can't be its own oracle, so its targets are still checked against reviewed fixtures and the native producer.

### U-9: Python through Ruff's parser

**Decided.** Python is parsed with `ruff_python_parser`, pinned to a Ruff release tag behind Unfold's adapter. The PoC's binder rules are ported to Rust. Pyright's evaluator stays as a developer-only oracle.

**Options**

| Option | For | Against |
| --- | --- | --- |
| The vendored Pyright parser (the PoC and Proposal B) | Proven in the PoC, with exact UTF-16 ranges | Needs a JavaScript runtime inside a Rust app, plus 114 MB of vendored source |
| **Ruff's parser** | Rust-native, fast, and current with the latest grammar (used by Ruff and ty). Exact ranges and a token stream | An internal crate, not a stable public library |
| tree-sitter-python | Portable and fast | The PoC measured it rejecting Python 3.13 type-parameter defaults (D-84), and it has no semantics |
| CPython's `ast` module | The official grammar | Needs Python installed, which breaks the "nothing to install" rule, and offsets need converting |
| Our own parser | Full control | Duplicates a great deal of grammar work |

**Why.** It's the best Rust-native Python grammar, it needs no interpreter, and it keeps the production parser separate from the oracle's grammar. The PoC shared Pyright's grammar between the two and noted the risk of common parser defects (D-84).

**Pros**
- Speed.
- Maintenance by Astral.
- Tokens for syntax colors.
- More independence between production and the oracle than the PoC had.

**Cons**
- **An internal crate.** It is pinned to a git tag, and its API can change between tags.
- **Version-specific syntax.** Rejecting syntax by grammar version, for 3.12 through 3.14, has to be verified in Phase 0.
- **The biggest port.** The Python binder (the write census, C3 lookup, decorator proofs, and the framework detectors' proofs) is the largest and riskiest part of the rewrite.

**Revisit if** the parser's API becomes unworkable. We would then vendor a copy at a tag.

### U-10: One shared backend behind narrow language adapters

**Decided.** We keep the PoC's best structural decision (D-4 and D-82). Language crates answer narrow questions, and one backend owns selection, roles, slots, the value policy, the reason order, module labels, edges, entrances, and validation.

Adapters are split into two parts for caching:
- **per-file extraction**, whose facts are serializable and hold no live syntax tree;
- **whole-program linking**.

A tiny "alien" test language keeps the backend honest.

**Options.** We could have given each language its own pipeline, or kept the shared backend (chosen).

**Pros**
- One precision policy.
- New languages don't touch the policy.
- The policy is tested without any real language.

**Cons**
- The interface has to be designed for caching, so facts must be complete without the syntax tree.
- Some duplication between language crates.

### U-11: The PoC's semantics carry over, with its known defects fixed

**Decided.** These carry over as they are:
- the vocabularies;
- the precision policy and its reason order;
- entrance semantics and evidence;
- structural keys;
- activation closure;
- the claims Flow makes.

The PoC's known defects are handled like this:
- **C-2, C-4, C-5, C-6, and C-12** are fixed.
- **C-1 and C-3** disappear along with the checker-backed producer.
- **C-8 and C-9** are decided with fixtures in Phase 2.

**Why.** These semantics were verified independently, at 100% precision, and the reading experience they produce is the one you liked.

**Pros**
- Trust.
- An executable specification.
- No re-arguing settled questions.

**Cons**
- Recall is lower than a guessing tool's, so readers see many unresolved chips, and the UI must explain reasons well.
- The PoC's deferred limits remain until the roadmap reaches them. These include:
  - Express route chains;
  - TypeScript decorator routing;
  - CommonJS;
  - the opaque Flow regions.

### U-12: UTF-8 offsets in the engine, UTF-16 at the UI

**Decided.** The engine and the store use UTF-8 byte offsets. `unfold-reader` converts them to UTF-16 when it builds what the UI renders, using one tested module with a per-file index.

**Options**

| Option | For | Against |
| --- | --- | --- |
| UTF-16 everywhere (the PoC and Proposal B) | Matches JavaScript strings; the PoC's format used it | Every Rust parser span would need converting inside the engine |
| UTF-8 everywhere, including the UI | One unit | JavaScript would convert everywhere, and get it wrong |
| **UTF-8 inside, UTF-16 at the boundary** | Matches oxc and Ruff, and keeps one conversion point | A boundary to get exactly right |

**Pros**
- No conversion inside the engine.
- The UI never sees bytes.

**Cons**
- The conversion has to handle byte-order marks, CRLF, U+2028 and U+2029, and characters outside the Basic Multilingual Plane. Property tests and Unicode fixtures cover these.
- The reference comparison has to convert the PoC's UTF-16 offsets into bytes.
- Board signatures are defined on bytes.

## Compiling

### U-13: Compile once, then update in the background

**Decided (your decision).**
- A **Compile** button builds the graph the first time and brings it fully up to date whenever you press it.
- After the first Compile, **background updates** watch the folder and apply changes shortly after files are saved, redoing only what the change requires (U-14).
- Background updates can be paused for each folder.
- **Rebuild from scratch** lives in the menu.

**Options**

| Option | For | Against |
| --- | --- | --- |
| Manual Compile every time (Proposal B; the PoC's Re-index) | Predictable, and no background CPU use | The graph goes stale while you work, and every Compile repeats work |
| Compile automatically on open, with no button | Zero clicks | Surprise CPU use and writes to a folder you may only have wanted to browse, with no chance to narrow the scope first |
| **Compile once, then update in the background** | The graph stays current without thinking, and the first Compile is a deliberate step | More machinery |
| Live updates on every keystroke | The most immediate | Impossible: Unfold reads files on disk, not your editor's unsaved buffers |

**Why.** It's your call, and it's the right one for a reader that sits beside an editor. Background updates mean you never recompile the whole graph by hand.

The explicit first Compile does four jobs:
- you consent to the CPU work;
- you can narrow the scope of a large repository;
- you see what will be read;
- it is the moment `.unfold/` appears (U-18).

**Pros**
- The graph stays current.
- Cards update after a save.
- Boards stay connected while you edit.
- Nothing happens on first open until you choose.

**Cons**
- Background CPU and battery use. Updates run at low priority on a few threads and can be paused.
- More machinery: the watcher, batching, cancellation, and handling broken files.
- The graph lags the disk for a moment after a save; the status bar shows when.

### U-14: What gets recompiled: per-file facts in V1, early cutoff in V2

**Decided**
- **V1.** Updates re-extract only the changed files, using a per-file facts cache. They rebuild cross-file indexes and re-resolve every call site in memory, then write only the rows that changed, in one transaction.
- **V2.** Adds *early cutoff*: re-resolve only the files an edit can affect, worked out from interface hashes and recorded dependencies.
- **After V2.** A query-based incremental framework (Salsa) is considered only if early cutoff isn't enough.

**The thinking.** "Recompile only the part that was edited" is easy for some facts and hard for others:

- **Per-file facts** depend only on one file's bytes and settings: blocks, slots, names, roles, local bindings, declarations, and call sites. Caching them by content hash is simple and safe.
- **Cross-file facts** are edges and entrances, and they depend on other files in ways that aren't local:

| An edit in one file | Changes elsewhere |
| --- | --- |
| Renaming an exported function | Calls to it in every importer |
| Adding a subclass that overrides a method, in a new file | Calls through the base class in files that never import the new one become `overridden` |
| Reassigning a module member in Python | That member loses its identity everywhere |
| Changing a router's mount prefix | The trigger paths of routes declared in other files |
| Creating a missing file | `not_found` imports elsewhere start to resolve |

Re-resolving only the edited file would miss these, and the miss would be silent: a stale link that looks resolved. That is the one failure Unfold never allows. So V1 makes correctness structural, by re-resolving everything in memory, and V2 adds the bookkeeping behind a test that proves it changes nothing.

**Options**

| Option | Work per update | Risk |
| --- | --- | --- |
| A full rebuild each time | Proportional to the repository, including parsing and I/O | None, but slow |
| **V1: cached per-file facts, a full in-memory relink, row diffs** | Parsing proportional to the change; resolution proportional to the repository, in memory | None beyond a full build, because it shares the full build's code path |
| **V2: early cutoff** | Mostly proportional to the change | Missing a dependency produces a stale link, so it is gated by the incremental-equals-full tests |
| A query-based framework (Salsa) | Proportional to the change, at fine grain | A large framework commitment that every rule must be written around |

**Pros of V1**
- Correct by construction.
- Simple.
- Writes are proportional to the change.
- We expect updates in well under a second on repositories with a few thousand files. Phase 2 measures this and Phase 5 sets a hard limit.

**Cons of V1**
- The CPU work of each update grows with the repository, so very large monorepos may lag. That is what V2 is for.

**Pros of V2.** Most edits are inside function bodies, and then only the edited file is re-resolved.

**Cons of V2**
- Recording dependencies is subtle.
- It needs separate interface and body hashes for each file, and records of what each resolution consulted, including failed lookups and census entries.

**Revisit if** V1 misses the update-latency limit on target repositories. Early cutoff (Phase 9) then moves into V1.

### U-15: A file that stops parsing keeps its last good facts

**Decided**
- **During background updates**, a file that no longer parses keeps the facts from the last version that did. Its code is withheld: the UI keeps its last verified render dimmed, in memory only, and never applies old ranges to new bytes. Problems shows the syntax error.
- **On a first Compile**, files that don't parse are left out, and the graph is marked incomplete. The Compile never aborts because of them.

**Options**

| Option | For | Against |
| --- | --- | --- |
| Abort the whole compile (the PoC's default, and Proposal B's) | Every graph comes from one coherent snapshot | Hostile while editing, because one broken file freezes everything. A first Compile with one bad file produces no graph at all |
| Drop the file from the graph | Simple | The graph flickers as you type: entrances vanish and cards turn into "missing" placeholders mid-edit |
| **Keep its last good facts** | Stable while you edit, and consistent with the PoC's staleness model, in which names stay navigable and code is withheld | The graph mixes versions from file to file |
| Parse with error recovery | Up-to-date facts | Recovered trees can have wrong ranges, which breaks the exact-range guarantee |

**Why.** Background compilation runs while people are mid-edit, so a broken file is a normal, temporary state.

This revises the review's earlier advice to "never mix versions in V1", which assumed manual compiles. With background updates, keeping last good facts is necessary. It stays coherent because each file records the hash its facts came from, and determinism is defined over those effective inputs.

**Pros**
- A stable graph and stable boards while typing.
- A clear message in Problems.

**Cons**
- The graph can mix versions from file to file. The status bar and the file tree show it.
- Rebuild from scratch loses last good facts.
- A file that has never parsed is absent until it's fixed, and calls into it are `not_indexed`.

### U-16: Discovery and scope without Git

**Decided**
- **Walking.** Discovery walks the folder with the `ignore` crate, which honors `.gitignore`, `.ignore`, `.git/info/exclude`, and global excludes, and never runs Git.
- **Always excluded:** `node_modules`, Python environments and caches (`.venv`, `venv`, `__pycache__`, `.mypy_cache`, `.pytest_cache`, `.ruff_cache`), `.git`, and `.unfold`. Declaration files never get blocks.
- **Scope** lives in `.unfold/config.toml`, as `[scope] include` and `exclude`. The file tree offers Exclude from graph, Include in graph, and Graph only this folder.
- **First Compile.** The screen shows file counts and skipped folders, and lets you choose folders before compiling.
- **Python-only and mixed folders** compile correctly the first time.

**Options**

| Option | For | Against |
| --- | --- | --- |
| Git's file list (the PoC) | Exactly matches what Git tracks | Needs Git and a repository; fails on a plain folder |
| **A walk with ignore rules** | Works anywhere, with ripgrep's semantics | Edge-case differences from Git's ignore rules |
| Explicit file lists only | Exact | Unusable in an app |
| Proposal B's full scope recipe, with context roots | Very explicit | Heavy for V1. We adopted the persisted scope and dropped context roots |

**Pros**
- Works without Git or a repository.
- The scope is reproducible and shareable.
- Large monorepos can be narrowed before the first Compile.

**Cons**
- The ignore semantics differ from Git's in rare cases, which tests cover.
- Scope changes go into the shared settings. A personal scope layer is deferred until someone needs it (see "Still open").

### U-17: Freshness: verify before showing, cheap checks first

**Decided**
- **Before showing code**, read the file's bytes, hash them, compare the hash with the one the facts came from, and only then slice those same bytes.
- **Cheap checks first.** On reopen, and for each watcher event, compare size and modification time, and hash only when they differ.
- **Settings inputs are fingerprinted:** `.unfold/config.toml`, the `tsconfig.json` files and their `extends` chains, `package.json`, and `pyproject.toml`.
- **Installed dependencies aren't tracked.**

**Options**

| Option | For | Against |
| --- | --- | --- |
| Trust modification times | Fast | Times can lie, and then code would show against the wrong bytes |
| Rehash every file every time | Simple | Slow to reopen large folders |
| **Cheap checks, then hash** | Fast and correct | Two steps instead of one |
| Proposal B's full lookup provenance, recording every resolution lookup including missing candidates | Catches every context change | Unnecessary in V1, because the full relink already reacts to created and deleted files. It becomes part of V2's early cutoff |

**Pros**
- Code never shows against changed bytes.
- Folders reopen fast.

**Cons**
- Installing or upgrading packages isn't detected. The impact is low, because external labels don't depend on installation (PoC D-41).
- If the watcher misses an event, the graph can lag until the next Compile or reopen. Compile always checks every file.

## Storage and files

### U-18: A `.unfold/` folder in every repository

**Decided (your decision).** Every folder you compile gets a `.unfold/` folder at its root, where Unfold keeps everything about that folder:

```text
.unfold/
├── config.toml        # shared; written only when a setting differs from the defaults
├── boards/            # shared; published boards
└── local/             # machine-only; its .gitignore contains "*"
    ├── cache/         # disposable: graph.sqlite and the lock
    ├── drafts/        # your unpublished boards; not disposable
    └── state.json     # tabs, layout, last board, background-update setting
```

**Options**

| Option | For | Against |
| --- | --- | --- |
| Everything in the OS app-data folder (Proposal A's first design) | Nothing written into your folder until you publish | Data lives apart from its folder. Moving or deleting a folder orphans it, and it needs a registry mapping paths to projects |
| Nothing in the repository; boards through Save As (Proposal B) | Never touches your folder | Teammates who open the repository see no boards; sharing becomes manual file-passing |
| An opt-in `.unfold/` only for shared files (the review's recommendation) | Minimal writes | Information is split between the repository and app data |
| **`.unfold/` in every compiled folder, with a self-ignored `local/`** | Everything about a repository lives with it | Unfold writes into your folder after the first Compile |

**Why.** You decided that each repository gets its own `.unfold/` and that Unfold keeps its information there.

**Pros**
- Moving, renaming, or deleting a folder carries its graph, drafts, and settings with it, and nothing is left orphaned elsewhere.
- Clones and worktrees are independent.
- Settings and boards are shared and reviewed through Git like code.
- Coding agents working in the repository can read boards and settings directly.
- It's easy to inspect, and easy to reset by deleting `local/cache/`.
- Git shows nothing until you change a setting or publish a board, because `local/` ignores itself.
- It replaces the PoC's three locations (`.codecodegraph/`, `.codecodegraph-drafts/`, `codegraph/boards/`) with one.

**Cons**
- Unfold writes into your folder after the first Compile.
- Every clone and worktree has its own cache.
- Read-only folders, and network volumes where SQLite's write-ahead log is unsafe, need a fallback location in the OS cache folder.
- Tools that watch the whole repository might notice writes. Writes are confined to `local/cache/` and documented.
- Drafts sit near a disposable cache, so they get their own `drafts/` folder, and Clear graph cache never touches them. Deleting all of `local/` would lose them.
- Opening a subfolder of a repository creates a second `.unfold/` there.

### U-19: The graph is a SQLite cache

**Decided.** The graph, the per-file facts cache, and the search index live in one SQLite database at `.unfold/local/cache/graph.sqlite`:
- `rusqlite` with SQLite bundled;
- write-ahead logging;
- one transaction per update;
- FTS5 for search.

It is rebuilt whenever the engine version changes, so there are no database migrations.

**Options**

| Option | For | Against |
| --- | --- | --- |
| The PoC's six-file CSV package | Portable and reviewable | Needs the consistent-read protocol and directory swaps, can't be updated in part, and is slow to query |
| One immutable SQLite file per compile, swapped in through a pointer (Proposal B) | Immutable generations, and old ones are kept | Every background update would rewrite a whole file. Write-ahead logging gives the same atomic visibility with less machinery |
| **One SQLite database per folder** | Transactions, indexes, full-text search, partial updates | Needs a local filesystem |
| A custom memory-mapped binary format (for example, rkyv) | The fastest loads | We'd build our own transactions, indexes, and search |
| An embedded graph database | Graph queries | The queries are simple: children, incoming and outgoing edges, closures. The PoC showed maps are enough (D-3) |
| Memory only | Simplest | The graph is lost on every restart |

**Pros**
- Atomic commits.
- Readers see the last committed graph during an update.
- Updates can be partial.
- Fast indexed queries, so the UI never loads everything.
- Search comes with it.
- It's one file.

**Cons**
- Schema design work.
- Write-ahead logging needs a local filesystem, which the fallback location (U-18) handles.
- The database isn't a public format (U-21).

### U-20: Identity: file-scoped IDs and structural keys

**Decided.** Blocks are identified by `(file_id, local_id)`, numbered in preorder within each file. Structural keys are interned, so each key keeps its ID for the life of the database. Resolved targets, entrance targets, and board references all point at keys.

**Options**

| Option | For | Against |
| --- | --- | --- |
| Generation-wide sequential IDs (the PoC) | Simple and deterministic | Any change renumbers everything |
| IDs from content hashes | Stable for unchanged content | Can collide and change with every edit |
| **File-scoped IDs plus structural keys** | An edit renumbers only its own file, and edges into unchanged definitions stay valid | Ordinal keys shift when code is inserted before them |
| Persistent IDs assigned by the compiler | Survive edits | Needs a history of matches; the PoC deferred it |

**Pros**
- Updates stay small.
- Boards reconnect by key (PoC D-73).
- This identity is what makes V2's early cutoff possible.

**Cons**
- Ordinal keys such as `if[0]` shift, so boards keep guards and signatures.
- Renames and moves need a manual repair.
- Key strings take space, which interning reduces.

### U-21: Only files people commit are contracts

**Decided.** Versioned contracts are:
- board files;
- `.unfold/config.toml`;
- the canonical export, which is used by tests;
- the IPC types.

The graph database is private and can change in any release.

**Options.** We could keep a public graph format, like the PoC's CSV package or Proposal B's explicit graph contract. We chose to keep it private.

**Pros**
- We can change the storage freely.
- Compatibility work goes only where people commit files.

**Cons**
- Other tools can't consume the graph until a stable agent interface exists (V2).
- Scripts that read the SQLite file directly will break across versions.

### U-22: A fresh board format

**Decided**
- **Format.** `unfold-board` version 1, with a JSON Schema. A card's `state` is renamed `view`, and the `blocks` map is renamed `labels`. Everything else keeps the meaning it had in the PoC: positions, folds, groups, descriptions, guards, and signatures.
- **Storage.** Drafts live in `.unfold/local/drafts/`. **Publish** writes `.unfold/boards/<slug>.json`, and **Export** writes Mermaid wherever you choose.
- **No old boards.** Unfold doesn't read `codegraph-board` files. The sample "Order placement" board is converted once, as a test fixture.

**Options**

| Option | For | Against |
| --- | --- | --- |
| Keep `codegraph-board` version 3 and import older versions (Proposal A) | Old boards just work | Carries old names into a fresh product, and only two boards exist |
| **A fresh format with no promise** (Proposal B) | Clean names | Existing boards need a one-off conversion |
| Boards inside a database (Proposal B's state store) | Transactions | Can't be reviewed in Git, and agents can't edit them |

**Pros**
- Clean names.
- JSON files that diff and review well.
- Agents can write boards.
- Deterministic serialization keeps merge conflicts rare.

**Cons**
- The two existing boards need a one-off conversion.
- Concurrent edits to one board can still conflict in Git.
- Rich authoring (frames, notes, nested groups) waits for V2.

### U-23: Settings in TOML, in three places

**Decided**

| Place | Holds |
| --- | --- |
| `.unfold/config.toml` | Shared settings: scope, languages, disabled detectors, Python projects, manual entrances. Written only when a setting differs from the defaults. Versioned, unknown keys are rejected, and the app edits it while preserving comments |
| `.unfold/local/state.json` | Personal state for this folder: open tabs, layout, last board, whether background updates are on |
| The OS app-data folder | App preferences: theme, keyboard, editor command, background defaults, thread limit, and recent folders |

**Options**

| Option | For | Against |
| --- | --- | --- |
| JSON, as in the PoC's `codegraph.json` | Familiar in TypeScript repositories | No comments |
| **TOML** | Comments; familiar from Cargo and `pyproject.toml` | Less familiar to some TypeScript users |
| YAML | Common | Indentation pitfalls |
| Settings only inside the app (Proposal B) | Nothing in the repository | Teammates can't share manual entrances or scope |

**Pros**
- A clear split between shared, personal, and app settings.
- Comments explain choices.
- Manual entrances travel with the code.

**Cons**
- There's no personal scope override in V1, so a scope change made from the tree is a shared change.
- Teams may need a short note on what to commit.

## The app

### U-24: React UI: Rust computes, TypeScript lays out

**Decided.** The UI uses:
- React 19 and Vite;
- React Flow for the canvas;
- TanStack Query for data, and Zustand with Immer for state;
- TanStack Virtual for long lists;
- cmdk for the command palette, and lucide for icons.

Rust computes everything that needs the graph or the source bytes: the render models, highlighting, the Flow model, relations, closures, board health, and Mermaid. TypeScript does layout, because layout needs measured sizes: card sizing, reflow, Arrange, arrow routing, and node placement inside Flow.

**Options**

| Option | For | Against |
| --- | --- | --- |
| A native UI toolkit | Native feel | A full rebuild, and the Paper and CSS workflow is lost (U-6) |
| Svelte or Solid | Smaller runtimes | We'd lose the PoC's React components and React Flow |
| **React** | Reuses the PoC's components and React Flow | A larger runtime |
| Keeping the reader's derivations in TypeScript, as the PoC did | Reuse | Duplicates the engine's model, and the UI would need all the data |

**Pros**
- A thin UI with one source of truth.
- Derivations are testable headless in Rust.
- The v7.1 components and design tokens carry over.

**Cons**
- Two languages.
- Payloads must be designed to stay small, with pagination and data fetched per card.
- Layout logic sits apart from the data it lays out.

### U-25: Typed commands generated from Rust

**Decided.** Commands, events, and channels are defined in Rust. tauri-specta (pinned, because it's a release candidate) generates their TypeScript types, with ts-rs as the fallback. Queries are paginated.

**Options.** We considered hand-written TypeScript types, runtime schemas such as Proposal B's Zod schemas, and types generated from Rust (chosen).

**Pros**
- One definition, so the UI and the engine can't drift.
- Type errors appear at build time.

**Cons**
- A dependency on a release candidate.
- A code-generation step.
- Validation runs only in Rust. That's acceptable, because Rust validates every input anyway.

### U-26: An IDE-shaped window

**Decided.** The window has:
- an activity bar with Files, Entrances, Search, and Boards;
- tabs for boards and read-only source files;
- a right inspector and a bottom panel (Problems, Index log, References);
- a status bar and a command palette;
- a keyboard map;
- welcome and first-Compile screens.

**Options.** We could keep the PoC's single sidebar, which stacked boards, entrances, and a flat Definitions list capped at 400 items, or adopt the IDE layout both proposals described (chosen).

**Pros**
- Familiar to developers.
- Each job gets its own space.
- It scales to large repositories, with a virtualized tree and lists, and outlines inside the tree.

**Cons**
- More surfaces to build and test.
- The risk of drifting toward an editor, which the non-goals (U-4) guard against.

### U-27: The reader keeps the v7.1 rules

**Decided.** The rules from the v7.1 reader and from your Paper review comments hold everywhere:
- cards sized to their content, with no scrolling inside;
- every opened function as a full card;
- calls in the code as links, with arrows leaving from the calling line;
- no step numbers;
- Flow as a Mermaid-style diagram of block types and plain labels, never raw code;
- one quiet description line;
- cards fold only when you fold them;
- zoom is a camera, and Fit shows everything;
- no overlays;
- the top switch is a lens;
- the inspector is closed by default and never shows code.

Two small changes follow from these rules:
- Entrance cards list their handlers without numbered circles.
- Mermaid export uses the same labels as the app (the authored label, or the fallback name) instead of exact source, which the PoC used.

**Why.** These came from your own review of the designs, and they were built and approved in v7.1.

**Pros**
- A design that has already been validated.
- The rules are consistent across the app and exports.

**Cons**
- An exported diagram without authored labels shows names from the code, not the code itself. An export option can add source text if anyone needs it.

### U-28: Problems lists only real problems

**Decided.** Problems lists:
- files that don't parse or can't be read;
- settings errors;
- unsupported grammar versions;
- ambiguous Python module roots;
- board health.

Unresolved calls are facts about the code, not problems. They appear on call chips, in the inspector, and as counts in the Index log.

**Options.** We could list unresolved calls grouped by reason, as Proposal A did, or leave them out, as Proposal B did (chosen).

**Pros.** Problems stays short and actionable.

**Cons.** Someone auditing unresolved calls uses the Index log's counts. A filterable coverage view could come later.

### U-29: Inspector and board outline

**Decided.** The inspector is closed by default and never shows code. It gains **Reached from**, the list of entrances that statically reach the selection, which the PoC computed but never showed. The outline of the open board moves from the v7.1 right sidebar into the Boards view.

**Pros**
- The inspector is purely about the selection.
- The outline sits next to the list of boards.
- Reached-from answers "what calls this, ultimately?".

**Cons**
- The outline moves from where v7.1 put it.
- Reached-from closures cost some computation on large repositories, so they are computed on demand and cached.

### U-30: Search covers definitions, entrances, files, and text

**Decided.** One Search view has tabs for All, Definitions, Entrances, Files, and Text:
- names are found with FTS5 and ranked with `nucleo`;
- text is searched with ripgrep's libraries;
- ⌘P and ⌘T open quick-open.

**Options.** We could search the catalog first and add text search later, as Proposal B suggested, or include everything in V1, as Proposal A did (chosen).

**Pros**
- What developers expect from an IDE.
- Text search is cheap with ripgrep's libraries.

**Cons**
- Text search reads files, so it's bounded, streamed, and cancellable.
- More UI to build.

### U-31: One process with a locked-down webview

**Decided**
- **One process.** The engine runs on its own thread with a rayon pool, and SQLite has one writer and a pool of readers.
- **A locked-down webview.** It may call only Unfold's commands, every path is confined to the folder, and a strict content security policy applies.
- **Privacy.** No telemetry, and no network use other than the update check.

**Options**

| Option | For | Against |
| --- | --- | --- |
| Proposal B's process tree: a main process, a workspace service, and a disposable analysis process | Crash isolation, and a JavaScript heap released after each job | Necessary for a Node engine, not for Rust. It adds transport and supervision |
| **One process** | Simple, with no serialization between the engine and the store | A hard crash takes the app down |

**Pros**
- Simplicity.
- Rust's memory safety.
- A panic in one job is caught.

**Cons**
- A stack overflow on pathological input could still crash the app. Fuzzing, depth limits, larger stacks for engine threads, and autosaved drafts mitigate this.

**Revisit if** crashes appear in the field. The engine is a library, so compiles can move into a child process.

## Engineering

### U-32: Repository layout: a Cargo workspace and a pnpm UI

**Decided.** The repository holds:
- seven crates: `unfold-core`, `unfold-engine`, `unfold-lang-typescript`, `unfold-lang-python`, `unfold-store`, `unfold-reader`, and `unfold-workspace`;
- `desktop/`, the Tauri app;
- `ui/`, the React app;
- `testdata/`, `tools/` (with `xtask` and the oracles), `e2e/`, and `fuzz/`;
- docs, with one file per decision.

The dependency rules are in the [architecture](architecture.md#153-dependency-rules).

**Options**

| Option | For | Against |
| --- | --- | --- |
| Eleven crates (Proposal A) | Very fine boundaries | Premature, with boilerplate before there's code |
| One TypeScript package (Proposal B) | One toolchain | Not possible with a Rust engine |
| **Seven crates** | The heavy parser dependencies are isolated, which helps compile times, and every boundary can be tested headless | Some boundaries may need adjusting |

**Pros**
- Clear ownership.
- Builds and tests that can be parallelized.
- One copy of the test data.

**Cons**
- Two toolchains, Cargo and pnpm.
- Contributors need Rust.

### U-33: Correctness evidence: frozen PoC reference, divergence ledger, independent oracles

**Decided**

- **Freeze.** Before the PoC is retired, its outputs are frozen once: the six-file generations, the sources, the reports, the expected entrance trees, and the oracle outputs. They are imported into `testdata/reference/` and converted to Unfold's canonical form, with UTF-16 offsets converted to bytes and numeric IDs replaced by keys.
- **Ledger.** Every allowed difference is listed in `testdata/reference/divergences.toml` with its reason, such as "C-2 fixed: logger.info now resolves". An unlisted difference fails, and so does a listed difference that no longer occurs.
- **Oracles.** The TypeScript checker (through `@typescript/typescript6`) and Pyright 1.1.407's evaluator run as developer-only Node tools. Their outputs are cached by a hash of the corpus, so pull-request CI doesn't need Node.
- **Gates.** Every resolved call and named reference must agree with an oracle, which is 100% precision. Recall floors can only go up.

**Options**

| Option | For | Against |
| --- | --- | --- |
| Port the PoC's tests | Reuses known cases | Wrong language, and tied to the old format |
| Write new expected outputs from scratch, treating the PoC only as a behavioral reference (Proposal B) | No legacy | Loses an automatic check against 854 tests' worth of settled behavior |
| **A frozen reference, a divergence ledger, and independent oracles** (Proposal A) | An executable specification in which every change is explained, and known defects are listed instead of enshrined | Conversion work, and the ledger needs discipline |

**Pros**
- Nothing changes silently.
- Precision stays independently proven.

**Cons**
- Nightly CI needs Node.
- The reference can only be as good as the PoC's coverage, so new cases add new fixtures.

### U-34: Testing at every layer

**Decided**
- **Each crate** has its own kind of test: unit tests, property tests, snapshots, the alien language, and fault injection.
- **Detector cases** are data-driven: positive forms, decoys, aliases, spreads, and dynamic values.
- **Fuzzing** covers both language adapters and the backend: any bytes must produce a graph or a reported failure, never a crash.
- **Property tests** cover the UTF-8 and UTF-16 mapping, validator invariants, determinism, and incremental-equals-full over random edit sequences.
- **UI tests** run components on fixtures the engine generates, and Playwright journeys run against recorded command responses in Chromium and WebKit.
- **App journeys** run on built binaries with WebdriverIO.
- **Packaged checks** cover running with no Node or Python, paths with spaces and Unicode, read-only folders, and execution sentinels that prove nothing runs.

**Why.** The PoC earned trust by testing each layer independently. Background compilation adds one new invariant that needs its own test: a graph built by updates must equal a fresh compile.

**Pros**
- Each layer is checked on its own terms.
- Mistakes are caught close to their cause.

**Cons**
- A large test investment, and three-platform CI takes time and money.

### U-35: Platforms: macOS first, all three in CI

**Decided.** V1 is released on macOS, signed, notarized, and updating itself. Windows and Linux are built and tested in CI from the first commit, and released in V2 once their suites pass.

**Options**

| Option | For | Against |
| --- | --- | --- |
| Release all three in V1 | The widest reach | Slower to V1, and WebKitGTK risk on the critical path |
| macOS only, adding the others later (Proposal B's default) | Focus | Portability problems would surface late |
| **macOS first, with all three in CI** (Proposal A) | Focus, plus early warning | CI cost |

**Pros**
- Focus.
- Path and case-sensitivity bugs show up at once.

**Cons**
- CI time and cost.
- Linux rendering issues are known early but fixed later.

### U-36: Build order: a vertical slice first

**Decided**
1. **Phase 0** freezes the PoC and runs the spikes.
2. **Phase 1** builds a thin end-to-end TypeScript slice: open a folder, Compile, open an entrance, follow a call, save a file and watch the card update, then relaunch and find the draft.
3. **Later phases** deepen each layer.

The [migration plan](migration-plan.md) has the details.

**Options**

| Option | For | Against |
| --- | --- | --- |
| Layer by layer (Proposal A) | Each layer is finished before the next | Nothing usable until Phase 3, and Python waits until Phase 4 |
| **A vertical slice first** (Proposal B) | Usable early, and it proves the whole architecture, including the update path, before most of the code exists | Some rework as layers deepen |

**Pros**
- Early feedback on the UI and the update loop.
- Risks surface while they're still cheap to fix.

**Cons**
- The first slice covers few rules, so parity arrives later in the plan.

## What we decided against

| Option | Proposed by | Why not |
| --- | --- | --- |
| Electron with a TypeScript engine | Proposal B | A Rust engine was chosen (U-5), and Tauri follows from it (U-6) |
| The TypeScript 6 checker as the production analyzer | Proposal B | It's the retiring line, uses large amounts of memory, and can't be its own oracle (U-7) |
| The vendored Pyright parser | The PoC, Proposal B | It needs a JavaScript runtime (U-9) |
| tree-sitter parsers | Considered | Syntax only, and gaps in the Python grammar were measured (U-7, U-9) |
| A graph database | Considered | The queries are simple (U-19) |
| A public graph format, such as the CSV package | The PoC | Only committed files are contracts (U-21) |
| One SQLite file per compile, plus an activation pointer | Proposal B | Write-ahead logging gives the same atomicity with less machinery (U-19) |
| Nothing in the repository; Save As only | Proposal B | You chose `.unfold/` (U-18) |
| Keeping the graph only in app data | Proposal A | You chose `.unfold/` (U-18) |
| A manual Compile every time | Proposal B | You chose background updates (U-13) |
| Aborting a compile when a file doesn't parse | The PoC, Proposal B | Hostile during background updates (U-15) |
| Re-resolving only the edited file in V1 | Discussed | Silent stale links (U-14) |
| Reading old `codegraph-board` files | Proposal A | We're starting fresh (U-22) |
| Unresolved calls in Problems | Proposal A | Noise (U-28) |
| Eleven crates | Proposal A | Premature (U-32) |
| Building layer by layer | Proposal A | A usable app too late (U-36) |
| Recording every resolution lookup in V1 | Proposal B | V1 relinks fully. Lookups are recorded only for V2's early cutoff (U-17) |
| A multi-process engine | Proposal B | Not needed with Rust (U-31) |

## Still open

| Question | Until it's decided | Decided by or in |
| --- | --- | --- |
| Does the name "Unfold" pass a trademark check, and what's the bundle identifier? | "Unfold" is used internally | Phase 0, before the name reaches `.unfold/`, `unfold-board`, and the bundle identifier |
| Does V1 need the type checker? | No | The Phase 0 recall check (U-8) |
| Which real-world repositories to pin for recall checks and budgets | None pinned | Phase 0 |
| C-8: should destructured bindings from literal dynamic imports resolve? C-9: should property locators match calls? | The PoC's behavior | Phase 2, with fixtures |
| A personal scope layer (`.unfold/local/config.toml`) | Not in V1 | When someone needs it |
| Should Mermaid export optionally include source text? | Labels only | When someone asks |
| The final keyboard shortcuts | The [architecture's table](architecture.md#116-keyboard) | The Phase 6 review |
