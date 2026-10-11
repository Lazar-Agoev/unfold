import { describe, expect, it, vi } from "vitest";
import cardCreateOrder from "../../../../../testdata/workspace/commands/card_create_order.json";
import type { AgentPage, AgentShellEvent } from "../../../shared/agent-api";
import { pageRevision } from "../../agents/revision";
import type { BoardSummary, CardView, LabeledLine, Page, ReadBoardResult, TypeDefinition } from "../engine-types";
import { listBoards, readPage } from "./pages";
import { harness, PROJECT, projectRow, recorded } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const CREATE_ORDER = "src/routes/orders.ts#function:createOrder";
const PLACE = "src/services/order-service.ts#class:OrderService/method:place";
const ENTRANCE = 'entrance:["express.route","http","POST","/orders","src/routes/orders.ts"]';
const CART = "src/cart.ts#interface:Cart";
const CART_ITEM = "src/cart.ts#interface:CartItem";
const BOARD = { project: PROJECT, id: ".unfold/local/drafts/order-placement.json" };
const BOARD_INFO = { title: "Order placement", place: "draft", changes: 0, revision: "r1" } as const;

const PAGE: AgentPage = {
  id: "page-1",
  title: "Placing an order",
  defaultView: "diagram",
  cards: [
    { id: "post-orders", ref: { entrance: ENTRANCE }, at: [0, 0] },
    { id: "create-order", ref: { block: CREATE_ORDER }, at: [320, 0], view: "flow", description: { text: "Places the order now, or queues it.", author: "agent:claude", signature: "s" } },
    { id: "place", ref: { block: PLACE }, at: [800, 0], folded: true },
    { id: "cart", ref: { block: CART }, at: [0, 400] },
    { id: "cart-item", ref: { block: CART_ITEM }, at: [400, 400] },
    { id: "charge", ref: { block: "src/pay.ts#function:charge", project: "payments" }, at: [800, 400] },
  ],
  groups: [{ id: "queue-it", card: "create-order", title: "Queue it for a worker", members: [`${CREATE_ORDER}/if[0]/variable:job`, `${CREATE_ORDER}/if[0]/return[0]`], collapsed: true, author: "agent:claude" }],
  labels: [{ key: `${CREATE_ORDER}/variable:draft`, text: "Read the order draft", author: "person:lazar", signature: "s" }],
  guards: [],
  links: [
    {
      id: "save-order",
      from: "create-order",
      to: "orders",
      kind: "writes",
      step: `${CREATE_ORDER}/variable:order/call[0]`,
      anchors: [{ key: "src/db/repository.ts#class:OrderRepository" }],
      author: "agent:codex",
      label: { text: "the order", author: "agent:codex", signature: "s" },
    },
    { id: "talks", from: "place", to: "charge", label: { text: "calls over HTTP", author: "person:lazar", signature: "s" } },
  ],
  frames: [
    { id: "server", title: "server", members: ["post-orders", "create-order", "checks"], look: "box", icon: "api", author: "agent:claude" },
    { id: "checks", title: "Shared", members: ["place"], look: "box", collapsed: true, author: "person:lazar", description: { text: "Code both services call.", author: "person:lazar", signature: "s" } },
  ],
  resources: [
    {
      id: "orders",
      kind: "table",
      name: "orders",
      columns: [{ name: "id", key: "primary" }, { name: "customer_id", type: "text", key: "foreign", references: "customers.id" }],
      anchors: [{ file: "db/schema.sql", lines: [3, 9] }],
      at: [800, 200],
      author: "agent:claude",
    },
  ],
  notes: [{ id: "note-1", text: "The client gets a job id, not the order.", at: [0, -40], attach: "create-order", author: "agent:claude" }],
};

const ENTRANCE_CARD: CardView = {
  type: "entrance",
  header: { key: ENTRANCE, kind: "http", title: "POST /orders", label: null, method: "POST", trigger: "/orders", triggerStatus: "exact", detector: "express.route", language: "typescript" },
  evidence: { path: "src/routes/orders.ts", startLine: 40, endLine: 40, source: "current", code: null, text: null, pointer: null },
  targets: [{ position: 0, role: "handler", text: "createOrder", resolution: "resolved", target: CREATE_ORDER, name: "createOrder", path: "src/routes/orders.ts", line: 25, note: null, reason: null, module: null }],
  applications: ["server"],
} as unknown as CardView;

