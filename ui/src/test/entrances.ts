/** Entrance catalog responses for tests: a project with launches by category, and one without. */

import type { AppRow, EntranceOverview, EntranceScope, LaunchRow } from "../ipc";

type Overview = { readonly view: "overview" } & EntranceOverview;
type Scope = { readonly view: "scope" } & EntranceScope;

function launchRow(overrides: Partial<LaunchRow> & Pick<LaunchRow, "key" | "folder" | "command">): LaunchRow {
  return {
    id: null,
    label: null,
    schedule: null,
    scheduleGloss: null,
    description: null,
    status: "ok",
    problem: null,
    note: null,
    targetSetByHand: false,
    starts: null,
    importPaths: [],
    reaches: null,
    routes: null,
    declaredIn: { path: "unfold.toml", line: null },
    editable: "configured",
    ...overrides,
  };
}

function app(overrides: Partial<AppRow> & Pick<AppRow, "id" | "name" | "launches">): AppRow {
  return { description: null, origin: "declared", entrances: overrides.launches.length, routes: 0, alsoIn: [], ...overrides };
}

const DASHBOARD_WEB = launchRow({
  key: "entrance:configured:dashboard-web",
  id: "dashboard-web",
  folder: "data-pipelines",
  command: 'sh -c "uvicorn dashboard_builder.app:app --port ${PORT:-8010}"',
  status: "unreadable",
  problem: "Can't read sh -c",
});

export const INVOICE_START = launchRow({
  key: "entrance:configured:invoice-start",
  id: "invoice-start",
  label: "Start API",
  folder: "data-pipelines",
  command: "uvicorn api.main:app --host 0.0.0.0 --port 8000",
  reaches: 184,
  routes: 14,
});

const REVIEW_WORKER = launchRow({
  key: 'entrance:["package.script","process","","worker","src/worker.ts"]',
  folder: "pathway-agents",
  command: "npm run worker",
  reaches: 64,
  declaredIn: { path: "package.json", line: 11 },
  editable: "override",
});

/** The overview: one application in each category. */
export const OVERVIEW: Overview = {
  view: "overview",
  project: "p-orders0000",
  counts: { applications: 4, launches: 5, routes: 14, entrances: 22 },
  categories: [
    {
      category: "needs_look",
      applications: [
        app({
          id: "dashboard-builder",
          name: "Dashboard builder",
          description: "Query service called only by pathway-web's server.",
          entrances: 7,
          routes: 5,
          launches: [
            DASHBOARD_WEB,
            launchRow({
              key: "entrance:configured:dashboard-export",
              id: "dashboard-export",
              folder: "data-pipelines",
              command: "python -m dashboard_builder.export --from-env",
              schedule: "0 10 * * *",
              scheduleGloss: "daily at 10:00",
              reaches: 22,
            }),
          ],
        }),
      ],
    },
    {
      category: "services",
      applications: [
        app({
          id: "invoice-hitl",
          name: "Invoice HITL API",
          description: "Invoice upload, parsing runs, and human review.",
          entrances: 15,
          routes: 14,
          launches: [INVOICE_START],
        }),
      ],
    },
    { category: "workers", applications: [app({ id: "review-worker", name: "Review worker", origin: "inferred", launches: [REVIEW_WORKER] })] },
    {
      category: "scheduled",
      applications: [
        app({
          id: "purchase-eval",
          name: "Purchase ingestion eval",
          launches: [
            launchRow({
              key: "entrance:configured:purchase-eval",
              id: "purchase-eval",
              folder: "data-pipelines",
              command: "python3 evals/run_purchase_ingestion_eval.py",
              schedule: "0 11 * * *",
              scheduleGloss: "daily at 11:00",
              status: "not_indexed",
              note: "evals/ is in .gitignore, so this code isn't indexed",
            }),
          ],
        }),
      ],
    },
  ],
  unreached: 2,
};

/** Invoice HITL API's first rows: its launch, /runs, and a router Dashboard builder also mounts. */
export const INVOICE_SCOPE: Scope = {
  view: "scope",
  project: "p-orders0000",
  sections: [
    { title: "Launch", kind: "launch", mountedFrom: null, alsoIn: [], count: 1 },
    { title: "/runs", kind: "http", mountedFrom: null, alsoIn: [], count: 2 },
    { title: "/dashboard-builder", kind: "http", mountedFrom: "dashboard_builder/router.py", alsoIn: ["Dashboard builder"], count: 2 },
  ],
  rows: [
    { key: INVOICE_START.key, id: INVOICE_START.id, section: 0, kind: "process", method: null, trigger: null, title: "process invoice-start", handler: null, path: "unfold.toml", line: 3, reaches: 184, alsoIn: [] },
    { key: "entrance:post-runs", id: null, section: 1, kind: "http", method: "POST", trigger: "/runs", title: "POST /runs", handler: "create_run", path: "api/runs.py", line: 10, reaches: 22, alsoIn: [] },
    { key: "entrance:get-run", id: null, section: 1, kind: "http", method: "GET", trigger: "/runs/{run_id}", title: "GET /runs/{run_id}", handler: "get_run", path: "api/runs.py", line: 20, reaches: 6, alsoIn: [] },
    { key: "entrance:model", id: null, section: 2, kind: "http", method: "GET", trigger: "/model/{model}", title: "GET /model/{model}", handler: "model", path: "dashboard_builder/router.py", line: 4, reaches: 3, alsoIn: ["Dashboard builder"] },
    { key: "entrance:explore", id: null, section: 2, kind: "http", method: "POST", trigger: "/explore", title: "POST /explore", handler: "explore", path: "dashboard_builder/router.py", line: 9, reaches: 4, alsoIn: ["Dashboard builder"] },
  ],
  total: 15,
  nextCursor: "c5",
};

/** The entrances no launch reaches. */
export const UNREACHED_SCOPE: Scope = {
  view: "scope",
  project: "p-orders0000",
  sections: [{ title: "Commands", kind: "command", mountedFrom: null, alsoIn: [], count: 2 }],
  rows: [
    { key: "entrance:configured:import", id: "import", section: 0, kind: "command", method: null, trigger: "import", title: "command import", handler: "import_orders", path: "unfold.toml", line: 15, reaches: 7, alsoIn: [] },
    { key: "entrance:configured:report", id: "report", section: 0, kind: "command", method: null, trigger: "report", title: "command report", handler: "report_command", path: "unfold.toml", line: 21, reaches: null, alsoIn: [] },
  ],
  total: 2,
  nextCursor: null,
};

/** A project with routes but no launches. */
export const NO_LAUNCHES: Overview = {
  view: "overview",
  project: "p-orders0000",
  counts: { applications: 0, launches: 0, routes: 31, entrances: 31 },
  categories: [],
  unreached: 31,
};
