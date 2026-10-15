// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { queryClient } from "../ipc/client";
import type { BoardDocument, Card, CardView, DrawnFlow, Key, Note, Page, Resource } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import * as store from "../state/boards/store";
import { useLayout } from "../state/layout";
import { setShowAllCalls } from "../state/showAllCalls";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { BoardView } from "../board/BoardView";

const toast = vi.hoisted(() => ({ showToast: vi.fn() }));
vi.mock(import("../components/Toast"), async (load) => ({ ...(await load()), ...toast }));

const P = "p-shop000000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const PLACE = "shop/service.py#place_order";
const SAVE = "shop/store.py#save_order";
const AUTHOR = "person:maya";

if (!("DOMMatrixReadOnly" in globalThis)) {
  // React Flow reads the viewport transform through DOMMatrixReadOnly; jsdom lacks it.
  class Matrix {
    m22 = 1;
  }
  Object.defineProperty(globalThis, "DOMMatrixReadOnly", { configurable: true, value: Matrix });
}

/** A ResizeObserver that reports each element once it is observed, so React Flow measures the nodes. */
class MeasuringResizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe(target: Element): void {
    queueMicrotask(() => {
      this.callback([{ target, contentRect: { x: 0, y: 0, width: 400, height: 220 } } as unknown as ResizeObserverEntry], this);
    });
  }
  unobserve(): void {
    // Each element is reported once.
  }
  disconnect(): void {
    // Each element is reported once.
  }
}

const original = globalThis.ResizeObserver;
beforeAll(() => {
  Object.defineProperty(globalThis, "ResizeObserver", { configurable: true, value: MeasuringResizeObserver });
});
afterAll(() => {
  Object.defineProperty(globalThis, "ResizeObserver", { configurable: true, value: original });
});

const GOLDENS = import.meta.glob<DrawnFlow>("../../../testdata/goldens/drawn/*.json", { eager: true, import: "default" });

/** place_order as drawn, or a callee's card with its key and name: a definition with no steps. */
function model(key: Key): CardView | undefined {
  const flow = GOLDENS["../../../testdata/goldens/drawn/01-place-order.json"];
  if (!flow || (key !== PLACE && key !== SAVE)) {
    return undefined;
  }
  const drawn = structuredClone(flow);
  return key === PLACE ? { type: "definition", flow: drawn } : { type: "definition", flow: { ...drawn, header: { ...drawn.header, key, label: "save_order" }, steps: { steps: [] }, routes: [] } };
}

let engine: FakeEngine;
let refuseSave: { code: string; message: string } | null;

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  setShowAllCalls(false);
  useLayout.setState({ selectedCollapsed: true });
  toast.showToast.mockClear();
  refuseSave = null;
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => {
        const found = model(key);
        if (!found) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "no such key" };
        }
        return found;
      },
      callers: ({ key }) => ({ project: P, key, total: 1, callers: [{ key: PLACE, name: "place_order", qualifiedName: "place_order", path: "shop/service.py", line: 10, sites: 1 }] }),
      list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
      save_board: ({ board }) => {
        if (refuseSave) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw refuseSave;
        }
        return boardState(P, ID, boardDocument(board.pages as BoardDocument["pages"]));
      },
    },
  });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

const PLACE_CARD: Card = { id: "place", ref: { block: PLACE }, at: [0, 0] };
const SAVE_CARD: Card = { id: "save", ref: { block: SAVE }, at: [900, 0] };

function open(cards: Card[], overrides: Partial<Page> = {}) {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", cards, overrides)])));
}

function renderBoard() {
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "shop")]));
  // Who you are is known before any menu opens: Edit and the hand edits sign with it.
  queryClient.setQueryData(engineKey("app_info", {}), { username: "maya", engineVersion: "1" });
  return render(
    <QueryClientProvider client={queryClient}>
      <div style={{ width: 1200, height: 800 }}>
        <BoardView board={REF} />
      </div>
    </QueryClientProvider>,
  );
}

function present<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) {
    throw new Error("not drawn yet");
  }
  return value;
}

function shownPage(): Page {
  return present(store.getOpenBoard(REF)?.document.pages[0]);
}

/** Opens the menu named `menu` on `element` and picks `item`. */
async function choose(element: HTMLElement, menu: string, item: string): Promise<void> {
  fireEvent.contextMenu(element);
  const opened = await screen.findByRole("menu", { name: menu });
  act(() => {
    within(opened).getByRole("menuitem", { name: item }).click();
  });
}

/** The links Copy link put on the clipboard. */
function copied(): unknown[] {
  return engine.windowCalls.filter((entry) => entry.command === "copy_to_clipboard").map((entry) => entry.args);
}

const card = (container: HTMLElement, id: string) => present(container.querySelector<HTMLElement>(`.board-card[data-card-id="${id}"]:not([data-loading])`));
const resource = (container: HTMLElement, id: string) => present(container.querySelector<HTMLElement>(`.resource-card[data-resource-id="${id}"]`));
const note = (container: HTMLElement, id: string) => present(container.querySelector<HTMLElement>(`.board-note[data-note-id="${id}"]`));

