// @vitest-environment jsdom
import type { AgentConnectionSpec, AgentEditSuccess, AgentFailure, AgentOp } from "@unfold/desktop/agent-api";
import { renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../../../ipc/client";
import type { BoardDocument, Card, CardView, Connection, Frame, Page, Relations, Resource, TypeDefinition, TypeMember } from "../../../ipc/generated";
import { engineKey } from "../../../ipc/queries";
import { fixture, installFakeEngine, type FakeEngine } from "../../../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../../../test/workspace";
import { useShownBoardStore } from "../../shownBoard";
import * as store from "../store";
import { applyAgentEdit } from "./edit";
import { resetWorking, useWorkingItems } from "./working";

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const TARGET = { board: REF, page: "page-1" };
// Keys as the goldens write them; nothing here takes them apart.
const CREATE = "src/routes/orders.ts#createOrder";
const WORKER = "entrance:worker/orders";
/** createOrder's `ordersQueue.add(…)` call, a call its card draws. */
const STEP = "src/routes/orders.ts#createOrder/if[0]/then/call:ordersQueue.add";
const ORDER = "src/types.ts#Order";
const CUSTOMER = "src/types.ts#Customer";
const CLAUDE = { author: "agent:claude", name: "Claude" } as const;

const cards: Card[] = [
  { id: "create-order", ref: { block: CREATE }, at: [0, 0], view: "compact" },
  { id: "worker", ref: { entrance: WORKER }, at: [800, 0] },
];

const field = (name: string, extra: Partial<TypeMember> = {}): TypeMember => ({ name, kind: "property", optional: false, readonly: false, default: false, type: { text: "string", spans: [] }, ...extra });

/** `interface Order extends OrderDraft { id; status }`, and `interface Customer { id; email }`. */
const definitions: Readonly<Record<string, TypeDefinition>> = {
  [ORDER]: {
    key: ORDER,
    kind: "interface",
    name: "Order",
    path: "src/types.ts",
    line: 20,
    language: "typescript",
    members: [field("id"), field("status")],
    inherited: [{ from: { text: "OrderDraft", spans: [] }, members: [field("customerId")] }],
    uses: 3,
  },
  [CUSTOMER]: { key: CUSTOMER, kind: "interface", name: "Customer", path: "src/types.ts", line: 4, language: "typescript", members: [field("id"), field("email")], inherited: [], uses: 1 },
};

const table = (id: string, type: string, extra: Partial<Resource> = {}): Resource => ({ id, kind: "table", name: id, type, anchors: [{ key: "src/db/repository.ts#OrderRepository" }], at: [0, 600], author: "agent:claude", ...extra });

let engine: FakeEngine;
let saves: number;
let refuseSave: { code: string; message: string } | null;

function noCalls(key: string): Relations {
  return { key, callers: [], callersTotal: 0, callersNext: null, outgoing: [], reachedFrom: [] };
}

function open(overrides: Partial<Page> = {}): void {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", cards, overrides)])));
}

function current(): Page {
  const found = store.getOpenBoard(REF)?.document.pages[0];
  if (!found) {
    throw new Error("the page isn't open");
  }
  return found;
}

async function edit(ops: AgentOp[], title = "Connect"): Promise<AgentEditSuccess> {
  const result = await applyAgentEdit({ requestId: 1, board: REF, page: "page-1", agent: CLAUDE, title, ops });
  if (!result.ok) {
    throw new Error(`refused: ${result.code}: ${result.message}`);
  }
  return result;
}

/** A refused edit: nothing of it applies, and the board keeps its history. */
async function refusal(ops: AgentOp[]): Promise<AgentFailure> {
  const before = store.getOpenBoard(REF)?.document;
  const result = await applyAgentEdit({ requestId: 1, board: REF, page: "page-1", agent: CLAUDE, title: "Connect", ops });
  if (result.ok) {
    throw new Error("the edit applied");
  }
  expect(store.getOpenBoard(REF)?.document).toBe(before);
  return result;
}

/** A connection the window passes to the save, which refuses it: its refusal is the answer. Returns the connection as the save was asked to draw it. */
async function savesThenRefuses(connections: AgentConnectionSpec[]): Promise<Connection | undefined> {
  const refused = await refusal([{ op: "connect", connections }]);
  expect(refused).toEqual({ ok: false, ...refuseSave });
  const asked = engine.callsOf("save_board").at(-1) as { readonly board: BoardDocument } | undefined;
  return asked?.board.pages[0]?.links.at(-1);
}

beforeEach(() => {
  store.resetBoards();
  resetWorking();
  queryClient.clear();
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  useShownBoardStore.setState({ board: null });
  saves = 0;
  refuseSave = null;
  engine = installFakeEngine({
    handlers: {
      card: ({ key }): CardView => {
        if (key !== CREATE) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "No such key." };
        }
        return fixture("card", "card_create_order");
      },
      type_definition: ({ key }) => {
        const found = definitions[key];
        if (!found) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "No such type." };
        }
        return found;
      },
      relations: ({ key }) => noCalls(key),
      save_board: (args) => {
        if (refuseSave) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw refuseSave;
        }
        saves += 1;
        // The save fills in a channel from the entrance a connection ends at: the worker's queue.
        const pages = structuredClone(args.board.pages) as BoardDocument["pages"];
        for (const link of pages.flatMap((item) => item.links)) {
          if (link.to === "worker" && link.kind === "queue") {
            link.channel = "orders";
          }
        }
        return boardState(P, ID, boardDocument(pages, { title: args.board.title }), { revision: "r2" });
      },
    },
  });
});