function blockCard(key: string, kind: string, qualifiedName: string, line: number): CardView {
  return {
    type: "block",
    header: { key, kind, name: qualifiedName, qualifiedName, path: key.slice(0, key.indexOf("#")), startLine: line, endLine: line + 5, language: "typescript", fileState: "current", source: "current", owner: "" },
    code: null,
    flow: null,
  } as unknown as CardView;
}

const DEFINITIONS: Readonly<Record<string, TypeDefinition>> = {
  [CART]: {
    key: CART,
    kind: "interface",
    name: "Cart",
    path: "src/cart.ts",
    line: 12,
    language: "typescript",
    members: [
      { name: "items", kind: "property", optional: false, readonly: false, default: false, type: { text: "CartItem[]", spans: [{ start: 0, end: 8, style: "declared", target: CART_ITEM }] } },
      { name: "owner", kind: "property", optional: false, readonly: false, default: false, type: { text: "string", spans: [{ start: 0, end: 6, style: "builtin" }] } },
    ],
    inherited: [],
    uses: 3,
  },
  [CART_ITEM]: { key: CART_ITEM, kind: "interface", name: "CartItem", path: "src/cart.ts", line: 6, language: "typescript", members: [], inherited: [], uses: 1 },
};

const LABEL: LabeledLine = {
  source: `${CREATE_ORDER}/variable:order/call[0]`,
  seq: 0,
  label: { inputs: [{ type: { text: "OrderDraft", spans: [] } }], inputsUnknown: false, output: { text: "Promise<Order>", spans: [] } },
};

const CARDS: Readonly<Record<string, () => CardView>> = {
  [CREATE_ORDER]: () => recorded<CardView>(cardCreateOrder),
  [ENTRANCE]: () => ENTRANCE_CARD,
  [PLACE]: () => blockCard(PLACE, "method", "OrderService.place", 13),
  [CART]: () => blockCard(CART, "interface", "Cart", 12),
  [CART_ITEM]: () => blockCard(CART_ITEM, "interface", "CartItem", 6),
};

function file(page: AgentPage = PAGE): ReadBoardResult {
  return {
    project: PROJECT,
    id: BOARD.id,
    place: "draft",
    name: "order-placement",
    document: { format: "unfold-board", version: 1, title: "Order placement", projects: [], pages: [page as unknown as Page, { ...(page as unknown as Page), id: "page-2", title: "Async path" }] },
    revision: "r1",
    published: null,
    heldBy: null,
  };
}

function app(window?: (request: AgentShellEvent) => unknown) {
  return harness({
    engine: {
      card: (args) => {
        const answer = CARDS[String(args["key"])];
        if (!answer) {
          throw new Error(`no card ${String(args["key"])}`);
        }
        return answer();
      },
      type_definition: (args) => DEFINITIONS[String(args["key"])],
      line_labels: () => [LABEL],
      read_board: () => file(),
      export_mermaid_text: () => ({ text: "flowchart LR\n  c0[createOrder]\n" }),
    },
    ...(window ? { window } : {}),
  });
}

const held = (request: AgentShellEvent) =>
  request.type === "agent_read" ? { ok: true, page: PAGE, board: { ...BOARD_INFO, unsaved: true }, sizes: { "create-order": { width: 520.4, height: 310 } } } : request.type === "agent_open_board" ? { ok: true } : undefined;

