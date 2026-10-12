import { describe, expect, it } from "vitest";
import type { Card, CardView, FlowModel, Group, Key, Page, ProjectId } from "../ipc/generated";
import type { Selection } from "../state/boards";
import { fixture } from "../test/engine";
import { page } from "../test/workspace";
import { entranceTargetSource, selectionSource, type SourceContext } from "./viewCode";

const P: ProjectId = "p-orders0000";
const ORDERS = "src/routes/orders.ts";
const CREATE = `${ORDERS}#function:createOrder`;
const IF = `${CREATE}/if[0]`;
const PLACE = "src/services/order-service.ts#class:OrderService/method:place";
const ENTRANCE = "entrance:configured:import";
const CART = "src/cart.ts#interface:Cart";
const GONE = "src/gone.ts#function:gone";

const createOrder = fixture("card", "card_create_order") as Extract<CardView, { type: "block" }>;
const entrance = fixture("card", "card_first_entrance") as Extract<CardView, { type: "entrance" }>;
const [importHandler] = entrance.targets;
if (!importHandler) {
  throw new Error("the fixture's entrance has a target");
}

/** `OrderService.place`, a callee shown expanded in place: one call step with a recorded focus. */
const place: CardView = (() => {
  const flow: FlowModel = {
    key: PLACE,
    path: "src/services/order-service.ts",
    language: "typescript",
    kind: "method",
    source: "current",
    nodes: [
      { id: 0, kind: "start", key: PLACE, label: "place", lane: 0, startLine: 8, endLine: 14 },
      { id: 1, kind: "call", key: `${PLACE}/n1`, label: "insertOrder(…)", lane: 0, startLine: 10, endLine: 10, focus: { startLine: 10, startColumn: 11, endLine: 10, endColumn: 29 } },
      { id: 2, kind: "end", label: "End", lane: 0 },
    ],
    routes: [],
    tree: [],
    start: 0,
    end: 2,
    lanes: 1,
    warnings: [],
  };
  return {
    type: "block",
    header: { key: PLACE, kind: "method", name: "place", qualifiedName: "OrderService.place", path: flow.path, startLine: 8, endLine: 14, language: "typescript", fileState: "current", source: "current", owner: "src/services/order-service.ts#" },
    code: null,
    flow,
  };
})();

const cart: CardView = {
  type: "block",
  header: { key: CART, kind: "interface", name: "Cart", qualifiedName: "Cart", path: "src/cart.ts", startLine: 12, endLine: 18, language: "typescript", fileState: "current", source: "current", owner: "src/cart.ts#" },
  code: null,
  flow: null,
};

const MODELS: Readonly<Record<Key, CardView>> = { [CREATE]: createOrder, [PLACE]: place, [ENTRANCE]: entrance, [CART]: cart };

const context: SourceContext = {
  project: (name) => (name === undefined ? P : null),
  model: (_project, key) => MODELS[key],
};

const CARDS: readonly Card[] = [
  { id: "create-order", ref: { block: CREATE }, at: [0, 0] },
  { id: "import", ref: { entrance: ENTRANCE }, at: [0, 400] },
  { id: "cart", ref: { block: CART }, at: [600, 0] },
  { id: "gone", ref: { block: GONE }, at: [600, 400] },
  { id: "elsewhere", ref: { block: CREATE, project: "shop" }, at: [900, 0] },
];

const queueGroup: Group = { id: "queue-it", card: "create-order", title: "Queue it", members: [`${IF}/variable:job`, `${IF}/expression_statement[0]`, `${IF}/return[0]`], collapsed: true };

const PAGE: Page = page("page-1", CARDS, {
  groups: [queueGroup],
  resources: [
    { id: "orders-table", kind: "table", name: "orders", at: [0, 800], anchors: [{ file: "db/schema.sql", lines: [3, 9] }, { key: CREATE }], author: "person:maya" },
    { id: "audit", kind: "table", name: "audit", at: [300, 800], anchors: [{ key: CART }], author: "person:maya" },
  ],
  links: [{ id: "writes", from: "create-order", to: "orders-table", kind: "writes", anchors: [{ key: CREATE }] }],
  proposals: [{ id: "notify", card: "create-order", after: `${CREATE}/variable:order`, title: "Notify the customer", author: "person:maya" }],
});

const source = (selection: Selection) => selectionSource(PAGE, selection, context);

