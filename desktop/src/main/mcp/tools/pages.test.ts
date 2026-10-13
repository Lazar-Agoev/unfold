import { describe, expect, it, vi } from "vitest";
import cardCreateOrder from "../../../../../testdata/workspace/commands/card_create_order.json";
import type { AgentPage, AgentShellEvent } from "../../../shared/agent-api";
import { pageRevision } from "../../agents/revision";
import type { BoardSummary, CardView, LabeledLine, LineLabel, Page, ReadBoardResult, TypeDefinition } from "../engine-types";
import { listBoards, readPage } from "./pages";
import { harness, PROJECT, projectRow, recorded } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const CREATE_ORDER = "src/routes/orders.ts#function:createOrder";
const PLACE = "src/services/order-service.ts#class:OrderService/method:place";
const ENTRANCE = 'entrance:["express.route","http","POST","/orders","src/routes/orders.ts"]';
const CART = "src/cart.ts#interface:Cart";
const CART_ITEM = "src/cart.ts#interface:CartItem";
const BASKET = "src/cart.ts#class:Basket";
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

const CART_DEFINITION: TypeDefinition = {
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
};

const DEFINITIONS: Readonly<Record<string, TypeDefinition>> = {
  [CART]: CART_DEFINITION,
  [CART_ITEM]: { key: CART_ITEM, kind: "interface", name: "CartItem", path: "src/cart.ts", line: 6, language: "typescript", members: [], inherited: [], uses: 1 },
  [BASKET]: { ...CART_DEFINITION, key: BASKET, kind: "class", name: "Basket", line: 20, members: CART_DEFINITION.members.slice(0, 1) },
};

const ORDER_LABEL: LineLabel = { inputs: [{ type: { text: "OrderDraft", spans: [] } }], inputsUnknown: false, output: { text: "Promise<Order>", spans: [] } };

const LABEL: LabeledLine = { source: `${CREATE_ORDER}/variable:order/call[0]`, seq: 0, label: ORDER_LABEL };

