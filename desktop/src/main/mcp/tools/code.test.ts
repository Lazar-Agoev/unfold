import { describe, expect, it, vi } from "vitest";
import cardCreateOrder from "../../../../../testdata/workspace/commands/card_create_order.json";
import cardFirstEntrance from "../../../../../testdata/workspace/commands/card_first_entrance.json";
import relationsCreateOrder from "../../../../../testdata/workspace/commands/relations_create_order.json";
import type { AgentPage } from "../../../shared/agent-api";
import type { CardView, Relations, TypeDefinition, TypeUses } from "../engine-types";
import { getRelations, readCode } from "./code";
import { harness, PROJECT, projectRow, recorded } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const CREATE_ORDER = "src/routes/orders.ts#function:createOrder";
const CART = "src/cart.ts#interface:Cart";
const BOARD = { project: PROJECT, id: ".unfold/boards/order-placement.json" };
const PAGE: AgentPage = {
  id: "page-1",
  title: "Placing an order",
  defaultView: "flow",
  cards: [
    { id: "create-order", ref: { block: CREATE_ORDER }, at: [0, 0] },
    { id: "charge", ref: { block: "src/pay.ts#function:charge", project: "payments" }, at: [400, 0] },
  ],
  groups: [{ id: "queue-it", card: "create-order", title: "Queue it", members: [], collapsed: false }],
  labels: [],
  guards: [],
  links: [],
};
const BOARD_INFO = { title: "Order placement", place: "published", changes: 0, revision: "r1", unsaved: false };

const LABEL = { source: "src/routes/orders.ts#function:createOrder/variable:order/call[0]", seq: 0, label: { inputs: [{ type: { text: "OrderDraft", spans: [] } }], inputsUnknown: false, output: { text: "Promise<Order>", spans: [] } } };

const CART_DEFINITION: TypeDefinition = {
  key: CART,
  kind: "interface",
  name: "Cart",
  path: "src/cart.ts",
  line: 3,
  language: "typescript",
  members: [
    { name: "items", kind: "property", optional: false, readonly: true, default: false, type: { text: "CartItem[]", spans: [{ start: 0, end: 8, style: "declared", target: "src/cart.ts#interface:CartItem" }] } },
    { name: "total", kind: "property", optional: true, readonly: false, default: false, type: { text: "Money", spans: [{ start: 0, end: 5, style: "unresolved", note: "a variable, not a type" }] } },
  ],
  inherited: [{ from: { text: "Base", spans: [{ start: 0, end: 4, style: "declared", target: "src/base.ts#interface:Base" }] }, members: [{ name: "id", kind: "property", optional: false, readonly: false, default: false, type: { text: "string", spans: [{ start: 0, end: 6, style: "builtin" }] } }] }],
  uses: 6,
};

function app(extra: Parameters<typeof harness>[0] = {}) {
  return harness({
    engine: {
      card: (args) => recorded<CardView>(String(args["key"]).startsWith("entrance:") ? cardFirstEntrance : cardCreateOrder),
      relations: () => recorded<Relations>(relationsCreateOrder),
      line_labels: () => [LABEL],
      type_definition: () => CART_DEFINITION,
      board_refs: () => ({ refs: [{ project: PROJECT, board: BOARD.id, title: "Order placement", page: "page-1", pageTitle: "Placing an order" }], truncated: false }),
      ...extra.engine,
    },
    ...(extra.projects ? { projects: extra.projects } : {}),
    window: (request) => (request.type === "agent_read" ? { ok: true, page: PAGE, board: BOARD_INFO } : undefined),
  });
}

