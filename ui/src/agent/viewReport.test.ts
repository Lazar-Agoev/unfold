// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Card, CardView, Page } from "../ipc/generated";
import * as store from "../state/boards/store";
import { showBoard, useShownBoardStore } from "../state/shownBoard";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page } from "../test/workspace";
import { REPORT_DELAY_MS, currentView, installViewReports, visibleItems } from "./viewReport";

const P = "p-orders0000";
const ID = ".unfold/boards/orders.json";
const OTHER = { project: P, id: ".unfold/boards/b.json" };
const REF = { project: P, id: ID };
const TARGET = { board: REF, page: "page-1" };

const cards: Card[] = [
  { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] },
  { id: "b", ref: { block: "src/b.ts#function:b" }, at: [2000, 0] },
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
      header: { key, kind: "function", form: "function", label: key, path: "src/a.ts", line: 1, async: false, generator: false, definedWith: [], imports: [], actions },
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
  ["a", drawnCard("src/a.ts#function:a", "combined")],
  ["b", drawnCard("src/b.ts#function:b", "proposed")],
]);
const drawn = (cardId: string) => DRAWN.get(cardId);

let engine: FakeEngine;

beforeEach(() => {
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
      [{ kind: "card", cardId: "a", key: "src/a.ts#function:a" }, { kind: "card", id: "a" }],
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
});
