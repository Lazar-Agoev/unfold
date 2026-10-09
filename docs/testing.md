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
| UI | `ui/src/**/*.test.ts(x)` (Vitest, Testing Library, jsdom) | Components, views, keyboard dispatch, stores, persistence |
| Desktop | `desktop/src/**/*.test.ts` (Vitest) | App paths, the `app://` URL, the engine-host router, window placement, recent folders, export file names, update policy |
| Addon | `crates/unfold-node/test/addon.test.mjs` (`node --test`) | The built addon through its JavaScript surface |
| Mermaid and schema checks | `tools/oracles/check-mermaid.mjs`, `check-board-schema.mjs` | Snapshots parse with the real Mermaid parser; boards validate against `docs/schemas/unfold-board.schema.json` |
| Packaged smoke | `desktop/scripts/smoke.mts` (`just smoke`) | Fuses, window hardening, a Compile with no Node or Python on `PATH`, and nothing written outside `.unfold/` ([desktop/README.md](../desktop/README.md#packaging)) |
| End-to-end journeys | `e2e/journeys/` (Playwright on the built app) | The first vertical slice, the shell, the reader, background compile, several windows, and a keyboard-only journey |
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
| End-to-end journeys | `just e2e` (builds the release addon, the UI, and the shell first; pass a spec to run one) |
| Budgets | `just e2e-budgets` |
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
| Building the 602-file corpus | 11.3 s serial, 3.7 GB peak | First Compile in the app 2.4–4.5 s (parallel), 1.13 GB peak across all processes ([Budgets](#budgets-measured)) |

## Budgets (measured)

[Architecture §17](architecture.md#17-performance-targets)'s budgets, measured on the built,
unpackaged app with the release addon by `just e2e-budgets` (`e2e/budgets/budgets.spec.ts`, which
writes `e2e/test-results/budgets.json`). Memory is the working set of every Unfold process (main,
GPU, network, renderers, and the engine host) from Electron's `app.getAppMetrics()`. Latencies are
taken in the window: engine commands are timed around `window.unfold.invoke`, and "updated card" is
the first animation frame whose card shows the saved text. The 602-file corpus is TypeScript 6.0.3's
`src` (`testdata/corpora.lock.toml`), copied into a temporary folder.

Measured on 2026-10-09 on an Apple M5 (10 cores, 32 GB, macOS 26.2), while other builds were running
on the same machine; numbers vary by about ±30% between runs.

| Interaction | Target | Measured | |
| --- | --- | --- | --- |
| Launch to the welcome window | < 1.5 s | 0.35–0.7 s (median of 3 launches 0.4 s) | ✓ |
| Open a compiled folder to a usable graph (602 files: workbench shown and a definition search answers) | < 1 s | 0.1–0.15 s. Before the fix, in two of three runs the first reopen after the Compile took 2.1–2.5 s. **After the fix:** the warm reopen check reads 0 files in about 180 ms (previously all 602 files in about 330 ms) | ✓ after the fix (see below) |
| First Compile, 602-file TypeScript source | < 5 s, < 1 GB peak | 2.4–4.5 s; 1.13 GB peak for all processes | time ✓, memory ✗ |
| First Compile, 2,000-file synthetic repository | < 5 s | 0.9–1.0 s | ✓ |
| Save to updated card, 2,000-file repository | < 1 s (p95) | p95 0.44–0.49 s (p50 0.43–0.45 s; `graph_updated` arrives at 0.43–0.45 s, of which 0.3 s is the debounce) | ✓ |
| Save to updated card, `ts-orders`, two windows (journey) | < 1 s | 0.39–0.43 s | ✓ |
| Search, 95th percentile (602 files, 40 queries) | < 100 ms | 13–20 ms (p50 2–4 ms) | ✓ |
| Open a cached card, 95th percentile (602 files, 50 definitions, `card` round trip) | < 150 ms | 75–81 ms warm (169 ms once under load); 76–98 ms cold | ✓ |
| Canvas with 50 cards (602-file corpus, wheel pan and ctrl-wheel zoom at Fit) | 60 fps | 120 fps (the display's rate), worst frame 9–16 ms | ✓ |
| Cancel acknowledged (Rebuild from scratch, 602 files, cancelled 0.2–1.4 s in) | < 250 ms | Before the fix: 0.05–0.14 s when cancelled early; 0.74–1.18 s when cancelled after about 0.8 s. **After the fix (release):** 18–133 ms wherever it is cancelled, the commit included | ✓ after the fix |
| Idle memory, one window, 2,000-file synthetic repository open | < 600 MB | 675 MB: main 178, renderer 191, GPU 96, network 47, engine host 162 | ✗ |
| Idle memory, one window, 602-file corpus open (for comparison) | | 1.22–1.48 GB, of which the engine host 0.95 GB | |
| Each further window on the same folder | < 150 MB more | 86–153 MB | ✓ (one sample at 153 MB) |
| Engine-host restart until every window is usable (3 windows, 602 files) | < 2 s | median 0.63 s, worst 1.14 s; 0.37 s with two windows on `ts-orders` | ✓ |

What the misses point at:

- **Cancel (fixed).** Cancelling late in a compile used to wait up to about a second, because the
  store commit couldn't be cancelled. An explicit Compile or Rebuild is now cancellable until its
  commit finishes (the commit rolls back; see [Cancellation and threads](compiling.md#cancellation-and-threads)),
  and the engine agent's measurement after the fix is 18–133 ms on the 602-file corpus (release). The
  full `just e2e-budgets` run has not been repeated since.
- **Reopen (fixed).** The slow first reopen after a Compile was the workspace recompiling the folder
  in the background instead of reusing its graph (the reopen facts-cache miss). After the fix, the warm
  reopen check on the 602-file corpus reads 0 files and takes about 180 ms (previously 602 files,
  about 330 ms), measured by the engine agent; the full `just e2e-budgets` run has not been repeated
  since.
- **Memory.** On the 2,000-file repository the engine host holds 162 MB; Electron's own processes
  (main, one renderer, GPU, network) take about 510 MB, so the 600 MB budget is missed by the shell
  rather than the graph. On the 602-file corpus (262,508 blocks) the engine host holds about 0.95 GB
  idle, and the first Compile peaks at 1.13 GB for all processes.