describe("a card's menu", () => {
  it("Show callers adds the cards that call it beside it, as one undo step", async () => {
    open([SAVE_CARD]);
    const { container } = renderBoard();
    await choose(await waitFor(() => card(container, "save")), "Card actions", "Show callers");
    await waitFor(() => {
      expect(shownPage().cards.map((item) => item.ref)).toEqual([{ block: SAVE }, { block: PLACE }]);
    });
    expect(engine.callsOf("callers")).toEqual([{ project: P, key: SAVE, limit: 20 }]);
    act(() => {
      store.undo(REF);
    });
    expect(shownPage().cards).toHaveLength(1);
  });

  it("Copy link copies a link to the card", async () => {
    open([PLACE_CARD]);
    const { container } = renderBoard();
    await choose(await waitFor(() => card(container, "place")), "Card actions", "Copy link");
    await waitFor(() => {
      expect(copied()).toEqual([{ text: "unfold://shop/.unfold/boards/b.json?page=page-1&item=place" }]);
    });
  });

  it("Remove from page takes the card off the page, as one undo step", async () => {
    open([PLACE_CARD, SAVE_CARD]);
    const { container } = renderBoard();
    await choose(await waitFor(() => card(container, "save")), "Card actions", "Remove from page");
    expect(shownPage().cards.map((item) => item.id)).toEqual(["place"]);
    act(() => {
      store.undo(REF);
    });
    expect(shownPage().cards.map((item) => item.id)).toEqual(["place", "save"]);
  });
});

describe("a resource card's menu", () => {
  const orders: Resource = { id: "orders", kind: "queue", name: "orders", anchors: [], at: [0, 600], author: AUTHOR };

  it("lists Describe…, Draw link…, Copy link, and Remove resource card", async () => {
    open([PLACE_CARD], { resources: [orders] });
    const { container } = renderBoard();
    fireEvent.contextMenu(await waitFor(() => resource(container, "orders")));
    const menu = await screen.findByRole("menu", { name: "Resource card actions" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Describe…", "Draw link…", "Copy link", "Remove resource card"]);
  });

  it("Describe… selects it and opens Selected at its description", async () => {
    open([PLACE_CARD], { resources: [orders] });
    const { container } = renderBoard();
    await choose(await waitFor(() => resource(container, "orders")), "Resource card actions", "Describe…");
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "resource", resourceId: "orders" });
    expect(useLayout.getState().selectedCollapsed).toBe(false);
  });

  it("Draw link… starts a link from it", async () => {
    open([PLACE_CARD], { resources: [orders] });
    const { container } = renderBoard();
    await choose(await waitFor(() => resource(container, "orders")), "Resource card actions", "Draw link…");
    expect(store.getPageView(PAGE).linking).toEqual({ from: "orders", to: null });
  });

  it("Copy link copies a link to it", async () => {
    open([PLACE_CARD], { resources: [orders] });
    const { container } = renderBoard();
    await choose(await waitFor(() => resource(container, "orders")), "Resource card actions", "Copy link");
    await waitFor(() => {
      expect(copied()).toEqual([{ text: "unfold://shop/.unfold/boards/b.json?page=page-1&item=orders" }]);
    });
  });

  it("Remove resource card takes it off the page, as one undo step", async () => {
    open([PLACE_CARD], { resources: [orders] });
    const { container } = renderBoard();
    await choose(await waitFor(() => resource(container, "orders")), "Resource card actions", "Remove resource card");
    expect(shownPage().resources ?? []).toEqual([]);
    act(() => {
      store.undo(REF);
    });
    expect(shownPage().resources).toEqual([orders]);
  });
});

describe("a note's menu", () => {
  const reading: Note = { id: "reading", text: "Read the worker next.", at: [0, 600], author: "agent:claude" };

  it("Edit rewrites it in place, which makes it yours", async () => {
    open([PLACE_CARD], { notes: [reading] });
    const { container } = renderBoard();
    await choose(await waitFor(() => note(container, "reading")), "Note actions", "Edit");
    const field = within(note(container, "reading")).getByRole("textbox", { name: "Note" });
    fireEvent.change(field, { target: { value: "Read the worker first." } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(shownPage().notes).toEqual([{ ...reading, text: "Read the worker first.", author: AUTHOR }]);
  });

  it("Copy link copies a link to it", async () => {
    open([PLACE_CARD], { notes: [reading] });
    const { container } = renderBoard();
    await choose(await waitFor(() => note(container, "reading")), "Note actions", "Copy link");
    await waitFor(() => {
      expect(copied()).toEqual([{ text: "unfold://shop/.unfold/boards/b.json?page=page-1&item=reading" }]);
    });
  });

  it("Remove note takes it off the page, as one undo step", async () => {
    open([PLACE_CARD], { notes: [reading] });
    const { container } = renderBoard();
    await choose(await waitFor(() => note(container, "reading")), "Note actions", "Remove note");
    expect(shownPage().notes ?? []).toEqual([]);
    act(() => {
      store.undo(REF);
    });
    expect(shownPage().notes).toEqual([reading]);
  });
});

describe("drawing a link the board refuses", () => {
  it("says why in a toast and draws nothing", async () => {
    open([PLACE_CARD, SAVE_CARD]);
    const { container } = renderBoard();
    await choose(await waitFor(() => card(container, "save")), "Card actions", "Draw link…");
    fireEvent.click(card(container, "place"));
    const dialog = await screen.findByRole("dialog", { name: "Draw link" });
    refuseSave = { code: "board_problem", message: "page `page-1`, connection `save-other-place`: no." };
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Label" }), { target: { value: "posts to Slack" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Draw" }));
    await waitFor(() => {
      expect(toast.showToast).toHaveBeenCalledWith({ title: "Couldn't draw the link", body: refuseSave?.message, tone: "danger" });
    });
    expect(shownPage().links).toEqual([]);
    expect(store.getPageView(PAGE).linking).toBeNull();
    const board = store.getOpenBoard(REF);
    expect(board && store.isDirty(board)).toBe(false);
  });
});
