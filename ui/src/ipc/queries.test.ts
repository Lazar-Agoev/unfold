import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { projectRow, workspaceView } from "../test/workspace";
import type { CardView, CommandArgs, CommandName, DrawnFlow, Events, Header, Step, WorkspaceView } from "./generated";
import { applyEngineEvent, engineKey, engineQueryOptions, resetEngineQueries } from "./queries";

const P = "p-orders0000";
const Q = "p-payments00";
const ORDERS = "src/routes/orders.ts";
const PAY = "src/pay.ts";
// Keys are opaque: nothing here takes them apart.
const CREATE = "key-create";
const LIST = "key-list";
const OTHER = "key-other";
const OPENS = "key-opens";
const MISSING = "key-missing";
const ENTRANCE = "key-entrance";

let client: QueryClient;

function seed<C extends CommandName>(command: C, args: CommandArgs<C>, data: unknown = {}): void {
  client.setQueryData(engineKey(command, args), data);
}

/** A query with no data yet (still loading, or answered with an error). */
function seedEmpty<C extends CommandName>(command: C, args: CommandArgs<C>): void {
  client.getQueryCache().build(client, { queryKey: engineKey(command, args) });
}

/** A drawn flow of a definition in `path`, opening `inset` in place when given. */
function flow(key: string, path: string, inset?: DrawnFlow): DrawnFlow {
  const actions = { after: false, start: false, open: false };
  const header: Header = { key, kind: "function", form: "function", label: key, path, state: "current", line: 1, async: false, generator: false, definedWith: [], imports: [], actions };
  const steps: Step[] = inset
    ? [{ id: 0, state: "reached", kind: "call", key: `${key}-call`, callee: "pay", form: "call", to: { outcome: "project" }, awaited: false, sometimes: false, calls: [], callbacks: [], inset, actions }]
    : [];
  return { header, combinable: [], steps: { steps }, routes: [] };
}

function definition(key: string, path: string, inset?: DrawnFlow): CardView {
  return { type: "definition", flow: flow(key, path, inset) };
}

const START: CardView = { type: "start", key: ENTRANCE, kind: "entrance", app: "Shop API", name: "List orders", on: "GET /orders", code: [{ key: LIST, state: "found", label: "listOrders", path: ORDERS, line: 4 }] };

/** Which seeded queries are now stale, as `command:project:detail`. */
function stale(): string[] {
  return client
    .getQueryCache()
    .getAll()
    .filter((query) => query.state.isInvalidated)
    .map((query) => label(query.queryKey))
    .sort();
}

/** Which queries are left in the cache. */
function cached(): string[] {
  return client
    .getQueryCache()
    .getAll()
    .map((query) => label(query.queryKey))
    .sort();
}

function label(key: readonly unknown[]): string {
  const [, command, args] = key as [string, string, Record<string, unknown>];
  const project = typeof args["project"] === "string" ? args["project"] : "";
  const detail = args["path"] ?? args["key"] ?? args["query"] ?? args["boards"] ?? args["projects"] ?? "";
  return `${command}:${project}:${Array.isArray(detail) ? detail.join(",") : typeof detail === "string" ? detail : ""}`;
}

/** The same per-project queries in P and Q, plus the app-wide ones. */
function seedAll(): void {
  for (const project of [P, Q]) {
    seed("card", { project, key: CREATE }, definition(CREATE, ORDERS));
    seed("card", { project, key: LIST }, definition(LIST, ORDERS));
    seed("card", { project, key: OTHER }, definition(OTHER, "src/other.ts"));
    seed("card", { project, key: OPENS, open: [[`${OPENS}-call`]] }, definition(OPENS, "src/opens.ts", flow("key-pay", PAY)));
    seedEmpty("card", { project, key: MISSING });
    seed("card", { project, key: ENTRANCE }, START);
    seed("relations", { project, key: OTHER });
    seed("callers", { project, key: OTHER });
    seed("entrances", { project });
    seed("project_summary", { project });
    seed("problems", { project });
    seed("list_boards", { project });
    seed("board_refs", { project, key: CREATE });
    seed("call_contract", { project, key: OTHER });
    seed("type_definition", { project, key: OTHER });
    seed("type_uses", { project, key: OTHER });
  }
  seed("search", { query: "all" });
  seed("search", { query: "only-p", projects: [P] });
  seed("search", { query: "only-q", projects: [Q] });
  seed("search", { query: "defs", kinds: ["definitions"] });
  seed("get_settings", {});
  seed("app_info", {});
}

const UPDATED: Events["graph_updated"] = {
  type: "graph_updated",
  project: P,
  changedFiles: [ORDERS],
  removedFiles: [],
  changedKeys: [CREATE],
  inputsChanged: false,
  everything: false,
  complete: true,
};

