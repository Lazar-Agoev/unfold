# Working in Unfold

## User constraints

- Do not perform git operations unless the user explicitly asks. The user handles git.
- Do not run database migrations. The user handles migrations.
- A requested design document is a file task: save the complete Markdown under `docs/`
  (normally `docs/designs/<topic>.md`) and link it in the final response.

## Product vision

Read [the fixed product vision](docs/product-vision.md) before proposing or changing product behavior or UX.
Proposals and designs must follow it. Do not edit it unless the user explicitly requests a vision change.

## Start with the task

Unfold statically reads TypeScript/Python using a Rust engine, Electron shell, and React UI.
Choose the owning area below; [docs/README.md](docs/README.md) links its focused reference.
The architecture is an optional overview, not mandatory onboarding.

1. Find files with `rg --files <area>`; search symbols with `rg -n '<symbol>' <area>`.
2. Read the entry point, relevant module comment, and a nearby example/test. For a long
   reference, find headings first: `rg -n '^#{1,3} ' docs/<document>.md`.
3. Expand only for an affected dependency, caller, invariant, or contract. Once behavior,
   constraints, and affected files are clear, implement; do not audit the entire repo.
4. For a design, inspect the affected reference/code, state assumptions/open questions,
   and write the document. Unresolved choices do not require a build or exhaustive research.

## Code owners

| Area | Entry point |
| --- | --- |
| React | `ui/src/<feature>/`, `state/`, `ipc/`, `theme/` |
| Electron/MCP | `desktop/src/main/`, `preload/`, `engine-host/`, `shared/`; `main/mcp/tools/` |
| Commands/project lifecycle | `crates/unfold-workspace/src/lib.rs`, `commands/`, `ipc/table.rs`, `projects.rs` |
| Reader/boards | `crates/unfold-reader/src/lib.rs`, `code/`, `flow/`, `contracts/`, `board/` |
| Graph contracts | `crates/unfold-core/src/lib.rs`, `model.rs`, `typed.rs`, `vocab.rs`, `validate/`, `canonical/` |
| Compiler | `crates/unfold-engine/src/lib.rs`, `facts/`, `update/`, `resolve/`, `settings/`, `discovery/` |
| Language extraction/linking | `crates/unfold-lang-{typescript,python}/src/lib.rs` |
| SQLite cache | `crates/unfold-store/src/lib.rs`, `schema.rs`, `writer/`, `read/` |
| Addon/tools/journeys | `crates/unfold-node/`, `tools/xtask/`, `e2e/` |

## Preserve these boundaries

- Resolve calls/type names only with proof of one permitted target; retain uncertainty.
- Record written structure/types, preserve deterministic output, and never execute analyzed code.
- Engine ranges are UTF-8 bytes; rendered ranges are UTF-16. Keys cross files; local IDs do not.
- Verify source against the facts hash before displaying or signing it.
- SQLite is a rebuildable cache; published boards/shared config are contracts.
- Project commands name their project, confine paths, and bound payloads.

Source owns exact fields/enums/versions/SQL/command names. References own semantic rules.
If they disagree, identify the discrepancy. Rust command types/table generate
`ui/src/ipc/generated/index.ts` via `cargo xtask gen-ipc`; never edit the mirror manually.

## Keep context focused

Search the owning directory first. Include fixtures/goldens for the relevant behavior;
avoid enumerating all test data. Read lockfiles, generated files, license notices, and
build outputs only for their specific tasks. Search one generated type rather than loading
its entire file. Sibling PoC code and product MCP teaching assets are optional task-specific context.

## Validate the affected behavior

Choose the smallest checks that verify the changed contract, then stop when they pass.
Repeat/broaden only after another edit, failure, or specific unresolved risk.

- Docs/design: verify links, paths, semantics; no app build. Static doc-coverage checks may apply.
- Rust: `cargo test -p <crate> <filter>`; broaden for affected shared contracts.
- UI/desktop: `pnpm --filter @unfold/<package> test <file>` and that package's `typecheck`.
- IPC types: regenerate, `cargo xtask gen-ipc --check`, and affected Rust/UI checks.

**App E2E is conditional:** run a matching journey when a changed integrated workflow needs
the running app, an Electron/rendered-interaction regression cannot be reproduced in focused
tests, or the user requests it. Docs, isolated logic, and styling without interaction changes
do not trigger E2E. Full suites require an explicit request or broad app changes that cannot
be covered by a bounded set of journeys. Budget and packaged-smoke checks apply to their own
performance/packaging boundaries.

`just e2e` invokes all app builds. Repeat selected tests with
`pnpm --filter @unfold/e2e test <spec>` only while those outputs match current sources.
Use [testing](docs/testing.md#choose-checks-for-a-local-change) for details and report actual checks.