afterEach(() => {
  engine.uninstall();
  store.resetBoards();
});

describe("add_resources", () => {
  it("adds a table backed by a type, its columns the type's fields with the marks laid on, placed beside the card it's near", async () => {
    open({ defaultView: "compact" });
    const result = await edit([
      {
        op: "add_resources",
        resources: [
          {
            kind: "table",
            name: "orders",
            type: ORDER,
            columns: [{ name: "id", key: "primary" }],
            anchors: [{ key: "src/db/repository.ts#OrderRepository" }],
            description: "Placed orders, kept in memory.",
            near: "create-order",
          },
        ],
      },
    ]);
    const [item] = result.items;
    expect(item).toMatchObject({
      kind: "resource",
      id: "orders",
      status: "created",
      resource: "table",
      name: "orders",
      type: ORDER,
      columns: [{ name: "id", type: "string", key: "primary" }, { name: "status", type: "string" }, { name: "customerId", type: "string" }],
    });
    const orders = current().resources?.[0];
    expect(orders).toMatchObject({
      kind: "table",
      type: ORDER,
      columns: [{ name: "id", key: "primary" }],
      author: "agent:claude",
      description: { text: "Placed orders, kept in memory.", author: "agent:claude" },
    });
    // Right of createOrder's compact card, level with its top.
    expect(orders?.at[0]).toBeGreaterThan(240);
    expect(orders?.at[1]).toBe(0);
    expect(store.getOpenBoard(REF)?.history.past.map((entry) => entry.name)).toEqual(["Claude · Connect"]);
    const { result: working } = renderHook(() => useWorkingItems(TARGET));
    expect([...working.current]).toEqual(["orders"]);
  });

  it("updates the card of the same kind and name, keeps one asked as it is, and puts it in a frame", async () => {
    open({ resources: [table("orders", ORDER)], frames: [{ id: "data", title: "data", members: ["create-order"], look: "box", author: "agent:claude" } satisfies Frame] });
    const kept = await edit([{ op: "add_resources", resources: [{ kind: "table", name: "orders", type: ORDER, anchors: [{ key: "src/db/repository.ts#OrderRepository" }] }] }]);
    expect(kept.items).toMatchObject([{ id: "orders", status: "kept" }]);
    expect(saves).toBe(0);
    const moved = await edit([{ op: "add_resources", resources: [{ kind: "table", name: "orders", columns: [{ name: "id", key: "primary" }], type: ORDER, frame: "data" }] }]);
    expect(moved.items).toMatchObject([{ id: "orders", status: "updated", frame: "data" }]);
    expect(current().frames?.[0]?.members).toEqual(["create-order", "orders"]);
    // Anchors left out are kept.
    expect(current().resources?.[0]?.anchors).toEqual([{ key: "src/db/repository.ts#OrderRepository" }]);
  });

  it("refuses a type or columns on anything but a table, marks with types on a type-backed table, and a type that isn't there", async () => {
    open();
    expect((await refusal([{ op: "add_resources", resources: [{ kind: "queue", name: "orders", type: ORDER }] }])).message).toContain("Only a table has a type or columns");
    expect((await refusal([{ op: "add_resources", resources: [{ kind: "table", name: "orders", type: ORDER, columns: [{ name: "id", type: "uuid" }] }] }])).message).toContain(
      "is backed by a type",
    );
    const missing = await refusal([{ op: "add_resources", resources: [{ kind: "table", name: "orders", type: "src/types.ts#Gone" }] }]);
    expect(missing.message).toContain("has no type declaration or class with the key");
    expect((await refusal([{ op: "add_resources", resources: [{ kind: "queue", name: "  " }] }])).code).toBe("invalid_op");
  });

  it("leaves a person's resource card as it is unless the person asked", async () => {
    open({ resources: [table("orders", ORDER, { author: "person:maya" })] });
    const refused = await refusal([{ op: "add_resources", resources: [{ id: "orders", kind: "table", name: "orders", type: CUSTOMER }] }]);
    expect(refused.code).toBe("people_text");
    const result = await edit([{ op: "add_resources", resources: [{ id: "orders", kind: "table", name: "orders", type: CUSTOMER }], replacePeopleText: true }]);
    expect(result.items).toMatchObject([{ status: "updated", type: CUSTOMER }]);
    expect(current().resources?.[0]?.author).toBe("agent:claude");
  });

  it("answers with the engine's refusal when the save won't ground the card, and nothing applies", async () => {
    open();
    refuseSave = {
      code: "board_problem",
      message: 'page `page-1`, resource `orders`: Can\'t add the table "orders": the column "customer_id" doesn\'t appear in its anchors (OrderRepository). Cite where the column is defined, or leave it out.',
    };
    const refused = await refusal([{ op: "add_resources", resources: [{ kind: "table", name: "orders", columns: [{ name: "customer_id" }], anchors: [{ key: "src/db/repository.ts#OrderRepository" }] }] }]);
    expect(refused).toEqual({ ok: false, ...refuseSave });
    expect(store.getOpenBoard(REF)?.history.past).toEqual([]);
  });
});

