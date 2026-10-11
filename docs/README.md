# Unfold's documentation

Each fact has one owner. Other documents link to it instead of repeating it. The three design documents describe
what Unfold should be and why; the reference documents describe what is built, and change with the code.

## Design

| Document | Owns |
| --- | --- |
| [Architecture](architecture.md) | What we build: the product and its guarantees, the engine, compiling and storage (including `workspace.json`, `windows.json`, and `state.json`), the formats, the reader, the desktop app (the workspace, projects and their liveness, the studio window, Home, Add, the peek sheet, boards across projects, several windows), commands, security, packaging, repository layout, testing layers, and performance targets |
| [Decisions and trade-offs](decisions-and-tradeoffs.md) | Why: how the design came about (the proposals, the move to Electron, the studio redesign, agents in Unfold), every decision (U-1 to U-63) with the options weighed, what was rejected, and what is still open |

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