describe("read_page", () => {
  it("reads everything on a page from the window that holds it, with ids, and what the board draws between cards", async () => {
    const test = app(held);
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], tabs: [BOARD] });
    const { text, isError } = await test.call(readPage);
    expect(isError).toBe(false);
    const lines = text.split("\n");
    expect(lines.slice(0, 2)).toEqual([
      `Unfold · ts-orders › Order placement (draft) › Placing an order · rev ${pageRevision(PAGE)} · as Claude`,
      `Ids: project ${PROJECT}, board ${BOARD.id}, page page-1. Cards without their own view show Diagram. The window has edits it hasn't saved yet; they're included.`,
    ]);
    expect(lines).toContain("Cards (6):");
    expect(lines).toContain(`- POST /orders (card id post-orders): http entrance; key ${ENTRANCE}; Diagram (the page's default); at [0, 0]; in frame server`);
    expect(lines).toContain(
      `- createOrder (card id create-order): function at src/routes/orders.ts:25; key ${CREATE_ORDER}; Flow; at [320, 0], 520×310; in frame server; description by you: “Places the order now, or queues it.”`,
    );
    expect(lines).toContain(`- OrderService.place (card id place): method at src/services/order-service.ts:13; key ${PLACE}; Diagram (the page's default); folded; at [800, 0]; in frame checks`);
    expect(lines).toContain(
      "- charge (card id charge): key src/pay.ts#function:charge; from project payments; Diagram (the page's default); at [800, 400]; code unavailable: its project payments isn't in the workspace",
    );
    expect(lines).toContain("- “Queue it for a worker” (step group id queue-it) in card create-order: 2 statements, lines 28–30; collapsed; titled by you");
    expect(lines).toContain("Frames (2), nested as they are:");
    expect(lines).toContain("- server (frame id server; box, api; by you): holds POST /orders (post-orders), createOrder (create-order)");
    expect(lines).toContain("  - Shared (frame id checks; box, collapsed; by Lazar (a person)): holds OrderService.place (place); description by Lazar (a person): “Code both services call.”");
    expect(lines).toContain(
      "- table “orders” (resource id orders): at [800, 200]; columns id (primary), customer_id text (foreign → customers.id); anchors db/schema.sql:3–9; by you",
    );
    expect(lines).toContain(
      `- writes connection createOrder (create-order) → orders (orders) (link id save-order): from step at src/routes/orders.ts:32 (key ${CREATE_ORDER}/variable:order/call[0]); anchors OrderRepository (key src/db/repository.ts#class:OrderRepository); by Codex (an agent); label by Codex (an agent): “the order”`,
    );
    expect(lines).toContain("- link OrderService.place (place) → charge (charge) (link id talks): label by Lazar (a person): “calls over HTTP”");
    expect(lines).toContain("- note note-1, attached to createOrder (create-order) at offset [0, -40], by you: “The client gets a job id, not the order.”");
    expect(lines).toContain("- POST /orders (post-orders) → createOrder (create-order): handler createOrder");
    expect(lines).toContain(
      `- createOrder (create-order) → OrderService.place (place): calls orderService.place at line 32: (OrderDraft) → Promise<Order>; source ${CREATE_ORDER}/variable:order/call[0], seq 0`,
    );
    expect(lines).toContain(`- Cart (cart) → CartItem (cart-item): field items: CartItem[] (CartItem: ${CART_ITEM})`);
    expect(lines).toContain("Flow steps still unlabeled, in cards shown in Flow (label_steps labels them) (7):");
    expect(lines).toContain(`- createOrder (create-order): line 27 condition “req.header?” (key ${CREATE_ORDER}/if[0])`);
    expect(lines.some((line) => line.includes("/variable:draft)"))).toBe(false);
    expect(test.commands()).not.toContain("read_board");
  });

  it("reads a board no window holds from its file, a board's first page by default, and pages long pages", async () => {
    const test = app();
    const { text } = await test.call(readPage, { board: BOARD.id, project: "ts-orders" });
    expect(text.split("\n")[0]).toBe(`Unfold · ts-orders › Order placement (draft) › Placing an order · rev ${pageRevision(PAGE)} · as Claude`);
    expect(text).not.toContain("520×310");
    const second = await test.call(readPage, { page: `unfold://ts-orders/${BOARD.id}?page=page-2` });
    expect(second.text.split("\n")[0]).toContain("› Async path ·");

    const many: AgentPage = { ...PAGE, cards: Array.from({ length: 130 }, (_, index) => ({ id: `c${index}`, ref: { block: CART_ITEM }, at: [index, 0] })), groups: [], links: [], frames: [], resources: [], notes: [] };
    const paged = harness({ engine: { card: CARDS[CART_ITEM] ?? (() => null), type_definition: () => DEFINITIONS[CART_ITEM], line_labels: () => [], read_board: () => file(many) } });
    const first = await paged.call(readPage, { board: BOARD.id, project: "ts-orders" });
    expect(first.text).toContain('Showing 120 of 131. Next page: read_page with cursor "120".');
    expect(first.text).not.toContain("Next: check");
    const rest = await paged.call(readPage, { board: BOARD.id, project: "ts-orders", cursor: "120" });
    expect(rest.text).toContain("- CartItem (card id c129)");
    expect(rest.text).not.toContain("Next page");
    expect(rest.text.split("\n").at(-1)).toBe("Next: check reviews the page; pass its rev as expectRevision to an edit that builds on what you read.");
  });

  it("gives the page as Mermaid, exported by the window that holds the board", async () => {
    const test = app(held);
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], tabs: [BOARD] });
    const { text } = await test.call(readPage, { format: "mermaid" });
    expect(text.split("\n").slice(1)).toEqual([
      "The page as Mermaid, as last saved (the window's unsaved edits aren't in it):",
      "```mermaid",
      "flowchart LR",
      "  c0[createOrder]",
      "```",
    ]);
    expect(test.windowCalls).toEqual([[1, "export_mermaid_text", { project: PROJECT, board: { id: BOARD.id, page: "page-1" } }]]);
  });

  it("says which board it means when none is in view", async () => {
    const { text, isError } = await app().call(readPage);
    expect(isError).toBe(true);
    expect(text).toBe(
      "Can't tell which board you mean: no board is in view in Unfold, and none was named. Pass board (an id from list_boards) or a link the person pasted, or ask them to open the board.",
    );
  });
});

