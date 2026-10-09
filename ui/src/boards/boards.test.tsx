// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { BoardBanner } from "../canvas/BoardBanner";
import type { BoardDocument, BoardState } from "../ipc/generated";
import { ShellSlot } from "../shell/slots";
import * as store from "../state/boards/store";
import { openBoardTab, useTabs } from "../state/tabs";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { useBoardTabs } from "../editor/useBoardTabs";
import { BoardChrome } from "./BoardChrome";
import { closeBoardDialog, setRenaming } from "./dialogs";

const ID = ".unfold/local/drafts/orders.json";
const KEY = "src/routes/orders.ts#function:createOrder";

function document(title = "Orders"): BoardDocument {
  return { format: "unfold-board", version: 1, title, defaultView: "code", cards: [{ id: "create-order", ref: { block: KEY }, at: [0, 0] }], groups: [], labels: {}, guards: {} };
}

function boardState(fields: Partial<BoardState> = {}): BoardState {
  return {
    id: ID,
    place: "draft",
    name: "orders",
    document: document(),
    revision: "r1",
    health: { items: [], missing: 0, outdated: 0, brokenGroups: 0, review: 0 },
    outline: { rows: [], unlabeled: [] },
    label: null,
    ...fields,
  };
}

let engine: FakeEngine;

/** The editor's link between open boards and tabs (a published board's tab follows its new ID). */
function BoardTabs() {
  useBoardTabs();
  return null;
}

function renderChrome() {
  return renderWithClient(
    <>
      <BoardTabs />
      <ShellSlot name="titlebar.board" />
      <ShellSlot name="titlebar.actions" />
      <ShellSlot name="titlebar.menu" />
      <BoardChrome />
    </>,
  );
}

function open(state: BoardState): void {
  store.adoptBoardState(state);
  openBoardTab(state.id);
}

