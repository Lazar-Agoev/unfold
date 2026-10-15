# Testing

Pick the smallest checks that verify the contract you changed, and stop when they pass. Broaden only after another
edit, a failure, or a specific risk you can name. CI runs the broad gate on every pull request.

## Choose checks for a local change

| Change | Start with | Add |
| --- | --- | --- |
| Docs | Links, paths, and what the code does | Nothing: prose needs no build |
| Records, keys, validation, canonical export (`unfold-core`) | `cargo test -p unfold-core` | Both language crates, the reader, and the workspace goldens: every layer reads these shapes |
| Lowering, binding, linking (a language crate) | That crate's tests and the goldens they write | `cargo xtask oracles check` on the in-repo corpora when a call's outcome can change |
| Settings, entrances, the update pipeline (`unfold-engine`) | `cargo test -p unfold-engine` | The language crates' goldens when entrance states can change |
| The cache (`unfold-store`) | `cargo test -p unfold-store` | Workspace tests when a read changes shape |
| Drawing, overlay, code panel model, boards (`unfold-reader`) | `cargo test -p unfold-reader` | `ui/src/flow` tests when a drawn flow changes; they read `testdata/goldens/drawn/` |
| Commands, events, IPC types | `cargo test -p unfold-workspace --test commands`, `cargo xtask gen-ipc --check` | UI tests of the queries the command feeds |
| UI component, state, styling | Vitest for the file, `pnpm --filter @unfold/ui typecheck` | A journey only when the change is a workflow the unit tests can't drive |
| Desktop shell, MCP tools, the guide | Vitest for the file, `pnpm --filter @unfold/desktop typecheck` | `teaching.test.ts` for guide and tool-description changes |
| An integrated workflow | The matching journey | |
| Packaging | `just bundle-dir` | |

**E2E is conditional.** Run a journey when the changed workflow needs the running app, when a regression needs
Electron or rendered interaction that focused tests can't reach, or when asked. Docs, isolated logic, and styling
without interaction changes don't trigger it. Run every journey only when asked, or when a broad app change can't be
bounded to specific journeys. Report the checks you actually ran and any coverage gap that matters.

## Focused commands

```sh
cargo test -p unfold-reader <filter>
cargo test -p unfold-lang-python --test python <filter>
pnpm --filter @unfold/ui test <file>
pnpm --filter @unfold/desktop test <file>
pnpm --filter @unfold/ui typecheck
cargo xtask gen-ipc --check
```

Run `source "$HOME/.cargo/env"` first if `cargo` isn't on `PATH`. Format and lint what you touched.

## Where tests live

