// @vitest-environment jsdom
import type { AgentCardResult, AgentEditSuccess, AgentOp } from "@unfold/desktop/agent-api";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { queryClient } from "../../../ipc/client";
import type { BoardDocument, Card, CardView, Key, Page, TypeDefinition, TypeMember, TypeSpan } from "../../../ipc/generated";
import { engineKey } from "../../../ipc/queries";
import { fixture, installFakeEngine, type FakeEngine } from "../../../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../../../test/workspace";
import * as store from "../store";
import { applyAgentEdit } from "./edit";
import { resetWorking } from "./working";

vi.mock("../../../components/Toast", () => ({ showToast: vi.fn() }));

const P = "p-shop000000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const CREATE = "src/routes/orders.ts#function:createOrder";
const CART = "src/cart.ts#interface:Cart";
const ITEM = "src/cart.ts#interface:CartItem";
const PRICE = "src/cart.ts#class:Price";

let engine: FakeEngine;

function typeModel(key: Key, kind: "interface" | "class", name: string): CardView {
  return {
    type: "block",
    header: { key, kind, name, qualifiedName: name, path: "src/cart.ts", startLine: 1, endLine: 6, language: "typescript", fileState: "current", source: "current", owner: "src/cart.ts#" },
    code: null,
    flow: null,
  };
}

/** A field whose written type is `text`, with one span over its first name. */
function field(name: string, text: string, span: Omit<TypeSpan, "start" | "end"> | null): TypeMember {
  const first = text.replace(/[^\w.].*$/, "");
  return { name, kind: "property", optional: false, readonly: false, default: false, type: { text, spans: span ? [{ start: 0, end: first.length, ...span }] : [] } };
}

const DEFINITIONS: Readonly<Record<Key, TypeDefinition>> = {
  [CART]: {
    key: CART,
    kind: "interface",
    name: "Cart",
    path: "src/cart.ts",
    line: 1,
    language: "typescript",
    members: [
      field("owner", "User", { style: "unresolved", note: "not found" }),
      field("items", "CartItem[]", { style: "declared", target: ITEM }),
      field("createdAt", "Date", { style: "builtin", module: "global" }),
      field("meta", "Stripe.Metadata", { style: "external", module: "stripe", note: "stripe" }),
      field("updatedAt", "Date", { style: "builtin", module: "global" }),
    ],
    inherited: [],
    uses: 2,
  },
  [ITEM]: { key: ITEM, kind: "interface", name: "CartItem", path: "src/cart.ts", line: 8, language: "typescript", members: [field("price", "Price", { style: "declared", target: PRICE })], inherited: [], uses: 1 },
  [PRICE]: { key: PRICE, kind: "class", name: "Price", path: "src/cart.ts", line: 12, language: "typescript", members: [field("amount", "number", null)], inherited: [], uses: 1 },
};

function open(cards: Card[] = []): void {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", cards)])));
}

function onPage(): Page {
  const found = store.getOpenBoard(REF)?.document.pages[0];
  if (!found) {
    throw new Error("the page isn't open");
  }
  return found;
}

async function edit(ops: AgentOp[]): Promise<AgentEditSuccess> {
  const result = await applyAgentEdit({ requestId: 1, board: REF, page: "page-1", agent: { author: "agent:claude", name: "Claude" }, title: "Add cards", ops });
  if (!result.ok) {
    throw new Error(`refused: ${result.code}: ${result.message}`);
  }
  return result;
}

const cardsOf = (result: AgentEditSuccess) => result.items.filter((item): item is AgentCardResult => item.kind === "card");

beforeEach(() => {
  store.resetBoards();
  resetWorking();
  queryClient.clear();
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "shop")]));
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => (key === CREATE ? fixture("card", "card_create_order") : typeModel(key, key === PRICE ? "class" : "interface", key.slice(key.lastIndexOf(":") + 1))),
      type_definition: ({ key }) => {
        const found = DEFINITIONS[key];
        if (!found) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "Not a type." };
        }
        return found;
      },
      relations: ({ key }) => ({ key, callers: [], callersTotal: 0, callersNext: null, outgoing: { onBoard: [], openable: [], externalOrUnresolved: [] }, targetNames: {}, reachedFrom: [] }),
      save_board: (args) => boardState(P, ID, boardDocument(structuredClone(args.board.pages) as BoardDocument["pages"]), { revision: "r2" }),
    },
  });
});

afterEach(() => {
  engine.uninstall();
  store.resetBoards();
});

