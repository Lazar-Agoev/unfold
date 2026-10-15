import { produce } from "immer";
import { describe, expect, it } from "vitest";
import type { Card, Page } from "../../ipc/generated";
import { page } from "../../test/workspace";
import { addConnection, claimOf, saysConnection, setConnectionClaim, setConnectionKind, setLinkDescription, setLinkLabel } from "./connections";
import { moveItems, removeItems } from "./document";
import { moveFrame } from "./frames";
import { addNote, placeNote, setNoteText } from "./notes";
import { addResource, saysClaim, setResourceClaim, setResourceDescription } from "./resources";

const cards: Card[] = [
  { id: "create-order", ref: { block: "src/routes/orders.ts#createOrder" }, at: [0, 0] },
  { id: "worker", ref: { entrance: "entrance:worker" }, at: [600, 0] },
];
const base = (): Page => page("page-1", cards);
const STEP = "src/routes/orders.ts#createOrder/if[0]/then/call:ordersQueue.add";

describe("resource cards", () => {
  it("are added with what they say, a table's type and columns only on a table", () => {
    const next = produce(base(), (draft) => {
      addResource(draft, { kind: "table", name: "orders", type: "src/types.ts#Order", columns: [{ name: "id", key: "primary" }], anchors: [{ key: "src/db/repository.ts#OrderRepository" }], at: [10.4, 20.6], author: "agent:claude" });
      addResource(draft, { kind: "queue", name: "orders", type: "src/types.ts#Order", columns: [{ name: "x" }], anchors: [], at: [0, 0], author: "agent:claude" });
    });
    expect(next.resources).toEqual([
      { id: "orders", kind: "table", name: "orders", type: "src/types.ts#Order", columns: [{ name: "id", key: "primary" }], anchors: [{ key: "src/db/repository.ts#OrderRepository" }], at: [10, 21], author: "agent:claude" },
      { id: "orders-2", kind: "queue", name: "orders", anchors: [], at: [0, 0], author: "agent:claude" },
    ]);
  });

  it("change hands when what they say changes, and keep their description apart", () => {
    const start = produce(base(), (draft) => {
      addResource(draft, { kind: "cache", name: "sessions", anchors: [], at: [0, 0], author: "person:maya" });
      setResourceDescription(draft, "sessions", "Who is signed in.", "person:maya");
    });
    const resource = start.resources?.[0];
    expect(resource && saysClaim(resource, { kind: "cache", name: "sessions", anchors: [] })).toBe(true);
    const same = produce(start, (draft) => {
      setResourceClaim(draft, "sessions", { kind: "cache", name: "sessions", anchors: [] }, "agent:claude");
    });
    expect(same).toBe(start);
    const changed = produce(start, (draft) => {
      setResourceClaim(draft, "sessions", { kind: "cache", name: "sessions", anchors: [{ file: "redis.conf", lines: [1, 4] }] }, "agent:claude");
    });
    expect(changed.resources?.[0]).toMatchObject({ author: "agent:claude", anchors: [{ file: "redis.conf", lines: [1, 4] }], description: { text: "Who is signed in.", author: "person:maya" } });
    const cleared = produce(changed, (draft) => {
      setResourceDescription(draft, "sessions", " ", "person:maya");
    });
    expect(cleared.resources?.[0]?.description).toBeUndefined();
  });

  it("can end connections, and go with the connections to them, leaving their notes where they were", () => {
    const start = produce(base(), (draft) => {
      addResource(draft, { kind: "table", name: "orders", anchors: [], at: [100, 200], author: "agent:claude" });
      addConnection(draft, { kind: "writes", from: "create-order", to: "orders", anchors: [], author: "person:maya" });
      addNote(draft, { text: "Kept in memory.", at: [0, 120], attach: "orders", author: "agent:claude" });
    });
    expect(start.links).toEqual([{ id: "create-order-writes-orders", from: "create-order", to: "orders", kind: "writes", author: "person:maya" }]);
    const removed = produce(start, (draft) => {
      expect(removeItems(draft, ["orders"])).toEqual(["create-order-writes-orders", "orders"]);
    });
    expect(removed.resources).toBeUndefined();
    expect(removed.notes).toEqual([{ id: "note", text: "Kept in memory.", at: [100, 320], author: "agent:claude" }]);
  });
});

