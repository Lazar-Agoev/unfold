// @vitest-environment jsdom
import "../../test/dom";
import { act, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { CallContract, TypeDefinition, TypeUses } from "../../ipc/generated";
import { engineKey } from "../../ipc/queries";
import * as store from "../../state/boards/store";
import { usePeekTarget } from "../../state/overlays";
import { renderWithClient, testQueryClient } from "../../test/render";
import { boardDocument, boardState, page } from "../../test/workspace";
import { LineContract, bindingText, sentText } from "./LineContract";

const P = "p-shop000000";
const ID = ".unfold/local/drafts/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const CALL = "src/orders/create.ts#function:createOrder/try[0]/call[0]";
const CHARGE_PAYMENT = "src/payments.ts#function:chargePayment";
const CART = "src/cart.ts#interface:Cart";
const CART_ITEM = "src/cart.ts#interface:CartItem";
const USER = "src/user.ts#interface:User";
const CHARGE = "src/payments.ts#interface:Charge";
const DECLINED = "src/errors.ts#class:CardDeclinedError";

const LINE = { arrowId: "a|call|b", project: P, source: CALL, seq: 0, from: "a", to: "b", line: 18 };

const contract: CallContract = {
  source: CALL,
  seq: 0,
  target: CHARGE_PAYMENT,
  callerLine: 18,
  sends: [
    {
      argument: { index: 0, text: "cart" },
      parameter: { index: 0, name: "cart", kind: "positional", optional: false, default: false, type: { text: "Cart", spans: [{ start: 0, end: 4, style: "declared", target: CART }] } },
    },
    {
      argument: { index: 1, text: "req.user" },
      parameter: { index: 1, name: "user", kind: "positional", optional: false, default: false, type: { text: "User", spans: [{ start: 0, end: 4, style: "declared", target: USER }] } },
    },
  ],
  returns: {
    binding: "variable",
    text: "charge",
    awaited: true,
    type: { text: "Promise<Charge>", spans: [{ start: 0, end: 7, style: "builtin" }, { start: 8, end: 14, style: "declared", target: CHARGE }] },
  },
  throws: [{ type: { text: "CardDeclinedError", spans: [{ start: 0, end: 17, style: "declared", target: DECLINED }] }, line: 27 }],
  caught: true,
  calleeLines: [22, 23, 24, 27],
};

const cart: TypeDefinition = {
  key: CART,
  kind: "interface",
  name: "Cart",
  path: "src/cart.ts",
  line: 4,
  language: "typescript",
  members: [
    { name: "id", kind: "property", optional: false, readonly: false, default: false, type: { text: "CartId", spans: [{ start: 0, end: 6, style: "declared", target: "src/cart.ts#type_alias:CartId" }] } },
    { name: "items", kind: "property", optional: false, readonly: false, default: false, type: { text: "CartItem[]", spans: [{ start: 0, end: 8, style: "declared", target: CART_ITEM }] } },
    { name: "coupon", kind: "property", optional: true, readonly: false, default: false, type: { text: "string", spans: [] } },
  ],
  inherited: [],
  uses: 6,
};

const cartItem: TypeDefinition = {
  key: CART_ITEM,
  kind: "interface",
  name: "CartItem",
  path: "src/cart.ts",
  line: 7,
  language: "typescript",
  members: [{ name: "sku", kind: "property", optional: false, readonly: false, default: false, type: { text: "string", spans: [] } }],
  inherited: [],
  uses: 3,
};

const uses: TypeUses = {
  total: 6,
  places: [
    { owner: CHARGE_PAYMENT, label: "chargePayment", path: "src/payments.ts", line: 14 },
    { owner: "src/payments.ts#function:total", label: "total", path: "src/payments.ts", line: 22 },
  ],
};

function renderContract() {
  const client = testQueryClient();
  client.setQueryData(engineKey("call_contract", { project: P, source: CALL, seq: 0 }), contract);
  client.setQueryData(engineKey("type_definition", { project: P, key: CART }), cart);
  client.setQueryData(engineKey("type_definition", { project: P, key: CART_ITEM }), cartItem);
  client.setQueryData(engineKey("type_uses", { project: P, key: CART }), uses);
  return renderWithClient(<LineContract target={PAGE} line={LINE} x={0} y={0} />, client);
}

function PeekProbe() {
  const peek = usePeekTarget();
  return <output aria-label="peek">{peek ? `${peek.path}:${peek.line ?? ""}` : ""}</output>;
}

beforeEach(() => {
  store.resetBoards();
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1")])));
  store.select(PAGE, { kind: "line", ...LINE });
});

afterEach(() => {
  store.resetBoards();
});

describe("the line's contract", () => {
  it("lists what the call sends, returns, and throws, caller on the left and callee on the right", () => {
    renderContract();
    const section = screen.getByRole("region", { name: "Call to chargePayment" });
    const rows = [...section.querySelectorAll(".line-contract-row")].map((row) => [...row.children].map((cell) => cell.textContent));
    expect(rows).toEqual([
      ["Sends", "cart", "cart: Cart"],
      ["", "req.user", "user: User"],
      ["Returns", "charge", "Promise<Charge>"],
      ["Throws", "catch", "CardDeclinedError"],
    ]);
  });

  it("opens a written type under its row, nests its members' types, and closes it again", () => {
    renderContract();
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Cart" }));
    });
    expect(store.getPageView(PAGE).openTypes).toEqual({ "send:0": 0 });
    const definition = screen.getByRole("group", { name: "interface Cart" });
    expect(within(definition).getByText("cart.ts:4")).not.toBeNull();
    expect(within(definition).getByText("coupon?")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Cart" }).getAttribute("aria-expanded")).toBe("true");

    act(() => {
      fireEvent.click(within(definition).getByRole("button", { name: "CartItem" }));
    });
    expect(screen.getByRole("group", { name: "interface CartItem" })).not.toBeNull();
    expect(store.getPageView(PAGE).openTypes).toEqual({ "send:0": 0, "send:0/1": 0 });

    // Closing the row closes what was opened inside it.
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Cart" }));
    });
    expect(screen.queryByRole("group", { name: "interface Cart" })).toBeNull();
    expect(store.getPageView(PAGE).openTypes).toEqual({});
  });

  it("lists where a type is used, and opens the peek sheet at a place or at the definition", () => {
    renderWithClient(<PeekProbe />);
    renderContract();
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Cart" }));
    });
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Used in 6 places" }));
    });
    const popover = screen.getByRole("dialog", { name: "Where Cart is used" });
    const places = within(popover).getAllByRole("listitem");
    expect(places.map((place) => place.textContent)).toEqual(["chargePaymentpayments.ts:14", "totalpayments.ts:22"]);
    const total = places[1];
    if (!total) {
      throw new Error("the popover lists two places");
    }
    act(() => {
      fireEvent.click(total);
    });
    expect(screen.getByRole("status", { hidden: true, name: "peek" }).textContent).toBe("src/payments.ts:22");
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Peek source" }));
    });
    expect(screen.getByRole("status", { hidden: true, name: "peek" }).textContent).toBe("src/cart.ts:4");
  });

  it("forgets the open types when the selection moves off the line", () => {
    store.toggleOpenType(PAGE, "returns", 1);
    expect(store.getPageView(PAGE).openTypes).toEqual({ returns: 1 });
    store.select(PAGE, { kind: "line", ...LINE });
    expect(store.getPageView(PAGE).openTypes).toEqual({ returns: 1 });
    store.select(PAGE, { kind: "none" });
    expect(store.getPageView(PAGE).openTypes).toEqual({});
  });
});

describe("the caller's column", () => {
  it("says where the result lands and what fills a parameter", () => {
    expect(bindingText({ binding: "variable", text: "charge", awaited: true })).toEqual({ text: "charge", dim: false });
    expect(bindingText({ binding: "argument", text: "toOrder", awaited: false })).toEqual({ text: "toOrder(…)", dim: false });
    expect(bindingText({ binding: "discard", awaited: false }).dim).toBe(true);
    expect(sentText({ parameter: { index: 2, name: "currency", kind: "positional", optional: true, default: true } })).toEqual({ text: "default", dim: true });
    expect(sentText({ argument: { index: 0 }, unpaired: { reason: "spread", note: "spread" } })).toEqual({ text: "…", dim: true });
  });
});
