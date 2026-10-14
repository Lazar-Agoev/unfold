// @vitest-environment jsdom
import "../test/dom";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../ipc/client";
import type { Card, CardView, Connection, Health, HealthItem, Page, Resource, TypeDefinition, TypeMember, View } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import { removeCard, setCardView } from "../state/boards";
import * as store from "../state/boards/store";
import { fixture, installFakeEngine, type FakeEngine } from "../test/engine";
import { HEALTHY, boardDocument, boardState, page, projectRow, workspaceView } from "../test/workspace";
import { BoardSidebar } from "../board/BoardSidebar";
import { BoardView } from "../board/BoardView";

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const PAGE = { board: REF, page: "page-1" };
const CREATE = "src/routes/orders.ts#createOrder";
const ORDER = "src/types.ts#Order";
const CUSTOMER = "src/types.ts#Customer";
const JOB = "src/types.ts#OrderJob";

if (!("DOMMatrixReadOnly" in globalThis)) {
  // React Flow reads the viewport transform through DOMMatrixReadOnly; jsdom lacks it.
  class Matrix {
    m22 = 1;
  }
  Object.defineProperty(globalThis, "DOMMatrixReadOnly", { configurable: true, value: Matrix });
}

/** A ResizeObserver that reports each element once it is observed, so React Flow measures the nodes and draws the lines (jsdom sizes every element 400 × 220). */
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

const createOrder = fixture("card", "card_create_order");
/** The call `orderService.place` in createOrder: the step a queue connection leaves from. */
const ENQUEUE = `${CREATE}/call:orderService.place`;

const field = (name: string, extra: Partial<TypeMember> = {}): TypeMember => ({ name, kind: "property", optional: false, readonly: false, default: false, type: { text: "string", spans: [] }, ...extra });
const definition = (key: string, name: string, members: TypeMember[], extra: Partial<TypeDefinition> = {}): TypeDefinition => ({
  key,
  kind: "interface",
  name,
  path: "src/types.ts",
  line: 4,
  language: "typescript",
  members,
  inherited: [],
  uses: 2,
  ...extra,
});
const DEFINITIONS: Readonly<Record<string, TypeDefinition>> = {
  [ORDER]: definition(ORDER, "Order", [field("id"), field("status"), field("note", { optional: true })], {
    inherited: [{ from: { text: "OrderDraft", spans: [] }, members: [field("customerId"), ...Array.from({ length: 10 }, (_, index) => field(`extra${index}`))] }],
  }),
  [CUSTOMER]: definition(CUSTOMER, "Customer", [field("id"), field("email")]),
  [JOB]: definition(JOB, "OrderJob", [field("orderId")]),
};

const cards: Card[] = [
  { id: "create-order", ref: { block: CREATE }, at: [0, 0] },
  { id: "worker", ref: { block: "src/workers/orders.ts#processOrder" }, at: [2400, 0] },
];
const table = (id: string, type: string, at: [number, number], extra: Partial<Resource> = {}): Resource => ({
  id,
  kind: "table",
  name: id,
  type,
  anchors: [{ key: "src/db/repository.ts#OrderRepository" }],
  at,
  author: "agent:claude",
  ...extra,
});
const queue: Connection = {
  id: "enqueue",
  from: "create-order",
  to: "worker",
  kind: "queue",
  step: ENQUEUE,
  channel: "orders",
  carries: JOB,
  label: { text: "place-order job", author: "agent:claude", signature: "" },
  anchors: [{ key: "src/queues/queues.ts#ordersQueue" }, { file: "db/schema.sql", lines: [3, 9] }],
  author: "agent:claude",
};

let engine: FakeEngine;
let repairs: unknown[];

function model(key: string): CardView {
  if (key === CREATE || createOrder.type !== "definition") {
    return createOrder;
  }
  return { type: "definition", flow: { ...createOrder.flow, header: { ...createOrder.flow.header, key, label: "processOrder" }, steps: { steps: [] }, routes: [] } };
}

function open(overrides: Partial<Page>, health: Health = HEALTHY, defaultView: View = "compact") {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", cards, { defaultView, ...overrides })]), { health }));
}