beforeEach(() => {
  store.resetBoards();
  closeBoardDialog();
  setRenaming(null);
  useTabs.setState({ tabs: [], activeId: null });
  engine = installFakeEngine({
    handlers: {
      save_board: ({ id, board }) => boardState({ id, place: store.getBoard(id)?.place ?? "draft", document: board, revision: "r-saved" }),
      publish_board: ({ id, name }) =>
        name === undefined
          ? { status: "name_taken", path: ".unfold/boards/orders.json", suggestion: "orders-2" }
          : { status: "done", board: boardState({ id: `.unfold/boards/${name}.json`, place: "published", name, document: store.getBoard(id)?.document ?? document() }) },
      delete_board: () => null,
      export_mermaid_text: () => ({ text: "flowchart LR\n  a --> b\n" }),
      reload_board: ({ id }) => boardState({ id, document: document("Theirs"), revision: "r-disk" }),
      rename_board: ({ id, name }) =>
        name === "orders-flow"
          ? { status: "name_taken", path: ".unfold/boards/orders-flow.json", suggestion: "orders-flow-2" }
          : { status: "done", board: boardState({ id: `.unfold/boards/${name ?? "x"}.json`, place: "published", name: name ?? "x", document: store.getBoard(id)?.document ?? document() }) },
    },
  });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("the board in the title bar", () => {
  it("shows the title with a Draft chip, and renames it as an undoable edit", async () => {
    open(boardState());
    renderChrome();
    expect(screen.getByText("Draft")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Orders, rename" }));
    const input = screen.getByRole("textbox", { name: "Board title" });
    fireEvent.change(input, { target: { value: "Order placement" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      expect(engine.callsOf("save_board")).toHaveLength(1);
    });
    expect(store.getBoard(ID)?.document.title).toBe("Order placement");
    expect(screen.getByRole("button", { name: "Order placement, rename" })).toBeTruthy();
    // A draft's file name is internal: no rename_board.
    expect(engine.callsOf("rename_board")).toEqual([]);
    act(() => {
      store.undo(ID);
    });
    expect(store.getBoard(ID)?.document.title).toBe("Orders");
  });

  it("publishes, and lets the person pick another name when the file exists", async () => {
    open(boardState());
    act(() => {
      store.editBoard(ID, (draft) => {
        draft.cards[0] = { ...draft.cards[0], id: "create-order", ref: { block: KEY }, at: [10, 10] };
      });
    });
    renderChrome();
    fireEvent.click(screen.getByRole("button", { name: "Publish" }));
    const confirm = await screen.findByRole("dialog", { name: "Publish board" });
    expect(within(confirm).getByText(".unfold/boards/orders.json")).toBeTruthy();
    fireEvent.click(within(confirm).getByRole("button", { name: "Publish" }));

    const prompt = await screen.findByRole("dialog", { name: "That name is taken" });
    expect(within(prompt).getByText(".unfold/boards/orders.json")).toBeTruthy();
    const field = within(prompt).getByRole("textbox", { name: "File name" });
    expect((field as HTMLInputElement).value).toBe("orders-2");
    fireEvent.change(field, { target: { value: "bad name!" } });
    fireEvent.click(within(prompt).getByRole("button", { name: "Publish" }));
    expect(await within(prompt).findByText(/Use letters, digits/)).toBeTruthy();
    fireEvent.change(field, { target: { value: "orders-2" } });
    fireEvent.click(within(prompt).getByRole("button", { name: "Publish" }));

    await waitFor(() => {
      expect(store.getBoard(".unfold/boards/orders-2.json")).toBeDefined();
    });
    // The unsaved edit was saved first; the published board keeps its undo history and tab.
    const order = engine.calls.map((entry) => entry.command).filter((command) => command === "save_board" || command === "publish_board");
    expect(order).toEqual(["save_board", "publish_board", "publish_board"]);
    expect(engine.callsOf("publish_board")).toEqual([{ id: ID }, { id: ID, name: "orders-2" }]);
    expect(store.getBoard(ID)).toBeUndefined();
    expect(store.getBoard(".unfold/boards/orders-2.json")?.history.past).toHaveLength(1);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText(".unfold/boards/orders-2.json")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Publish" })).toBeNull();
  });

  it("renames a published board's file, asking for another name when it is taken", async () => {
    open(boardState({ id: ".unfold/boards/orders.json", place: "published" }));
    renderChrome();
    fireEvent.click(screen.getByRole("button", { name: "Orders, rename" }));
    const input = screen.getByRole("textbox", { name: "Board title" });
    fireEvent.change(input, { target: { value: "Orders flow" } });
    fireEvent.keyDown(input, { key: "Enter" });
    const prompt = await screen.findByRole("dialog", { name: "Another board has that name" });
    fireEvent.click(within(prompt).getByRole("button", { name: "Rename" }));
    await waitFor(() => {
      expect(store.getBoard(".unfold/boards/orders-flow-2.json")).toBeDefined();
    });
    expect(engine.callsOf("rename_board")).toEqual([
      { id: ".unfold/boards/orders.json", name: "orders-flow" },
      { id: ".unfold/boards/orders.json", name: "orders-flow-2" },
    ]);
    expect(store.getBoard(".unfold/boards/orders-flow-2.json")?.document.title).toBe("Orders flow");
  });

  it("deletes a board only after asking, and closes it", async () => {
    open(boardState());
    renderChrome();
    fireEvent.click(screen.getByRole("button", { name: "Board Menu" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete…" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(dialog.textContent).toContain("Delete “Orders”?");
    expect(engine.callsOf("delete_board")).toEqual([]);
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(store.getBoard(ID)).toBeDefined();

    fireEvent.click(screen.getByRole("button", { name: "Board Menu" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete…" }));
    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Delete" }));
    await waitFor(() => {
      expect(store.getBoard(ID)).toBeUndefined();
    });
    expect(engine.callsOf("delete_board")).toEqual([{ id: ID }]);
  });

  it("copies the board as Mermaid through the main process", async () => {
    open(boardState());
    renderChrome();
    fireEvent.click(screen.getByRole("button", { name: "Board Menu" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Copy Mermaid" }));
    await waitFor(() => {
      expect(engine.windowCalls).toContainEqual({ command: "copy_to_clipboard", args: { text: "flowchart LR\n  a --> b\n" } });
    });
    expect(engine.callsOf("export_mermaid_text")).toEqual([{ board: ID }]);
    fireEvent.click(screen.getByRole("button", { name: "Board Menu" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Export Mermaid…" }));
    await waitFor(() => {
      expect(engine.windowCalls).toContainEqual({ command: "export_mermaid", args: { boardId: ID, title: "Orders" } });
    });
  });
});

describe("the conflict banner", () => {
  function conflicted(): void {
    open(boardState());
    act(() => {
      store.editBoard(ID, (draft) => {
        draft.title = "Mine";
      });
    });
    void store.handleOutsideChange(ID, "r-disk");
  }

  it("keeps this window's version when asked", async () => {
    conflicted();
    renderWithClient(<BoardBanner boardId={ID} />);
    const banner = await screen.findByRole("alert");
    expect(banner.textContent).toContain("changed on disk");
    fireEvent.click(within(banner).getByRole("button", { name: "Keep mine" }));
    await waitFor(() => {
      expect(screen.queryByRole("alert")).toBeNull();
    });
    expect(engine.callsOf("save_board")).toEqual([{ id: ID, board: expect.objectContaining({ title: "Mine" }) as unknown, force: true }]);
  });

  it("takes the file on disk when asked, undoably", async () => {
    conflicted();
    renderWithClient(<BoardBanner boardId={ID} />);
    fireEvent.click(within(await screen.findByRole("alert")).getByRole("button", { name: "Take theirs" }));
    await waitFor(() => {
      expect(store.getBoard(ID)?.document.title).toBe("Theirs");
    });
    expect(screen.queryByRole("alert")).toBeNull();
    act(() => {
      store.undo(ID);
    });
    expect(store.getBoard(ID)?.document.title).toBe("Mine");
  });

  it("shows a failed save quietly, with Retry", async () => {
    open(boardState());
    let fail = true;
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        save_board: ({ id, board }) => {
          if (fail) {
            // eslint-disable-next-line @typescript-eslint/only-throw-error
            throw { code: "board_problem", message: "the source changed: compile first" };
          }
          return boardState({ id, document: board });
        },
      },
    });
    act(() => {
      store.editBoard(ID, (draft) => {
        draft.title = "Edited";
      });
    });
    await act(async () => {
      await store.saveBoard(ID);
    });
    renderWithClient(<BoardBanner boardId={ID} />);
    const notice = screen.getByRole("status");
    expect(notice.textContent).toContain("Not saved: the source changed: compile first");
    fail = false;
    fireEvent.click(within(notice).getByRole("button", { name: "Retry" }));
    await waitFor(() => {
      expect(screen.queryByRole("status")).toBeNull();
    });
  });
});
