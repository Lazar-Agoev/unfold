// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { BoardView } from "../board/BoardView";
import { Toaster } from "../components";
import { queryClient } from "../ipc/client";
import type { Card, CardView, CodeView, CodeViewArgs, Health } from "../ipc/generated";
import { engineKey, installEngineInvalidation } from "../ipc/queries";
import * as store from "../state/boards/store";
import { closeCode, getCodeOpen } from "../state/codePanel";
import { showBoard } from "../state/shownBoard";
import { installFakeEngine, fixture, type FakeEngine } from "../test/engine";
import { HEALTHY, boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";

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

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const FILE = "src/routes/orders.ts";
// Keys as the goldens write them; nothing here takes them apart.
const KEY = "src/routes/orders.ts#createOrder";
const IF = "src/routes/orders.ts#createOrder/if[0]";
const PLACE = "src/routes/orders.ts#createOrder/call:orderService.place";
const OTHER = "src/routes/orders.ts#cancelOrder";
const ENTRANCE = "entrance:server/place-an-order";

const createOrder = fixture("card", "card_create_order");
/** `createOrder`'s own lines (25–35), as the reader shows the definition: nothing marked. */
const DEFINITION = fixture("code_view", "code_view_create_order");

/** The view of `orderService.place(draft)` on line 32: its statement, the call marked. */
const PLACE_VIEW: CodeView = {
  ...DEFINITION,
  lines: DEFINITION.lines.filter((line) => line.number === 32).map((line) => ({ ...line, text: line.text.trimStart() })),
  mark: { expression: [{ line: 32, start: 14, end: 39 }] },
  path: [
    { label: FILE, showing: false },
    { label: "createOrder", key: KEY, showing: false },
    { label: "orderService.place", key: PLACE, showing: true },
  ],
  definitions: [],
};

/** The same call once the path widened to `createOrder`: its line stays marked. */
const PLACE_WIDENED: CodeView = {
  ...DEFINITION,
  mark: { lines: [32, 32], expression: [{ line: 32, start: 16, end: 41 }] },
  path: [
    { label: FILE, showing: false },
    { label: "createOrder", key: KEY, showing: true },
    { label: "orderService.place", key: PLACE, showing: false },
  ],
};

function line(number: number, text = `line ${number}`): CodeView["lines"][number] {
  return { number, text, tokens: [], calls: [] };
}

/** A whole file of `count` lines, with `cancelOrder` defined on line 2. */
function wholeFile(count: number, overrides: Partial<CodeView> = {}): CodeView {
  return {
    file: FILE,
    state: "current",
    lines: Array.from({ length: count }, (_, index) => line(index + 1)),
    path: [{ label: FILE, showing: true }],
    definitions: [
      { line: 2, key: OTHER },
      { line: 25, key: KEY },
    ],
    ...overrides,
  };
}

/** A card drawn as another definition. */
function renamed(view: CardView, key: string, label: string): CardView {
  return view.type === "definition" ? { ...view, flow: { ...view.flow, header: { ...view.flow.header, key, label } } } : view;
}

let engine: FakeEngine;
let stopInvalidation: () => void;
let answer: (args: CodeViewArgs) => CodeView;

function state(cards: Card[], health?: Health) {
  return boardState(P, ID, boardDocument([page("page-1", cards)]), health ? { health } : {});
}

const EXPANDED: Card = { id: "create", ref: { block: KEY }, at: [0, 0], view: "expanded" };

function renderBoard(cards: Card[] = [EXPANDED], health?: Health) {
  store.adoptBoardState(state(cards, health));
  showBoard(REF);
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  return render(
    <QueryClientProvider client={queryClient}>
      <div style={{ width: 1200, height: 800 }}>
        <BoardView board={REF} />
      </div>
      <Toaster />
    </QueryClientProvider>,
  );
}

async function loadedCard(container: HTMLElement): Promise<HTMLElement> {
  return waitFor(() => {
    const card = container.querySelector<HTMLElement>('[data-card-id="create"]:not([data-loading])');
    if (!card) {
      throw new Error("the card isn't loaded");
    }
    return card;
  });
}

/** An element the test needs. */
function need(element: Element | null | undefined, what: string): Element {
  if (!element) {
    throw new Error(`no ${what}`);
  }
  return element;
}

function panel(): HTMLElement {
  return screen.getByRole("complementary", { name: "Code panel" });
}

function numbers(): string[] {
  return [...panel().querySelectorAll(".code-number")].map((item) => item.textContent);
}

function codeViews(): CodeViewArgs[] {
  return engine.callsOf("code_view") as unknown as CodeViewArgs[];
}

/** Clicks the card header's `</>`. */
async function openFromHeader(container: HTMLElement): Promise<void> {
  const card = await loadedCard(container);
  const button = card.querySelector<HTMLElement>(".card-view-code");
  if (!button) {
    throw new Error("no </> on the header");
  }
  fireEvent.click(button);
  await waitFor(() => {
    expect(numbers().length).toBeGreaterThan(0);
  });
}

/** Clicks the `</>` of a block drawn in the card. */
async function openBlock(container: HTMLElement, key: string): Promise<void> {
  await loadedCard(container);
  const button = container.querySelector<HTMLElement>(`[data-key="${key}"] > .flow-code`);
  if (!button) {
    throw new Error(`no </> on ${key}`);
  }
  fireEvent.click(button);
  await waitFor(() => {
    expect(numbers().length).toBeGreaterThan(0);
  });
}

function rowOf(number: number): HTMLElement {
  const row = [...panel().querySelectorAll<HTMLElement>(".code-line")].find((item) => item.querySelector(".code-number")?.textContent === String(number));
  if (!row) {
    throw new Error(`no line ${number}`);
  }
  return row;
}

function place(element: Element | null | undefined): string {
  const style = (element as HTMLElement | null | undefined)?.style;
  return style ? `${style.left},${style.top},${style.width},${style.height}` : "none";
}

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  closeCode();
  answer = (args) => {
    if (args.show?.kind === "block") {
      return PLACE_WIDENED;
    }
    if (args.show?.kind === "file") {
      return wholeFile(40, { mark: { lines: [32, 32], expression: [] }, path: PLACE_WIDENED.path.map((step, index) => ({ ...step, showing: index === 0 })) });
    }
    if (args.target.kind === "block" && args.target.key === PLACE) {
      return PLACE_VIEW;
    }
    if (args.target.kind === "lines") {
      return { ...DEFINITION, lines: [line(26, "const draft: OrderDraft = req.body;")], path: [{ label: FILE, showing: false }, { label: "line 26", showing: true }], definitions: [] };
    }
    return DEFINITION;
  };
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => {
        if (key === KEY) {
          return createOrder;
        }
        if (key === ENTRANCE) {
          return fixture("card", "card_place_an_order");
        }
        if (key === OTHER) {
          return renamed(createOrder, OTHER, "cancelOrder");
        }
        // Rejections are plain {code, message} data, as from the real preload.
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw { code: "not_found", message: "no such key" };
      },
      code_view: (args) => answer(args),
      open_in_editor: () => null,
      list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
      save_board: () => state([]),
    },
  });
  stopInvalidation = installEngineInvalidation(queryClient);
});

