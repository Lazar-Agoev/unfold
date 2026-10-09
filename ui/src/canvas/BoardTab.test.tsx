// @vitest-environment jsdom
import "../test/dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BoardDocument, BoardState } from "../ipc/generated";
import { UnfoldError } from "../ipc/unfold";
import { ActionIds, actions } from "../keyboard";
import { fixture } from "../test/engine";

const engine = vi.hoisted(() => ({ call: vi.fn(), onEngineEvent: vi.fn(() => () => undefined) }));
vi.mock("../state/boards/engine", () => engine);

const store = await import("../state/boards/store");
const { BoardTab } = await import("./BoardTab");

const createOrder = fixture("card", "card_create_order");
const ID = ".unfold/local/drafts/b.json";
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

function state(cards: BoardDocument["cards"]): BoardState {
  return {
    id: ID,
    place: "draft",
    name: "b",
    document: { format: "unfold-board", version: 1, title: "B", defaultView: "code", cards, groups: [], labels: {}, guards: {} },
    revision: "r1",
    health: { items: [], missing: 0, outdated: 0, brokenGroups: 0, review: 0 },
    outline: { rows: [], unlabeled: [] },
    label: null,
  };
}

function renderBoard() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <div style={{ width: 1200, height: 800 }}>
        <BoardTab boardId={ID} active />
      </div>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  store.resetBoards();
  engine.call.mockReset();
  engine.call.mockImplementation((command: string, args: { key?: string }) => {
    if (command === "card" && args.key === KEY) return Promise.resolve(createOrder);
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
      actions.run(ActionIds.fitBoard);
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

  it("switches a card's own view as a board edit, and the lens without one", async () => {
    store.adoptBoardState(state([{ id: "create-order", ref: { block: KEY }, at: [0, 0] }]));
    const { container } = renderBoard();
    await waitFor(() => {
      expect(container.querySelector(".code-view")).not.toBeNull();
    });
    act(() => {
      store.applyLensToBoards("flow");
    });
    await waitFor(() => {
      expect(container.querySelector(".flow-view")).not.toBeNull();
    });
    expect(store.getBoard(ID)?.edits).toBe(0);
    act(() => {
      screen.getByRole("radio", { name: "Code" }).click();
    });
    await waitFor(() => {
      expect(container.querySelector(".code-view")).not.toBeNull();
    });
    expect(store.getBoard(ID)?.document.cards[0]?.view).toBe("code");
    expect(store.getBoard(ID)?.edits).toBe(1);
  });
});
