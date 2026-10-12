// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FileView } from "../ipc";
import { actions } from "../keyboard";
import type * as Boards from "../state/boards";
import { openBoard } from "../state/boards";
import { resetBoards } from "../state/boards/store";
import { initialLayout, useLayout } from "../state/layout";
import { closePeek, openPeek } from "../state/overlays";
import { useTabs } from "../state/tabs";
import { useWindowStore } from "../state/window";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { PeekSheet } from "./PeekSheet";

const adds = vi.hoisted(() => ({
  addBlock: vi.fn(),
  newBoard: vi.fn(() => Promise.resolve({ project: "p", id: "d" })),
}));
vi.mock("../state/boards", async (original) => ({ ...(await original<typeof Boards>()), ...adds }));

const ORDERS = "p-orders0000";
const PATH = "src/routes/orders.ts";
const LIST_ORDERS = "src/routes/orders.ts#function:listOrders";
const BOARD = { project: ORDERS, id: ".unfold/local/drafts/orders.json" };

const TYPES_PATH = "src/types.ts";
const SCHEMA_PATH = "db/schema.sql";
const QUERY = "src/types.ts#interface:Query";

/** A file whose first line declares an interface. */
function typesView(): FileView {
  const view = fixture("file_view", "file_view_orders_page");
  return { ...view, path: TYPES_PATH, outline: [{ key: QUERY, kind: "interface", startLine: 1, endLine: 3, start: 0, end: 10, groupable: false }] };
}

/** An evidence file: text with line numbers, nothing compiled. */
function schemaView(): FileView {
  const text = "create table orders (\n  id text primary key\n);\n";
  return {
    ...fixture("file_view", "file_view_orders_page"),
    path: SCHEMA_PATH,
    language: null,
    graph: "evidence",
    lineCount: 3,
    startLine: 1,
    endLine: 3,
    textStart: 0,
    text,
    lines: [
      { number: 1, start: 0, end: 21, indent: 0 },
      { number: 2, start: 22, end: 43, indent: 2 },
      { number: 3, start: 44, end: 46, indent: 0 },
    ],
    spans: [],
    outline: [],
    links: [],
  };
}

const NOTIFY_PATH = "src/routes/notify.ts";
const NOTIFY_LINES = ["export function notify(name: string) {", '  if (name === "é😀") {', "    send(name);", "  }", "}"];
/** How `file_view` says the notify file relates to the graph. */
let notifyGraph: FileView["graph"] = "current";

/** A small file with a non-ASCII condition on line 2, as `file_view` returns it (UTF-16 offsets). */
function notifyView(): FileView {
  let start = 0;
  const lines = NOTIFY_LINES.map((line, index) => {
    const row = { number: index + 1, start, end: start + line.length, indent: line.length - line.trimStart().length };
    start += line.length + 1;
    return row;
  });
  return {
    ...fixture("file_view", "file_view_orders_page"),
    path: NOTIFY_PATH,
    graph: notifyGraph,
    lineCount: NOTIFY_LINES.length,
    startLine: 1,
    endLine: NOTIFY_LINES.length,
    textStart: 0,
    text: `${NOTIFY_LINES.join("\n")}\n`,
    lines,
    spans: [],
    outline: [],
    links: [],
  };
}

let engine: FakeEngine;

beforeEach(() => {
  vi.clearAllMocks();
  engine = installFakeEngine({
    handlers: {
      workspace: () => workspaceView([projectRow(ORDERS, "storefront-api")]),
      file_view: ({ path }) =>
        path === TYPES_PATH ? typesView() : path === SCHEMA_PATH ? schemaView() : path === NOTIFY_PATH ? notifyView() : fixture("file_view", "file_view_orders_page"),
      open_in_editor: () => ({ editor: "code" }) as never,
      open_board: () => ({
        status: "opened",
        board: boardState(ORDERS, BOARD.id, boardDocument([page("page-1", [{ id: "listOrders", ref: { block: LIST_ORDERS }, at: [0, 0] }], { title: "Placing an order" })])),
      }),
    },
  });
  resetBoards();
  notifyGraph = "current";
  useTabs.setState({ boards: [], active: "home" });
  useLayout.setState(initialLayout);
  useWindowStore.setState({ info: null, selectedProject: ORDERS });
});

