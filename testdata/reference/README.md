# Oracles

The precision and recall gates ([testing](../../docs/testing.md#precision-and-recall-gates)): every call,
construction, and passed definition Unfold resolves must name the target an independent oracle names, and recall
stays at or above a floor that only rises. The oracles' answers are cached here, keyed by a hash of each corpus, so
`cargo xtask oracles check` needs no Node.

```text
oracles/
├── corpora.toml                 # the corpora: folder or lock id, languages, settings, Pyright rows
├── <corpus>.typescript.jsonl    # the TypeScript checker's answers (tools/oracles/typescript-oracle.mjs)
├── <corpus>.python.jsonl        # Pyright's answers, converted from saved rows
├── floors.toml                  # recall floors
└── justified.toml               # resolved links the oracle doesn't confirm, each with its reason
```

```sh
cargo xtask oracles check [CORPUS...] [--actual <export.jsonl>] [--strict] [--limit N]
cargo xtask oracles refresh [CORPUS...] [--language typescript|python] [--poc <graph-compiler>]
```

## Corpora

| Corpus | Sources | Languages | Oracle files | Runs |
| --- | --- | --- | --- | --- |
| `ts-orders` | `playgrounds/ts-orders` | TypeScript | committed | CI |
| `entrances` | `fixtures/entrances` | TypeScript | committed | CI |
| `py-orders` | `playgrounds/py-orders` | Python | committed | CI |
| `mixed-playground` | `playgrounds` | both | committed | CI |
| `python-ranges` | `fixtures/python-ranges` (source root `fixtures`) | Python | committed | CI |
| `fastapi-template` | lock `fastapi-full-stack-template-0.12.0` (source root `backend`) | Python | committed | locally |
| `typescript-6.0.3-src-upstream` | lock `typescript-6.0.3-src-upstream`: the v6.0.3 tag's `src/`, 601 files | TypeScript | `target/oracles/` | locally |
| `typescript-6.0.3-src` | lock `typescript-6.0.3-src`: the proof of concept's 602-file copy | TypeScript | `target/oracles/` | locally |
| `codegraph-self` | lock `codegraph-self`: the proof of concept's compiler, 186 files | TypeScript | `target/oracles/` | locally |

- **Lock corpora** are found at `$UNFOLD_CORPORA_DIR/<lock id>/`, else at the lock's proof-of-concept checkout plus
  `poc_local_path`. Before comparing, the check verifies the compiled files against the lock's `listing_sha256` or
  `fingerprints`, so a different copy is never measured. `check` without names skips lock corpora that aren't
  available (`--strict` fails instead); named corpora must be available.
- **Every corpus compiles from a temporary copy** (symbolic links left out), with `settings` from `corpora.toml`
  written into the copy when the corpus has no `unfold.toml`. Nothing is written where the corpus lives.
- **The large TypeScript oracles aren't committed.** The 602-file corpus's answers are about 23 MB and change
  whenever node keys do, so `refresh` writes them to `target/oracles/` wherever the corpus is available. The upstream
  tag lacks one build-generated file of the proof of concept's copy
  (`compiler/diagnosticInformationMap.generated.ts`), so it is a corpus of its own with its own floor.

The proof of concept is the local checkout named in `corpora.lock.toml`'s `[poc]` (commit `5c98a6e`, 2026-10-08).
Only its saved Pyright rows and its corpus copies are read; nothing is written there.

## Oracle files

One JSON object per line: a header, then `call` rows (one per call site, with the oracle's `target` or an `outcome`
without one, and an optional `refused` policy refusal beside a target) and `reference` rows (definitions passed as
arguments, by `site` and `position`). Sites and targets are named by node key. The fields and the TypeScript rules
are in [tools/oracles/README.md](../../tools/oracles/README.md#typescript-checker-oracle).

- **`corpus_sha256`** hashes the corpus: a `sha256sum`-style listing of the language's sources and every
  configuration input of the graph. `check` recomputes it from its compile; when it differs the check fails with
  "stale oracle", naming the `refresh` to run.
- **TypeScript** files come from `tools/oracles/typescript-oracle.mjs` (Node and
  `pnpm install --filter @unfold/oracles`).
- **Python** files are the proof of concept's saved Pyright 1.1.407 rows
  (`compilers/codegraph/reports/python-typescript/phase5/macos/{playground,ranges,target}-reference.json`),
  converted by `refresh`: paths through `corpora.toml`'s `paths`, UTF-16 offsets to UTF-8 bytes through each file's
  text, a site named by the call node with exactly its range, and a target by the definition node that ends where
  Pyright's does and starts between its statement's start (first decorator, or `async`) and its `def` or `class`
  keyword. A row counts only when Pyright's targets are exactly one indexed declaration. Before converting, `refresh`
  checks that the corpus's sources are the ones the rows describe (`poc_source_sha256` in `corpora.lock.toml`). No
  Pyright runs, and no row is invented: a corpus without saved rows has no Python oracle.

## The gates

- **Precision (exact).** Every `calls` link of the language that reaches a project definition, and every `passes`
  link (at the argument whose range holds the passed name), must name the oracle's target for that call node and
  position. Anything else (another target, no target, or no oracle site) is a disagreement and fails, unless
  `justified.toml` lists it with a written reason. An entry names the corpus, language, call node, position, and
  Unfold's target, and must match exactly one disagreement; an entry that matches nothing is stale and fails.
- **Recall (floors).** The call sites the oracle resolves (one target, no policy refusal) that Unfold resolves to
  the same target, over the call sites the oracle resolves. Passed definitions are reported, not gated. Each floor
  in `floors.toml` records its counts; recall passes when it is at least that ratio, compared exactly, and the check
  says when a floor can rise. Raise it then; never lower it.

The justified entries are one case, labeled C-8, in `ts-orders` and in `mixed-playground`:
`const { printReport } = await import("./jobs/report.ts")` binds that module's export
([TypeScript](../../docs/languages/typescript.md)), while the checker's symbol is a binding element with no body.