describe("View code's source of a selection", () => {
  it("opens a function card at its definition's first line, without columns", () => {
    expect(source({ kind: "card", cardId: "create-order", key: null }).target).toEqual({ project: P, path: ORDERS, line: 25, focus: { startLine: 25, endLine: 25 } });
    // Its Start pill (the card's own key) is the same.
    expect(source({ kind: "card", cardId: "create-order", key: CREATE }).target?.focus).toEqual({ startLine: 25, endLine: 25 });
  });

  it("opens a real step at its recorded focus: a condition's own expression, not the whole if", () => {
    expect(source({ kind: "card", cardId: "create-order", key: IF }).target).toEqual({
      project: P,
      path: ORDERS,
      line: 27,
      focus: { startLine: 27, startColumn: 6, endLine: 27, endColumn: 46 },
    });
  });

  it("opens a call step that has an inline expansion at the caller's call expression, in the caller's file", () => {
    expect(source({ kind: "card", cardId: "create-order", key: `${CREATE}/variable:order` }).target).toEqual({
      project: P,
      path: ORDERS,
      line: 32,
      focus: { startLine: 32, startColumn: 16, endLine: 32, endColumn: 41 },
    });
  });

  it("opens a combined block at the span of its statements, in its function's file, without columns", () => {
    expect(source({ kind: "group", groupId: "queue-it" }).target).toEqual({ project: P, path: ORDERS, line: 28, focus: { startLine: 28, endLine: 30 } });
  });

  it("opens a step inside a call expanded in place in the callee's file, and the inset's title at the callee's definition", () => {
    expect(source({ kind: "card", cardId: "create-order", key: `${PLACE}/n1`, owner: PLACE }).target).toEqual({
      project: P,
      path: "src/services/order-service.ts",
      line: 10,
      focus: { startLine: 10, startColumn: 11, endLine: 10, endColumn: 29 },
    });
    expect(source({ kind: "card", cardId: "create-order", key: PLACE, owner: PLACE }).target).toEqual({
      project: P,
      path: "src/services/order-service.ts",
      line: 8,
      focus: { startLine: 8, endLine: 8 },
    });
  });

  it("opens a Shift-selected run at its statements' lines", () => {
    expect(source({ kind: "card", cardId: "create-order", key: `${CREATE}/variable:order`, last: `${CREATE}/expression_statement[1]` }).target?.focus).toEqual({ startLine: 32, endLine: 34 });
  });

  it("opens an entrance card at its declaration evidence, keeping a configuration file's lines", () => {
    expect(source({ kind: "card", cardId: "import", key: null }).target).toEqual({ project: P, path: "unfold.toml", line: 5, focus: { startLine: 5, endLine: 9 } });
  });

  it("opens a resolved entrance target at that handler's definition, apart from the entrance's evidence", () => {
    const handler = importHandler;
    expect(entranceTargetSource(P, handler).target).toEqual({ project: P, path: "src/jobs/import-orders.ts", line: 6, focus: { startLine: 6, endLine: 6 } });
    expect(entranceTargetSource(P, handler).target?.path).not.toBe(source({ kind: "card", cardId: "import", key: null }).target?.path);
  });

  it("opens a type card at its definition", () => {
    expect(source({ kind: "card", cardId: "cart", key: null }).target).toEqual({ project: P, path: "src/cart.ts", line: 12, focus: { startLine: 12, endLine: 12 } });
  });

  it("opens a resource card or a connection at the first source it cites, an evidence range highlighting its lines", () => {
    expect(source({ kind: "resource", resourceId: "orders-table" }).target).toEqual({ project: P, path: "db/schema.sql", line: 3, focus: { startLine: 3, endLine: 9 } });
    expect(source({ kind: "resource", resourceId: "audit" }).target).toEqual({ project: P, path: "src/cart.ts", line: 12, focus: { startLine: 12, endLine: 12 } });
    expect(source({ kind: "link", linkId: "writes" }).target).toEqual({ project: P, path: ORDERS, line: 25, focus: { startLine: 25, endLine: 25 } });
  });

  it("has no source, never a guessed one, for unresolved or external targets, missing references, and what has no code", () => {
    const handler = importHandler;
    expect(entranceTargetSource(P, { ...handler, resolution: "unresolved", target: null, path: null, line: null }).target).toBeNull();
    expect(entranceTargetSource(P, { ...handler, resolution: "external", target: null, path: null, line: null }).target).toBeNull();
    expect(source({ kind: "card", cardId: "gone", key: null }).target).toBeNull();
    expect(source({ kind: "card", cardId: "elsewhere", key: null })).toEqual({ target: null, reason: "Its project isn't connected." });
    expect(source({ kind: "card", cardId: "create-order", key: `${CREATE}/nope` }).target).toBeNull();
    expect(source({ kind: "card", cardId: "create-order", key: `${PLACE}/n9`, owner: GONE }).target).toBeNull();
    expect(source({ kind: "none" }).target).toBeNull();
    expect(source({ kind: "frame", frameId: "api" }).target).toBeNull();
  });

  it("opens a proposed step's placement: the step it goes after, never code of its own", () => {
    expect(source({ kind: "proposal", proposalId: "notify" }).target).toEqual({
      project: P,
      path: ORDERS,
      line: 32,
      focus: { startLine: 32, startColumn: 16, endLine: 32, endColumn: 41 },
    });
    const gone: Page = { ...PAGE, proposals: [{ id: "notify", card: "create-order", after: `${CREATE}/variable:gone`, title: "Notify the customer", author: "person:maya" }] };
    expect(selectionSource(gone, { kind: "proposal", proposalId: "notify" }, context)).toEqual({
      target: null,
      reason: "The step it goes after isn't in the code now. A proposed step has no code of its own.",
    });
    expect(source({ kind: "proposal", proposalId: "nowhere" }).target).toBeNull();
  });

  it("opens a combined block only while its function's code verifies", () => {
    const stale: SourceContext = { ...context, model: (_project, key) => (key === CREATE ? { ...createOrder, code: { ...createOrder.code, source: { state: "changed" } } as never } : MODELS[key]) };
    expect(selectionSource(PAGE, { kind: "group", groupId: "queue-it" }, stale).target).toBeNull();
  });
});
