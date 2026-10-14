// @vitest-environment jsdom
import "../../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../../ipc/client";
import type { CallContract, Card, CardView, Page, TypeDefinition } from "../../ipc/generated";
import { engineKey } from "../../ipc/queries";
import * as store from "../../state/boards/store";
import { closeCode, getCodeOpen } from "../../state/codePanel";
import { installFakeEngine, fixture, type FakeEngine } from "../../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../../test/workspace";
import { SelectedSection } from "./SelectedSection";

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
// Keys as the goldens write them; nothing here takes them apart.
const CREATE = "src/routes/orders.ts#createOrder";
const PLACE = "entrance:server/place-an-order";
const CART = "key-cart";
const CHARGE_CALL = "key-charge-call";
const BRANCH = "src/routes/orders.ts#createOrder/if[0]/then";

/** A type's card: its drawn flow's header says it is a type. */
const cartView: CardView = {
  type: "definition",
  flow: {
    header: {
      key: CART,
      kind: "type",
      form: "interface",
      label: "Cart",
      path: "src/cart.ts",
      line: 12,
      async: false,
      generator: false,
      definedWith: [],
      imports: [],
      actions: { after: false, start: false, open: false },
    },
    combinable: [],
    steps: { steps: [] },
    routes: [],
  },
};

/** `charge_card(order["card"], total)` at line 19, its value landing in `receipt`; it raises `PaymentDeclined`, caught. */
const CHARGE: CallContract = {
  call: CHARGE_CALL,
  line: 19,
  text: 'charge_card(order["card"], total)',
  callee: { outcome: "project", key: "key-charge-card" },
  sends: [
    { argument: { index: 0, text: 'order["card"]' }, parameter: { index: 0, name: "card", kind: "positional", optional: false, default: false } },
    { argument: { index: 1, text: "total" }, parameter: { index: 1, name: "amount", kind: "positional", optional: false, default: false } },
  ],
  returns: { type: { text: "Receipt", spans: [] }, awaited: false, lands: { kind: "binds", target: "receipt" } },
  raises: [{ exit: "key-raise", line: 30, type: { text: "PaymentDeclined", spans: [] }, caughtBy: "key-except" }],
};

const cart: TypeDefinition = { key: CART, kind: "interface", name: "Cart", path: "src/cart.ts", line: 12, language: "typescript", members: [], inherited: [], uses: 6 };

const CARDS: readonly Card[] = [
  { id: "create-order", ref: { block: CREATE }, at: [0, 0] },
  { id: "place-an-order", ref: { entrance: PLACE }, at: [-600, 0] },
  { id: "cart", ref: { block: CART }, at: [0, 600] },
];

let engine: FakeEngine;

/** Opens the board with the three cards on its page, and whatever else is given. */
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

function card(key: string): CardView {
  if (key === CART) {
    return cartView;
  }
  return key === PLACE ? fixture("card", "card_place_an_order") : fixture("card", "card_create_order");
}

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => card(key),
      type_definition: () => cart,
      call_contract: () => CHARGE,
      type_uses: () => ({ places: [{ owner: CREATE, label: "createOrder", path: "src/routes/orders.ts", line: 25, kind: "annotation" }], total: 1 }),
      relations: () => fixture("relations", "relations_create_order"),
    },
  });
  openPage();
});

afterEach(() => {
  store.resetBoards();
  closeCode();
  engine.uninstall();
});

/** The text of each relation row: its label, then its value. */
function relationRows(): string[][] {
  return [...document.querySelectorAll(".relation-row")].map((row) => [row.querySelector(".relation-label")?.textContent ?? "", row.querySelector(".relation-value")?.textContent ?? ""]);
}

function selectNow(selection: Parameters<typeof store.select>[1]): void {
  act(() => {
    store.select(PAGE, selection);
  });
}

