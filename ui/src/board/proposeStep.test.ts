import { describe, expect, it } from "vitest";
import type { Card, CardView, Group, Key, Page, ProjectId } from "../ipc/generated";
import { placementProblem, type Selection } from "../state/boards";
import { fixture } from "../test/engine";
import { page } from "../test/workspace";
import { proposalChoice } from "./proposeStep";
import type { SourceContext } from "./viewCode";

const P: ProjectId = "p-orders0000";
const CREATE = "src/routes/orders.ts#function:createOrder";
const IF = `${CREATE}/if[0]`;
const ORDER = `${CREATE}/variable:order`;
const RETURN = `${IF}/return[0]`;
const PLACE = "src/services/order-service.ts#class:OrderService/method:place";
const CART = "src/cart.ts#interface:Cart";

const createOrder = fixture("card", "card_create_order") as Extract<CardView, { type: "block" }>;
const cart: CardView = {
  type: "block",
  header: { key: CART, kind: "interface", name: "Cart", qualifiedName: "Cart", path: "src/cart.ts", startLine: 12, endLine: 18, language: "typescript", fileState: "current", source: "current", owner: "src/cart.ts#" },
  code: null,
  flow: null,
};
const MODELS: Readonly<Record<Key, CardView>> = { [CREATE]: createOrder, [CART]: cart };
const context: SourceContext = { project: (name) => (name === undefined ? P : null), model: (_project, key) => MODELS[key] };

const CARDS: readonly Card[] = [
  { id: "create-order", ref: { block: CREATE }, at: [0, 0] },
  { id: "cart", ref: { block: CART }, at: [600, 0] },
];
const placeGroup: Group = { id: "place-it", card: "create-order", title: "Place it now", members: [ORDER, `${CREATE}/expression_statement[0]`], collapsed: true };

const choice = (selection: Selection, onPage: Page = page("page-1", CARDS)) => proposalChoice(onPage, selection, context);
const step = (key: Key, extra: Partial<Extract<Selection, { kind: "card" }>> = {}): Selection => ({ kind: "card", cardId: "create-order", key, ...extra });

describe("where Add proposed step after… is offered", () => {
  it("after a real step of the card's own Flow, with a decision's actual paths", () => {
    expect(choice(step(ORDER))).toEqual({ cardId: "create-order", after: ORDER, paths: [] });
    expect(choice(step(IF))).toEqual({ cardId: "create-order", after: IF, paths: ["yes", "no"] });
  });

  it("not after a step nothing runs after, the function itself, a statement Flow doesn't draw, or a type", () => {
    expect(choice(step(RETURN))).toBeNull();
    expect(choice(step(CREATE))).toBeNull();
    expect(choice({ kind: "card", cardId: "create-order", key: null })).toBeNull();
    expect(choice(step(`${CREATE}/nope`))).toBeNull();
    expect(choice({ kind: "card", cardId: "cart", key: `${CART}/property:id` })).toBeNull();
  });

  it("not on a step shown in a call expanded in place, on a run, or inside a collapsed combined block", () => {
    expect(choice(step(`${PLACE}/n1`, { owner: PLACE }))).toBeNull();
    expect(choice(step(ORDER, { last: `${CREATE}/expression_statement[1]` }))).toBeNull();
    expect(choice(step(ORDER), page("page-1", CARDS, { groups: [placeGroup] }))).toBeNull();
    expect(choice(step(ORDER), page("page-1", CARDS, { groups: [{ ...placeGroup, collapsed: false }] }))).toEqual({ cardId: "create-order", after: ORDER, paths: [] });
  });

  it("not while the card's Flow isn't current", () => {
    const stale: SourceContext = { ...context, model: (_project, key) => (key === CREATE ? { ...createOrder, flow: createOrder.flow ? { ...createOrder.flow, source: "changed" } : null } : MODELS[key]) };
    expect(proposalChoice(page("page-1", CARDS), step(ORDER), stale)).toBeNull();
  });

  it("follows the one placement rule the agent's propose does", () => {
    const flow = createOrder.flow;
    if (!flow) {
      throw new Error("the fixture has a Flow model");
    }
    expect(placementProblem(flow, ORDER, undefined)).toBeNull();
    expect(placementProblem(flow, IF, "no")).toBeNull();
    expect(placementProblem(flow, IF, undefined)).toMatchObject({ kind: "needsPath", paths: ["yes", "no"] });
    expect(placementProblem(flow, IF, "each")).toMatchObject({ kind: "noSuchPath" });
    expect(placementProblem(flow, ORDER, "yes")).toMatchObject({ kind: "oneWay" });
    expect(placementProblem(flow, RETURN, undefined)).toMatchObject({ kind: "terminal" });
    expect(placementProblem(flow, CREATE, undefined)).toEqual({ kind: "start" });
    expect(placementProblem(flow, `${CREATE}/nope`, undefined)).toEqual({ kind: "notStep" });
  });
});
