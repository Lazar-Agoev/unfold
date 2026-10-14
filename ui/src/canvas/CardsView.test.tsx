// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { Card, CardView, DrawnFlow, Key, StartCard, TypeDefinition, TypeMember, TypeText, View } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import * as store from "../state/boards/store";
import { setShowAllCalls } from "../state/showAllCalls";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { BoardView } from "../board/BoardView";
import { NODE_PORT, callPort, codePort } from "./arrows";

const P = "p-shop000000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const PLACE = "shop/service.py#place_order";
const LOAD = "shop/store.py#load_order";
const CHARGE = "shop/payments.py#charge_card";
const SAVE = "shop/store.py#save_order";
const API = "shop/api.py#";
const CALLBACK = 'src/routes/orders.ts#/call:ordersRouter.post("/orders/:id")/callback[0]';
const CART = "src/cart.ts#Cart";
const ITEM = "src/cart.ts#CartItem";
const DRAFT = "src/cart.ts#Draft";
const STATUS = "src/cart.ts#Status";
const CURRENCY = "src/cart.ts#Currency";
const WIDE = "src/cart.ts#Wide";
const PRICE = "src/cart.ts#Price";
const START = "entrance:shop-api/place-an-order";
const LOAD_CALL = `${PLACE}/call:load_order`;
const CHARGE_CALL = `${PLACE}/try[0]/body/call:charge_card`;

if (!("DOMMatrixReadOnly" in globalThis)) {
  // React Flow reads the viewport transform through DOMMatrixReadOnly; jsdom lacks it.
  class Matrix {
    m22 = 1;
  }
  Object.defineProperty(globalThis, "DOMMatrixReadOnly", { configurable: true, value: Matrix });
}

/** A ResizeObserver that reports each element once it is observed, so React Flow measures the nodes and draws the arrows. */
class MeasuringResizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe(target: Element): void {
    queueMicrotask(() => {
      this.callback([{ target, contentRect: { x: 0, y: 0, width: 400, height: 220 } } as unknown as ResizeObserverEntry], this);
    });
  }
  unobserve(): void {
    // Each element is reported once.
  }
  disconnect(): void {
    // Each element is reported once.
  }
}

const original = globalThis.ResizeObserver;
beforeAll(() => {
  Object.defineProperty(globalThis, "ResizeObserver", { configurable: true, value: MeasuringResizeObserver });
});
afterAll(() => {
  Object.defineProperty(globalThis, "ResizeObserver", { configurable: true, value: original });
});

const GOLDENS = import.meta.glob<DrawnFlow>("../../../testdata/goldens/drawn/*.json", { eager: true, import: "default" });

/** A drawn flow of the nine examples, as the IPC sends it. */
function drawn(name: string): DrawnFlow {
  const flow = GOLDENS[`../../../testdata/goldens/drawn/${name}.json`];
  if (!flow) {
    throw new Error(`no golden ${name}`);
  }
  return structuredClone(flow);
}

/** A definition with no steps: a callee's card, a type's, or a class's header. */
function header(key: Key, label: string, kind: DrawnFlow["header"]["kind"], form: DrawnFlow["header"]["form"], line = 4): CardView {
  const flow = drawn("02-place-order-ts");
  const { signature: _signature, ...rest } = flow.header;
  return { type: "definition", flow: { ...flow, header: { ...rest, key, label, kind, form, path: "src/cart.ts", line }, steps: { steps: [] }, routes: [] } };
}

function written(text: string, target?: Key): TypeText {
  const name = text.replace(/[^\w].*$/, "");
  return { text, spans: [{ start: 0, end: name.length, to: target ? { outcome: "project", key: target } : { outcome: "builtin" } }] };
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
  }),
  [ITEM]: definition(ITEM, "interface", "CartItem", 6, { members: [member("sku", written("string"))] }),
  [STATUS]: definition(STATUS, "type_alias", "Status", 20, { aliased: { text: '"open" | "paid"', spans: [] } }),
  [CURRENCY]: definition(CURRENCY, "enum", "Currency", 30, { members: [member("Usd", undefined, { kind: "value", value: '"usd"' }), member("Eur", undefined, { kind: "value", value: '"eur"' })] }),
  [WIDE]: definition(WIDE, "interface", "Wide", 40, { members: Array.from({ length: 14 }, (_, index) => member(`f${index}`, written("string"))) }),
  [PRICE]: definition(PRICE, "class", "Price", 50, { members: [member("amount", written("number"))] }),
};

const start = (code: StartCard["code"]): CardView => ({ type: "start", key: START, kind: "entrance", app: "Shop API", name: "Place an order", on: "POST /orders/{order_id}", about: "Charges and saves an order.", code });

