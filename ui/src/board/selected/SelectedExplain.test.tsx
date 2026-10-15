// @vitest-environment jsdom
import "../../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { ReviewButton } from "../../canvas/cards/ReviewBanner";
import { queryClient } from "../../ipc/client";
import type { BoardDocument, Connection, HealthItem, Page, RepairSpec, Resource, SearchResults } from "../../ipc/generated";
import { engineKey } from "../../ipc/queries";
import * as store from "../../state/boards/store";
import { installFakeEngine, type FakeEngine } from "../../test/engine";
import { HEALTHY, boardDocument, boardState, page, projectRow, workspaceView } from "../../test/workspace";
import { SelectedSection } from "./SelectedSection";

const P = "p-shop000000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const REFUND = "shop/refunds.py#refund_order";
const MOVED = "shop/refunds.py#refund";
const REFUND_CALL = `${REFUND}/call:issue_refund`;
const AUTHOR = "person:lazar";

const orders: Resource = {
  id: "orders",
  kind: "table",
  name: "orders",
  columns: [{ name: "id", key: "primary" }, { name: "customer_id" }, { name: "total" }],
  anchors: [],
  at: [0, 0],
  author: AUTHOR,
};
const customers: Resource = { id: "customers", kind: "table", name: "customers", columns: [{ name: "id", key: "primary" }, { name: "email" }], anchors: [], at: [600, 0], author: AUTHOR };
const relates: Connection = { id: "owner", from: "orders", to: "customers", kind: "relates", author: "agent:claude" };
const drawnLink: Connection = { id: "slack", from: "orders", to: "customers", kind: "other", label: { text: "posts to Slack", author: AUTHOR, signature: "" }, author: AUTHOR };

let engine: FakeEngine;
let refusal: string | null;
let repairs: RepairSpec[];

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  refusal = null;
  repairs = [];
  engine = installFakeEngine({
    handlers: {
      save_board: ({ board }) => {
        if (refusal !== null) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "invalid_board", message: refusal };
        }
        return boardState(P, ID, boardDocument(board.pages as BoardDocument["pages"]));
      },
      repair_board: ({ repair }) => {
        repairs.push(repair);
        const current = store.getOpenBoard(REF)?.document;
        const pages = (current?.pages ?? []).map((item) => (repair.kind === "remove" ? { ...item, cards: [] } : item));
        return boardState(P, ID, boardDocument(pages));
      },
      search: (): SearchResults => ({
        query: "refund",
        definitions: { hits: [{ project: P, key: MOVED, name: "refund", qualifiedName: "refund", path: "shop/refunds.py", kind: "function", line: 9, app: null, score: 600 }], total: 1, truncated: false },
        entrances: { hits: [], total: 0, truncated: false },
        files: { hits: [], total: 0, truncated: false },
      }),
    },
  });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

function openPage(overrides: Partial<Page>): void {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [{ id: "refund", ref: { block: REFUND }, at: [0, 400] }], overrides)])));
}

function currentPage(): Page | undefined {
  return store.getOpenBoard(REF)?.document.pages[0];
}

function renderSelected(selection: Parameters<typeof store.select>[1]) {
  act(() => {
    store.select(PAGE, selection);
  });
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "shop")]));
  queryClient.setQueryData(engineKey("app_info", {}), { username: "lazar", engineVersion: "0" });
  return render(
    <QueryClientProvider client={queryClient}>
      <SelectedSection target={PAGE} />
    </QueryClientProvider>,
  );
}

/** Picks `label` from the menu a menu button opens. */
function pick(button: HTMLElement, menuName: string, label: string): void {
  fireEvent.click(button);
  act(() => {
    within(screen.getByRole("menu", { name: menuName })).getByRole("menuitemcheckbox", { name: label }).click();
  });
}

describe("Selected · a drawn link's kind", () => {
  it("is Other until a kind is picked (Calls only leaves from a call); picking one makes you its author, saved as one undo step", async () => {
    openPage({ resources: [orders, customers], links: [{ ...drawnLink, author: "agent:claude" }] });
    renderSelected({ kind: "link", linkId: "slack" });
    const kind = screen.getByRole("button", { name: "Other" });
    fireEvent.click(kind);
    const menu = screen.getByRole("menu", { name: "Connection kind" });
    expect(within(menu).getAllByRole("menuitemcheckbox").map((item) => item.textContent)).toEqual(["HTTP", "Queue", "Reads", "Writes", "Relates", "Other"]);
    act(() => {
      within(menu).getByRole("menuitemcheckbox", { name: "Writes" }).click();
    });
    await waitFor(() => {
      expect(currentPage()?.links[0]).toMatchObject({ kind: "writes", author: AUTHOR, label: { text: "posts to Slack" } });
    });
    expect(engine.callsOf("save_board")).toHaveLength(1);
    act(() => {
      store.undo(REF);
    });
    expect(currentPage()?.links[0]).toMatchObject({ kind: "other", author: "agent:claude" });
  });
});

