// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SearchGroup, SearchResults } from "../ipc";
import type * as Boards from "../state/boards";
import { openBoard } from "../state/boards";
import { resetBoards } from "../state/boards/store";
import { closeAdd, openAdd } from "../state/overlays";
import { useShownBoardStore } from "../state/shownBoard";
import { useWindowStore } from "../state/window";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { AddBox } from "./AddBox";

const adds = vi.hoisted(() => ({
  addBlock: vi.fn(),
  openEntrance: vi.fn(() => Promise.resolve()),
  traceEntrance: vi.fn(() => Promise.resolve({ project: "p", id: "d" })),
  newBoard: vi.fn(() => Promise.resolve({ project: "p", id: "d" })),
}));
vi.mock("../state/boards", async (original) => ({ ...(await original<typeof Boards>()), ...adds }));

const ORDERS = "p-orders0000";
const PAYMENTS = "p-payments00";
const BOARD = { project: ORDERS, id: ".unfold/boards/orders.json" };

/** A search golden (`search_place`, `search_worker`) with its hits in the orders project. */
function searchResults(name: string): SearchResults {
  const results = fixture("search", name);
  const inOrders = <T extends { project: string }>(group: SearchGroup<T>): SearchGroup<T> => ({ ...group, hits: group.hits.map((hit) => ({ ...hit, project: ORDERS })) });
  return { ...results, definitions: inOrders(results.definitions), entrances: inOrders(results.entrances), files: inOrders(results.files) };
}

const CART = "src/cart.ts#interface:Cart";

/** One interface among the definitions. */
function typeResults(): SearchResults {
  const results = searchResults("search_place");
  const hit = { project: ORDERS, key: CART, name: "Cart", qualifiedName: "Cart", path: "src/cart.ts", kind: "interface", line: 12, application: null, score: 90, nameMatches: [0, 1, 2, 3] };
  return { ...results, query: "cart", definitions: { hits: [hit], total: 1, truncated: false } };
}

let engine: FakeEngine;

beforeEach(() => {
  vi.clearAllMocks();
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspaceView([projectRow(ORDERS, "storefront-api"), projectRow(PAYMENTS, "payments")]),
      search: ({ query }) => (query === "cart" ? typeResults() : searchResults(query === "worker" ? "search_worker" : "search_place")),
      preview: ({ key }) => ({ ...fixture("preview", key.startsWith("entrance:") ? "preview_first_entrance" : "preview_create_order"), project: ORDERS }),
      board_refs: () => ({ refs: [], truncated: false }),
      open_board: () => boardState(ORDERS, BOARD.id, boardDocument([page("page-1", [], { title: "Placing an order" })])),
    },
    streams: {
      search_text: () => Promise.resolve({ files: 0, matches: 0, searched: 0, truncated: false, filesTruncated: false, projects: [] }),
    },
  });
  resetBoards();
  useShownBoardStore.setState({ board: null });
  useWindowStore.setState({ info: null, selectedProject: ORDERS });
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

function press(key: string, init: KeyboardEventInit = {}): void {
  fireEvent.keyDown(input(), { key, ...init });
}

describe("the Add box", () => {
  it("opens when asked, and closes on Escape", () => {
    renderWithClient(<AddBox />);
    act(() => {
      openAdd();
    });
    expect(screen.getByRole("dialog", { name: "Add" })).toBeTruthy();
    press("Escape");
    expect(screen.queryByRole("dialog", { name: "Add" })).toBeNull();
  });

  it("searches every project with one call while typing, groups results by kind, and adds from Home to a new board", async () => {
    renderWithClient(<AddBox />);
    act(() => {
      openAdd();
    });
    type("place");
    const row = await screen.findByRole("option", { name: /Function OrderService\.place/ });
    expect(engine.callsOf("search")).toEqual([{ query: "place", limit: 50 }]);
    expect(screen.getByRole("tab", { name: "All" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tab", { name: /Functions/ }).textContent).toBe("Functions1");
    expect(row.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByText("1 project · 1 result")).toBeTruthy();
    await screen.findByRole("heading", { name: "createOrder" });
    // The preview shows a definition's first 5 lines, then how many more it has.
    expect(document.querySelectorAll(".add-preview-line")).toHaveLength(5);
    expect(screen.getByText("6 more lines")).toBeTruthy();

    // Tab switches the kind; the kind's own search asks for 500.
    press("Tab");
    expect(screen.getByRole("tab", { name: /Entrances/ }).getAttribute("aria-selected")).toBe("true");
    press("Tab");
    await waitFor(() => {
      expect(engine.callsOf("search")).toContainEqual({ query: "place", kinds: ["definitions"], limit: 500 });
    });
    press("Enter");
    await waitFor(() => {
      expect(adds.newBoard).toHaveBeenCalledWith(ORDERS, { title: "place" });
    });
  });

  it("shows a type declaration with its kind and adds it as a type card in Diagram view", async () => {
    await act(async () => {
      await openBoard(BOARD);
    });
    renderWithClient(<AddBox />);
    act(() => {
      openAdd();
    });
    type("cart");
    const row = await screen.findByRole("option", { name: "Interface Cart, src/cart.ts" });
    expect(within(row).getByText("TYPE")).toBeTruthy();
    expect(within(row).getByText("interface").className).toBe("add-row-type-kind");
    expect(row.querySelector(".add-row-name")?.textContent).toBe("interface Cart");
    press("Enter");
    await waitFor(() => {
      expect(adds.addBlock).toHaveBeenCalledWith({ board: BOARD, page: "page-1" }, CART, "diagram");
    });
  });

  it("names each entrance's application and counts each file's definitions", async () => {
    renderWithClient(<AddBox />);
    act(() => {
      openAdd();
    });
    type("worker");
    const launch = await screen.findByRole("option", { name: /Entrance .* worker/ });
    expect(within(launch).getByText("storefront-api · worker")).toBeTruthy();
    const file = screen.getByRole("option", { name: "File src/worker.ts" });
    expect(within(file).getByText("1 definition")).toBeTruthy();
  });

  it("streams text matches from two characters, with the text options on the Text tab", async () => {
    renderWithClient(<AddBox />);
    act(() => {
      openAdd();
    });
    fireEvent.click(screen.getByRole("tab", { name: "Text" }));
    expect(screen.getByText(/Type to find text/)).toBeTruthy();
    type("pl");
    await waitFor(() => {
      expect(engine.callsOf("search_text")).toEqual([{ query: "pl", caseSensitive: false, wholeWord: false, regex: false }]);
    });
    fireEvent.click(screen.getByRole("button", { name: "Match case" }));
    await waitFor(() => {
      expect(engine.callsOf("search_text").at(-1)).toEqual({ query: "pl", caseSensitive: true, wholeWord: false, regex: false });
    });
  });
});