afterEach(() => {
  act(() => {
    closePeek();
  });
  engine.uninstall();
});

function sheet(): HTMLElement {
  return screen.getByRole("complementary", { name: `Source of ${PATH}` });
}

function row(line: number): HTMLElement | null {
  return sheet().querySelector(`[data-line="${line}"]`);
}

describe("the peek sheet", () => {
  it("shows the whole file read-only, with syntax colors, a fold and ⊕ on each definition", async () => {
    renderWithClient(<PeekSheet />);
    act(() => {
      openPeek({ project: ORDERS, path: PATH });
    });
    await screen.findByText("50 lines · TypeScript");
    expect(engine.callsOf("file_view")[0]).toEqual({ project: ORDERS, path: PATH, range: { startLine: 1, endLine: 200 } });
    expect(row(2)?.textContent).toContain('import { asyncHandler } from "../lib/async-handler.ts";');
    expect(row(2)?.querySelector(".tok-keyword")?.textContent).toBe("import");
    expect(within(sheet()).getByText("a new draft")).toBeTruthy();

    // ⊕ beside a definition (only definitions get one); from Home it starts a draft.
    expect(row(1)?.querySelector(".peek-add-button")).toBeNull();
    fireEvent.click(within(sheet()).getByRole("button", { name: "Add listOrders to the page" }));
    await waitFor(() => {
      expect(adds.newBoard).toHaveBeenCalledWith(ORDERS, { title: "listOrders" });
    });

    // Folding a definition hides its body behind a pill.
    fireEvent.click(within(sheet()).getByRole("button", { name: "Fold lines 11–13" }));
    expect(row(12)).toBeNull();
    fireEvent.click(within(sheet()).getByRole("button", { name: "Unfold 2 lines" }));
    expect(row(12)).not.toBeNull();
  });

  it("marks a definition already on the page and adds others to it", async () => {
    await act(async () => {
      await openBoard(BOARD);
    });
    renderWithClient(<PeekSheet />);
    act(() => {
      openPeek({ project: ORDERS, path: PATH });
    });
    await screen.findByRole("img", { name: "listOrders is on the page" });
    expect(within(sheet()).getByText("Placing an order")).toBeTruthy();
  });

  it("adds a type declaration from its ⊕ as a type card in Diagram view", async () => {
    await act(async () => {
      await openBoard(BOARD);
    });
    renderWithClient(<PeekSheet />);
    act(() => {
      openPeek({ project: ORDERS, path: TYPES_PATH });
    });
    fireEvent.click(await screen.findByRole("button", { name: "Add Query to the page" }));
    expect(adds.addBlock).toHaveBeenCalledWith({ board: BOARD, page: "page-1" }, ORDERS, QUERY, "diagram");
  });

  it("shows an evidence file as text with line numbers", async () => {
    renderWithClient(<PeekSheet />);
    act(() => {
      openPeek({ project: ORDERS, path: SCHEMA_PATH });
    });
    await screen.findByText("3 lines · evidence file");
    const evidence = screen.getByRole("complementary", { name: `Source of ${SCHEMA_PATH}` });
    expect(evidence.querySelector('[data-line="2"]')?.textContent).toBe("2  id text primary key");
    expect(evidence.querySelector(".peek-add-button")).toBeNull();
  });

  it("opens at a text match: the current match and its line are marked, other matches tick the scrollbar", async () => {
    renderWithClient(<PeekSheet />);
    const matches = [
      { line: 9, text: "export const ordersRouter = Router();", column: 0, ranges: [[13, 25]] as [number, number][] },
      { line: 2, text: "import { asyncHandler }", column: 0, ranges: [[9, 21]] as [number, number][] },
    ];
    act(() => {
      openPeek({ project: ORDERS, path: PATH, line: 9, matches, current: 0 });
    });
    await screen.findByText("50 lines · TypeScript");
    expect(row(9)?.classList.contains("current")).toBe(true);
    expect(row(9)?.querySelector(".peek-match.current")?.textContent).toBe("ordersRouter");
    expect(row(2)?.querySelector(".peek-match.other")?.textContent).toBe("asyncHandler");
    expect(sheet().querySelectorAll(".peek-tick")).toHaveLength(2);
  });

  it("pins, opens in the editor, and closes on Escape", async () => {
    renderWithClient(<PeekSheet />);
    act(() => {
      openPeek({ project: ORDERS, path: PATH, line: 3 });
    });
    await screen.findByText("50 lines · TypeScript");
    expect(actions.isEnabled("peek.openInEditor")).toBe(true);

    fireEvent.click(within(sheet()).getByRole("button", { name: "Pin" }));
    expect(useLayout.getState().peek.pinned).toBe(true);
    expect(sheet().classList.contains("sheet-pinned")).toBe(true);

    fireEvent.click(within(sheet()).getByRole("button", { name: "More" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: /Open in editor/ }));
    await waitFor(() => {
      expect(engine.callsOf("open_in_editor")).toEqual([{ project: ORDERS, path: PATH, line: 3 }]);
    });

    fireEvent.keyDown(sheet(), { key: "Escape" });
    expect(screen.queryByRole("complementary")).toBeNull();
    expect(actions.isEnabled("peek.openInEditor")).toBe(false);
  });

  describe("View code's focus", () => {
    const condition = 'name === "é😀"';
    const startColumn = NOTIFY_LINES[1]?.indexOf(condition) ?? 0;
    const focus = { startLine: 2, startColumn, endLine: 2, endColumn: startColumn + condition.length };
    const notify = () => screen.getByRole("complementary", { name: `Source of ${NOTIFY_PATH}` });
    const notifyRow = (line: number) => notify().querySelector(`[data-line="${line}"]`);

    it("tints the step's rows and marks its exact expression, by UTF-16 columns, on verified source", async () => {
      renderWithClient(<PeekSheet />);
      act(() => {
        openPeek({ project: ORDERS, path: NOTIFY_PATH, line: 2, focus });
      });
      await screen.findByText("5 lines · TypeScript");
      expect(notifyRow(2)?.classList.contains("focus")).toBe(true);
      expect(notifyRow(1)?.classList.contains("focus")).toBe(false);
      expect(notifyRow(3)?.classList.contains("focus")).toBe(false);
      expect([...notify().querySelectorAll(".peek-focus")].map((mark) => mark.textContent).join("")).toBe(condition);
      expect(within(notify()).queryByRole("note")).toBeNull();
    });

    it("tints every row of a lines-only focus (a combined block), with no expression", async () => {
      renderWithClient(<PeekSheet />);
      act(() => {
        openPeek({ project: ORDERS, path: NOTIFY_PATH, line: 2, focus: { startLine: 2, endLine: 4 } });
      });
      await screen.findByText("5 lines · TypeScript");
      expect([1, 2, 3, 4, 5].map((line) => notifyRow(line)?.classList.contains("focus"))).toEqual([false, true, true, true, false]);
      expect(notify().querySelector(".peek-focus")).toBeNull();
    });

    it("never draws a range on a file changed since the compile: it says so instead", async () => {
      notifyGraph = "not_compiled";
      renderWithClient(<PeekSheet />);
      act(() => {
        openPeek({ project: ORDERS, path: NOTIFY_PATH, line: 2, focus });
      });
      await screen.findByText("5 lines · TypeScript · changed since the last update");
      expect(within(notify()).getByRole("note").textContent).toBe("Changed since the last compile · the range isn't shown");
      expect(notify().querySelector(".peek-row.focus")).toBeNull();
      expect(notify().querySelector(".peek-focus")).toBeNull();
      // The sheet itself is as always: the file, its lines, its close button.
      expect(notifyRow(2)?.textContent).toContain(condition);
      expect(within(notify()).getByRole("button", { name: "Close" })).toBeTruthy();
    });
  });
});
