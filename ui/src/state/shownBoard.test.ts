// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page } from "../test/workspace";
import { canUndo, getOpenBoard, openBoard, undo } from "./boards";
import { closeBoard } from "./boards/open";
import { editBoard, resetBoards } from "./boards/store";
import { getShownBoard, hideBoard, reportShownBoard, showBoard, showHome, useShownBoardStore } from "./shownBoard";

const P = "p-orders0000";
const A = { project: P, id: ".unfold/boards/a.json" };
const B = { project: P, id: ".unfold/boards/b.json" };

let engine: FakeEngine;

beforeEach(() => {
  engine = installFakeEngine({
    handlers: {
      open_board: ({ project, id }) => boardState(project, id, boardDocument([page("page-1"), page("page-2")])),
      save_board: ({ project, id, board }) => boardState(project, id, boardDocument([page("page-1"), page("page-2")], { title: board.title }), { revision: "r2" }),
      close_board: () => null,
    },
  });
  resetBoards();
  useShownBoardStore.setState({ board: null });
});

afterEach(() => {
  engine.uninstall();
});

/** What the window reported to the main process, in order. */
function reports(): unknown[] {
  return engine.windowCalls.filter((entry) => entry.command === "report_shown_board").map((entry) => entry.args);
}

describe("the shown board", () => {
  it("switches between Home and one board at a time", () => {
    expect(getShownBoard()).toBeNull();
    showBoard(A);
    expect(getShownBoard()).toEqual(A);
    showBoard(B);
    expect(getShownBoard()).toEqual(B);
    showHome();
    expect(getShownBoard()).toBeNull();
  });

  it("shows Home when the board in front closes, and nothing changes when another one does", () => {
    showBoard(B);
    hideBoard(A);
    expect(getShownBoard()).toEqual(B);
    hideBoard(B);
    expect(getShownBoard()).toBeNull();
  });

  it("reports what it shows now, then each change, until stopped", () => {
    showBoard(A);
    const stop = reportShownBoard();
    showBoard(A);
    showHome();
    showHome();
    showBoard(B);
    stop();
    showHome();
    expect(reports()).toEqual([{ board: { project: P, board: A.id } }, { board: null }, { board: { project: P, board: B.id } }]);
  });
});

describe("boards the window opens", () => {
  it("each keep their session and undo history while another board or Home shows", async () => {
    await openBoard(A, { page: "page-2" });
    editBoard(A, (draft) => {
      draft.title = "Order flow";
    });
    await openBoard(B);
    expect(getShownBoard()).toEqual(B);
    showHome();
    await openBoard(A);
    expect(engine.callsOf("open_board")).toEqual([
      { project: P, id: A.id },
      { project: P, id: B.id },
    ]);
    expect(getShownBoard()).toEqual(A);
    expect(getOpenBoard(A)?.activePage).toBe("page-2");
    expect(getOpenBoard(A)?.document.title).toBe("Order flow");
    expect(canUndo(A)).toBe(true);
    undo(A);
    expect(getOpenBoard(A)?.document.title).not.toBe("Order flow");
    expect(getOpenBoard(B)).toBeDefined();
  });

  it("opens one in the background without showing it", async () => {
    showBoard(A);
    await openBoard(B, { background: true });
    expect(getOpenBoard(B)).toBeDefined();
    expect(getShownBoard()).toEqual(A);
  });

  it("closes one only on request, saving it first, and Home shows if it was in front", async () => {
    await openBoard(A);
    editBoard(A, (draft) => {
      draft.title = "Order flow";
    });
    await closeBoard(A);
    expect(engine.callsOf("save_board")).toHaveLength(1);
    expect(engine.callsOf("close_board")).toEqual([{ project: P, id: A.id }]);
    expect(getOpenBoard(A)).toBeUndefined();
    expect(getShownBoard()).toBeNull();
  });
});
