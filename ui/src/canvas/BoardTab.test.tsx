// @vitest-environment jsdom
import "../test/dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Card } from "../ipc/generated";
import { UnfoldError } from "../ipc/unfold";
import { ActionIds, actions } from "../keyboard";
import { fixture } from "../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";

const engine = vi.hoisted(() => ({ call: vi.fn(), onEngineEvent: vi.fn(() => () => undefined), windowCommand: vi.fn() }));
vi.mock("../state/boards/engine", () => engine);

const store = await import("../state/boards/store");
const { engineKey } = await import("../ipc/queries");
const { BoardTab } = await import("./BoardTab");

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

function state(cards: Card[]) {
  return boardState(P, ID, boardDocument([page("page-1", cards)]));
}

function renderBoard() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  return render(
    <QueryClientProvider client={client}>
      <div style={{ width: 1200, height: 800 }}>
        <BoardTab board={REF} active />
      </div>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  store.resetBoards();
  engine.call.mockReset();
  engine.call.mockImplementation((command: string, args: { project?: string; key?: string }) => {
    if (command === "card" && args.project === P && args.key === KEY) return Promise.resolve(createOrder);
    if (command === "card") return Promise.reject(new UnfoldError({ code: "not_found", message: "no such key" }));
    return Promise.resolve(null);
  });
});

describe("the board tab", () => {
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
    await act(async () => {
      actions.run(ActionIds.fit);
      actions.run(ActionIds.zoomOut);
      await new Promise((done) => setTimeout(done, 700));
    });
    expect(card()).toBe(before);
  });

  it("shows a card whose key is gone as a placeholder", async () => {
    store.adoptBoardState(state([{ id: "gone", ref: { block: GONE }, at: [0, 0] }]));
    const { container } = renderBoard();
    await screen.findByText("This reference is missing from the graph.");
    expect(container.querySelector(".block-card.placeholder")).not.toBeNull();
  });

  it("shows a card of a project that isn't connected as a placeholder", async () => {
    store.adoptBoardState(state([{ id: "pay", ref: { block: KEY, project: "payments" }, at: [0, 0] }]));
    const { container } = renderBoard();
    await screen.findByText("From payments · not connected");
    expect(container.querySelector('.placeholder[data-card-id="pay"]')).not.toBeNull();
    expect(engine.call).not.toHaveBeenCalledWith("card", expect.anything());
  });

  it("switches a card's own view as a board edit, and the page's lens without one", async () => {
    store.adoptBoardState(state([{ id: "create-order", ref: { block: KEY }, at: [0, 0] }]));
    const { container } = renderBoard();
    await waitFor(() => {
      expect(container.querySelector(".code-view")).not.toBeNull();
    });
    act(() => {
      store.setPageLens(PAGE, "flow");
    });
    await waitFor(() => {
      expect(container.querySelector(".flow-view")).not.toBeNull();
    });
    expect(store.getOpenBoard(REF)?.edits).toBe(0);
    act(() => {
      screen.getByRole("radio", { name: "Code" }).click();
    });
    await waitFor(() => {
      expect(container.querySelector(".code-view")).not.toBeNull();
    });
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.view).toBe("code");
    expect(store.getOpenBoard(REF)?.edits).toBe(1);
  });
});