describe("read_code", () => {
  it("reads a function: verified source, signature, Flow steps with keys, throws, and each call with its label", async () => {
    const tools = app();
    const { text } = await tools.call(readCode, { key: CREATE_ORDER, project: "ts-orders" });
    const lines = text.split("\n");
    expect(lines.slice(0, 6)).toEqual([
      "Unfold · ts-orders · as Claude",
      `function createOrder at src/routes/orders.ts:25–35; key ${CREATE_ORDER}.`,
      "Signature: export async function createOrder(req: Request, res: Response): Promise<void> {",
      "Source:",
      "25 | export async function createOrder(req: Request, res: Response): Promise<void> {",
      "26 |   const draft: OrderDraft = req.body;",
    ]);
    expect(lines).toContain("Flow steps (paths from written structure; names are fallbacks until labeled):");
    expect(lines).toContain(`- 27–31 condition: fallback name “req.header?”; key ${CREATE_ORDER}/if[0]`);
    expect(lines).toContain(`- 28 call (branch lane 1): fallback name “ordersQueue.add”; key ${CREATE_ORDER}/if[0]/variable:job`);
    expect(lines).toContain("Throws nothing directly.");
    expect(lines).toContain(
      `- line 32: calls orderService.place → OrderService.place (key src/services/order-service.ts#class:OrderService/method:place), (OrderDraft) → Promise<Order>; source ${CREATE_ORDER}/variable:order/call[0], seq 0`,
    );
    expect(lines).toContain(`- line 28: calls ordersQueue.add → external, bullmq; source ${CREATE_ORDER}/if[0]/variable:job/call[0], seq 0`);
    expect(lines).toContain(`- line 29: calls res.status().json → unresolved: receiver type unknown; source ${CREATE_ORDER}/if[0]/expression_statement[0]/call[0], seq 0`);
    expect(lines).toContain("Called from 1 place.");
    expect(tools.engineCalls.find(([command]) => command === "line_labels")?.[1]).toEqual({
      project: PROJECT,
      lines: [
        { source: `${CREATE_ORDER}/variable:order/call[0]`, seq: 0 },
        { source: `${CREATE_ORDER}/expression_statement[0]/call[0]`, seq: 0 },
      ],
    });
  });

  it("pages long functions by lines, and leaves out what the view doesn't ask for", async () => {
    const tools = app();
    const flow = await tools.call(readCode, { key: CREATE_ORDER, project: PROJECT, view: "flow", lines: [32, 34] });
    expect(flow.text).not.toContain("Source");
    expect(flow.text).not.toContain("Calls (");
    expect(flow.text).toContain(`- 32 call: fallback name “OrderService.place”; key ${CREATE_ORDER}/variable:order`);
    expect(flow.text).not.toContain("req.header?");
    const code = await tools.call(readCode, { key: CREATE_ORDER, project: PROJECT, view: "code", lines: [26, 27] });
    expect(code.text).toContain("Source, lines 26–27 of 25–35:\n26 |   const draft: OrderDraft = req.body;\n27 |   if (");
    expect(code.text).toContain("Calls (1):");
    expect(code.text).toContain("More: read_code with lines [28, 35].");
    expect(code.text).not.toContain("Flow steps");
    expect((await tools.call(readCode, { key: CREATE_ORDER, lines: [30, 20] })).isError).toBe(true);
    expect((await tools.call(readCode, { key: CREATE_ORDER, lines: [1, 5] })).text).toBe(
      "Can't read those lines: createOrder spans lines 25–35. Pass lines within them, or leave lines out for the first page.",
    );
  });

  it("ends a page of long lines early, so the size guard never cuts where the next page starts", async () => {
    const card = recorded<CardView>(cardCreateOrder);
    if (card.type !== "block" || card.code?.source.state !== "current") {
      throw new Error("createOrder's card has current source");
    }
    const texts = Array.from({ length: 200 }, (_, index) => `const line${index} = "${"x".repeat(200)}";`);
    let at = 0;
    const lines = texts.map((line, index) => {
      const entry = { number: 25 + index, start: at, end: at + line.length, indent: 0 };
      at += line.length + 1;
      return entry;
    });
    const long: CardView = { ...card, header: { ...card.header, endLine: 224 }, code: { ...card.code, source: { ...card.code.source, text: texts.join("\n"), lines, outline: [], dedent: 0 } } };
    const { text } = await app({ engine: { card: () => long } }).call(readCode, { key: CREATE_ORDER, project: PROJECT, view: "code" });
    const shown = Number(/Source, lines 25–(\d+) of 25–224:/.exec(text)?.[1]);
    expect(shown).toBeLessThan(25 + 149);
    expect(text).toContain(`More: read_code with lines [${shown + 1}, ${Math.min(224, shown + 150)}].`);
    expect(text).not.toContain(`const line${shown - 24} =`);
  });

  it("says when the file changed instead of showing source", async () => {
    const changed = recorded<Extract<CardView, { type: "block" }>>(cardCreateOrder);
    const tools = app({ engine: { card: () => ({ ...changed, header: { ...changed.header, source: "changed" }, code: changed.code ? { ...changed.code, source: { state: "changed" } } : null }) } });
    const { text } = await tools.call(readCode, { key: CREATE_ORDER });
    expect(text).toContain("The file changed since the last compile, so Unfold shows no source, steps, or throws: compile_project, then read it again.");
    expect(text).not.toContain("Signature:");
    expect(text).not.toContain("Flow steps");
  });

  it("reads a type's members with their links, inherited members, and uses", async () => {
    const card = recorded<Extract<CardView, { type: "block" }>>(cardCreateOrder);
    const tools = app({ engine: { card: () => ({ ...card, header: { ...card.header, key: CART, kind: "interface", qualifiedName: "Cart", path: "src/cart.ts", startLine: 3, endLine: 7 }, code: null, flow: null }) } });
    const { text } = await tools.call(readCode, { key: CART });
    expect(text.split("\n").slice(1)).toEqual([
      `interface Cart at src/cart.ts:3–7; key ${CART}.`,
      "Members (2):",
      "- readonly items: CartItem[] (CartItem: src/cart.ts#interface:CartItem)",
      "- total?: Money (Money: unresolved, a variable, not a type)",
      "Inherited from Base (Base: src/base.ts#interface:Base) (1):",
      "- id: string",
      "Used in 6 places (get_relations lists them).",
      "Next: read_call with a call's source for what it sends; get_relations for callers and uses; add_cards puts it on a page.",
    ]);
    expect(tools.commands()).not.toContain("relations");
  });

  it("reads an entrance: where it's declared and its handlers", async () => {
    const { text } = await app().call(readCode, { key: 'entrance:["manual","command","","import","src/jobs/import-orders.ts#importOrders"]' });
    expect(text.split("\n").slice(1, 9)).toEqual([
      'Entrance command import (command, detector manual); key entrance:["manual","command","","import","src/jobs/import-orders.ts#importOrders"].',
      "In cli.",
      "Declared at .unfold/config.toml:6–9 (/entrance/0):",
      "6 | [[entrance]]",
      '7 | kind = "command"',
      '8 | trigger = "import"',
      '9 | handler = "src/jobs/import-orders.ts#importOrders"',
      "Handlers, in order (1):",
    ]);
    expect(text).toContain("1. src/jobs/import-orders.ts#importOrders → importOrders at src/jobs/import-orders.ts:6 (key src/jobs/import-orders.ts#function:importOrders)");
  });

  it("defaults to the card selected in the person's view, in the project the card reads from", async () => {
    const tools = app({ projects: [projectRow(PROJECT, "ts-orders"), projectRow("p-payments00", "payments")] });
    tools.report({ board: BOARD, page: "page-1", selection: { kind: "step", card: "create-order", key: `${CREATE_ORDER}/if[0]` }, visible: [], tabs: [BOARD] });
    const step = await tools.call(readCode, { view: "flow" });
    expect(step.text).toContain(`Selected in the person's view: the step ${CREATE_ORDER}/if[0].`);
    expect(tools.engineCalls.find(([command]) => command === "card")?.[1]).toEqual({ project: PROJECT, key: CREATE_ORDER, lens: "both" });

    tools.report({ board: BOARD, page: "page-1", selection: { kind: "group", id: "queue-it" }, visible: [], tabs: [BOARD] });
    await tools.call(readCode, {});
    expect(tools.engineCalls.filter(([command]) => command === "card").at(-1)?.[1]).toMatchObject({ key: CREATE_ORDER });

    await tools.call(readCode, { key: `unfold://ts-orders/${BOARD.id}?page=page-1&item=charge` });
    expect(tools.engineCalls.filter(([command]) => command === "card").at(-1)?.[1]).toEqual({ project: "p-payments00", key: "src/pay.ts#function:charge", lens: "both" });

    tools.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], tabs: [BOARD] });
    expect((await tools.call(readCode, {})).text).toBe(
      "Can't tell which code you mean: no key was given and no card is selected in Unfold. Pass key: from search, list_entrances, read_page, or get_relations.",
    );
    expect((await tools.call(readCode, { key: `unfold://ts-orders/${BOARD.id}?page=page-1&item=nope` })).text).toBe(
      "Can't read the card: there is no card with id “nope” on the page “Placing an order”. read_page lists the page's cards with their ids and keys.",
    );
  });
});