describe("Selected · a card", () => {
  it("shows its name and where it is, its description, Called by, and Reached from on the relations", async () => {
    selectNow({ kind: "card", cardId: "create-order", key: null });
    renderSelected();
    expect(await screen.findByRole("heading", { name: "createOrder" })).not.toBeNull();
    expect(screen.getByText("src/routes/orders.ts:25")).not.toBeNull();
    await screen.findByText("Server › Place an order");
    expect(relationRows()).toEqual([
      ["Called by", "src/routes/orders.ts"],
      ["Reached from", "Server › Place an order"],
    ]);
    expect(engine.callsOf("relations")).toEqual([{ project: P, key: CREATE, callersLimit: 3 }]);
    expect(engine.callsOf("card")[0]).toEqual({ project: P, key: CREATE });
    fireEvent.change(screen.getByRole("textbox", { name: "Description" }), { target: { value: "Takes an order." } });
    expect(currentPage()?.cards[0]?.description).toEqual({ text: "Takes an order.", author: "person:lazar", signature: "" });
  });

  it("brings a caller onto the page named after it, and an entrance it is reached from as its start card", async () => {
    openPage({ cards: [CARDS[0] ?? { id: "x", ref: { block: "x" }, at: [0, 0] }] });
    selectNow({ kind: "card", cardId: "create-order", key: null });
    renderSelected();
    fireEvent.click(await screen.findByRole("button", { name: "src/routes/orders.ts" }));
    expect(currentPage()?.cards.at(-1)).toMatchObject({ id: "src-routes-orders-ts", ref: { block: "src/routes/orders.ts#" } });
    selectNow({ kind: "card", cardId: "create-order", key: null });
    fireEvent.click(await screen.findByRole("button", { name: "Server › Place an order" }));
    expect(currentPage()?.cards.at(-1)).toMatchObject({ id: "place-an-order", ref: { entrance: PLACE } });
  });

  it("lists the code an entrance starts at, each bringing its card onto the page", async () => {
    selectNow({ kind: "card", cardId: "place-an-order", key: null });
    renderSelected();
    expect(await screen.findByRole("heading", { name: "Place an order" })).not.toBeNull();
    expect(screen.getByText("Server · POST /orders")).not.toBeNull();
    expect(relationRows()).toEqual([
      ["Starts at", "requireUser"],
      ["", "validateBody"],
      ["", "createOrder"],
    ]);
    fireEvent.click(screen.getByRole("button", { name: "requireUser" }));
    expect(currentPage()?.cards.at(-1)).toMatchObject({ id: "require-user", ref: { block: "src/routes/middleware.ts#requireUser" } });
    expect(engine.callsOf("relations")).toHaveLength(0);
  });

  it("says where a type is used, and lists no calls for it", async () => {
    selectNow({ kind: "card", cardId: "cart", key: null });
    renderSelected();
    const uses = await screen.findByRole("button", { name: "Used in 6 places" });
    fireEvent.click(uses);
    const popover = await screen.findByRole("dialog", { name: "Where Cart is used" });
    expect(await within(popover).findByText("createOrder")).not.toBeNull();
    expect(engine.callsOf("relations")).toHaveLength(0);
  });
});

describe("Selected · a block", () => {
  it("shows the block as its card draws it, and names it page-wide", async () => {
    selectNow({ kind: "card", cardId: "create-order", key: BRANCH });
    renderSelected();
    expect(await screen.findByRole("heading", { name: 'req.header("prefer") === "respond-async"' })).not.toBeNull();
    expect(screen.getByText(/^Part · in/).textContent).toBe("Part · in createOrder");
    fireEvent.change(screen.getByRole("textbox", { name: "Name" }), { target: { value: "Wants an async reply?" } });
    expect(currentPage()?.names).toEqual([{ key: BRANCH, text: "Wants an async reply?", author: "person:lazar", signature: "" }]);
    // The card is asked for again with the page's names.
    await waitFor(() => {
      expect(engine.callsOf("card")).toContainEqual({ project: P, key: CREATE, overlay: { names: [{ key: BRANCH, text: "Wants an async reply?" }] } });
    });
    fireEvent.click(screen.getByRole("button", { name: "createOrder" }));
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "card", cardId: "create-order", key: null });
  });
});

