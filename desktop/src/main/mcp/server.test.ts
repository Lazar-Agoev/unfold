import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { z } from "zod";
import cardCreateOrder from "../../../../testdata/workspace/commands/card_create_order.json";
import compilePreview from "../../../../testdata/workspace/commands/compile_preview.json";
import fileViewPage from "../../../../testdata/workspace/commands/file_view_orders_page.json";
import relationsCreateOrder from "../../../../testdata/workspace/commands/relations_create_order.json";
import searchPlace from "../../../../testdata/workspace/commands/search_place.json";
import type { AgentPage, AgentShellEvent, ViewReport } from "../../shared/agent-api";
import { AgentBridge, type AgentWindows } from "../agents/bridge";
import { pageRevision } from "../agents/revision";
import { ViewTracker } from "../agents/views";
import { EngineRequestError } from "../engine-host";
import type { WindowEntry } from "../windows";
import type { McpServices } from "./context";
import type { BoardList, ProjectSummary, WorkspaceView } from "./engine-types";
import { INSTRUCTIONS } from "./instructions";
import { definePrompt } from "./prompt";
import { McpService } from "./service";
import { TOOLS } from "./tools";

vi.mock("electron", async () => (await import("../fake-electron")).electronModule);

const PROJECT = "p-7k2m9q4xab";
const BOARD = { project: PROJECT, id: ".unfold/local/drafts/orders.json" };
const PAGE: AgentPage = {
  id: "page-1",
  title: "Placing an order",
  defaultView: "flow",
  cards: [{ id: "create-order", ref: { block: "src/orders.ts#function:createOrder" }, at: [0, 0] }],
  groups: [{ id: "checks", card: "create-order", title: "Check the request", members: [], collapsed: false }],
  labels: [],
  guards: [],
  links: [],
};

const WORKSPACE: WorkspaceView = {
  projects: [
    { id: PROJECT, name: "ts-orders", path: "/code/ts-orders", expanded: true, live: true, state: { kind: "current" }, git: { branch: "main", detached: null, worktrees: 1 } },
    {
      id: "p-resting000",
      name: "py-orders",
      path: "/code/py-orders",
      expanded: false,
      live: false,
      state: { kind: "resting", last: { kind: "current" }, atMs: 0 },
      git: null,
    },
  ],
  selected: PROJECT,
  recentBoards: [],
  removed: [],
};

const SUMMARY: ProjectSummary = {
  project: PROJECT,
  name: "ts-orders",
  path: "/code/ts-orders",
  state: { kind: "current" },
  status: { state: { kind: "current", files: 12, languages: ["typescript"] }, fallback: null, watching: "events", backgroundUpdates: true, complete: true },
  updatedAtMs: 0,
  files: 12,
  languages: ["typescript"],
  applications: 1,
  entrances: 3,
  health: { problems: 0, doesntParse: 0, settingsError: null, unreachedEntrances: 0, unindexedLaunches: 0, boardsNeedingReview: 0, boards: [] },
  git: { branch: "main", detached: null, worktrees: [] },
};

const BOARDS: BoardList = {
  project: PROJECT,
  emptyFolders: [],
  boards: [
    {
      project: PROJECT,
      id: BOARD.id,
      place: "draft",
      name: "orders",
      title: "Order placement",
      folder: null,
      pages: 1,
      cards: 1,
      projects: [],
      changes: 0,
      updatedAtMs: 0,
      heldBy: null,
      label: null,
      error: null,
    },
  ],
};

const CREATE_ORDER = "src/routes/orders.ts#function:createOrder";

