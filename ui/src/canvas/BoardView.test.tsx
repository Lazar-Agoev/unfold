// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { Card, Connection } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import * as store from "../state/boards/store";
import { installFakeEngine, fixture, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { markWorking, resetWorking } from "../state/boards/agent/working";
import { BoardView } from "../board/BoardView";

const createOrder = fixture("card", "card_create_order");
const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const KEY = "src/routes/orders.ts#createOrder";
const GONE = "key-gone";

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

describe("cards on the board view", () => {
  it("renders cards, and zoom and Fit move the camera without changing what a card shows", async () => {
    store.adoptBoardState(state([{ id: "create-order", ref: { block: KEY }, at: [0, 0] }]));
    const { container } = renderBoard();
    await waitFor(() => {
      expect(container.querySelector('[data-card-id="create-order"]:not([data-loading])')).not.toBeNull();
    });
    const card = () => container.querySelector('[data-card-id="create-order"]')?.innerHTML;
    const before = card();
    const viewport = () => container.querySelector<HTMLElement>(".react-flow__viewport")?.style.transform;
    const cameraBefore = viewport();
    const dock = screen.getByRole("toolbar", { name: "Board" });
    await act(async () => {
      fireEvent.click(within(dock).getByRole("button", { name: "Zoom in" }));
      await new Promise((done) => setTimeout(done, 700));
    });
    expect(viewport()).not.toBe(cameraBefore);
    expect(screen.getByText(/^\d+%$/).textContent).not.toBe("100%");
    await act(async () => {
      fireEvent.click(within(dock).getByRole("button", { name: "Fit" }));
      fireEvent.click(within(dock).getByRole("button", { name: "Zoom out" }));
      await new Promise((done) => setTimeout(done, 700));
    });
    expect(card()).toBe(before);
  });

  it("shows an agent at work: the top bar says so, the cards it changed are outlined, and its words carry the mark", async () => {
    const description = { text: "Takes an order.", author: "agent:claude", signature: "" };
    store.adoptBoardState(
      state([
        { id: "create-order", ref: { block: KEY }, at: [0, 0], description },
        { id: "gone", ref: { block: GONE }, at: [900, 0] },
      ]),
    );
    const { container } = renderBoard();
    await screen.findByText("Takes an order.");
    expect(container.querySelector(".card-description .agent-mark")?.getAttribute("title")).toBe("Written by Claude, an agent");
    expect(screen.queryByText(/is working…/)).toBeNull();

    act(() => {
      markWorking(PAGE, { author: "agent:claude", name: "Claude" }, ["create-order"]);
    });
    expect(screen.getByText("Claude is working…")).not.toBeNull();
    await waitFor(() => {
      expect(container.querySelector('.react-flow__node.agent-working > .board-card[data-card-id="create-order"]')).not.toBeNull();
    });
    expect(container.querySelector('.react-flow__node.agent-working [data-card-id="gone"]')).toBeNull();
  });

  it("shows a card whose key is gone as a missing card", async () => {
    store.adoptBoardState(state([{ id: "gone", ref: { block: GONE }, at: [0, 0] }]));
    const { container } = renderBoard();
    await screen.findByText("This reference is missing from the graph.");
    expect(container.querySelector('[data-card-id="gone"]')).not.toBeNull();
  });

  it("keeps each page's camera when switching pages", async () => {
    store.adoptBoardState(state([{ id: "create-order", ref: { block: KEY }, at: [0, 0] }]));
    renderBoard();
    act(() => {
      store.setCamera(PAGE, { x: 12, y: 34, zoom: 0.5 });
      store.setActivePage(REF, "page-2");
    });
    expect(await screen.findByRole("heading", { name: "Page page-2" })).not.toBeNull();
    act(() => {
      store.setActivePage(REF, "page-1");
    });
    await screen.findByRole("heading", { name: "Page page-1" });
    expect(store.getPageView(PAGE).camera).toEqual({ x: 12, y: 34, zoom: 0.5 });
  });
});