describe("Selected · a relationship", () => {
  it("sets its columns from the two tables' columns and its cardinality, each saved as one undo step", async () => {
    openPage({ resources: [orders, customers], links: [relates] });
    renderSelected({ kind: "link", linkId: "owner" });
    const [from, to] = screen.getAllByRole("button", { name: "Column" });
    if (!from || !to) {
      throw new Error("no column pickers");
    }
    pick(from, "Column of orders", "customer_id");
    await waitFor(() => {
      expect(currentPage()?.links[0]).toMatchObject({ fromColumn: "customer_id", author: AUTHOR });
    });
    fireEvent.click(to);
    expect(within(screen.getByRole("menu", { name: "Column of customers" })).getAllByRole("menuitemcheckbox").map((item) => item.textContent)).toEqual(["id", "email"]);
    act(() => {
      within(screen.getByRole("menu", { name: "Column of customers" })).getByRole("menuitemcheckbox", { name: "id" }).click();
    });
    await waitFor(() => {
      expect(currentPage()?.links[0]).toMatchObject({ fromColumn: "customer_id", toColumn: "id" });
    });
    pick(screen.getByRole("button", { name: "Choose" }), "Cardinality", "Many to one");
    await waitFor(() => {
      expect(currentPage()?.links[0]?.cardinality).toBe("many-to-one");
    });
    expect(await screen.findByRole("button", { name: "Many to one" })).not.toBeNull();
    act(() => {
      store.undo(REF);
    });
    expect(currentPage()?.links[0]?.cardinality).toBeUndefined();
    expect(currentPage()?.links[0]?.toColumn).toBe("id");
    act(() => {
      store.redo(REF);
    });
    expect(currentPage()?.links[0]?.cardinality).toBe("many-to-one");
  });

  it("leaves the relationship as it was when the board refuses the change", async () => {
    refusal = "`missing` is not a column of customers";
    openPage({ resources: [orders, customers], links: [relates] });
    renderSelected({ kind: "link", linkId: "owner" });
    pick(screen.getByRole("button", { name: "Choose" }), "Cardinality", "One to one");
    await waitFor(() => {
      expect(engine.callsOf("save_board")).toHaveLength(1);
    });
    expect(currentPage()?.links[0]?.cardinality).toBeUndefined();
    expect(store.canUndo(REF)).toBe(false);
  });
});