afterEach(() => {
  act(() => {
    closeCode();
  });
  store.resetBoards();
  stopInvalidation();
  engine.uninstall();
});

describe("the code panel", () => {
  it("opens from a card's `</>` beside the top bar, full height, with the card's lines and their numbers", async () => {
    const { container } = renderBoard();
    expect(screen.queryByRole("complementary", { name: "Code panel" })).toBeNull();
    await openFromHeader(container);
    expect(codeViews().at(-1)).toEqual({ project: P, target: { kind: "block", key: KEY } });
    // It sits beside the top bar and the stage, not under the top bar.
    expect(panel().parentElement?.classList.contains("board-view")).toBe(true);
    expect(panel().previousElementSibling?.querySelector(".board-topbar")).not.toBeNull();
    expect(numbers()).toEqual(["25", "26", "27", "28", "29", "30", "31", "32", "33", "34", "35"]);
    expect(rowOf(28).querySelector(".code-text")?.textContent).toBe("    const job = await ordersQueue.add(\"place-order\", { order: draft });");
    expect(rowOf(25).querySelector(".tok-keyword")?.textContent).toBe("export");
    // A definition card's view is the block itself: nothing is marked.
    expect(panel().querySelectorAll(".code-line.marked, .code-mark")).toHaveLength(0);
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "create", key: null });
  });

  it("closes on × and on Escape", async () => {
    const { container } = renderBoard();
    await openFromHeader(container);
    fireEvent.click(within(panel()).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("complementary", { name: "Code panel" })).toBeNull();
    await openFromHeader(container);
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.queryByRole("complementary", { name: "Code panel" })).toBeNull();
  });

  it("marks a call's expression, and the path widens the view while the selection stays marked and selected", async () => {
    const { container } = renderBoard();
    await openBlock(container, PLACE);
    expect(codeViews().at(-1)).toEqual({ project: P, target: { kind: "block", key: PLACE } });
    expect(numbers()).toEqual(["32"]);
    expect([...panel().querySelectorAll(".code-mark")].map((item) => item.textContent).join("")).toBe("orderService.place(draft)");
    const path = panel().querySelector(".code-path");
    expect([...(path?.querySelectorAll(".code-crumb") ?? [])].map((item) => item.textContent)).toEqual([FILE, "createOrder", "orderService.place"]);
    expect(path?.querySelector(".code-crumb.showing")?.textContent).toBe("orderService.place");

    fireEvent.click(within(panel()).getByRole("button", { name: "createOrder" }));
    await waitFor(() => {
      expect(numbers()).toHaveLength(11);
    });
    expect(codeViews().at(-1)).toEqual({ project: P, target: { kind: "block", key: PLACE }, show: { kind: "block", key: KEY } });
    expect(panel().querySelector(".code-crumb.showing")?.textContent).toBe("createOrder");
    expect([...panel().querySelectorAll(".code-line.marked .code-number")].map((item) => item.textContent)).toEqual(["32"]);
    expect([...panel().querySelectorAll(".code-mark")].map((item) => item.textContent).join("")).toBe("orderService.place(draft)");
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "create", key: PLACE });

    fireEvent.click(within(panel()).getByRole("button", { name: FILE }));
    await waitFor(() => {
      expect(numbers()).toHaveLength(40);
    });
    expect(codeViews().at(-1)).toEqual({ project: P, target: { kind: "block", key: PLACE }, show: { kind: "file", path: FILE } });

    fireEvent.click(within(panel()).getByRole("button", { name: "orderService.place" }));
    await waitFor(() => {
      expect(numbers()).toEqual(["32"]);
    });
    expect(getCodeOpen()?.show).toBeNull();
  });

  it.each([
    ["changed", "Changed since the last compile · showing the whole file"],
    ["doesNotParse", "Doesn't parse · showing the whole file"],
    ["notAnalyzed", "Not analyzed"],
  ] as const)("says when the file is %s, and marks nothing", async (stateName, text) => {
    answer = () => ({ ...PLACE_WIDENED, state: stateName });
    const { container } = renderBoard();
    await openBlock(container, PLACE);
    expect(within(panel()).getByRole("status").textContent).toBe(text);
    expect(numbers()).toHaveLength(11);
    expect(panel().querySelectorAll(".code-line.marked, .code-mark")).toHaveLength(0);
  });

  it("says when the block isn't found in the code, with the card's repairs", async () => {
    answer = () => ({ state: "notFound", lines: [], path: [], definitions: [] });
    const health: Health = { ...HEALTHY, missing: 1, items: [{ page: "page-1", kind: "missing", target: { card: "create" }, message: "createOrder is missing from the graph.", repairs: ["remove"] }] };
    const { container } = renderBoard([EXPANDED], health);
    const card = await loadedCard(container);
    fireEvent.click(need(card.querySelector(".card-view-code"), "</>"));
    const banner = await within(await screen.findByRole("complementary", { name: "Code panel" })).findByRole("status");
    expect(within(banner).getByText("Not found in the code")).not.toBeNull();
    fireEvent.click(within(banner).getByRole("button", { name: "Review" }));
    expect(await screen.findByText("createOrder is missing from the graph.")).not.toBeNull();
    expect(within(panel()).getByRole("button", { name: "Open in editor" }).hasAttribute("disabled")).toBe(true);
  });

  it("re-requests its view when the compile catches up, and crops again", async () => {
    answer = () => ({ ...PLACE_WIDENED, state: "changed" });
    const { container } = renderBoard();
    await openBlock(container, PLACE);
    expect(within(panel()).getByRole("status").textContent).toBe("Changed since the last compile · showing the whole file");
    answer = () => PLACE_VIEW;
    act(() => {
      engine.emit({ type: "graph_updated", project: P, changedFiles: [FILE], removedFiles: [], changedKeys: [], everything: false });
    });
    await waitFor(() => {
      expect(numbers()).toEqual(["32"]);
    });
    expect(within(panel()).queryByRole("status")).toBeNull();
  });

  it("opens its file in the editor at the marked line, and says why it couldn't", async () => {
    const { container } = renderBoard();
    await openBlock(container, PLACE);
    fireEvent.click(within(panel()).getByRole("button", { name: "Open in editor" }));
    await waitFor(() => {
      expect(engine.callsOf("open_in_editor")).toEqual([{ project: P, path: FILE, line: 32 }]);
    });
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        card: () => createOrder,
        code_view: (args) => answer(args),
        open_in_editor: () => {
          // Rejections are plain {code, message} data, as from the real preload.
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "editor_not_found", message: "The editor command `zed` wasn't found." };
        },
      },
    });
    fireEvent.click(within(panel()).getByRole("button", { name: "Open in editor" }));
    expect(await screen.findByText("The editor command `zed` wasn't found.")).not.toBeNull();
  });

  it("renders a long file in pages of 200 lines as they scroll into view, starting at its mark", async () => {
    answer = () => wholeFile(450);
    const { container } = renderBoard();
    await openFromHeader(container);
    expect(numbers()).toHaveLength(200);
    expect(numbers().at(-1)).toBe("200");
    const scroller = panel().querySelector<HTMLElement>(".code-lines");
    if (!scroller) {
      throw new Error("no lines");
    }
    Object.defineProperty(scroller, "scrollTop", { configurable: true, value: 4000 });
    fireEvent.scroll(scroller);
    await waitFor(() => {
      expect(numbers()).toContain("400");
    });
    expect(numbers()).not.toContain("401");

    act(() => {
      closeCode();
    });
    queryClient.removeQueries({ queryKey: ["engine", "code_view"] });
    answer = () => wholeFile(450, { mark: { lines: [300, 302], expression: [] } });
    await openFromHeader(container);
    await waitFor(() => {
      expect(numbers()).toContain("300");
    });
    expect([...panel().querySelectorAll(".code-line.marked .code-number")].map((item) => item.textContent)).toEqual(["300", "301", "302"]);
  });

  it("starts each new view at its left, with its line numbers showing", async () => {
    const { container } = renderBoard();
    await openFromHeader(container);
    const scroller = panel().querySelector<HTMLElement>(".code-lines");
    if (!scroller) {
      throw new Error("no lines");
    }
    // jsdom doesn't scroll: a writable value stands in for the scrolled-right lines.
    Object.defineProperty(scroller, "scrollLeft", { configurable: true, writable: true, value: 300 });
    act(() => {
      store.select(PAGE, { kind: "card", cardId: "create", key: PLACE });
    });
    await waitFor(() => {
      expect(numbers()).toEqual(["32"]);
    });
    expect(scroller.scrollLeft).toBe(0);
  });
});

