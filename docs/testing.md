# Testing

Select checks for the changed behavior. This guide is a routing reference, not a checklist
for every task. CI defines broad platform gates; local work normally uses a bounded subset.

## Choose checks for a local change

Choose the smallest layer that can verify the changed contract. After it passes, stop;
broaden or repeat only for another edit, a failure, or a specific unresolved risk.

| Change | Start with | Add app-level checks when |
| --- | --- | --- |
| Docs/design | Links, paths, current semantics | No app build or Electron journey for prose alone |
| Isolated Rust logic/extraction/storage | Focused crate unit/integration tests and relevant validator/canonical/reference cases | A changed integrated workflow needs more than those tests |
| UI component/state/styling | Focused Vitest, UI typecheck; visual inspection for appearance | Rendered interactions/workflows cannot be adequately checked in components |
| IPC/events | `Host` command tests, generated-type check, router/UI tests | Real process transport, ordering, or reconnection is part of the change |
| Publish/restore/windows/background updates | Local logic tests plus the matching journey for changed integrated behavior | Run that spec/test first |
| Packaging/fuses/assets | The unpacked build (`just bundle-dir`) | App behavior is also affected |

E2E is required when verifying the changed workflow needs the running app, a regression
requires Electron/rendered interactions absent from focused tests, or the user asks for it.
Docs, proposals, isolated logic, and styling without interaction changes do not trigger it.
Reserve the full suite for an explicit request or broad app changes whose risk cannot be
bounded to specific journeys. Report the checks actually run and material coverage gaps.

## Focused commands

```sh
cargo test -p unfold-reader <test-filter>
pnpm --filter @unfold/ui test <test-file>
pnpm --filter @unfold/ui typecheck
pnpm --filter @unfold/desktop test <test-file>
pnpm --filter @unfold/desktop typecheck
cargo xtask gen-ipc --check
```

Use the owning crate/package, not necessarily those examples. Lint/format the affected code.
A changed graph contract may need adjacent extractor, store, reader, canonical, and workspace
checks; a local component does not automatically need every Rust suite.

## Test inputs and goldens

Fixtures live under `testdata/`; tests compile temporary copies. Do not modify shared fixture
folders during a test. Read [testdata conventions](../testdata/README.md) only for fixture work.

`UNFOLD_BLESS=1` updates supported goldens. Use it only for intentional output changes and
review the result; do not bless unexplained failures. Canonical exports, command JSON,
Mermaid, and detector expectations check different contracts.
Schema/Mermaid parser checks live in `tools/oracles/`.

## Precision and recall gates

Cached TypeScript-checker/Pyright answers are independent oracles. Every resolved call,
construction, or reference must agree with its target unless a specific justified entry
applies. Recall floors are per corpus/language and may not be lowered to accept a change.
Stale justifications fail. Written policy refusals stay distinct from genuine missed targets.

```sh
cargo xtask oracles check <corpus>
```

Corpus definitions, oracle hashes, floors, and justified disagreements live under
`testdata/reference/`; [its README](../testdata/reference/README.md#oracles) owns that workflow.
Refreshing an oracle requires the appropriate developer tooling, not a runtime dependency.
Large pinned corpora and ignored tests are separate checks for related changes.

## End-to-end journeys

Playwright drives the built unpackaged Electron app using `ui/dist`, `desktop/out`, and the
release addon. Each journey gets isolated app-data/home and temporary fixture copies. One
worker runs journeys serially; a failure retains its sandbox and reports the path.

`just e2e <spec>` invokes all three builds. When outputs still match the current sources,
repeat just the selected test with `pnpm --filter @unfold/e2e test <spec> -g "<title>"`.
After an edit, rebuild affected outputs: `just addon-release` for Rust/addon,
`pnpm --filter @unfold/ui build` for UI, `pnpm --filter @unfold/desktop build` for shell.
Existing output folders alone do not prove freshness; shared contracts may affect several outputs.

| Behavior | Spec under `e2e/journeys/` |
| --- | --- |
| First launch/project connection | `first-launch.spec.ts` |
| Home/health | `home.spec.ts`, `project-menu.spec.ts` |
| Cards, canvas, board operations | `board.spec.ts`, `boards.spec.ts` |
| Diagram expansion, View code, Combine, proposed steps, calls expanded in place | `diagrams.spec.ts` |
| Publish/drafts/conflicts | `publish.spec.ts` |
| Add/search/source peek | `add.spec.ts`, `peek.spec.ts` |
| Project-spanning boards | `cross-project.spec.ts` |
| Window restore/engine restart | `windows.spec.ts` |
| File watching/reopen/branch updates | `background.spec.ts` |
| Settings/keyboard | `settings.spec.ts`, `keyboard.spec.ts` |
| Typed lines and agents | `typed-lines.spec.ts`, `agents.spec.ts` |

## Broad gates

| Gate | Command / owner |
| --- | --- |
| All Rust tests / lints | `just test`, `just check` |
| JS typecheck/lint/tests | `just js-check` |
| IPC generation | `cargo xtask gen-ipc --check` |
| Addon integration | `just addon-test` |
| Schema/Mermaid | `tools/oracles/check-board-schema.mjs`, `check-mermaid.mjs` |
| Unpacked app | `just bundle-dir` |
| All journeys | `just e2e` |
| Dependency/notices | `cargo deny check`, `cargo xtask notices --check` |
| Ignored/property tests | Relevant test modules |

Exact CI commands live in `.github/workflows/pr.yml`.
A configured workflow is not evidence that it has passed.