beforeEach(() => {
  client = new QueryClient();
  seedAll();
});

describe("engine keys", () => {
  it("key every call by command and arguments", () => {
    expect(engineKey("card", { project: P, key: CREATE })).toEqual(["engine", "card", { project: P, key: CREATE }]);
    expect(engineQueryOptions("list_boards", { project: P })).not.toHaveProperty("staleTime");
  });
});

describe("invalidation by event", () => {
  it("workspace_changed replaces the workspace without a refetch", () => {
    const next = workspaceView([projectRow(P, "orders")]);
    applyEngineEvent(client, { type: "workspace_changed", workspace: next });
    expect(client.getQueryData(engineKey("workspace", {}))).toEqual(next);
    expect(stale()).toEqual([]);
  });

  it("project_state patches the row and refreshes only that project's summary", () => {
    const before = workspaceView([projectRow(P, "orders"), projectRow(Q, "payments")]);
    client.setQueryData(engineKey("workspace", {}), before);
    applyEngineEvent(client, { type: "project_state", project: P, state: { kind: "compiling", done: 3, total: 10 } });
    const after = client.getQueryData<WorkspaceView>(engineKey("workspace", {}));
    expect(after?.projects[0]?.state).toEqual({ kind: "compiling", done: 3, total: 10 });
    expect(after?.projects[1]).toBe(before.projects[1]);
    expect(stale()).toEqual([`project_summary:${P}:`]);
  });

  it("a workspace read answered before a state change doesn't put the old state back", async () => {
    const key = engineKey("workspace", {});
    const fresh = workspaceView([projectRow(P, "orders")]);
    const old = workspaceView([{ ...projectRow(P, "orders"), state: { kind: "updating", files: null } }]);
    let answerOld: (() => void) | undefined;
    let reads = 0;
    const queryFn = () => {
      reads += 1;
      // The first read was answered before the change, but its answer lands after the change's event.
      return reads === 1
        ? new Promise<WorkspaceView>((resolve) => {
            answerOld = () => {
              resolve(old);
            };
          })
        : Promise.resolve(fresh);
    };
    // The window has the workspace and is reading it again (a reconnect).
    client.setQueryData(key, old);
    const observer = new QueryObserver(client, { queryKey: key, queryFn });
    const unsubscribe = observer.subscribe(() => undefined);
    await vi.waitFor(() => {
      expect(answerOld).toBeDefined();
    });
    applyEngineEvent(client, { type: "project_state", project: P, state: { kind: "current" } });
    answerOld?.();
    await vi.waitFor(() => {
      expect(observer.getCurrentResult().isFetching).toBe(false);
    });
    expect(client.getQueryData<WorkspaceView>(key)?.projects[0]?.state).toEqual({ kind: "current" });
    expect(reads).toBe(2);
    unsubscribe();
  });

  it("project_reset drops every query of that project and nothing else", () => {
    applyEngineEvent(client, { type: "project_reset", project: P });
    const left = cached();
    expect(left.some((entry) => entry.includes(`:${P}:`))).toBe(false);
    expect(left).toContain(`card:${Q}:${CREATE}`);
    expect(left).toContain("search::all");
    expect(stale()).toEqual([]);
  });

  it("graph_updated refreshes the cards that draw a changed file or key, and the project's derived reads, in that project only", () => {
    applyEngineEvent(client, UPDATED);
    expect(stale()).toEqual(
      [
        `call_contract:${P}:${OTHER}`,
        `callers:${P}:${OTHER}`,
        // The changed key, another card of the changed file, a card not loaded yet, and every start card.
        `card:${P}:${CREATE}`,
        `card:${P}:${ENTRANCE}`,
        `card:${P}:${LIST}`,
        `card:${P}:${MISSING}`,
        `entrances:${P}:`,
        `project_summary:${P}:`,
        `relations:${P}:${OTHER}`,
        `type_definition:${P}:${OTHER}`,
        `type_uses:${P}:${OTHER}`,
        // Searches that included the project.
        "search::all",
        "search::defs",
        "search::only-p",
      ].sort(),
    );
  });

  it("graph_updated refreshes a card whose call opened in place draws a changed file", () => {
    applyEngineEvent(client, { ...UPDATED, changedFiles: [PAY], changedKeys: [] });
    expect(stale().filter((entry) => entry.startsWith("card:"))).toEqual([`card:${P}:${ENTRANCE}`, `card:${P}:${MISSING}`, `card:${P}:${OPENS}`]);
  });

  it("entrances_changed refreshes what lists or draws the project's apps and entrances", () => {
    applyEngineEvent(client, { type: "entrances_changed", project: P });
    expect(stale()).toEqual(
      [`card:${P}:${ENTRANCE}`, `card:${P}:${MISSING}`, `entrances:${P}:`, `project_summary:${P}:`, `relations:${P}:${OTHER}`, "search::all", "search::defs", "search::only-p"].sort(),
    );
  });

  it("graph_updated with everything refreshes every query of the project, its searches, and the workspace", () => {
    client.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
    applyEngineEvent(client, { ...UPDATED, changedFiles: [], changedKeys: [], everything: true });
    const refreshed = stale();
    expect(refreshed.filter((entry) => entry.includes(`:${Q}:`) || entry === "search::only-q")).toEqual([]);
    expect(refreshed).toContain(`problems:${P}:`);
    expect(refreshed).toContain(`list_boards:${P}:`);
    expect(refreshed).toContain("workspace::");
    expect(refreshed).toContain("search::all");
    expect(refreshed).not.toContain("get_settings::");
  });

  it("files_changed refreshes the cards that draw a changed file (and those not loaded yet), and file search", () => {
    applyEngineEvent(client, { type: "files_changed", project: P, paths: [ORDERS], truncated: false });
    expect(stale()).toEqual([`card:${P}:${CREATE}`, `card:${P}:${ENTRANCE}`, `card:${P}:${LIST}`, `card:${P}:${MISSING}`, "search::all", "search::only-p"].sort());
  });

  it("files_changed truncated refreshes every card of the project", () => {
    applyEngineEvent(client, { type: "files_changed", project: P, paths: [], truncated: true });
    expect(stale().filter((entry) => !entry.startsWith("search"))).toEqual(
      [`card:${P}:${CREATE}`, `card:${P}:${ENTRANCE}`, `card:${P}:${LIST}`, `card:${P}:${MISSING}`, `card:${P}:${OPENS}`, `card:${P}:${OTHER}`].sort(),
    );
  });

  it("boards_changed refreshes the project's board list, summary, and board_refs", () => {
    applyEngineEvent(client, { type: "boards_changed", project: P, boards: [".unfold/boards/a.json"], folders: false });
    expect(stale()).toEqual([`board_refs:${P}:${CREATE}`, `list_boards:${P}:`, `project_summary:${P}:`].sort());
  });

  it("problems_changed refreshes the project's problems and summary", () => {
    applyEngineEvent(client, { type: "problems_changed", project: Q, count: 2 });
    expect(stale()).toEqual([`problems:${Q}:`, `project_summary:${Q}:`]);
  });

  it("leaves the cache alone for settings changes, compile progress, and reloaded boards", () => {
    applyEngineEvent(client, { type: "config_changed", project: P });
    applyEngineEvent(client, { type: "compile_progress", project: P, stage: "extract", done: 1, total: 2 });
    applyEngineEvent(client, { type: "board_reloaded", project: P, board: "b", file: { kind: "deleted" } });
    expect(stale()).toEqual([]);
  });
});

