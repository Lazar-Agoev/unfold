# Test data

Inputs and expected outputs for Unfold's tests. Crates never hold test data of their own
([architecture §15](../docs/architecture.md#15-repository-layout)). Everything here was carried over from the
proof of concept (PoC) at `/Users/lazaragoev/Desktop/repos/graph-compiler`, commit `5c98a6e` (2026-10-08),
unless a section says otherwise.

```text
testdata/
├── playgrounds/
│   ├── .unfold/config.toml       # settings for the mixed folder (both playgrounds together)
│   ├── expected-entrances.txt    # mixed folder: 50 entrances, 56 targets
│   ├── ts-orders/                # ← PoC ts-playground/ (Express, BullMQ, node-cron)
│   │   ├── .unfold/config.toml
│   │   └── expected-entrances.txt   # 25 entrances, 28 targets
│   └── py-orders/                # ← PoC py-playground/ (FastAPI, Celery, APScheduler)
│       ├── .unfold/config.toml
│       └── expected-entrances.txt   # 25 entrances, 28 targets
├── fixtures/                     # small exact cases
│   ├── worked/ classes/ entrances/ playground-v1/   # ← PoC compilers/codegraph/test/fixtures/…
│   ├── python/policy/            # ← PoC fixtures/python (binder positives and refusals)
│   ├── python-ranges/            # Unicode/CRLF range corpus (see corpora.lock.toml)
│   ├── typed-lines/              # typed records: a TypeScript and a Python orders flow (written for Unfold)
│   ├── typescript-evidence/      # TypeScript binding and evidence cases with expected edges (its README.md)
│   └── typescript-typed/         # TypeScript typed-record cases with expected `.typed` records (written for Unfold)
├── detectors/<detector>/<case>/   # detector cases: sources, .unfold/config.toml, expected-entrances.txt
├── boards/
│   ├── order-placement.json      # sample board, unfold-board v1
│   └── order-placement*.mmd      # its Mermaid export (snapshots: as saved, and with every card in Flow)
├── mermaid/<playground>.json     # snapshots: Unfold's Mermaid for every entrance and every callable's Flow
├── workspace/commands/           # golden JSON of representative commands on ts-orders (unfold-workspace tests)
├── reference/                    # frozen PoC generations; see reference/README.md
│   └── poc/<corpus>/
└── corpora.lock.toml             # larger corpora pinned by URL and hash, not copied
```

## Conventions

- **Never compile in place.** Tests copy a playground or fixture folder to a temporary directory (`tempfile`)
  before compiling, so nothing writes `.unfold/local/` or anything else into `testdata/`. Only blessing
  (`UNFOLD_BLESS=1`) writes here.
- **`.unfold/local/` is never committed.** Only `.unfold/config.toml` (and published `.unfold/boards/`) belong in
  test folders.
- **Bytes are exact.** Source hashes, board signatures, and offsets depend on exact bytes, and some fixtures are
  CRLF or Latin-1 on purpose. `testdata/.gitattributes` turns off line-ending conversion. Don't reformat anything.
- **Nothing is installed or run.** The playgrounds keep `package.json`, `pyproject.toml`, and scripts as evidence
  inputs only. There is no `node_modules`, `dist`, `.venv`, cache, or lockfile.
- Paths in expectations are relative to the folder being opened, separated by `/`.

## Playgrounds

Copied byte for byte. Every path in the PoC's frozen manifests exists here with the same SHA-256.

- **`ts-orders`** ← `ts-playground/`: `src/**` (34 TypeScript sources), `package.json`, `tsconfig.json`, `README.md`,
  plus `scripts/export.js` and `report-processor.js`. JavaScript is not indexed in V1; the two files exist so launch
  resolution can see them (`legacy:export` → `not_indexed`; the report worker's `new URL(...)` stays `unsupported`).
  The PoC README still describes `npm` commands and the root `codegraph.json`; read it as history.
- **`py-orders`** ← `py-playground/`: `src/**` (50 Python sources), `legacy/**`, `tests/**`, `scripts/**`,
  `examples/**`, `pyproject.toml`, `README.md`.
- **Mixed:** open `testdata/playgrounds/` itself to get the PoC's 84-source mixed build. The nested
  `ts-orders/.unfold/` and `py-orders/.unfold/` belong to those subfolders and are not read then; the mixed settings
  repeat their content with `ts-orders/` and `py-orders/` prefixes.

### Settings

The PoC's `codegraph.json` files became `.unfold/config.toml` ([architecture §9.2](../docs/architecture.md#92-project-settings)):
each `entrypoints[]` item is an `[[entrance]]` table with the same `kind`, `trigger`, and `handler`, and locators keep
the PoC forms (`path#name`, `path:line`). `python.sourceRoots` became `[[python.project]] source_roots`.

- **The indexed set.** The PoC built its playground generations from an explicit file list: only `src/` in each
  playground. Unfold discovers files instead, so the Python settings exclude `legacy/**`, `scripts/**`, and
  `tests/**` from scope, which reproduces the PoC's 50 Python sources. `legacy` stays a source root, as in the PoC,
  so the `orders-legacy-export` console script launches into an existing but unindexed module (`not_indexed`).
  TypeScript needs no exclusion: only `src/**/*.ts` is TypeScript.
- **The PoC root `"exclude": ["package.json"]`** named the graph-compiler repository's own tooling `package.json`,
  which kept that package out of the PoC's package detectors. No such file exists here (the mixed folder has no root
  `package.json`, and in `ts-orders` the root `package.json` *is* the playground's evidence input), so the
  exclusion was dropped rather than translated.
- No Python grammar `version` is set, matching the PoC (whose default and Unfold's are both 3.14).

### Expected entrances (`expected-entrances.txt`)

Derived mechanically from the frozen generations (`entrypoints.csv`, `entrypoint_targets.csv`, and `blocks.csv` for
target keys) and checked against the PoC's trees in `docs/testing.md` ("Expected Entrances view") and
`py-playground/expected-outcomes.md`. The TypeScript file is identical whether derived from the native, the
checker-backed, or the mixed generation; the Python file is identical from the Python or the mixed generation.

| File | Entrances | Targets | Outcomes |
| --- | ---: | ---: | --- |
| `ts-orders/expected-entrances.txt` | 25 | 28 | 25 resolved, 1 `wrapper`, 1 `unsupported`, 1 `not_indexed` |
| `py-orders/expected-entrances.txt` | 25 | 28 | 26 resolved, 1 `wrapper`, 1 `not_indexed` |
| `expected-entrances.txt` (mixed) | 50 | 56 | the union, with `ts-orders/` and `py-orders/` prefixes |

Format: UTF-8, LF, one record per line, fields separated by a single tab. Lines starting with `#` and empty lines are
comments. An empty field is written as `-`; no field contains a tab.

```text
entrance<TAB>detector<TAB>kind<TAB>method<TAB>trigger<TAB>trigger_status<TAB>evidence_path:start_line
<TAB>target<TAB>role<TAB>resolution<TAB>what<TAB>target_text
```

- An `entrance` line is followed by its `target` lines in position order (position 0 first).
- `trigger_status` is `exact` or `unknown` (an unknown trigger is written `-`). `method` is set for `http` only.
- `evidence_path:start_line` is the evidence file and the one-based line where the evidence range starts. For
  manual entrances it is `.unfold/config.toml` and the line of that entrance's `[[entrance]]` header.
- `what` depends on `resolution`: the target block's **structural key** for `resolved`, the module label for
  `external`, and the reason (`wrapper`, `unsupported`, `not_indexed`, …) for `unresolved`.
- `target_text` is the target's written text (part of the entrance key). For manual entrances it is the locator as
  written in the settings file.
- Lines are in the [architecture §6.8](../docs/architecture.md#68-determinism) entrance order: evidence path (byte
  order), start, end, detector, kind, method, trigger. Manual entrances sort in table order.

Structural keys follow the PoC derivation (`path#`, `kind:name` with `%`, `/`, `#`, `[`, `]` percent-escaped and
`[n]` for a repeated name, `kind[n]` for unnamed steps), rewritten to the opened folder's paths. A checker compares
the set of entrance lines and, per entrance, the ordered target lines; the line order itself is also expected.

## Fixtures

Copied from `compilers/codegraph/test/fixtures/{worked,classes,entrances,playground-v1,python}`. Their expected rows
are the reference generations in `reference/` (the `*-golden` folders went there, not here).

- `entrances/codegraph.json` → `entrances/.unfold/config.toml` (one manual worker entrance).
- `python/policy/{single-root,duplicate-roots}/codegraph.json` → `.unfold/config.toml` with
  `[[python.project]] root = "."`, the same `source_roots`, and `version = "3.14"`. Open each of those subfolders
  on its own.
- `playground-v1/` is the PoC's frozen v1 release target; its graph root is `playground-v1/`, not
  `playground-v1/ts-playground/`.
- `python-ranges/fixtures/unicode.py` is the PoC's range corpus (astral and Greek identifiers, CRLF), written out of
  `spikes/python/prepare.mjs` byte for byte; its hash matches the PoC's recall-floor lock. Its source root is
  `fixtures`.
- `python/policy/encoding/latin1_cookie.py` is Latin-1 on purpose: it must fail as a file.

### Typed lines (`fixtures/typed-lines/`)

Written for Unfold (not carried over): one project with a TypeScript and a Python orders flow whose calls carry
written types, for the typed records ([graph model](../docs/graph-model.md#typed-records)) and the app's typed
lines. Open the folder itself; `.unfold/config.toml` makes `py/` a Python project with source root `src`.

- `ts/` (`tsconfig.json` with NodeNext, `package.json` naming `stripe` as a dependency, nothing installed):
  `orders/create.ts` (`createOrder` calls `reserveInventory(cart.items)` and `chargePayment(cart, req.user)` inside
  a `try`/`catch`, a three-argument `toOrder`, a passed callback `notify(order, summary)`), `inventory.ts` (a
  generic `first<T>`), `payments.ts` (`import Stripe from "stripe"`, `Stripe.PaymentIntent`, `throw new
  CardDeclinedError(…)`), `cart.ts` (interfaces, type aliases, merged `interface Address`, an object-literal alias, a
  utility alias, an enum), `user.ts` (a class with a field and parameter properties), `errors.ts`, `legacy.ts`
  (nothing annotated), and the heritage cases: `orders/model.ts` (interfaces extending with generics, a base reached
  twice, a merged base and a library's base, a class that extends and implements) and `orders/cycle.ts` (two
  interfaces that extend each other).
- `py/src/shop/`: `models.py` (a `@dataclass`, a `TypedDict`, a pydantic `BaseModel` with a string forward
  reference, a `NamedTuple`, `Money = Decimal`, `Literal` and `Annotated`), `payments.py` (positional-only,
  keyword-only, `*args`/`**kwargs`, a static and a class method, `raise CardDeclined(…)`, a method called through
  `self`), `orders.py` (keyword arguments, a method called through an instance and through its class, expansions, a
  `TypeVar`, a PEP 695 type parameter), `errors.py`, and `accounts.py` (a dataclass chain with `Generic[T]`, a pydantic
  chain with a base reached twice and a keyword argument, a `TypedDict` subclass, a base that is a call).
- `expected/typescript.jsonl`: the snapshot of the TypeScript project's type declaration blocks and typed records, as
  canonical export lines (`crates/unfold-lang-typescript/tests/structure/typed.rs`; `UNFOLD_BLESS=1` rewrites it).
- `expected/python.jsonl`: the same for the Python project's typed records (`crates/unfold-lang-python/tests/typed.rs`).

## Detector cases (`detectors/`)

One folder per case under `detectors/<detector id>/`: the sources, a `.unfold/config.toml` (usually one
`[[python.project]]` with `source_roots = ["src"]`), and an `expected-entrances.txt` in the format above whose comment
lines say what the case pins. The Python cases (`pyproject.script`, `fastapi.route`, `celery.task`, `apscheduler.job`)
were written for Unfold from the proof of concept's focused suites (`test/python-scripts.test.ts`,
`python-fastapi.test.ts`, `python-celery.test.ts`, `python-apscheduler.test.ts`): each variant of a PoC test became
a case (variants that would affect each other got folders of their own), and the expected outcomes were checked
against that test's assertions. `crates/unfold-lang-python/tests/entrances.rs` compiles every case through the engine
(validation included) and compares its entrances; `UNFOLD_BLESS=1` rewrites the records and keeps the comment lines.

The TypeScript cases (`package.script`, `package.bin`, `express.route`, `bullmq.worker`, `node-cron.schedule`) and the
`manual` cases were written the same way from `test/schema2-launch.test.ts`, `schema2-express.test.ts`,
`schema2-framework-jobs.test.ts`, and `schema2-manual.test.ts` (plus a few Unfold cases: script references, C-5
probing, evidence pointers, bin names, and the C-9 property locators); each usually has a `tsconfig.json` and no
`node_modules`. `crates/unfold-lang-typescript/tests/entrances.rs` compiles them; its rendering escapes a backslash,
tab, or line break inside a trigger or text (`\\`, `\t`, `\n`, `\r`).

## Sample board (`boards/order-placement.json`)

Converted once from the PoC's `codegraph/boards/order-placement.board.json` (`codegraph-board` v3) to `unfold-board`
v1 ([architecture §9.1](../docs/architecture.md#91-board-files), [U-22](../docs/decisions-and-tradeoffs.md#u-22-a-fresh-board-format)):
`format` and `version` replaced, each card's `state` renamed `view`, the board's `blocks` renamed `labels`, and
everything else kept (positions, groups, descriptions, guards, signatures, authors, texts). It targets
`testdata/playgrounds/ts-orders` opened alone, so every block key, guard owner, and group member lost its
`ts-playground/` prefix. The one entrance reference (`POST /orders`, first target `requireUser`) needed no change.
Signatures are hashes of source text, which is unchanged: all 69 (29 labels, 35 guards, 3 card descriptions,
2 group descriptions) were re-verified against the `ts-orders` bytes using the frozen native generation's ranges;
the files involved are ASCII, so UTF-16 and byte ranges coincide. Serialized with sorted keys, two-space
indentation, and a final newline.

`crates/unfold-reader/tests/sample_board.rs` loads it against the `ts-orders` reference graph with every description
and label attached and healthy, checks that it round-trips byte for byte, and snapshots its Mermaid export
(`order-placement.mmd`, identical to the PoC's apart from handler numbers, and `order-placement-flow.mmd`, every card
switched to Flow). `UNFOLD_BLESS=1` rewrites the snapshots; `tools/oracles/check-mermaid.mjs` parses them and
`testdata/mermaid/*.json` with the real Mermaid parser.

## Reference and corpora

See [`reference/README.md`](reference/README.md) for the frozen PoC generations and their path mapping, and
[`corpora.lock.toml`](corpora.lock.toml) for the larger corpora (TypeScript 6.0.3 `src`, the PoC's own source,
the FastAPI full-stack template 0.12.0, and the Python range and isolation corpora).