describe("connect", () => {
  it("draws a queue connection leaving from its step, the save filling in the channel from the worker it ends at", async () => {
    open();
    const result = await edit([
      {
        op: "connect",
        connections: [{ kind: "queue", from: "create-order", step: STEP, to: "worker", carries: "src/types.ts#OrderJob", label: "place-order job", description: "Queued when the client asks.", anchors: [{ key: "src/queues/queues.ts#/call:Queue" }] }],
      },
    ]);
    expect(result.items).toEqual([
      { kind: "link", id: "create-order-queue-worker", status: "created", from: "create-order", to: "worker", name: "place-order job", connection: "queue", step: STEP, channel: "orders", carries: "src/types.ts#OrderJob" },
    ]);
    expect(current().links[0]).toMatchObject({ kind: "queue", author: "agent:claude", step: STEP, channel: "orders", anchors: [{ key: "src/queues/queues.ts#/call:Queue" }], label: { text: "place-order job", author: "agent:claude" }, description: { text: "Queued when the client asks." } });
    // Again, the same connection is found and kept.
    const again = await edit([{ op: "connect", connections: [{ kind: "queue", from: "create-order", step: STEP, to: "worker", carries: "src/types.ts#OrderJob", channel: "orders", anchors: [{ key: "src/queues/queues.ts#/call:Queue" }] }] }]);
    expect(again.items).toMatchObject([{ id: "create-order-queue-worker", status: "kept" }]);
  });

  it("joins two tables' columns with a cardinality, and refuses a column a table doesn't have", async () => {
    open({ resources: [table("orders", ORDER), table("customers", CUSTOMER)] });
    const result = await edit([{ op: "connect", connections: [{ kind: "relates", from: "orders", to: "customers", fromColumn: "customerId", toColumn: "id", cardinality: "many-to-one" }] }]);
    expect(current().links[0]).toMatchObject({ kind: "relates", fromColumn: "customerId", toColumn: "id", cardinality: "many-to-one" });
    expect(result.items).toMatchObject([{ connection: "relates", status: "created" }]);
    // A column the table doesn't have is the save's to refuse, and its refusal is the answer.
    refuseSave = { code: "board_problem", message: "Can't connect the table \"orders\" to the table \"customers\" (relates): \"customer_id\" isn't a column of the table \"orders\"." };
    expect(await savesThenRefuses([{ kind: "relates", from: "orders", to: "customers", fromColumn: "customer_id", toColumn: "id" }])).toMatchObject({ fromColumn: "customer_id" });
  });

  it("refuses a step its from card doesn't draw, and leaves the board's rules to the save: labels, steps that aren't calls, columns, and ends", async () => {
    open({ combined: [{ id: "checks", title: "Checks", members: [`${CREATE}/if[0]`], collapsed: false, author: "agent:claude", signature: "" }] });
    const elsewhere = await refusal([{ op: "connect", connections: [{ kind: "calls", from: "create-order", to: "worker", step: "key-elsewhere" }] }]);
    expect(elsewhere).toMatchObject({ code: "invalid_op" });
    expect(elsewhere.message).toContain("isn't drawn on createOrder");
    expect(engine.callsOf("save_board")).toEqual([]);
    refuseSave = { code: "board_invalid", message: "The board refuses it." };
    expect(await savesThenRefuses([{ kind: "other", from: "create-order", to: "worker" }])).toMatchObject({ kind: "other" });
    // A part of the card is drawn, but isn't a call.
    expect(await savesThenRefuses([{ kind: "calls", from: "create-order", to: "worker", step: `${CREATE}/if[0]/then` }])).toMatchObject({ step: `${CREATE}/if[0]/then` });
    expect(await savesThenRefuses([{ kind: "writes", from: "create-order", to: "worker", cardinality: "one-to-one" }])).toMatchObject({ cardinality: "one-to-one" });
    expect(await savesThenRefuses([{ kind: "http", from: "checks", to: "worker" }])).toMatchObject({ from: "checks" });
    expect(await savesThenRefuses([{ kind: "http", from: "gone", to: "worker" }])).toMatchObject({ from: "gone" });
    expect(await savesThenRefuses([{ kind: "http", from: "create-order", to: "worker", id: "worker" }])).toMatchObject({ id: "worker" });
  });

  it("refuses a connection that repeats a compiled call, from its step or from anywhere in the card without one", async () => {
    const place: Card = { id: "place", ref: { block: "src/services/order-service.ts#OrderService.place" }, at: [400, 0] };
    store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [...cards, place])])));
    const stepped = await refusal([{ op: "connect", connections: [{ kind: "http", from: "create-order", to: "place", step: `${CREATE}/call:orderService.place` }] }]);
    expect(stepped).toMatchObject({ code: "invalid_op" });
    expect(stepped.message).toBe(
      "the http connection from createOrder to src/services/order-service.ts#OrderService.place repeats a compiled call: createOrder calls src/services/order-service.ts#OrderService.place (orderService.place), and Unfold already draws that arrow. Connections are for what the graph can't show: HTTP, queues, data, and where an unknown call goes. get_relations says what Unfold draws.",
    );
    expect((await refusal([{ op: "connect", connections: [{ kind: "other", from: "create-order", to: "place", label: "places it" }] }])).message).toContain("repeats a compiled call");
    // From another call of the card, the line isn't the compiled one.
    const queued = await edit([{ op: "connect", connections: [{ kind: "queue", from: "create-order", to: "place", step: STEP }] }]);
    expect(queued.items).toMatchObject([{ kind: "link", status: "created", connection: "queue" }]);
  });

  it("refuses a connection that repeats a call a collapsed combined step holds, whose arrow leaves from the step", async () => {
    const PLACE = "src/services/order-service.ts#OrderService.place";
    const PLACE_CALL = `${CREATE}/call:orderService.place`;
    const drawn = fixture("card", "card_create_order");
    if (drawn.type !== "definition") {
      throw new Error("createOrder's card draws a flow");
    }
    const steps = drawn.flow.steps.steps.map((step) =>
      step.kind === "call" && step.key === PLACE_CALL
        ? { id: step.id, state: step.state, kind: "combined" as const, item: "place-it", title: "Place it", members: [PLACE_CALL], collapsed: true, holds: [PLACE_CALL], lines: [32, 32] as [number, number], conditions: 0, loops: 0, calls: [{ key: PLACE_CALL, callee: "orderService.place", to: PLACE }], library: 0, unknown: 0 }
        : step,
    );
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        card: ({ key }): CardView => {
          if (key !== CREATE) {
            // eslint-disable-next-line @typescript-eslint/only-throw-error
            throw { code: "not_found", message: "No such key." };
          }
          return { ...drawn, flow: { ...drawn.flow, steps: { ...drawn.flow.steps, steps } } };
        },
        relations: ({ key }) => noCalls(key),
      },
    });
    const place: Card = { id: "place", ref: { block: PLACE }, at: [400, 0] };
    store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [...cards, place])])));
    expect((await refusal([{ op: "connect", connections: [{ kind: "http", from: "create-order", to: "place" }] }])).message).toContain("createOrder calls src/services/order-service.ts#OrderService.place (orderService.place)");
  });

  it("leaves a person's connection as it is unless the person asked, then makes it the agent's", async () => {
    const drawn: Connection = { id: "talks", from: "create-order", to: "worker", kind: "other", label: { text: "sends jobs", author: "person:maya", signature: "" }, author: "person:maya" };
    open({ links: [drawn] });
    expect((await refusal([{ op: "connect", connections: [{ id: "talks", kind: "queue", from: "create-order", to: "worker" }] }])).code).toBe("people_text");
    const result = await edit([{ op: "connect", connections: [{ id: "talks", kind: "queue", from: "create-order", to: "worker" }], replacePeopleText: true }]);
    expect(result.items).toMatchObject([{ id: "talks", status: "updated", connection: "queue", name: "sends jobs" }]);
    expect(current().links[0]).toMatchObject({ kind: "queue", author: "agent:claude", label: { author: "person:maya" } });
  });
});

