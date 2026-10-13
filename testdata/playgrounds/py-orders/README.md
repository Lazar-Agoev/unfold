# Python orders fixture

A static-analysis corpus with FastAPI routes/dependencies, Celery tasks, APScheduler jobs,
console scripts, project roots, and ordinary service/class code. It exercises Python binding,
imports, receivers, annotations, framework origins, and conservative body proof.

This is test data. Tests compile temporary copies without importing the application,
installing dependencies, connecting external services, or running Python. `pyproject.toml`
and shared `unfold.toml` declarations are analyzed inputs.

## Expectations

`expected-entrances.txt` records entrance expectations. Frozen graph/reader outputs and
independent Pyright answers live under `testdata/reference/`. Excluded source roots are
intentional examples of `not_indexed`; unresolved/wrapper/unsupported targets are part of
the expected policy, not automatically missing test setup.

Use the relevant Python binder/typed/entrance test and fixture for an edit. Broader semantic
changes may need oracle checks for `py-orders` or `mixed-playground`.

- [Test-data conventions](../../README.md)
- [Python rules](../../../docs/languages/python.md)
- [Entrances](../../../docs/engine.md#entrance-detectors)
- [Validation policy](../../../docs/testing.md#precision-and-recall-gates)
- [TypeScript counterpart](../ts-orders/README.md)
