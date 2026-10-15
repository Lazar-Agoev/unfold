import { describe, expect, it, vi } from "vitest";
import cardCreateOrder from "../../../../../testdata/workspace/commands/card_create_order.json";
import cardPlaceAnOrder from "../../../../../testdata/workspace/commands/card_place_an_order.json";
import codeTextCreateOrder from "../../../../../testdata/workspace/commands/code_text_create_order.json";
import relationsCreateOrder from "../../../../../testdata/workspace/commands/relations_create_order.json";
import type { AgentPage } from "../../../shared/agent-api";
import type { CardView, CodeText, Relations, TypeDefinition, TypeMember, TypeUses } from "../engine-types";
import { getRelations, readCode } from "./code";
import { harness, PROJECT, recorded } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const CREATE_ORDER = "src/routes/orders.ts#createOrder";
const PLACE_AN_ORDER = "entrance:server/place-an-order";
const CART = "src/cart.ts#Cart";
const BOARD = { project: PROJECT, id: ".unfold/boards/order-placement.json" };
const PAGE: AgentPage = {
  id: "page-1",
  title: "Placing an order",
  defaultView: "expanded",
  cards: [
    { id: "create-order", ref: { block: CREATE_ORDER }, at: [0, 0] },
    { id: "charge", ref: { block: "src/pay.ts#charge" }, at: [400, 0] },
  ],
  names: [],
  combined: [],
  guards: [],
  links: [],
};
const BOARD_INFO = { title: "Order placement", revision: "r1", unsaved: false };

const CART_DEFINITION: TypeDefinition = {
  key: CART,
  kind: "interface",
  name: "Cart",
  path: "src/cart.ts",
  line: 3,
  language: "typescript",
  members: [
    { name: "items", kind: "property", optional: false, readonly: true, default: false, type: { text: "CartItem[]", spans: [{ start: 0, end: 8, to: { outcome: "project", key: "src/cart.ts#CartItem" } }] } },
    { name: "total", kind: "property", optional: true, readonly: false, default: false, type: { text: "Money", spans: [{ start: 0, end: 5, to: { outcome: "unknown", note: "a variable, not a type" } }] } },
  ],
  inherited: [
    {
      from: { text: "Base", spans: [{ start: 0, end: 4, to: { outcome: "project", key: "src/base.ts#Base" } }] },
      members: [{ name: "id", kind: "property", optional: false, readonly: false, default: false, type: { text: "string", spans: [{ start: 0, end: 6, to: { outcome: "builtin" } }] } }],
    },
  ],
  uses: 6,
};

/** A type's card: createOrder's drawn flow with a type's header. */
function typeCard(): CardView {
  const card = recorded<Extract<CardView, { type: "definition" }>>(cardCreateOrder);
  return { type: "definition", flow: { ...card.flow, header: { ...card.flow.header, key: CART, kind: "type", form: "interface", label: "Cart", path: "src/cart.ts", line: 3 } } };
}

function app(extra: Parameters<typeof harness>[0] = {}) {
  return harness({
    engine: {
      card: (args) => (args["key"] === PLACE_AN_ORDER ? recorded<CardView>(cardPlaceAnOrder) : args["key"] === CART ? typeCard() : recorded<CardView>(cardCreateOrder)),
      code_text: () => recorded<CodeText>(codeTextCreateOrder),
      relations: () => recorded<Relations>(relationsCreateOrder),
      type_definition: () => CART_DEFINITION,
      board_refs: () => ({ refs: [{ project: PROJECT, board: BOARD.id, title: "Order placement", page: "page-1", pageTitle: "Placing an order" }], truncated: false }),
      ...extra.engine,
    },
    window: (request) => (request.type === "agent_read" ? { ok: true, page: PAGE, board: BOARD_INFO } : undefined),
  });
}

