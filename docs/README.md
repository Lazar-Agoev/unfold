# Unfold's documentation

Each fact has one owner. Other documents link to it instead of repeating it. The design documents describe
what Unfold should be and why; the reference documents describe what is built, and change with the code.

## Find the context for your task

Start with the row that matches the task, then follow dependencies as needed. The architecture
and decision history are references, not a required onboarding sequence. Crate `src/lib.rs`
and module doc comments provide short implementation overviews.

| Task | Start in code | Read the relevant section |
| --- | --- | --- |
| Write a proposal | Entry points for the affected area in [AGENTS.md](../AGENTS.md#code-owners) | The reference below that owns the behavior; consult individual decisions when relevant |
| UI feature or styling | `ui/src/<feature>/`, `ui/src/state/`, `ui/src/theme/` | [UI shell](ui-shell.md) |
| Add or change an engine command | `crates/unfold-workspace/src/commands/`, `src/ipc/table.rs`, `ui/src/ipc/queries.ts` | [Adding a command](ipc.md#adding-a-command) |
| Electron, windows, or transport | `desktop/src/main/`, `preload/`, `engine-host/`, `shared/` | [Desktop overview](../desktop/README.md), [How a command travels](ipc.md#how-a-command-travels) |
| Agent MCP tool or edit | `desktop/src/main/mcp/tools/`, `desktop/src/main/agents/`, `ui/src/state/boards/agent/` | Tool module comments and the MCP teaching files under `desktop/src/main/mcp/` |
| Board format or persistence | `crates/unfold-reader/src/board/`, `crates/unfold-workspace/src/commands/boards/` | [Boards](reader.md#boards), [Board schema](schemas/unfold-board.schema.json) |
| Code, Flow, or typed lines | `crates/unfold-reader/src/code/`, `flow/`, `contracts/`; corresponding UI folders | [Code](reader.md#code-view), [Flow](reader.md#flow-view), [Typed lines](reader.md#typed-lines) |
| Add a graph fact | `crates/unfold-core/src/model.rs`, `typed.rs`, `vocab.rs`, `validate/`, `canonical/`; affected extractor | [Records](graph-model.md#records), [Validation invariants](graph-model.md#validation-invariants); storage and readers if affected |
| SQL schema or query | `crates/unfold-store/src/schema.rs`, `writer/`, `read/` | [Store overview](../crates/unfold-store/src/lib.rs), [Graph records](graph-model.md#records) |
| Compile, watcher, or project lifecycle | `crates/unfold-engine/src/update/`; `engine_thread.rs`, `watcher.rs`, and `projects.rs` in `crates/unfold-workspace/src/` | [Compiling and updates](compiling.md) |
| Language resolution | `crates/unfold-lang-typescript/src/` or `crates/unfold-lang-python/src/`; `crates/unfold-engine/src/resolve/` | [TypeScript](languages/typescript.md) or [Python](languages/python.md) |
| Entrance detector | The language crate's `src/detect/`, matching `tests/entrances.rs` and `testdata/detectors/<detector>/` | [Entrances](entrances.md) |
| Settings or file scope | `crates/unfold-engine/src/settings/`, `discovery/`, `crates/unfold-workspace/src/settings_edit.rs` | [Settings and scope](settings-and-scope.md) |
| Choose checks or investigate a failure | Nearby tests first; integration tests in the affected crate | [Testing](testing.md) |

Use `rg -n '^#{1,3} ' docs/<document>.md` to find the section before reading a long document.
For a proposal, save the Markdown file under `docs/proposals/`, mark its status, describe the
change and affected contracts, and record assumptions or open questions. Inspect the current
implementation without building the app just to write the proposal.

Implementation shapes and versions live in code: Rust records own the graph model, the store's
`schema.rs` owns executable SQL, and Rust command types own IPC shapes. Generated TypeScript is
their UI mirror. Reference docs own the semantics; if code and a reference disagree, identify
the discrepancy instead of silently treating one as a new design decision. Built proposals
retain design history and may describe a pre-implementation baseline.

## Design

| Document | Owns |
| --- | --- |
| [Architecture](architecture.md) | What we build: the product and its guarantees, the engine, compiling and storage (including `workspace.json`, `windows.json`, and `state.json`), the formats, the reader, the desktop app (the workspace, projects and their liveness, the studio window, Home, Add, the peek sheet, boards across projects, several windows), commands, security, packaging, repository layout, testing layers, and performance targets |
| [Decisions and trade-offs](decisions-and-tradeoffs.md) | Why: how the design came about (the proposals, the move to Electron, the studio redesign, agents in Unfold), every decision (U-1 to U-63) with the options weighed, what was rejected, and what is still open |

## Proposals and design history

| Document | Status and role |
| --- | --- |
| [Typed lines](proposals/typed-lines.md) | Built. Original design, pre-implementation baseline, and rationale; current behavior is documented in the graph model, reader, and IPC references |

## Reference

| Document | Owns |
| --- | --- |
| [Graph model](graph-model.md) | The records (files, inputs, blocks, slots, edges, entrances, targets), their vocabularies, ranges, structural keys, canonical order and export, validation invariants, value policy, reason order, module labels, provenance, and labels |
| [Compiling and updates](compiling.md) | The engine's single update entry point, its stages, the facts cache, files that don't parse, settings and inputs, cancellation and threads, progress and statistics, reopening, determinism, and the workspace lifecycle |
| [Settings and scope](settings-and-scope.md) | `.unfold/config.toml` (keys, the project's shared name, strictness, Python projects, how the app edits it), file discovery and classification, the skipped-files report, scope, and settings hashing |
| [Entrances](entrances.md) | The launch-command grammar, manual entrances and locators, every TypeScript and Python detector's rules, mixed repositories, and decision C-9 |
| [TypeScript](languages/typescript.md) | The TypeScript front end: parsing, syntax mapping, project configuration, modules, exports, the emit map, binding, evidence, linking, known limits, decisions S2 and C-8, and parity |
| [Python](languages/python.md) | The Python front end: grammar versions, encodings, syntax mapping, source roots and imports, the binder, module labels, known limits, detectors and problems, and parity |
| [Reader](reader.md) | Derived views (owner, catalog, closure, applications, relations), the Code and Flow views, boards (format, pages and folders, cards from other projects, signatures, guards, drawn links, health, repairs, outlines, files, drafts, Publish, and unpublished changes), and Mermaid export |
| [Commands and events (IPC)](ipc.md) | How a command travels, trusted commands, liveness per command, the addon's surface and closing writes, every engine command, window command, and event, error codes, payload limits, and how to add a command |
| [UI shell](ui-shell.md) | The renderer as built: module layout, the frame, the state model, data from the engine and invalidation by project, screen details, overlays, the keyboard map and custom bindings, theme tokens, components, and where V1's parts went |
| [Testing](testing.md) | The test kinds, how to run each gate, the golden and divergence-ledger policies, precision and recall results, the proof of concept's baseline, and measured budgets |
| [User guide](user-guide.md) | Using the app, in plain language |
| [Board schema](schemas/unfold-board.schema.json) | The JSON Schema of `unfold-board` v1 files (described in [the reader](reader.md#format)) |

## Elsewhere in the repository

- **Crates.** Each crate's `src/lib.rs` begins with a `//!` overview of what it owns, and every module and public
  item has its own doc comment; `cargo doc --workspace --no-deps --open` renders them. The crates and their
  dependency rules are in [architecture §15](architecture.md#15-repository-layout).
- [desktop/README.md](../desktop/README.md): the Electron processes, the transport, window commands, window restore,
  packaging, and the smoke test.
- [testdata/README.md](../testdata/README.md): the playgrounds, fixtures, detector cases, and sample board, and their
  conventions. [testdata/reference/README.md](../testdata/reference/README.md): the frozen proof-of-concept
  reference, the divergence ledger, and the oracles.
- [tools/oracles/README.md](../tools/oracles/README.md): the developer-only Node scripts (the TypeScript checker
  oracle, the reader export, the Mermaid and schema checks).
- [fuzz/README.md](../fuzz/README.md): the fuzz targets.
- [testdata/fixtures/typescript-evidence/README.md](../testdata/fixtures/typescript-evidence/README.md): the
  TypeScript evidence cases and their deliberate differences from the proof of concept.
