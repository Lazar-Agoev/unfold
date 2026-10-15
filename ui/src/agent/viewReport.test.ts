// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { codeViewArgs } from "../code/view";
import { engineKey, queryClient } from "../ipc";
import type { Card, CardView, DrawnFlow, Page } from "../ipc/generated";
import * as store from "../state/boards/store";
import { cardArgsOf, cardQuery } from "../state/cards";
import { closeCode, openCode, widenCode, type CodeOpen } from "../state/codePanel";
import { showBoard, useShownBoardStore } from "../state/shownBoard";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page } from "../test/workspace";
import { REPORT_DELAY_MS, codeReport, currentView, installViewReports, visibleItems } from "./viewReport";

/** A CLI's flow with two calls opened in place, one inside the other: `main`, and in it `place_order`. */
const [CLI] = Object.values(import.meta.glob<DrawnFlow>("../../../testdata/goldens/drawn/07-cli.json", { eager: true, import: "default" }));

const P = "p-orders0000";
const ID = ".unfold/boards/orders.json";
const OTHER = { project: P, id: ".unfold/boards/b.json" };
const REF = { project: P, id: ID };
const TARGET = { board: REF, page: "page-1" };

const cards: Card[] = [
  { id: "a", ref: { block: "src/routes/orders.ts#createOrder" }, at: [0, 0] },
  { id: "b", ref: { block: "src/services/order-service.ts#OrderService.place" }, at: [2000, 0] },
];
const PAGE: Page = page("page-1", cards, {
  combined: [{ id: "a-checks", title: "Checks", members: ["key-a-if"], collapsed: true, author: "agent:claude", signature: "" }],
  links: [{ id: "a-to-b", from: "a", to: "b", kind: "calls", author: "person:maya" }],
  proposals: [{ id: "b-email", after: "key-b-call", title: "Email a confirmation", author: "person:maya" }],
});

/** A card as drawn: a's draws the combined step, b's the proposed step. */
function drawnCard(key: string, step: "combined" | "proposed"): CardView {
  const actions = { after: true, start: true, open: false };
  return {
    type: "definition",
    flow: {
      header: { key, kind: "function", form: "function", label: key, path: "src/a.ts", state: "current", line: 1, async: false, definedWith: [], imports: [], actions },
      combinable: [],
      steps: {
        steps: [
          step === "combined"
            ? { id: 0, state: "reached", kind: "combined", item: "a-checks", title: "Checks", members: ["key-a-if"], collapsed: true, lines: [2, 4], conditions: 1, loops: 0, calls: [], library: 0, unknown: 0 }
            : { id: 0, state: "reached", kind: "proposed", item: "b-email", title: "Email a confirmation" },
        ],
      },
      routes: [],
    },
  };
}

const DRAWN: ReadonlyMap<string, CardView> = new Map([
  ["a", drawnCard("src/routes/orders.ts#createOrder", "combined")],
  ["b", drawnCard("src/services/order-service.ts#OrderService.place", "proposed")],
]);
const drawn = (cardId: string) => DRAWN.get(cardId);

let engine: FakeEngine;

beforeEach(() => {
  closeCode();
  queryClient.clear();
  store.resetBoards();
  useShownBoardStore.setState({ board: null });
  engine = installFakeEngine();
});

afterEach(() => {
  engine.uninstall();
  vi.useRealTimers();
});

describe("visibleItems", () => {
  const rects = new Map([
    ["a", { x: 0, y: 0, width: 500, height: 300 }],
    ["b", { x: 2000, y: 0, width: 500, height: 300 }],
  ]);

  it("lists the cards the camera shows, the combined and proposed steps they draw, and connections with an end in view", () => {
    expect(visibleItems(PAGE, rects, { x: 0, y: 0, zoom: 1 }, { width: 1000, height: 800 }, drawn)).toEqual(["a", "a-checks", "a-to-b"]);
    // Zoomed out to a quarter, the canvas shows 4000 units across.
    expect(visibleItems(PAGE, rects, { x: 0, y: 0, zoom: 0.25 }, { width: 1000, height: 800 }, drawn)).toEqual(["a", "b", "a-checks", "b-email", "a-to-b"]);
    // Panned right past a.
    expect(visibleItems(PAGE, rects, { x: -1800, y: 0, zoom: 1 }, { width: 1000, height: 800 }, drawn)).toEqual(["b", "b-email", "a-to-b"]);
    // Cards not drawn yet show none of the page's combined or proposed steps.
    expect(visibleItems(PAGE, rects, null, null, () => undefined)).toEqual(["a", "b", "a-to-b"]);
  });

  it("lists everything before the canvas has a camera or a size", () => {
    expect(visibleItems(PAGE, rects, null, { width: 1000, height: 800 }, drawn)).toEqual(["a", "b", "a-checks", "b-email", "a-to-b"]);
    expect(visibleItems(PAGE, rects, { x: 0, y: 0, zoom: 1 }, null, drawn)).toHaveLength(5);
  });

  it("lists the frames in view, and leaves out what a collapsed frame hides", () => {
    const framed: Page = { ...PAGE, frames: [{ id: "server", title: "server", members: ["b"], look: "box", author: "agent:claude", collapsed: true }] };
    const shown = new Map([
      ["a", { x: 0, y: 0, width: 500, height: 300 }],
      ["server", { x: 1990, y: 0, width: 280, height: 90 }],
    ]);
    expect(visibleItems(framed, shown, null, null, drawn)).toEqual(["a", "a-checks", "a-to-b", "server"]);
    expect(visibleItems(framed, shown, { x: -1800, y: 0, zoom: 1 }, { width: 1000, height: 800 }, drawn)).toEqual(["server"]);
  });
});

