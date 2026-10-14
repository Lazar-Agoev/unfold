# Working in Unfold

## User constraints

- Do not perform git operations unless the user explicitly asks. The user handles git.
- Do not run database migrations. The user handles migrations.
- A requested design document is a file task: save the complete Markdown under `docs/`
  (normally `docs/designs/<topic>.md`) and link it in the final response.

## Product vision

Read [the product vision](docs/product-vision.md) before proposing or changing product behavior or UX. Designs and
code follow it. Do not edit it unless the user explicitly asks for a vision change.

## Start with the task

Unfold reads TypeScript and Python statically with a Rust engine, and shows the result in an Electron shell with a
React UI. Find the owning area below; [docs/README.md](docs/README.md) links its reference.

1. Find files with `rg --files <area>` and symbols with `rg -n '<symbol>' <area>`.
2. Read the entry point, its module comment, and a nearby test. In a long reference, list the headings first:
   `rg -n '^#{1,3} ' docs/<document>.md`.
3. Widen only for an affected caller, dependency, invariant, or contract. Once the behavior and the files are clear,
   implement; don't audit the whole repository.
4. For a design, read the affected reference and code, state assumptions and open questions, and write the document.

## Code owners

| Area | Entry points |
| --- | --- |
| Graph model: nodes, links, typed records, keys, validation, canonical export | `crates/unfold-core/src/`: `lib.rs`, `model.rs`, `vocab.rs`, `typed.rs`, `key.rs`, `validate/`, `canonical/` |
| Compiler: settings and entrances, discovery, update pipeline, resolution | `crates/unfold-engine/src/`: `lib.rs`, `settings/`, `entrances.rs`, `discovery/`, `update/`, `facts/`, `resolve/`, `links.rs` |
| Language front ends: lowering, binding, linking | `crates/unfold-lang-{typescript,python}/src/`: `lib.rs`, `lower/`, `bind/`, `link` |
| SQLite cache | `crates/unfold-store/src/`: `lib.rs`, `schema.rs`, `writer/`, `read/` |
| Reader: drawn flow, overlay, code panel model, contracts, boards | `crates/unfold-reader/src/`: `lib.rs`, `tree.rs`, `catalog.rs`, `drawing/`, `overlay/`, `code_view/`, `contracts/`, `board/` |
| Projects, commands, events, watcher | `crates/unfold-workspace/src/`: `lib.rs`, `projects.rs`, `open_boards.rs`, `commands/`, `ipc/table.rs`, `watcher.rs` |
| React UI | `ui/src/`: `<feature>/` (`home/`, `canvas/`, `flow/`, `code/`, `add/`, `explain/`, `entrances/`), `state/`, `ipc/`, `theme/` |
| Electron shell and MCP server | `desktop/src/`: `main/`, `engine-host/`, `preload/`, `shared/`; tools in `main/mcp/tools/` |
| Addon, developer tasks, journeys | `crates/unfold-node/`, `tools/xtask/`, `tools/oracles/`, `e2e/` |

## Keep these guarantees

- A call or type name resolves only with proof of exactly one target; otherwise it stays unknown, with its reason.
- Only written facts are recorded, never inferred ones. Analyzed code never runs.
- Output is deterministic: the same code and settings give the same graph, byte for byte.
- Source is verified against its hash before it is shown or signed. Files that stop parsing keep their last good
  facts.
- Engine ranges are UTF-8 bytes; rendered ranges are UTF-16.
- Only `unfold-core`'s key module parses keys. Everything else treats a key as an opaque string and gets kinds,
  parents, and files from records.
- Writes into a project stay inside its `.unfold/`, plus `unfold.toml` when its settings change. Project commands name their
  project, confine paths, and bound payloads. The MCP server answers only this machine.
- The board file and `unfold.toml` are shared contracts; the SQLite cache is rebuildable. When a cache shape changes,
  bump the store's `SCHEMA_VERSION` or the facts payload versions and the cache rebuilds.

## Keep the code small

- **One way to do each thing.** No adapters, second paths, or wrappers kept for old callers.
- **Formats have one version.** No converters, aliases, or readers for older shapes; an old file fails like any
  invalid file.
- **Delete means delete:** code, tests, fixtures, docs, CSS, settings, commands, generated types, tools, and
  dependencies together. No stubs, commented-out code, or `allow(dead_code)`. `pnpm knip` and `cargo machete` find
  leftovers.
- **Generated files:** the Rust command types generate `ui/src/ipc/generated/index.ts` through
  `cargo xtask gen-ipc`. Never edit it by hand.
- The code owns exact fields, enums, versions, SQL, and command names; references own the rules. When they
  disagree, say so.

## Keep context focused

Search the owning folder first. Open only the fixtures and goldens of the behavior you're changing. Read lockfiles,
generated files, notices, and build output only for a task about them, and search a generated file rather than
loading it. The MCP guide in `desktop/src/main/mcp/guide/` teaches agents that use Unfold; it isn't onboarding for
this repository.

## Validate the affected behavior

Pick the smallest checks that verify the changed contract, and stop when they pass.

- Docs: links, paths, and what the code does; no build.
- Rust: `cargo test -p <crate> <filter>`; widen for shared contracts. Run `source "$HOME/.cargo/env"` first if
  `cargo` isn't on `PATH`.
- UI or desktop: `pnpm --filter @unfold/<package> test <file>` and that package's `typecheck`.
- IPC types: regenerate, then `cargo xtask gen-ipc --check` and the affected Rust and UI checks.
- Goldens: `UNFOLD_BLESS=1` rewrites them; read the diff before keeping it.

**E2E is conditional.** Run a matching journey when a changed workflow needs the running app, when a regression needs
Electron or rendered interaction that focused tests can't reach, or when asked. Run every journey only when asked or
for a broad app change. `just e2e` builds the app first; `pnpm --filter @unfold/e2e test <spec>` repeats a journey
while the builds match the sources. [Testing](docs/testing.md#choose-checks-for-a-local-change) has the details;
report the checks you actually ran.
