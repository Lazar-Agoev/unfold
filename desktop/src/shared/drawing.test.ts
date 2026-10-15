import { describe, expect, it } from "vitest";
import cardCreateOrder from "../../../testdata/workspace/commands/card_create_order.json";
import cardPlaceAnOrder from "../../../testdata/workspace/commands/card_place_an_order.json";
import reserveFirstFree from "../../../testdata/goldens/drawn/04-reserve-first-free.json";
import cli from "../../../testdata/goldens/drawn/07-cli.json";
import shaped from "../../../testdata/goldens/drawn/place-order-shaped.json";
import type { CardView, DrawnFlow } from "../main/mcp/engine-types";
import { cardShape, flowBlocks, flowless, pageArrows, projectCalls, visitFlow, type DrawnCard } from "./drawing";

const flowOf = (json: unknown): DrawnFlow => structuredClone(json) as DrawnFlow;
const definition = (flow: DrawnFlow): CardView => ({ type: "definition", flow });
const START: CardView = structuredClone(cardPlaceAnOrder) as CardView;
const CREATE_ORDER: CardView = structuredClone(cardCreateOrder) as CardView;

/** A definition card's drawn flow with no steps, of `key`. */
function leaf(key: string, kind: DrawnFlow["header"]["kind"] = "function"): CardView {
  const flow = flowOf(cli);
  return definition({ ...flow, header: { ...flow.header, key, kind }, combinable: [], steps: { steps: [] }, routes: [] });
}

describe("the walk of a drawn flow", () => {
  it("reaches every call box, where it is drawn: in a frame's header, a test, an exit, and the flows opened in place", () => {
    const boxes: string[] = [];
    visitFlow(flowOf(reserveFirstFree), {
      box: (box) => {
        boxes.push(box.key);
      },
    });
    const FREE = "booking/slots.py#reserve_first_free";
    expect(boxes).toEqual(expect.arrayContaining([`${FREE}/for[0]/call:list_slots`, `${FREE}/for[0]/body/if[1]/then/call:is_free`, `${FREE}/for[0]/else/raise[0]/call:NoFreeSlot`]));

    const around: string[] = [];
    visitFlow(flowOf(cli), {
      box: (box, at) => {
        if (box.key === "shop/service.py#place_order/try[0]/except[0]/return[0]/call:mark_failed") {
          around.push(...at.around, at.flow.header.key);
        }
      },
    });
    // An exit's value call two calls deep: the calls opened around it, then the flow it is drawn in.
    expect(around).toEqual(["shop/cli.py#/if[0]/then/call:main", "shop/cli.py#main/call:place_order", "shop/service.py#place_order"]);
  });

  it("lists a flow's blocks, its definition first, those of the calls it opens in place only when asked", () => {
    const flow = flowOf(cli);
    const all = flowBlocks(flow).map((block) => block.key);
    const own = flowBlocks(flow, false).map((block) => block.key);
    expect(all[0]).toBe("shop/cli.py#");
    expect(own).toEqual(["shop/cli.py#", "shop/cli.py#/if[0]", "shop/cli.py#/if[0]/then", "shop/cli.py#/if[0]/then/call:main"]);
    expect(all).toContain("shop/service.py#place_order/try[0]/except[0]/return[0]/call:mark_failed");
    expect(flowBlocks(flow).find((block) => block.key === "shop/service.py#place_order/try[0]/except[0]")).toMatchObject({ form: "except", label: "except PaymentDeclined as error", lines: [20, 22] });
  });

  it("lists the project calls drawn in the order they run: boxes, and those a collapsed combined step holds", () => {
    expect(projectCalls(flowOf(shaped), true).map((call) => call.to)).toEqual(["shop/store.py#load_order", "shop/payments.py#charge_card", "shop/store.py#mark_failed", "shop/store.py#save_order", "shop/store.py#reserve_stock"]);
    expect(projectCalls(flowOf(cli), false)).toEqual([{ call: "shop/cli.py#/if[0]/then/call:main", callee: "main", to: "shop/cli.py#main" }]);
    expect(projectCalls(flowOf(cli), true).map((call) => call.to)).toContain("shop/store.py#mark_failed");
  });
});

describe("how a card shows", () => {
  it("shows a type's fields in every view, a class's while compact", () => {
    expect(cardShape(leaf("t", "type"), "expanded")).toBe("fields");
    expect(cardShape(leaf("c", "class"), "compact")).toBe("fields");
    expect(cardShape(leaf("c", "class"), "expanded")).toBe("expanded");
    expect(cardShape(START, "expanded")).toBe("expanded");
    expect(cardShape(undefined, "compact")).toBe("compact");
  });

  it("draws no flow on an app's or entrance's card, or a type's", () => {
    expect(flowless({ entrance: "entrance:server/place-an-order" }, START)).toBe("start");
    expect(flowless({ block: "t" }, leaf("t", "type"))).toBe("type");
    expect(flowless({ block: "c" }, leaf("c", "class"))).toBeUndefined();
    expect(flowless({ block: "gone" }, undefined)).toBeUndefined();
  });
});

describe("a page's arrows", () => {
  const card = (id: string, key: string, model: CardView | undefined, view: DrawnCard["view"] = "compact"): DrawnCard => ({ id, key, view, model, definition: undefined });

  it("say what each stands for: the calls, with their callees; the code an entrance starts at", () => {
    const PLACE = "src/services/order-service.ts#OrderService.place";
    const CREATE = "src/routes/orders.ts#createOrder";
    const [start, call, ...rest] = pageArrows([card("start", "entrance:server/place-an-order", START), card("create", CREATE, CREATE_ORDER, "expanded"), card("place", PLACE, leaf(PLACE))]);
    expect(rest).toEqual([]);
    expect(start).toMatchObject({ source: "start", target: "create", kind: "start", port: { at: "edge" }, dashed: false, code: { key: CREATE, state: "found" } });
    expect(call).toEqual({ source: "create", target: "place", kind: "call", port: { at: "call", call: `${CREATE}/call:orderService.place` }, dashed: false, order: 0, calls: [{ call: `${CREATE}/call:orderService.place`, callee: "orderService.place", to: PLACE }] });
  });

  it("reach the first card of a key, never an app's or entrance's, and none from a card not loaded", () => {
    const SAVE = "shop/store.py#save_order";
    const page = [card("service", "shop/service.py#place_order", definition(flowOf(shaped))), card("save", SAVE, leaf(SAVE)), card("save-2", SAVE, leaf(SAVE)), card("later", "shop/store.py#reserve_stock", undefined)];
    expect(pageArrows(page).map((arrow) => [arrow.source, arrow.target, arrow.port])).toEqual([
      ["service", "save", { at: "edge" }],
      ["service", "later", { at: "edge" }],
    ]);
  });
});