describe("read_code", () => {
  it("reads a definition's compiled tree, each block with its key, as code_text prints it", async () => {
    const tools = app();
    const { text } = await tools.call(readCode, { key: CREATE_ORDER, project: "ts-orders" });
    const tree = recorded<CodeText>(codeTextCreateOrder).text.trimEnd();
    expect(text.split("\n").slice(0, 2)).toEqual(["Unfold · ts-orders · as Claude", `function createOrder at src/routes/orders.ts:25; key ${CREATE_ORDER}.`]);
    expect(text).toContain(`Compiled tree:\n${tree}\n`);
    expect(text).toContain(`L32  call:orderService.place                   ▸ src/services/order-service.ts#OrderService.place · binds order   key=${CREATE_ORDER}/call:orderService.place`);
    expect(text.split("\n").slice(-2)).toEqual(["Called from 1 place.", "Next: read_call with a call's key for what it sends; get_relations for callers; add_cards puts it on a page."]);
    expect(tools.commands()).not.toContain("type_definition");
  });

  it("pages a long tree by its lines", async () => {
    const long: CodeText = { text: Array.from({ length: 200 }, (_, index) => `L${index + 1}  call:step${index}`).join("\n"), labels: [] };
    const tools = app({ engine: { code_text: () => long } });
    const first = await tools.call(readCode, { key: CREATE_ORDER });
    expect(first.text).toContain("L150  call:step149\nShowing 150 of 200. Next page: read_code with cursor \"150\".");
    expect(first.text).toContain("Called from 1 place.");
    const second = await tools.call(readCode, { key: CREATE_ORDER, cursor: "150" });
    expect(second.text).toContain("Compiled tree:\nL151  call:step150");
    expect(second.text).not.toContain("Called from");
  });

  it("reads a type's members with their links, inherited members, and uses", async () => {
    const tools = app();
    const lines = (await tools.call(readCode, { key: CART })).text.split("\n");
    expect(lines).toContain("Members (2):");
    expect(lines).toContain("- readonly items: CartItem[] (CartItem: src/cart.ts#CartItem)");
    expect(lines).toContain("- total?: Money (Money: unresolved, a variable, not a type)");
    expect(lines).toContain("Inherited from Base (Base: src/base.ts#Base) (1):");
    expect(lines).toContain("Used in 6 places (get_relations lists them).");
    expect(tools.commands()).not.toContain("relations");
  });

  it("pages a class's members after its tree, keeping the lines after them", async () => {
    // A class with 120 methods: its tree has a line for each, and its members list them again.
    const methods = Array.from({ length: 120 }, (_, index) => `fetch_records_for_report_${index}`);
    const tree: CodeText = {
      text: methods.map((method, index) => `L${index * 8 + 10}  ${method}${" ".repeat(20)}function · key=data-pipelines/api/data_access.py#DataAccess.${method}`).join("\n"),
      labels: [],
    };
    const members: TypeMember[] = methods.map((method) => ({
      name: method,
      kind: "method",
      optional: false,
      readonly: false,
      default: false,
      type: { text: "(self, run_id: str, start: datetime, end: datetime) -> list[ReportRecord]", spans: [] },
    }));
    const tools = app({ engine: { code_text: () => tree, type_definition: () => ({ ...CART_DEFINITION, members, inherited: [] }) } });
    const seen = new Set<string>();
    let cursor: string | undefined;
    let pages = 0;
    do {
      const { text } = await tools.call(readCode, { key: CART, ...(cursor !== undefined ? { cursor } : {}) });
      pages += 1;
      expect(text).not.toContain("Cut here");
      expect(text.split("\n").at(-1)).toBe("Next: read_call with a call's key for what it sends; get_relations for callers and uses; add_cards puts it on a page.");
      for (const [line] of text.matchAll(/^- method \w+/gm)) {
        seen.add(line);
      }
      cursor = /Next page: read_code with cursor "(\d+)"/.exec(text)?.[1];
    } while (cursor !== undefined && pages < 10);
    expect(cursor).toBeUndefined();
    expect(seen.size).toBe(120);
  });

  it("reads an entrance: what starts it and the code it starts at", async () => {
    const tools = app();
    const { text } = await tools.call(readCode, { key: PLACE_AN_ORDER });
    expect(text.split("\n").slice(1, 7)).toEqual([
      `Entrance of Server Place an order; key ${PLACE_AN_ORDER}.`,
      "On: POST /orders",
      "Code, in the order it runs (3):",
      "1. requireUser (key src/routes/middleware.ts#requireUser): found at src/routes/middleware.ts:4",
      "2. validateBody (key src/routes/middleware.ts#validateBody): found at src/routes/middleware.ts:12",
      "3. createOrder (key src/routes/orders.ts#createOrder): found at src/routes/orders.ts:25",
    ]);
    expect(tools.commands()).not.toContain("code_text");
  });

  it("defaults to the card selected in the person's view, in the board's project", async () => {
    const tools = app();
    tools.report({ board: BOARD, page: "page-1", selection: { kind: "step", card: "create-order", key: `${CREATE_ORDER}/if[0]` }, visible: [], held: [BOARD] });
    const step = await tools.call(readCode, {});
    expect(step.text).toContain(`Selected in the person's view: the block ${CREATE_ORDER}/if[0].`);
    expect(tools.engineCalls.find(([command]) => command === "code_text")?.[1]).toEqual({ project: PROJECT, key: CREATE_ORDER });

    await tools.call(readCode, { key: `unfold://ts-orders/${BOARD.id}?page=page-1&item=charge` });
    expect(tools.engineCalls.filter(([command]) => command === "card").at(-1)?.[1]).toEqual({ project: PROJECT, key: "src/pay.ts#charge" });

    // A step shown in a call opened in place reads the definition that owns it.
    const PLACE = "src/services/order-service.ts#OrderService.place";
    tools.report({ board: BOARD, page: "page-1", selection: { kind: "step", card: "create-order", key: `${PLACE}/call:repo.insert`, owner: PLACE }, visible: [], held: [BOARD] });
    const owned = await tools.call(readCode, {});
    expect(tools.engineCalls.filter(([command]) => command === "card").at(-1)?.[1]).toEqual({ project: PROJECT, key: PLACE });
    expect(owned.text).toContain(`Selected in the person's view: the block ${PLACE}/call:repo.insert.`);

    tools.report({ board: BOARD, page: "page-1", selection: { kind: "combined", id: "queue-it" }, visible: [], held: [BOARD] });
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
      "Callers (1):",
      "- src/routes/orders.ts at src/routes/orders.ts:49, passes (key src/routes/orders.ts#; call src/routes/orders.ts#/call:ordersRouter.post)",
    ]);
    expect(lines).toContain(`- line 32: calls orderService.place → OrderService.place (key src/services/order-service.ts#OrderService.place); key ${CREATE_ORDER}/call:orderService.place`);
    expect(lines).toContain(`- line 28: calls ordersQueue.add → the library bullmq; key ${CREATE_ORDER}/if[0]/then/call:ordersQueue.add`);
    expect(lines).toContain(`- Order placement › Placing an order (board id ${BOARD.id}, page id page-1)`);
    expect(tools.engineCalls.find(([command]) => command === "relations")?.[1]).toEqual({ project: PROJECT, key: CREATE_ORDER, callersLimit: 5 });
  });

  it("lists a type's uses, heritage marked, and pages them", async () => {
    const uses: TypeUses = {
      total: 3,
      places: [
        { owner: "src/cart.ts#total", label: "total", path: "src/cart.ts", line: 12, kind: "annotation" },
        { owner: "src/cart.ts#PricedCart", label: "PricedCart", path: "src/cart.ts", line: 20, kind: "extends" },
      ],
    };
    const tools = app({ engine: { type_uses: () => uses, board_refs: () => ({ refs: [], truncated: false }) } });
    const { text } = await tools.call(getRelations, { key: CART, limit: 2 });
    expect(text.split("\n").slice(1)).toEqual([
      `Cart: key ${CART}.`,
      "Uses (1–2 of 3):",
      "- total at src/cart.ts:12 (key src/cart.ts#total)",
      "- PricedCart at src/cart.ts:20, extends (key src/cart.ts#PricedCart)",
      "No board shows it yet.",
      'More: get_relations with cursor "2" for the next page.',
      "Next: read_code with a key; add_cards puts code on a page.",
    ]);
    expect(tools.commands()).not.toContain("relations");
    await tools.call(getRelations, { key: CART, limit: 2, cursor: "2" });
    expect(tools.engineCalls.filter(([command]) => command === "type_uses").at(-1)?.[1]).toEqual({ project: PROJECT, key: CART, offset: 2, limit: 2 });
  });

  it("pages callers and calls with one cursor under the size guard, and keeps the lines after them", async () => {
    // As data-pipelines/ml/train_and_forecast.py#main in pathway-agents: 290 calls, long keys.
    const file = `data-pipelines/${"forecasting-models/".repeat(3)}train_and_forecast.py`;
    const outgoing = Array.from({ length: 290 }, (_, index) => ({
      call: `${file}#main/try[0]/body/for[0]/body/call:pipeline.step_${index}`,
      line: index + 10,
      kind: "calls" as const,
      text: `pipeline.step_${index}`,
      to: { outcome: "project" as const, key: `${file}#Pipeline.step_${index}` },
      targetName: `Pipeline.step_${index}`,
    }));
    const callers = Array.from({ length: 130 }, (_, index) => ({
      owner: `${file}#caller_${index}`,
      ownerName: `caller_${index}`,
      call: `${file}#caller_${index}/call:main`,
      path: file,
      line: index + 400,
      kind: "calls" as const,
      cursor: `${index}:0:${file}`,
    }));
    const tools = app({
      engine: {
        relations: ({ callersAfter, callersLimit }): Relations => {
          const start = callersAfter === undefined ? 0 : callers.findIndex((caller) => caller.cursor === callersAfter) + 1;
          return {
            ...recorded<Relations>(relationsCreateOrder),
            callers: callers.slice(start, start + Number(callersLimit)),
            callersTotal: callers.length,
            outgoing,
            reachedFrom: [{ key: "entrance:pipelines/train", name: "Train the models", app: "Pipelines" }],
          };
        },
      },
    });
    const calls = new Set<string>();
    const callersSeen: string[] = [];
    let cursor: string | undefined;
    let pages = 0;
    do {
      const { text } = await tools.call(getRelations, { key: CREATE_ORDER, limit: 100, ...(cursor !== undefined ? { cursor } : {}) });
      pages += 1;
      expect(text).not.toContain("Cut here");
      expect(text).toContain(`- Order placement › Placing an order (board id ${BOARD.id}, page id page-1)`);
      expect(text.split("\n").at(-1)).toBe("Next: read_code with a key; add_cards puts code on a page.");
      if (cursor === undefined) {
        expect(text).toContain("Reached at least from: Train the models in Pipelines (key entrance:pipelines/train).");
      }
      for (const [, key] of text.matchAll(/; key (\S+)$/gm)) {
        calls.add(key ?? "");
      }
      for (const [, key] of text.matchAll(/\(key (\S+); call /g)) {
        callersSeen.push(key ?? "");
      }
      cursor = /More: get_relations with cursor "([^"]+)"/.exec(text)?.[1];
    } while (cursor !== undefined && pages < 20);
    expect(cursor).toBeUndefined();
    expect(calls.size).toBe(290);
    // Each page of callers starts after the last one the page before showed: none twice, none missed.
    expect(callersSeen).toEqual(callers.map((caller) => caller.owner));
    const afters = tools.engineCalls.filter(([command]) => command === "relations").map(([, args]) => (args as { callersAfter?: string }).callersAfter);
    expect(afters[0]).toBeUndefined();
    expect(afters.slice(1).every((after) => after !== undefined && callers.some((caller) => caller.cursor === after))).toBe(true);
  });

  it("refuses a cursor whose offset isn't one", async () => {
    const tools = app();
    for (const cursor of ["x:1:0:a.ts", ":1:0:a.ts", "-1:1:0:a.ts"]) {
      expect((await tools.call(getRelations, { key: CREATE_ORDER, cursor })).text).toBe(
        `“${cursor}” isn't a cursor of this list. Leave cursor out for the first page, or pass the next cursor a result gave.`,
      );
    }
    expect(tools.commands()).not.toContain("relations");
  });
});