describe("Selected · an arrow", () => {
  const LINE = { kind: "line", arrowId: "create-order|charge|charge-card", call: CHARGE_CALL, from: "create-order", to: "cart" } as const;

  it("shows the call as written, then what it sends, returns, and throws, and whether that is caught, by the call's key", async () => {
    selectNow(LINE);
    renderSelected();
    expect(await screen.findByRole("heading", { name: 'charge_card(order["card"], total)' })).not.toBeNull();
    expect(screen.getByText("Call · line 19")).not.toBeNull();
    expect(relationRows()).toEqual([
      ["Sends", 'order["card"]→card'],
      ["", "total→amount"],
      ["Returns", "Receipt→receipt"],
      ["Throws", "PaymentDeclinedcaught"],
    ]);
    expect(engine.callsOf("call_contract")).toEqual([{ project: P, key: CHARGE_CALL }]);
  });

  it("opens a project type's definition in the code panel from its name", async () => {
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        card: ({ key }) => card(key),
        call_contract: () => ({
          ...CHARGE,
          returns: { type: { text: "Receipt | None", spans: [{ start: 0, end: 7, to: { outcome: "project", key: "key-receipt" } }] }, awaited: false, lands: { kind: "binds", target: "receipt" } },
          raises: [{ exit: "key-raise", line: 30, type: { text: "PaymentDeclined", spans: [{ start: 0, end: 15, to: { outcome: "library", module: "stripe" } }] } }],
        }),
      },
    });
    selectNow(LINE);
    renderSelected();
    fireEvent.click(await screen.findByRole("button", { name: "Receipt" }));
    expect(getCodeOpen()).toEqual({ project: P, target: { kind: "block", key: "key-receipt" }, show: null, cardId: null, gap: null });
    // A library's type has no source here: it only names its package.
    expect(screen.queryByRole("button", { name: "PaymentDeclined" })).toBeNull();
  });

  it("says when the call sends nothing, where nothing lands, and that a raise isn't caught", async () => {
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        card: ({ key }) => card(key),
        call_contract: () => ({
          ...CHARGE,
          text: "charge_card()",
          sends: [],
          returns: { awaited: false, lands: { kind: "nowhere" } },
          raises: [{ exit: "key-raise", line: 30, value: "PaymentDeclined(card)" }],
        }),
      },
    });
    selectNow(LINE);
    renderSelected();
    expect(await screen.findByRole("heading", { name: "charge_card()" })).not.toBeNull();
    expect(relationRows()).toEqual([
      ["Sends", "nothing"],
      ["Returns", "?→—"],
      ["Throws", "PaymentDeclined(card)"],
    ]);
  });

  it("shows nothing selected again once the arrow is let go", async () => {
    selectNow(LINE);
    renderSelected();
    await screen.findByRole("heading", { name: 'charge_card(order["card"], total)' });
    selectNow({ kind: "none" });
    expect(screen.queryByText("Sends")).toBeNull();
    expect(screen.getByText(/Select a card/)).not.toBeNull();
  });

  it("lists the calls an arrow stands for, each selecting its call", async () => {
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        card: ({ key }) => card(key),
        call_contract: ({ key }) => ({ ...CHARGE, call: key, text: key === "call-a" ? "orderService.place(order)" : "slack.post(text)", line: key === "call-a" ? 32 : 33 }),
      },
    });
    selectNow({ kind: "calls", arrowId: "create-order|cart", from: "create-order", to: "cart", calls: ["call-a", "call-b"] });
    renderSelected();
    expect(screen.getByRole("heading", { name: "2 calls" })).not.toBeNull();
    const first = await screen.findByRole("button", { name: /orderService\.place\(order\)\s*· line 32/ });
    expect(await screen.findByRole("button", { name: /slack\.post\(text\)\s*· line 33/ })).not.toBeNull();
    fireEvent.click(first);
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "line", arrowId: "create-order|cart", call: "call-a", from: "create-order", to: "cart" });
  });
});