describe("resetEngineQueries", () => {
  it("resets every project query after an engine restart, keeping settings and app info", async () => {
    await resetEngineQueries(client);
    const left = client
      .getQueryCache()
      .getAll()
      .filter((query) => query.state.data !== undefined)
      .map((query) => label(query.queryKey));
    expect(left.sort()).toEqual(["app_info::", "get_settings::"]);
  });
});

describe("the code panel's view", () => {
  /** The panel showing `createOrder` in ORDERS, a file in PAY, and one still loading, in P; and ORDERS in Q. */
  function seedViews(): void {
    client = new QueryClient();
    const view = (file: string) => ({ file, state: "current", lines: [], path: [], definitions: [] });
    seed("code_view", { project: P, target: { kind: "block", key: CREATE } }, view(ORDERS));
    seed("code_view", { project: P, target: { kind: "file", path: PAY } }, view(PAY));
    seedEmpty("code_view", { project: P, target: { kind: "block", key: MISSING } });
    seed("code_view", { project: Q, target: { kind: "block", key: CREATE } }, view(ORDERS));
  }

  function staleViews(): number {
    return client
      .getQueryCache()
      .getAll()
      .filter((query) => query.state.isInvalidated).length;
  }

  it("is asked for again whenever the project's graph changes, so it crops again", () => {
    seedViews();
    applyEngineEvent(client, UPDATED);
    expect(staleViews()).toBe(3);
  });

  it("shows a changed file at once: files_changed refreshes the views of that file (and those not loaded yet)", () => {
    seedViews();
    applyEngineEvent(client, { type: "files_changed", project: P, paths: [ORDERS], truncated: false });
    expect(staleViews()).toBe(2);
  });

  it("follows unfold.toml: entrances_changed refreshes the project's views", () => {
    seedViews();
    applyEngineEvent(client, { type: "entrances_changed", project: Q });
    expect(staleViews()).toBe(1);
  });
});
