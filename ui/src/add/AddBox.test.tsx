// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EntranceMap, SearchGroup, SearchResults } from "../ipc";
import { actions, type ActionSpec } from "../keyboard";
import type * as Boards from "../state/boards";
import { openBoard } from "../state/boards";
import { resetBoards } from "../state/boards/store";
import { closeAdd, openAdd } from "../state/overlays";
import { useTabs } from "../state/tabs";
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
const BOARD = { project: ORDERS, id: ".unfold/local/drafts/orders.json" };

function entrances(project: string): EntranceMap {
  return { ...fixture("entrances", "entrances"), project };
}

const FIRST = entrances(ORDERS).applications[0]?.entrances[0]?.key ?? "";

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
      entrances: ({ project }) => (project === ORDERS ? entrances(ORDERS) : { ...entrances(PAYMENTS), applications: [] }),
      search: ({ query }) => (query === "cart" ? typeResults() : searchResults(query === "worker" ? "search_worker" : "search_place")),
      preview: () => ({ ...fixture("preview", "preview_create_order"), project: ORDERS }),
      board_refs: () => ({ refs: [], truncated: false }),
      open_board: () => ({
        status: "opened",
        board: boardState(ORDERS, BOARD.id, boardDocument([page("page-1", [{ id: "import", ref: { entrance: FIRST }, at: [0, 0] }], { title: "Placing an order" })])),
      }),
    },
    streams: {
      search_text: () => Promise.resolve({ files: 0, matches: 0, searched: 0, truncated: false, filesTruncated: false, projects: [] }),
    },
  });
  resetBoards();
  useTabs.setState({ boards: [], active: "home" });
  useWindowStore.setState({ info: null, selectedProject: ORDERS });
});

afterEach(() => {
  act(() => {
    closeAdd();
  });
  engine.uninstall();
});

function input(): HTMLInputElement {
  return screen.getByRole("combobox", { name: "Find or run a command" });
}

function type(text: string): void {
  fireEvent.change(input(), { target: { value: text } });
}

function press(key: string, init: KeyboardEventInit = {}): void {
  fireEvent.keyDown(input(), { key, ...init });
}

describe("the Add box", () => {
  it("opens with A, ⌘K or /, and closes on Escape", async () => {
    renderWithClient(<AddBox />);
    await waitFor(() => {
      expect(actions.get("add.open")?.shortcuts).toEqual(["A", "Mod+K", "/"]);
    });
    act(() => {
      actions.run("add.open");
    });
    expect(screen.getByRole("dialog", { name: "Add" })).toBeTruthy();
    press("Escape");
    expect(screen.queryByRole("dialog", { name: "Add" })).toBeNull();
  });

  it("maps every connected project's applications before typing, marking what the page holds", async () => {
    await act(async () => {
      await openBoard(BOARD);
    });
    renderWithClient(<AddBox />);
    act(() => {
      openAdd();
    });
    await waitFor(() => {
      expect(input().placeholder).toBe("Find entrances, functions, files and text in 2 projects");
    });
    expect(screen.getByText("Adds to “Placing an order”")).toBeTruthy();
    const map = await screen.findByRole("group", { name: "server" });
    expect(within(map).getAllByRole("button").length).toBe(9);
    const lookIn = screen.getByRole("radiogroup", { name: "Look in" });
    await waitFor(() => {
      expect(within(lookIn).getByRole("radio", { name: /All projects/ }).textContent).toContain("18");
    });
    const cli = screen.getByRole("group", { name: "cli" });
    expect(within(cli).getAllByText("on page")).toHaveLength(1);
    expect(screen.getByText("No entrances found.")).toBeTruthy();

    // Look in narrows the map to one project.
    fireEvent.click(within(lookIn).getByRole("radio", { name: /payments/ }));
    expect(screen.queryByRole("group", { name: "server" })).toBeNull();
    fireEvent.click(within(lookIn).getByRole("radio", { name: /All projects/ }));

    // ↓ walks the chips; Enter adds the entrance to the page, ⌘Enter onto a new page.
    press("ArrowDown");
    press("ArrowDown");
    press("Enter");
    await waitFor(() => {
      expect(adds.openEntrance).toHaveBeenCalledTimes(1);
    });
    const [target, project, key] = adds.openEntrance.mock.calls[0] as unknown as [unknown, string, string];
    expect(target).toEqual({ board: BOARD, page: "page-1" });
    expect(project).toBe(ORDERS);
    expect(key).toBe(entrances(ORDERS).applications[0]?.entrances[1]?.key);
  });

  it("searches every project with one call while typing, groups results by kind, and adds from Home to a new draft", async () => {
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
      expect(adds.addBlock).toHaveBeenCalledWith({ board: BOARD, page: "page-1" }, ORDERS, CART, "diagram");
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
      openAdd({ mode: "find", kind: "text" });
    });
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

  it("lists commands after >, bolds the match, and runs the chosen one after closing", async () => {
    const run = vi.fn();
    const specs: ActionSpec[] = [
      { id: "help.copyDiagnostics", title: "Copy Diagnostics", category: "Help", run },
      { id: "board.copyMermaid", title: "Copy Mermaid", category: "Board", group: "Share", run: () => undefined },
      { id: "hidden", title: "Copy Hidden", category: "Board", showInAdd: false, run: () => undefined },
    ];
    const unregister = actions.register(specs);
    renderWithClient(<AddBox />);
    act(() => {
      openAdd({ mode: "commands" });
    });
    expect(input().value).toBe("> ");
    type("> copy");
    const list = screen.getByRole("listbox", { name: "Commands" });
    const options = within(list).getAllByRole("option");
    expect(options.map((option) => option.getAttribute("aria-label"))).toEqual(["Share › Copy Mermaid", "Copy Diagnostics"]);
    expect(list.querySelector("mark.add-command-match")?.textContent).toBe("Copy");
    press("ArrowDown");
    press("Enter");
    expect(screen.queryByRole("dialog", { name: "Add" })).toBeNull();
    await waitFor(() => {
      expect(run).toHaveBeenCalledTimes(1);
    });
    unregister();
  });
});
