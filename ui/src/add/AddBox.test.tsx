// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FileHit, SearchHit, SearchResults } from "../ipc";
import type * as Boards from "../state/boards";
import { openBoard } from "../state/boards";
import { resetBoards } from "../state/boards/store";
import type * as Overlays from "../state/overlays";
import { closeAdd, openAdd } from "../state/overlays";
import { useShownBoardStore } from "../state/shownBoard";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { AddBox } from "./AddBox";

const adds = vi.hoisted(() => ({
  addBlock: vi.fn(),
  openEntrance: vi.fn(() => Promise.resolve()),
  openPeek: vi.fn(),
}));
vi.mock("../state/boards", async (original) => ({ ...(await original<typeof Boards>()), addBlock: adds.addBlock, openEntrance: adds.openEntrance }));
vi.mock("../state/overlays", async (original) => ({ ...(await original<typeof Overlays>()), openPeek: adds.openPeek }));

const ORDERS = "p-orders0000";
const BOARD = { project: ORDERS, id: ".unfold/boards/orders.json" };
const PAGE = { board: BOARD, page: "page-1" };

function hit(key: string, name: string, kind: string, score: number, extra: Partial<SearchHit> = {}): SearchHit {
  return { project: ORDERS, key, name, qualifiedName: name, path: "shop/store.py", kind, line: 12, application: null, score, nameMatches: [], ...extra };
}

const FILE: FileHit = { project: ORDERS, path: "tests/test_orders.py", name: "test_orders.py", definitions: 3, score: 900, matches: [] };

/** `order`: an entrance, three definitions (one a class), and a file. */
const RESULTS: SearchResults = {
  query: "order",
  entrances: {
    hits: [hit("entrance:place", "Place an order", "http", 700, { qualifiedName: "POST /orders/{order_id}", application: "Shop API", path: "unfold.toml", line: null })],
    total: 1,
    truncated: false,
  },
  definitions: {
    hits: [
      hit("shop/service.py#function:place_order", "place_order", "function", 600, { path: "shop/service.py", line: 9 }),
      hit("shop/store.py#function:load_order", "load_order", "function", 500),
      hit("shop/types.py#class:Order", "Order", "class", 400, { path: "shop/types.py", line: 4 }),
    ],
    total: 3,
    truncated: false,
  },
  files: { hits: [FILE], total: 1, truncated: false },
  projects: [{ project: ORDERS, definitions: 3, entrances: 1, files: 1 }],
  tookMs: 1,
};

let engine: FakeEngine;

beforeEach(async () => {
  vi.clearAllMocks();
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspaceView([projectRow(ORDERS, "shop")]),
      search: () => RESULTS,
      board_refs: () => ({ refs: [], truncated: false }),
      open_board: () => boardState(ORDERS, BOARD.id, boardDocument([page("page-1", [], { title: "Placing an order" })])),
    },
  });
  resetBoards();
  useShownBoardStore.setState({ board: null });
  await act(async () => {
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

  it("doesn't open without a board", () => {
    useShownBoardStore.setState({ board: null });
    openBox();
    expect(screen.queryByRole("dialog", { name: "Add" })).toBeNull();
  });

  it("lists names in the board's project as one ranked list, each row with its kind", async () => {
    openBox();
    type("order");
    const list = await screen.findByRole("listbox", { name: "Results" });
    expect(engine.callsOf("search")).toEqual([{ query: "order", projects: [ORDERS] }]);
    const rows = within(list).getAllByRole("option");
    expect(rows.map((row) => row.querySelector(".add-row-kind")?.textContent)).toEqual(["entrance", "function", "function", "class", "file"]);
    expect(rows.map((row) => row.querySelector(".add-row-name")?.textContent)).toEqual(["Place an order", "place_order", "load_order", "Order", "tests/test_orders.py"]);
    expect(rows.map((row) => row.querySelector(".add-row-meta")?.textContent)).toEqual(["Shop API · POST /orders/{order_id}", "shop/service.py:9", "shop/store.py:12", "shop/types.py:4", undefined]);
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
      expect(adds.addBlock).toHaveBeenCalledWith(PAGE, "shop/service.py#function:place_order", undefined);
    });
    expect(screen.queryByRole("dialog", { name: "Add" })).toBeNull();
  });

  it("adds an entrance with its handlers on Enter", async () => {
    openBox();
    type("order");
    await screen.findByRole("listbox", { name: "Results" });
    press("Enter");
    await waitFor(() => {
      expect(adds.openEntrance).toHaveBeenCalledWith(PAGE, "entrance:place");
    });
  });

  it("opens a file in the peek sheet", async () => {
    openBox();
    type("order");
    fireEvent.click(await screen.findByRole("option", { name: "file tests/test_orders.py" }));
    await waitFor(() => {
      expect(adds.openPeek).toHaveBeenCalledWith({ project: ORDERS, path: "tests/test_orders.py" });
    });
    expect(adds.addBlock).not.toHaveBeenCalled();
  });
});