function summary(id: string, title: string, overrides: Partial<BoardSummary> = {}): BoardSummary {
  return { project: PROJECT, id, place: "draft", name: id, title, folder: null, pages: 2, cards: 6, projects: [], changes: 0, updatedAtMs: 0, heldBy: null, label: null, error: null, ...overrides };
}

describe("list_boards", () => {
  it("lists a project's boards with their pages, publish state, health, and whether one is open or in view", async () => {
    const test = harness({
      engine: {
        list_boards: () => ({
          project: PROJECT,
          boards: [
            summary(".unfold/boards/orders.json", "Orders", { place: "published", changes: 2, folder: "flows", label: "1 missing · 2 outdated", heldBy: 1 }),
            summary(BOARD.id, "Order placement"),
            summary(".unfold/boards/broken.json", "Broken", { place: "published", error: "the file isn't a board" }),
          ],
          emptyFolders: ["old"],
        }),
        read_board: (args) => file({ ...PAGE, title: args["id"] === BOARD.id ? "Placing an order" : "Overview" }),
      },
    });
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], tabs: [BOARD] });
    const { text } = await test.call(listBoards);
    expect(text.split("\n")).toEqual([
      "Unfold · ts-orders · as Claude",
      `ts-orders (project id ${PROJECT}) has 3 boards:`,
      "- Orders (board id .unfold/boards/orders.json): published, 2 unpublished changes, in flows, 6 cards, needs review: 1 missing · 2 outdated (check lists it), open in Unfold",
      "  pages: Overview (page id page-1), Async path (page id page-2)",
      `- Order placement (board id ${BOARD.id}): draft, 6 cards, in view`,
      "  pages: Placing an order (page id page-1), Async path (page id page-2)",
      "- Broken (board id .unfold/boards/broken.json): published, 6 cards, can't be read: the file isn't a board",
      "  2 pages",
      "Empty folders: old.",
      "Next: read_page with a board's and a page's id; check with a board's id lists its problems.",
    ]);
  });

  it("lists every project's drafts and unpublished changes when no project is named or in view", async () => {
    const test = harness({
      projects: [projectRow(PROJECT, "ts-orders"), projectRow("p-pay", "payments")],
      engine: {
        drafts: () => ({ drafts: [summary(".unfold/local/drafts/pay.json", "Payments", { project: "p-pay" })] }),
        read_board: () => file(),
      },
    });
    const { text } = await test.call(listBoards);
    expect(text.split("\n")).toEqual([
      "Unfold · as Claude",
      "2 projects and no board in view, so these are every project's drafts and published boards with unpublished changes, newest first (1):",
      "- Payments (board id .unfold/local/drafts/pay.json): in payments, draft, 6 cards",
      "  pages: Placing an order (page id page-1), Async path (page id page-2)",
      "Next: list_boards with project lists all of one project's boards.",
    ]);
    expect(test.commands()).not.toContain("list_boards");
  });
});