describe("currentView", () => {
  it("reports Home with the open boards, and the shown board's page and selection; an arrow by its calls", () => {
    store.adoptBoardState(boardState(P, ID, boardDocument([PAGE])));
    store.adoptBoardState(boardState(P, OTHER.id));
    expect(currentView(null)).toEqual({ selection: { kind: "none" }, visible: [], held: [REF, OTHER] });

    showBoard(REF);
    store.select(TARGET, { kind: "card", cardId: "a", key: "key-a-if" });
    expect(currentView(null)).toEqual({
      board: REF,
      page: "page-1",
      selection: { kind: "step", card: "a", key: "key-a-if" },
      // No card is drawn in this window yet, so no combined or proposed step shows.
      visible: ["a", "b", "a-to-b"],
      held: [REF, OTHER],
    });
    for (const [selection, reported] of [
      [{ kind: "card", cardId: "a", key: null }, { kind: "card", id: "a" }],
      [{ kind: "card", cardId: "a", key: "src/routes/orders.ts#createOrder" }, { kind: "card", id: "a" }],
      [{ kind: "field", cardId: "a", key: "src/a.ts#Order", index: 1 }, { kind: "card", id: "a" }],
      [{ kind: "combined", combinedId: "a-checks" }, { kind: "combined", id: "a-checks" }],
      [{ kind: "link", linkId: "a-to-b" }, { kind: "link", id: "a-to-b" }],
      [{ kind: "frame", frameId: "server" }, { kind: "frame", id: "server" }],
      [{ kind: "proposal", proposalId: "b-email" }, { kind: "proposal", id: "b-email" }],
      [{ kind: "line", arrowId: "x", call: "key-a-call", from: "a", to: "b" }, { kind: "arrow", from: "a", to: "b", calls: ["key-a-call"] }],
      [{ kind: "calls", arrowId: "x", calls: ["key-1", "key-2"], from: "a", to: "b" }, { kind: "arrow", from: "a", to: "b", calls: ["key-1", "key-2"] }],
    ] as const) {
      store.select(TARGET, selection);
      expect(currentView(null).selection).toEqual(reported);
    }
  });
});

describe("a step in a call opened in place", () => {
  it("names the definition that owns it, from the card as the window draws it", () => {
    const cli: Card = { id: "cli", ref: { block: "shop/cli.py#" }, at: [0, 0], view: "expanded", open: [["shop/cli.py#/if[0]/then/call:main", "shop/cli.py#main/call:place_order"]] };
    store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [cli])])));
    showBoard(REF);
    const args = cardArgsOf(TARGET, "cli");
    if (!args || !CLI) {
      throw new Error("The CLI card isn't on the page");
    }
    const selected = (key: string) => {
      store.select(TARGET, { kind: "card", cardId: "cli", key });
      return currentView(null).selection;
    };
    // Before the card is drawn, the window can't tell which definition holds the step.
    expect(selected("shop/service.py#place_order/call:load_order")).toEqual({ kind: "step", card: "cli", key: "shop/service.py#place_order/call:load_order" });
    queryClient.setQueryData(cardQuery(args).queryKey, { type: "definition", flow: CLI });
    expect(selected("shop/service.py#place_order/call:load_order")).toEqual({ kind: "step", card: "cli", key: "shop/service.py#place_order/call:load_order", owner: "shop/service.py#place_order" });
    expect(selected("shop/cli.py#main/call:place_order")).toEqual({ kind: "step", card: "cli", key: "shop/cli.py#main/call:place_order", owner: "shop/cli.py#main" });
    // The card's own step has no owner to name.
    expect(selected("shop/cli.py#/if[0]")).toEqual({ kind: "step", card: "cli", key: "shop/cli.py#/if[0]" });
  });
});

