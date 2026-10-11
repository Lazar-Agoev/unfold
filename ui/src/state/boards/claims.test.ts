import { produce } from "immer";
import { describe, expect, it } from "vitest";
import type { Card, Page } from "../../ipc/generated";
import { page } from "../../test/workspace";
import { addConnection, claimOf, saysConnection, setConnectionClaim, setConnectionKind, setLinkDescription } from "./connections";
import { addLink, moveItems, removeItems } from "./document";
import { moveFrame } from "./frames";
import { NOTE_MAX, addNote, noteLength, noteText, placeNote, setNoteText } from "./notes";
import { addResource, saysClaim, setResourceClaim, setResourceDescription } from "./resources";

const cards: Card[] = [
  { id: "create-order", ref: { block: "src/routes/orders.ts#function:createOrder" }, at: [0, 0] },
  { id: "worker", ref: { entrance: "entrance:worker" }, at: [600, 0] },
];
const base = (): Page => page("page-1", cards);
const STEP = "src/routes/orders.ts#function:createOrder/if[0]/variable:job/call[0]";

describe("resource cards", () => {
  it("are added with what they say, a table's type and columns only on a table", () => {
    const next = produce(base(), (draft) => {
      addResource(draft, { kind: "table", name: "orders", type: "src/types.ts#interface:Order", columns: [{ name: "id", key: "primary" }], anchors: [{ key: "src/db.ts#class:Repo" }], at: [10.4, 20.6], author: "agent:claude" });
      addResource(draft, { kind: "queue", name: "orders", type: "src/types.ts#interface:Order", columns: [{ name: "x" }], anchors: [], at: [0, 0], author: "agent:claude" });
    });
    expect(next.resources).toEqual([
      { id: "orders", kind: "table", name: "orders", type: "src/types.ts#interface:Order", columns: [{ name: "id", key: "primary" }], anchors: [{ key: "src/db.ts#class:Repo" }], at: [10, 21], author: "agent:claude" },
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

  it("can end drawn links, and go with the links to them, leaving their notes where they were", () => {
    const start = produce(base(), (draft) => {
      addResource(draft, { kind: "table", name: "orders", anchors: [], at: [100, 200], author: "agent:claude" });
      addLink(draft, "create-order", "orders");
      addNote(draft, { text: "Kept in memory.", at: [0, 120], attach: "orders", author: "agent:claude" });
    });
    expect(start.links).toEqual([{ id: "create-order-to-orders", from: "create-order", to: "orders" }]);
    const removed = produce(start, (draft) => {
      expect(removeItems(draft, ["orders"])).toEqual(["create-order-to-orders", "orders"]);
    });
    expect(removed.resources).toBeUndefined();
    expect(removed.notes).toEqual([{ id: "note", text: "Kept in memory.", at: [100, 320], author: "agent:claude" }]);
  });
});

describe("connections", () => {
  const queue = { kind: "queue", from: "create-order", to: "worker", step: STEP, channel: "orders", anchors: [{ key: "src/queues.ts#variable:ordersQueue" }] } as const;

  it("are links with a kind and an author; columns and cardinality only on a relationship", () => {
    const next = produce(base(), (draft) => {
      addConnection(draft, { ...queue, author: "agent:claude" });
      addConnection(draft, { kind: "writes", from: "create-order", to: "worker", fromColumn: "a", cardinality: "one-to-one", anchors: [], author: "agent:claude" });
    });
    expect(next.links).toEqual([
      { id: "create-order-queue-worker", from: "create-order", to: "worker", author: "agent:claude", kind: "queue", step: STEP, channel: "orders", anchors: [{ key: "src/queues.ts#variable:ordersQueue" }] },
      { id: "create-order-writes-worker", from: "create-order", to: "worker", author: "agent:claude", kind: "writes" },
    ]);
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

  it("let a person give a drawn link a kind, keeping its ends and label, and describe it", () => {
    const start = produce(base(), (draft) => {
      addLink(draft, "create-order", "worker");
    });
    const next = produce(start, (draft) => {
      setConnectionKind(draft, "create-order-to-worker", "event", "person:maya");
      setLinkDescription(draft, "create-order-to-worker", "Emitted after saving.", "person:maya");
    });
    expect(next.links[0]).toEqual({
      id: "create-order-to-worker",
      from: "create-order",
      to: "worker",
      kind: "event",
      author: "person:maya",
      description: { text: "Emitted after saving.", author: "person:maya", signature: "" },
    });
  });
});

describe("notes", () => {
  it("are plain text of 1 to 400 characters, counted as the file counts them", () => {
    expect(noteText("  Read the worker next.  ")).toBe("Read the worker next.");
    expect(noteText("   ")).toBeNull();
    expect(noteText("x".repeat(NOTE_MAX + 1))).toBeNull();
    expect(noteLength("😀".repeat(NOTE_MAX))).toBe(NOTE_MAX);
    expect(noteText("😀".repeat(NOTE_MAX))).not.toBeNull();
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
