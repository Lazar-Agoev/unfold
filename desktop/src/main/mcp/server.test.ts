import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { z } from "zod";
import cardCreateOrder from "../../../../testdata/workspace/commands/card_create_order.json";
import fileViewPage from "../../../../testdata/workspace/commands/file_view_orders_page.json";
import relationsCreateOrder from "../../../../testdata/workspace/commands/relations_create_order.json";
import searchPlace from "../../../../testdata/workspace/commands/search_place.json";
import type { AgentPage, AgentShellEvent, ViewReport } from "../../shared/agent-api";
import { AgentBridge, type AgentWindows } from "../agents/bridge";
import { pageRevision } from "../agents/revision";
import { ViewTracker } from "../agents/views";
import { EngineRequestError } from "../engine-host";
import type { WindowEntry } from "../windows";
import type { McpServices, PageImages } from "./context";
import type { BoardHealthView, BoardList, Page, ProjectSummary, ReadBoardResult, WorkspaceView } from "./engine-types";
import { INSTRUCTIONS } from "./instructions";
import { definePrompt } from "./prompt";
import { McpService } from "./service";
import { PROMPTS, TOOLS } from "./tools";

vi.mock("electron", async () => (await import("../fake-electron")).electronModule);

const PROJECT = "p-7k2m9q4xab";
const BOARD = { project: PROJECT, id: ".unfold/boards/orders.json" };
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
    { id: PROJECT, name: "ts-orders", path: "/code/ts-orders", expanded: true, live: true, state: { kind: "current" } },
    {
      id: "p-resting000",
      name: "py-orders",
      path: "/code/py-orders",
      expanded: false,
      live: false,
      state: { kind: "resting", last: { kind: "current" }, atMs: 0 },
    },
  ],
  selected: PROJECT,
};

const SUMMARY: ProjectSummary = {
  project: PROJECT,
  name: "ts-orders",
  path: "/code/ts-orders",
  state: { kind: "current" },
  status: { state: { kind: "current", files: 12, languages: ["typescript"] }, watching: "events", complete: true },
  updatedAtMs: 0,
  files: 12,
  languages: ["typescript"],
  health: { doesntParse: 0, boardsNeedingReview: 0, boards: [] },
};

const BOARDS: BoardList = {
  project: PROJECT,
  emptyFolders: [],
  boards: [
    {
      project: PROJECT,
      id: BOARD.id,
      name: "orders",
      title: "Order placement",
      folder: null,
      pages: 1,
      cards: 1,
      updatedAtMs: 0,
      label: null,
      error: null,
    },
  ],
};

const CREATE_ORDER = "src/routes/orders.ts#function:createOrder";

/** The board as its file holds it. */
const FILE: ReadBoardResult = {
  project: PROJECT,
  id: BOARD.id,
  name: "orders",
  document: { format: "unfold-board", version: 1, title: "Order placement", pages: [PAGE as unknown as Page] },
  revision: "r1",
};

/** The board's health: the card is missing. */
const HEALTH: BoardHealthView = {
  health: {
    items: [{ page: "page-1", kind: "missing", target: { card: "create-order" }, message: "src/orders.ts#function:createOrder is not in the graph.", repairs: ["reconnect", "removeCard"] }],
    missing: 1,
    outdated: 0,
    brokenGroups: 0,
    review: 0,
    brokenLinks: 0,
  },
  outlines: [{ page: "page-1", rows: [{ card: "create-order", depth: 0 }], unlabeled: [] }],
  label: "1 missing",
};

/** Every tool of design §6.11, in its order. */
const SIGNATURES = [
  "get_guide",
  "get_current_view",
  "list_projects",
  "get_project",
  "add_project",
  "compile_project",
  "update_project_settings",
  "create_board",
  "organize_boards",
  "edit_pages",
  "show",
  "search",
  "list_entrances",
  "read_code",
  "read_call",
  "read_file",
  "get_relations",
  "list_boards",
  "read_page",
  "check",
  "get_screenshot",
  "add_cards",
  "set_view",
  "arrange_page",
  "remove_items",
  "group_steps",
  "frame_cards",
  "add_resources",
  "connect",
  "add_notes",
  "describe",
  "label_steps",
  "propose",
  "repair",
  "finish_work",
];

