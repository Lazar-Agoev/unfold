# Python orders playground

An orders service written to exercise the compiler: FastAPI routes and dependencies, Celery tasks, APScheduler jobs,
console scripts, and ordinary services and classes. It covers binding, imports across source roots, receivers,
annotations, and calls that stay unknown on purpose.

This is test data. Tests compile temporary copies; nothing is imported, installed, or run. `pyproject.toml` is an
analyzed input. `[scope]` leaves `legacy/`, `scripts/`, and `tests/` out of the graph, and `legacy/` stays a source
root, so the legacy export's entrance points at code Unfold doesn't analyze.

## What it pins

- **`unfold.toml`**: 7 apps and 18 entrances, one per console script and what each handles. "Admin stats"
  deliberately names a definition that moved, so the missing state and its one fix stay covered.
- **Goldens** in `testdata/goldens/`: `py-orders.jsonl` (the canonical export), `py-orders.entrances.txt` (each
  `code` key's state), and `py-orders.flows.txt` (every definition drawn).
- **Oracles**: the corpora `py-orders` and `mixed-playground` in `testdata/reference/oracles/`, from saved Pyright
  answers.

Keep the difficult cases: they are what the goldens and oracles check. To change one, edit one behavior, run the
Python crate's tests, read the golden diff before blessing, and run `cargo xtask oracles check py-orders
mixed-playground` when a call's outcome can change. Editing a Python source here makes its oracle stale for good:
`refresh` converts the saved Pyright rows only for the exact sources they describe.

- [Test data](../../README.md)
- [Python front end](../../../docs/languages/python.md)
- [Testing](../../../docs/testing.md#precision-and-recall-gates)
- [TypeScript counterpart](../ts-orders/README.md)
