# Python orders playground

A runnable Python 3.12+ port of the [TypeScript orders service](../ts-playground/README.md), using FastAPI, Celery, and APScheduler. The API, worker, scheduler, and CLI each have a process-local, in-memory order store. Payments and email are local preview adapters; no external payment or mail service is contacted.

## Install and check

Run from the repository root. [uv](https://docs.astral.sh/uv/) installs the application only for these explicit runtime commands; graph indexing does not need this environment or installed dependencies.

```sh
uv sync --project py-playground --frozen
uv run --project py-playground --frozen pytest -q py-playground/tests
uv run --project py-playground --frozen python py-playground/scripts/smoke.py
```

`uv.lock` pins the complete runtime and development dependency resolution. `.python-version` selects Python 3.12. The wheel includes the regular `orders_playground` package, the `orders_extensions` namespace package, and the separately located legacy exporter. Python 3.12 type-parameter syntax is intentional in the retry helper.

The test suite exercises route dependencies, CRUD, a wrapped route, queue submission in eager mode, Celery task functions, APScheduler registrations and one-shot handlers, CLI dispatch, error handling, and Python-specific examples. The smoke script starts a real Uvicorn server on a temporary loopback port, sends HTTP requests, runs CLI commands, and shuts the server down. Neither check requires Redis. These checks do not prove real broker delivery or a long-running scheduler's clock behavior.

## Run the service

Run each long-lived process in its own terminal:

```sh
uv run --project py-playground --frozen orders-api
uv run --project py-playground --frozen orders-worker
uv run --project py-playground --frozen orders-scheduler
uv run --project py-playground --frozen orders report
uv run --project py-playground --frozen orders import py-playground/examples/orders.json
uv run --project py-playground --frozen orders retry
uv run --project py-playground --frozen orders replay
```

The default API address is `http://127.0.0.1:8000`. Set `PORT` or `API_VERSION` to change the listener or status route. The API supports `GET /health`, `GET /health/live`, `GET /v1/status`, `GET /admin/stats`, `GET /orders`, `GET /orders/{order_id}`, `POST /orders`, and `DELETE /orders/{order_id}`. POST and DELETE require an `x-user` header. This header is a fixture dependency, not production authentication.

```sh
curl -fsS http://127.0.0.1:8000/health
curl -fsS -X POST http://127.0.0.1:8000/orders \
  -H 'Content-Type: application/json' -H 'x-user: demo' \
  --data '{"customer_id":"demo","email":"demo@example.test","items":[{"sku":"BOOK","quantity":2,"price":12.5}]}'
```

Add `Prefer: respond-async` to enqueue an order instead of inserting it in the API's store. That branch returns a Celery job ID and requires Redis plus the worker process. `BROKER_URL` defaults to `redis://127.0.0.1:6379/0`. `REPORT_QUEUE` defaults to `reports`; the worker subscribes to `orders`, `emails`, and this configured report queue. The worker uses Celery's single-process pool for predictable local operation. API and worker stores are independent, so an asynchronously processed order does not appear in the API's list.

`ORDERS_EAGER=1` enables Celery eager execution when task definitions are loaded in the same process, as in the tests. It does not cause the API to import workers. APScheduler runs cleanup at 02:00 UTC and metrics every 15 minutes. The CLI report reads its own process's empty store.

The legacy exporter is runnable but intentionally excluded from the graph's `src/` census:

```sh
uv run --project py-playground --frozen orders-legacy-export py-playground/examples/orders.json
```

An optional second argument writes a new file; existing destinations are preserved. There are no database migrations.

## Indexing corpus

The graph corpus is every `.py` file under `py-playground/src/`, sorted by repository-relative path. Tests, runtime smoke tooling, `.venv`, and `legacy/` are excluded. `src/orders_extensions/` deliberately has no `__init__.py` and exercises namespace-package imports. The pure examples in `src/orders_playground/features/` are indexed but are not imported by the service launchers.

`codegraph.json` keeps Python manual declarations and source roots separate from the repository's TypeScript configuration. Paths in this package configuration are relative to `py-playground/`. The `legacy` source root permits an unindexed script target to be identified without giving it blocks. Python indexing support and its commands are owned by the [compiler guide](../compilers/codegraph/README.md), while [expected-outcomes.md](expected-outcomes.md) maps the complete fixture coverage and Entrances membership. No indexing operation invokes `uv`, imports this package, starts the service, or connects to Redis.

Framework API references: [FastAPI dependencies](https://fastapi.tiangolo.com/tutorial/dependencies/), [Celery eager tasks](https://docs.celeryq.dev/en/stable/userguide/configuration.html#task-always-eager), and [APScheduler cron triggers](https://apscheduler.readthedocs.io/en/3.x/modules/triggers/cron.html).
