import { describe, expect, it, vi } from "vitest";
import cardCreateOrder from "../../../../../testdata/workspace/commands/card_create_order.json";
import cardPlaceAnOrder from "../../../../../testdata/workspace/commands/card_place_an_order.json";
import flowTextCreateOrder from "../../../../../testdata/workspace/commands/flow_text_create_order.json";
import type { AgentPage, AgentShellEvent } from "../../../shared/agent-api";
import { pageRevision } from "../../agents/revision";
import type { BoardSummary, CardView, FlowText, NodeKind, Page, ReadBoardResult, TypeDefinition } from "../engine-types";
import { listBoards, readPage } from "./pages";
import { harness, PROJECT, projectRow, recorded } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const CREATE_ORDER = "src/routes/orders.ts#createOrder";
const PLACE = "src/services/order-service.ts#OrderService.place";
const ENTRANCE = "entrance:server/place-an-order";
const CART = "src/cart.ts#Cart";
const CART_ITEM = "src/cart.ts#CartItem";
const BASKET = "src/cart.ts#Basket";
const PLACE_CALL = `${CREATE_ORDER}/call:orderService.place`;
const BOARD = { project: PROJECT, id: ".unfold/boards/order-placement.json" };
const BOARD_INFO = { title: "Order placement", revision: "r1" } as const;

