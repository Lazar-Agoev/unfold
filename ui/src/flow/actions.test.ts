import { beforeEach, describe, expect, it, vi } from "vitest";
import { boardDocument, boardState, page } from "../test/workspace";

const engine = vi.hoisted(() => ({
  call: vi.fn(() => Promise.resolve(null)),
  onEngineEvent: vi.fn(() => () => undefined),
  windowCommand: vi.fn(() => Promise.resolve(null)),
}));
vi.mock("../state/boards/engine", () => engine);

const store = await import("../state/boards/store");
const { flowActions, flowSelection } = await import("./actions");

const REF = { project: "p-orders0000", id: ".unfold/boards/b.json" };
const PAGE = { board: REF, page: "page-1" };
const CALL = "shop/service.py#place_order/call:load_order";

function cards() {
  return store.getOpenBoard(REF)?.document.pages[0]?.cards ?? [];
}

beforeEach(() => {
  store.resetBoards();
  store.adoptBoardState(
    boardState(
      REF.project,
      REF.id,
      boardDocument([
        page("page-1", [{ id: "place-order", ref: { block: "shop/service.py#place_order" }, at: [0, 0] }], {
          combined: [{ id: "stop", title: "Stop early", members: ["shop/service.py#place_order/if[0]"], collapsed: true, author: "Ada", signature: "s" }],
        }),
      ]),
    ),
  );
});

describe("flowActions", () => {
  it("selects a block in the card, a combined step, or a proposed step on the page", () => {
    const actions = flowActions(PAGE, "place-order");
    actions.onSelect?.({ kind: "block", key: CALL });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "place-order", key: CALL });
    actions.onSelect?.({ kind: "combined", item: "stop" });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "combined", combinedId: "stop" });
    actions.onSelect?.({ kind: "proposed", item: "p-1" });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "proposal", proposalId: "p-1" });
  });

  it("opens and closes a call in place on the card", () => {
    const actions = flowActions(PAGE, "place-order");
    actions.onToggleOpen?.([CALL]);
    expect(cards()[0]?.open).toEqual([[CALL]]);
    actions.onToggleOpen?.([CALL]);
    expect(cards()[0]?.open).toBeUndefined();
  });

  it("opens and collapses a combined step", () => {
    flowActions(PAGE, "place-order").onToggleCombined?.("stop", false);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.combined[0]?.collapsed).toBe(false);
  });
});

describe("flowSelection", () => {
  it("shows a block selected only in its own card", () => {
    expect(flowSelection({ kind: "card", cardId: "place-order", key: CALL }, "place-order")).toEqual({ kind: "block", key: CALL });
    expect(flowSelection({ kind: "card", cardId: "other", key: CALL }, "place-order")).toBeNull();
    expect(flowSelection({ kind: "card", cardId: "place-order", key: null }, "place-order")).toBeNull();
    expect(flowSelection({ kind: "combined", combinedId: "stop" }, "place-order")).toEqual({ kind: "combined", item: "stop" });
    expect(flowSelection({ kind: "proposal", proposalId: "p-1" }, "place-order")).toEqual({ kind: "proposed", item: "p-1" });
    expect(flowSelection({ kind: "none" }, "place-order")).toBeNull();
  });
});