| Package | Tests |
| --- | --- |
| `unfold-core` | Unit tests beside the code; `testkit` (the `testkit` feature) holds `GraphBuilder`, the sample `shop` graph, `assert_golden`, and `spans_are_well_formed` for highlighters |
| `unfold-engine` | `tests/backend/` (the shared pipeline through an in-memory test language, determinism, validation, updates), `tests/entrances/` (`[[app]]` tables with the real front ends: states, fixes, settings-only edits), `tests/discovery.rs` |
| `unfold-lang-python` | One binary, `tests/python/`: lowering, binder, typed records, census, failures, robustness, goldens |
| `unfold-lang-typescript` | One binary, `tests/structure/`: lowering, evidence, resolution, typed records, census, configuration, goldens |
| `unfold-store` | `tests/store/`; fault injection and concurrency in `src/tests/`; the `testkit` feature adds `Reader::stats`, the row counts the tests compare |
| `unfold-reader` | Unit tests beside each module (drawing, overlay, contracts, code panel, boards) on golden graphs loaded through the canonical reader, so nothing compiles; `tests/` for the two sample boards |
| `unfold-workspace` | `tests/commands/` (every command through `Host`, as the addon calls it), `tests/workspace/`, `tests/incremental.rs` (random edit sequences must equal a full compile), `tests/goldens.rs`; the `testkit` feature adds `Workspace::notify_changes`, how tests with the watcher off report changed paths |
| `unfold-node` | `test/addon.test.mjs`, through the built addon (`just addon-test`) |
| `ui`, `desktop` | Vitest files beside the code. UI component tests answer engine commands from the command goldens (`ui/src/test/engine.ts`); desktop tests use a fake Electron (`desktop/src/main/fake-electron.ts`) |
| `e2e` | Five Playwright journeys ([below](#end-to-end-journeys)) |

Every compiled graph in a test passes validation, including the language's **census**: each call in the parse tree
is exactly one call node.

## Goldens

Tests compile temporary copies of `testdata/` folders and compare the output with files there. The layout of
`testdata/` is in [its README](../testdata/README.md).

| Golden (under `testdata/`) | Written by | Holds |
| --- | --- | --- |
| `goldens/{ts-orders,py-orders}.jsonl` | Each language crate's `goldens.rs` | The playground's canonical export. The reader's tests load it |
| `goldens/{ts-orders,py-orders}.entrances.txt` | The same tests | The state of every `code` key in the playground's `[[app]]` tables, with fixes |
| `goldens/mixed-playground.*`, `goldens/typed-lines.*` | `unfold-workspace/tests/goldens.rs` | The two-language folders: export, entrance states, and `typed-lines.flows.txt` |
| `goldens/{ts-orders,py-orders,mixed-playground}.flows.txt` | `unfold-reader`'s drawing tests | Every definition drawn with no overlay, printed as agent text |
| `goldens/drawn/*.json` | `unfold-reader`'s drawing tests | Drawn flows as the IPC sends them. The UI's layout tests read them |
| `goldens/sample.jsonl` | `unfold-core`'s canonical tests | The sample `shop` graph |
| `workspace/commands/*.json`, `workspace/commands/{py-orders,mixed-playground}/*.json` | `unfold-workspace/tests/commands/goldens.rs` | Command results, normalized: every command on `ts-orders` (the UI tests' engine answers), and `project_summary`, `entrances`, `card` (with and without its agent text), and `code_view` on `py-orders` and the mixed playground |
| `boards/order-system.json` | `unfold-workspace/tests/commands/claims.rs` | The example board, saved to itself with real signatures |
| `fixtures/typed-lines/expected/*.jsonl`, `fixtures/typescript-typed/*.typed`, `fixtures/typescript-structure/*.tree`, `fixtures/typescript-evidence/*/expected.links` | The language crates | Typed records, node trees, links per case |

- **Output is deterministic.** The playground goldens are compiled on 1 and on 4 threads and must match. Shuffled
  file orders give identical exports too.
- **Bless on purpose.** `UNFOLD_BLESS=1` rewrites every golden a test run reaches. Read the diff and confirm each
  change is intended before keeping it. Never bless an unexplained failure.
- **Never compile in place.** Only blessing writes into `testdata/`.

## Precision and recall gates

Independent oracles check resolution: the TypeScript checker and Pyright, with their answers cached under
`testdata/reference/oracles/`, so the check needs no Node.

- **Precision is exact.** Every `calls` link Unfold resolves to a project definition, and every `passes` link, must
  name the oracle's target. A `library` or `builtin` call fails when the oracle names a project target for it, or
  finds it declared in another module than Unfold's label (`ecmascript` against `global`, `python:json` against
  `python:builtins`); where the oracle has no answer, the label isn't compared. A disagreement fails unless
  `justified.toml` lists it with a reason, and an entry that matches nothing is stale and fails.
- **Recall has floors that only rise.** When the check reports recall above a floor, raise the floor to the
  reported counts. Never lower one to accept a change.
- **A stale oracle fails.** Each oracle file records a hash of its corpus. Rerun `cargo xtask oracles refresh
  <corpus>` when the corpus changes.

```sh
cargo xtask oracles check ts-orders entrances py-orders mixed-playground python-ranges   # what CI runs
UNFOLD_CORPORA_DIR=<dir> cargo xtask oracles check fastapi-template typescript-6.0.3-src  # large corpora, locally
```

CI checks the five in-repository corpora. The pinned large corpora (the FastAPI template, TypeScript's `src`, the
proof of concept's own source) run locally from `UNFOLD_CORPORA_DIR`; run them when a change can move resolution.
Corpora, oracle files, floors, and justified entries are described in
[testdata/reference/README.md](../testdata/reference/README.md); the TypeScript oracle's rules in
[tools/oracles/README.md](../tools/oracles/README.md).

## End-to-end journeys

Playwright drives the built, unpackaged app: `ui/dist`, `desktop/out`, and the release addon. Each test gets its own
temporary folder with a user-data folder, a home folder, and fresh copies of the playgrounds. One worker runs the
journeys in order, because the MCP server listens on a fixed port. A failing test attaches each window's screenshot
and the app's output, and keeps its folder.

| Journey (`e2e/journeys/`) | Playground | What it covers |
| --- | --- | --- |
| `home.spec.ts` | `ts-orders` | Adding a project compiles it; Home's state line, How it starts, Boards, and Health; Fix on the missing key; Open `unfold.toml`; Settings; the engine host restarting |
| `trace.spec.ts` | `py-orders` | Tracing an entrance onto a board; a call opened in place; Show callers; Show all calls; Add by name; a source edit on disk |
| `code-panel.spec.ts` | `ts-orders` | The code panel for a call, a part, a gap tick, and a combined step; the path; hover and click; a file from Add; an entrance's lines; a changed file; Open in editor |
| `explain.spec.ts` | `py-orders` | Name, Combine, Describe, a proposed step; a table, links, a relationship; Undo and Redo; review and Accept; a board file changed on disk |
| `agent.spec.ts` | `ts-orders` | An MCP client reads, traces, shapes a card in one call, frames, connects, checks, finishes, and declares an entrance; one Undo takes the shaping off |

`just e2e [spec]` builds all three outputs, then runs the journeys. While the outputs still match the sources,
repeat one test with `pnpm --filter @unfold/e2e test <spec> -g "<title>"`. After an edit, rebuild what it affects:
`just addon-release` for Rust, `pnpm --filter @unfold/ui build`, `pnpm --filter @unfold/desktop build`. An output
folder that exists isn't proof it is current.

## Broad gates

`.github/workflows/pr.yml` runs one macOS job on every pull request and push to `main`, plus `cargo deny` on Linux.

| Gate | Command in `pr.yml` |
| --- | --- |
| Rust format | `cargo fmt --all -- --check` |
| Rust lints | `cargo clippy --workspace --all-targets -- -D warnings` |
| Rust tests | `cargo test --workspace` |
| IPC types are current | `cargo xtask gen-ipc --check` |
| Precision and recall | `cargo xtask oracles check ts-orders entrances py-orders mixed-playground python-ranges` |
| TypeScript typecheck, lint | `pnpm -r typecheck`, `pnpm -r lint` |
| Unused files, exports, dependencies | `pnpm knip` |
| Unit tests (desktop, UI) | `pnpm -r --filter "!@unfold/node" --filter "!@unfold/e2e" test` |
| Addon | `pnpm --filter @unfold/node build` (release), then `pnpm --filter @unfold/node test` |
| Third-party notices | `cargo xtask notices --check` |
| Journeys | the UI and desktop builds, then `pnpm --filter ./e2e test` |
| Rust dependency licenses and advisories | `cargo deny check` (its own Linux job) |

Locally, `just check`, `just test`, `just js-check`, `just addon-test`, and `just e2e` run the same checks.

Not in CI: `cargo machete` (unused Rust dependencies), `node tools/oracles/check-board-schema.mjs` (board files
against the JSON Schema with an independent validator), the large corpora, and `just bundle-dir`. A workflow that
exists isn't evidence that it passed.