function current(): Page {
  const found = store.getOpenBoard(REF)?.document.pages[0];
  if (!found) {
    throw new Error("the page isn't open");
  }
  return found;
}

function renderBoard(sidebar = false) {
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  queryClient.setQueryData(engineKey("app_info", {}), { username: "maya", engineVersion: "0" });
  return render(
    <QueryClientProvider client={queryClient}>
      <div style={{ width: 1200, height: 800 }}>
        {sidebar ? <BoardSidebar board={REF} /> : null}
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

const resourceCard = (container: HTMLElement, id: string) => container.querySelector<HTMLElement>(`[data-resource-id="${id}"]`);
const chipOf = (container: HTMLElement, id: string) => container.querySelector<HTMLElement>(`.connection-chip[data-link="${id}"]`);

beforeEach(() => {
  store.resetBoards();
  queryClient.clear();
  repairs = [];
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => model(key),
      type_definition: ({ key }) => {
        const found = DEFINITIONS[key];
        if (!found) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "No such type." };
        }
        return found;
      },
      type_uses: () => ({ key: JOB, places: [], total: 0 }),
      list_boards: () => ({ project: P, boards: [], emptyFolders: [] }),
      save_board: (args) => boardState(P, ID, boardDocument(structuredClone(args.board.pages) as Page[])),
      repair_board: (args) => {
        repairs.push(args.repair);
        return boardState(P, ID, store.getOpenBoard(REF)?.document);
      },
    },
  });
});

afterEach(() => {
  store.resetBoards();
  engine.uninstall();
});

describe("resource cards", () => {
  it("draw a table backed by a type: its kind, name, and agent mark, the type's fields with the marks laid on, and 'N more' past 12", async () => {
    open({ resources: [table("orders", ORDER, [0, 600], { columns: [{ name: "id", key: "primary" }, { name: "customerId", key: "foreign", references: "customers.id" }] })] });
    const { container } = renderBoard();
    const card = await waitFor(() => {
      const found = resourceCard(container, "orders");
      expect(found?.querySelector(".rcard-columns")).not.toBeNull();
      return present(found);
    });
    expect(card.querySelector(".rcard-kind")?.textContent).toBe("table");
    expect(card.querySelector(".rcard-glyph")?.getAttribute("data-icon")).toBe("resource-table");
    expect(card.querySelector(".card-title")?.textContent).toBe("orders");
    expect(within(card).getByText("agent")).not.toBeNull();
    const cells = [...(card.querySelector(".rcard-columns")?.children ?? [])].map((cell) => cell.textContent);
    expect(cells.slice(0, 12)).toEqual(["id", "string", "PK", "status", "string", "", "note?", "string", "", "customerId", "string", "FK → customers.id"]);
    // Three own fields and eleven inherited: twelve listed, two behind "N more".
    expect(card.querySelectorAll(".rcard-name")).toHaveLength(12);
    expect(card.querySelector(".rcard-name[title='From OrderDraft']")?.textContent).toBe("customerId");
    fireEvent.click(within(card).getByRole("button", { name: "2 more" }));
    expect(card.querySelectorAll(".rcard-name")).toHaveLength(14);
  });

  it("draw a table that lists its own columns as written, and other kinds as their glyph, word, name, and description", async () => {
    open({
      resources: [
        { id: "items", kind: "table", name: "order_items", columns: [{ name: "sku", type: "text", key: "unique" }, { name: "qty", type: "int", optional: true }], anchors: [], at: [0, 600], author: "person:maya" },
        { id: "emails", kind: "queue", name: "emails", anchors: [], at: [800, 600], author: "agent:claude", description: { text: "Confirmation emails, sent in turn.", author: "agent:claude", signature: "" } },
      ],
    });
    const { container } = renderBoard();
    const items = await waitFor(() => present(resourceCard(container, "items")));
    expect([...(items.querySelector(".rcard-columns")?.children ?? [])].map((cell) => cell.textContent)).toEqual(["sku", "text", "unique", "qty?", "int", ""]);
    expect(within(items).queryByText("agent")).toBeNull();
    const emails = present(resourceCard(container, "emails"));
    expect(emails.querySelector(".rcard-glyph")?.getAttribute("data-icon")).toBe("resource-queue");
    expect(emails.querySelector(".rcard-columns")).toBeNull();
    expect(emails.querySelector(".card-description")?.textContent).toContain("Confirmation emails, sent in turn.");
  });

  it("show what needs review about them with the repairs it offers, applied through repair_board", async () => {
    const outdated: HealthItem = {
      page: "page-1",
      kind: "outdated",
      target: { resource: "orders" },
      message: "The table “orders” changed since it was drawn.",
      repairs: ["resign", "remove"],
    };
    open({ resources: [table("orders", ORDER, [0, 600])] }, { ...HEALTHY, items: [outdated], outdated: 1 });
    const { container } = renderBoard();
    const card = await waitFor(() => present(resourceCard(container, "orders")?.querySelector<HTMLElement>(".card-review")));
    expect(card.textContent).toContain("Changed since it was drawn");
    fireEvent.click(within(card).getByRole("button", { name: "Review" }));
    const popover = await screen.findByRole("dialog", { name: "Review" });
    expect(within(popover).getByText("Remove from page")).not.toBeNull();
    await act(async () => {
      fireEvent.click(present(within(popover).getAllByRole("button", { name: "Apply" })[0]));
      await Promise.resolve();
    });
    await waitFor(() => {
      expect(repairs).toEqual([{ page: "page-1", kind: "resign", target: { resource: "orders" } }]);
    });
  });
});

