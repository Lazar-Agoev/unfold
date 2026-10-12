// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { Card, Link } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import { ActionIds, actions } from "../keyboard";
import * as store from "../state/boards/store";
import { installFakeEngine, fixture, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { markWorking, resetWorking } from "../state/boards/agent/working";
import { BoardView } from "./BoardView";

const createOrder = fixture("card", "card_create_order");
const P = "p-orders0000";
const ID = ".unfold/local/drafts/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const KEY = "src/routes/orders.ts#function:createOrder";
const GONE = "src/gone.ts#function:gone";

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

function state(cards: Card[], links: Link[] = []) {
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
      callers: () => fixture("callers", "callers_create_order"),
      save_board: () => state([]),
      save_board_view: () => null,
    },
  });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("the board view", () => {
  it("renders cards, and zoom and Fit move the camera without changing what a card shows", async () => {
    store.adoptBoardState(state([{ id: "create-order", ref: { block: KEY }, at: [0, 0] }]));
    const { container } = renderBoard();
    await waitFor(() => {
      expect(container.querySelector('.block-card[data-card-id="create-order"] .code-view')).not.toBeNull();
    });
    const card = () => container.querySelector('.block-card[data-card-id="create-order"]')?.innerHTML;
    const before = card();
    const viewport = () => container.querySelector<HTMLElement>(".react-flow__viewport")?.style.transform;
    const cameraBefore = viewport();
    await act(async () => {
      actions.run(ActionIds.zoomIn);
      await new Promise((done) => setTimeout(done, 700));
    });
    expect(viewport()).not.toBe(cameraBefore);
    expect(screen.getByText(/^\d+%$/).textContent).not.toBe("100%");
    await act(async () => {
      actions.run(ActionIds.fit);
      actions.run(ActionIds.zoomOut);
      await new Promise((done) => setTimeout(done, 700));
    });
    expect(card()).toBe(before);
  });

  it("shows the page, its publish state, and the dock", async () => {
    store.adoptBoardState(state([]));
    renderBoard();
    expect(await screen.findByRole("heading", { name: "Page page-1" })).not.toBeNull();
    expect(screen.getByText("Draft")).not.toBeNull();
    const dock = screen.getByRole("toolbar", { name: "Board" });
    expect(within(dock).getByRole("button", { name: "Add" })).not.toBeNull();
    expect(within(dock).getByRole("button", { name: "Forward" })).toHaveProperty("disabled", true);
    expect(screen.getByText("Open an entrance or a definition to start this page.")).not.toBeNull();
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
    expect(container.querySelector(".block-card.missing")).not.toBeNull();
  });

  it("shows a card of a project that isn't connected as a placeholder, with who added it", async () => {
    const description = { text: "Sends the receipt.", author: "person:maya", signature: "" };
    store.adoptBoardState(state([{ id: "pay", ref: { block: KEY, project: "payments" }, at: [0, 0], description }]));
    const { container } = renderBoard();
    await screen.findByText("From payments · not connected");
    expect(screen.getByText("Maya added this card. Connect the project to read it.")).not.toBeNull();
    expect(container.querySelector('.placeholder-card[data-card-id="pay"] .placeholder-connect')?.textContent).toBe("Connect project");
    expect(engine.callsOf("card")).toHaveLength(0);
  });

  it("expands and collapses one card with its chevron as a board edit, and switches the page's lens without one", async () => {
    store.adoptBoardState(state([{ id: "create-order", ref: { block: KEY }, at: [0, 0] }]));
    const { container } = renderBoard();
    await waitFor(() => {
      expect(container.querySelector(".code-view")).not.toBeNull();
    });
    // Code has no chevron.
    expect(screen.queryByRole("button", { name: "Collapse createOrder" })).toBeNull();
    act(() => {
      actions.run(ActionIds.lensDiagram);
    });
    // The page's Code default leaves its cards expanded in Diagram: their Flow.
    await waitFor(() => {
      expect(container.querySelector(".flow-view")).not.toBeNull();
    });
    expect(store.getOpenBoard(REF)?.edits).toBe(0);
    const collapse = await screen.findByRole("button", { name: "Collapse createOrder" });
    expect(collapse.getAttribute("aria-expanded")).toBe("true");
    act(() => {
      collapse.click();
    });
    await waitFor(() => {
      expect(container.querySelector('.diagram-card[data-card-id="create-order"]')).not.toBeNull();
    });
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.view).toBe("diagram");
    expect(store.getOpenBoard(REF)?.edits).toBe(1);
    // A click on the chevron never selects the card.
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "none" });
    const expand = await screen.findByRole("button", { name: "Expand createOrder" });
    act(() => {
      expand.click();
    });
    await waitFor(() => {
      expect(container.querySelector(".flow-view")).not.toBeNull();
    });
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.view).toBe("flow");
    expect(store.getOpenBoard(REF)?.edits).toBe(2);
    // A trip through Code keeps it expanded.
    act(() => {
      actions.run(ActionIds.lensCode);
    });
    await waitFor(() => {
      expect(container.querySelector(".code-view")).not.toBeNull();
    });
    act(() => {
      actions.run(ActionIds.lensDiagram);
    });
    await waitFor(() => {
      expect(container.querySelector(".flow-view")).not.toBeNull();
    });
    expect(store.getOpenBoard(REF)?.edits).toBe(2);
  });

  it("draws a link from a card's menu to the next card clicked, and selects it", async () => {
    store.adoptBoardState(
      state([
        { id: "a", ref: { block: KEY }, at: [0, 0] },
        { id: "b", ref: { block: KEY, project: "payments" }, at: [800, 0] },
      ]),
    );
    const { container } = renderBoard();
    await screen.findByText("From payments · not connected");
    act(() => {
      store.setLinking(PAGE, "a");
    });
    expect(await screen.findByText(/Click the card to link to/)).not.toBeNull();
    act(() => {
      container.querySelector<HTMLElement>('.placeholder-card[data-card-id="b"]')?.click();
    });
    const links = store.getOpenBoard(REF)?.document.pages[0]?.links ?? [];
    expect(links).toEqual([{ id: "a-to-b", from: "a", to: "b" }]);
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "link", linkId: "a-to-b" });
    expect(store.getPageView(PAGE).linking).toBeNull();
  });

  it("cancels link drawing on Escape", async () => {
    store.adoptBoardState(state([{ id: "a", ref: { block: KEY }, at: [0, 0] }]));
    renderBoard();
    act(() => {
      store.setLinking(PAGE, "a");
    });
    await screen.findByText(/Click the card to link to/);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(store.getPageView(PAGE).linking).toBeNull();
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
