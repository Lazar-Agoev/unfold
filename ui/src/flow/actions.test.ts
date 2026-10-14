import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CardView, DrawnFlow } from "../ipc/generated";
import { boardDocument, boardState, page } from "../test/workspace";

const engine = vi.hoisted(() => ({
  call: vi.fn(() => Promise.resolve(null)),
  onEngineEvent: vi.fn(() => () => undefined),
  windowCommand: vi.fn(() => Promise.resolve(null)),
}));
vi.mock("../state/boards/engine", () => engine);

const store = await import("../state/boards/store");
const codePanel = await import("../state/codePanel");
const { flowActions, flowSelection } = await import("./actions");

const REF = { project: "p-orders0000", id: ".unfold/boards/b.json" };
const PAGE = { board: REF, page: "page-1" };
const CALL = "shop/service.py#place_order/call:load_order";
const GOLDENS = import.meta.glob<DrawnFlow>("../../../testdata/goldens/drawn/01-place-order.json", { eager: true, import: "default" });
const VIEW = { type: "definition", flow: Object.values(GOLDENS)[0] } as CardView;

function cards() {
  return store.getOpenBoard(REF)?.document.pages[0]?.cards ?? [];
}

beforeEach(() => {
  codePanel.closeCode();
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
    const actions = flowActions(PAGE, "place-order", VIEW);
    actions.onSelect?.({ kind: "block", key: CALL });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "place-order", key: CALL });
    actions.onSelect?.({ kind: "combined", item: "stop" });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "combined", combinedId: "stop" });
    actions.onSelect?.({ kind: "proposed", item: "p-1" });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "proposal", proposalId: "p-1" });
  });

  it("opens and closes a call in place on the card", () => {
    const actions = flowActions(PAGE, "place-order", VIEW);
    actions.onToggleOpen?.([CALL]);
    expect(cards()[0]?.open).toEqual([[CALL]]);
    actions.onToggleOpen?.([CALL]);
    expect(cards()[0]?.open).toBeUndefined();
  });

  it("opens and collapses a combined step", () => {
    flowActions(PAGE, "place-order", VIEW).onToggleCombined?.("stop", false);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.combined[0]?.collapsed).toBe(false);
  });

  it("selects a block and opens its code from its `</>`", () => {
    flowActions(PAGE, "place-order", VIEW).onViewCode?.({ kind: "block", key: CALL });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "place-order", key: CALL });
    expect(codePanel.getCodeOpen()).toEqual({ project: REF.project, target: { kind: "block", key: CALL }, show: null, cardId: "place-order", gap: null });
  });

  it("opens a thin row's lines from its `</>`, with nothing selected", () => {
    store.select(PAGE, { kind: "card", cardId: "place-order", key: CALL });
    const row = { calls: [], lines: [21, 23] as [number, number], unknown: 0 };
    flowActions(PAGE, "place-order", VIEW).onViewRow?.("shop/service.py", row);
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "none" });
    expect(codePanel.getCodeOpen()).toEqual({ project: REF.project, target: { kind: "lines", path: "shop/service.py", lines: [21, 23] }, show: null, cardId: "place-order", gap: null });
  });

  it("opens a gap tick's lines, outlining the tick", () => {
    store.select(PAGE, { kind: "card", cardId: "place-order", key: CALL });
    flowActions(PAGE, "place-order", VIEW).onOpenLines?.("shop/service.py", [17, 17]);
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "none" });
    expect(codePanel.getCodeOpen()).toEqual({
      project: REF.project,
      target: { kind: "lines", path: "shop/service.py", lines: [17, 17] },
      show: null,
      cardId: null,
      gap: { path: "shop/service.py", line: 17 },
    });
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
