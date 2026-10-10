# Testing

How Unfold's correctness and performance gates are run. The layers and gates themselves are in
[architecture §16](architecture.md#16-testing); this file records how each gate is checked and what
it measured. Sections are added by the step that introduces each gate.

## Test kinds

Test inputs and expected outputs live under `testdata/` ([its README](../testdata/README.md)), never in a crate.
Tests compile copies of `testdata/` folders in temporary directories and never write into `testdata/` except when
blessing.

| Kind | Where | What it checks |
| --- | --- | --- |
| Unit | `#[cfg(test)]` modules in every crate; `*.test.ts(x)` beside UI and desktop modules | One module's rules |
| Integration | `crates/<crate>/tests/` | A crate through its public API: the engine's backend with an "alien" test language (`unfold-engine/tests/backend/`), the store's commits, reads, and search (`unfold-store/tests/store/`), the workspace's lifecycle, settings, updates, and command table (`unfold-workspace/tests/{workspace,commands}/`) |
| Structure and evidence | `unfold-lang-typescript/tests/structure/`, `unfold-lang-python/tests/{structure,binder,failures}.rs` | Syntax mapping, configuration, emit map, exports, binding, and the edges of every evidence fixture (`testdata/fixtures/typescript-evidence/`, `testdata/fixtures/python/`) |
| Parity | `tests/structure/parity.rs` (TypeScript), `tests/parity.rs` (Python), `cargo xtask parity` | The canonical export equals the proof of concept's converted reference, apart from ledger entries ([below](#the-divergence-ledger)) |
| Reader reference | `unfold-reader/tests/{reference,flow_reference,mermaid_reference,sample_board}.rs` | Owner, catalog, closure, applications, relations, Flow, and Mermaid equal what the proof of concept's reader derives; the sample board round-trips byte for byte |
| Oracles | `cargo xtask oracles check` | Precision and recall against the TypeScript checker and Pyright ([below](#precision-and-recall-gates)) |
| Detector cases | `testdata/detectors/<detector>/<case>/`, run by each language's `tests/entrances.rs` | Every detector's rules, one folder per case with its `expected-entrances.txt` |
| Property: incremental equals full | `unfold-engine/tests/backend/update/incremental.rs`, `unfold-workspace/tests/incremental.rs` | Random edit sequences through background updates end with the same graph as a fresh full compile, record for record. `PROPTEST_CASES` sets the number of sequences (default 6; nightly runs more in release) |
| Property: robustness and determinism | `unfold-lang-typescript/tests/structure/{robustness,determinism}.rs`, `unfold-lang-python/tests/robustness.rs`, `unfold-engine/tests/backend/determinism.rs` | No panic or stack overflow on arbitrary, truncated, or deeply nested input; the same export across thread counts and input orders |
| Goldens and snapshots | `unfold-workspace/tests/commands/goldens.rs` (`testdata/workspace/commands/`), Mermaid snapshots (`testdata/boards/*.mmd`, `testdata/mermaid/`), evidence and mapping cases | Exact output, reviewed when it changes ([below](#goldens-and-snapshots)) |
| Ignored (large corpora, benchmarks) | `#[ignore]` tests, such as the 602-file corpus parse and determinism and the store's first-commit benchmark | Run with `cargo test --release --workspace -- --ignored` (nightly) |
| Fuzz | `fuzz/` ([its README](../fuzz/README.md)) | No input makes the front ends or the backend panic, overflow, or hang |
| UI | `ui/src/**/*.test.ts(x)` (Vitest, Testing Library, jsdom) | The kit components, Home, the board and its canvas geometry, Add, the peek sheet, settings, keyboard dispatch, and the stores (boards, tabs, workspace, layout) |
| Desktop | `desktop/src/**/*.test.ts` (Vitest) | App paths, the `app://` URL, the IPC handlers, the application menu, the workspace file, window placement, export file names, update policy, the engine-host router |
| Addon | `crates/unfold-node/test/addon.test.mjs` (`node --test`) | The built addon through its JavaScript surface |
| Mermaid and schema checks | `tools/oracles/check-mermaid.mjs`, `check-board-schema.mjs` | Snapshots parse with the real Mermaid parser; boards validate against `docs/schemas/unfold-board.schema.json` |
| Packaged smoke | `desktop/scripts/smoke.mts` (`just smoke`) | Fuses, window hardening, a Compile with no Node or Python on `PATH`, and nothing written outside `.unfold/` ([desktop/README.md](../desktop/README.md#packaging)) |
| End-to-end journeys | `e2e/journeys/` (Playwright on the built app) | The studio, journey by journey, and a check that every row of the redesign's §6 and §8 tables has a home in them ([below](#end-to-end-journeys)) |
| Budgets | `e2e/budgets/` (`just e2e-budgets`) | The [architecture §17](architecture.md#17-performance-targets) performance targets on this machine |

## Running the gates locally

| Gate | Command |
| --- | --- |
| Every Rust test | `just test` (`cargo test --workspace`); add `-- --ignored` with `--release` for the large-corpus tests |
| Format and lints | `just check` (rustfmt, then clippy with warnings as errors) |
| Parity | `cargo xtask parity <corpus>` for `mixed-playground`, `playground-ts-native`, `python-playground`, and `entrances` (it compiles a temporary copy of the corpus folder; `--actual <export.jsonl>` checks an export made elsewhere) |
| Oracles | `cargo xtask oracles check` (the in-repository corpora, plus any pinned corpus found locally; `--strict` fails when one is missing) |
| Generated IPC types | `cargo xtask gen-ipc --check` |
| Third-party notices | `cargo xtask notices --check` |
| Rust dependencies | `cargo deny check` |
| TypeScript typecheck, lint, Vitest | `just js-check` |
| Addon | `just addon-test` (builds the debug addon first) |
| Packaged smoke | `just bundle-dir`, then `just smoke` |
| End-to-end journeys | `just e2e` (builds the release addon, the UI, and the shell first); `just e2e journeys/publish.spec.ts` runs one spec |
| Budgets | `just e2e-budgets` (writes `e2e/test-results/budgets.json`) |
| Fuzz | `cargo +nightly fuzz run <target>` from `fuzz/` |

CI runs the same commands: `.github/workflows/pr.yml` on pull requests (on macOS, Windows, and Linux) and
`nightly.yml` for the long property tests, ignored tests, every available oracle corpus, the packaged smoke test,
the full journeys, and two minutes of fuzzing per target. The workflows are written but have not run yet.

## Goldens and snapshots

Golden files are compared byte for byte through `unfold_core::testkit::assert_golden` (and each test's own
rendering for detector cases and evidence fixtures). When output changes on purpose, rerun the failing tests with
`UNFOLD_BLESS=1` to rewrite the files, then review the diff as carefully as code: a blessed golden is a claim that
the new output is right. Never bless to make a test pass without reading the change, and never edit a golden by
hand. Detector-case blessing keeps the case's comment lines. References under `testdata/reference/` are not
goldens: `poc/` is never edited, and `canonical/` is written only by `cargo xtask reference import`.

## The divergence ledger

`testdata/reference/divergences.toml` lists every allowed difference between the proof of concept's reference and
Unfold's export, one record (optionally one field) per entry, each with its reason
([the format](../testdata/reference/README.md#the-divergence-ledger)). `cargo xtask parity` fails on a difference
no entry covers (**unlisted**) and on an entry that covers nothing (**stale**), so the ledger always says exactly
how Unfold differs. Add an entry only with an explanation that points to a decision or a fixed defect; remove one
as soon as it goes stale. Today it holds 55 entries: the C-2 object-literal member calls, the C-8 dynamic-import
call, and the `tsconfig.json` inputs Unfold fingerprints, in `playground-ts-native`, `mixed-playground`, and
`entrances`; `python-playground` has none. Oracle disagreements have their own ledger with the same rules,
`testdata/reference/oracles/justified.toml`.

## Precision and recall gates

The V1 gates ([migration plan §7](migration-plan.md)): **precision**, every resolved call and named
reference agrees with an independent oracle on every pinned corpus, in both languages; **recall**,
at or above every recorded floor, and floors only rise.

- **Oracles.** TypeScript: the published TypeScript 6.0.3 checker, through the developer-only Node
  script `tools/oracles/typescript-oracle.mjs` ([its README](../tools/oracles/README.md#typescript-checker-oracle)).
  Python: the proof of concept's saved Pyright 1.1.407 evaluator rows, converted onto Unfold's
  blocks. Neither shares code with Unfold's engine, and neither reads Unfold's edges.
- **Cached answers.** `testdata/reference/oracles/<corpus>.<language>.jsonl`, keyed by a hash of the
  corpus bytes, so `cargo xtask oracles check` runs without Node. A changed corpus fails as a stale
  oracle until `cargo xtask oracles refresh <corpus>` regenerates it.
- **Precision.** Every resolved `calls`, `constructs`, and `references` edge must name the oracle's
  target. A disagreement fails unless `testdata/reference/oracles/justified.toml` lists it with a
  reason (today only decision C-8's `printReport()`, in `ts-orders` and the mixed playground); an
  entry that no longer matches fails as stale.
- **Recall.** The proof of concept's definition: call sites the oracle resolves that Unfold resolves
  to the same target, over the call sites the oracle resolves (policy refusals such as an overridden
  method are left out of both). `testdata/reference/oracles/floors.toml` holds one floor per corpus
  and language, set to what was achieved and never below the proof of concept's.
- **Where it runs.** Pull requests check the in-repository corpora (`ts-orders`, `entrances`,
  `py-orders`, `mixed-playground`, `python-ranges`). Nightly CI downloads the pinned FastAPI template
  and the TypeScript 6.0.3 source by their locked hashes, regenerates the checker's answers for the
  latter, and checks both. The PoC's exact 602-file corpus and its self corpus run wherever the
  proof-of-concept checkout is (`cargo xtask oracles check` picks them up from `corpora.lock.toml`).

Measured on 2026-10-09 (all at 100% precision):

| Corpus | Language | Recall | Proof of concept |
| --- | --- | --- | --- |
| `ts-orders` | TypeScript | 71 of 71 | 46 of 71 |
| `entrances` | TypeScript | 3 of 3 | |
| `mixed-playground` | TypeScript + Python | 71 of 71; 91 of 96 | 46 of 71; 91 of 96 |
| `py-orders` | Python | 91 of 96 | 91 of 96 |
| `python-ranges` | Python | 3 of 3 | 3 of 3 |
| `fastapi-template` | Python | 185 of 232 | 185 of 232 |
| `typescript-6.0.3-src` (602 files) | TypeScript | 70,720 of 73,649 (96.02%) | 70,717 of 73,649 (96.0%) |
| `typescript-6.0.3-src-upstream` (601 files) | TypeScript | 68,590 of 71,519 (95.90%) | |
| `codegraph-self` (186 files) | TypeScript | 6,805 of 6,807 (99.97%) | 4,440 of 4,440, measured in place |

Details, the corpus list, and the file formats: [testdata/reference/README.md](../testdata/reference/README.md#oracles).

## The proof of concept's baseline

The proof of concept's recorded results ([migration plan §1](migration-plan.md#1-where-we-start)) beside Unfold's,
as recorded in this file and in [testdata/reference/README.md](../testdata/reference/README.md#results-2026-10-09).

| Measure | Proof of concept | Unfold |
| --- | --- | --- |
| Tests | 854 compiler, 57 reader, and 3 browser tests passed | The suites in [Test kinds](#test-kinds); no pass count is recorded here |
| Precision | 100% against the oracles | 100% on every corpus, with one justified disagreement (C-8) |
| TypeScript recall, 602-file compiler source | 96.0% (native producer) | 96.02% (70,720 of 73,649) |
| TypeScript recall, codegraph source | 100% (4,440 of 4,440, in place) | 99.97% (6,805 of 6,807, from a copy) |
| TypeScript recall, playground | 46 of 71 | 71 of 71 |
| Python recall floors | 91 of 96 (playground), 185 of 232 (FastAPI template), 3 of 3 (ranges) | The same: 91 of 96, 185 of 232, 3 of 3 |
| Building the 602-file corpus | 11.3 s serial, 3.7 GB peak | First Compile in the app 2.3 s (parallel), 1,052 MB peak across all processes ([Budgets](#budgets-measured)) |

## End-to-end journeys

`e2e/` drives the built, unpackaged app through Playwright's Electron support: `ui/dist`, `desktop/out`, and the
release addon in `crates/unfold-node`, which `just e2e` builds first. Each test gets its own sandbox (a temporary
folder with a user-data folder, a home folder, and copies of the playgrounds `ts-orders` and `py-orders`, or a
synthetic repository), so tests never share state and nothing is written into `testdata/`. Tests run one app at a
time (one worker), because some of them time what they do. A failed test keeps its sandbox and prints its path.

| Spec (`e2e/journeys/`) | What it covers (redesign section) |
| --- | --- |
| `first-launch.spec.ts` | An empty workspace; Add project with the picker and ⌘O, a dropped folder, a dropped file, a folder from the command line and from a second launch; the not-compiled Home with what it reads and skips, its scope tree, and the first Compile (§4.1–§4.2) |
| `home.spec.ts` | A compiled project's Home: the header with the branch and worktree switcher, How it starts and Trace, the Boards section, the sidebar tree, Recents and Drafts; Health and activity for a file that doesn't parse (§4.3) |
| `board.spec.ts` | Reading a board: following calls and ⌥-click inline, On this page, Selected, the trail with Back and Forward, the lens and a card's own view; pages, descriptions, groups, Flow labels, undo and redo across pages; Show callers, a drawn link, Arrange, the zoom and Fit, the canvas and card menus (§4.4) |
| `boards.spec.ts` | A board's life outside publishing: rename in place, Duplicate, Share as Mermaid, Fold all, Delete; folders and Move to folder; a board's file edited or deleted outside the app; a failed save and Retry (§4.7) |
| `publish.spec.ts` | Drafts and publishing: a trace saved with its cards in place (left at once, published, and shown again with no unpublished changes); the first Publish naming the file; unpublished changes, Publish, Discard changes, Move back to drafts; a taken file name; a published file that changed underneath, resolved both ways (§4.7) |
| `add.spec.ts` | Add: opening it on a board (A, ⌘K, /), the map, Enter and ⌘Enter; search across two projects with kind tabs, counts, the preview, and Look in; text search options; commands after `>`, ⇧⌘P, ⌘P, ⌘T, and Copy Diagnostics (§4.5) |
| `peek.spec.ts` | The source peek sheet: a whole file with folds, call links, and ⊕; the pin, the menu, Escape; a text match at its line and Open source from a card (§4.6) |
| `cross-project.spec.ts` | A board spanning projects: a `py-orders` card on a `ts-orders` board, a drawn and labeled link, and a placeholder while the shared name doesn't match (§4.8) |
| `windows.spec.ts` | Several windows: New Window, two windows updating after a save, a board held by another window, Open in new window keeping undo, windows and tabs restored after quitting, and a killed engine host reconnecting every window without losing an edit (§4.9, §4.10) |
| `background.spec.ts` | Background updates: a saved file updates the graph and the open card; a described card asks for review; a broken file keeps its last version; a 500-file branch switch ends equal to a fresh compile; edits made while closed; a read-only folder; a deleted cache; a case-only rename (§4.10) |
| `project-menu.spec.ts` | The project menu: Compile again, Rebuild from scratch, Cancel, Pause and Resume, Settings…, Add entrance…, Health and activity, Reveal in Finder, Copy diagnostics, Remove from workspace… (§4.2, §4.10) |
| `settings.spec.ts` | Project settings (Scope, Detectors, the name, the guard on unsaved edits), the manual entrance editor from the project menu, Home, and a card, and Preferences (the theme, a key binding) (§4.11) |
| `keyboard.spec.ts` | Every shortcut of the §8 keyboard map once, with its effect; a plain key never fires while typing and no window shortcut fires under a dialog; the application menu's File, View, and Go items |
| `coverage.spec.ts` | Runs without the app: every row of the redesign's "Where everything goes" table (§6, 33 rows) and keyboard map (§8, 19 rows) has a home in the journeys |

The coverage journey keeps the journeys honest against the redesign. `e2e/src/coverage.ts` maps each row of
§6 and §8 (its first column) to the spec, test, and step that exercise its new home, and `coverage.spec.ts`
reads `docs/redesign/studio-redesign.md` and the spec sources: a row without a home, a mapped step that was
renamed or deleted, and a mapping for a row that no longer exists all fail. When a journey's step is renamed,
update its mapping; when the redesign gains a row, give it a step.

To run them: `just e2e` runs every journey; `just e2e journeys/board.spec.ts journeys/boards.spec.ts` runs
some, and further arguments go to Playwright (`-g "<test title>"`). `just e2e` rebuilds the release addon, the
UI, and the shell each time; `pnpm --filter @unfold/e2e test <spec>` reuses the last build.

## Budgets (measured)

[Architecture §17](architecture.md#17-performance-targets)'s budgets, measured on the built,
unpackaged app with the release addon by `just e2e-budgets` (`e2e/budgets/budgets.spec.ts`, which
writes `e2e/test-results/budgets.json`). Memory is the working set of every Unfold process (main,
GPU, network, renderers, and the engine host) from Electron's `app.getAppMetrics()`; where two
figures are given, the second counts only the main process, the engine host, and the renderers.
Latencies are taken in the window: engine commands are timed around `window.unfold.invoke`, and
"updated card" is the first animation frame whose card shows the saved text. The 602-file corpus is
TypeScript 6.0.3's `src` (`testdata/corpora.lock.toml`), copied into a temporary folder; the
2,000-file repository is `e2e/src/synthetic.ts`'s.

Measured on 2026-10-10 on an Apple M5 (10 cores, 32 GB, macOS 26.2) with the release build; numbers
vary by about ±30% between runs.

| Interaction | Target | Measured | |
| --- | --- | --- | --- |
| Launch to the first window (Home) | < 1.5 s | 327 ms (median of 3 launches) | ✓ |
| First Compile, 602-file TypeScript source | < 5 s, < 1 GB peak | 2,316 ms; 1,052 MB peak for all processes | time ✓, memory ✗ |
| Search, 95th percentile (602 files) | < 100 ms | 16 ms (p50 2.5 ms) | ✓ |
| Open a cached card, 95th percentile (602 files, 50 definitions, `card` round trip) | < 150 ms | 80 ms (cold p95 78 ms) | ✓ |
| Cancel acknowledged (Rebuild from scratch, 602 files) | < 250 ms | 108 ms median, 149 ms worst | ✓ |
| Open a compiled, resting project to a usable graph (602 files: its Home shows and a card answers) | < 1 s | 203 ms and 146 ms ("Graph current" after 1,995 ms and 1,446 ms; see below) | ✓ |
| Canvas with 50 cards (pan and zoom at Fit) | 60 fps | 120 fps (the display's rate), worst frame 10 ms | ✓ |
| First Compile, 2,000-file repository | < 5 s | 802 ms | ✓ |
| Save to updated card, 2,000-file repository (p95, a body-only edit) | < 1 s (V1); < 300 ms (V2) | 447 ms (p50 439 ms) | V1 ✓, V2 ✗ |
| Idle memory, one window, 2,000-file repository open | < 600 MB | 665 MB all processes; 517 MB main + engine host + renderer (engine host 163 MB) | ✗ |
| Idle memory, one window, 602-file corpus open (for comparison) | | 1,393 MB all processes, of which the engine host 934 MB | |
| Each further window on the workspace | < 150 MB more | 143 MB, then 77 MB | ✓ |
| Engine-host restart until every window is usable (3 windows) | < 2 s | 613 ms median, 1,120 ms worst | ✓ |
| Idle memory, three projects connected (2,000 files, `ts-orders`, `py-orders`), all live | < 600 MB (as one window) | 625 MB all processes; 483 MB main + engine host + renderer (engine host 156 MB) | ✗ by 25 MB |
| The same, resting applied (the 2,000 files shown and live, two resting) | < 600 MB (as one window) | 489 MB all processes; 375 MB main + engine host + renderer | ✓ |

What the misses point at:

- **Memory.** The graph is not what misses. With the 2,000-file repository open, the engine host holds
  163 MB; Electron's own processes (main, one renderer, GPU, network) take about 500 MB of the 665 MB, as they
  took 510 of V1's 675 MB, so the 600 MB budget is missed by the shell. Three connected projects cost little
  more than one: the engine host holds 156 MB for all three live, and resting the two that aren't shown
  brings the whole app to 489 MB. The 602-file corpus (262,508 blocks) is heavier: its engine host holds about
  0.93 GB idle, and its first Compile peaks at 1,052 MB for all processes.
- **Save to updated card (V2).** 447 ms at p95, of which the background update's debounce is 0.3 s; the
  V2 target needs incremental relinking.
- **Graph current after reopening.** The project is usable within about 0.2 s, but its Home says "Graph
  current" only after the reopen check, which still links, resolves, validates, and commits an empty change
  when no file changed (about 0.4 s of it on 602 files). The redesign build's follow-up F-16 skips that work
  when nothing changed; these two figures change with it.
