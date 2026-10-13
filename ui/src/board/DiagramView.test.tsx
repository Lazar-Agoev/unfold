// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { Card, CardView, Key, TypeDefinition, TypeMember, TypeText } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import * as store from "../state/boards/store";
import { installFakeEngine, fixture, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { BoardView } from "./BoardView";

const P = "p-orders0000";
const Q = "p-shop000000";
const ID = ".unfold/local/drafts/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const KEY = "src/routes/orders.ts#function:createOrder";
const ENTRANCE = 'entrance:["manual","command","","import","src/jobs/import-orders.ts#importOrders"]';
const CART = "src/cart.ts#interface:Cart";
const ITEM = "src/cart.ts#interface:CartItem";
const STATUS = "src/cart.ts#type_alias:Status";
const CURRENCY = "src/cart.ts#enum:Currency";
const WIDE = "src/cart.ts#interface:Wide";
const DRAFT = "src/cart.ts#interface:Draft";
const PRICE = "src/cart.ts#class:Price";

if (!("DOMMatrixReadOnly" in globalThis)) {
  // React Flow reads the viewport transform through DOMMatrixReadOnly; jsdom lacks it.
  class Matrix {
    m22 = 1;
  }
  Object.defineProperty(globalThis, "DOMMatrixReadOnly", { configurable: true, value: Matrix });
}

/** A type declaration's card model: its header (a type card reads its fields from `type_definition`). */
function typeModel(key: Key, kind: "interface" | "type_alias" | "enum", name: string, line: number): CardView {
  return {
    type: "block",
    header: { key, kind, name, qualifiedName: name, path: "src/cart.ts", startLine: line, endLine: line + 4, language: "typescript", fileState: "current", source: "current", owner: "src/cart.ts#" },
    code: null,
    flow: null,
  };
}

/** A class's card model: createOrder's Code and Flow under a class header (its fields come from `type_definition`). */
function classModel(key: Key, name: string, line: number): CardView {
  const model = fixture("card", "card_create_order");
  if (model.type !== "block") {
    throw new Error("createOrder's card isn't a block");
  }
  return { ...model, header: { ...model.header, key, kind: "class", name, qualifiedName: name, path: "src/cart.ts", startLine: line } };
}

function written(text: string, target?: Key): TypeText {
  const name = text.replace(/[^\w].*$/, "");
  return { text, spans: target ? [{ start: 0, end: name.length, style: "declared", target }] : [{ start: 0, end: name.length, style: "builtin", module: "global" }] };
}

function member(name: string, type: TypeText | undefined, extra: Partial<TypeMember> = {}): TypeMember {
  return { name, kind: "property", optional: false, readonly: false, default: false, ...(type ? { type } : {}), ...extra };
}

function definition(key: Key, kind: TypeDefinition["kind"], name: string, line: number, extra: Partial<TypeDefinition> = {}): TypeDefinition {
  return { key, kind, name, path: "src/cart.ts", line, language: "typescript", members: [], inherited: [], uses: 2, ...extra };
}

const DEFINITIONS: Readonly<Record<Key, TypeDefinition>> = {
  [CART]: definition(CART, "interface", "Cart", 12, {
    members: [
      member("owner", written("User"), { optional: true }),
      member("items", written("CartItem[]", ITEM)),
      member("createdAt", written("Date"), { readonly: true }),
      member("coupon", written("string"), { default: true }),
    ],
    inherited: [{ from: written("Draft", DRAFT), members: [member("lines", written("CartItem[]", ITEM))] }],
    uses: 6,
  }),
  [ITEM]: definition(ITEM, "interface", "CartItem", 6, { members: [member("sku", written("string"))] }),
  [STATUS]: definition(STATUS, "type_alias", "Status", 20, { aliased: { text: '"open" | "paid"', spans: [] } }),
  [CURRENCY]: definition(CURRENCY, "enum", "Currency", 30, { members: [member("Usd", undefined, { kind: "value", value: '"usd"' }), member("Eur", undefined, { kind: "value", value: '"eur"' })] }),
  [WIDE]: definition(WIDE, "interface", "Wide", 40, { members: Array.from({ length: 14 }, (_, index) => member(`f${index}`, written("string"))) }),
  [PRICE]: definition(PRICE, "class", "Price", 50, { members: [member("amount", written("number"))] }),
};

const MODELS: Readonly<Record<Key, CardView>> = {
  [CART]: typeModel(CART, "interface", "Cart", 12),
  [ITEM]: typeModel(ITEM, "interface", "CartItem", 6),
  [STATUS]: typeModel(STATUS, "type_alias", "Status", 20),
  [CURRENCY]: typeModel(CURRENCY, "enum", "Currency", 30),
  [WIDE]: typeModel(WIDE, "interface", "Wide", 40),
};

let engine: FakeEngine;

function open(cards: Card[], defaultView: "code" | "flow" | "diagram" = "code") {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", cards, { defaultView })], { projects: cards.some((card) => card.ref.project) ? ["shop"] : [] })));
}

function renderBoard() {
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders"), projectRow(Q, "shop")]));
  return render(
    <QueryClientProvider client={queryClient}>
      <div style={{ width: 1200, height: 800 }}>
        <BoardView board={REF} />
      </div>
    </QueryClientProvider>,
  );
}

/** `value`, once it is there (inside `waitFor`, which tries again until it is). */
function present<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) {
    throw new Error("not drawn yet");
  }
  return value;
}

