# Oracles

Developer-only tools that export reference outputs from the frozen proof of concept (product name
"Codegraph", at `/Users/lazaragoev/Desktop/repos/graph-compiler`) and run independent oracles over
Unfold's output. Nothing here ships, and Unfold's tests never run these tools: they read the outputs
committed under `testdata/reference/`.

The PoC is only read. Its TypeScript runs in memory through the PoC's own installed `tsx`
(`reader/node_modules/tsx`, with its transform cache disabled); nothing is built, installed, or
written inside the PoC checkout.

## Reader reference

`export-reader.mjs` loads the PoC's frozen mixed playground generation (`.codecodegraph/` at the PoC
root, the same generation as `testdata/reference/poc/mixed-playground`) through the PoC's own
`reader/core/repository.ts` and writes, for each playground, what the PoC reader derives:

```sh
node tools/oracles/export-reader.mjs --poc /Users/lazaragoev/Desktop/repos/graph-compiler
# or: UNFOLD_POC=… pnpm --filter @unfold/oracles export-reader
```

| File | Contents (PoC function) |
| --- | --- |
| `catalog.json` | Every definition: `key`, `kind`, `name` (the label), `qualifiedName` (`Repository.definitions()`) |
| `applications.json` | `applications` (startup key, the sidebar's name, launch and member entrance keys), `otherEntrances`, `unindexedLaunches` (`Repository.overview()`) |
| `relations.json` | Per definition key: `callers` (owner, occurrence, kind, position), `outgoing` (owned edges in source order), `reachedFrom` (`Repository.relations()`) |
| `closures.json` | Per entrance key: the sorted closure (`Repository.closure()`), seeded with every resolved target, or for a process its startup plus, for a console script, the callable's module root |
| `mermaid.json` | Per entrance key: the PoC's Mermaid for that entrance and its handlers (`exportEntrance()`, what `codegraph export --entrance <key> --mermaid` prints), with the corpus's paths mapped in the text |
| `flows.json` | Per definition key: the PoC's Flow model (`buildFlow()`), compacted: exact source (`code`) dropped, chips reduced to occurrence, kind, target, and name, nested definitions to key, lines, and exits |
| `board-order-placement.mmd` | `ts-orders` only: the Mermaid of the PoC's `codegraph/boards/order-placement.board.json` (`exportMermaid()`, `codegraph export --board order-placement --mermaid`), the board `testdata/boards/order-placement.json` was converted from |

Outputs go to `testdata/reference/poc-reader/<playground>/` (`--out` overrides the root):

| Playground | Corpus whose path mapping applies |
| --- | --- |
| `ts-orders` | `playground-ts-native` (`ts-playground/` → ``) |
| `py-orders` | `python-playground` (`py-playground/` → ``) |

- **Paths and block keys** are mapped by the corpus's `paths` in `testdata/reference/corpora.toml`
  (read from that file), exactly as `cargo xtask reference import` maps them.
- **Entrance keys** are the keys in `testdata/reference/canonical/<corpus>.jsonl`. Each PoC entrance
  is matched to the canonical entrance with the same detector, kind, method, trigger, and first
  target text (a manual locator mapped like a path), in order among equal ones; the script fails if
  any entrance does not match exactly one.
- **Order** is deterministic: keys are sorted by UTF-16 code units, outgoing edges keep source
  order, and the JSON is indented with two spaces. Running the script twice gives identical bytes.
- The script fails if any playground source in the PoC checkout no longer matches the generation's
  hash, because the PoC's closures skip unverified files.

`crates/unfold-reader/tests/reference.rs` loads the canonical graphs into a temporary store and
compares every view with these files; `flow_reference.rs` compares the Flow of every definition,
`mermaid_reference.rs` every entrance diagram (structurally, modulo the labels decision U-27
changed), and `sample_board.rs` the sample board's diagram. `docs/reader.md` lists exactly what
each comparison allows to differ.

## Checks

Two scripts check Unfold's own outputs with independent tools. Install their dependencies once
with `pnpm install --filter @unfold/oracles` (Mermaid, jsdom, and Ajv, pinned in `package.json`).

```sh
node tools/oracles/check-mermaid.mjs        # or: pnpm --filter @unfold/oracles check-mermaid
node tools/oracles/check-board-schema.mjs   # or: pnpm --filter @unfold/oracles check-board-schema
```

- **`check-mermaid.mjs`** parses every Mermaid snapshot Unfold's tests write
  (`testdata/boards/*.mmd`, and every diagram in `testdata/mermaid/*.json`) with the real Mermaid
  parser (`mermaid.parse` over jsdom) and fails if any does not parse. Pass files to check others.
- **`check-board-schema.mjs`** validates `testdata/boards/*.json` (or the files passed) against
  `crates/unfold-reader/schema/unfold-board.schema.json` with Ajv (draft 2020-12, strict mode), and
  checks that `docs/schemas/unfold-board.schema.json` is an identical copy.

## TypeScript checker oracle

`typescript-oracle.mjs` is the independent precision oracle for TypeScript (migration plan Steps 4
and 7, [U-33](../../docs/decisions-and-tradeoffs.md#u-33-correctness-evidence-frozen-poc-reference-divergence-ledger-independent-oracles)).
It asks the published `typescript` package, pinned exactly in `package.json` (6.0.3, the version
the proof of concept's checker-backed producer used), what every call, construction, and named
argument of a corpus targets, and writes one JSONL file that `cargo xtask oracles check` compares
with Unfold's graph without Node. You normally run it through `cargo xtask oracles refresh`, which
compiles a temporary copy of the corpus, writes its canonical export, and calls:

```sh
pnpm install --filter @unfold/oracles     # once
node tools/oracles/typescript-oracle.mjs --folder <corpus> --export <canonical.jsonl> \
  --corpus <name> --out testdata/reference/oracles/<name>.typescript.jsonl
```

It reads the corpus and the export and writes only `--out`. The export names blocks; nothing else in
it is used (never Unfold's edges), so the answers are the checker's alone.

**Programs.** Files are grouped by their nearest `tsconfig.json` inside the folder, as the proof of
concept grouped them (`source/configuration.ts`): each group is one program whose roots are exactly
its indexed files, with the configuration's compiler options (its own `files`/`include`/`exclude`
ignored) and `noEmit`; without a configuration, `ESNext`/`NodeNext`. Indexed sources come from the
bytes the export hashed (each hash is checked), so offsets agree with Unfold's.

**Sites.** Every `CallExpression` (except `import(...)`), `NewExpression`, and
`TaggedTemplateExpression` in an indexed file is a site, matched to the Unfold `call`/`new` block
with exactly its byte range (`getStart()` to `end`, UTF-16 converted to UTF-8). A site with no such
block keeps its file and range; Unfold blocks with no site are counted in the header.

**What the checker names** (`typescript-oracle/checker.mjs`, following the proof of concept's
checker evidence in `frontend/typescript/evidence.ts`): the callee's symbol
(`getSymbolAtLocation`, with a literal element key looked up on the receiver's type), aliases
followed, each non-null constituent of a union receiver asked separately, and `super(...)` the
`extends` class. Namespace, interface, and type-alias declarations merged into a value are set
aside. Each remaining declaration is mapped to the body it denotes, in the policy's contexts
(`docs/graph-model.md`, "Value policy and reason order"):

| Declaration | `call` | `construct` | `reference` |
| --- | --- | --- | --- |
| Function, function expression, arrow, method with a body | that block | that block | that block |
| Class | none | its own constructor with a body, else the class | the class |
| `const` initialized with a function, arrow, or class expression (through transparent wrappers) | that block | that block | that block |
| Class field initialized with a function or arrow | that block | that block | that block |
| Property of an object literal that is a `const` initializer (C-2), with a function value | that block | that block | that block |
| Overload or ambient signature beside its implementation | the implementation | the implementation | the implementation |
| Parameter, `let`/`var`, accessor, other properties and values | none (`parameter`, `mutable_binding`, `accessor`, `not_concrete`) | | |

Declarations outside the indexed files make the site `external`; several different bodies, or a
mix of indexed and outside declarations, make it `ambiguous`; no declaration is `unknown`. A
`reference` target must be a function, method, or class. The body is mapped to the Unfold block of
the same kind with exactly its range; a body with no block is `unmapped` (never seen so far).

**Refusals kept beside the target.** When the checker names one body but the shared policy would
still refuse the site, the row keeps the target and adds `refused`: `overridden` (an instance method
or field that a subclass of the receiver's class overrides, by the proof of concept's override
census over every class in the corpus) or `unbound_this` (`this` in a non-arrow function that is
not a class member). Precision compares the target; recall leaves these sites out, as the proof of
concept's checker-backed producer did.

With these rules the oracle reproduces the proof of concept's checker-backed denominators exactly:
73,649 resolved calls and 4,192 named-argument targets on the 602-file TypeScript corpus, and 71
calls on `ts-orders`.

**Output** (`<corpus>.typescript.jsonl`; the Python files share the format):

```text
{"t":"header","format":"unfold-oracle","version":1,"corpus":"ts-orders","language":"typescript","oracle":"typescript","oracle_version":"6.0.3","corpus_sha256":"…","export_sha256":"…","files":34,"sites":212,"resolved":71,"references":15,"unfold_sites_without_checker_site":0}
{"t":"call","site":"src/cli.ts#…/call[0]","target":"src/jobs/import-orders.ts#function:importOrders"}
{"t":"call","site":"src/cli.ts#…/call[0]","outcome":"not_concrete"}
{"t":"call","site":"src/…","target":"src/…#…/method:list","refused":"overridden"}
{"t":"reference","site":"src/jobs/retry-failed.ts#…/call[0]","position":0,"target":"src/jobs/send-email.ts#function:flushOutbox"}
```

- `corpus_sha256` is SHA-256 of a `sha256sum`-style listing (`"<sha256>  <path>\n"`, sorted by path
  bytes) of the corpus's TypeScript sources and every configuration input of the export;
  `cargo xtask oracles check` recomputes it from its own compile and reports a stale oracle when it
  differs. `export_sha256` identifies the export used to name blocks.
- `resolved` counts `call` rows with a target and no `refused`: the recall denominator.
- Rows are sorted by file path bytes, then range; references follow their call by position.
  Running the script twice gives identical bytes.

The PoC's 602-file corpus takes about 15 seconds and 1.9 GB peak memory on an M-series Mac
(`refresh` passes `--max-old-space-size=8192`) and writes about 23 MB, which is why its oracle outputs live in `target/oracles/` instead of the
repository ([testdata/reference/README.md](../../testdata/reference/README.md#oracles)).

