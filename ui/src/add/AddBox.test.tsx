// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EntrancesArgs, EntrancesView, SearchGroup, SearchResults } from "../ipc";
import type * as Boards from "../state/boards";
import { openBoard } from "../state/boards";
import { resetBoards } from "../state/boards/store";
import { closeAdd, openAdd, useDialog } from "../state/overlays";
import { useShownBoardStore } from "../state/shownBoard";
import { useWindowStore } from "../state/window";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { INVOICE_SCOPE, OVERVIEW, REVIEW_WORKER, UNREACHED_SCOPE } from "../test/entrances";
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

/** The orders project's catalog (one application per category); payments has none. */
function entrances({ project, application, unreached, cursor }: EntrancesArgs): EntrancesView {
  if (project !== ORDERS) {
    return { view: "overview", project, counts: { applications: 0, launches: 0, routes: 0, entrances: 0 }, categories: [], unreached: 0 };
  }
  if (unreached === true) {
    return UNREACHED_SCOPE;
  }
  if (application === undefined) {
    return OVERVIEW;
  }
  if (application === "review-worker") {
    const row = { key: REVIEW_WORKER.key, id: null, section: 0, kind: "process" as const, method: null, trigger: "worker", title: "process worker", handler: null, path: "package.json", line: 11, reaches: 64, alsoIn: [] };
    return { view: "scope", project, sections: [{ title: "Launch", kind: "launch", mountedFrom: null, alsoIn: [], count: 1 }], rows: [row], total: 1, nextCursor: null };
  }
  if (cursor === undefined) {
    return INVOICE_SCOPE;
  }
  const row = { key: "entrance:list-runs", id: null, section: 1, kind: "http" as const, method: "GET" as const, trigger: "/runs", title: "GET /runs", handler: "list_runs", path: "api/runs.py", line: 30, reaches: 2, alsoIn: [] };
  return { ...INVOICE_SCOPE, rows: [row], nextCursor: null };
}

