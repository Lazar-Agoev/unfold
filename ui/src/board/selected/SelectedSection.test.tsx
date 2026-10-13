// @vitest-environment jsdom
import "../../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../../ipc/client";
import type { CallContract, Card, CardView, Page, TypeDefinition } from "../../ipc/generated";
import { engineKey } from "../../ipc/queries";
import * as store from "../../state/boards/store";
import { installFakeEngine, fixture, type FakeEngine } from "../../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../../test/workspace";
import { SelectedSection } from "./SelectedSection";

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const KEY = "src/routes/orders.ts#function:createOrder";
const SLACK = "src/lib/slack.ts#variable:slack/method:post";
const CART = "src/cart.ts#interface:Cart";

const cartModel: CardView = {
  type: "block",
  header: { key: CART, kind: "interface", name: "Cart", qualifiedName: "Cart", path: "src/cart.ts", startLine: 12, endLine: 18, language: "typescript", fileState: "current", source: "current", owner: "src/cart.ts#" },
  code: null,
  flow: null,
};

/** `charge_card(order["card"], total)` at line 19, its value landing in `receipt`; it throws `PaymentDeclined`, caught. */
const CHARGE: CallContract = {
  source: `${KEY}/variable:receipt/call[0]`,
  seq: 0,
  target: "src/lib/payments.ts#function:charge_card",
  callerLine: 19,
  sends: [
    { argument: { index: 0, text: 'order["card"]' }, parameter: { index: 0, name: "card", kind: "positional", optional: false, default: false } },
    { argument: { index: 1, text: "total" }, parameter: { index: 1, name: "amount", kind: "positional", optional: false, default: false } },
  ],
  returns: { binding: "variable", text: "receipt", awaited: false, type: { text: "Receipt", spans: [] } },
  throws: [{ type: { text: "PaymentDeclined", spans: [] }, line: 30 }],
  caught: true,
  calleeLines: [12],
};

const cart: TypeDefinition = { key: CART, kind: "interface", name: "Cart", path: "src/cart.ts", line: 12, language: "typescript", members: [], inherited: [], uses: 6 };

const CARDS: readonly Card[] = [
  { id: "create-order", ref: { block: KEY }, at: [0, 0] },
  { id: "slack", ref: { block: SLACK }, at: [600, 0] },
  { id: "cart", ref: { block: CART }, at: [0, 600] },
];

let engine: FakeEngine;

/** Opens the board with the three cards on its page, and these frames, resource cards, or notes. */
function openPage(overrides: Partial<Page> = {}): void {
  store.resetBoards();
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", CARDS, overrides)])));
}

/** The page as the board holds it now. */
function currentPage(): Page | undefined {
  return store.getOpenBoard(REF)?.document.pages[0];
}

function renderSelected() {
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  queryClient.setQueryData(engineKey("app_info", {}), { username: "lazar", engineVersion: "0" });
  return render(
    <QueryClientProvider client={queryClient}>
      <SelectedSection target={PAGE} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => (key === CART ? cartModel : fixture("card", "card_create_order")),
      type_definition: () => cart,
      call_contract: () => CHARGE,
      type_uses: () => ({ places: [{ owner: KEY, label: "createOrder", path: "src/routes/orders.ts", line: 25 }], total: 1 }),
      relations: () => {
        throw new Error("a type declaration lists no calls");
      },
    },
  });
  openPage();
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

/** The text of each relation row: its label, then its value. */
function relationRows(): string[][] {
  return [...document.querySelectorAll(".relation-row")].map((row) => [row.querySelector(".relation-label")?.textContent ?? "", row.querySelector(".relation-value")?.textContent ?? ""]);
}