/** The engine's answers by command, as untrusted requests reach it (a streamed command's messages go to `stream`). */
const ANSWERS: Readonly<Record<string, (args: Readonly<Record<string, unknown>>, stream: (message: unknown) => void) => unknown>> = {
  workspace: () => WORKSPACE,
  list_boards: (args) => (args["project"] === PROJECT ? BOARDS : { ...BOARDS, boards: [] }),
  project_summary: () => SUMMARY,
  project_config: () => ({
    exists: true,
    text: "",
    settings: { languages: null, include: [], exclude: ["dist/**"], disabledDetectors: [], pythonProjects: [], entrances: [], evidence: ["docs/queues.md"] },
    error: null,
    name: { value: "ts-orders", isDefault: true },
  }),
  entrances: () => ({
    project: PROJECT,
    applications: [{ startup: "src/server.ts", name: "server", language: "typescript", path: "src/server.ts", launches: [], total: 3, entrances: [] }],
    unreached: { total: 0, entrances: [] },
    unindexedLaunches: [],
  }),
  compile_preview: () => compilePreview,
  compile: () => ({ files: 12, changedFiles: 12, removedFiles: 0, changedEntrances: 3, complete: true, problems: 0, durationMs: 900 }),
  update_project_config: () => ({ exists: true, text: '[detectors]\ndisabled = ["node-cron.schedule"]\n', settings: null, error: null, name: { value: "ts-orders", isDefault: true } }),
  peek: () => ({ key: "entrance:x" }),
  search: () => ({ ...searchPlace, projects: [] }),
  search_text: (_args, stream) => {
    stream({ project: PROJECT, path: "src/routes/orders.ts", matches: [{ line: 12, text: "res.json(await repo.list());", column: 0, ranges: [[0, 3]] }] });
    return { files: 1, matches: 1, searched: 12, truncated: false, filesTruncated: false, projects: [] };
  },
  file_view: () => fileViewPage,
  card: () => cardCreateOrder,
  relations: () => relationsCreateOrder,
  line_labels: () => [],
  board_refs: () => ({ refs: [], truncated: false }),
  call_contract: () => ({
    source: `${CREATE_ORDER}/variable:order/call[0]`,
    seq: 0,
    target: "src/services/order-service.ts#class:OrderService/method:place",
    callerLine: 32,
    sends: [],
    returns: { binding: "variable", text: "order", awaited: true },
    throws: [],
    caught: false,
    calleeLines: [],
  }),
};

const BOARD_INFO = { title: "Order placement", place: "draft", changes: 0, revision: "r1" } as const;

/** What the window answers each agent request with, as the UI does: the window command's kind and result. */
function windowAnswer(request: AgentShellEvent): ["read" | "open" | "create" | "board" | "show", unknown] | null {
  switch (request.type) {
    case "agent_read":
      return ["read", { ok: true, page: PAGE, board: { ...BOARD_INFO, unsaved: false } }];
    case "agent_open_board":
      return ["open", { ok: true }];
    case "agent_create_board":
      return ["create", { ok: true, board: BOARD, info: BOARD_INFO, folder: null, pages: [{ id: "page-1", title: "Placing an order" }] }];
    case "agent_edit_board":
      return ["board", { ok: true, board: BOARD_INFO, pages: [{ id: "page-1", title: "Placing an order" }], results: [{ op: "add_page", status: "created", page: "page-2", title: "More" }] }];
    case "agent_show":
      return ["show", { ok: true, page: "page-1" }];
    default:
      return null;
  }
}

/** A window that holds the board and answers agent requests at once, as the UI does. */
function fakeWindows(bridge: () => AgentBridge): AgentWindows {
  const entry = { id: 1, loaded: true } as WindowEntry;
  return {
    get: (id) => (id === 1 ? entry : undefined),
    all: () => [entry],
    create: () => entry,
    focus: () => undefined,
    deliver: (_entry, event) => {
      const request = event as AgentShellEvent & { requestId: number };
      const answer = windowAnswer(request);
      if (answer) {
        queueMicrotask(() => {
          bridge().settle(1, answer[0], request.requestId, answer[1]);
        });
      }
    },
  };
}

interface Running {
  readonly service: McpService;
  readonly url: URL;
  readonly views: ViewTracker;
  readonly calls: [string, Readonly<Record<string, unknown>>][];
}

const running: McpService[] = [];
const clients: Client[] = [];

beforeEach(() => {
  vi.spyOn(console, "warn").mockImplementation(() => undefined);
  vi.spyOn(console, "error").mockImplementation(() => undefined);
});

afterEach(async () => {
  await Promise.all(clients.splice(0).map((client) => client.close()));
  await Promise.all(running.splice(0).map((service) => service.close()));
  vi.restoreAllMocks();
});

