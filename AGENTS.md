# Working in Unfold

## User constraints

- Do not perform git operations unless the user explicitly asks. The user handles git.
- Do not run database migrations. The user handles migrations.
- A requested design document is a file task: save the complete Markdown document under
  `docs/` (normally `docs/proposals/<topic>.md`) and link it in the final response.

## Start with the task

Unfold is a code-reading desktop app: React UI → Electron transport → Node addon → Rust
workspace, reader, and compiler. It statically analyzes TypeScript and Python; it never runs
the analyzed code. This repository has several independent areas; a task rarely needs all of them.

Choose the owning area below. The task map in
[docs/README.md](docs/README.md#find-the-context-for-your-task) links the relevant reference
sections when needed. `architecture.md` is a system-wide design reference;
`decisions-and-tradeoffs.md` is decision history. Neither is required reading for every task.

1. Find the owning area below, then read its entry point and a nearby example or test.
2. Search filenames with `rg --files <area>` and symbols with `rg -n '<symbol>' <area>`.
   Inspect headings before reading long docs: `rg -n '^#{1,3} ' docs/<document>.md`.
3. Expand to dependencies when an import, caller, invariant, or changed contract requires it.
   Once the behavior, relevant constraints, and affected files are clear, proceed with the task.

For a proposal, inspect the relevant reference section and implementation entry points, state
assumptions and open questions, and write the document. An unresolved design choice can remain
an open question; it does not require an exhaustive repo audit or a build.

## Code owners

| Area | Entry point / ownership |
| --- | --- |
| React UI | `ui/src/`; feature folders, `state/`, `ipc/`, and `theme/` |
| Electron and agent MCP | `desktop/src/main/`, `preload/`, `engine-host/`, `shared/`; MCP tools in `main/mcp/tools/` |
| Engine commands and project lifecycle | `crates/unfold-workspace/src/lib.rs`; `commands/`, `ipc/table.rs`, `projects.rs` |
| Reader views and board files | `crates/unfold-reader/src/lib.rs`; `code/`, `flow/`, `contracts/`, `board/` |
| Graph records and invariants | `crates/unfold-core/src/lib.rs`; `model.rs`, `typed.rs`, `vocab.rs`, `validate/`, `canonical/` |
| Language-neutral compiler | `crates/unfold-engine/src/lib.rs`; `language.rs`, `facts/`, `update/`, `resolve/` |
| Language extraction and linking | `crates/unfold-lang-typescript/src/lib.rs`, `crates/unfold-lang-python/src/lib.rs` |
| SQLite cache | `crates/unfold-store/src/lib.rs`; `schema.rs`, `writer/`, `read/` |
| Addon and developer tools | `crates/unfold-node/`, `tools/xtask/`; integration journeys in `e2e/` |

## Boundaries to preserve

- Resolve a call or type link only with proof of exactly one target; retain uncertainty.
- Source ranges are UTF-8 bytes in the engine and UTF-16 offsets in rendered code.
- Structural keys identify blocks across files; local IDs identify blocks within one file.
- Verify source against its facts hash before displaying it. Preserve deterministic output.
- The SQLite graph is a rebuildable cache. Published boards and shared config are contracts.
- Project commands name their project; paths are confined to it. Keep IPC payloads bounded.

Read the detailed invariant only when the task touches it. Existing crate and module doc
comments explain local contracts without requiring the full architecture document.

## Keep context focused

Start source searches in the owning directory. Include fixtures and goldens when investigating
or testing that behavior; avoid enumerating all of `testdata/` for application changes.
Lockfiles, `THIRD_PARTY_NOTICES.md`, build outputs, and generated files are useful for their
specific tasks, not general onboarding. Search a generated file for the needed type rather
than reading it in full. Consult the sibling `graph-compiler` only for a PoC-specific question.

Rust command types and `crates/unfold-workspace/src/ipc/table.rs` own the engine IPC contract.
`ui/src/ipc/generated/index.ts` is generated with `cargo xtask gen-ipc`; never edit it manually.
For implemented behavior, use the owning code and reference doc. Check a proposal's status:
a built proposal may retain its historical baseline and rationale.

## Validate the affected behavior

Default to the smallest checks that verify the changed behavior. Choose them from the affected
contracts, not from the size of the repository. After they pass, expand or repeat only for a
new change, a failure, or a specific unresolved risk.

| Change | Start with |
| --- | --- |
| Docs only | Check local links, referenced paths, and consistency; no application build required |
| One Rust area | `cargo test -p <crate> <test-filter>`; broaden when shared contracts change |
| UI | `pnpm --filter @unfold/ui test <test-file>` and `pnpm --filter @unfold/ui typecheck` |
| Desktop | `pnpm --filter @unfold/desktop test <test-file>` and `pnpm --filter @unfold/desktop typecheck` |
| Engine IPC contract | Regenerate types, then `cargo xtask gen-ipc --check` and affected Rust/UI checks |

Use [docs/testing.md](docs/testing.md) for the gates relevant to the change. Full workspace
tests, release-addon builds, packaging, oracles, and E2E are separate checks; run them when
the changed boundary or the user's request calls for them. Report what was actually checked.

**E2E is conditional.** Run a matching journey when the changed behavior needs the running
app to verify a workflow across screens, processes, or persistence (such as Publish, window
restore, or background updates), a regression depends on Electron or rendered interactions
that focused tests cannot reproduce, or the user explicitly requests E2E. Docs, proposals,
isolated logic, and styling that does not change interactions do not need E2E by default.
Run the relevant spec or test first; reserve the full suite for an explicit request or broad
app changes whose integration risk cannot be bounded to specific journeys. Budget measurements
are for performance changes; packaged smoke tests are for packaging and packaged-runtime changes.
`just e2e` invokes all app builds; repeat focused runs with `pnpm --filter @unfold/e2e test <spec>`
only while those builds match the current sources. See [check selection](docs/testing.md#choose-checks-for-a-local-change).