describe("connections", () => {
  it("draw each kind's line in the authored style, with a chip saying its kind and what travels, and the agent mark", async () => {
    open({
      resources: [table("orders", ORDER, [0, 1200]), table("customers", CUSTOMER, [2400, 1200])],
      links: [
        queue,
        { id: "writes", from: "worker", to: "orders", kind: "writes", author: "agent:claude" },
        { id: "owner", from: "orders", to: "customers", kind: "relates", fromColumn: "customerId", toColumn: "id", cardinality: "many-to-one", author: "person:maya" },
      ],
    });
    const { container } = renderBoard();
    // The carried type's name comes from its definition.
    const chip = await waitFor(() => {
      const found = present(chipOf(container, "enqueue"));
      expect(found.getAttribute("aria-label")).toBe("Connection queue · orders · OrderJob · place-order job");
      return found;
    });
    expect(within(chip).getByText("agent")).not.toBeNull();
    expect(container.querySelector('.board-connection[data-link="enqueue"]')?.classList.contains("kind-queue")).toBe(true);
    expect(container.querySelector('.board-connection[data-link="enqueue"] .connection-head')).not.toBeNull();
    // Writes: the chip names the table; a relationship has crow's-foot ends and no head, and joins columns.
    expect(await waitFor(() => present(chipOf(container, "writes")?.getAttribute("aria-label")))).toBe("Connection writes · orders");
    // A relationship is drawn once its tables' columns (their types' fields) are there to join.
    const relates = await waitFor(() => present(container.querySelector('.board-connection[data-link="owner"]')));
    expect(relates.classList.contains("kind-relates")).toBe(true);
    expect(relates.querySelector(".connection-foot")).not.toBeNull();
    expect(relates.querySelector(".connection-head")).toBeNull();
    expect(chipOf(container, "owner")?.getAttribute("aria-label")).toBe("Connection N : 1");
    expect(resourceCard(container, "orders")?.querySelector('[data-handleid="col-out-right:customerId"]')).not.toBeNull();
    expect(resourceCard(container, "customers")?.querySelector('[data-handleid="col-in-left:id"]')).not.toBeNull();
  });

  it("place their chips in the page's label layout: never over a card or each other", async () => {
    open({
      links: [
        { id: "first", from: "create-order", to: "worker", kind: "http", channel: "POST /orders", author: "agent:claude" },
        { id: "second", from: "create-order", to: "worker", kind: "calls", channel: "Orders.Place", author: "agent:claude" },
        { id: "third", from: "create-order", to: "worker", kind: "other", label: { text: "places the queued order", author: "person:maya", signature: "" }, author: "person:maya" },
      ],
    });
    const { container } = renderBoard();
    const [first, second, third] = await waitFor(() => [present(chipOf(container, "first")), present(chipOf(container, "second")), present(chipOf(container, "third"))]);
    const box = (chip: HTMLElement | undefined, height: number) => {
      const [x, y] = /translate\(([-\d.]+)px, ([-\d.]+)px\)$/.exec(chip?.style.transform ?? "")?.slice(1).map(Number) ?? [Number.NaN, Number.NaN];
      const width = Number.parseFloat(chip?.style.width ?? "");
      return { left: (x ?? 0) - width / 2, right: (x ?? 0) + width / 2, top: (y ?? 0) - height / 2, bottom: (y ?? 0) + height / 2 };
    };
    const labels = [box(first, 20), box(second, 20), box(third, 20)];
    // The three lines draw on top of each other; their labels sit apart, clear of both cards (400px wide at 0 and 2400).
    labels.forEach((a, index) => {
      for (const b of labels.slice(index + 1)) {
        expect(a.right <= b.left || b.right <= a.left || a.bottom <= b.top || b.bottom <= a.top).toBe(true);
      }
      expect(a.left).toBeGreaterThanOrEqual(400);
      expect(a.right).toBeLessThanOrEqual(2400);
    });
  });

  it("leave from their step's call on an expanded card, and from the card's edge when it is compact", async () => {
    open({ links: [queue] }, HEALTHY, "expanded");
    const { container } = renderBoard();
    const port = `step:${ENQUEUE}`;
    await waitFor(() => {
      expect(container.querySelector(`[data-card-id="create-order"] .card-body [data-handleid="${port}"]`)).not.toBeNull();
    });
    expect(container.querySelector('.react-flow__edge[data-id="enqueue"]')).not.toBeNull();
    act(() => {
      store.editPage(PAGE, (draft) => {
        setCardView(draft, "create-order", "compact");
      });
    });
    await waitFor(() => {
      expect(container.querySelector('.block-card.compact[data-card-id="create-order"]')).not.toBeNull();
    });
    expect(container.querySelector(`[data-card-id="create-order"] [data-handleid="${port}"]`)).toBeNull();
  });

  it("are selected from the chip, its carried type's name included", async () => {
    open({ links: [queue] });
    const { container } = renderBoard();
    const chip = await waitFor(() => {
      const found = present(chipOf(container, "enqueue"));
      expect(found.textContent).toContain("OrderJob");
      return found;
    });
    fireEvent.click(chip);
    expect(store.getPageView(PAGE).selection).toEqual({ kind: "link", linkId: "enqueue" });
  });

  it("show a review dot on the chip while something about them needs review", async () => {
    const broken: HealthItem = { page: "page-1", kind: "brokenLink", target: { connection: "enqueue" }, message: "Its carried type is gone.", repairs: ["remove"] };
    open({ links: [queue] }, { ...HEALTHY, items: [broken], brokenLinks: 1 });
    const { container } = renderBoard();
    const chip = await waitFor(() => present(chipOf(container, "enqueue")));
    expect(chip.querySelector(".chip-review")?.getAttribute("aria-label")).toBe("Lost an end or what it cites");
  });
});

describe("notes", () => {
  it("sit at their offset from what they're attached to, and stay where they were, free, when it goes", async () => {
    open({ notes: [{ id: "note", text: "Queued orders are placed by the worker.", at: [0, 300], attach: "create-order", author: "agent:claude" }] });
    const { container } = renderBoard();
    const note = await waitFor(() => present(container.querySelector<HTMLElement>('[data-note-id="note"]')));
    expect(note.textContent).toContain("Queued orders are placed by the worker.");
    expect(within(note).getByText("agent")).not.toBeNull();
    expect(note.closest<HTMLElement>(".react-flow__node")?.style.transform).toContain("translate(0px,300px)");
    act(() => {
      removeCard(PAGE, "create-order");
    });
    expect(current().notes).toEqual([{ id: "note", text: "Queued orders are placed by the worker.", at: [0, 300], author: "agent:claude" }]);
  });
});