describe("connections", () => {
  const queue = { kind: "queue", from: "create-order", to: "worker", step: STEP, channel: "orders", anchors: [{ key: "src/queues/queues.ts#/call:Queue" }] } as const;

  it("are links with a kind and an author, saying what they're given: columns off a relationship are the save's to refuse", () => {
    const next = produce(base(), (draft) => {
      addConnection(draft, { ...queue, author: "agent:claude" });
      addConnection(draft, { kind: "writes", from: "create-order", to: "worker", fromColumn: "a", cardinality: "one-to-one", anchors: [], author: "agent:claude" });
    });
    expect(next.links).toEqual([
      { id: "create-order-queue-worker", from: "create-order", to: "worker", author: "agent:claude", kind: "queue", step: STEP, channel: "orders", anchors: [{ key: "src/queues/queues.ts#/call:Queue" }] },
      { id: "create-order-writes-worker", from: "create-order", to: "worker", author: "agent:claude", kind: "writes", fromColumn: "a", cardinality: "one-to-one" },
    ]);
  });

  it("lose a relationship's columns and cardinality with its kind", () => {
    const related = produce(base(), (draft) => {
      addConnection(draft, { kind: "relates", from: "create-order", to: "worker", fromColumn: "a", toColumn: "b", cardinality: "one-to-one", anchors: [], author: "person:maya" });
      setConnectionKind(draft, "create-order-relates-worker", "other", "person:maya");
    });
    expect(related.links[0]).toEqual({ id: "create-order-relates-worker", from: "create-order", to: "worker", author: "person:maya", kind: "other" });
  });

  it("are rewritten only when what they say changes, which makes the writer the one who drew it", () => {
    const start = produce(base(), (draft) => {
      addConnection(draft, { ...queue, id: "enqueue", author: "person:maya" });
    });
    const link = start.links[0];
    if (!link) {
      throw new Error("the link was drawn");
    }
    expect(saysConnection(link, { ...claimOf(link), kind: "queue" })).toBe(true);
    const same = produce(start, (draft) => {
      setConnectionClaim(draft, "enqueue", { ...queue }, "agent:claude");
    });
    expect(same).toBe(start);
    const changed = produce(start, (draft) => {
      setConnectionClaim(draft, "enqueue", { ...queue, channel: "orders-v2", anchors: [] }, "agent:claude");
    });
    expect(changed.links[0]).toEqual({ id: "enqueue", from: "create-order", to: "worker", author: "agent:claude", kind: "queue", step: STEP, channel: "orders-v2" });
  });

  it("let a person change a connection's kind, keeping its ends, and label and describe it", () => {
    const start = produce(base(), (draft) => {
      addConnection(draft, { kind: "other", from: "create-order", to: "worker", anchors: [], author: "agent:claude" });
    });
    const next = produce(start, (draft) => {
      setConnectionKind(draft, "create-order-other-worker", "queue", "person:maya");
      setLinkLabel(draft, "create-order-other-worker", "order jobs", "person:maya");
      setLinkDescription(draft, "create-order-other-worker", "Emitted after saving.", "person:maya");
    });
    expect(next.links[0]).toEqual({
      id: "create-order-other-worker",
      from: "create-order",
      to: "worker",
      kind: "queue",
      author: "person:maya",
      label: { text: "order jobs", author: "person:maya", signature: "" },
      description: { text: "Emitted after saving.", author: "person:maya", signature: "" },
    });
  });

  it("keep an other connection's label, which says what it is", () => {
    const labeled = produce(base(), (draft) => {
      addConnection(draft, { kind: "other", from: "create-order", to: "worker", anchors: [], author: "person:maya" });
      setLinkLabel(draft, "create-order-other-worker", "shares a cache", "person:maya");
      setLinkLabel(draft, "create-order-other-worker", " ", "person:maya");
    });
    expect(labeled.links[0]?.label?.text).toBe("shares a cache");
    const queued = produce(labeled, (draft) => {
      setConnectionKind(draft, "create-order-other-worker", "queue", "person:maya");
      setLinkLabel(draft, "create-order-other-worker", "", "person:maya");
    });
    expect(queued.links[0]?.label).toBeUndefined();
  });
});

describe("notes", () => {
  it("are rewritten trimmed, however long: how long a note may be is the board's save's to check", () => {
    const start = produce(base(), (draft) => {
      addNote(draft, { text: "Read the worker next.", at: [0, 0], author: "agent:claude" });
    });
    const long = "x".repeat(401);
    const next = produce(start, (draft) => {
      setNoteText(draft, "note", `  ${long}  `, "person:maya");
    });
    expect(next.notes?.[0]).toMatchObject({ text: long, author: "person:maya" });
  });

  it("are rewritten by whoever edits them, and placed free or at an offset from what they're attached to", () => {
    const start = produce(base(), (draft) => {
      addNote(draft, { text: "Queued orders are placed later.", at: [0, 120.4], attach: "create-order", author: "agent:claude" });
    });
    const next = produce(start, (draft) => {
      setNoteText(draft, "note", "Queued orders are placed by the worker.", "person:maya");
      setNoteText(draft, "note", " ", "person:maya");
    });
    expect(next.notes?.[0]).toEqual({ id: "note", text: "Queued orders are placed by the worker.", at: [0, 120], attach: "create-order", author: "person:maya" });
    const free = produce(next, (draft) => {
      placeNote(draft, "note", [300, 40], null);
    });
    expect(free.notes?.[0]).toMatchObject({ at: [300, 40] });
    expect(free.notes?.[0]?.attach).toBeUndefined();
  });

  it("move with what they're attached to, and an attached one dragged keeps its attachment", () => {
    const start = produce(base(), (draft) => {
      addNote(draft, { text: "On the card.", at: [0, 120], attach: "create-order", author: "agent:claude" });
      addNote(draft, { id: "free", text: "Free.", at: [50, 50], author: "agent:claude" });
      draft.frames = [{ id: "server", title: "server", members: ["create-order", "free"], look: "box", author: "agent:claude" }];
    });
    const moved = produce(start, (draft) => {
      moveItems(draft, { "create-order": [100, 100], note: [100, 400] });
    });
    expect(moved.notes?.find((note) => note.id === "note")).toMatchObject({ at: [0, 300], attach: "create-order" });
    const framed = produce(start, (draft) => {
      moveFrame(draft, "server", 10, 20);
    });
    expect(framed.notes?.map((note) => note.at)).toEqual([
      [0, 120],
      [60, 70],
    ]);
  });
});