describe("the code panel in the report", () => {
  const CREATE = "src/routes/orders.ts#createOrder";
  const block: CodeOpen = { project: P, target: { kind: "block", key: CREATE }, show: null, cardId: "a" };

  it("says what the panel opened on, and the file and lines it shows once they load", () => {
    expect(codeReport(block, undefined)).toEqual({ key: CREATE, widened: false });
    expect(codeReport(block, fixture("code_view", "code_view_create_order"))).toEqual({ key: CREATE, file: "src/routes/orders.ts", lines: [25, 35], widened: false, state: "current" });
    const span: CodeOpen = { ...block, target: { kind: "lines", path: "src/routes/orders.ts", lines: [32, 33] }, show: { kind: "file", path: "src/routes/orders.ts" } };
    expect(codeReport(span, undefined)).toEqual({ span: [32, 33], file: "src/routes/orders.ts", widened: true });
    expect(codeReport({ ...block, target: { kind: "file", path: "db/schema.sql" } }, undefined)).toEqual({ file: "db/schema.sql", widened: false });
    expect(codeReport({ ...block, target: { kind: "start", key: "entrance:server" } }, undefined)).toEqual({ key: "entrance:server", widened: false });
  });

  it("is in the window's view while the panel is open on the shown board", () => {
    store.adoptBoardState(boardState(P, ID, boardDocument([PAGE])));
    showBoard(REF);
    expect(currentView(null).code).toBeUndefined();
    openCode({ project: P, target: block.target, cardId: "a" });
    expect(currentView(null).code).toEqual({ key: CREATE, widened: false });
    queryClient.setQueryData(engineKey("code_view", codeViewArgs(block)), fixture("code_view", "code_view_create_order"));
    expect(currentView(null).code).toEqual({ key: CREATE, file: "src/routes/orders.ts", lines: [25, 35], widened: false, state: "current" });
    widenCode({ kind: "file", path: "src/routes/orders.ts" });
    // Widened, its lines are the file's, which haven't loaded.
    expect(currentView(null).code).toEqual({ key: CREATE, widened: true });
  });
});

describe("installViewReports", () => {
  it("reports a change once it settles, only when the view changed, and at once on focus", async () => {
    vi.useFakeTimers();
    store.adoptBoardState(boardState(P, ID, boardDocument([PAGE])));
    const stop = installViewReports();
    await vi.advanceTimersByTimeAsync(REPORT_DELAY_MS);
    expect(engine.windowCalls).toEqual([{ command: "report_view", args: { selection: { kind: "none" }, visible: [], held: [REF] } }]);

    showBoard(REF);
    store.select(TARGET, { kind: "card", cardId: "b", key: null });
    await vi.advanceTimersByTimeAsync(REPORT_DELAY_MS - 1);
    expect(engine.windowCalls).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(engine.windowCalls).toHaveLength(2);
    expect(engine.windowCalls[1]?.args).toMatchObject({ board: REF, selection: { kind: "card", id: "b" } });

    // A change that leaves the view as it was isn't reported; gaining focus always is.
    store.select(TARGET, { kind: "card", cardId: "b", key: null });
    await vi.advanceTimersByTimeAsync(REPORT_DELAY_MS);
    expect(engine.windowCalls).toHaveLength(2);
    window.dispatchEvent(new Event("focus"));
    expect(engine.windowCalls).toHaveLength(3);
    stop();
  });

  it("reports when the code panel opens, and again when its lines load", async () => {
    vi.useFakeTimers();
    store.adoptBoardState(boardState(P, ID, boardDocument([PAGE])));
    showBoard(REF);
    const stop = installViewReports();
    await vi.advanceTimersByTimeAsync(REPORT_DELAY_MS);
    const target = { kind: "block", key: "src/routes/orders.ts#createOrder" } as const;
    openCode({ project: P, target, cardId: "a" });
    await vi.advanceTimersByTimeAsync(REPORT_DELAY_MS);
    expect(engine.windowCalls.at(-1)?.args).toMatchObject({ code: { key: target.key, widened: false } });
    queryClient.setQueryData(engineKey("code_view", codeViewArgs({ project: P, target, show: null, cardId: "a" })), fixture("code_view", "code_view_create_order"));
    await vi.advanceTimersByTimeAsync(REPORT_DELAY_MS);
    expect(engine.windowCalls.at(-1)?.args).toMatchObject({ code: { key: target.key, file: "src/routes/orders.ts", lines: [25, 35], state: "current" } });
    closeCode();
    await vi.advanceTimersByTimeAsync(REPORT_DELAY_MS);
    expect(engine.windowCalls.at(-1)?.args).not.toHaveProperty("code");
    stop();
  });
});
