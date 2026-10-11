import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { projectRow, workspaceView } from "../test/workspace";
import type { CommandArgs, CommandName, Events, Preferences, WorkspaceView } from "./generated";
import { applyEngineEvent, engineKey, engineQueryOptions, keyFile, resetEngineQueries } from "./queries";

const P = "p-orders0000";
const Q = "p-payments00";
const ORDERS = "src/routes/orders.ts";
const CREATE = `${ORDERS}#function:createOrder`;
const LIST = `${ORDERS}#function:listOrders`;
const OTHER = "src/other.ts#function:x";
const ENTRANCE = 'entrance:["express.route","http","GET","/orders"]';

let client: QueryClient;

function seed<C extends CommandName>(command: C, args: CommandArgs<C>): void {
  client.setQueryData(engineKey(command, args), {});
}

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
    seed("file_view", { project, path: ORDERS });
    seed("file_view", { project, path: "src/other.ts" });
    seed("card", { project, key: CREATE });
    seed("card", { project, key: LIST });
    seed("card", { project, key: OTHER });
    seed("card", { project, key: ENTRANCE });
    seed("peek", { project, key: OTHER });
    seed("peek", { project, key: LIST });
    seed("relations", { project, key: OTHER });
    seed("callers", { project, key: OTHER });
    seed("preview", { project, key: OTHER });
    seed("entrances", { project });
    seed("project_summary", { project });
    seed("index_log", { project });
    seed("problems", { project });
    seed("project_config", { project });
    seed("compile_preview", { project });
    seed("scope_dir", { project, path: "" });
    seed("list_boards", { project });
    seed("board_thumbnails", { project, boards: [".unfold/boards/a.json"] });
    seed("board_thumbnails", { project, boards: [".unfold/boards/b.json"] });
    seed("board_refs", { project, key: CREATE });
    seed("line_labels", { project, lines: [{ source: OTHER, seq: 0 }] });
    seed("call_contract", { project, source: OTHER, seq: 0 });
    seed("type_definition", { project, key: OTHER });
    seed("type_uses", { project, key: OTHER });
  }
  seed("search", { query: "all" });
  seed("search", { query: "only-p", projects: [P] });
  seed("search", { query: "only-q", projects: [Q] });
  seed("search", { query: "defs", kinds: ["definitions"] });
  seed("drafts", {});
  seed("get_preferences", {});
  seed("app_info", {});
}

const UPDATED: Events["graph_updated"] = {
  type: "graph_updated",
  project: P,
  changedFiles: [ORDERS],
  removedFiles: [],
  changedKeys: [CREATE],
  changedEntrances: [],
  inputsChanged: false,
  everything: false,
  complete: true,
};

beforeEach(() => {
  client = new QueryClient();
  seedAll();
});