/** What the reader draws for each key. */
function model(key: Key): CardView | undefined {
  const callback = drawn("09-orders-route");
  const models: Readonly<Record<Key, CardView>> = {
    [PLACE]: { type: "definition", flow: drawn("01-place-order") },
    [LOAD]: header(LOAD, "load_order", "function", "function"),
    [CHARGE]: header(CHARGE, "charge_card", "function", "function"),
    [SAVE]: header(SAVE, "save_order", "function", "function"),
    [API]: { type: "definition", flow: drawn("08-api") },
    [CALLBACK]: { type: "definition", flow: { ...callback, header: { ...callback.header, key: CALLBACK, kind: "function", form: "lambda", label: "callback to ordersRouter.post", signature: "async (req, res) =>" } } },
    [CART]: header(CART, "Cart", "type", "interface", 12),
    [ITEM]: header(ITEM, "CartItem", "type", "interface", 6),
    [STATUS]: header(STATUS, "Status", "type", "type_alias", 20),
    [CURRENCY]: header(CURRENCY, "Currency", "type", "enum", 30),
    [WIDE]: header(WIDE, "Wide", "type", "interface", 40),
    [PRICE]: { type: "definition", flow: { ...drawn("01-place-order"), header: { ...drawn("01-place-order").header, key: PRICE, kind: "class", form: "class", label: "Price", path: "src/cart.ts", line: 50 } } },
    [START]: start([
      { key: LOAD, state: "found", label: "load_order", path: "shop/store.py", line: 12 },
      { key: "shop/gone.py#gone", state: "missing" },
      { key: CHARGE, state: "found", label: "charge_card", path: "shop/payments.py", line: 12 },
    ]),
  };
  return models[key];
}

let engine: FakeEngine;

function open(cards: Card[], defaultView: View = "compact") {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", cards, { defaultView })])));
}

function renderBoard() {
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "shop")]));
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
const loaded = (container: HTMLElement, id: string) => present(container.querySelector<HTMLElement>(`.board-card[data-card-id="${id}"]:not([data-loading])`));
const portsOf = (element: Element | null | undefined) => [...(element?.querySelectorAll(".card-port") ?? [])].map((port) => port.getAttribute("data-handleid"));

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  setShowAllCalls(false);
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => {
        const found = model(key);
        if (!found) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "no such key" };
        }
        return found;
      },
      type_definition: ({ key }) => {
        const found = DEFINITIONS[key];
        if (!found) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "no such type" };
        }
        return found;
      },
      list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
      save_board: () => boardState(P, ID),
    },
  });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("definition cards", () => {
  it("are compact by default: the chevron, ƒ, the name, where it is, and the whole description", async () => {
    open([{ id: "place", ref: { block: PLACE }, at: [0, 0], description: { text: "Charges the card,\nthen saves.", author: "agent:claude", signature: "" } }]);
    const { container } = renderBoard();
    const card = await waitFor(() => loaded(container, "place"));
    expect(card.classList.contains("compact")).toBe(true);
    expect(card.querySelector(".card-glyph")?.textContent).toBe("ƒ");
    expect(card.querySelector(".card-title")?.textContent).toBe("place_order");
    expect(card.querySelector(".card-location")?.textContent).toBe("shop/service.py:9");
    expect(card.querySelector(".card-description.open p")?.textContent).toBe("Charges the card,\nthen saves.");
    expect(within(card).getByText("agent")).not.toBeNull();
    expect(card.querySelector(".flow, .card-code")).toBeNull();
  });

  it("expand with their chevron, as a board edit: what they say from their code, then their drawn flow", async () => {
    open([{ id: "place", ref: { block: PLACE }, at: [0, 0] }]);
    const { container } = renderBoard();
    const expand = await waitFor(() => present(loaded(container, "place").querySelector<HTMLElement>('[aria-label="Expand place_order"]')));
    act(() => {
      expand.click();
    });
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.view).toBe("expanded");
    const card = await waitFor(() => present(cardElement(container, "place")?.querySelector(".flow") ? cardElement(container, "place") : null));
    expect(card.querySelector(".card-signature")?.textContent).toBe("def place_order(order_id)");
    expect(card.querySelector(".card-doc")?.textContent).toBe("From codeCharge an order and save it.");
    expect([...card.querySelectorAll(".flow-call-name")].map((name) => name.textContent)).toContain("load_order");
    act(() => {
      store.undo(REF);
    });
    await waitFor(() => {
      expect(cardElement(container, "place")?.querySelector(".flow")).toBeNull();
    });
  });

  it("show a module's imports and an inline function's label and λ", async () => {
    open(
      [
        { id: "api", ref: { block: API }, at: [0, 0] },
        { id: "callback", ref: { block: CALLBACK }, at: [0, 900] },
      ],
      "expanded",
    );
    const { container } = renderBoard();
    const api = await waitFor(() => present(loaded(container, "api").querySelector(".card-links")));
    expect(api.textContent).toBe("Importsfastapishop.authshop.service");
    expect(api.querySelector(".card-link.project")?.textContent).toBe("shop.auth");
    expect(loaded(container, "api").querySelector(".card-glyph")?.textContent).toBe("M");
    const callback = await waitFor(() => loaded(container, "callback"));
    expect(callback.querySelector(".card-glyph")?.textContent).toBe("λ");
    expect(callback.querySelector(".card-title")?.textContent).toBe("callback to ordersRouter.post");
    expect(callback.querySelector(".card-signature")?.textContent).toBe("async (req, res) =>");
  });

  it("show a card whose key is gone as a missing card", async () => {
    open([{ id: "gone", ref: { block: "shop/gone.py#gone" }, at: [0, 0] }]);
    const { container } = renderBoard();
    await screen.findByText("This reference is missing from the graph.");
    expect(cardElement(container, "gone")?.classList.contains("missing")).toBe(true);
  });
});

