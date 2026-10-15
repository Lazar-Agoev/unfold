# Documentation

Start with [AGENTS.md](../AGENTS.md) for working rules and code owners. Pick the reference for your task, find its
headings, then read the owning code and a nearby test. There is no required reading order.

For product behavior and UX, read [the product vision](product-vision.md). It is fixed: designs and code follow it,
and only the owner changes it.

## References

| Task | Reference | Code |
| --- | --- | --- |
| Processes, ownership, guarantees, storage, security | [Architecture](architecture.md) | Each crate's `src/lib.rs`; `desktop/src/main/` |
| Blocks, links, what a call's value is, typed records, keys, validation, the canonical export | [Graph model](graph-model.md) | `crates/unfold-core/src/` |
| `unfold.toml`, entrances, discovery, the update pipeline, the caches, project lifecycle | [Engine](engine.md) | `crates/unfold-engine/src/`, `crates/unfold-store/src/`, `crates/unfold-workspace/src/` |
| TypeScript or Python lowering, binding, and resolution | [TypeScript](languages/typescript.md), [Python](languages/python.md) | `crates/unfold-lang-typescript/src/`, `crates/unfold-lang-python/src/` |
| The drawn flow, the overlay, the code panel's model, boards and their health | [Reader](reader.md) | `crates/unfold-reader/src/` |
| Commands, events, transport, adding a command | [IPC](ipc.md) | `crates/unfold-workspace/src/commands/`, `ipc/table.rs`; `ui/src/ipc/` |
| Screens, state, the canvas, the code panel, agents in the window | [UI shell](ui-shell.md) | `ui/src/` |
| Electron, the MCP server, the render window, packaging | [Desktop](../desktop/README.md) | `desktop/src/` |
| Choosing checks, goldens, oracles, journeys, CI | [Testing](testing.md) | Nearby tests; `e2e/` |
| Using the app | [User guide](user-guide.md) | |

## Contracts

- The [board JSON Schema](schemas/unfold-board.schema.json) is the shared board-file contract. It is a copy of
  `crates/unfold-reader/schema/unfold-board.schema.json`. A reader test checks that the two are identical and that
  the schema states the reader's limits; `tools/oracles/check-board-schema.mjs` validates the sample boards against
  it.
- `unfold.toml` is the shared settings contract ([Engine](engine.md)).
- The SQLite cache is private and rebuildable.

References explain rules and meaning. The code owns exact fields, enum lists, versions, SQL, command names, caps, and
generated types: link to them rather than copying them. When code and a reference disagree, say so instead of
picking one silently.

## Design records

- `designs/` holds the design documents and build records: core scope, diagram blocks, entrance definitions,
  viewing code, MCP capabilities, the core rebuild plan and its build log, with their HTML pages.
- `analysis/` holds an analysis of the compiler.

The references above hold the rules that stay true; these records keep the reasoning and the history. A new design
goes in `designs/<topic>.md` with a status line, the contracts it touches, and its open questions. Once it is built,
fold its lasting rules into the reference that owns the subject.

## Other material

- [Test data](../testdata/README.md), [oracles](../testdata/reference/README.md), and
  [oracle tools](../tools/oracles/README.md).
- `desktop/src/main/mcp/guide/` teaches agents that use Unfold through MCP, and `desktop/src/main/mcp/tasks.md` is
  the manual suite for tuning that teaching. Both are product content, not onboarding for working in this
  repository.
