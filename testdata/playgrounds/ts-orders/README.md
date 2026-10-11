# TypeScript orders fixture

A static-analysis corpus with Express routes, BullMQ workers, node-cron schedules, package
scripts/bins, a CLI, and mixed module/launch shapes. Orders are in-memory per process; queue,
payment, and email adapters provide recognizable declarations for compiler/reader tests.

This is test data, not a second workspace application. Tests analyze temporary copies without
starting the API/workers, connecting Redis, installing packages, or executing launch scripts.
Package metadata and written source are the inputs; generated output is not required.

## Expectations

`expected-entrances.txt` records entrance expectations. `testdata/reference/` contains frozen
canonical/reader and oracle answers. Relevant fixtures exercise imports/exports, emitted paths,
annotations, callbacks, constructors, scopes, detectors, and conservative unresolved cases.
Do not remove difficult examples or bless changed output without explaining the change.

## Working on a case

Find the relevant source and its language/detector test, edit one behavior, and run that focused
check. Larger changes may need parity/oracle checks for `ts-orders` or `mixed-playground`.
Runtime scripts in `package.json` are analyzed examples, not prerequisites for compiler tests.

- [Test-data conventions](../../README.md)
- [TypeScript rules](../../../docs/languages/typescript.md)
- [Entrances](../../../docs/engine.md#entrance-detectors)
- [Validation policy](../../../docs/testing.md#precision-and-recall-gates)
