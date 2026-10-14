// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { BoardDocument, Card, CardView, DrawnFlow, Key, Page } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import * as store from "../state/boards/store";
import { setShowAllCalls } from "../state/showAllCalls";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { BoardView } from "../board/BoardView";
import { callPort } from "./arrows";

const P = "p-shop000000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const PLACE = "shop/service.py#place_order";
const SAVE = "shop/store.py#save_order";
const LOAD_CALL = `${PLACE}/call:load_order`;
const SAVE_CALL = `${PLACE}/call:save_order`;

if (!("DOMMatrixReadOnly" in globalThis)) {
  // React Flow reads the viewport transform through DOMMatrixReadOnly; jsdom lacks it.
  class Matrix {
    m22 = 1;
  }
  Object.defineProperty(globalThis, "DOMMatrixReadOnly", { configurable: true, value: Matrix });
}

/** A ResizeObserver that reports each element once it is observed, so React Flow measures the nodes and draws the arrows. */
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

function drawn(name: string): DrawnFlow {
  const flow = GOLDENS[`../../../testdata/goldens/drawn/${name}.json`];
  if (!flow) {
    throw new Error(`no golden ${name}`);
  }
  return structuredClone(flow);
}

/** A callee's card: a definition with no steps. */
function leaf(key: Key, label: string): CardView {
  const flow = drawn("02-place-order-ts");
  return { type: "definition", flow: { ...flow, header: { ...flow.header, key, label, kind: "function", form: "function" }, steps: { steps: [] }, routes: [] } };
}

/** What the reader draws: place_order shaped when the page combines its steps, else as compiled. */
let shaped = false;
function model(key: Key): CardView | undefined {
  if (key === PLACE) {
    return { type: "definition", flow: drawn(shaped ? "place-order-shaped" : "01-place-order") };
  }
  return key === SAVE ? leaf(SAVE, "save_order") : undefined;
}

let engine: FakeEngine;

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  setShowAllCalls(false);
  shaped = false;
  engine = installFakeEngine({
    handlers: {
      app_info: () => ({ username: "maya", engineVersion: "1" }),
      card: ({ key }) => {
        const found = model(key);
        if (!found) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "no such key" };
        }
        return found;
      },
      list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
      save_board: ({ board }) => boardState(P, ID, boardDocument(board.pages as BoardDocument["pages"])),
    },
  });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

const PLACE_CARD: Card = { id: "place", ref: { block: PLACE }, at: [0, 0], view: "expanded" };
const SAVE_CARD: Card = { id: "save", ref: { block: SAVE }, at: [900, 0] };

function open(cards: Card[], overrides: Partial<Page> = {}) {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", cards, overrides)])));
}

function renderBoard() {
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "shop")]));
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

const loaded = (container: HTMLElement, id: string) => present(container.querySelector<HTMLElement>(`.board-card[data-card-id="${id}"]:not([data-loading])`));

