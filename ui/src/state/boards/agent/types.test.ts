// @vitest-environment jsdom
import type { AgentCardResult, AgentEditSuccess, AgentOp } from "@unfold/desktop/agent-api";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { queryClient } from "../../../ipc/client";
import type { BoardDocument, Card, CardView, Key, LinkEnd, Page, TypeDefinition, TypeMember } from "../../../ipc/generated";
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
// Keys are opaque: what each one names comes from its card.
const CREATE = "src/routes/orders.ts#createOrder";
const CART = "key-cart";
const ITEM = "key-cart-item";
const PRICE = "key-price";
const NAMES: Readonly<Record<Key, string>> = { [CART]: "Cart", [ITEM]: "CartItem", [PRICE]: "Price" };

let engine: FakeEngine;

/** A class's or a type's card: its drawn flow's header says which, and names it. */
function typeModel(key: Key): CardView {
  const kind = key === PRICE ? "class" : "type";
  return {
    type: "definition",
    flow: {
      header: {
        key,
        kind,
        form: kind === "class" ? "class" : "interface",
        label: NAMES[key] ?? key,
        path: "src/cart.ts",
        line: 1,
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
}

/** A field whose written type is `text`, with one span over its first name reaching `to`. */
function field(name: string, text: string, to: LinkEnd | null): TypeMember {
  const first = text.replace(/[^\w.].*$/, "");
  return { name, kind: "property", optional: false, readonly: false, default: false, type: { text, spans: to ? [{ start: 0, end: first.length, to }] : [] } };
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
      field("owner", "User", { outcome: "unknown", note: "not found" }),
      field("items", "CartItem[]", { outcome: "project", key: ITEM }),
      field("createdAt", "Date", { outcome: "builtin", module: "global" }),
      field("meta", "Stripe.Metadata", { outcome: "library", module: "stripe", note: "stripe" }),
      field("updatedAt", "Date", { outcome: "builtin", module: "global" }),
    ],
    inherited: [],
    uses: 2,
  },
  [ITEM]: { key: ITEM, kind: "interface", name: "CartItem", path: "src/cart.ts", line: 8, language: "typescript", members: [field("price", "Price", { outcome: "project", key: PRICE })], inherited: [], uses: 1 },
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
      card: ({ key }) => (key === CREATE ? fixture("card", "card_create_order") : typeModel(key)),
      type_definition: ({ key }) => {
        const found = DEFINITIONS[key];
        if (!found) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "Not a type." };
        }
        return found;
      },
      relations: ({ key }) => ({ key, callers: [], callersTotal: 0, callersNext: null, outgoing: [], reachedFrom: [] }),
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
    const result = await edit([{ op: "add_cards", cards: [{ ref: { block: CART }, view: "compact", follow: { types: 1 } }] }]);
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
    expect(onPage().cards.map((card) => card.view)).toEqual(["compact", "compact"]);
    expect(itemCard?.at[0]).toBeGreaterThan(cartCard?.at[0] ?? 0);
    expect(result.warnings).toEqual([]);

    const deeper = await edit([{ op: "add_cards", cards: [{ ref: { block: CART }, follow: { types: 2 } }] }]);
    expect(cardsOf(deeper).map((card) => `${card.name} ${card.status}`)).toEqual(["Cart kept", "CartItem kept", "Price created"]);
  });

  it("follows at most two levels of types", async () => {
    open();
    const result = await edit([{ op: "add_cards", cards: [{ ref: { block: CART }, follow: { types: 9 } }] }]);
    expect(cardsOf(result).map((card) => card.name)).toEqual(["Cart", "CartItem", "Price"]);
  });

  it("warns, and adds nothing more, when the card isn't a type", async () => {
    open();
    const result = await edit([{ op: "add_cards", cards: [{ ref: { block: CREATE }, follow: { types: 1 } }] }]);
    expect(cardsOf(result).map((card) => card.name)).toEqual(["createOrder"]);
    expect(result.warnings).toEqual(["follow.types follows a type's fields, and createOrder isn't a type declaration or a class, so no types were added for it."]);
  });

  it("places a type beside the type whose field names it, and arranges a type after the types that link to it", async () => {
    open([{ id: "item", ref: { block: ITEM }, at: [0, 0], view: "compact" }]);
    await edit([{ op: "add_cards", cards: [{ ref: { block: PRICE }, view: "compact" }] }]);
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
  it("expands a class to its class-level code and makes it compact again; a type's card stays its fields", async () => {
    open([
      { id: "price", ref: { block: PRICE }, at: [0, 0], view: "compact" },
      { id: "cart", ref: { block: CART }, at: [600, 0], view: "compact" },
    ]);
    const expanded = await edit([{ op: "set_view", items: [{ id: "price", view: "expanded" }] }]);
    expect(expanded.items).toMatchObject([{ kind: "card", id: "price", status: "updated", view: "expanded", open: [] }]);
    expect(onPage().cards[0]?.view).toBe("expanded");
    const compact = await edit([{ op: "set_view", items: [{ id: "price", view: "compact" }] }]);
    expect(compact.items).toMatchObject([{ id: "price", status: "updated", view: "compact" }]);

    const result = await applyAgentEdit({ requestId: 2, board: REF, page: "page-1", agent: { author: "agent:claude", name: "Claude" }, title: "Set view", ops: [{ op: "set_view", items: [{ id: "cart", view: "expanded" }] }] });
    expect(result.ok ? "applied" : result.message).toContain("Cart is a type card: it keeps its fields in every view");
    const kept = await edit([{ op: "set_view", items: [{ id: "cart", view: "compact" }] }]);
    expect(kept.items[0]).toMatchObject({ id: "cart", status: "kept" });
    expect(kept.items[0]).not.toHaveProperty("view");
    expect(onPage().cards[1]).toMatchObject({ view: "compact" });
  });
});
