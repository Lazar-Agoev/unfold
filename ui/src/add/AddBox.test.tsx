// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SearchHit, SearchResults } from "../ipc";
import type * as Boards from "../state/boards";
import { openBoard } from "../state/boards";
import { resetBoards } from "../state/boards/store";
import { closeAdd, openAdd } from "../state/overlays";
import { showBoard, useShownBoardStore } from "../state/shownBoard";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { AddBox } from "./AddBox";

const adds = vi.hoisted(() => ({ addRef: vi.fn() }));
vi.mock("../state/boards", async (original) => ({ ...(await original<typeof Boards>()), addRef: adds.addRef }));

const ORDERS = "p-orders0000";
const BOARD = { project: ORDERS, id: ".unfold/boards/orders.json" };
const PAGE = { board: BOARD, page: "page-1" };
const OTHER = { project: ORDERS, id: ".unfold/boards/refunds.json" };

function hit(key: string, name: string, kind: string, score: number, extra: Partial<SearchHit> = {}): SearchHit {
  return { project: ORDERS, key, name, qualifiedName: name, path: "shop/store.py", kind, line: 12, app: null, score, ...extra };
}

/** `order`: an entrance, and three definitions (one a class). */
const RESULTS: SearchResults = {
  query: "order",
  entrances: {
    hits: [hit("place-key", "Place an order", "entrance", 700, { qualifiedName: "Shop API · POST /orders/{order_id}", app: "Shop API", path: "", line: null })],
    total: 1,
    truncated: false,
  },
  definitions: {
    hits: [
      hit("place-order-key", "place_order", "function", 600, { path: "shop/service.py", line: 9 }),
      hit("load-order-key", "load_order", "function", 500),
      hit("order-key", "Order", "class", 400, { path: "shop/types.py", line: 4 }),
    ],
    total: 3,
    truncated: false,
  },
  files: { hits: [], total: 0, truncated: false },
};

let engine: FakeEngine;

beforeEach(async () => {
  vi.clearAllMocks();
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspaceView([projectRow(ORDERS, "shop")]),
      search: () => RESULTS,
      board_refs: () => ({ refs: [], truncated: false }),
      open_board: ({ id }) => boardState(ORDERS, id, boardDocument([page("page-1", [], { title: "Placing an order" })])),
    },
  });
  resetBoards();
  useShownBoardStore.setState({ board: null });
  await act(async () => {
    await openBoard(OTHER, { background: true });
    await openBoard(BOARD);
  });
});

afterEach(() => {
  act(() => {
    closeAdd();
  });
  engine.uninstall();
});

function input(): HTMLInputElement {
  return screen.getByRole("combobox", { name: "Find" });
}

function type(text: string): void {
  fireEvent.change(input(), { target: { value: text } });
}

function press(key: string): void {
  fireEvent.keyDown(input(), { key });
}

function openBox(): void {
  renderWithClient(<AddBox />);
  act(() => {
    openAdd();
  });
}

describe("the Add box", () => {
  it("opens on a board, and closes on Escape", () => {
    openBox();
    expect(screen.getByRole("dialog", { name: "Add" })).toBeTruthy();
    press("Escape");
    expect(screen.queryByRole("dialog", { name: "Add" })).toBeNull();
  });

  it("closes when the window shows another board, and stays closed there and on coming back", () => {
    openBox();
    expect(screen.getByRole("dialog", { name: "Add" })).toBeTruthy();
    act(() => {
      showBoard(OTHER);
    });
    expect(screen.queryByRole("dialog", { name: "Add" })).toBeNull();
    act(() => {
      showBoard(BOARD);
    });
    expect(screen.queryByRole("dialog", { name: "Add" })).toBeNull();
  });

  it("doesn't open without a board", () => {
    useShownBoardStore.setState({ board: null });
    openBox();
    expect(screen.queryByRole("dialog", { name: "Add" })).toBeNull();
  });

  it("lists names in the board's project as one ranked list, each row with its kind, and no files yet", async () => {
    openBox();
    type("order");
    const list = await screen.findByRole("listbox", { name: "Results" });
    expect(engine.callsOf("search")).toEqual([{ query: "order", kinds: ["definitions", "entrances"], projects: [ORDERS] }]);
    const rows = within(list).getAllByRole("option");
    expect(rows.map((row) => row.querySelector(".add-row-kind")?.textContent)).toEqual(["entrance", "function", "function", "class"]);
    expect(rows.map((row) => row.querySelector(".add-row-name")?.textContent)).toEqual(["Place an order", "place_order", "load_order", "Order"]);
    expect(rows.map((row) => row.querySelector(".add-row-meta")?.textContent)).toEqual(["Shop API · POST /orders/{order_id}", "shop/service.py:9", "shop/store.py:12", "shop/types.py:4"]);
    expect(rows[0]?.getAttribute("aria-selected")).toBe("true");
    expect(screen.queryByRole("tab")).toBeNull();
  });

  it("adds the selected definition to the page on Enter", async () => {
    openBox();
    type("order");
    await screen.findByRole("listbox", { name: "Results" });
    press("ArrowDown");
    press("Enter");
    await waitFor(() => {
      expect(adds.addRef).toHaveBeenCalledWith(PAGE, { block: "place-order-key" }, "place_order");
    });
    expect(screen.queryByRole("dialog", { name: "Add" })).toBeNull();
  });

  it("adds an entrance's card on Enter", async () => {
    openBox();
    type("order");
    await screen.findByRole("listbox", { name: "Results" });
    press("Enter");
    await waitFor(() => {
      expect(adds.addRef).toHaveBeenCalledWith(PAGE, { entrance: "place-key" }, "Place an order");
    });
  });
});
