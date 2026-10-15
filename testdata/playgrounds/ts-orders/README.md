# TypeScript orders playground

An orders service written to exercise the compiler: Express routes and middleware, BullMQ workers, node-cron
schedules, a CLI behind a package `bin`, package scripts, and ordinary services and classes. Queue, payment, and email
adapters give recognizable declarations to call.

This is test data. Tests compile temporary copies; nothing is installed, started, or run. `package.json` and
`tsconfig.json` are analyzed inputs, and the two JavaScript files (`report-processor.js`, `scripts/export.js`) are
there so entrances can point at code Unfold doesn't analyze.

## What it pins

- **`unfold.toml`**: 7 apps and 18 entrances, one per process the package starts and what each handles. "Admin
  stats" deliberately names a definition that moved, so the missing state and its one fix stay covered.
- **Goldens** in `testdata/goldens/`: `ts-orders.jsonl` (the canonical export), `ts-orders.entrances.txt` (each
  `code` key's state), `ts-orders.flows.txt` (every definition drawn); the command results in
  `testdata/workspace/commands/`; and the two boards in `testdata/boards/`.
- **Oracles**: the corpora `ts-orders` and `mixed-playground` in `testdata/reference/oracles/`.

Keep the difficult cases: they are what the goldens and oracles check. To change one, edit one behavior, run the
TypeScript crate's tests and those of `unfold-reader` and `unfold-workspace` (the drawn flows and command results),
read the golden diff before blessing, and run `cargo xtask oracles check ts-orders mixed-playground` when a call's
outcome can change.

- [Test data](../../README.md)
- [TypeScript front end](../../../docs/languages/typescript.md)
- [Testing](../../../docs/testing.md#precision-and-recall-gates)
- [Python counterpart](../py-orders/README.md)