async function start(overrides: Partial<McpServices> = {}, options: Partial<ConstructorParameters<typeof McpService>[0]> = {}): Promise<Running> {
  const views = new ViewTracker();
  const calls: Running["calls"] = [];
  const bridge: AgentBridge = new AgentBridge({ windows: fakeWindows(() => bridge), views, timeoutMs: 1_000 });
  const service = new McpService({
    services: {
      bridge,
      engine: (command, args, onStream) => {
        calls.push([command, args]);
        const answer = ANSWERS[command];
        return answer
          ? Promise.resolve(answer(args, (message) => onStream?.(message)))
          : Promise.reject(new EngineRequestError({ code: "not_found", message: `no fake for ${command}` }));
      },
      confirmAddProject: () => Promise.resolve(false),
      addProject: () => Promise.reject(new Error("no tool adds projects yet")),
      ...overrides,
    },
    version: "1.2.3",
    onStatus: () => undefined,
    ...options,
  });
  running.push(service);
  await service.configure({ enabled: true, port: 0 });
  return { service, url: new URL(service.status().address), views, calls };
}

/** A client on the current revision, or on an earlier one (the `initialize` handshake). */
async function connect(url: URL, era: "current" | "initialize", name = "claude-code", userAgent?: string): Promise<Client> {
  const client = new Client({ name, version: "2.0.0" }, era === "current" ? { versionNegotiation: { mode: { pin: "2026-07-28" } } } : {});
  await client.connect(new StreamableHTTPClientTransport(url, userAgent === undefined ? {} : { requestInit: { headers: { "user-agent": userAgent } } }));
  clients.push(client);
  return client;
}

function text(result: Awaited<ReturnType<Client["callTool"]>>): string {
  return result.content.map((block) => (block.type === "text" ? block.text : "")).join("\n");
}