const CARDS: Readonly<Record<string, () => CardView>> = {
  [CREATE_ORDER]: () => recorded<CardView>(cardCreateOrder),
  [ENTRANCE]: () => ENTRANCE_CARD,
  [PLACE]: () => blockCard(PLACE, "method", "OrderService.place", 13),
  [CART]: () => blockCard(CART, "interface", "Cart", 12),
  [CART_ITEM]: () => blockCard(CART_ITEM, "interface", "CartItem", 6),
  [BASKET]: () => blockCard(BASKET, "class", "Basket", 20),
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
      `Ids: project ${PROJECT}, board ${BOARD.id}, page page-1. The page opens in Diagram; cards without their own setting are compact. The window has edits it hasn't saved yet; they're included.`,
    ]);
    expect(lines).toContain("Cards (6):");
    expect(lines).toContain(`- POST /orders (card id post-orders): http entrance; key ${ENTRANCE}; compact (the page's default); at [0, 0]; in frame server`);
    expect(lines).toContain(
      `- createOrder (card id create-order): function at src/routes/orders.ts:25; key ${CREATE_ORDER}; expanded; at [320, 0], 520×310; in frame server; description by you: “Places the order now, or queues it.”`,
    );
    expect(lines).toContain(`- OrderService.place (card id place): method at src/services/order-service.ts:13; key ${PLACE}; compact (the page's default); folded; at [800, 0]; in frame checks`);
    expect(lines).toContain(`- Cart (card id cart): interface at src/cart.ts:12; key ${CART}; fields; at [0, 400]`);
    expect(lines).toContain(
      "- charge (card id charge): key src/pay.ts#function:charge; from project payments; compact (the page's default); at [800, 400]; code unavailable: its project payments isn't in the workspace",
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
    expect(lines).toContain("Flow steps still unlabeled, in expanded cards (label_steps labels them) (5):");
    expect(lines).toContain(`- createOrder (create-order): line 27 condition “req.header?” (key ${CREATE_ORDER}/if[0])`);
    expect(lines.some((line) => line.includes("/variable:draft)"))).toBe(false);
    // The collapsed step group hides its steps.
    expect(lines.some((line) => line.includes("/variable:job)") || line.includes("/return[0])"))).toBe(false);
    expect(test.commands()).not.toContain("read_board");
  });

  it("counts unlabeled steps as the outline does: not Start or End, nor the steps a collapsed step group of the card hides", async () => {
    const steps = async (groups: AgentPage["groups"]) => {
      const page: AgentPage = { ...PAGE, groups };
      const test = app((request) => (request.type === "agent_read" ? { ok: true, page, board: BOARD_INFO, sizes: {} } : undefined));
      test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], tabs: [BOARD] });
      const { text } = await test.call(readPage);
      return text
        .split("\n")
        .flatMap((line) => (line.startsWith("- createOrder (create-order): ") ? [line.slice(line.indexOf("(key ") + 5 + CREATE_ORDER.length, -1)] : []));
    };
    const group = (id: string, card: string, members: string[], collapsed: boolean) => ({ id, card, title: id, members: members.map((member) => `${CREATE_ORDER}/${member}`), collapsed });
    // A collapsed group hides its members and the steps under them; the Return outside it still counts.
    expect(await steps([group("queue", "create-order", ["if[0]/variable:job", "if[0]/expression_statement[0]"], true)])).toEqual([
      "/if[0]",
      "/if[0]/return[0]",
      "/variable:order",
      "/expression_statement[0]",
      "/expression_statement[1]",
    ]);
    expect(await steps([group("branch", "create-order", ["if[0]"], true)])).toEqual(["/variable:order", "/expression_statement[0]", "/expression_statement[1]"]);
    // An expanded group, another card's collapsed group, and a key that only shares a prefix hide nothing.
    expect(
      await steps([group("open", "create-order", ["if[0]"], false), group("elsewhere", "place", ["variable:order"], true), group("prefix", "create-order", ["expression_statement"], true)]),
    ).toHaveLength(7);
  });

  it("draws type arrows only from type cards showing their fields, and to the first card of a key, as the canvas does", async () => {
    const BASE = "src/cart.ts#interface:Base";
    const withBase: TypeDefinition = {
      ...CART_DEFINITION,
      inherited: [
        {
          from: { text: "Base<CartItem>", spans: [{ start: 0, end: 4, style: "declared", target: BASE }, { start: 5, end: 13, style: "declared", target: CART_ITEM }] },
          members: [{ name: "extra", kind: "property", optional: false, readonly: false, default: false, type: { text: "CartItem", spans: [{ start: 0, end: 8, style: "declared", target: CART_ITEM }] } }],
        },
      ],
    };
    const typeLines = async (cards: AgentPage["cards"]) => {
      const page: AgentPage = { ...PAGE, cards, groups: [], links: [], frames: [], resources: [], notes: [] };
      const test = harness({ engine: { card: (args) => CARDS[String(args["key"])]?.() ?? null, type_definition: (args) => (args["key"] === CART ? withBase : DEFINITIONS[String(args["key"])]), line_labels: () => [], read_board: () => file(page) } });
      const { text } = await test.call(readPage, { board: BOARD.id, project: "ts-orders" });
      return { lines: text.split("\n").filter((line) => line.includes(": field ")), definitions: test.commands().filter((command) => command === "type_definition").length };
    };
    // Base has no card, so Cart lists the fields it inherits, with their arrows; the second CartItem card gets none.
    expect(await typeLines([{ id: "cart", ref: { block: CART }, at: [0, 0] }, { id: "item", ref: { block: CART_ITEM }, at: [400, 0] }, { id: "item-2", ref: { block: CART_ITEM }, at: [800, 0] }])).toEqual({
      lines: ["- Cart (cart) → CartItem (item): field items: CartItem[] (CartItem: src/cart.ts#interface:CartItem)", "- Cart (cart) → CartItem (item): field from Base<CartItem>: extra: CartItem (CartItem: src/cart.ts#interface:CartItem)"],
      definitions: 3,
    });
    // A type card keeps its fields, and their arrows, in every view.
    expect(await typeLines([{ id: "cart", ref: { block: CART }, at: [0, 0], view: "code" }, { id: "item", ref: { block: CART_ITEM }, at: [400, 0], view: "code" }])).toEqual({
      lines: ["- Cart (cart) → CartItem (item): field items: CartItem[] (CartItem: src/cart.ts#interface:CartItem)", "- Cart (cart) → CartItem (item): field from Base<CartItem>: extra: CartItem (CartItem: src/cart.ts#interface:CartItem)"],
      definitions: 2,
    });
    // A class shows its fields only while compact: expanded (a saved Flow, or a legacy Code view), it shows its detail.
    const basket = async (view: "diagram" | "flow" | "code") => typeLines([{ id: "basket", ref: { block: BASKET }, at: [0, 0], view }, { id: "item", ref: { block: CART_ITEM }, at: [400, 0] }]);
    expect(await basket("diagram")).toEqual({ lines: ["- Basket (basket) → CartItem (item): field items: CartItem[] (CartItem: src/cart.ts#interface:CartItem)"], definitions: 2 });
    expect(await basket("flow")).toEqual({ lines: [], definitions: 1 });
    expect(await basket("code")).toEqual({ lines: [], definitions: 1 });
  });

  it("lists each card's expansion and calls expanded in place, and proposed steps under their cards with their anchors' health", async () => {
    const CALL = `${CREATE_ORDER}/variable:order/call[0]`;
    const page: AgentPage = {
      ...PAGE,
      defaultView: "code",
      cards: [
        { id: "create-order", ref: { block: CREATE_ORDER }, at: [0, 0], view: "flow", expanded: [[CALL], [CALL, `${PLACE}/call[0]`]] },
        { id: "place", ref: { block: PLACE }, at: [400, 0], view: "diagram" },
      ],
      groups: [],
      links: [],
      frames: [],
      resources: [],
      notes: [],
      labels: [{ key: `${CREATE_ORDER}/variable:order`, text: "Place the order now", author: "agent:claude", signature: "s" }],
      proposals: [
        { id: "email", card: "create-order", after: `${CREATE_ORDER}/variable:order`, title: "Email a confirmation", reason: "The queued path sends one.", author: "person:maya" },
        { id: "notify", card: "create-order", after: `${CREATE_ORDER}/if[0]`, path: "yes", title: "Notify the customer", author: "agent:claude" },
        { id: "retry", card: "create-order", after: `${CREATE_ORDER}/if[1]`, title: "Retry", author: "agent:claude" },
        { id: "audit", card: "place", after: `${PLACE}/call[0]`, title: "Audit the order", author: "agent:claude" },
      ],
    };
    const review = "The step this proposal goes after changed kind or owner; check where it goes, then accept the current code, reconnect it to another step, or remove the proposal";
    const test = harness({
      engine: {
        card: (args) => CARDS[String(args["key"])]?.() ?? null,
        line_labels: () => [],
        board_health: () => ({
          health: { items: [{ page: "page-1", kind: "review", target: { proposal: "audit" }, message: review, repairs: ["accept", "reconnect", "removeProposal"] }], missing: 0, outdated: 0, brokenGroups: 0, review: 1, notConnected: 0, brokenLinks: 0 },
          outlines: [],
          label: null,
        }),
      },
      window: (request) => (request.type === "agent_read" ? { ok: true, page, board: { ...BOARD_INFO, unsaved: false }, sizes: {} } : request.type === "agent_open_board" ? { ok: true } : undefined),
    });
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], tabs: [BOARD] });
    const lines = (await test.call(readPage)).text.split("\n");
    expect(lines[1]).toContain("The page opens in Code, every card's source; in Diagram, cards without their own setting are expanded.");
    expect(lines).toContain(`- createOrder (card id create-order): function at src/routes/orders.ts:25; key ${CREATE_ORDER}; expanded, calls expanded in place [${CALL}], [${CALL} › ${PLACE}/call[0]]; at [0, 0]`);
    expect(lines).toContain(`- OrderService.place (card id place): method at src/services/order-service.ts:13; key ${PLACE}; compact; at [400, 0]`);
    const at = lines.indexOf("Proposed steps (4), by card: intentions, not code:");
    expect(lines.slice(at + 1, at + 7)).toEqual([
      "- createOrder (create-order):",
      `  - “Email a confirmation” (proposal id email): after the step “Place the order now” (call; line 32; key ${CREATE_ORDER}/variable:order); by Maya (a person); reason “The queued path sends one.”`,
      `  - “Notify the customer” (proposal id notify): after the step “req.header?” (condition; line 27; key ${CREATE_ORDER}/if[0]), on its “yes” path; by you`,
      `  - “Retry” (proposal id retry): after the step ${CREATE_ORDER}/if[1]; by you; anchor missing: no step of its card's code has this key now (propose with its id and a new after moves it; remove_items takes it off)`,
      "- OrderService.place (place):",
      `  - “Audit the order” (proposal id audit): after the step ${PLACE}/call[0]; by you; anchor held for review: ${review} (propose with its id and a new after moves it; remove_items takes it off)`,
    ]);
    // In the Code lens nothing is expanded, so no step counts as unlabeled.
    expect(lines.some((line) => line.startsWith("Flow steps still unlabeled"))).toBe(false);
    expect(test.windowCalls).toEqual([[1, "board_health", { project: PROJECT, id: BOARD.id }]]);
  });

  it("labels each arrow with its own project's line label when two projects share keys", async () => {
    const cards: AgentPage["cards"] = [
      { id: "create-order", ref: { block: CREATE_ORDER }, at: [0, 0] },
      { id: "place", ref: { block: PLACE }, at: [400, 0] },
      { id: "create-order-b", ref: { block: CREATE_ORDER, project: "payments" }, at: [0, 400] },
      { id: "place-b", ref: { block: PLACE, project: "payments" }, at: [400, 400] },
    ];
    const page: AgentPage = { ...PAGE, cards, groups: [], links: [], frames: [], resources: [], notes: [] };
    const receipt: LabeledLine = { ...LABEL, label: { ...ORDER_LABEL, output: { text: "Promise<Receipt>", spans: [] } } };
    const test = harness({
      projects: [projectRow(PROJECT, "ts-orders"), projectRow("p-pay", "payments")],
      engine: {
        card: (args) => CARDS[String(args["key"])]?.() ?? null,
        line_labels: (args) => (args["project"] === PROJECT ? [LABEL] : [receipt]),
        read_board: () => file(page),
      },
    });
    const { text } = await test.call(readPage, { board: BOARD.id, project: "ts-orders" });
    const source = `source ${CREATE_ORDER}/variable:order/call[0], seq 0`;
    expect(text).toContain(`- createOrder (create-order) → OrderService.place (place): calls orderService.place at line 32: (OrderDraft) → Promise<Order>; ${source}`);
    expect(text).toContain(`- createOrder (create-order-b) → OrderService.place (place-b): calls orderService.place at line 32: (OrderDraft) → Promise<Receipt>; ${source}`);
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

  it("pages a page of long lines before the size guard would cut it, so every line stays reachable", async () => {
    const description = { text: "d".repeat(300), author: "agent:claude", signature: "s" };
    const ids = Array.from({ length: 130 }, (_, index) => `c${index}`);
    const many: AgentPage = { ...PAGE, cards: ids.map((id, index) => ({ id, ref: { block: CART_ITEM }, at: [index, 0], description })), groups: [], links: [], frames: [], resources: [], notes: [] };
    const paged = harness({ engine: { card: CARDS[CART_ITEM] ?? (() => null), type_definition: () => DEFINITIONS[CART_ITEM], line_labels: () => [], read_board: () => file(many) } });
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
  return { project: PROJECT, id, place: "draft", name: id, title, folder: null, pages: 2, cards: 6, projects: [], changes: 0, updatedAtMs: 0, label: null, error: null, ...overrides };
}

describe("list_boards", () => {
  it("lists a project's boards with their pages, publish state, health, and whether one is open or in view", async () => {
    const test = harness({
      engine: {
        list_boards: () => ({
          project: PROJECT,
          boards: [
            summary(".unfold/boards/orders.json", "Orders", { place: "published", changes: 2, folder: "flows", label: "1 missing · 2 outdated" }),
            summary(BOARD.id, "Order placement"),
            summary(".unfold/boards/broken.json", "Broken", { place: "published", error: "the file isn't a board" }),
          ],
          emptyFolders: ["old"],
        }),
        read_board: (args) => file({ ...PAGE, title: args["id"] === BOARD.id ? "Placing an order" : "Overview" }),
      },
    });
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], tabs: [BOARD, { project: PROJECT, id: ".unfold/boards/orders.json" }] });
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