describe("Selected · a name held for review", () => {
  const held: HealthItem = { page: "page-1", kind: "review", target: { name: REFUND_CALL }, message: "issue_refund changed since it was named.", repairs: ["accept", "reconnect", "remove"] };

  it("says why, with the card banner's Review: Accept, Reconnect…, and Remove from page", async () => {
    store.adoptBoardState(
      boardState(P, ID, boardDocument([page("page-1", [{ id: "refund", ref: { block: REFUND }, at: [0, 400] }], { names: [{ key: REFUND_CALL, text: "Refund", author: AUTHOR, signature: "" }] })]), {
        health: { ...HEALTHY, items: [held], review: 1 },
      }),
    );
    renderSelected({ kind: "card", cardId: "refund", key: REFUND_CALL });
    expect(screen.getByText(/issue_refund changed since it was named\./)).not.toBeNull();
    expect(screen.getByRole("textbox", { name: "Name" }).hasAttribute("disabled")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Review" }));
    const menu = screen.getByRole("menu", { name: "Review" });
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Accept", "Reconnect…", "Remove from page"]);
    act(() => {
      within(menu).getByRole("menuitem", { name: "Accept" }).click();
    });
    await waitFor(() => {
      expect(repairs).toEqual([{ page: "page-1", kind: "accept", key: REFUND_CALL }]);
    });
  });
});

describe("a card's review", () => {
  const review: HealthItem = { page: "page-1", kind: "review", target: { card: "refund" }, message: "refund_order changed since it was described.", repairs: ["accept", "reconnect", "remove"] };

  const outdated: HealthItem = { page: "page-1", kind: "outdated", target: { card: "refund" }, message: "Code changed since this description was written", repairs: ["resign", "removeDescription"] };

  function renderReview(item: HealthItem = review) {
    openPage({});
    queryClient.setQueryData(engineKey("app_info", {}), { username: "lazar", engineVersion: "0" });
    render(
      <QueryClientProvider client={queryClient}>
        <ReviewButton target={PAGE} project={P} items={[item]} />
      </QueryClientProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Review" }));
    return screen.getByRole("menu", { name: "Review" });
  }

  it("offers Accept, Reconnect…, and Remove from page, each explained", () => {
    const menu = renderReview();
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((item) => item.textContent)).toEqual(["Accept", "Reconnect…", "Remove from page"]);
    expect(items[2]?.className).toContain("danger");
  });

  it("accepts the current code through the board's repair", async () => {
    const menu = renderReview();
    act(() => {
      within(menu).getByRole("menuitem", { name: "Accept" }).click();
    });
    await waitFor(() => {
      expect(repairs).toEqual([{ page: "page-1", kind: "accept", key: REFUND }]);
    });
  });

  it("offers the same menu for a card changed since it was described, without removing only its description", () => {
    const menu = renderReview(outdated);
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Accept", "Reconnect…", "Remove from page"]);
  });

  it("accepts a description written for code that has changed by signing it against the current code", async () => {
    const menu = renderReview(outdated);
    act(() => {
      within(menu).getByRole("menuitem", { name: "Accept" }).click();
    });
    await waitFor(() => {
      expect(repairs).toEqual([{ page: "page-1", kind: "resign", target: { card: "refund" } }]);
    });
  });

  it("reconnects a card changed since it was described by pointing the card at another block", async () => {
    const menu = renderReview(outdated);
    act(() => {
      within(menu).getByRole("menuitem", { name: "Reconnect…" }).click();
    });
    const popover = screen.getByRole("dialog", { name: "Reconnect" });
    fireEvent.change(within(popover).getByRole("searchbox"), { target: { value: "refund" } });
    fireEvent.click(await within(popover).findByRole("option", { name: /refund/ }));
    fireEvent.click(within(popover).getByRole("button", { name: "Reconnect" }));
    await waitFor(() => {
      expect(repairs).toEqual([{ page: "page-1", kind: "reconnect", from: REFUND, to: MOVED }]);
    });
  });

  it("removes a card changed since it was described from the page", async () => {
    const menu = renderReview(outdated);
    act(() => {
      within(menu).getByRole("menuitem", { name: "Remove from page" }).click();
    });
    await waitFor(() => {
      expect(repairs).toEqual([{ page: "page-1", kind: "remove", target: { card: "refund" } }]);
    });
  });

  it("reconnects to a block found by name", async () => {
    const menu = renderReview();
    act(() => {
      within(menu).getByRole("menuitem", { name: "Reconnect…" }).click();
    });
    const popover = screen.getByRole("dialog", { name: "Reconnect" });
    const reconnect = within(popover).getByRole("button", { name: "Reconnect" });
    expect(reconnect.hasAttribute("disabled")).toBe(true);
    fireEvent.change(within(popover).getByRole("searchbox"), { target: { value: "refund" } });
    fireEvent.click(await within(popover).findByRole("option", { name: /refund/ }));
    fireEvent.click(reconnect);
    await waitFor(() => {
      expect(repairs).toEqual([{ page: "page-1", kind: "reconnect", from: REFUND, to: MOVED }]);
    });
  });

  it("removes the card from the page, as one undo step", async () => {
    const menu = renderReview();
    act(() => {
      within(menu).getByRole("menuitem", { name: "Remove from page" }).click();
    });
    await waitFor(() => {
      expect(currentPage()?.cards).toEqual([]);
    });
    expect(repairs).toEqual([{ page: "page-1", kind: "remove", target: { card: "refund" } }]);
    act(() => {
      store.undo(REF);
    });
    expect(currentPage()?.cards.map((item) => item.id)).toEqual(["refund"]);
  });
});

describe("Selected · words the board refuses", () => {
  it("keeps a note as typed, however long, and shows what the save refuses under it while you type", async () => {
    openPage({ notes: [{ id: "n", text: "Read the worker next.", at: [0, 0], author: AUTHOR }] });
    renderSelected({ kind: "note", noteId: "n" });
    refusal = "A note holds at most 400 characters; this one has 401.";
    const long = "x".repeat(401);
    fireEvent.change(screen.getByRole("textbox", { name: "Note" }), { target: { value: long } });
    expect(currentPage()?.notes?.[0]?.text).toBe(long);
    await act(() => store.saveBoard(REF));
    expect(screen.getByText(refusal)).toBeTruthy();
  });

  it("keeps a proposed step's title as typed, and shows what the save refuses under it while you type", async () => {
    openPage({ proposals: [{ id: "p", after: REFUND_CALL, title: "Email them", author: AUTHOR }] });
    renderSelected({ kind: "proposal", proposalId: "p" });
    refusal = "A proposed step's title is one line, without tabs or other control characters.";
    fireEvent.change(screen.getByRole("textbox", { name: "Proposed step title" }), { target: { value: "Email\u0085them" } });
    expect(currentPage()?.proposals?.[0]?.title).toBe("Email\u0085them");
    await act(() => store.saveBoard(REF));
    expect(screen.getByText(refusal)).toBeTruthy();
  });
});