describe("add_notes", () => {
  it("attaches a note beside its item at an offset, and puts a free one beside the item it's near", async () => {
    open();
    const result = await edit([
      {
        op: "add_notes",
        notes: [
          { text: "Queued orders are placed by the worker.", attach: "create-order" },
          { id: "reading", text: "Start at the worker.", near: "worker" },
        ],
      },
    ]);
    expect(result.items).toMatchObject([
      { kind: "note", id: "note", status: "created", name: "Queued orders are placed by the worker.", attach: "create-order" },
      { kind: "note", id: "reading", status: "created", name: "Start at the worker." },
    ]);
    const [attached, free] = current().notes ?? [];
    // Under createOrder's node: its offset starts at the card's corner.
    expect(attached?.at[0]).toBe(0);
    expect(attached?.at[1]).toBeGreaterThan(40);
    expect(free?.attach).toBeUndefined();
    expect(free?.at[1]).toBeGreaterThan(0);
  });

  it("updates a note by id, keeping its attachment, and refuses blank text and a person's note", async () => {
    open({ notes: [{ id: "n", text: "Read me first.", at: [0, 80], attach: "create-order", author: "agent:claude" }] });
    const result = await edit([{ op: "add_notes", notes: [{ id: "n", text: "Read createOrder first." }] }]);
    expect(result.items).toMatchObject([{ id: "n", status: "updated", attach: "create-order" }]);
    expect(current().notes?.[0]).toMatchObject({ text: "Read createOrder first.", at: [0, 80], attach: "create-order" });
    // Blank text is the save's to refuse.
    refuseSave = { code: "board_invalid", message: "invalid board at /pages/0/notes/1/text: must not be empty" };
    expect(await refusal([{ op: "add_notes", notes: [{ text: " " }] }])).toEqual({ ok: false, ...refuseSave });
    refuseSave = null;
    open({ notes: [{ id: "n", text: "Mine.", at: [0, 80], author: "person:maya" }] });
    expect((await refusal([{ op: "add_notes", notes: [{ id: "n", text: "Theirs." }] }])).code).toBe("people_text");
  });
});