const PAGE: AgentPage = {
  id: "page-1",
  title: "Placing an order",
  defaultView: "compact",
  cards: [
    { id: "post-orders", ref: { entrance: ENTRANCE }, at: [0, 0] },
    { id: "create-order", ref: { block: CREATE_ORDER }, at: [320, 0], view: "expanded", description: { text: "Places the order now, or queues it.", author: "agent:claude", signature: "s" } },
    { id: "place", ref: { block: PLACE }, at: [800, 0] },
    { id: "cart", ref: { block: CART }, at: [0, 400] },
    { id: "cart-item", ref: { block: CART_ITEM }, at: [400, 400] },
  ],
  names: [{ key: `${CREATE_ORDER}/if[0]`, text: "Asked to queue it?", author: "person:lazar", signature: "s" }],
  combined: [{ id: "queue-it", title: "Queue it for a worker", members: [`${CREATE_ORDER}/if[0]`], collapsed: true, author: "agent:claude", signature: "s" }],
  guards: [],
  links: [
    {
      id: "save-order",
      from: "create-order",
      to: "orders",
      kind: "writes",
      step: PLACE_CALL,
      anchors: [{ key: "src/db/repository.ts#OrderRepository" }],
      author: "agent:codex",
      label: { text: "the order", author: "agent:codex", signature: "s" },
    },
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

/** A definition's card that draws nothing but its header. */
function definitionCard(key: string, kind: NodeKind, form: "method" | "interface" | "class", label: string, line: number): CardView {
  return {
    type: "definition",
    flow: {
      header: { key, kind, form, label, path: key.slice(0, key.indexOf("#")), line, async: false, generator: false, definedWith: [], imports: [], actions: { after: false, start: true, open: false } },
      combinable: [],
      steps: { steps: [] },
      routes: [],
    },
  };
}

const CART_DEFINITION: TypeDefinition = {
  key: CART,
  kind: "interface",
  name: "Cart",
  path: "src/cart.ts",
  line: 12,
  language: "typescript",
  members: [
    { name: "items", kind: "property", optional: false, readonly: false, default: false, type: { text: "CartItem[]", spans: [{ start: 0, end: 8, to: { outcome: "project", key: CART_ITEM } }] } },
    { name: "owner", kind: "property", optional: false, readonly: false, default: false, type: { text: "string", spans: [{ start: 0, end: 6, to: { outcome: "builtin" } }] } },
  ],
  inherited: [],
  uses: 3,
};

const DEFINITIONS: Readonly<Record<string, TypeDefinition>> = {
  [CART]: CART_DEFINITION,
  [CART_ITEM]: { key: CART_ITEM, kind: "interface", name: "CartItem", path: "src/cart.ts", line: 6, language: "typescript", members: [], inherited: [], uses: 1 },
  [BASKET]: { ...CART_DEFINITION, key: BASKET, kind: "class", name: "Basket", line: 20, members: CART_DEFINITION.members.slice(0, 1) },
};

const CARDS: Readonly<Record<string, () => CardView>> = {
  [CREATE_ORDER]: () => recorded<CardView>(cardCreateOrder),
  [ENTRANCE]: () => recorded<CardView>(cardPlaceAnOrder),
  [PLACE]: () => definitionCard(PLACE, "function", "method", "OrderService.place", 13),
  [CART]: () => definitionCard(CART, "type", "interface", "Cart", 12),
  [CART_ITEM]: () => definitionCard(CART_ITEM, "type", "interface", "CartItem", 6),
  [BASKET]: () => definitionCard(BASKET, "class", "class", "Basket", 20),
};

function card(args: Readonly<Record<string, unknown>>): CardView {
  const answer = CARDS[String(args["key"])];
  if (!answer) {
    throw new Error(`no card ${String(args["key"])}`);
  }
  return answer();
}

function file(page: AgentPage = PAGE): ReadBoardResult {
  return {
    project: PROJECT,
    id: BOARD.id,
    name: "order-placement",
    document: { format: "unfold-board", version: 1, title: "Order placement", pages: [page as unknown as Page, { ...(page as unknown as Page), id: "page-2", title: "Async path" }] },
    revision: "r1",
  };
}

function app(window?: (request: AgentShellEvent) => unknown) {
  return harness({
    engine: {
      card,
      flow_text: () => recorded<FlowText>(flowTextCreateOrder),
      type_definition: (args) => DEFINITIONS[String(args["key"])],
      read_board: () => file(),
    },
    ...(window ? { window } : {}),
  });
}

const held = (request: AgentShellEvent) =>
  request.type === "agent_read" ? { ok: true, page: PAGE, board: { ...BOARD_INFO, unsaved: true }, sizes: { "create-order": { width: 520.4, height: 310 } } } : request.type === "agent_open_board" ? { ok: true } : undefined;

describe("read_page", () => {
  it("reads everything on a page from the window that holds it, with ids, and what the board draws between cards", async () => {
    const test = app(held);
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
    const { text, isError } = await test.call(readPage);
    expect(isError).toBe(false);
    const lines = text.split("\n");
    expect(lines.slice(0, 2)).toEqual([
      `Unfold · ts-orders › Order placement › Placing an order · rev ${pageRevision(PAGE)} · as Claude`,
      `Ids: project ${PROJECT}, board ${BOARD.id}, page page-1. Cards without their own setting are compact. The window has edits it hasn't saved yet; they're included.`,
    ]);
    expect(lines).toContain("Cards (5):");
    expect(lines).toContain(`- Place an order (card id post-orders): entrance of Server; key ${ENTRANCE}; compact (the page's default); at [0, 0]; in frame server`);
    const createOrder = lines.indexOf(
      `- createOrder (card id create-order): function at src/routes/orders.ts:25; key ${CREATE_ORDER}; expanded; at [320, 0], 520×310; in frame server; description by you: “Places the order now, or queues it.”`,
    );
    expect(createOrder).toBeGreaterThan(0);
    // An expanded card's drawn flow, as flow_text prints it, shaped by the page's overlay and the card's open paths.
    expect(lines[createOrder + 1]).toBe(`    ( createOrder )   key=${CREATE_ORDER}`);
    expect(test.engineCalls.find(([command]) => command === "flow_text")?.[1]).toEqual({
      project: PROJECT,
      key: CREATE_ORDER,
      overlay: { names: [{ key: `${CREATE_ORDER}/if[0]`, text: "Asked to queue it?" }], combined: [{ id: "queue-it", title: "Queue it for a worker", members: [`${CREATE_ORDER}/if[0]`], collapsed: true }], proposed: [] },
      open: [],
    });
    expect(test.engineCalls.filter(([command]) => command === "flow_text")).toHaveLength(1);
    expect(lines).toContain(`- OrderService.place (card id place): method at src/services/order-service.ts:13; key ${PLACE}; compact (the page's default); at [800, 0]; in frame checks`);
    expect(lines).toContain(`- Cart (card id cart): interface at src/cart.ts:12; key ${CART}; fields; at [0, 400]`);
    expect(lines).toContain(`- “Asked to queue it?” on ${CREATE_ORDER}/if[0], by Lazar (a person)`);
    expect(lines).toContain(`- “Queue it for a worker” (combined step id queue-it): combines ${CREATE_ORDER}/if[0]; collapsed; titled by you`);
    expect(lines).toContain("Frames (2), nested as they are:");
    expect(lines).toContain("- server (frame id server; box, api; by you): holds Place an order (post-orders), createOrder (create-order)");
    expect(lines).toContain("  - Shared (frame id checks; box, collapsed; by Lazar (a person)): holds OrderService.place (place); description by Lazar (a person): “Code both services call.”");
    expect(lines).toContain(
      "- table “orders” (resource id orders): at [800, 200]; columns id (primary), customer_id text (foreign → customers.id); anchors db/schema.sql:3–9; by you",
    );
    expect(lines).toContain(
      `- writes connection createOrder (create-order) → orders (orders) (link id save-order): from the call ${PLACE_CALL}; anchors OrderRepository (key src/db/repository.ts#OrderRepository); by Codex (an agent); label by Codex (an agent): “the order”`,
    );
    expect(lines).toContain("- note note-1, attached to createOrder (create-order) at offset [0, -40], by you: “The client gets a job id, not the order.”");
    expect(lines).toContain("- Place an order (post-orders) → createOrder (create-order): starts at createOrder");
    expect(lines).toContain(`- createOrder (create-order) → OrderService.place (place): calls orderService.place (call key ${PLACE_CALL})`);
    expect(lines).toContain(`- Cart (cart) → CartItem (cart-item): field items: CartItem[] (CartItem: ${CART_ITEM})`);
    expect(test.commands()).not.toContain("read_board");
  });

  it("draws type arrows only from type cards showing their fields, and to the first card of a key, as the canvas does", async () => {
    const BASE = "src/cart.ts#Base";
    const withBase: TypeDefinition = {
      ...CART_DEFINITION,
      inherited: [
        {
          from: { text: "Base<CartItem>", spans: [{ start: 0, end: 4, to: { outcome: "project", key: BASE } }, { start: 5, end: 13, to: { outcome: "project", key: CART_ITEM } }] },
          members: [{ name: "extra", kind: "property", optional: false, readonly: false, default: false, type: { text: "CartItem", spans: [{ start: 0, end: 8, to: { outcome: "project", key: CART_ITEM } }] } }],
        },
      ],
    };
    const typeLines = async (cards: AgentPage["cards"]) => {
      const page: AgentPage = { ...PAGE, cards, names: [], combined: [], links: [], frames: [], resources: [], notes: [] };
      const test = harness({ engine: { card, flow_text: () => ({ text: "", labels: [] }), type_definition: (args) => (args["key"] === CART ? withBase : DEFINITIONS[String(args["key"])]), read_board: () => file(page) } });
      const { text } = await test.call(readPage, { board: BOARD.id, project: "ts-orders" });
      return { lines: text.split("\n").filter((line) => line.includes(": field ")), definitions: test.commands().filter((command) => command === "type_definition").length };
    };
    const fields = [
      `- Cart (cart) → CartItem (item): field items: CartItem[] (CartItem: ${CART_ITEM})`,
      `- Cart (cart) → CartItem (item): field from Base<CartItem>: extra: CartItem (CartItem: ${CART_ITEM})`,
    ];
    // Base has no card, so Cart lists the fields it inherits, with their arrows; the second CartItem card gets none.
    expect(await typeLines([{ id: "cart", ref: { block: CART }, at: [0, 0] }, { id: "item", ref: { block: CART_ITEM }, at: [400, 0] }, { id: "item-2", ref: { block: CART_ITEM }, at: [800, 0] }])).toEqual({
      lines: fields,
      definitions: 3,
    });
    // A type card keeps its fields, and their arrows, in every view.
    expect(await typeLines([{ id: "cart", ref: { block: CART }, at: [0, 0], view: "expanded" }, { id: "item", ref: { block: CART_ITEM }, at: [400, 0], view: "expanded" }])).toEqual({ lines: fields, definitions: 2 });
    // A class shows its fields only while compact: expanded, it shows its class-level code.
    const basket = async (view: "compact" | "expanded") => typeLines([{ id: "basket", ref: { block: BASKET }, at: [0, 0], view }, { id: "item", ref: { block: CART_ITEM }, at: [400, 0] }]);
    expect(await basket("compact")).toEqual({ lines: [`- Basket (basket) → CartItem (item): field items: CartItem[] (CartItem: ${CART_ITEM})`], definitions: 2 });
    expect(await basket("expanded")).toEqual({ lines: [], definitions: 1 });
  });

  it("lists each card's view and calls open in place, and proposed steps with their anchors' health", async () => {
    const page: AgentPage = {
      ...PAGE,
      defaultView: "expanded",
      cards: [
        { id: "create-order", ref: { block: CREATE_ORDER }, at: [0, 0], open: [[PLACE_CALL], [PLACE_CALL, `${PLACE}/call:validateOrder`]] },
        { id: "place", ref: { block: PLACE }, at: [400, 0], view: "compact" },
      ],
      names: [],
      combined: [],
      links: [],
      frames: [],
      resources: [],
      notes: [],
      proposals: [
        { id: "email", after: PLACE_CALL, title: "Email a confirmation", reason: "The queued path sends one.", author: "person:maya" },
        { id: "audit", start: PLACE, title: "Audit the order", author: "agent:claude" },
      ],
    };
    const review = "The block this proposal goes after changed kind or owner; check where it goes, then accept the current code, reconnect it, or remove the proposal";
    const test = harness({
      engine: {
        card,
        flow_text: () => recorded<FlowText>(flowTextCreateOrder),
        board_health: () => ({
          health: { items: [{ page: "page-1", kind: "review", target: { proposal: "audit" }, message: review, repairs: ["accept", "reconnect", "remove"] }], missing: 0, outdated: 0, review: 1, brokenCombined: 0, brokenLinks: 0 },
          label: null,
        }),
      },
      window: (request) => (request.type === "agent_read" ? { ok: true, page, board: { ...BOARD_INFO, unsaved: false }, sizes: {} } : request.type === "agent_open_board" ? { ok: true } : undefined),
    });
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
    const lines = (await test.call(readPage)).text.split("\n");
    expect(lines[1]).toContain("Cards without their own setting are expanded.");
    expect(lines).toContain(
      `- createOrder (card id create-order): function at src/routes/orders.ts:25; key ${CREATE_ORDER}; expanded (the page's default), calls open in place [${PLACE_CALL}], [${PLACE_CALL} › ${PLACE}/call:validateOrder]; at [0, 0]`,
    );
    expect(lines).toContain(`- OrderService.place (card id place): method at src/services/order-service.ts:13; key ${PLACE}; compact; at [400, 0]`);
    expect(test.engineCalls.find(([command]) => command === "card")?.[1]).toMatchObject({
      overlay: { proposed: [{ id: "email", title: "Email a confirmation", after: PLACE_CALL }, { id: "audit", title: "Audit the order", start: PLACE }] },
      open: [[PLACE_CALL], [PLACE_CALL, `${PLACE}/call:validateOrder`]],
    });
    const at = lines.indexOf("Proposed steps, intentions, not code (2):");
    expect(lines.slice(at + 1, at + 3)).toEqual([
      `- “Email a confirmation” (proposal id email): after ${PLACE_CALL}; by Maya (a person); reason “The queued path sends one.”`,
      `- “Audit the order” (proposal id audit): at the start of ${PLACE}; by you; anchor held for review: ${review} (propose with its id and a new spot moves it; remove_items takes it off)`,
    ]);
    expect(test.windowCalls).toEqual([[1, "board_health", { project: PROJECT, id: BOARD.id }]]);
  });

  it("reads a board no window holds from its file, a board's first page by default, and pages long pages", async () => {
    const test = app();
    const { text } = await test.call(readPage, { board: BOARD.id, project: "ts-orders" });
    expect(text.split("\n")[0]).toBe(`Unfold · ts-orders › Order placement › Placing an order · rev ${pageRevision(PAGE)} · as Claude`);
    expect(text).not.toContain("520×310");
    const second = await test.call(readPage, { page: `unfold://ts-orders/${BOARD.id}?page=page-2` });
    expect(second.text.split("\n")[0]).toContain("› Async path ·");

    const many: AgentPage = { ...PAGE, cards: Array.from({ length: 130 }, (_, index) => ({ id: `c${index}`, ref: { block: CART_ITEM }, at: [index, 0] })), names: [], combined: [], links: [], frames: [], resources: [], notes: [] };
    const paged = harness({ engine: { card, type_definition: () => DEFINITIONS[CART_ITEM], read_board: () => file(many) } });
    const first = await paged.call(readPage, { board: BOARD.id, project: "ts-orders" });
    expect(first.text).toContain('Showing 120 of 131. Next page: read_page with cursor "120".');
    expect(first.text).not.toContain("Next: check");
    const rest = await paged.call(readPage, { board: BOARD.id, project: "ts-orders", cursor: "120" });
    expect(rest.text).toContain("- CartItem (card id c129)");
    expect(rest.text).not.toContain("Next page");
    expect(rest.text.split("\n").at(-1)).toBe("Next: check reviews the page; pass its rev as expectRevision to an edit that builds on what you read.");
  });

  it("pages a page of long lines before the size guard would cut it, so every line stays reachable", async () => {
    const description = { text: "d".repeat(300), author: "agent:claude", signature: "s" };
    const ids = Array.from({ length: 130 }, (_, index) => `c${index}`);
    const many: AgentPage = { ...PAGE, cards: ids.map((id, index) => ({ id, ref: { block: CART_ITEM }, at: [index, 0], description })), names: [], combined: [], links: [], frames: [], resources: [], notes: [] };
    const paged = harness({ engine: { card, type_definition: () => DEFINITIONS[CART_ITEM], read_board: () => file(many) } });
    const seen: string[] = [];
    let cursor: string | undefined;
    do {
      const { text } = await paged.call(readPage, { board: BOARD.id, project: "ts-orders", ...(cursor !== undefined ? { cursor } : {}) });
      expect(text).not.toContain("Cut here");
      seen.push(...text.split("\n").flatMap((line) => /^- CartItem \(card id (c\d+)\)/.exec(line)?.[1] ?? []));
      cursor = /Next page: read_page with cursor "(\d+)"/.exec(text)?.[1];
    } while (cursor !== undefined);
    expect(seen).toEqual(ids);
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
  return { project: PROJECT, id, name: id, title, folder: null, pages: 2, cards: 6, updatedAtMs: 0, label: null, error: null, ...overrides };
}

describe("list_boards", () => {
  it("lists a project's boards, last edited first, with their pages, folders, health, and whether one is open or in view", async () => {
    const test = harness({
      engine: {
        list_boards: () => ({
          project: PROJECT,
          boards: [
            summary(".unfold/boards/broken.json", "Broken", { updatedAtMs: Date.UTC(2026, 9, 11), error: "the file isn't a board" }),
            summary(".unfold/boards/orders.json", "Orders", { updatedAtMs: Date.UTC(2026, 9, 13), folder: "flows", label: "1 missing · 2 outdated" }),
            summary(BOARD.id, "Order placement", { updatedAtMs: Date.UTC(2026, 9, 12) }),
          ],
          emptyFolders: ["old"],
        }),
        read_board: (args) => file({ ...PAGE, title: args["id"] === BOARD.id ? "Placing an order" : "Overview" }),
      },
    });
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD, { project: PROJECT, id: ".unfold/boards/orders.json" }] });
    const { text } = await test.call(listBoards);
    expect(text.split("\n")).toEqual([
      "Unfold · ts-orders · as Claude",
      `ts-orders (project id ${PROJECT}) has 3 boards:`,
      "- Orders (board id .unfold/boards/orders.json): in flows, edited 2026-10-13T00:00:00.000Z, 6 cards, needs review: 1 missing · 2 outdated (check lists it), open in Unfold",
      "  pages: Overview (page id page-1), Async path (page id page-2)",
      `- Order placement (board id ${BOARD.id}): edited 2026-10-12T00:00:00.000Z, 6 cards, in view`,
      "  pages: Placing an order (page id page-1), Async path (page id page-2)",
      "- Broken (board id .unfold/boards/broken.json): edited 2026-10-11T00:00:00.000Z, 6 cards, can't be read: the file isn't a board",
      "  2 pages",
      "Empty folders: old.",
      "Next: read_page with a board's and a page's id; check with a board's id lists its problems.",
    ]);
  });

  it("asks which project when several are open and no board is in view", async () => {
    const test = harness({ projects: [projectRow(PROJECT, "ts-orders"), projectRow("p-pay", "payments")] });
    const { text } = await test.call(listBoards);
    expect(text).toBe("Can't tell which project you mean: the workspace has 2 projects and no board is in view. Pass project: a name or id from list_projects.");
    expect(test.commands()).not.toContain("list_boards");
  });
});