describe("Selected · what the page says", () => {
  it("renames, collapses, and removes a combined step", () => {
    openPage({ combined: [{ id: "checks", title: "Checks", members: [BRANCH], collapsed: false, author: "agent:claude", signature: "" }] });
    selectNow({ kind: "combined", combinedId: "checks" });
    renderSelected();
    expect(screen.getByText(/^COMBINED/).textContent).toBe("COMBINED · 1 block · titled by Claude · agent");
    fireEvent.change(screen.getByRole("textbox", { name: "Combined step title" }), { target: { value: "Guard" } });
    expect(currentPage()?.combined[0]).toMatchObject({ title: "Guard", author: "person:lazar" });
    fireEvent.click(screen.getByRole("button", { name: "Collapse" }));
    expect(currentPage()?.combined[0]?.collapsed).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Uncombine" }));
    expect(currentPage()?.combined).toEqual([]);
  });

  it("says where a proposed step would go, as the page's cards draw it", async () => {
    openPage({ proposals: [{ id: "p", after: BRANCH, title: "Log it", author: "agent:claude" }] });
    selectNow({ kind: "proposal", proposalId: "p" });
    renderSelected();
    expect(await screen.findByText('after req.header("prefer") === "respond-async"')).not.toBeNull();
  });

  it("shows a connection: its kind, label, ends by their names, the call it leaves from, and what it cites", async () => {
    openPage({
      links: [
        {
          id: "enqueue",
          from: "create-order",
          to: "cart",
          kind: "queue",
          label: { text: "place-order job", author: "agent:claude", signature: "" },
          step: CHARGE_CALL,
          channel: "orders",
          anchors: [{ file: "db/schema.sql", lines: [3, 9] }],
          author: "agent:claude",
        },
      ],
    });
    selectNow({ kind: "link", linkId: "enqueue" });
    renderSelected();
    expect(screen.getByText("Queue connection")).not.toBeNull();
    expect(screen.getByRole("textbox", { name: "Label" })).toHaveProperty("value", "place-order job");
    expect(await screen.findByText('charge_card(order["card"], total) · line 19')).not.toBeNull();
    expect(await screen.findByRole("button", { name: "createOrder" })).not.toBeNull();
    expect(screen.getByText("db/schema.sql:3–9")).not.toBeNull();
  });

  it("keeps a space typed at the end of a note while it is edited, and saves the note trimmed", () => {
    openPage({ notes: [{ id: "note", text: "Queued", at: [0, 0], author: "agent:claude" }] });
    selectNow({ kind: "note", noteId: "note" });
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

  it("shows a frame's title, look, author, description, what it holds by name, and the connections in and out", async () => {
    const frame = { look: "box" as const, author: "agent:claude" };
    openPage({
      frames: [
        { id: "system", title: "Order system", members: ["server"], ...frame, look: "zone" },
        { id: "server", title: "Order API", members: ["create-order", "place-an-order"], ...frame },
      ],
      links: [{ id: "to-cart", from: "create-order", to: "cart", kind: "reads", label: { text: "loads", author: "person:maya", signature: "" }, author: "person:maya" }],
    });
    selectNow({ kind: "frame", frameId: "server" });
    renderSelected();
    expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Frame title" }).value).toBe("Order API");
    expect(screen.getByText(/Box frame/).textContent).toContain("Box frame in Order system · drawn by Claude · agent");
    fireEvent.change(screen.getByRole("textbox", { name: "Frame description" }), { target: { value: "Takes orders." } });
    expect(currentPage()?.frames?.[1]?.description).toEqual({ text: "Takes orders.", author: "person:lazar", signature: "" });
    await waitFor(() => {
      expect([...document.querySelectorAll(".relation-row")].map((row) => row.textContent)).toEqual(["HoldscreateOrder", "Place an order", "Lines→ Cart · loads"]);
    });
    fireEvent.click(screen.getByRole("button", { name: "Collapse" }));
    expect(currentPage()?.frames?.[1]?.collapsed).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Remove frame" }));
    expect(currentPage()?.frames).toEqual([{ id: "system", title: "Order system", members: ["create-order", "place-an-order"], ...frame, look: "zone" }]);
  });

  it("names what a frame holds and brings each into view, a resource card and a note too", () => {
    openPage({
      frames: [{ id: "checkout", title: "Checkout", members: ["cart", "orders", "note"], look: "box", author: "person:lazar" }],
      resources: [{ id: "orders", kind: "table", name: "orders", anchors: [], at: [0, 900], author: "person:lazar" }],
      notes: [{ id: "note", text: "Paid carts only", at: [0, 1000], author: "person:lazar" }],
    });
    const show = (name: string) => {
      selectNow({ kind: "frame", frameId: "checkout" });
      fireEvent.click(screen.getByRole("button", { name }));
      return store.getPageView(PAGE);
    };
    renderSelected();
    expect(show("table orders")).toMatchObject({ selection: { kind: "resource", resourceId: "orders" }, focus: { cardIds: ["orders"] } });
    expect(show("Paid carts only")).toMatchObject({ selection: { kind: "note", noteId: "note" }, focus: { cardIds: ["note"] } });
  });
});
