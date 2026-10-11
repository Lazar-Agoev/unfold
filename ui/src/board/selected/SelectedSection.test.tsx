// @vitest-environment jsdom
import "../../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../../ipc/client";
import type { CardView, TypeDefinition } from "../../ipc/generated";
import { engineKey } from "../../ipc/queries";
import * as store from "../../state/boards/store";
import { installFakeEngine, fixture, type FakeEngine } from "../../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../../test/workspace";
import { SelectedSection } from "./SelectedSection";

const P = "p-orders0000";
const ID = ".unfold/local/drafts/b.json";
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

const cart: TypeDefinition = { key: CART, kind: "interface", name: "Cart", path: "src/cart.ts", line: 12, language: "typescript", members: [], inherited: [], uses: 6 };

let engine: FakeEngine;

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
      type_uses: () => ({ places: [{ owner: KEY, label: "createOrder", path: "src/routes/orders.ts", line: 25 }], total: 1 }),
      relations: () => {
        throw new Error("a type declaration lists no calls");
      },
    },
  });
  store.adoptBoardState(
    boardState(
      P,
      ID,
      boardDocument([
        page("page-1", [
          { id: "create-order", ref: { block: KEY }, at: [0, 0] },
          { id: "slack", ref: { block: SLACK }, at: [600, 0] },
          { id: "cart", ref: { block: CART }, at: [0, 600] },
        ]),
      ]),
    ),
  );
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("Selected", () => {
  it("lists the calls an arrow from a Diagram node stands for, each opening its contract on that arrow", async () => {
    const calls = [
      { source: `${KEY}/variable:order/call[0]`, seq: 0, line: 32 },
      { source: `${KEY}/expression_statement[0]/call[0]`, seq: 0, line: 33 },
    ];
    act(() => {
      store.select(PAGE, { kind: "calls", arrowId: "create-order|node|slack", project: P, from: "create-order", to: "slack", calls });
    });
    renderSelected();
    expect(screen.getByRole("heading", { name: "2 calls" })).not.toBeNull();
    const first = await screen.findByRole("button", { name: /orderService\.place\(…\)/ });
    expect(screen.getByRole("button", { name: /slack\.post\(…\)\s*· line 33/ })).not.toBeNull();
    fireEvent.click(first);
    expect(store.getPageView(PAGE).selection).toEqual({
      kind: "line",
      arrowId: "create-order|node|slack",
      project: P,
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
});