describe.each(["current", "initialize"] as const)("the MCP server, on %s", (era) => {
  it("offers its tools, prompts, and instructions", async () => {
    const { url } = await start();
    const client = await connect(url, era);
    expect(client.getProtocolEra()).toBe(era === "current" ? "modern" : "legacy");
    expect(client.getNegotiatedProtocolVersion()).toBe(era === "current" ? "2026-07-28" : "2025-11-25");
    expect(client.getServerVersion()).toMatchObject({ name: "unfold", version: "1.2.3" });
    expect(client.getInstructions()).toBe(INSTRUCTIONS);
    expect(client.getServerCapabilities()).toMatchObject({ tools: {}, prompts: {} });
    for (const left of ["resources", "logging", "completions"]) {
      expect(client.getServerCapabilities()).not.toHaveProperty(left);
    }

    const { tools } = await client.listTools();
    expect(tools.map((tool) => tool.name)).toEqual(TOOLS.map((tool) => tool.name));
    for (const tool of tools) {
      expect(tool.annotations, tool.name).toMatchObject(TOOLS.find((defined) => defined.name === tool.name)?.annotations ?? {});
      expect(tool._meta?.["anthropic/alwaysLoad"]).toBe(["get_guide", "get_current_view"].includes(tool.name) ? true : undefined);
    }
    expect((await client.listPrompts()).prompts).toEqual([]);
  });

  it("calls each tool, as the agent the client names", async () => {
    const { url, views, calls } = await start();
    const client = await connect(url, era);

    const guide = text(await client.callTool({ name: "get_guide", arguments: { topic: "start" } }));
    expect(guide.split("\n")[0]).toBe("Unfold · as Claude");
    expect(guide).toContain("# Working in Unfold");
    expect(text(await client.callTool({ name: "get_guide", arguments: { topic: "diagrams" } }))).toContain(
      `The guide topic "diagrams" isn't written yet. Written topics: start.`,
    );

    expect(text(await client.callTool({ name: "get_current_view", arguments: {} }))).toContain("No Unfold window has reported");
    const report: ViewReport = { board: BOARD, page: "page-1", selection: { kind: "card", id: "create-order" }, lens: "flow", visible: ["create-order", "checks"], tabs: [BOARD] };
    views.report(1, report);
    const view = text(await client.callTool({ name: "get_current_view", arguments: {} })).split("\n");
    expect(view).toEqual([
      `Unfold · ts-orders › Order placement (draft) › Placing an order · rev ${pageRevision(PAGE)} · as Claude`,
      `Ids: project ${PROJECT}, board ${BOARD.id}, page page-1.`,
      "Lens: Flow.",
      "Selected: card create-order: src/orders.ts#function:createOrder.",
      "The page has 1 cards. In view: 2 items:",
      "- card create-order: src/orders.ts#function:createOrder",
      '- step group checks in card create-order: "Check the request"',
      "Next: read_page reads the whole page. From now on, pass these board and page ids to other tools.",
    ]);

    const projects = text(await client.callTool({ name: "list_projects", arguments: {} })).split("\n");
    expect(projects).toEqual([
      "Unfold · as Claude",
      "2 projects:",
      `- ts-orders (id ${PROJECT}): current · TypeScript · branch main · 1 board`,
      "- py-orders (id p-resting000): resting (last: current) · 0 boards",
      "Next: get_project for one project's applications, settings, and boards.",
    ]);
    // Listing never wakes a resting project.
    expect(calls.filter(([command, args]) => command === "project_summary" && args["project"] === "p-resting000")).toEqual([]);

    const project = text(await client.callTool({ name: "get_project", arguments: { project: "ts-orders" } }));
    expect(project.split("\n")[0]).toBe("Unfold · ts-orders · as Claude");
    expect(project).toContain("1 application, 3 entrances in all:\n- server (src/server.ts, TypeScript): 3 entrances");
    expect(project).toContain("evidence files docs/queues.md");
    expect(project).toContain(`- Order placement (id ${BOARD.id}): draft, 1 page, 1 card`);
  });

  it("calls each tool that sets up, navigates, and reads code", async () => {
    const { url } = await start({
      confirmAddProject: () => Promise.resolve(true),
      addProject: () => Promise.resolve({ status: "added", project: PROJECT }),
    });
    const client = await connect(url, era);
    const calls: [string, Record<string, unknown>][] = [
      ["add_project", { path: "/code/ts-orders" }],
      ["compile_project", { project: "ts-orders" }],
      ["update_project_settings", { project: "ts-orders", edits: [{ op: "set_detector", detector: "node-cron.schedule", enabled: false }] }],
      ["create_board", { project: "ts-orders", title: "Order placement", pages: ["Placing an order"] }],
      ["organize_boards", { project: "ts-orders", ops: [{ op: "rename", board: BOARD.id, title: "Orders" }] }],
      ["edit_pages", { board: BOARD.id, project: "ts-orders", ops: [{ op: "add", title: "More" }] }],
      ["show", { board: BOARD.id, project: "ts-orders", page: "page-1" }],
      ["search", { query: "place", kinds: ["functions", "text"] }],
      ["list_entrances", { project: "ts-orders" }],
      ["read_code", { key: CREATE_ORDER, project: "ts-orders" }],
      ["read_call", { source: `${CREATE_ORDER}/variable:order/call[0]`, project: "ts-orders" }],
      ["read_file", { project: "ts-orders", path: "src/routes/orders.ts" }],
      ["get_relations", { key: CREATE_ORDER, project: "ts-orders" }],
    ];
    for (const [name, args] of calls) {
      const result = await client.callTool({ name, arguments: args });
      expect(result.isError, `${name}: ${text(result)}`).not.toBe(true);
      expect(text(result), name).toMatch(/^Unfold · (ts-orders.* · )?as Claude\n/);
    }
  });

  it("refuses an op agents can't do, saying why", async () => {
    const { url } = await start();
    const client = await connect(url, era);
    const result = await client.callTool({ name: "organize_boards", arguments: { project: "ts-orders", ops: [{ op: "delete", board: BOARD.id }] } });
    expect(result.isError).toBe(true);
    expect(text(result)).toContain("Agents can't delete, publish, or discard boards");
  });

  it("refuses with errors that say what to do instead", async () => {
    const { url } = await start();
    const client = await connect(url, era);
    const missing = await client.callTool({ name: "get_project", arguments: { project: "shop" } });
    expect(missing.isError).toBe(true);
    expect(text(missing)).toBe(
      "Can't find the project: no project named or with the id “shop” is in the workspace. list_projects shows each project's name and id.",
    );
    const invalid = await client.callTool({ name: "get_guide", arguments: { topic: "everything" } });
    expect(invalid.isError).toBe(true);
  });
});

