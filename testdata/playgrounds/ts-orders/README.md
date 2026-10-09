# Orders playground

A small orders service with an Express API, BullMQ workers, node-cron schedules,
and a command-line interface. Orders live in memory for each process; Redis holds
queued work. Payment and email adapters log their actions locally.

Run these commands from the repository root with Node 26.

## Install and check

```sh
npm --prefix ts-playground install
npm run playground:check
npm run playground:build
```

After the lockfile exists, use `npm --prefix ts-playground ci` for a repeatable
installation. Dependencies belong to this package. The root check and build
scripts use the existing TypeScript fork in `compilers/typescript/built/local/`.
The package's own `build` script uses its pinned TypeScript 6.0.3 compiler.

Both builds use strict NodeNext checking and emit from `src/` into `dist/`.
Relative `.ts` imports are rewritten to `.js`, including dynamic imports.
Plain JavaScript files under `scripts/` and `report-processor.js` run directly
and are outside the TypeScript build.

## Run

The API and workers need Redis at `127.0.0.1:6379`. Set `REDIS_HOST` and
`REDIS_PORT` to use another instance. `PORT` defaults to `3000`, `API_VERSION` to
`1`, and `REPORT_QUEUE` to `reports`.

```sh
npm run playground:run
npm --prefix ts-playground run worker
npm --prefix ts-playground run scheduler
npm --prefix ts-playground run cli -- report
npm --prefix ts-playground run cli -- import /absolute/path/to/orders.json
```

Run each long-lived process in its own terminal. `playground:run` rebuilds with
the fork before starting the API. `npm --prefix ts-playground start` instead
builds with the package compiler. `npm --prefix ts-playground run dev` watches
the server source using tsx. The report worker resolves its sandbox processor with
a file URL relative to its module, so source and compiled launches both find the
package's `report-processor.js` independently of the working directory.

The CLI also supports `replay` and `retry`. Imports accept an array of order
drafts with `customerId`, `email`, and `items`; every item has `sku`, `quantity`,
and `price`. Each process has its own order store, so the report command does
not inspect the running API's memory.

The legacy exporter reads a JSON array of orders and writes summary records:

```sh
npm --prefix ts-playground run legacy:export -- /absolute/path/to/orders.json
```

Provide a second path to create an output file; existing files are preserved.

## Index

Indexing reads source without starting the API, workers, schedules, or Redis
connections. Generate a sorted list of TypeScript source paths relative to the
repository root, then pass both the root and the list explicitly:

```sh
node --input-type=module <<'NODE'
import { readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
function sources(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      return ["node_modules", "dist"].includes(entry.name) ? [] : sources(path);
    }
    return /\.(ts|tsx|mts|cts)$/.test(entry.name) && !/\.d\.(ts|mts|cts)$/.test(entry.name) ? [path] : [];
  });
}
writeFileSync("/tmp/codegraph-playground-files.json", JSON.stringify(sources("ts-playground/src").sort(), null, 2));
NODE
npm run codegraph:build
node compilers/codegraph/dist/main.js index . --files-from /tmp/codegraph-playground-files.json
node compilers/codegraph/dist/main.js validate .
node compilers/codegraph/dist/main.js index . --frontend typescript --files-from /tmp/codegraph-playground-files.json
node compilers/codegraph/dist/main.js validate .
```

The first index uses the native front end; the second uses the TypeScript-backed
front end. Both publish to the repository's `.codecodegraph/` directory, replacing
the previous generation. Explicit inputs avoid Git discovery and exclude
dependencies, emitted JavaScript, the JavaScript exporter, and the sandbox
processor. The root `codegraph.json` declares CLI commands and shutdown handling
and excludes the repository's tooling `package.json` from configuration inputs.

For the documentation map, see [Codegraph documentation](../docs/README.md).
The [playground coverage and expected Entrances view](../docs/testing.md#playground-coverage) describe what the index proves.
The [front-end differences](../docs/compiler.md#front-end-differences) explain the two outputs.
Both commands above replace the same generation; run the native index last when testing native output (review P-2).
See [the compiler guide](../compilers/codegraph/README.md#build-and-run) for commands and [the format](../docs/format.md#package-and-generations) for generation consistency.