describe("add_cards with follow types", () => {
  it("adds the types a type's fields name, in its view and right of it, and says where following stopped", async () => {
    open();
    const result = await edit([{ op: "add_cards", cards: [{ key: CART, view: "diagram", follow: { types: 1 } }] }]);
    const [cart, item] = cardsOf(result);
    expect(cardsOf(result).map((card) => [card.name, card.status, card.via])).toEqual([
      ["Cart", "created", undefined],
      ["CartItem", "created", { from: "cart", direction: "types", depth: 1 }],
    ]);
    // Each name following can't go through once, with the first field that writes it.
    expect(cart?.stops).toEqual([
      { reason: "unresolved", type: "User", field: "owner", note: "not found" },
      { reason: "builtin", type: "Date", field: "createdAt", note: "global" },
      { reason: "external", type: "Stripe.Metadata", field: "meta", note: "stripe" },
    ]);
    // CartItem names Price: one declared type past the depth asked for.
    expect(item?.stops).toEqual([{ reason: "depth", count: 1 }]);
    const [cartCard, itemCard] = onPage().cards;
    expect(onPage().cards.map((card) => card.view)).toEqual(["diagram", "diagram"]);
    expect(itemCard?.at[0]).toBeGreaterThan(cartCard?.at[0] ?? 0);
    expect(result.warnings).toEqual([]);

    const deeper = await edit([{ op: "add_cards", cards: [{ key: CART, follow: { types: 2 } }] }]);
    expect(cardsOf(deeper).map((card) => `${card.name} ${card.status}`)).toEqual(["Cart kept", "CartItem kept", "Price created"]);
  });

  it("follows at most two levels of types", async () => {
    open();
    const result = await edit([{ op: "add_cards", cards: [{ key: CART, follow: { types: 9 } }] }]);
    expect(cardsOf(result).map((card) => card.name)).toEqual(["Cart", "CartItem", "Price"]);
  });

  it("warns, and adds nothing more, when the card isn't a type", async () => {
    open();
    const result = await edit([{ op: "add_cards", cards: [{ key: CREATE, follow: { types: 1 } }] }]);
    expect(cardsOf(result).map((card) => card.name)).toEqual(["createOrder"]);
    expect(result.warnings).toEqual(["follow.types follows a type's fields, and createOrder isn't a type declaration or a class, so no types were added for it."]);
  });

  it("places a type beside the type whose field names it, and arranges a type after the types that link to it", async () => {
    open([{ id: "item", ref: { block: ITEM }, at: [0, 0], view: "diagram" }]);
    await edit([{ op: "add_cards", cards: [{ key: PRICE, view: "diagram" }] }]);
    const placed = new Map(onPage().cards.map((card) => [card.id, card.at]));
    expect(placed.get("price")?.[0]).toBeGreaterThan(placed.get("item")?.[0] ?? 0);

    store.editPage({ board: REF, page: "page-1" }, (draft) => {
      for (const card of draft.cards) {
        card.at = card.id === "price" ? [0, 0] : [3000, 3000];
      }
    });
    await edit([{ op: "arrange" }]);
    const arranged = new Map(onPage().cards.map((card) => [card.id, card.at]));
    expect(arranged.get("price")?.[0]).toBeGreaterThan(arranged.get("item")?.[0] ?? 0);
  });
});

describe("set_view on type cards", () => {
  it("expands a class to its detail, taking the older flow and code views too; a type declaration stays its fields", async () => {
    open([
      { id: "price", ref: { block: PRICE }, at: [0, 0], view: "diagram" },
      { id: "cart", ref: { block: CART }, at: [600, 0], view: "diagram" },
    ]);
    const expanded = await edit([{ op: "set_view", items: [{ id: "price", expanded: true }] }]);
    expect(expanded.items).toMatchObject([{ kind: "card", id: "price", status: "updated", expanded: true, expandedCalls: [] }]);
    expect(onPage().cards[0]?.view).toBe("flow");
    const compact = await edit([{ op: "set_view", items: [{ id: "price", view: "diagram" }] }]);
    expect(compact.items).toMatchObject([{ id: "price", status: "updated", expanded: false }]);
    // Older input: "code" (a page-wide lens now) and "flow" both expand it.
    await edit([{ op: "set_view", items: [{ id: "price", view: "code" }] }]);
    expect(onPage().cards[0]?.view).toBe("flow");
    await edit([{ op: "set_view", items: [{ id: "price", expanded: false }] }]);
    const flow = await edit([{ op: "set_view", items: [{ id: "price", view: "flow" }] }]);
    expect(flow.items).toMatchObject([{ id: "price", status: "updated", expanded: true }]);

    for (const item of [{ id: "cart", expanded: true }, { id: "cart", view: "flow" as const }, { id: "cart", view: "code" as const }]) {
      const result = await applyAgentEdit({ requestId: 2, board: REF, page: "page-1", agent: { author: "agent:claude", name: "Claude" }, title: "Set view", ops: [{ op: "set_view", items: [item] }] });
      expect(result.ok ? "applied" : result.message).toContain("is a type card: it keeps its fields in every view");
    }
    const kept = await edit([{ op: "set_view", items: [{ id: "cart", view: "diagram", folded: true }] }]);
    expect(kept.items[0]).not.toHaveProperty("expanded");
    expect(onPage().cards[1]).toMatchObject({ view: "diagram", folded: true });
  });
});