describe("who an agent is", () => {
  it("is the clientInfo of each request on the current revision", async () => {
    const { url, service } = await start();
    const codex = await connect(url, "current", "codex-mcp-client");
    expect(text(await codex.callTool({ name: "get_guide", arguments: { topic: "start" } }))).toMatch(/^Unfold · as Codex\n/);
    const other = await connect(url, "current", "Cursor Agent!");
    expect(text(await other.callTool({ name: "list_projects", arguments: {} }))).toMatch(/^Unfold · as Cursoragent\n/);
    expect(service.status().agents.map((agent) => [agent.author, agent.lastActedMs !== null])).toEqual([
      ["agent:cursoragent", true],
      ["agent:codex", true],
    ]);
  });

  it("is learned from initialize, by User-Agent, on earlier revisions", async () => {
    const { url, service } = await start();
    const claude = await connect(url, "initialize", "claude-code", "claude-code/2.0.31 (cli)");
    const codex = await connect(url, "initialize", "codex-mcp-client", "node");
    expect(text(await claude.callTool({ name: "get_guide", arguments: { topic: "start" } }))).toMatch(/^Unfold · as Claude\n/);
    expect(text(await codex.callTool({ name: "get_guide", arguments: { topic: "start" } }))).toMatch(/^Unfold · as Codex\n/);
    expect(service.status().agents.map((agent) => agent.author)).toEqual(["agent:codex", "agent:claude"]);
  });

  it("falls back to the User-Agent's product without initialize", async () => {
    const { url } = await start();
    const response = await fetch(url, {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json, text/event-stream", "user-agent": "zed/0.200" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/call", params: { name: "get_guide", arguments: { topic: "start" } } }),
    });
    expect(response.status).toBe(200);
    expect(await response.text()).toContain("Unfold · as Zed");
  });
});

describe("the server's framework", () => {
  it("serves the prompts it is given", async () => {
    const prompt = definePrompt({
      name: "tour",
      title: "Tour",
      description: "An onboarding board for a project.",
      args: z.object({ project: z.string() }),
      text: ({ project }) => `Make a tour of ${project}.`,
    });
    const { url } = await start({}, { prompts: [prompt] });
    const client = await connect(url, "current");
    expect((await client.listPrompts()).prompts.map((listed) => listed.name)).toEqual(["tour"]);
    const got = await client.getPrompt({ name: "tour", arguments: { project: "ts-orders" } });
    expect(got.messages).toEqual([{ role: "user", content: { type: "text", text: "Make a tour of ts-orders." } }]);
  });

  it("teaches from the engine's errors", async () => {
    const { url } = await start({
      engine: (command) =>
        command === "workspace"
          ? Promise.resolve(WORKSPACE)
          : Promise.reject(new EngineRequestError({ code: "engine_unavailable", message: "the Unfold engine could not be loaded" })),
    });
    const client = await connect(url, "current");
    const result = await client.callTool({ name: "get_project", arguments: { project: PROJECT } });
    expect(result.isError).toBe(true);
    expect(text(result)).toBe("The Unfold engine could not be loaded. Unfold's engine isn't running. Ask the person to check Unfold, then try again.");
  });

  it("answers a tool that fails inside Unfold with an internal error, and keeps serving", async () => {
    const { url } = await start({ engine: () => Promise.reject(new Error("boom")) });
    const client = await connect(url, "current");
    const result = await client.callTool({ name: "list_projects", arguments: {} });
    expect(result.isError).toBe(true);
    expect(text(result)).toBe("list_projects failed inside Unfold. Try again; if it fails again, tell the person.");
    expect(console.error).toHaveBeenCalledWith("[mcp] list_projects failed:", new Error("boom"));
    expect(text(await client.callTool({ name: "get_guide", arguments: { topic: "start" } }))).toContain("# Working in Unfold");
  });
});