/** The entrance on the board's page. */
const ON_PAGE = "entrance:post-runs";

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
      entrances,
      search: ({ query }) => (query === "cart" ? typeResults() : searchResults(query === "worker" ? "search_worker" : "search_place")),
      preview: ({ key }) => ({ ...fixture("preview", key.startsWith("entrance:") ? "preview_first_entrance" : "preview_create_order"), project: ORDERS }),
      board_refs: () => ({ refs: [], truncated: false }),
      open_board: () => boardState(ORDERS, BOARD.id, boardDocument([page("page-1", [{ id: "import", ref: { entrance: ON_PAGE }, at: [0, 0] }], { title: "Placing an order" })])),
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

  it("browses the board's project before typing: LOOK IN, the application's rows, the preview, and Enter", async () => {
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

    // LOOK IN: All projects with the total, the board's project unfolded by category and selected.
    const lookIn = await screen.findByRole("tree", { name: "Look in" });
    await waitFor(() => {
      expect(within(lookIn).getByRole("treeitem", { name: /All projects/ }).textContent).toBe("All projects22");
    });
    const project = within(lookIn).getByRole("treeitem", { name: /storefront-api/ });
    expect(project.getAttribute("aria-expanded")).toBe("true");
    expect(project.getAttribute("aria-selected")).toBe("true");
    expect(within(lookIn).getByText("Needs a look")).toBeTruthy();
    expect(within(lookIn).getByRole("treeitem", { name: /Not reached from a launch/ }).textContent).toContain("2");
    expect(screen.getByRole("heading", { name: "storefront-api" })).toBeTruthy();
    expect(screen.getByText("4 applications · 5 launches · 14 routes")).toBeTruthy();

    // An application: its launch first, then its routes by path, the one on the page marked.
    fireEvent.click(within(lookIn).getByRole("treeitem", { name: /Invoice HITL API/ }));
    expect(await screen.findByRole("heading", { name: "Invoice HITL API" })).toBeTruthy();
    expect(screen.getByText("1 launch · 14 routes")).toBeTruthy();
    const launch = await screen.findByRole("option", { name: "Launch Start API uvicorn api.main:app --host 0.0.0.0 --port 8000" });
    expect(launch.getAttribute("aria-selected")).toBe("true");
    const list = screen.getByRole("listbox", { name: "Entrances" });
    expect(within(list).getByRole("option", { name: "POST /runs on page" })).toBeTruthy();
    expect(within(list).getByText("also in Dashboard builder", { exact: false })).toBeTruthy();

    // The launch's preview: its command, what it reaches, where it is declared, and Edit.
    const preview = screen.getByRole("complementary", { name: "Preview" });
    expect(await within(preview).findByRole("heading", { name: "Start API" })).toBeTruthy();
    expect(within(preview).getByText("data-pipelines $")).toBeTruthy();
    expect(within(preview).getByText("184 functions · 14 routes")).toBeTruthy();
    expect(within(preview).getByText("unfold.toml")).toBeTruthy();

    // ↓ selects the route (the input points at it), and Enter adds it to the page.
    press("ArrowDown");
    const route = within(list).getByRole("option", { name: "POST /runs on page" });
    expect(input().getAttribute("aria-activedescendant")).toBe(route.id);
    expect(await within(preview).findByText("importOrders")).toBeTruthy();
    press("Enter");
    await waitFor(() => {
      expect(adds.openEntrance).toHaveBeenCalledWith({ board: BOARD, page: "page-1" }, ORDERS, ON_PAGE);
    });
  });

  it("pages an application's rows, walks LOOK IN with ← ↑ ↓, restores the selection, and opens the launch editor", async () => {
    function OpenDialog() {
      const dialog = useDialog();
      return <output data-testid="dialog">{dialog ? JSON.stringify(dialog) : ""}</output>;
    }
    renderWithClient(
      <>
        <AddBox />
        <OpenDialog />
      </>,
    );
    act(() => {
      openAdd();
    });
    const lookIn = await screen.findByRole("tree", { name: "Look in" });
    fireEvent.click(await within(lookIn).findByRole("treeitem", { name: /All projects/ }));
    expect(await screen.findByRole("heading", { name: "All projects" })).toBeTruthy();
    expect(screen.getByText("2 projects · 4 applications")).toBeTruthy();

    // The continuation loads the next page as the end of the list comes near.
    fireEvent.click(screen.getByRole("option", { name: /^Invoice HITL API/ }));
    expect(await screen.findByRole("option", { name: "GET /runs" })).toBeTruthy();
    expect(engine.callsOf("entrances")).toContainEqual({ project: ORDERS, application: "invoice-hitl", limit: 100, cursor: "c5" });

    // ← goes to LOOK IN; ↓ selects the next scope; → comes back to the list.
    press("ArrowLeft");
    const invoice = within(lookIn).getByRole("treeitem", { name: /Invoice HITL API/ });
    expect(input().getAttribute("aria-activedescendant")).toBe(invoice.id);
    press("ArrowDown");
    expect(await screen.findByRole("heading", { name: "Review worker" })).toBeTruthy();
    press("ArrowRight");
    expect(input().getAttribute("aria-activedescendant")).toBe((await screen.findByRole("option", { name: /^Launch npm run worker/ })).id);

    // Add opens again where it was.
    press("Escape");
    act(() => {
      openAdd();
    });
    expect(await screen.findByRole("heading", { name: "Review worker" })).toBeTruthy();

    // A configured handler's preview offers Edit too.
    const reopened = screen.getByRole("tree", { name: "Look in" });
    fireEvent.click(within(reopened).getByRole("treeitem", { name: /Not reached from a launch/ }));
    expect((await screen.findByRole("option", { name: "COMMAND import" })).getAttribute("aria-selected")).toBe("true");
    const handler = screen.getByRole("complementary", { name: "Preview" });
    expect(await within(handler).findByRole("button", { name: "Edit" })).toBeTruthy();
    fireEvent.click(within(reopened).getByRole("treeitem", { name: /Review worker/ }));
    expect(await screen.findByRole("heading", { name: "Review worker" })).toBeTruthy();

    // Edit on a detected launch opens its override in the launch editor, after Add closes.
    const preview = screen.getByRole("complementary", { name: "Preview" });
    fireEvent.click(await within(preview).findByRole("button", { name: "Edit" }));
    expect(screen.queryByRole("dialog", { name: "Add" })).toBeNull();
    expect(JSON.parse(screen.getByTestId("dialog").textContent)).toMatchObject({ kind: "launchEditor", project: ORDERS, mode: "override" });
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