describe("describing and removing claims", () => {
  it("describes resource cards and connections", async () => {
    open({ resources: [table("orders", ORDER)], links: [{ id: "q", from: "create-order", to: "worker", kind: "queue", author: "agent:claude" }] });
    const result = await edit([
      {
        op: "describe",
        items: [
          { id: "orders", text: "Placed orders." },
          { id: "q", text: "The job carries the draft." },
        ],
      },
    ]);
    expect(result.items).toMatchObject([
      { kind: "resource", id: "orders", status: "created" },
      { kind: "link", id: "q", status: "created", connection: "queue" },
    ]);
    const page = current();
    expect(page.resources?.[0]?.description?.text).toBe("Placed orders.");
    expect(page.links.find((link) => link.id === "q")?.description?.text).toBe("The job carries the draft.");
  });

  it("removes resource cards with the links to them, leaving attached notes where they were, and a person's only when asked", async () => {
    open({
      resources: [table("orders", ORDER)],
      links: [{ id: "writes", from: "create-order", to: "orders", kind: "writes", author: "agent:claude" }],
      notes: [
        { id: "kept", text: "In memory.", at: [0, 120], attach: "orders", author: "agent:claude" },
        { id: "maya", text: "Ask Maya.", at: [400, 400], author: "person:maya" },
      ],
    });
    const result = await edit([{ op: "remove_items", ids: ["orders"] }]);
    expect(result.items).toMatchObject([
      { kind: "resource", id: "orders", status: "removed" },
      { kind: "link", id: "writes", status: "removed", connection: "writes" },
    ]);
    expect(current().notes?.[0]).toEqual({ id: "kept", text: "In memory.", at: [0, 720], author: "agent:claude" });
    expect((await refusal([{ op: "remove_items", ids: ["maya"] }])).code).toBe("people_text");
    const removed = await edit([{ op: "remove_items", ids: ["maya"], replacePeopleText: true }]);
    expect(removed.items).toMatchObject([{ kind: "note", id: "maya", status: "removed" }]);
  });

  it("arranges and moves resource cards, their notes going along", async () => {
    open({ resources: [table("orders", ORDER)], notes: [{ id: "n", text: "Note.", at: [0, 120], attach: "orders", author: "agent:claude" }] });
    const result = await edit([{ op: "arrange", moves: [{ id: "orders", rightOf: "worker" }] }]);
    const orders = current().resources?.[0];
    const worker = current().cards.find((card) => card.id === "worker");
    expect(orders && worker && orders.at[0]).toBeGreaterThan(worker?.at[0] ?? 0);
    expect(result.items.filter((item) => item.kind === "resource")).toMatchObject([{ kind: "resource", id: "orders", status: "updated" }]);
    // Arrange puts the note under the table it is attached to; the move takes it along.
    const note = current().notes?.[0];
    expect(note?.attach).toBe("orders");
    expect(note?.at[0]).toBe(0);
    expect(note?.at[1]).toBeGreaterThan(80);
  });

  it("arranges a page of resource cards alone", async () => {
    store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [], { resources: [table("orders", ORDER, { at: [0, 3000] }), table("customers", CUSTOMER, { at: [6000, 0] })] })])));
    const result = await edit([{ op: "arrange" }]);
    expect(result.items.filter((item) => item.kind === "resource").map((item) => item.id).sort()).toEqual(["customers", "orders"]);
  });

  it("puts a note on a connection beside its line, clear of the cards, when it arranges", async () => {
    open({
      resources: [table("orders", ORDER)],
      links: [{ id: "writes", from: "worker", to: "orders", kind: "writes", author: "agent:claude" }],
      notes: [{ id: "n", text: "In memory.", at: [-900, 0], attach: "writes", author: "agent:claude" }],
    });
    await edit([{ op: "arrange" }]);
    const arranged = current();
    const note = arranged.notes?.[0];
    const [worker, orders] = [arranged.cards.find((card) => card.id === "worker"), arranged.resources?.[0]];
    // The offset from the midpoint of its ends' corners now puts it between them, not at the old offset.
    expect(note?.attach).toBe("writes");
    expect(note?.at).not.toEqual([-900, 0]);
    const middle = [((worker?.at[0] ?? 0) + (orders?.at[0] ?? 0)) / 2, ((worker?.at[1] ?? 0) + (orders?.at[1] ?? 0)) / 2];
    const x = (middle[0] ?? 0) + (note?.at[0] ?? 0);
    expect(x).toBeGreaterThan(Math.min(worker?.at[0] ?? 0, orders?.at[0] ?? 0) - 400);
    expect(x).toBeLessThan(Math.max(worker?.at[0] ?? 0, orders?.at[0] ?? 0) + 400);
  });
});
