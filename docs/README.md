# Documentation

Start with [AGENTS.md](../AGENTS.md) for working rules and code ownership. Choose one reference
below, search its headings, then read the owning code and a nearby test. There is no required
sequence through the docs.

For product behavior and UX, read [the fixed product vision](product-vision.md). Proposals, designs, and
implementation must follow it; only an explicit user request to change the vision authorizes editing it.

## Find the context for your task

| Task | Reference | Implementation entry point |
| --- | --- | --- |
| System/process boundary | [Architecture](architecture.md) | Crate `src/lib.rs` overviews; `desktop/src/main/` |
| Record, key, range, validation, export | [Graph model](graph-model.md) | `crates/unfold-core/src/{model,typed,vocab,key}.rs`, `validate/`, `canonical/` |
| Compile, cache, watcher, project lifecycle | [Engine](engine.md#updates) | `crates/unfold-engine/src/update/`, `crates/unfold-workspace/src/` |
| Shared settings or discovery | [Config/scope](engine.md#shared-config) | `crates/unfold-engine/src/settings/`, `discovery/`, workspace `settings_edit.rs` |
| Entrance or launch command | [Entrances](engine.md#entrance-detectors) | Language `src/detect/`; engine `launch/`, `entrances/`, `locator.rs` |
| TypeScript/Python extraction or resolution | [TypeScript](languages/typescript.md), [Python](languages/python.md) | The relevant language crate's `src/lib.rs` and module |
| Code, Flow, types, boards, Publish/repair | [Reader](reader.md) | `crates/unfold-reader/src/`; workspace `commands/boards/` |
| New command, transport, event, invalidation | [IPC](ipc.md#adding-a-command) | Workspace `commands/`, `ipc/table.rs`; `ui/src/ipc/queries.ts` |
| UI state, interaction, keyboard, theme | [UI shell](ui-shell.md) | `ui/src/<feature>/`, `state/`, `keyboard/`, `theme/` |
| Electron, MCP, windows, packaged runtime | [Desktop](../desktop/README.md) | `desktop/src/main/`, `engine-host/`, `preload/`, `shared/` |
| Choose checks | [Testing](testing.md#choose-checks-for-a-local-change) | Nearby tests, then the relevant crate/journey |
| Using the product | [User guide](user-guide.md) | User-facing behavior; optional for development |

## Ownership

References explain current semantics and constraints. Code owns exact fields, enum lists,
version numbers, SQL, command names, numeric caps, and generated types. Link to those definitions
instead of copying them into another guide. If implementation and a semantic requirement
disagree, identify the discrepancy rather than silently inventing a new requirement.

The [board JSON Schema](schemas/unfold-board.schema.json) remains the shared file-format
contract, paired with the reader's model and parsing tests. SQL is a rebuildable private cache.

## Design documents

When requested, save a scoped design under `docs/designs/<topic>.md`, mark its status, name
its affected contracts, and record assumptions/open questions. Read only the relevant current
reference and implementation. After implementation, fold lasting requirements into the owning
reference instead of keeping a second description of current behavior.

## Local reference material

These are optional for their specific tasks:

- [Test-data conventions](../testdata/README.md) and [frozen reference/oracles](../testdata/reference/README.md).
- [Oracle tools](../tools/oracles/README.md) and [fuzzing](../fuzz/README.md).
- `desktop/src/main/mcp/guide/` and `tasks.md`: product teaching assets for agents using
  Unfold, not onboarding material for agents developing this repository.