describe("arrows from call blocks", () => {
  it("leave an expanded card from each call's block, and select the call: Selected shows its contract", async () => {
    open([
      { id: "place", ref: { block: PLACE }, at: [0, 0], view: "expanded" },
      { id: "load", ref: { block: LOAD }, at: [900, 0] },
      { id: "charge", ref: { block: CHARGE }, at: [900, 400] },
    ]);
    const { container } = renderBoard();
    await waitFor(() => {
      expect(portsOf(cardElement(container, "place")?.querySelector(".card-body")).sort()).toEqual([callPort(CHARGE_CALL), callPort(LOAD_CALL)].sort());
    });
    const hit = await waitFor(() => present(container.querySelector<HTMLElement>(`.board-arrow-hit[data-arrow="place|${callPort(LOAD_CALL)}|load"]`)));
    fireEvent.click(hit);
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "line", arrowId: `place|${callPort(LOAD_CALL)}|load`, call: LOAD_CALL, from: "place", to: "load" });
  });

  it("leave a compact card from its edge, one arrow standing for every call to a card", async () => {
    const twice = { type: "definition", flow: JSON.parse(JSON.stringify(drawn("01-place-order")).replaceAll(`"key":"${CHARGE}"`, `"key":"${LOAD}"`)) as DrawnFlow } as const;
    engine.uninstall();
    engine = installFakeEngine({ handlers: { card: ({ key }) => (key === PLACE ? twice : (model(key) ?? twice)), save_board: () => boardState(P, ID) } });
    open([
      { id: "place", ref: { block: PLACE }, at: [0, 0] },
      { id: "load", ref: { block: LOAD }, at: [900, 0] },
    ]);
    const { container } = renderBoard();
    await waitFor(() => {
      expect(portsOf(cardElement(container, "place"))).toEqual([NODE_PORT]);
    });
    const hit = await waitFor(() => present(container.querySelector<HTMLElement>(".board-arrow-hit")));
    expect(hit.getAttribute("aria-label")).toBe("2 calls");
    fireEvent.click(hit);
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "calls", arrowId: `place|${NODE_PORT}|load`, from: "place", to: "load", calls: [LOAD_CALL, CHARGE_CALL] });
  });
});

describe("Show all calls", () => {
  it("is passed into each card's query, so the reader draws builtins", async () => {
    open([{ id: "place", ref: { block: PLACE }, at: [0, 0] }]);
    const { container } = renderBoard();
    await waitFor(() => loaded(container, "place"));
    expect(engine.callsOf("card").every((args) => args["showAllCalls"] === undefined)).toBe(true);
    act(() => {
      setShowAllCalls(true);
    });
    await waitFor(() => {
      expect(engine.callsOf("card").some((args) => args["key"] === PLACE && args["showAllCalls"] === true)).toBe(true);
    });
  });
});