describe("Selected · an arrow", () => {
  const LINE = { kind: "line", arrowId: "create-order|call|slack", source: CHARGE.source, seq: 0, from: "create-order", to: "slack", line: 19 } as const;

  it("shows the call as written, then what it sends, returns, and throws, and whether that is caught", async () => {
    act(() => {
      store.select(PAGE, LINE);
    });
    renderSelected();
    expect(await screen.findByRole("heading", { name: 'charge_card(order["card"], total)' })).not.toBeNull();
    expect(screen.getByText("Call · line 19")).not.toBeNull();
    expect(relationRows()).toEqual([
      ["Sends", 'order["card"]→card'],
      ["", "total→amount"],
      ["Returns", "Receipt→receipt"],
      ["Throws", "PaymentDeclinedcaught"],
    ]);
    expect(engine.callsOf("call_contract")).toEqual([{ project: P, source: CHARGE.source, seq: 0 }]);
  });

  it("says when the call sends nothing, where nothing lands, and that a throw isn't caught", async () => {
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        card: () => fixture("card", "card_create_order"),
        call_contract: () => ({ ...CHARGE, sends: [], returns: { binding: "discard", awaited: false }, caught: false }),
      },
    });
    act(() => {
      store.select(PAGE, LINE);
    });
    renderSelected();
    expect(await screen.findByRole("heading", { name: "charge_card()" })).not.toBeNull();
    expect(relationRows()).toEqual([
      ["Sends", "nothing"],
      ["Returns", "?→—"],
      ["Throws", "PaymentDeclined"],
    ]);
  });

  it("shows nothing selected again once the arrow is let go", async () => {
    act(() => {
      store.select(PAGE, LINE);
    });
    renderSelected();
    await screen.findByRole("heading", { name: 'charge_card(order["card"], total)' });
    act(() => {
      store.select(PAGE, { kind: "none" });
    });
    expect(screen.queryByText("Sends")).toBeNull();
    expect(screen.getByText(/Select a card/)).not.toBeNull();
  });
});

describe("Selected", () => {
  it("lists the calls an arrow from a Diagram node stands for, each selecting its call", async () => {
    const calls = [
      { source: `${KEY}/variable:order/call[0]`, seq: 0, line: 32 },
      { source: `${KEY}/expression_statement[0]/call[0]`, seq: 0, line: 33 },
    ];
    act(() => {
      store.select(PAGE, { kind: "calls", arrowId: "create-order|node|slack", from: "create-order", to: "slack", calls });
    });
    renderSelected();
    expect(screen.getByRole("heading", { name: "2 calls" })).not.toBeNull();
    const first = await screen.findByRole("button", { name: /orderService\.place\(…\)/ });
    expect(screen.getByRole("button", { name: /slack\.post\(…\)\s*· line 33/ })).not.toBeNull();
    fireEvent.click(first);
    expect(store.getPageView(PAGE).selection).toEqual({
      kind: "line",
      arrowId: "create-order|node|slack",
      source: `${KEY}/variable:order/call[0]`,
      seq: 0,
      from: "create-order",
      to: "slack",
      line: 32,
    });
  });

  it("says where a type card's type is used, and lists no calls for a type declaration", async () => {
    act(() => {
      store.select(PAGE, { kind: "card", cardId: "cart", key: null });
    });
    renderSelected();
    const uses = await screen.findByRole("button", { name: "Used in 6 places" });
    fireEvent.click(uses);
    const popover = await screen.findByRole("dialog", { name: "Where Cart is used" });
    expect(await within(popover).findByText("createOrder")).not.toBeNull();
    expect(engine.callsOf("relations")).toHaveLength(0);
  });

  it("keeps a space typed at the end of a note while it is edited, and saves the note trimmed", () => {
    openPage({ notes: [{ id: "note", text: "Queued", at: [0, 0], author: "agent:claude" }] });
    act(() => {
      store.select(PAGE, { kind: "note", noteId: "note" });
    });
    renderSelected();
    const field = screen.getByRole<HTMLTextAreaElement>("textbox", { name: "Note" });
    fireEvent.change(field, { target: { value: "Queued " } });
    expect(field.value).toBe("Queued ");
    fireEvent.change(field, { target: { value: "Queued orders\n" } });
    expect(field.value).toBe("Queued orders\n");
    expect(currentPage()?.notes?.[0]).toMatchObject({ text: "Queued orders", author: "person:lazar" });
    fireEvent.blur(field);
    expect(field.value).toBe("Queued orders");
  });

  it("names what a frame holds and brings each into view, a resource card and a note too", () => {
    openPage({
      frames: [{ id: "checkout", title: "Checkout", members: ["cart", "orders", "note"], look: "box", author: "person:lazar" }],
      resources: [{ id: "orders", kind: "table", name: "orders", anchors: [], at: [0, 900], author: "person:lazar" }],
      notes: [{ id: "note", text: "Paid carts only", at: [0, 1000], author: "person:lazar" }],
    });
    const show = (name: string) => {
      act(() => {
        store.select(PAGE, { kind: "frame", frameId: "checkout" });
      });
      fireEvent.click(screen.getByRole("button", { name }));
      return store.getPageView(PAGE);
    };
    renderSelected();
    expect(show("table orders")).toMatchObject({ selection: { kind: "resource", resourceId: "orders" }, focus: { cardIds: ["orders"] } });
    expect(show("Paid carts only")).toMatchObject({ selection: { kind: "note", noteId: "note" }, focus: { cardIds: ["note"] } });
  });
});
