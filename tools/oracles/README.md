# Oracles

Developer-only tools that run independent oracles and checks over Unfold's output. Nothing here
ships, and Unfold's tests never run these tools.

## Checks

One script checks Unfold's own output with an independent tool. Install its dependency once with
`pnpm install --filter @unfold/oracles` (Ajv, pinned in `package.json`).

```sh
node tools/oracles/check-board-schema.mjs   # or: pnpm --filter @unfold/oracles check-board-schema
```

- **`check-board-schema.mjs`** validates `testdata/boards/*.json` (or the files passed) against
  `crates/unfold-reader/schema/unfold-board.schema.json` with Ajv (draft 2020-12, strict mode), and
  checks that `docs/schemas/unfold-board.schema.json` is an identical copy.

## TypeScript checker oracle

`typescript-oracle.mjs` is the independent precision oracle for TypeScript
([testing policy](../../docs/testing.md#precision-and-recall-gates)).
It asks the published `typescript` package, pinned exactly in `package.json` (6.0.3), what every
call, construction, and passed definition of a corpus targets, and writes one JSONL file that `cargo xtask oracles check` compares
with Unfold's graph without Node. You normally run it through `cargo xtask oracles refresh`, which
compiles a temporary copy of the corpus, writes its canonical export, and calls:

```sh
pnpm install --filter @unfold/oracles     # once
node tools/oracles/typescript-oracle.mjs --folder <corpus> --export <canonical.jsonl> \
  --corpus <name> --out <name>.typescript.jsonl
```

It reads the corpus and the export and writes only `--out` (`refresh` writes into `testdata/reference/oracles/` for
a committed corpus, `target/oracles/` for the large TypeScript ones). The export names nodes; nothing else in it is
used (never Unfold's links), so the answers are the checker's alone.

**Programs.** Files are grouped by their nearest `tsconfig.json` inside the folder: each group is one program whose roots are exactly
its indexed files, with the configuration's compiler options (its own `files`/`include`/`exclude`
ignored) and `noEmit`; without a configuration, `ESNext`/`NodeNext`. Indexed sources come from the
bytes the export hashed (each hash is checked), so offsets agree with Unfold's.

**Sites.** Every `CallExpression` (except `import(...)`), `NewExpression`, and
`TaggedTemplateExpression` in an indexed file is a site, matched to the Unfold call node (a call or
a construction) with exactly its byte range (`getStart()` to `end`, UTF-16 converted to UTF-8). A
site with no such node keeps its file and range; Unfold call nodes with no site (such as a dynamic
`import(...)`) are counted in the header.

**What the checker names** (`typescript-oracle/checker.mjs`): the callee's symbol
(`getSymbolAtLocation`, with a literal element key looked up on the receiver's type), aliases
followed, each non-null constituent of a union receiver asked separately, and `super(...)` the
`extends` class. Namespace, interface, and type-alias declarations merged into a value are set
aside. Each remaining declaration is mapped to the body it denotes, in the value policy's contexts
([graph model](../../docs/graph-model.md)):

| Declaration | `call` | `construct` | `reference` |
| --- | --- | --- | --- |
| Function, function expression, arrow, method with a body | that block | that block | that block |
| Class | none | its own constructor with a body, else the class | the class |
| `const` initialized with a function or arrow (through transparent wrappers) | that block | that block | that block |
| `const` initialized with a class expression (through transparent wrappers) | none | its own constructor with a body, else the class | the class |
| Class field initialized with a function or arrow | that block | that block | that block |
| Property of an object literal that is a `const` initializer (C-2), with a function value | that block | that block | that block |
| Body-less, non-ambient overload signature beside its implementation | the implementation | the implementation | the implementation |
| Parameter, `let`/`var`, accessor, other properties and values | none (`parameter`, `mutable_binding`, `accessor`, `not_concrete`) | | |

Declarations outside the indexed files make the site `external`; several different bodies, or a
mix of indexed and outside declarations, make it `ambiguous`; no declaration is `unknown`. A
`reference` target must be a function, method, or class. The body is mapped to the Unfold
definition with exactly its range (a class node for a class, a function node for a function,
method, or constructor); a body with no such node is `unmapped`.

**Where an outside declaration lives.** An `external` row records the `module` label of the files
that declare the callee, named as Unfold labels that location
([TypeScript › Module labels](../../docs/languages/typescript.md#module-labels)):

| Declared in | `module` |
| --- | --- |
| A standard edition in the default library: `lib.es5.d.ts`, `lib.es2015.*` and later, not `lib.esnext.*` | `ecmascript` |
| The rest of the default library (`lib.dom.d.ts`, `lib.esnext.*`, …) | `global` |
| `@types/node`: inside `declare module "fs"`, or a global | `node:fs`, or `global` |
| A package under `node_modules` (`@types/a__b` is `@a/b`) | its name: `express`, `@scope/name` |
| Any other file | its path in the corpus |

Declarations in several of these give their labels as a sorted JSON array, the way Unfold writes
the label of several possible origins. The corpora are compiled from copies without
`node_modules`, so in practice the checker sees only the default library: package imports are
`unknown` to it, and only `ecmascript` and `global` are compared.

**Refusals kept beside the target.** When the checker names one body but the shared policy would
still refuse the site, the row keeps the target and adds `refused`: `overridden` (an instance method
or field that a subclass of the receiver's class overrides, by an override census over every class
in the corpus) or `unbound_this` (`this` in a non-arrow function that is
not a class member). Precision compares the target; recall leaves these sites out.

**Output** (`<corpus>.typescript.jsonl`; the Python files share the format):

```text
{"t":"header","format":"unfold-oracle","version":…,"corpus":"ts-orders","language":"typescript","oracle":"typescript","oracle_version":"6.0.3","corpus_sha256":"…","export_sha256":"…","files":34,"sites":212,"resolved":71,"references":15,"unfold_sites_without_checker_site":1}
{"t":"call","site":"src/cli.ts#main/switch[0]/case[0]/call:importOrders","target":"src/jobs/import-orders.ts#importOrders"}
{"t":"call","site":"src/cli.ts#main/switch[0]/case[1]/call:printReport","outcome":"not_concrete"}
{"t":"call","site":"src/cli.ts#/call:main().catch","outcome":"external","module":"ecmascript"}
{"t":"call","site":"src/services/order-service.ts#OrderService.place/call:this.payments.charge","target":"src/services/payment.ts#PaymentProvider.charge","refused":"overridden"}
{"t":"reference","site":"src/jobs/retry-failed.ts#retryFailed/call:withRetry","position":0,"target":"src/jobs/send-email.ts#flushOutbox"}
```

- `corpus_sha256` is SHA-256 of a `sha256sum`-style listing (`"<sha256>  <path>\n"`, sorted by path
  bytes) of the corpus's TypeScript sources and every configuration input of the export;
  `cargo xtask oracles check` recomputes it from its own compile and reports a stale oracle when it
  differs. `export_sha256` identifies the export used to name nodes.
- `resolved` counts `call` rows with a target and no `refused`: the recall denominator.
- Rows are sorted by file path bytes, then range; references follow their call by position.
  Running the script twice gives identical bytes.

The 602-file TypeScript corpus needs a large heap (`refresh` passes `--max-old-space-size=8192`) and writes about
23 MB, which is why its oracle outputs live in `target/oracles/` instead of the repository
([testdata/reference/README.md](../../testdata/reference/README.md#corpora)).