/** A render window that draws a 1×1 image and finds nothing to fix. */
const RENDERER: PageImages = {
  screenshot: () => Promise.resolve({ ok: true, png: Buffer.from("png"), width: 1, height: 1 }),
  check: ({ page }) => Promise.resolve({ ok: true, title: "Order placement", page, pageTitle: "Placing an order", health: [], checks: [] }),
  checkBoard: () => Promise.resolve({ ok: true, title: "Order placement", pages: [{ page: "page-1", pageTitle: "Placing an order", health: [], checks: [] }] }),
};

/** The engine's answers by command, as untrusted requests reach it (a streamed command's messages go to `stream`). */
const ANSWERS: Readonly<Record<string, (args: Readonly<Record<string, unknown>>, stream: (message: unknown) => void) => unknown>> = {
  workspace: () => WORKSPACE,
  list_boards: (args) => (args["project"] === PROJECT ? BOARDS : { ...BOARDS, boards: [] }),
  project_summary: () => SUMMARY,
  project_config: () => ({
    exists: true,
    text: "",
    settings: { languages: null, include: [], exclude: ["dist/**"], disabledDetectors: [], pythonProjects: [], applications: [], entrances: [], overrides: [], evidence: ["docs/queues.md"] },
    error: null,
    name: { value: "ts-orders", isDefault: true },
  }),
  entrances: () => ({
    view: "overview",
    project: PROJECT,
    counts: { applications: 1, launches: 0, routes: 2, entrances: 3 },
    categories: [{ category: "services", applications: [{ id: "src/server.ts#", name: "server", description: null, origin: "inferred", entrances: 3, routes: 2, alsoIn: [], launches: [] }] }],
    unreached: 0,
  }),
  compile: () => ({ files: 12, changedFiles: 12, removedFiles: 0, changedEntrances: 3, complete: true, problems: 0, durationMs: 900 }),
  update_project_config: () => ({ exists: true, text: '[scope]\nexclude = ["scripts"]\n', settings: null, error: null, name: { value: "ts-orders", isDefault: true } }),
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
  read_board: () => FILE,
  board_health: () => HEALTH,
  export_mermaid_text: () => ({ text: "flowchart LR\n  c0[createOrder]\n" }),
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

const BOARD_INFO = { title: "Order placement", revision: "r1" } as const;

/** What the window answers each agent request with, as the UI does: the window command's kind and result. */
function windowAnswer(request: AgentShellEvent): ["edit" | "read" | "open" | "create" | "board" | "show" | "repair", unknown] | null {
  switch (request.type) {
    case "agent_edit":
      return ["edit", { ok: true, page: PAGE, board: BOARD_INFO, items: [{ kind: "card", id: "create-order", status: "kept", key: CREATE_ORDER, name: "createOrder" }], warnings: [] }];
    case "agent_repair":
      return ["repair", { ok: true, page: PAGE, board: BOARD_INFO }];
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
    current: () => entry,
    open: () => entry,
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
      renderer: RENDERER,
      engine: (command, args, onStream) => {
        calls.push([command, args]);
        const answer = ANSWERS[command];
        return answer
          ? Promise.resolve(answer(args, (message) => onStream?.(message)))
          : Promise.reject(new EngineRequestError({ code: "not_found", message: `no fake for ${command}` }));
      },
      engineAs: (_windowId, command, args) => {
        calls.push([command, args]);
        const answer = ANSWERS[command];
        return answer ? Promise.resolve(answer(args, () => undefined)) : Promise.reject(new EngineRequestError({ code: "not_found", message: `no fake for ${command}` }));
      },
      confirmAddProject: () => Promise.resolve(false),
      addProject: () => Promise.reject(new Error("no tool adds projects yet")),
      ...overrides,
    },
    version: "1.2.3",
    onStatus: () => undefined,
    port: 0,
    ...options,
  });
  running.push(service);
  await service.start();
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
    const { prompts } = await client.listPrompts();
    expect(prompts.map((prompt) => prompt.name)).toEqual(PROMPTS.map((prompt) => prompt.name));
    expect(prompts.find((prompt) => prompt.name === "explain-entrance")?.arguments).toEqual([
      { name: "entrance", description: expect.stringContaining("a route's method") as string, required: false },
      { name: "path", description: expect.stringContaining("POST /orders") as string, required: false },
    ]);
    const explain = await client.getPrompt({ name: "explain-entrance", arguments: { entrance: "POST", path: "/orders" } });
    expect(explain.messages).toEqual([{ role: "user", content: { type: "text", text: expect.stringContaining("Explain the entrance “POST /orders” end to end") as string } }]);
    const simplify = await client.getPrompt({ name: "simplify-function", arguments: {} });
    expect(simplify.messages[0]?.content).toMatchObject({ type: "text", text: expect.stringContaining("the function whose card is selected in Unfold") as string });
  });

  it("calls each tool, as the agent the client names", async () => {
    const { url, views, calls } = await start();
    const client = await connect(url, era);

    const guide = text(await client.callTool({ name: "get_guide", arguments: { topic: "start" } }));
    expect(guide.split("\n")[0]).toBe("Unfold · as Claude");
    expect(guide).toContain("# Working in Unfold");
    expect(text(await client.callTool({ name: "get_guide", arguments: { topic: "diagrams" } }))).toContain("# Diagrams: pages that read well");

    expect(text(await client.callTool({ name: "get_current_view", arguments: {} }))).toContain("No Unfold window has reported");
    const report: ViewReport = { board: BOARD, page: "page-1", selection: { kind: "card", id: "create-order" }, lens: "diagram", visible: ["create-order", "checks"], held: [BOARD] };
    views.report(1, report);
    const view = text(await client.callTool({ name: "get_current_view", arguments: {} })).split("\n");
    expect(view).toEqual([
      `Unfold · ts-orders › Order placement › Placing an order · rev ${pageRevision(PAGE)} · as Claude`,
      `Ids: project ${PROJECT}, board ${BOARD.id}, page page-1.`,
      "Lens: Diagram.",
      "Selected: card create-order: src/orders.ts#function:createOrder (expanded).",
      "The page has 1 cards. In view: 2 items:",
      "- card create-order: src/orders.ts#function:createOrder (expanded)",
      '- step group checks in card create-order: "Check the request"',
      "Next: read_page reads the whole page. From now on, pass these board and page ids to other tools.",
    ]);

    const projects = text(await client.callTool({ name: "list_projects", arguments: {} })).split("\n");
    expect(projects).toEqual([
      "Unfold · as Claude",
      "2 projects:",
      `- ts-orders (id ${PROJECT}): current · TypeScript · 1 board`,
      "- py-orders (id p-resting000): resting (last: current) · 0 boards",
      "Next: get_project for one project's applications, settings, and boards.",
    ]);
    // Listing never wakes a resting project.
    expect(calls.filter(([command, args]) => command === "project_summary" && args["project"] === "p-resting000")).toEqual([]);

    const project = text(await client.callTool({ name: "get_project", arguments: { project: "ts-orders" } }));
    expect(project.split("\n")[0]).toBe("Unfold · ts-orders · as Claude");
    expect(project).toContain("1 application, 3 entrances in all:\n- server (id src/server.ts#, no launch defined): 3 entrances");
    expect(project).toContain("evidence files docs/queues.md");
    expect(project).toContain(`- Order placement (id ${BOARD.id}): 1 page, 1 card`);
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
      ["update_project_settings", { project: "ts-orders", edits: [{ op: "exclude_from_graph", path: "scripts" }] }],
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

  it("calls each tool that reads, checks, builds, and repairs boards", async () => {
    const { url, views } = await start();
    views.report(1, { board: BOARD, page: "page-1", selection: { kind: "none" }, lens: "diagram", visible: [], held: [BOARD] });
    const client = await connect(url, era);
    const at = { board: BOARD.id, project: "ts-orders", page: "page-1" };
    const calls: [string, Record<string, unknown>][] = [
      ["list_boards", { project: "ts-orders" }],
      ["read_page", at],
      ["read_page", { ...at, format: "mermaid" }],
      ["check", at],
      ["add_cards", { ...at, cards: [{ key: CREATE_ORDER }] }],
      ["set_view", { ...at, items: [{ id: "create-order", expanded: true, expandCalls: [[`${CREATE_ORDER}/variable:order/call[0]`]] }] }],
      ["arrange_page", at],
      ["remove_items", { ...at, ids: ["create-order"] }],
      ["group_steps", { ...at, card: "create-order", groups: [{ title: "Read the draft", lines: [26, 26] }] }],
      ["frame_cards", { ...at, frames: [{ title: "server", members: ["create-order"] }] }],
      ["add_resources", { ...at, resources: [{ kind: "table", name: "orders", anchors: [{ key: "src/db/repository.ts#class:OrderRepository" }] }] }],
      ["connect", { ...at, connections: [{ kind: "writes", from: "create-order", to: "orders", description: "It inserts the order." }] }],
      ["add_notes", { ...at, notes: [{ text: "The client gets a job id, not the order." }] }],
      ["describe", { ...at, items: [{ id: "create-order", text: "Places the order." }] }],
      ["label_steps", { ...at, card: "create-order", labels: [{ step: `${CREATE_ORDER}/variable:draft`, text: "Read the draft" }] }],
      ["propose", { ...at, card: "create-order", proposals: [{ after: `${CREATE_ORDER}/variable:order`, title: "Email a confirmation" }] }],
      ["repair", { ...at, item: "create-order", repair: "remove_card" }],
      ["finish_work", { summary: "Built the order flow" }],
    ];
    for (const [name, args] of calls) {
      const result = await client.callTool({ name, arguments: args });
      expect(result.isError, `${name}: ${text(result)}`).not.toBe(true);
      expect(text(result), name).toMatch(/^Unfold · (ts-orders.* · )?as Claude\n/);
    }
    const shot = await client.callTool({ name: "get_screenshot", arguments: at });
    expect(shot.content).toEqual([
      { type: "text", text: `Unfold · ts-orders › Order placement › Placing an order · rev ${pageRevision(PAGE)} · as Claude\nThe page (page id page-1), 1×1 px.\nNext: look at it as the person will, give a one-line verdict in the chat, and fix what you see with targeted edits.` },
      { type: "image", data: Buffer.from("png").toString("base64"), mimeType: "image/png" },
    ]);
  });

  it("refuses an op agents can't do, saying why", async () => {
    const { url } = await start();
    const client = await connect(url, era);
    const result = await client.callTool({ name: "organize_boards", arguments: { project: "ts-orders", ops: [{ op: "delete", board: BOARD.id }] } });
    expect(result.isError).toBe(true);
    expect(text(result)).toContain("Agents can't delete boards");
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

describe("the tools", () => {
  it("are exactly the 35, design §6.11's and propose, in their order", () => {
    expect(TOOLS.map((tool) => tool.name)).toEqual(SIGNATURES);
  });

  it("mark board reads read-only, removals destructive, and creating or updating by id idempotent", () => {
    const hints = (name: string) => TOOLS.find((tool) => tool.name === name)?.annotations;
    for (const name of ["list_boards", "read_page", "check", "get_screenshot"]) {
      expect(hints(name), name).toEqual({ readOnlyHint: true });
    }
    for (const name of ["add_cards", "set_view", "arrange_page", "group_steps", "frame_cards", "add_resources", "connect", "add_notes", "describe", "label_steps", "propose", "finish_work"]) {
      expect(hints(name), name).toEqual({ readOnlyHint: false, destructiveHint: false, idempotentHint: true });
    }
    for (const name of ["remove_items", "repair"]) {
      expect(hints(name), name).toEqual({ readOnlyHint: false, destructiveHint: true, idempotentHint: false });
    }
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