const cardElement = (container: HTMLElement, id: string) => container.querySelector<HTMLElement>(`.board-card[data-card-id="${id}"]`);

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => {
        const model =
          key === KEY ? fixture("card", "card_create_order") : key === ENTRANCE ? fixture("card", "card_first_entrance") : key === PRICE ? classModel(PRICE, "Price", 50) : MODELS[key];
        if (!model) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "no such key" };
        }
        return model;
      },
      type_definition: ({ key }) => {
        const found = DEFINITIONS[key];
        if (!found) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "no such type" };
        }
        return found;
      },
      line_labels: () => [],
      list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
      save_board: () => boardState(P, ID),
    },
  });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("the lens", () => {
  it("has two positions at the top, Diagram and Code, and no switch on the cards", async () => {
    open([{ id: "create-order", ref: { block: KEY }, at: [0, 0] }]);
    const { container } = renderBoard();
    const lens = await screen.findByRole("radiogroup", { name: "Lens" });
    expect(within(lens).getAllByRole("radio").map((radio) => radio.textContent)).toEqual(["Diagram", "Code"]);
    expect(within(lens).getByRole("radio", { name: "Code" }).getAttribute("aria-checked")).toBe("true");
    await waitFor(() => {
      expect(cardElement(container, "create-order")?.querySelector(".code-view")).not.toBeNull();
    });
    expect(cardElement(container, "create-order")?.querySelector(".segmented, .card-expand")).toBeNull();
  });

  it("opens a page whose default is Flow in Diagram, with its cards expanded", async () => {
    open([{ id: "create-order", ref: { block: KEY }, at: [0, 0] }], "flow");
    const { container } = renderBoard();
    const lens = await screen.findByRole("radiogroup", { name: "Lens" });
    expect(within(lens).getByRole("radio", { name: "Diagram" }).getAttribute("aria-checked")).toBe("true");
    await waitFor(() => {
      expect(cardElement(container, "create-order")?.querySelector(".flow-view")).not.toBeNull();
    });
    expect(await screen.findByRole("button", { name: "Collapse createOrder" })).not.toBeNull();
    expect(store.getPageView(PAGE).lens).toBeNull();
  });

  it("shows a compact card as a Diagram node with ⌘1's action and the View menu, without a board edit", async () => {
    open([
      {
        id: "create-order",
        ref: { block: KEY },
        at: [0, 0],
        view: "diagram",
        description: { text: "Places the order now,\nor queues it.", author: "agent:claude", signature: "" },
      },
    ]);
    const { container } = renderBoard();
    await waitFor(() => {
      expect(container.querySelector(".code-view")).not.toBeNull();
    });
    act(() => {
      store.setPageLens(PAGE, "diagram");
    });
    const node = await waitFor(() => {
      const found = container.querySelector<HTMLElement>(".diagram-card");
      expect(found).not.toBeNull();
      return present(found);
    });
    expect(node.querySelector(".card-glyph")?.textContent).toBe("ƒ");
    expect(node.querySelector(".card-title")?.textContent).toBe("createOrder");
    expect(node.querySelector(".card-location")?.textContent).toBe("routes/orders.ts:25");
    // The whole description, wrapped, with the agent mark; no "more".
    expect(node.querySelector(".card-description.open p")?.textContent).toBe("Places the order now,\nor queues it.");
    expect(within(node).getByText("agent")).not.toBeNull();
    expect(within(node).queryByRole("button", { name: "more" })).toBeNull();
    expect(node.querySelector(".code-view")).toBeNull();
    expect(store.getPageView(PAGE).lens).toBe("diagram");
    expect(store.getOpenBoard(REF)?.edits).toBe(0);

    act(() => {
      store.setPageLens(PAGE, "code");
    });
    await waitFor(() => {
      expect(container.querySelector(".diagram-card")).toBeNull();
    });
  });

  it("makes one card compact with its chevron, as a board edit, leaving the others expanded", async () => {
    const place = "src/services/order-service.ts#class:OrderService/method:place";
    open(
      [
        { id: "create-order", ref: { block: KEY }, at: [0, 0] },
        { id: "place", ref: { block: place }, at: [900, 0] },
      ],
      "flow",
    );
    const { container } = renderBoard();
    const collapse = await screen.findByRole("button", { name: "Collapse createOrder" });
    act(() => {
      collapse.click();
    });
    await waitFor(() => {
      expect(container.querySelector('.diagram-card[data-card-id="create-order"]')).not.toBeNull();
    });
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.view).toBe("diagram");
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[1]?.view).toBeUndefined();
    expect(store.getOpenBoard(REF)?.edits).toBe(1);
    expect(container.querySelector('.diagram-card[data-card-id="place"]')).toBeNull();
    // Undo expands it again.
    act(() => {
      store.undo(REF);
    });
    await waitFor(() => {
      expect(cardElement(container, "create-order")?.querySelector(".flow-view")).not.toBeNull();
    });
  });

  it("shows an entrance as a node with its kind, and folds a node to its header", async () => {
    open(
      [
        { id: "import", ref: { entrance: ENTRANCE }, at: [0, 0] },
        { id: "create-order", ref: { block: KEY }, at: [600, 0], folded: true },
      ],
      "diagram",
    );
    const { container } = renderBoard();
    const node = await waitFor(() => {
      const found = cardElement(container, "import");
      expect(found?.querySelector(".node-mark")?.textContent).toBe("COMMAND");
      return present(found);
    });
    expect(node.querySelector(".card-title")?.textContent).toBe("import");
    expect(node.querySelector(".card-location")?.textContent).toBe("unfold.toml:5");
    // The chevron leads the header.
    expect(node.querySelector(".card-header")?.firstElementChild?.getAttribute("aria-label")).toBe("Expand import");
    const folded = cardElement(container, "create-order");
    expect(folded?.classList.contains("folded")).toBe(true);
    expect(folded?.querySelector(".card-description, .card-expand")).toBeNull();
  });

  it("expands an entrance to its declaration evidence and ordered targets, under a heading from their roles", async () => {
    open([{ id: "import", ref: { entrance: ENTRANCE }, at: [0, 0], view: "diagram" }], "diagram");
    const { container } = renderBoard();
    const expand = await screen.findByRole("button", { name: "Expand import" });
    act(() => {
      expand.click();
    });
    const card = await waitFor(() => present(container.querySelector<HTMLElement>('.entrance-card[data-card-id="import"]')));
    expect(card.querySelector(".ecard-location")?.textContent).toBe("unfold.toml:5");
    expect(card.querySelector(".ecard-heading")?.textContent).toBe("Handlers, in order");
    expect([...card.querySelectorAll(".ecard-handler-name")].map((row) => row.textContent)).toEqual(["importOrders"]);
    expect((await within(card).findByRole("button", { name: "Collapse import" })).getAttribute("aria-expanded")).toBe("true");
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.view).toBe("flow");
    // In Code it shows expanded, without a chevron.
    act(() => {
      store.setPageLens(PAGE, "code");
    });
    await waitFor(() => {
      expect(container.querySelector('.entrance-card[data-card-id="import"] .card-expand')).toBeNull();
    });
    expect(container.querySelector('.entrance-card[data-card-id="import"]')).not.toBeNull();
  });

  it("sends a node's calls from its edge, not from their lines", async () => {
    const place = "src/services/order-service.ts#class:OrderService/method:place";
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        card: ({ key }) => {
          if (key === KEY) {
            // createOrder calls `place` twice.
            const model = fixture("card", "card_create_order");
            if (model.type === "block" && model.code?.source.state === "current") {
              const links = model.code.source.links;
              model.code.source.links = [...links, ...links.filter((link) => link.target === place).map((link) => ({ ...link, source: `${link.source}x`, line: link.line + 1 }))];
            }
            return model;
          }
          return { ...fixture("card", "card_create_order"), header: { ...typeModel(place, "interface", "place", 3).header, kind: "method" } } as CardView;
        },
        line_labels: () => [],
        save_board: () => boardState(P, ID),
      },
    });
    open(
      [
        { id: "create-order", ref: { block: KEY }, at: [0, 0] },
        { id: "place", ref: { block: place }, at: [600, 0] },
      ],
      "diagram",
    );
    const { container } = renderBoard();
    await waitFor(() => {
      const ports = [...(cardElement(container, "create-order")?.querySelectorAll(".card-port") ?? [])].map((port) => port.getAttribute("data-handleid"));
      expect(ports).toEqual(["node"]);
    });
  });
});

