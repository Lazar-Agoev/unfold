import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CardView, DrawnFlow, OtherCall, ThinRow } from "../ipc/generated";
import { boardDocument, boardState, page } from "../test/workspace";

const engine = vi.hoisted(() => ({
  call: vi.fn(() => Promise.resolve(null)),
  onEngineEvent: vi.fn(() => () => undefined),
  windowCommand: vi.fn(() => Promise.resolve(null)),
}));
vi.mock("../state/boards/engine", () => engine);

const store = await import("../state/boards/store");
const codePanel = await import("../state/codePanel");
const { startLink } = await import("../state/boards/linking");
const { flowActions, flowSelection, selectForMenu } = await import("./actions");

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
    actions.onSelect({ kind: "block", key: CALL });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "place-order", key: CALL });
    actions.onSelect({ kind: "combined", item: "stop" });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "combined", combinedId: "stop" });
    actions.onSelect({ kind: "proposed", item: "p-1" });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "proposal", proposalId: "p-1" });
  });

  it("opens and closes a call in place on the card", () => {
    const actions = flowActions(PAGE, "place-order", VIEW);
    actions.onToggleOpen([CALL]);
    expect(cards()[0]?.open).toEqual([[CALL]]);
    actions.onToggleOpen([CALL]);
    expect(cards()[0]?.open).toBeUndefined();
  });

  it("opens and collapses a combined step", () => {
    flowActions(PAGE, "place-order", VIEW).onToggleCombined("stop", false);
    expect(store.getOpenBoard(REF)?.document.pages[0]?.combined[0]?.collapsed).toBe(false);
  });

  it("selects a block and opens its code from its `</>`", () => {
    flowActions(PAGE, "place-order", VIEW).onViewCode({ kind: "block", key: CALL });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "place-order", key: CALL });
    expect(codePanel.getCodeOpen()).toEqual({ project: REF.project, target: { kind: "block", key: CALL }, show: null, cardId: "place-order" });
  });

  it("opens a thin row's lines from its `</>`, with nothing selected, keeping which row it is", () => {
    store.select(PAGE, { kind: "card", cardId: "place-order", key: CALL });
    const warning: OtherCall = { key: "shop/service.py#place_order/try[0]/except[0]/call:logger.warning", callee: "logger.warning", form: "call", to: { outcome: "unknown" }, awaited: false, sometimes: false, inside: [], actions: { after: true, start: false, open: false } };
    const row: ThinRow = { calls: [warning], lines: [21, 21], unknown: 1 };
    flowActions(PAGE, "place-order", VIEW).onViewRow("shop/service.py", row);
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "none" });
    expect(codePanel.getCodeOpen()).toEqual({
      project: REF.project,
      target: { kind: "lines", path: "shop/service.py", lines: [21, 21] },
      show: null,
      cardId: "place-order",
      from: { kind: "row", key: warning.key },
    });
  });

  it("opens a gap tick's lines, outlining the tick and keeping where it sits", () => {
    store.select(PAGE, { kind: "card", cardId: "place-order", key: CALL });
    flowActions(PAGE, "place-order", VIEW).onOpenLines("shop/service.py", [17, 17]);
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "none" });
    expect(codePanel.getCodeOpen()).toEqual({
      project: REF.project,
      target: { kind: "lines", path: "shop/service.py", lines: [17, 17] },
      show: null,
      cardId: "place-order",
      from: { kind: "gap", place: { before: "shop/service.py#place_order/try[0]" } },
    });
  });

  it("selects a block for its menu without ending a link being picked there", () => {
    startLink(PAGE, "other");
    selectForMenu(PAGE, "place-order", { kind: "block", key: CALL });
    expect(store.getPageView(PAGE).linking).toEqual({ from: "other", to: null });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "place-order", key: CALL });
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