describe("engine keys", () => {
  it("key every call by command and arguments, and leave git_info always stale", () => {
    expect(engineKey("card", { project: P, key: CREATE })).toEqual(["engine", "card", { project: P, key: CREATE }]);
    expect(engineQueryOptions("git_info", { project: P })).toMatchObject({ staleTime: 0 });
    expect(engineQueryOptions("list_boards", { project: P })).not.toHaveProperty("staleTime");
    expect(keyFile(CREATE)).toBe(ORDERS);
    expect(keyFile(ENTRANCE)).toBe(ENTRANCE);
    expect(keyFile(`${ENTRANCE}#1`)).toBe(`${ENTRANCE}#1`);
    // A path may hold `#`; a step never does.
    expect(keyFile("src/c#/orders.ts#class:Orders/method:place")).toBe("src/c#/orders.ts");
    expect(keyFile("src/c#/orders.ts#")).toBe("src/c#/orders.ts");
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
    // The window has the workspace and is reading it again (a reconnect or focus refetch).
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

  it("graph_updated refreshes the changed files, keys, and the project's derived reads, in that project only", () => {
    applyEngineEvent(client, UPDATED);
    expect(stale()).toEqual(
      [
        `board_thumbnails:${P}:.unfold/boards/a.json`,
        `board_thumbnails:${P}:.unfold/boards/b.json`,
        `call_contract:${P}:`,
        `callers:${P}:${OTHER}`,
        // The changed key, another block of the changed file, and every entrance card.
        `card:${P}:${CREATE}`,
        `card:${P}:${ENTRANCE}`,
        `card:${P}:${LIST}`,
        `file_view:${P}:${ORDERS}`,
        `index_log:${P}:`,
        // Typed lines read other files than the caller's: every one of the project.
        `line_labels:${P}:`,
        `peek:${P}:${LIST}`,
        `preview:${P}:${OTHER}`,
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

  it("graph_updated refreshes the entrance map when entrances or inputs changed", () => {
    applyEngineEvent(client, { ...UPDATED, changedFiles: [], changedKeys: [], changedEntrances: [ENTRANCE] });
    expect(stale()).toContain(`entrances:${P}:`);
    expect(stale()).not.toContain(`entrances:${Q}:`);
    client = new QueryClient();
    seedAll();
    applyEngineEvent(client, { ...UPDATED, changedFiles: [], changedKeys: [], inputsChanged: true });
    expect(stale()).toContain(`entrances:${P}:`);
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
    expect(refreshed).not.toContain("drafts::");
    expect(refreshed).not.toContain("get_preferences::");
  });

  it("files_changed refreshes the changed files and their cards, and file search", () => {
    applyEngineEvent(client, { type: "files_changed", project: P, paths: [ORDERS], truncated: false });
    expect(stale()).toEqual([`card:${P}:${CREATE}`, `card:${P}:${LIST}`, `file_view:${P}:${ORDERS}`, "search::all", "search::only-p"].sort());
  });

  it("files_changed truncated refreshes every file view and card of the project", () => {
    applyEngineEvent(client, { type: "files_changed", project: P, paths: [], truncated: true });
    expect(stale().filter((entry) => !entry.startsWith("search"))).toEqual(
      [
        `card:${P}:${CREATE}`,
        `card:${P}:${ENTRANCE}`,
        `card:${P}:${LIST}`,
        `card:${P}:${OTHER}`,
        `file_view:${P}:${ORDERS}`,
        `file_view:${P}:src/other.ts`,
      ].sort(),
    );
  });

  it("boards_changed refreshes the project's board lists, the listed thumbnails, drafts, and every board_refs", () => {
    applyEngineEvent(client, { type: "boards_changed", project: P, boards: [".unfold/boards/a.json"], folders: false });
    expect(stale()).toEqual(
      [
        `board_refs:${P}:${CREATE}`,
        `board_refs:${Q}:${CREATE}`,
        `board_thumbnails:${P}:.unfold/boards/a.json`,
        "drafts::",
        `list_boards:${P}:`,
        `project_summary:${P}:`,
      ].sort(),
    );
  });

  it("problems_changed refreshes the project's problems and summary", () => {
    applyEngineEvent(client, { type: "problems_changed", project: Q, count: 2 });
    expect(stale()).toEqual([`problems:${Q}:`, `project_summary:${Q}:`]);
  });

  it("config_changed refreshes the project's settings, compile preview, and scope tree", () => {
    applyEngineEvent(client, { type: "config_changed", project: P });
    expect(stale()).toEqual([`compile_preview:${P}:`, `project_config:${P}:`, `scope_dir:${P}:`]);
  });

  it("preferences_changed replaces the preferences without a refetch", () => {
    const preferences = { theme: "dark" } as Preferences;
    applyEngineEvent(client, { type: "preferences_changed", preferences });
    expect(client.getQueryData(engineKey("get_preferences", {}))).toEqual(preferences);
    expect(stale()).toEqual([]);
  });

  it("leaves the cache alone for compile progress and board conflicts", () => {
    applyEngineEvent(client, { type: "compile_progress", project: P, stage: "extract", done: 1, total: 2 });
    applyEngineEvent(client, { type: "board_conflict", project: P, board: "b", file: "draft", savedRevision: "r1", diskRevision: null });
    expect(stale()).toEqual([]);
  });
});

describe("resetEngineQueries", () => {
  it("resets every project query after an engine restart, keeping preferences and app info", async () => {
    await resetEngineQueries(client);
    const left = client
      .getQueryCache()
      .getAll()
      .filter((query) => query.state.data !== undefined)
      .map((query) => label(query.queryKey));
    expect(left.sort()).toEqual(["app_info::", "get_preferences::"]);
  });
});
