import { describe, expect, it } from "vitest";
import type { CardView, DrawnFlow, StartCard, TypeDefinition, TypeMember, TypeText, View } from "../ipc/generated";
import { cardRefKey } from "../state/boards/keys";
import { fixture } from "../test/engine";
import { NODE_PORT, callPort, codePort, computeArrows, fieldPort, fieldRows, portsByCard, projectCalls, type ArrowCard } from "./arrows";

const GOLDENS = import.meta.glob<DrawnFlow>("../../../testdata/goldens/drawn/*.json", { eager: true, import: "default" });

/** A drawn flow of the nine examples, as the IPC sends it. */
function drawn(name: string): DrawnFlow {
  const flow = GOLDENS[`../../../testdata/goldens/drawn/${name}.json`];
  if (!flow) {
    throw new Error(`no golden ${name}`);
  }
  return structuredClone(flow);
}

const definition = (flow: DrawnFlow): CardView => ({ type: "definition", flow });
const P = "p-shop000000";
const PLACE = "shop/service.py#place_order";
const LOAD = "shop/store.py#load_order";
const CHARGE = "shop/payments.py#charge_card";
const SAVE = "shop/store.py#save_order";
const LOAD_CALL = `${PLACE}/call:load_order`;
const CHARGE_CALL = `${PLACE}/try[0]/body/call:charge_card`;
const SAVE_CALL = `${PLACE}/call:save_order`;

/** What the reader drew, keyed as the canvas keys it: by project and key. */
function models(entries: readonly (readonly [string, CardView])[]): Map<string, CardView> {
  return new Map(entries.map(([key, view]) => [cardRefKey(P, key), view]));
}

function card(id: string, key: string, view: View = "compact"): ArrowCard {
  return { id, key, project: P, view };
}

/** A definition card's drawn flow with no steps: a callee's. */
function leaf(key: string, kind: DrawnFlow["header"]["kind"] = "function"): CardView {
  const flow = drawn("02-place-order-ts");
  return definition({ ...flow, header: { ...flow.header, key, kind, form: kind === "class" ? "class" : kind === "type" ? "interface" : "function" }, steps: { steps: [] }, routes: [] });
}

describe("arrows from call blocks", () => {
  const page = models([
    [PLACE, definition(drawn("01-place-order"))],
    [LOAD, leaf(LOAD)],
    [CHARGE, leaf(CHARGE)],
  ]);

  it("leave an expanded card from each call whose definition has a card on the page", () => {
    const arrows = computeArrows([card("place", PLACE, "expanded"), card("load", LOAD), card("charge", CHARGE)], page);
    expect(arrows.map((arrow) => [arrow.sourceHandle, arrow.target, arrow.calls])).toEqual([
      [callPort(LOAD_CALL), "load", [LOAD_CALL]],
      [callPort(CHARGE_CALL), "charge", [CHARGE_CALL]],
    ]);
    expect(arrows.every((arrow) => arrow.kind === "call" && !arrow.dashed)).toBe(true);
    expect(portsByCard(arrows).get("place")).toEqual([callPort(CHARGE_CALL), callPort(LOAD_CALL)].sort());
  });

  it("leave a compact card from its edge, one arrow per card standing for every call to it in the order they run", () => {
    const flow = drawn("01-place-order");
    // save_order goes to load_order's card too: both calls are one arrow from the edge.
    const steps = JSON.parse(JSON.stringify(flow.steps).replaceAll(`"key":"${SAVE}"`, `"key":"${LOAD}"`)) as DrawnFlow["steps"];
    const arrows = computeArrows([card("place", PLACE), card("load", LOAD), card("charge", CHARGE)], models([[PLACE, definition({ ...flow, steps })], [LOAD, leaf(LOAD)], [CHARGE, leaf(CHARGE)]]));
    expect(arrows.map((arrow) => [arrow.sourceHandle, arrow.target, arrow.calls])).toEqual([
      [NODE_PORT, "load", [LOAD_CALL, SAVE_CALL]],
      [NODE_PORT, "charge", [CHARGE_CALL]],
    ]);
  });

  it("leave from the calls a card opens in place only while it is expanded", () => {
    const cli = drawn("07-cli");
    const inner = `${PLACE}/call:load_order`;
    const page = models([["shop/cli.py#", definition(cli)], [LOAD, leaf(LOAD)], [PLACE, leaf(PLACE)]]);
    const expanded = computeArrows([card("cli", "shop/cli.py#", "expanded"), card("load", LOAD), card("place", PLACE)], page);
    expect(expanded.map((arrow) => [arrow.sourceHandle, arrow.target])).toEqual([
      [callPort("shop/cli.py#main/call:place_order"), "place"],
      [callPort(inner), "load"],
    ]);
    // Compact, the card draws none of its insets: only its own calls count, and none has a card here.
    expect(computeArrows([card("cli", "shop/cli.py#"), card("load", LOAD), card("place", PLACE)], page)).toEqual([]);
  });

  it("lists a drawn flow's project calls in the order they run, its insets' only when asked", () => {
    expect(projectCalls(drawn("06-checkout"), false).map((item) => item.to)).toEqual([
      "shop/payments.py#charge_card",
      "shop/receipts.py#build_receipt",
      "shop/store.py#save_order",
      "shop/format.py#order_label",
      "shop/format.py#summary",
    ]);
    expect(projectCalls(drawn("07-cli"), false)).toEqual([{ call: "shop/cli.py#/if[0]/then/call:main", to: "shop/cli.py#main" }]);
    expect(projectCalls(drawn("07-cli"), true)).toHaveLength(7);
  });

  it("draw nothing to a card that isn't on the page, to the card itself, or from a card not loaded yet", () => {
    expect(computeArrows([card("place", PLACE, "expanded")], page)).toEqual([]);
    expect(computeArrows([card("place", PLACE, "expanded"), card("load", LOAD)], models([[LOAD, leaf(LOAD)]]))).toEqual([]);
  });
});