describe("a block's menu on the canvas", () => {
  it("offers what the block allows and combines from it: the popover beside the block, then the combined step drawn", async () => {
    open([PLACE_CARD]);
    const { container } = renderBoard();
    const call = await waitFor(() => present(loaded(container, "place").querySelector<HTMLElement>(`.flow-call[data-key="${LOAD_CALL}"]`)));
    fireEvent.contextMenu(call);
    const menu = await screen.findByRole("menu", { name: "Block actions" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["View code", "Open in place", "Name…", "Describe…", "Combine…", "Add proposed step after…"]);
    act(() => {
      within(menu).getByRole("menuitem", { name: "Combine…" }).click();
    });
    const dialog = await screen.findByRole("dialog", { name: "Combine steps" });
    fireEvent.click(within(dialog).getByRole("option", { name: "if" }));
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Title" }), { target: { value: "Load it, stop if missing" } });
    const combine = within(dialog).getByRole("button", { name: "Combine" });
    await waitFor(() => {
      expect(combine.hasAttribute("disabled")).toBe(false);
    });
    shaped = true;
    fireEvent.click(combine);
    await waitFor(() => {
      expect(shownPage().combined.map((item) => item.members)).toEqual([[LOAD_CALL, `${PLACE}/if[0]`]]);
    });
    expect(engine.callsOf("card").at(-1)).toMatchObject({ key: PLACE, overlay: { combined: [{ title: "Load it, stop if missing", collapsed: true }] } });
  });

  it("opens a call in place from Open in place, as one undo step", async () => {
    open([PLACE_CARD]);
    const { container } = renderBoard();
    fireEvent.contextMenu(await waitFor(() => present(loaded(container, "place").querySelector<HTMLElement>(`.flow-call[data-key="${LOAD_CALL}"]`))));
    act(() => {
      within(screen.getByRole("menu", { name: "Block actions" })).getByRole("menuitem", { name: "Open in place" }).click();
    });
    expect(shownPage().cards[0]?.open).toEqual([[LOAD_CALL]]);
    act(() => {
      store.undo(REF);
    });
    expect(shownPage().cards[0]?.open).toBeUndefined();
  });
});

describe("arrows from a collapsed combined step", () => {
  it("leave the card from the combined step that holds the call", async () => {
    shaped = true;
    open([PLACE_CARD, SAVE_CARD]);
    const { container } = renderBoard();
    await waitFor(() => {
      const ports = [...loaded(container, "place").querySelectorAll(".card-port")].map((port) => port.getAttribute("data-handleid"));
      expect(ports).toContain(callPort(SAVE_CALL));
    });
  });
});

describe("the canvas's menu", () => {
  it("has Add resource… and Fit; Add resource… opens its popover", async () => {
    open([PLACE_CARD]);
    const { container } = renderBoard();
    await waitFor(() => loaded(container, "place"));
    fireEvent.contextMenu(present(container.querySelector(".react-flow__pane")), { clientX: 300, clientY: 200 });
    const menu = await screen.findByRole("menu", { name: "Board actions" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Add resource…", "Fit"]);
    act(() => {
      within(menu).getByRole("menuitem", { name: "Add resource…" }).click();
    });
    expect(await screen.findByRole("dialog", { name: "Add resource" })).not.toBeNull();
  });
});

describe("drawing a link", () => {
  it("starts from Draw link…, ends at the next card clicked, and asks for its label: an Other connection, no step, no citation", async () => {
    open([PLACE_CARD, SAVE_CARD]);
    const { container } = renderBoard();
    fireEvent.contextMenu(await waitFor(() => loaded(container, "save")));
    act(() => {
      within(screen.getByRole("menu", { name: "Card actions" })).getByRole("menuitem", { name: "Draw link…" }).click();
    });
    expect(document.querySelector(".board-hint")?.textContent).toBe("Click what it links to · esc cancels");
    // A click where it starts waits.
    fireEvent.click(loaded(container, "save"));
    expect(screen.queryByRole("dialog", { name: "Draw link" })).toBeNull();
    fireEvent.click(loaded(container, "place"));
    const dialog = await screen.findByRole("dialog", { name: "Draw link" });
    // The label is required.
    fireEvent.click(within(dialog).getByRole("button", { name: "Draw" }));
    expect(within(dialog).getByText("Say what it is in a few words.")).not.toBeNull();
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Label" }), { target: { value: "posts to Slack" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Draw" }));
    await waitFor(() => {
      expect(shownPage().links).toEqual([{ id: "save-other-place", from: "save", to: "place", kind: "other", author: "person:maya", label: { text: "posts to Slack", author: "person:maya", signature: "" } }]);
    });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "link", linkId: "save-other-place" });
    expect(store.getPageView(PAGE).linking).toBeNull();
    act(() => {
      store.undo(REF);
    });
    expect(shownPage().links).toEqual([]);
  });

  it("lets go on Escape, and selects nothing while it waits", async () => {
    open([PLACE_CARD, SAVE_CARD]);
    const { container } = renderBoard();
    fireEvent.contextMenu(await waitFor(() => loaded(container, "save")));
    act(() => {
      within(screen.getByRole("menu", { name: "Card actions" })).getByRole("menuitem", { name: "Draw link…" }).click();
    });
    expect(store.getPageView(PAGE).linking).toEqual({ from: "save", to: null });
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(store.getPageView(PAGE).linking).toBeNull();
    expect(document.querySelector(".board-hint")).toBeNull();
  });
});
