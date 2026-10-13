// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Card, Page } from "../ipc/generated";
import * as store from "../state/boards/store";
import { showBoard, useShownBoardStore } from "../state/shownBoard";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page } from "../test/workspace";
import { REPORT_DELAY_MS, currentView, installViewReports, visibleItems } from "./viewReport";

const P = "p-orders0000";
const ID = ".unfold/boards/orders.json";
const OTHER = { project: P, id: ".unfold/local/drafts/b.json" };
const REF = { project: P, id: ID };
const TARGET = { board: REF, page: "page-1" };

const cards: Card[] = [
  { id: "a", ref: { block: "src/a.ts#function:a" }, at: [0, 0] },
  { id: "b", ref: { block: "src/b.ts#function:b" }, at: [2000, 0] },
];
const PAGE: Page = page("page-1", cards, {
  groups: [{ id: "a-checks", card: "a", title: "Checks", members: [], collapsed: false }],
  links: [{ id: "a-to-b", from: "a", to: "b" }],
  proposals: [{ id: "b-email", card: "b", after: "src/b.ts#function:b/call[0]", title: "Email a confirmation", author: "person:maya" }],
});

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

  it("lists the cards the camera shows, their step groups and proposed steps, and links with an end in view", () => {
    expect(visibleItems(PAGE, rects, { x: 0, y: 0, zoom: 1 }, { width: 1000, height: 800 })).toEqual(["a", "a-checks", "a-to-b"]);
    // Zoomed out to a quarter, the canvas shows 4000 units across.
    expect(visibleItems(PAGE, rects, { x: 0, y: 0, zoom: 0.25 }, { width: 1000, height: 800 })).toEqual(["a", "b", "a-checks", "b-email", "a-to-b"]);
    // Panned right past a.
    expect(visibleItems(PAGE, rects, { x: -1800, y: 0, zoom: 1 }, { width: 1000, height: 800 })).toEqual(["b", "b-email", "a-to-b"]);
  });

  it("lists everything before the canvas has a camera or a size", () => {
    expect(visibleItems(PAGE, rects, null, { width: 1000, height: 800 })).toEqual(["a", "b", "a-checks", "b-email", "a-to-b"]);
    expect(visibleItems(PAGE, rects, { x: 0, y: 0, zoom: 1 }, null)).toHaveLength(5);
  });

  it("lists the frames in view, and leaves out what a collapsed frame hides", () => {
    const framed: Page = { ...PAGE, frames: [{ id: "server", title: "server", members: ["b"], look: "box", author: "agent:claude", collapsed: true }] };
    const drawn = new Map([
      ["a", { x: 0, y: 0, width: 500, height: 300 }],
      ["server", { x: 1990, y: 0, width: 280, height: 90 }],
    ]);
    expect(visibleItems(framed, drawn, null, null)).toEqual(["a", "a-checks", "a-to-b", "server"]);
    expect(visibleItems(framed, drawn, { x: -1800, y: 0, zoom: 1 }, { width: 1000, height: 800 })).toEqual(["server"]);
  });
});

describe("currentView", () => {
  it("reports Home with the open boards, and the shown board's page, selection, and lens (Diagram or Code)", () => {
    store.adoptBoardState(boardState(P, ID, boardDocument([PAGE])));
    store.adoptBoardState(boardState(P, OTHER.id));
    expect(currentView(null)).toEqual({ selection: { kind: "none" }, visible: [], tabs: [REF, OTHER] });

    showBoard(REF);
    store.select(TARGET, { kind: "card", cardId: "a", key: "src/a.ts#function:a/if[0]" });
    expect(currentView(null)).toEqual({
      board: REF,
      page: "page-1",
      selection: { kind: "step", card: "a", key: "src/a.ts#function:a/if[0]" },
      lens: "code",
      visible: ["a", "b", "a-checks", "b-email", "a-to-b"],
      tabs: [REF, OTHER],
    });
    // A Flow lens is Diagram.
    store.setPageLens(TARGET, "flow");
    const line = { arrowId: "x", project: P, source: "src/a.ts#function:a/call[0]", seq: 0, from: "a", to: "b", line: 7 };
    store.select(TARGET, { kind: "line", ...line });
    expect(currentView(null)).toMatchObject({
      lens: "diagram",
      selection: { kind: "line", from: "a", to: "b", project: P, source: line.source, seq: 0, line: 7 },
    });
    for (const [selection, reported] of [
      [{ kind: "card", cardId: "a", key: null }, { kind: "card", id: "a" }],
      [{ kind: "card", cardId: "a", key: "src/a.ts#function:a" }, { kind: "card", id: "a" }],
      [{ kind: "group", groupId: "a-checks" }, { kind: "group", id: "a-checks" }],
      [{ kind: "link", linkId: "a-to-b" }, { kind: "link", id: "a-to-b" }],
      [{ kind: "frame", frameId: "server" }, { kind: "frame", id: "server" }],
      [{ kind: "proposal", proposalId: "b-email" }, { kind: "proposal", id: "b-email" }],
      // A step shown in a call expanded in place carries the block that owns it.
      [
        { kind: "card", cardId: "a", key: "src/b.ts#function:b/call[0]", owner: "src/b.ts#function:b" },
        { kind: "step", card: "a", key: "src/b.ts#function:b/call[0]", owner: "src/b.ts#function:b" },
      ],
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
    expect(engine.windowCalls).toEqual([{ command: "report_view", args: { selection: { kind: "none" }, visible: [], tabs: [REF] } }]);

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