describe("arrows from apps and entrances", () => {
  const start = (code: StartCard["code"]): CardView => ({ type: "start", key: "entrance:shop/place", kind: "entrance", app: "Shop API", name: "Place an order", on: "POST /orders/{order_id}", code });
  const code: StartCard["code"] = [
    { key: LOAD, state: "found", label: "load_order", path: "shop/store.py", line: 12 },
    { key: "shop/gone.py#gone", state: "missing" },
    { key: CHARGE, state: "stale", label: "charge_card", path: "shop/payments.py", line: 12 },
  ];
  const page = models([["entrance:shop/place", start(code)], [LOAD, leaf(LOAD)], [CHARGE, leaf(CHARGE)]]);

  it("point at the cards for their code in order: from the edge when compact, from each code row when expanded", () => {
    const compact = computeArrows([card("start", "entrance:shop/place"), card("load", LOAD), card("charge", CHARGE)], page);
    expect(compact.map((arrow) => [arrow.sourceHandle, arrow.target, arrow.kind, arrow.dashed, arrow.order])).toEqual([
      [NODE_PORT, "load", "start", false, 0],
      [NODE_PORT, "charge", "start", true, 2],
    ]);
    const expanded = computeArrows([card("start", "entrance:shop/place", "expanded"), card("load", LOAD), card("charge", CHARGE)], page);
    expect(expanded.map((arrow) => arrow.sourceHandle)).toEqual([codePort(0), codePort(2)]);
  });

  it("are never the target of a call", () => {
    const placeOn = models([[PLACE, definition(drawn("01-place-order"))], ["entrance:shop/place", start(code)]]);
    expect(computeArrows([card("place", PLACE, "expanded"), card("start", "entrance:shop/place")], placeOn)).toEqual([]);
    expect(computeArrows([card("start", "entrance:shop/place")], models([["entrance:shop/place", fixture("card", "card_place_an_order")]]))).toEqual([]);
  });
});

describe("type arrows", () => {
  const CART = "src/cart.ts#Cart";
  const ITEM = "src/cart.ts#Item";
  const BASE = "src/cart.ts#Base";
  const project = (start: number, end: number, key: string) => ({ start, end, to: { outcome: "project" as const, key } });
  const typed = (text: string, spans: TypeText["spans"]): TypeText => ({ text, spans });
  const member = (name: string, type: TypeText): TypeMember => ({ name, kind: "property", optional: false, readonly: false, default: false, type });
  const cart: TypeDefinition = {
    key: CART,
    kind: "interface",
    name: "Cart",
    path: "src/cart.ts",
    line: 1,
    language: "typescript",
    members: [member("items", typed("Item[]", [project(0, 4, ITEM)])), member("total", typed("number", [{ start: 0, end: 6, to: { outcome: "builtin" } }]))],
    inherited: [{ from: typed("Base", [project(0, 4, BASE)]), members: [member("id", typed("Item", [project(0, 4, ITEM)]))] }],
    uses: 0,
  };
  const definitions = new Map([[cardRefKey(P, CART), cart]]);

  it("leave each field's row for each card its written type names; a base on the page lists its own fields", () => {
    const page = models([[CART, leaf(CART, "type")], [ITEM, leaf(ITEM, "type")]]);
    const arrows = computeArrows([card("cart", CART, "expanded"), card("item", ITEM)], page, definitions);
    expect(arrows.map((arrow) => [arrow.sourceHandle, arrow.target, arrow.kind])).toEqual([
      [fieldPort("0"), "item", "type"],
      [fieldPort("from0/0"), "item", "type"],
    ]);
    const withBase = models([[CART, leaf(CART, "type")], [ITEM, leaf(ITEM, "type")], [BASE, leaf(BASE, "type")]]);
    const arrowsWithBase = computeArrows([card("cart", CART), card("item", ITEM), card("base", BASE)], withBase, definitions);
    expect(arrowsWithBase.map((arrow) => arrow.sourceHandle)).toEqual([fieldPort("0")]);
  });

  it("leave a class's fields only while it is compact; expanded, it draws its class-level code's calls", () => {
    const page = models([[CART, leaf(CART, "class")], [ITEM, leaf(ITEM, "type")]]);
    expect(computeArrows([card("cart", CART), card("item", ITEM)], page, definitions)).toHaveLength(2);
    expect(computeArrows([card("cart", CART, "expanded"), card("item", ITEM)], page, definitions)).toEqual([]);
  });

  it("list an alias's type, its members, then what it inherits, base by base", () => {
    expect(fieldRows({ ...cart, aliased: typed("Item", []) }).map((row) => row.row)).toEqual(["=", "0", "1", "from0/0"]);
  });
});
