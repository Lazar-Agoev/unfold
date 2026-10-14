// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { Dialog } from "../components";
import { queryClient } from "../ipc/client";
import type { Card, Connection } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import * as store from "../state/boards/store";
import { installFakeEngine, fixture, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { resetWorking } from "../state/boards/agent/working";
import { BoardView } from "./BoardView";

const createOrder = fixture("card", "card_create_order");
const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const KEY = "src/routes/orders.ts#createOrder";

if (!("DOMMatrixReadOnly" in globalThis)) {
  // React Flow reads the viewport transform through DOMMatrixReadOnly; jsdom lacks it.
  class Matrix {
    m22: number;
    constructor(transform?: string) {
      const scale = /scale\(([\d.]+)\)/.exec(transform ?? "");
      this.m22 = scale ? Number(scale[1]) : 1;
    }
  }
  Object.defineProperty(globalThis, "DOMMatrixReadOnly", { configurable: true, value: Matrix });
}

if (typeof globalThis.requestAnimationFrame !== "function") {
  // d3's camera transitions run on animation frames.
  globalThis.requestAnimationFrame = (callback) => setTimeout(() => { callback(performance.now()); }, 16);
  globalThis.cancelAnimationFrame = (handle) => { clearTimeout(handle); };
}

let engine: FakeEngine;
/** Why `save_board` fails, while it does. */
let saveFailure: string | null = null;
/** The board as its file holds it (`reload_board`). */
let onDisk = boardState(P, ID);

function state(cards: Card[], links: Connection[] = []) {
  return boardState(P, ID, boardDocument([page("page-1", cards, { links }), page("page-2")]));
}

function renderBoard() {
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  return render(
    <QueryClientProvider client={queryClient}>
      <div style={{ width: 1200, height: 800 }}>
        <BoardView board={REF} />
      </div>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  saveFailure = null;
  onDisk = boardState(P, ID);
  store.resetBoards();
  resetWorking();
  queryClient.clear();
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => {
        if (key === KEY) {
          return createOrder;
        }
        // Rejections are plain {code, message} data, as from the real preload.
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw { code: "not_found", message: "no such key" };
      },
      list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
      reload_board: () => onDisk,
      callers: () => fixture("callers", "callers_place"),
      save_board: () => {
        if (saveFailure !== null) {
          // Rejections are plain {code, message} data, as from the real preload.
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "io", message: saveFailure };
        }
        return state([]);
      },
    },
  });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("the board view", () => {
  it("applies Edit › Undo and Redo to the board while it is shown", async () => {
    store.adoptBoardState(state([]));
    const { unmount } = renderBoard();
    await screen.findByRole("heading", { name: "Page page-1" });
    act(() => {
      store.editBoard(REF, (document) => {
        document.title = "Order flow";
      });
    });
    act(() => {
      engine.emit({ type: "menu_command", command: "undo" });
    });
    expect(store.getOpenBoard(REF)?.document.title).not.toBe("Order flow");
    act(() => {
      engine.emit({ type: "menu_command", command: "redo" });
    });
    expect(store.getOpenBoard(REF)?.document.title).toBe("Order flow");
    // Once another board or Home shows, this board no longer hears them.
    unmount();
    engine.emit({ type: "menu_command", command: "undo" });
    expect(store.getOpenBoard(REF)?.document.title).toBe("Order flow");
  });

  it("leaves the board alone on Edit › Undo while a dialog is open over it", async () => {
    store.adoptBoardState(state([]));
    renderBoard();
    await screen.findByRole("heading", { name: "Page page-1" });
    act(() => {
      store.editBoard(REF, (document) => {
        document.title = "Order flow";
      });
    });
    const dialog = render(
      <Dialog open title="Delete page" width={400} placement="window" onClose={() => undefined}>
        <button type="button">Cancel</button>
      </Dialog>,
    );
    act(() => {
      engine.emit({ type: "menu_command", command: "undo" });
    });
    expect(store.getOpenBoard(REF)?.document.title).toBe("Order flow");
    dialog.unmount();
    act(() => {
      engine.emit({ type: "menu_command", command: "undo" });
    });
    expect(store.getOpenBoard(REF)?.document.title).not.toBe("Order flow");
  });

  it("shows the page's name and Show all calls in the top bar, with no lens, Share, or Publish, and the dock: Add, Arrange, and the camera", async () => {
    store.adoptBoardState(state([]));
    renderBoard();
    expect(await screen.findByRole("heading", { name: "Page page-1" })).not.toBeNull();
    expect(screen.getByRole("switch", { name: "Show all calls" })).not.toBeNull();
    expect(screen.queryByRole("radiogroup")).toBeNull();
    expect(screen.queryByRole("button", { name: /Share|Publish/ })).toBeNull();
    const dock = screen.getByRole("toolbar", { name: "Board" });
    expect(within(dock).getAllByRole("button").map((button) => button.getAttribute("aria-label") ?? button.textContent)).toEqual(["Add", "Arrange", "Zoom out", "Zoom in", "Fit"]);
    expect(screen.getByText("Open an entrance or a definition to start this page.")).not.toBeNull();
  });

  it("says “Not saved · Retry” when a save fails, keeping the work, and Retry writes it", async () => {
    store.adoptBoardState(state([]));
    renderBoard();
    await screen.findByRole("heading", { name: "Page page-1" });
    saveFailure = "The disk is full.";
    act(() => {
      store.editBoard(REF, (document) => {
        document.title = "Mine";
      });
    });
    await act(async () => {
      await store.saveBoard(REF);
    });
    const banner = screen.getByRole("status");
    expect(banner.textContent).toBe("Not saved ·Retry");
    expect(within(banner).getByText("Not saved ·").getAttribute("title")).toBe("The disk is full.");
    expect(store.getOpenBoard(REF)?.document.title).toBe("Mine");
    saveFailure = null;
    fireEvent.click(within(banner).getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      expect(screen.queryByText("Not saved ·")).toBeNull();
    });
    expect(engine.callsOf("save_board").at(-1)).toMatchObject({ project: P, id: ID, board: { title: "Mine" } });
  });

  it("says saving stopped while the file on disk isn't a valid board, and Save my version writes the board over it", async () => {
    store.adoptBoardState(state([]));
    renderBoard();
    await screen.findByRole("heading", { name: "Page page-1" });
    act(() => {
      store.fileChanged(REF, { kind: "invalid", message: "board file is not valid JSON at line 1, column 1" });
    });
    const banner = screen.getByRole("status");
    expect(banner.textContent).toBe("The board's file on disk isn't a valid board, so saving stopped.Save my version");
    expect(within(banner).getByText(/isn't a valid board/).getAttribute("title")).toBe("board file is not valid JSON at line 1, column 1");
    fireEvent.click(within(banner).getByRole("button", { name: "Save my version" }));
    await waitFor(() => {
      expect(screen.queryByRole("status")).toBeNull();
    });
    expect(engine.callsOf("save_board").at(-1)).toMatchObject({ project: P, id: ID, base: "r1", overwrite: true });
  });

  it("asks when the file changed while the board had edits: Keep my edits writes them, Load the file takes the file's version", async () => {
    store.adoptBoardState(state([]));
    renderBoard();
    await screen.findByRole("heading", { name: "Page page-1" });
    const theirs = boardState(P, ID, boardDocument([page("page-1"), page("page-2")], { title: "Theirs" }), { revision: "r9" });
    onDisk = theirs;
    const ask = () => {
      act(() => {
        store.editBoard(REF, (document) => {
          document.title = "Mine";
        });
        store.fileChanged(REF, { kind: "board", state: theirs });
      });
      const banner = screen.getByRole("status");
      expect(banner.textContent).toBe("The board's file changed on disk.Keep my edits·Load the file");
      return banner;
    };
    fireEvent.click(within(ask()).getByRole("button", { name: "Keep my edits" }));
    await waitFor(() => {
      expect(screen.queryByRole("status")).toBeNull();
    });
    expect(engine.callsOf("save_board").at(-1)).toMatchObject({ project: P, id: ID, overwrite: true, board: { title: "Mine" } });

    fireEvent.click(within(ask()).getByRole("button", { name: "Load the file" }));
    await waitFor(() => {
      expect(screen.queryByRole("status")).toBeNull();
    });
    expect(engine.callsOf("reload_board")).toEqual([{ project: P, id: ID }]);
    expect(store.getOpenBoard(REF)?.document.title).toBe("Theirs");
  });
});