describe("the panel and the canvas", () => {
  it("follows the selection while open, stays for what has no source, and doesn't open while closed", async () => {
    const { container } = renderBoard();
    await loadedCard(container);
    act(() => {
      store.select(PAGE, { kind: "card", cardId: "create", key: IF });
    });
    expect(screen.queryByRole("complementary", { name: "Code panel" })).toBeNull();
    await openFromHeader(container);
    act(() => {
      store.select(PAGE, { kind: "card", cardId: "create", key: PLACE });
    });
    await waitFor(() => {
      expect(numbers()).toEqual(["32"]);
    });
    expect(getCodeOpen()?.target).toEqual({ kind: "block", key: PLACE });
    act(() => {
      store.select(PAGE, { kind: "none" });
    });
    act(() => {
      store.select(PAGE, { kind: "note", noteId: "n" });
    });
    expect(getCodeOpen()?.target).toEqual({ kind: "block", key: PLACE });
    expect(numbers()).toEqual(["32"]);
  });

  it("outlines the selected block, lights a hovered line's block or its gap tick, and a click on a line selects without moving", async () => {
    const { container } = renderBoard();
    await openFromHeader(container);
    const call = container.querySelector(`.flow-call[data-key="${PLACE}"]`);

    fireEvent.mouseOver(need(rowOf(32).querySelector(".code-number"), "number"));
    await waitFor(() => {
      expect(place(container.querySelector(".flow-outline.lit"))).toBe(place(call));
    });
    fireEvent.mouseOver(need(rowOf(26).querySelector(".code-number"), "number"));
    await waitFor(() => {
      expect(place(container.querySelector(".flow-outline.lit"))).toBe(place(container.querySelector(".flow-gap")));
    });
    // The definition's own line lights its start.
    fireEvent.mouseOver(need(rowOf(25).querySelector(".code-number"), "number"));
    await waitFor(() => {
      expect(place(container.querySelector(".flow-outline.lit"))).toBe(place(container.querySelector(".flow-pill.start")));
    });
    fireEvent.mouseLeave(need(panel().querySelector(".code-lines"), "lines"));
    await waitFor(() => {
      expect(container.querySelector(".flow-outline.lit")).toBeNull();
    });

    const before = codeViews().length;
    fireEvent.click(need(rowOf(32).querySelector(".code-number"), "number"));
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "create", key: PLACE });
    await waitFor(() => {
      expect(place(container.querySelector(".flow-outline:not(.lit)"))).toBe(place(call));
    });
    expect(getCodeOpen()?.target).toEqual({ kind: "block", key: KEY });
    expect(codeViews()).toHaveLength(before);
    expect(numbers()).toHaveLength(11);
  });

  it("lights a compact card when a hovered line is on a block it holds", async () => {
    const { container } = renderBoard([{ id: "create", ref: { block: KEY }, at: [0, 0] }]);
    await openFromHeader(container);
    fireEvent.mouseOver(need(rowOf(32).querySelector(".code-number"), "number"));
    await waitFor(() => {
      expect(container.querySelector('[data-card-id="create"]')?.classList.contains("lit")).toBe(true);
    });
  });

  it("opens a gap tick's lines, outlining the tick", async () => {
    const { container } = renderBoard();
    await loadedCard(container);
    fireEvent.click(need(container.querySelector(".flow-gap"), "gap tick"));
    await waitFor(() => {
      expect(numbers()).toEqual(["26"]);
    });
    expect(codeViews().at(-1)).toEqual({ project: P, target: { kind: "lines", path: FILE, lines: [26, 26] } });
    expect(place(container.querySelector(".flow-outline:not(.lit)"))).toBe(place(container.querySelector(".flow-gap")));
  });

  it("opens View code from a block's menu", async () => {
    const { container } = renderBoard();
    await loadedCard(container);
    fireEvent.contextMenu(need(container.querySelector(`.flow-call[data-key="${PLACE}"]`), "call"));
    const menu = await screen.findByRole("menu", { name: "Block actions" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["View code"]);
    expect(screen.queryByRole("menu", { name: "Card actions" })).toBeNull();
    fireEvent.click(within(menu).getByRole("menuitem", { name: "View code" }));
    await waitFor(() => {
      expect(numbers()).toEqual(["32"]);
    });
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "create", key: PLACE });
  });

  it("adds a definition with no card on the page from `⊕`, and leaves the panel where it is", async () => {
    answer = () => wholeFile(40);
    const { container } = renderBoard();
    await openFromHeader(container);
    // createOrder has its card; cancelOrder doesn't.
    expect(within(panel()).getAllByRole("button", { name: /^Add the definition/ }).map((item) => item.getAttribute("aria-label"))).toEqual(["Add the definition on line 2 to the page"]);
    fireEvent.click(within(panel()).getByRole("button", { name: "Add the definition on line 2 to the page" }));
    await waitFor(() => {
      expect(store.getOpenBoard(REF)?.document.pages[0]?.cards.map((card) => card.ref)).toEqual([{ block: KEY }, { block: OTHER }]);
    });
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[1]?.id).toBe("cancel-order");
    await waitFor(() => {
      expect(within(panel()).queryByRole("button", { name: /^Add the definition/ })).toBeNull();
    });
    expect(getCodeOpen()?.target).toEqual({ kind: "block", key: KEY });
  });

  it("opens an entrance card's lines in unfold.toml", async () => {
    answer = () => ({
      file: "unfold.toml",
      state: "current",
      lines: [line(6, '{ name = "Place an order", on = "POST /orders/{order_id}" }')],
      path: [
        { label: "unfold.toml", showing: false },
        { label: "Shop API", key: "app:server", showing: false },
        { label: "Place an order", key: ENTRANCE, showing: true },
      ],
      definitions: [],
    });
    const { container } = renderBoard([{ id: "start", ref: { entrance: ENTRANCE }, at: [0, 0] }]);
    const card = await waitFor(() => need(container.querySelector('.start-card[data-card-id="start"] .card-view-code'), "</>"));
    fireEvent.click(card);
    await waitFor(() => {
      expect(numbers()).toEqual(["6"]);
    });
    expect(codeViews().at(-1)).toEqual({ project: P, target: { kind: "start", key: ENTRANCE } });
    fireEvent.click(within(panel()).getByRole("button", { name: "Shop API" }));
    await waitFor(() => {
      expect(codeViews().at(-1)).toEqual({ project: P, target: { kind: "start", key: ENTRANCE }, show: { kind: "start", key: "app:server" } });
    });
  });
});