describe("get_relations", () => {
  it("lists callers, calls, the entrances that reach it, and the boards that show it", async () => {
    const tools = app();
    const { text } = await tools.call(getRelations, { key: CREATE_ORDER, limit: 5 });
    const lines = text.split("\n");
    expect(lines.slice(1, 4)).toEqual([
      `createOrder: key ${CREATE_ORDER}.`,
      "Called from 1 place:",
      `- src/routes/orders.ts at src/routes/orders.ts:49, passed as argument 4 (key src/routes/orders.ts#; source src/routes/orders.ts#expression_statement[2]/call[0])`,
    ]);
    expect(lines).toContain("Calls (8):");
    expect(lines).toContain('Reached at least from: POST /orders (key entrance:["express.route","http","POST","/orders","requireUser"]).');
    expect(lines).toContain(`- Order placement › Placing an order (board id ${BOARD.id}, page id page-1)`);
    expect(tools.engineCalls.find(([command]) => command === "relations")?.[1]).toEqual({ project: PROJECT, key: CREATE_ORDER, callersOffset: 0, callersLimit: 5 });
  });

  it("lists a type's uses, heritage marked, and pages them", async () => {
    const uses: TypeUses = {
      total: 3,
      places: [
        { owner: "src/cart.ts#function:total", label: "total", path: "src/cart.ts", line: 12 },
        { owner: "src/cart.ts#interface:PricedCart", label: "PricedCart", path: "src/cart.ts", line: 20, heritage: "extends" },
      ],
    };
    const tools = app({ engine: { type_uses: () => uses, board_refs: () => ({ refs: [], truncated: false }) } });
    const { text } = await tools.call(getRelations, { key: CART, limit: 2 });
    expect(text.split("\n").slice(1)).toEqual([
      `Cart: key ${CART}.`,
      "Used in 3 places (1–2 here):",
      "- total at src/cart.ts:12 (key src/cart.ts#function:total)",
      "- PricedCart at src/cart.ts:20, extends it (key src/cart.ts#interface:PricedCart)",
      "No board shows it yet.",
      'More: get_relations with cursor "2".',
      "Next: read_code with a key; add_cards puts code on a page.",
    ]);
    expect(tools.commands()).not.toContain("relations");
    await tools.call(getRelations, { key: CART, limit: 2, cursor: "2" });
    expect(tools.engineCalls.filter(([command]) => command === "type_uses").at(-1)?.[1]).toEqual({ project: PROJECT, key: CART, offset: 2, limit: 2 });
  });
});
