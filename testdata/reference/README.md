# Oracles

The precision and recall gates ([docs/testing.md#choose-checks-for-a-local-change](../../docs/testing.md#choose-checks-for-a-local-change),
[testing policy](../../docs/testing.md#precision-and-recall-gates)):
every call, construction, and named reference Unfold resolves must name the target an independent
oracle names, and recall stays at or above a floor that only rises. The oracles' answers are cached
here, keyed by a hash of each corpus, so `cargo xtask oracles check` needs no Node.

The proof of concept (PoC, product name "Codegraph") is `/Users/lazaragoev/Desktop/repos/graph-compiler` at commit
`5c98a6ef6e5d578b6b8668ee8cb4f30316573588` (`5c98a6e`, 2026-10-08), read only.

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

## Corpora

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
  `unfold.toml` of its own. Nothing is written where the corpus lives.
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
  (not pinned yet).

## Oracle files

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
  at commit `5c98a6e`; the Linux copies are identical), converted by `refresh`: paths through
  `corpora.toml`'s `paths`, UTF-16 offsets to UTF-8 bytes
  through each file's text, and sites and targets named by the Unfold block with exactly that range
  (a target's decorated start; `class` for a class, a function-like block for a function). A row's
  target counts only when Pyright's canonical targets are exactly one indexed declaration, as the
  PoC's `comparePythonCalls` counted. Before converting, `refresh` checks that the corpus's sources
  are the ones the rows describe: the PoC's aggregate source hash (`poc_source_sha256` in
  `corpora.lock.toml`) must match. The header's `source` records the PoC commit, the rows file and
  its SHA-256, and Pyright's and typeshed's commits. No Pyright is run, and rows are never invented:
  a corpus without saved rows has no Python oracle.

## The gates

- **Precision (100%).** Every resolved `calls`/`constructs` edge (occurrence `seq` 0) and every
  resolved `references` edge of the language must name the oracle's target for that site (and
  argument position). Anything else (another target, no target, or no oracle site) is a
  disagreement, and fails unless `justified.toml` lists it with a written reason. An entry names
  the corpus, language, site, position (references), and Unfold's target, and must match exactly
  one disagreement; an entry that matches nothing is stale and fails.
- **Recall (floors).** As the PoC defined it: the call sites the oracle resolves (a target and no
  policy refusal) that Unfold resolves to the same target, over the call sites the oracle resolves.
  References are reported, not gated. `floors.toml` records each floor's counts; recall passes when
  it is at least that ratio, compared exactly, and the check says when a floor can rise.

## Results (2026-10-09)

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