describe("type cards", () => {
  it("list their fields: names as written, types, readonly, optional, defaults, and inherited fields under their base", async () => {
    open([{ id: "cart", ref: { block: CART }, at: [0, 0] }], "diagram");
    const { container } = renderBoard();
    const card = await waitFor(() => {
      const found = cardElement(container, "cart");
      expect(found?.querySelector(".tcard-fields")).not.toBeNull();
      return present(found);
    });
    expect(card.classList.contains("type-card")).toBe(true);
    expect(card.querySelector(".tcard-kind")?.textContent).toBe("interface");
    expect(card.querySelector(".card-title")?.textContent).toBe("Cart");
    expect(card.querySelector(".card-location")?.textContent).toBe("src/cart.ts:12");
    const cells = [...(card.querySelector(".tcard-fields")?.children ?? [])].map((cell) => cell.textContent);
    expect(cells).toEqual(["owner?", "User", "items", "CartItem[]", "readonly createdAt", "Date", "coupon", "string = …", "from Draft", "lines", "CartItem[]"]);
    // A declared type opens its definition in place. React Flow keeps a node hidden until it has
    // measured it, so its buttons are only accessible (and clickable) once it shows.
    const [declared] = await within(card).findAllByRole("button", { name: "CartItem" });
    fireEvent.click(present(declared));
    expect(await within(card).findByRole("group", { name: "interface CartItem" })).not.toBeNull();
  });

  it("show an alias's type and an enum's values; a type declaration shows its fields in Flow too", async () => {
    open(
      [
        { id: "status", ref: { block: STATUS }, at: [0, 0] },
        { id: "currency", ref: { block: CURRENCY }, at: [0, 400], view: "flow" },
      ],
      "diagram",
    );
    const { container } = renderBoard();
    const cellsOf = async (id: string) =>
      waitFor(() => {
        const cells = [...(cardElement(container, id)?.querySelector(".tcard-fields")?.children ?? [])].map((cell) => cell.textContent);
        expect(cells.length).toBeGreaterThan(0);
        return cells;
      });
    expect(await cellsOf("status")).toEqual(["=", '"open" | "paid"']);
    expect(await cellsOf("currency")).toEqual(["Usd", '"usd"', "Eur", '"eur"']);
    // Type cards have nothing to expand: no chevron, in either lens.
    expect(cardElement(container, "currency")?.querySelector(".card-expand")).toBeNull();
    act(() => {
      store.setPageLens(PAGE, "code");
    });
    expect(await cellsOf("currency")).toEqual(["Usd", '"usd"', "Eur", '"eur"']);
  });

  it("show a class's fields while compact, its existing detail expanded (a saved Flow or legacy Code view), and its Code in the Code lens", async () => {
    open(
      [
        { id: "price", ref: { block: PRICE }, at: [0, 0], view: "diagram" },
        { id: "price-flow", ref: { block: PRICE }, at: [0, 400], view: "flow" },
        { id: "price-code", ref: { block: PRICE }, at: [0, 1200], view: "code" },
        { id: "cart", ref: { block: CART }, at: [900, 0], view: "flow" },
      ],
      "diagram",
    );
    const { container } = renderBoard();
    const fieldsOf = (id: string) => [...(cardElement(container, id)?.querySelector(".tcard-fields")?.children ?? [])].map((cell) => cell.textContent);
    await waitFor(() => {
      expect(fieldsOf("price")).toEqual(["amount", "number"]);
      expect(cardElement(container, "price-flow")?.querySelector(".flow-view")).not.toBeNull();
      expect(cardElement(container, "price-code")?.querySelector(".flow-view")).not.toBeNull();
      expect(fieldsOf("cart")).not.toEqual([]);
    });
    // Compact, a class is a type card with the chevron that expands it; expanded, a block card, not a type card.
    expect(cardElement(container, "price")?.classList.contains("type-card")).toBe(true);
    expect(cardElement(container, "price-flow")?.classList.contains("type-card")).toBe(false);
    expect(cardElement(container, "price-code")?.querySelector(".tcard-fields")).toBeNull();
    expect(await within(present(cardElement(container, "price-flow"))).findByRole("button", { name: "Collapse Price", hidden: true })).not.toBeNull();
    // An interface keeps its fields, with nothing to expand, even when its saved view is Flow.
    expect(cardElement(container, "cart")?.querySelector(".card-expand")).toBeNull();

    const expand = await within(present(cardElement(container, "price"))).findByRole("button", { name: "Expand Price", hidden: true });
    act(() => {
      expand.click();
    });
    await waitFor(() => {
      expect(cardElement(container, "price")?.querySelector(".flow-view")).not.toBeNull();
    });
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.view).toBe("flow");

    act(() => {
      store.setPageLens(PAGE, "code");
    });
    await waitFor(() => {
      for (const id of ["price", "price-flow", "price-code"]) {
        expect(cardElement(container, id)?.querySelector(".code-view")).not.toBeNull();
        expect(cardElement(container, id)?.querySelector(".tcard-fields")).toBeNull();
      }
    });
    expect(fieldsOf("cart")).not.toEqual([]);
  });

  it("fold 12 fields or more behind “N more”", async () => {
    open([{ id: "wide", ref: { block: WIDE }, at: [0, 0] }], "diagram");
    const { container } = renderBoard();
    const more = await waitFor(() => {
      const found = cardElement(container, "wide")?.querySelector<HTMLElement>(".tcard-more");
      expect(found?.textContent).toBe("6 more");
      return present(found);
    });
    expect(cardElement(container, "wide")?.querySelectorAll(".tcard-name")).toHaveLength(8);
    fireEvent.click(more);
    await waitFor(() => {
      expect(cardElement(container, "wide")?.querySelectorAll(".tcard-name")).toHaveLength(14);
    });
  });

  it("anchor a type arrow at its field's row, only to a card of the same project", async () => {
    open(
      [
        { id: "cart", ref: { block: CART }, at: [0, 0] },
        { id: "item-elsewhere", ref: { block: ITEM, project: "shop" }, at: [600, 0] },
      ],
      "diagram",
    );
    const { container } = renderBoard();
    await waitFor(() => {
      expect(cardElement(container, "item-elsewhere")?.querySelector(".tcard-fields")).not.toBeNull();
    });
    expect(cardElement(container, "cart")?.querySelector('[data-handleid^="field:"]')).toBeNull();

    act(() => {
      store.editPage(PAGE, (draft) => {
        draft.cards.push({ id: "item", ref: { block: ITEM }, at: [600, 400] });
      });
    });
    await waitFor(() => {
      const rows = [...(cardElement(container, "cart")?.querySelectorAll(".tcard-type") ?? [])];
      // `items` (row 1) and the inherited `lines` name CartItem; the arrows leave from those rows.
      expect(rows.map((row) => row.querySelector(".react-flow__handle")?.getAttribute("data-handleid") ?? null)).toEqual([null, "field:1", null, null, "field:from0/0"]);
    });
  });
});
