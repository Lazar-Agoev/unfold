# Reference outputs from the proof of concept

The folders under `poc/` hold raw generations from the proof of concept (product name "Codegraph"), copied
unchanged. They are the behavioral reference for parity checks ([docs/testing.md#choose-checks-for-a-local-change](../../docs/testing.md#choose-checks-for-a-local-change),
[testing policy](../../docs/testing.md#precision-and-recall-gates)).
Never edit them. `cargo xtask reference import` converts them into Unfold's canonical form in `canonical/`
([below](#canonical-references)), and `cargo xtask parity` compares Unfold's output with that form, allowing only the
differences listed in [`divergences.toml`](#the-divergence-ledger).

- **Source:** `/Users/lazaragoev/Desktop/repos/graph-compiler`, commit `5c98a6ef6e5d578b6b8668ee8cb4f30316573588`
  (`5c98a6e`, 2026-10-08). The PoC has no remote and no `poc-final` tag; this commit is the freeze point.
- **Contents of each folder:** `manifest.json` plus `blocks.csv`, `slots.csv`, `edges.csv`, `entrypoints.csv`, and
  `entrypoint_targets.csv` (PoC schema 3, described in the PoC's `docs/format.md`).
- **Reader reference:** `poc-reader/<playground>/`, what the PoC's `reader/core` derives (Step 0 in the migration
  plan; [below](#reader-reference)).
- **Oracles:** `oracles/`, the cached answers of the independent precision oracles (the TypeScript
  checker and Pyright) and the precision and recall gates over them ([below](#oracles)).

```text
reference/
├── poc/<corpus>/            # raw PoC generations (never edited)
├── corpora.toml             # corpus → raw folder, testdata folder, path mapping, languages, settings sources
├── canonical/<corpus>.jsonl # the converted references (written only by `cargo xtask reference import`)
├── poc-reader/<playground>/ # the PoC reader's derived views (written only by tools/oracles/export-reader.mjs)
├── divergences.toml         # the divergence ledger
└── oracles/                 # oracle answers per corpus, the corpora, recall floors, justified disagreements
```

## Corpora

| Folder | What it is | Producer (manifest) | Files | Blocks / slots / edges | Entrances / targets | Size |
| --- | --- | --- | ---: | --- | --- | ---: |
| `poc/mixed-playground` | Both playgrounds built together at the PoC repository root (the repository's own `.codecodegraph/`) | `typescript/native/6.0.3-codegraph.1` + `python/python/1.1.407-codegraph.1` | 34 TS + 50 Py | 1,564 / 2,766 / 696 | 50 / 56 | 256,661 B |
| `poc/entrances` | The exact entrance fixture golden (`fixtures/entrances-golden`) | `typescript/typescript/6.0.3` | 5 TS | 31 / 46 / 15 | 4 / 4 | 5,387 B |
| `poc/playground-ts-native` | The TypeScript playground, native producer (`fixtures/playground-schema2-golden/native`) | `typescript/native/6.0.3-codegraph.1` | 34 TS | 676 / 1,168 / 303 | 25 / 28 | 101,812 B |
| `poc/playground-ts-checker` | The TypeScript playground, checker-backed producer (`fixtures/playground-schema2-golden/typescript`) | `typescript/typescript/6.0.3` | 34 TS | 676 / 1,168 / 303 | 25 / 28 | 101,276 B |
| `poc/python-playground` | The Python playground (`fixtures/python-playground-golden`) | `python/python/1.1.407-codegraph.1` | 50 Py | 888 / 1,598 / 393 | 25 / 28 | 152,651 B |

Every source and input hash in these manifests matches the copies in `testdata/` (checked when the data was
imported), except the PoC's `codegraph.json` files, which Unfold replaces with `.unfold/config.toml`.

### How the PoC built each generation

Commands are from the PoC's `compilers/codegraph/README.md` and `docs/testing.md`.

- **mixed-playground.** `npm run codegraph:files -- --root . --playgrounds` lists exactly `ts-playground/src/**/*.ts`
  and `py-playground/src/**/*.py` (34 + 50), then `node compilers/codegraph/dist/main.js index . --language auto
  --files-from …` publishes at the repository root with the default native TypeScript producer. Settings: root
  `codegraph.json` (five TypeScript manual entrances, `exclude: ["package.json"]`) and `py-playground/codegraph.json`
  (five Python manual entrances, `sourceRoots: ["src", "legacy"]`, default grammar 3.14). Evidence inputs:
  `ts-playground/package.json`, `py-playground/pyproject.toml`. `legacy/` is resolution context, not indexed.
  Generated 2026-10-08T22:50:49Z. Its rows equal the union of the single-language generations below (the PoC's
  mixed verifier checks this).
- **entrances.** Fixture root `compilers/codegraph/test/fixtures/entrances` with its own `codegraph.json` (worker
  `orders` → `src/worker.ts#processOrder`) and `package.json`; no dependencies installed. Both TypeScript producers
  must produce this exact generation (comparison ignores time and producer identity); the saved copy names the
  checker-backed producer. Written by `npm run codegraph:goldens -- --target entrances`.
- **playground-ts-native / playground-ts-checker.** Root: the PoC repository; an explicit 34-file census of
  `ts-playground/src`; settings: root `codegraph.json`; evidence input `ts-playground/package.json`. The playground's
  locked npm dependencies were installed (the checker-backed producer needs their declarations). Written by
  `npm run codegraph:goldens -- --target playground` (later migrated to schema 3 by `--target typescript-schema3`,
  which allowed only manifest envelope changes). Blocks, slots, entrances, targets, and inputs are identical between
  the two; 51 of 303 edges differ (25 C-2 object-literal member calls, 26 library calls external vs
  `unknown_receiver`). Unfold has only one TypeScript producer: native semantics plus the C-2 fix, so the
  checker-backed folder is a comparison aid, not a parity target (it also carries defects C-1/C-3).
- **python-playground.** Root: the PoC repository; the 50 `py-playground/src` files; settings: root `codegraph.json`
  (its TypeScript entrances do not apply to a Python-only build) and `py-playground/codegraph.json`; evidence input
  `py-playground/pyproject.toml`. Built without installing application dependencies. Written by
  `compilers/codegraph/test/verify-python.ts --write-goldens` after its precision, determinism, and recall gates.
  Generated 2026-10-08T22:40:08Z.

## Paths: reference ↔ Unfold folders

Reference paths are relative to the PoC repository root (or the fixture root for `entrances`). Map them by prefix:

| Reference path | Unfold folder opened | Path in that folder |
| --- | --- | --- |
| `ts-playground/src/…`, `ts-playground/package.json` | `testdata/playgrounds/ts-orders` | `src/…`, `package.json` |
| `ts-playground/…` | `testdata/playgrounds` (mixed) | `ts-orders/…` |
| `py-playground/src/…`, `py-playground/pyproject.toml` | `testdata/playgrounds/py-orders` | `src/…`, `pyproject.toml` |
| `py-playground/…` | `testdata/playgrounds` (mixed) | `py-orders/…` |
| `codegraph.json` (root; TS manual entrances) | `ts-orders` / mixed | `.unfold/config.toml` |
| `py-playground/codegraph.json` | `py-orders` / mixed | `.unfold/config.toml` |
| `src/…`, `package.json`, `codegraph.json` (entrances corpus) | `testdata/fixtures/entrances` | same paths; `codegraph.json` → `.unfold/config.toml` |

Which folder and settings pair with which corpus:

| Corpus | Unfold folder | Settings |
| --- | --- | --- |
| `mixed-playground` | `testdata/playgrounds/` | `testdata/playgrounds/.unfold/config.toml` |
| `playground-ts-native`, `playground-ts-checker` | `testdata/playgrounds/ts-orders/` | `ts-orders/.unfold/config.toml` |
| `python-playground` | `testdata/playgrounds/py-orders/` | `py-orders/.unfold/config.toml` |
| `entrances` | `testdata/fixtures/entrances/` | `entrances/.unfold/config.toml` |

Other things that change with the folder:

- **Structural keys** start with the file path, so `ts-playground/src/routes/orders.ts#function:createOrder`
  becomes `src/routes/orders.ts#function:createOrder` in `ts-orders` and `ts-orders/src/routes/orders.ts#…` in the
  mixed folder.
- **Manual entrance target text** is the locator as written in the settings, so it follows the new paths
  (`ts-playground/src/cli.ts:12` → `src/cli.ts:12`). Python locators in the PoC were already relative to
  `py-playground/`; in the mixed folder they gain `py-orders/`. Entrance keys contain the first target text, so the
  five manual TypeScript entrance keys change accordingly. Launch target text (`dist/server.js`,
  `orders_playground.server:main`) and detector target text do not change.
- **Manual entrance evidence** is `codegraph.json` with JSON pointer `/entrypoints/N` in the PoC; in Unfold it is the
  matching `[[entrance]]` table of `.unfold/config.toml`, with its own range and pointer `/entrance/M`. The importer
  re-derives it ([conversion rules](#conversion-rules)).
- **Input fingerprints** for `codegraph.json` don't carry over: the importer replaces them with one
  `.unfold/config.toml` input. Every other source and input hash does carry over.

## Offsets

The PoC records `offset_unit: "utf16"`: offsets are zero-based, half-open UTF-16 code-unit positions, and lines are
one-based. Unfold stores UTF-8 byte offsets ([source coordinates](../../docs/graph-model.md#text-ranges-and-lines)),
so the importer converts every offset using the source file's text. In the playgrounds only
`py-orders/src/orders_playground/features/unicode.py` contains non-ASCII text; in every other file the two units are
equal.

## Canonical references

`canonical/<corpus>.jsonl` is each generation in Unfold's canonical export (`docs/graph-model.md`, "Order and canonical export"),
written by core's canonical writer, with the paths of the testdata folder Unfold opens for that corpus. Each file is
directly comparable with the canonical export of compiling that folder.

| Corpus | Folder | Files | Blocks | Slots | Edges | Inputs | Entrances | Targets |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `mixed-playground` | `playgrounds` | 84 | 1,564 | 2,766 | 696 | 3 | 50 | 56 |
| `playground-ts-native` | `playgrounds/ts-orders` | 34 | 676 | 1,168 | 303 | 2 | 25 | 28 |
| `playground-ts-checker` | `playgrounds/ts-orders` | 34 | 676 | 1,168 | 303 | 2 | 25 | 28 |
| `python-playground` | `playgrounds/py-orders` | 50 | 888 | 1,598 | 393 | 2 | 25 | 28 |
| `entrances` | `fixtures/entrances` | 5 | 31 | 46 | 15 | 2 | 4 | 4 |

Every count equals the CSV row count (the manifest's for files). Inputs are the manifest's minus its `codegraph.json`
files plus one `.unfold/config.toml`. Entrances in every reference match the folder's `expected-entrances.txt`.

### Commands

```sh
cargo xtask reference import [CORPUS...] [--poc <graph-compiler>] [--check]
cargo xtask canonical-check testdata/reference/canonical/<corpus>.jsonl
cargo xtask canonical-diff <expected.jsonl> <actual.jsonl> [--scope files,inputs,blocks,slots,edges,entrances] [--limit N] [--json]
cargo xtask parity <corpus> [--actual <file.jsonl>] [--scope ...] [--ledger <file>] [--limit N] [--json]
```

- **`reference import`** converts every corpus in `corpora.toml` (or the named ones) and prints the record counts per
  type. Importing again gives identical bytes; `--check` fails instead of writing when a file is out of date (a test
  does the same). Sources and inputs are read from the testdata folder; `--poc` is only a fallback for files the
  folder lacks (read only, never written).
- **`canonical-diff`** matches records by identity, never by position: files and inputs by path, blocks by key, slots
  by block key, slot name, and index, edges by source key and `seq`, entrances by key, targets by entrance key and
  position. Each differing record is `missing` (only expected), `extra` (only actual), or `changed` (with each field's
  expected and actual value; `(absent)` for no value). It lists at most `--limit` records per record type and category
  and ends with the counts; `--json` prints only the counts per record type and category. Each side is indexed once in
  a hash map, so large corpora compare in linear time. It exits non-zero when the exports differ.
- **`parity`** compares an actual export with `canonical/<corpus>.jsonl`, then checks the differences against the
  ledger. It fails on an unlisted difference or a stale ledger entry. `playground-ts-checker` has `parity = false` and is
  refused: compare with it through `canonical-diff`. Without `--actual`, `parity` compiles a temporary copy of the
  corpus folder with the engine and compares that.

### `corpora.toml`

One `[[corpus]]` per reference: `name`, `raw` (the generation, relative to this folder), `folder` (the testdata folder
Unfold opens, relative to `testdata/`), `poc_root` (where the PoC built it, for `--poc`), `languages`, `parity`
(default true), `paths` (prefix mappings; the longest matching `poc` prefix wins and every path must match one), and
`settings` (the PoC `codegraph.json` files that became the folder's `.unfold/config.toml`, each with the index of the
`[[entrance]]` table holding its first manual entrance). The header comment in the file repeats this.

### Conversion rules

1. **Paths** go through the corpus mapping; structural keys are derived from the mapped paths.
2. **Bytes are verified.** Every source and input is read from the folder and must have the manifest's SHA-256.
3. **Offsets.** Every UTF-16 range becomes a byte range through the file's `Utf16Index` (core); a range that splits a
   surrogate pair or falls outside the text fails the import. Lines are unit-independent and kept.
4. **Blocks and keys.** Each file's tree is rebuilt from `parent_id` in CSV preorder, and keys come from core's
   `KeyBuilder`, so duplicate siblings get the same `[n]` ordinals as in the PoC. Edge `seq` is the edge's position
   among its source's rows. Block, edge, and target IDs become keys; entrance keys are re-derived by
   `Graph::normalize` after the canonical sort (`#n` numbering follows the new order).
5. **Settings.** The `codegraph.json` inputs collapse into one `.unfold/config.toml` input (role `unfold-config`, hash
   of the folder's file). A manual entrance with evidence `/entrypoints/N` in a settings source becomes table
   `first_entrance + N`: evidence path `.unfold/config.toml`, pointer `/entrance/<index>`, and a range from the table
   header's `[[` through the end of its last value (token-exact: no trailing comment or whitespace). Its locator target
   text, which the PoC wrote relative to the `codegraph.json`, becomes relative to the folder. The importer fails unless
   the table declares the same kind, trigger, method, label, and locators.
6. **Module labels** that are repository paths (under a nonempty mapped prefix) are mapped; package, built-in, and
   Python labels are kept. None occur in the current corpora.
7. **Validation.** The converted graph must pass core's validator against every source and input snapshot, and its
   record counts must equal the CSV rows.

**Why rule 5 is a conversion rule and not ledger entries.** Moving manual entrances from `codegraph.json` to
`.unfold/config.toml` is a format change the architecture prescribes, applied to every manual entrance of every
corpus; it says nothing about whether the engine behaves like the PoC. Listing it in the ledger would add a dozen
entries per corpus that never go away and would hide real regressions in manual evidence behind them. Instead the
importer derives the evidence Unfold must produce from the folder's own settings file and checks that the table
declares the same entrance, so a mismatch is caught at import time. The range definition above is the reference's
assumption about the manual detector: if the manual-evidence range contract in `docs/engine.md` changes, update
`tools/xtask/src/reference/settings.rs` to match and re-import, rather than adding divergences.

**Left to the ledger.** Differences that come from Unfold's own behavior, not from the format: the C-2 fix
(object-literal member calls such as `logger.info` resolve), other fixed defects, and inputs the PoC never
fingerprinted, such as `tsconfig.json` (role `tsconfig`) in `ts-orders` and `fixtures/entrances`.

## The divergence ledger

`divergences.toml` lists every allowed difference between a reference and Unfold's output, with its reason
([testing policy](../../docs/testing.md#precision-and-recall-gates)).
It holds 55 entries: the C-2 object-literal member calls and the C-8 dynamic-import call in `playground-ts-native` and
`mixed-playground`, and the `tsconfig.json` inputs Unfold fingerprints there and in `entrances`; `python-playground`
has none. The policy is in [docs/testing.md](../../docs/testing.md#the-divergence-ledger).

```toml
[[divergence]]
corpus = "playground-ts-native"
record = "edge"            # file | input | block | slot | edge | entrance | target
key = "src/jobs/cleanup.ts#function:cleanupExpiredOrders/expression_statement[0]/call[0]"
seq = 0                    # edges; slots use `slot` (+ `index`), targets use `position`
change = "changed"         # missing | extra | changed
# field = "resolution"     # optional, `changed` only: cover one field instead of the whole record
# expected = "unresolved"  # optional, with `field`: match only this exact change; "(absent)" for no value
# actual = "resolved"
reason = "C-2 fixed: logger.info now resolves"
```

- `key` is the record's identity: path (file, input), structural key (block, entrance), owning block key (slot), source
  key (edge), or entrance key (target).
- A difference is split into units: one per missing or extra record, one per changed field. Every unit must be covered
  by an entry (else it is **unlisted**), and every entry must cover at least one unit (else it is **stale**: the
  divergence no longer occurs, so remove the entry). Both fail `parity`.
- Selectors are explicit: one record, optionally one field. There are no pattern selectors; if a fix ever produces
  hundreds of identical entries, add a group selector then, with its own stale check.
- Unknown fields, unknown corpora, fields that don't belong to the record type, and repeated selectors are errors.
- With `--scope`, entries for record types outside the scope are ignored.

## Reader reference

`poc-reader/ts-orders/` and `poc-reader/py-orders/` hold what the PoC's own reader code (`reader/core`) derives from
the frozen mixed playground generation (the PoC's root `.codecodegraph/`, identical to `poc/mixed-playground`): the
definition catalog (`catalog.json`), applications with their launches and member entrances, other entrances, and
launches into unindexed code (`applications.json`), callers, owned outgoing edges, and reached-from per definition
(`relations.json`), the activation closure per entrance (`closures.json`), the Mermaid of every entrance
(`mermaid.json`), the Flow model of every definition (`flows.json`, without exact source), and, for `ts-orders`, the
Mermaid of the sample board (`board-order-placement.mmd`, from the PoC's `codegraph/boards/order-placement.board.json`).

- **Written by** `node tools/oracles/export-reader.mjs --poc <graph-compiler>` ([its README](../../tools/oracles/README.md)),
  which runs the PoC's TypeScript in memory and never writes inside the PoC. Running it again gives identical bytes.
- **Paths and keys** follow the per-playground corpora: `ts-orders` maps like `playground-ts-native` and `py-orders`
  like `python-playground` (the `paths` in `corpora.toml`). Entrance keys are the keys of the matching
  `canonical/<corpus>.jsonl` entrances, so the files compare directly with Unfold's views of those canonical graphs.
  The mixed generation's rows are the union of the two single-language generations and no edge crosses between the
  playgrounds, so per-playground views of the mixed generation equal the single-language ones.
- **Checked by** `crates/unfold-reader/tests/reference.rs`, which commits `canonical/playground-ts-native.jsonl` and
  `canonical/python-playground.jsonl` to a temporary store with Unfold's projections and compares every view. They are
  identical; the rules are in [`docs/reader.md`](../../docs/reader.md). `flow_reference.rs`, `mermaid_reference.rs`,
  and `sample_board.rs` compare Flows and Mermaid, allowing only the documented reference differences (labels instead
  of exact source in Mermaid, reader semantics; no handler numbers).

## Oracles

The precision and recall gates ([docs/testing.md#choose-checks-for-a-local-change](../../docs/testing.md#choose-checks-for-a-local-change),
[testing policy](../../docs/testing.md#precision-and-recall-gates)):
every call, construction, and named reference Unfold resolves must name the target an independent
oracle names, and recall stays at or above a floor that only rises. The oracles' answers are cached
here, keyed by a hash of each corpus, so `cargo xtask oracles check` needs no Node.

```text
oracles/
├── corpora.toml                 # the corpora under the gates: folder or lock id, languages, Pyright rows
├── <corpus>.typescript.jsonl    # the TypeScript checker's answers (tools/oracles/typescript-oracle.mjs)
├── <corpus>.python.jsonl        # Pyright's answers, converted from the PoC's saved rows
├── floors.toml                  # recall floors
└── justified.toml               # resolved edges the oracle doesn't confirm, each with its reason
```

```sh
cargo xtask oracles check [CORPUS...] [--actual <export.jsonl>] [--strict] [--limit N]
cargo xtask oracles refresh [CORPUS...] [--language typescript|python] [--poc <graph-compiler>]
```

### Corpora

| Corpus | Sources | Languages | Oracle file | Where it runs |
| --- | --- | --- | --- | --- |
| `ts-orders` | `playgrounds/ts-orders` | TypeScript | committed | pull requests |
| `entrances` | `fixtures/entrances` | TypeScript | committed | pull requests |
| `py-orders` | `playgrounds/py-orders` | Python | committed | pull requests |
| `mixed-playground` | `playgrounds` | both | committed | pull requests |
| `python-ranges` | `fixtures/python-ranges` (source root `fixtures`) | Python | committed | pull requests |
| `fastapi-template` | lock `fastapi-full-stack-template-0.12.0` (source root `backend`) | Python | committed | nightly (downloaded) |
| `typescript-6.0.3-src-upstream` | lock `typescript-6.0.3-src-upstream`: the v6.0.3 tag's `src/`, 601 files | TypeScript | `target/oracles/` | nightly (downloaded, regenerated) |
| `typescript-6.0.3-src` | lock `typescript-6.0.3-src`: the PoC's 602-file copy | TypeScript | `target/oracles/` | where the PoC checkout is |
| `codegraph-self` | lock `codegraph-self`: the PoC compiler's 186 files (`reports/`, `test/fixtures/` excluded) | TypeScript | `target/oracles/` | where the PoC checkout is |

- **Lock corpora** are found at `$UNFOLD_CORPORA_DIR/<lock id>/`, else at the lock's PoC checkout
  plus `poc_local_path`. Before comparing, the check verifies the compiled files against the lock's
  `listing_sha256` or `fingerprints`, so a different checkout is never measured. `check` without
  names skips lock corpora that aren't available (`--strict` fails instead); named corpora must be.
- **Every corpus compiles from a temporary copy** (symbolic links left out; Unfold never follows a
  linked folder), with `settings` from `corpora.toml` written into the copy when the corpus has no
  `.unfold/config.toml` of its own. Nothing is written where the corpus lives.
- **Why the large TypeScript oracle files aren't committed.** The 602-file corpus's answers are
  23 MB (100,000 call sites), regenerated whenever the engine's export changes block names. The
  corpus itself isn't in the repository either: its CI copy has to be downloaded, so the nightly
  job that downloads it also has Node and regenerates the answers in under a minute before checking
  them. Committing gzip would still churn megabytes per refresh for no gain in independence. The
  PoC's own copy differs from the upstream tag by one build-generated file
  (`compiler/diagnosticInformationMap.generated.ts`), so CI checks the upstream tag as its own corpus
  with its own floor, and the exact 602-file corpus runs where the PoC checkout is.
- **Not covered:** the PoC's isolation corpus (Pyright rows exist but its sources embed absolute
  paths and aren't pinned, see `corpora.lock.toml`), and the two Step 0 application repositories
  (not pinned yet). The playground's checker-backed generation (`poc/playground-ts-checker`) is a
  parity aid, not an oracle.

### Oracle files

One JSON object per line: a header, then `call` rows (one per call site, with the oracle's
`target` block or the `outcome` without one, and an optional `refused` policy refusal beside a
target) and `reference` rows (named arguments the oracle resolved, by `site` and `position`). The
fields and the TypeScript rules are in [`tools/oracles/README.md`](../../tools/oracles/README.md#typescript-checker-oracle).

- **`corpus_sha256`** hashes the corpus: a `sha256sum`-style listing of the language's sources and
  every configuration input of the graph. `check` recomputes it from its compile; when it differs
  the check fails with "stale oracle — rerun `cargo xtask oracles refresh <corpus>`".
- **TypeScript** files come from `tools/oracles/typescript-oracle.mjs` (needs Node and
  `pnpm install --filter @unfold/oracles`).
- **Python** files are the proof of concept's saved Pyright 1.1.407 evaluator rows
  (`compilers/codegraph/reports/python-typescript/phase5/macos/{playground,ranges,target}-reference.json`
  at commit `5c98a6e`; the Linux copies are identical), converted by `refresh` exactly as the
  reference import converts: paths through `corpora.toml`'s `paths`, UTF-16 offsets to UTF-8 bytes
  through each file's text, and sites and targets named by the Unfold block with exactly that range
  (a target's decorated start; `class` for a class, a function-like block for a function). A row's
  target counts only when Pyright's canonical targets are exactly one indexed declaration, as the
  PoC's `comparePythonCalls` counted. Before converting, `refresh` checks that the corpus's sources
  are the ones the rows describe: the PoC's aggregate source hash (`poc_source_sha256` in
  `corpora.lock.toml`) must match. The header's `source` records the PoC commit, the rows file and
  its SHA-256, and Pyright's and typeshed's commits. No Pyright is run, and rows are never invented:
  a corpus without saved rows has no Python oracle.

### The gates

- **Precision (100%).** Every resolved `calls`/`constructs` edge (occurrence `seq` 0) and every
  resolved `references` edge of the language must name the oracle's target for that site (and
  argument position). Anything else (another target, no target, or no oracle site) is a
  disagreement, and fails unless `justified.toml` lists it with a written reason. An entry names
  the corpus, language, site, position (references), and Unfold's target, and must match exactly
  one disagreement; an entry that matches nothing is stale and fails, like a stale divergence.
- **Recall (floors).** As the PoC defined it: the call sites the oracle resolves (a target and no
  policy refusal) that Unfold resolves to the same target, over the call sites the oracle resolves.
  References are reported, not gated. `floors.toml` records each floor's counts; recall passes when
  it is at least that ratio, compared exactly, and the check says when a floor can rise.

### Results (2026-10-09)

| Corpus | Language | Precision | Justified | Recall | Floor | Proof of concept |
| --- | --- | --- | ---: | --- | --- | --- |
| `ts-orders` | TypeScript | 87/87 | 1 (C-8) | 71/71 | 71/71 | 46 of 71 |
| `entrances` | TypeScript | 5/5 | 0 | 3/3 | 3/3 | |
| `mixed-playground` | TypeScript | 87/87 | 1 (C-8) | 71/71 | 71/71 | 46 of 71 |
| `mixed-playground` | Python | 108/108 | 0 | 91/96 | 91/96 | 91 of 96 |
| `py-orders` | Python | 108/108 | 0 | 91/96 | 91/96 | 91 of 96 |
| `python-ranges` | Python | 4/4 | 0 | 3/3 | 3/3 | 3 of 3 |
| `fastapi-template` | Python | 196/196 | 0 | 185/232 | 185/232 | 185 of 232 |
| `typescript-6.0.3-src` | TypeScript | 74,912/74,912 | 0 | 70,720/73,649 (96.02%) | same | 96.0% (70,717 of 73,649) |
| `typescript-6.0.3-src-upstream` | TypeScript | 72,782/72,782 | 0 | 68,590/71,519 (95.90%) | same | |
| `codegraph-self` | TypeScript | 6,859/6,859 | 0 | 6,805/6,807 (99.97%) | same | 4,440 of 4,440 (in place) |

Precision counts calls, constructions, and references together. The one justified disagreement is
decision C-8: `const { printReport } = await import("./jobs/report.ts")`, where the checker's symbol
is a binding element with no body. On the 602-file corpus the oracle's denominators equal the PoC
checker-backed producer's exactly (73,649 calls, 4,192 references), and every one of the 2,929
calls Unfold misses there is `unknown_receiver`. The upstream tag lacks the generated diagnostics
map, whose 2,130 `diag()` calls all resolve, hence its lower ratio. `codegraph-self` compiles from a
copy, without the PoC's TypeScript fork beside it, so its denominator differs from the PoC's in-place
run; its two misses are calls through a constant object literal that also spreads another object.

