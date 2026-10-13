# Oracles

Developer-only tools that run independent oracles and checks over Unfold's output. Nothing here
ships, and Unfold's tests never run these tools.

## Checks

Two scripts check Unfold's own outputs with independent tools. Install their dependencies once
with `pnpm install --filter @unfold/oracles` (Mermaid, jsdom, and Ajv, pinned in `package.json`).

```sh
node tools/oracles/check-mermaid.mjs        # or: pnpm --filter @unfold/oracles check-mermaid
node tools/oracles/check-board-schema.mjs   # or: pnpm --filter @unfold/oracles check-board-schema
```

- **`check-mermaid.mjs`** parses every Mermaid snapshot Unfold's tests write
  (`testdata/boards/*.mmd`) with the real Mermaid parser (`mermaid.parse` over jsdom) and fails if
  any does not parse. Pass files to check others.
- **`check-board-schema.mjs`** validates `testdata/boards/*.json` (or the files passed) against
  `crates/unfold-reader/schema/unfold-board.schema.json` with Ajv (draft 2020-12, strict mode), and
  checks that `docs/schemas/unfold-board.schema.json` is an identical copy.

## TypeScript checker oracle

`typescript-oracle.mjs` is the independent precision oracle for TypeScript
([testing policy](../../docs/testing.md#precision-and-recall-gates)).
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

The proof of concept's 602-file corpus takes about 15 seconds and 1.9 GB peak memory on an M-series Mac
(`refresh` passes `--max-old-space-size=8192`) and writes about 23 MB, which is why its oracle outputs live in `target/oracles/` instead of the
repository ([testdata/reference/README.md](../../testdata/reference/README.md#oracles)).