describe("app and entrance cards", () => {
  it("show the app in small caps, the name, and what starts it; code that isn't found is dashed with its state", async () => {
    open([
      { id: "start", ref: { entrance: START }, at: [0, 0] },
      { id: "load", ref: { block: LOAD }, at: [600, 0] },
      { id: "charge", ref: { block: CHARGE }, at: [600, 400] },
    ]);
    const { container } = renderBoard();
    const card = await waitFor(() => present(container.querySelector<HTMLElement>('.start-card[data-card-id="start"]')));
    expect(card.querySelector(".start-app small")?.textContent).toBe("Shop API");
    expect(card.querySelector(".start-name")?.textContent).toBe("Place an order");
    expect(card.querySelector(".start-on")?.innerHTML).toBe("<b>POST</b> /orders/{order_id}");
    // Compact: only the code that isn't found is listed; the arrows to its code leave from the edge, in order.
    expect([...card.querySelectorAll(".start-code.unfound")].map((row) => row.textContent)).toEqual(["shop/gone.py#gonemissing"]);
    expect(card.querySelectorAll(".start-code")).toHaveLength(1);
    await waitFor(() => {
      expect(portsOf(card)).toEqual([NODE_PORT]);
    });
  });

  it("list their code in order when expanded, each arrow leaving from its row", async () => {
    open([
      { id: "start", ref: { entrance: START }, at: [0, 0], view: "expanded" },
      { id: "load", ref: { block: LOAD }, at: [600, 0] },
      { id: "charge", ref: { block: CHARGE }, at: [600, 400] },
    ]);
    const { container } = renderBoard();
    const card = await waitFor(() => present(container.querySelector<HTMLElement>('.start-card[data-card-id="start"]')));
    expect(card.querySelector(".start-about")?.textContent).toBe("Charges and saves an order.");
    expect([...card.querySelectorAll(".start-code-name")].map((row) => row.textContent)).toEqual(["load_order", "shop/gone.py#gone", "charge_card"]);
    await waitFor(() => {
      expect(portsOf(card).sort()).toEqual([codePort(0), codePort(2)]);
    });
  });
});

describe("type cards", () => {
  it("list their fields: names as written, types, readonly, optional, defaults, and inherited fields under their base", async () => {
    open([{ id: "cart", ref: { block: CART }, at: [0, 0] }]);
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
    expect(card.querySelector(".type-name")?.textContent).toBe("CartItem");
  });

  it("show an alias's type and an enum's values, in every view, with nothing to expand", async () => {
    open([
      { id: "status", ref: { block: STATUS }, at: [0, 0] },
      { id: "currency", ref: { block: CURRENCY }, at: [0, 400], view: "expanded" },
    ]);
    const { container } = renderBoard();
    const cellsOf = async (id: string) =>
      waitFor(() => {
        const cells = [...(cardElement(container, id)?.querySelector(".tcard-fields")?.children ?? [])].map((cell) => cell.textContent);
        expect(cells.length).toBeGreaterThan(0);
        return cells;
      });
    expect(await cellsOf("status")).toEqual(["=", '"open" | "paid"']);
    expect(await cellsOf("currency")).toEqual(["Usd", '"usd"', "Eur", '"eur"']);
    expect(cardElement(container, "currency")?.querySelector(".card-expand")).toBeNull();
  });

  it("fold 12 fields or more behind “N more”", async () => {
    open([{ id: "wide", ref: { block: WIDE }, at: [0, 0] }]);
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

  it("anchor a type arrow at its field's row", async () => {
    open([
      { id: "cart", ref: { block: CART }, at: [0, 0] },
      { id: "item", ref: { block: ITEM }, at: [600, 400] },
    ]);
    const { container } = renderBoard();
    await waitFor(() => {
      const rows = [...(cardElement(container, "cart")?.querySelectorAll(".tcard-type") ?? [])];
      // `items` (row 1) and the inherited `lines` name CartItem; the arrows leave from those rows.
      expect(rows.map((row) => row.querySelector(".react-flow__handle")?.getAttribute("data-handleid") ?? null)).toEqual([null, "field:1", null, null, "field:from0/0"]);
    });
  });
});

describe("class cards", () => {
  it("show their fields while compact, with the chevron, and their class-level code when expanded", async () => {
    open([
      { id: "price", ref: { block: PRICE }, at: [0, 0] },
      { id: "price-open", ref: { block: PRICE }, at: [0, 400], view: "expanded" },
    ]);
    const { container } = renderBoard();
    const fieldsOf = (id: string) => [...(cardElement(container, id)?.querySelector(".tcard-fields")?.children ?? [])].map((cell) => cell.textContent);
    await waitFor(() => {
      expect(fieldsOf("price")).toEqual(["amount", "number"]);
      expect(cardElement(container, "price-open")?.querySelector(".flow")).not.toBeNull();
    });
    expect(cardElement(container, "price")?.classList.contains("type-card")).toBe(true);
    expect(cardElement(container, "price")?.querySelector(".tcard-kind")?.textContent).toBe("class");
    expect(cardElement(container, "price-open")?.classList.contains("type-card")).toBe(false);
    expect(cardElement(container, "price-open")?.querySelector(".tcard-fields")).toBeNull();
    const expand = present(cardElement(container, "price")?.querySelector<HTMLElement>('[aria-label="Expand Price"]'));
    act(() => {
      expand.click();
    });
    await waitFor(() => {
      expect(cardElement(container, "price")?.querySelector(".flow")).not.toBeNull();
    });
    expect(store.getOpenBoard(REF)?.document.pages[0]?.cards[0]?.view).toBe("expanded");
  });
});
