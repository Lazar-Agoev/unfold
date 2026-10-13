// @vitest-environment jsdom
import type { AgentEditSuccess, AgentFailure, AgentItemResult, AgentOp } from "@unfold/desktop/agent-api";
import { renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../../../ipc/client";
import type { BoardDocument, Card, CardView, Frame, Page, Relations, Resource } from "../../../ipc/generated";
import { engineKey } from "../../../ipc/queries";
import { fixture, installFakeEngine, type FakeEngine } from "../../../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../../../test/workspace";
import { useShownBoardStore } from "../../shownBoard";
import { keyDisplayName } from "../document";
import * as store from "../store";
import { applyAgentEdit } from "./edit";
import { resetWorking, useWorkingItems } from "./working";

const P = "p-orders0000";
const ID = ".unfold/local/drafts/b.json";
const REF = { project: P, id: ID };
const TARGET = { board: REF, page: "page-1" };
const CREATE = "src/routes/orders.ts#function:createOrder";
const PROCESS = "src/workers/orders.ts#function:processOrder";
const PLACE = "src/services/order-service.ts#class:OrderService/method:place";
const SLACK = "src/lib/slack.ts#variable:slack/method:post";
const CLAUDE = { author: "agent:claude", name: "Claude" } as const;

const cards: Card[] = [
  { id: "create-order", ref: { block: CREATE }, at: [0, 0] },
  { id: "process-order", ref: { block: PROCESS }, at: [0, 600] },
  { id: "place", ref: { block: PLACE }, at: [800, 0] },
  { id: "slack", ref: { block: SLACK }, at: [800, 600] },
];
const frame = (id: string, members: string[], extra: Partial<Frame> = {}): Frame => ({ id, title: id, members, look: "box", author: "agent:claude", ...extra });

let engine: FakeEngine;

/** "frame server updated": an item of a result, in words. */
function said(item: AgentItemResult, status = true): string {
  return [item.kind, "id" in item ? item.id : item.step, ...(status ? [item.status] : [])].join(" ");
}
let saves: number;

/** createOrder's golden card (it calls OrderService.place and slack.post), or a copy named for another key. */
function model(key: string): CardView {
  const card = fixture("card", "card_create_order");
  if (key === CREATE || card.type !== "block") {
    return card;
  }
  return { ...card, header: { ...card.header, key, name: keyDisplayName(key), qualifiedName: keyDisplayName(key) } };
}

function noCalls(key: string): Relations {
  return { key, callers: [], callersTotal: 0, callersNext: null, outgoing: { onBoard: [], openable: [], externalOrUnresolved: [] }, targetNames: {}, reachedFrom: [] };
}

function open(overrides: Partial<Page> = {}, onPage: Card[] = cards): void {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", onPage, overrides)])));
}

function current(): Page {
  const found = store.getOpenBoard(REF)?.document.pages[0];
  if (!found) {
    throw new Error("the page isn't open");
  }
  return found;
}

async function edit(ops: AgentOp[], title = "Frame cards"): Promise<AgentEditSuccess> {
  const result = await applyAgentEdit({ requestId: 1, board: REF, page: "page-1", agent: CLAUDE, title, ops });
  if (!result.ok) {
    throw new Error(`refused: ${result.code}: ${result.message}`);
  }
  return result;
}

/** A refused edit: nothing of it applies or saves. */
async function refusal(ops: AgentOp[]): Promise<AgentFailure> {
  const before = store.getOpenBoard(REF)?.document;
  const saved = saves;
  const result = await applyAgentEdit({ requestId: 1, board: REF, page: "page-1", agent: CLAUDE, title: "Frame cards", ops });
  if (result.ok) {
    throw new Error("the edit applied");
  }
  expect(store.getOpenBoard(REF)?.document).toBe(before);
  expect(saves).toBe(saved);
  return result;
}

beforeEach(() => {
  store.resetBoards();
  resetWorking();
  queryClient.clear();
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  useShownBoardStore.setState({ board: null });
  saves = 0;
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => model(key),
      relations: ({ key }) => noCalls(key),
      callers: ({ key }) => ({ project: P, key, total: 0, callers: [] }),
      save_board: (args) => {
        saves += 1;
        return boardState(P, ID, boardDocument(structuredClone(args.board.pages) as BoardDocument["pages"], { title: args.board.title }), { revision: "r2" });
      },
    },
  });
});

afterEach(() => {
  engine.uninstall();
  store.resetBoards();
});

describe("frame_cards", () => {
  it("creates frames as one undo step, signed by the agent, naming each frame and what it holds", async () => {
    open();
    const result = await edit([
      {
        op: "frame_cards",
        frames: [
          { title: "server", members: ["create-order"], icon: "api", description: "Takes orders over HTTP." },
          { id: "worker", title: "worker", members: ["process-order"], icon: "worker", collapsed: true },
          { title: "Order system", members: ["server", "worker"], look: "zone", direction: "down" },
        ],
      },
    ]);
    expect(result.items).toEqual([
      { kind: "frame", id: "server", status: "created", name: "server", look: "box", icon: "api", parent: "order-system", members: [{ id: "create-order", kind: "card", name: "createOrder" }] },
      { kind: "frame", id: "worker", status: "created", name: "worker", look: "box", icon: "worker", collapsed: true, parent: "order-system", members: [{ id: "process-order", kind: "card", name: "processOrder" }] },
      {
        kind: "frame",
        id: "order-system",
        status: "created",
        name: "Order system",
        look: "zone",
        direction: "down",
        members: [
          { id: "server", kind: "frame", name: "server" },
          { id: "worker", kind: "frame", name: "worker" },
        ],
      },
    ]);
    expect(current().frames?.[0]).toEqual({
      id: "server",
      title: "server",
      members: ["create-order"],
      look: "box",
      author: "agent:claude",
      icon: "api",
      description: { text: "Takes orders over HTTP.", author: "agent:claude", signature: "" },
    });
    expect(store.getOpenBoard(REF)?.history.past.map((entry) => entry.name)).toEqual(["Claude · Frame cards"]);
    const { result: working } = renderHook(() => useWorkingItems(TARGET));
    expect([...working.current].sort()).toEqual(["order-system", "server", "worker"]);
  });

  it("notes shared code in no frame that code in two frames calls", async () => {
    open();
    const result = await edit([
      {
        op: "frame_cards",
        frames: [
          { title: "server", members: ["create-order"] },
          { title: "worker", members: ["process-order"] },
        ],
      },
    ]);
    expect(result.warnings).toEqual([
      "OrderService.place is in no frame, and server and worker both call it. Consider a frame of its own (icon library).",
      "slack.post is in no frame, and server and worker both call it. Consider a frame of its own (icon library).",
    ]);
  });

  it("updates a frame by id, its members moving out to the frame that holds it, and keeps one asked as it is", async () => {
    open({ frames: [frame("zone", ["server"], { look: "zone" }), frame("server", ["create-order", "place"])] });
    const result = await edit([{ op: "frame_cards", frames: [{ id: "server", title: "Order API", members: ["create-order"], icon: "api" }] }]);
    expect(result.items).toMatchObject([{ id: "server", status: "updated", name: "Order API", icon: "api", parent: "zone" }]);
    expect(current().frames).toEqual([frame("zone", ["server", "place"], { look: "zone" }), { ...frame("server", ["create-order"]), title: "Order API", icon: "api" }]);
    const saved = saves;
    const kept = await edit([{ op: "frame_cards", frames: [{ id: "server", title: "Order API", members: ["create-order"] }] }]);
    expect(kept.items).toMatchObject([{ id: "server", status: "kept" }]);
    expect(saves).toBe(saved);
  });

  it("nests a frame in a parent, taking its members from there; a frame it leaves empty goes", async () => {
    open({ frames: [frame("server", ["create-order", "place"]), frame("old", ["worker"]), frame("worker", ["slack"])] });
    const result = await edit([
      {
        op: "frame_cards",
        frames: [
          { title: "Request checks", members: ["create-order"], parent: "server", collapsed: true },
          { id: "worker", title: "worker", members: ["slack"], parent: "server" },
        ],
      },
    ]);
    expect(result.items.map((item) => said(item))).toEqual(["frame request-checks created", "frame worker updated", "frame old removed"]);
    expect(current().frames).toEqual([
      frame("server", ["request-checks", "place", "worker"]),
      frame("worker", ["slack"]),
      { ...frame("request-checks", ["create-order"]), title: "Request checks", collapsed: true },
    ]);
  });

  it("refuses what breaks the frames' rules, saying why and what to do", async () => {
    open({ frames: [frame("a", ["b"]), frame("b", ["c"]), frame("c", ["create-order"]), frame("other", ["place"])], groups: [{ id: "checks", card: "create-order", title: "Checks", members: [], collapsed: false }] });
    const deep = await refusal([{ op: "frame_cards", frames: [{ title: "Too deep", members: ["slack"], parent: "c" }] }]);
    expect(deep).toMatchObject({ code: "invalid_op" });
    expect(deep.message).toContain("“Too deep” would be 4 frames deep, and frames nest at most 3 deep");
    const taken = await refusal([{ op: "frame_cards", frames: [{ title: "Mine", members: ["place"] }] }]);
    expect(taken.message).toContain("OrderService.place (id place) is already in the frame “other” (id other)");
    expect((await refusal([{ op: "frame_cards", frames: [{ title: "X", members: ["nope"] }] }])).message).toContain("There is no item with id “nope”");
    expect((await refusal([{ op: "frame_cards", frames: [{ title: "X", members: ["checks"] }] }])).message).toContain("is a step group");
    expect((await refusal([{ op: "frame_cards", frames: [{ title: "X", members: ["slack"], parent: "nope" }] }])).message).toContain("There is no frame with id “nope”");
    expect((await refusal([{ op: "frame_cards", frames: [{ id: "c", title: "c", members: ["a"] }] }])).message).toContain("frames can't hold each other in a cycle");
    expect((await refusal([{ op: "frame_cards", frames: [{ title: " ", members: ["slack"] }] }])).message).toContain("needs a title");
    expect((await refusal([{ op: "frame_cards", frames: [{ title: "X", members: [] }] }])).message).toContain("needs members");
    expect((await refusal([{ op: "frame_cards", frames: [{ id: "slack", title: "X", members: ["slack"] }] }])).message).toContain("is the id of a card");
  });

  it("won't change a person's title or description unless asked", async () => {
    open({ frames: [frame("server", ["create-order"], { author: "person:maya", description: { text: "Takes orders.", author: "person:maya", signature: "s" } })] });
    expect((await refusal([{ op: "frame_cards", frames: [{ id: "server", title: "API", members: ["create-order"] }] }])).code).toBe("people_text");
    expect((await refusal([{ op: "describe", items: [{ id: "server", text: "Different." }] }])).code).toBe("people_text");
    // Members are the agent's to adjust; the title stays the person's.
    const result = await edit([{ op: "frame_cards", frames: [{ id: "server", title: "server", members: ["create-order", "place"] }] }]);
    expect(result.items).toMatchObject([{ status: "updated", members: [{ id: "create-order" }, { id: "place" }] }]);
    expect(current().frames?.[0]?.author).toBe("person:maya");
    await edit([{ op: "frame_cards", frames: [{ id: "server", title: "Order API", members: ["create-order", "place"] }], replacePeopleText: true }]);
    expect(current().frames?.[0]).toMatchObject({ title: "Order API", author: "agent:claude" });
  });

  it("won't empty a person's frame by taking a frame out of it unless asked, nor what would go with it", async () => {
    const out = { op: "frame_cards", frames: [{ id: "server", title: "server", members: ["create-order"], parent: null }] } as const;
    open({ frames: [frame("mine", ["server"], { author: "person:maya", look: "zone" }), frame("server", ["create-order"])] });
    const refused = await refusal([out]);
    expect(refused).toMatchObject({ code: "people_text" });
    expect(refused.message).toContain("Can't change the frame “mine”: Maya wrote it.");
    // A frame of the agent's that it leaves empty goes, unless a person's line goes with it.
    open({ frames: [frame("zone", ["server"], { look: "zone" }), frame("server", ["create-order"])], links: [{ id: "to-zone", from: "slack", to: "zone" }] });
    expect((await refusal([out])).message).toContain("the link a person drew");
    const moved = await edit([{ ...out, replacePeopleText: true }]);
    expect(moved.items.map((item) => said(item))).toEqual(["frame server updated", "frame zone removed"]);
    expect(current()).toMatchObject({ frames: [frame("server", ["create-order"])], links: [] });
  });
});

describe("frames in the other operations", () => {
  it("set_view collapses and expands a frame, and refuses what a frame doesn't take", async () => {
    open({ frames: [frame("server", ["create-order"])] });
    const result = await edit([{ op: "set_view", items: [{ id: "server", collapsed: true }] }]);
    expect(result.items).toMatchObject([{ kind: "frame", id: "server", status: "updated", collapsed: true }]);
    expect(current().frames?.[0]?.collapsed).toBe(true);
    await edit([{ op: "set_view", items: [{ id: "server", collapsed: false }] }]);
    expect(current().frames?.[0]).not.toHaveProperty("collapsed");
    expect((await refusal([{ op: "set_view", items: [{ id: "server", view: "flow" }] }])).message).toContain("is a frame");
  });

  it("describe writes a frame's description", async () => {
    open({ frames: [frame("server", ["create-order"])] });
    const result = await edit([{ op: "describe", items: [{ id: "server", text: "Takes orders." }] }]);
    expect(result.items).toMatchObject([{ kind: "frame", id: "server", status: "created" }]);
    expect(current().frames?.[0]?.description).toEqual({ text: "Takes orders.", author: "agent:claude", signature: "" });
  });

  it("remove_items removes a frame keeping its members in the frame that held it, or with them when asked", async () => {
    open({
      frames: [frame("zone", ["server", "slack"], { look: "zone" }), frame("server", ["create-order", "place"])],
      links: [{ id: "to-server", kind: "calls", from: "slack", to: "server", author: "agent:claude" }],
    });
    const kept = await edit([{ op: "remove_items", ids: ["server"] }]);
    expect(kept.items.map((item) => said(item))).toEqual(["frame server removed", "link to-server removed"]);
    expect(current().frames).toEqual([frame("zone", ["create-order", "place", "slack"], { look: "zone" })]);
    expect(current().cards).toHaveLength(4);
    const all = await edit([{ op: "remove_items", ids: ["zone"], keepMembers: false }]);
    expect(all.items.map((item) => said(item, false))).toEqual(["frame zone", "card create-order", "card place", "card slack"]);
    expect(current().cards.map((item) => item.id)).toEqual(["process-order"]);
  });

  it("remove_items won't remove a person's frame unless asked, and a card that empties its frame takes the frame along", async () => {
    open({ frames: [frame("mine", ["slack"], { author: "person:maya" }), frame("agents", ["place"])] });
    expect((await refusal([{ op: "remove_items", ids: ["mine"] }])).message).toContain("the frame “mine”");
    expect((await refusal([{ op: "remove_items", ids: ["slack"] }])).code).toBe("people_text");
    const result = await edit([{ op: "remove_items", ids: ["place"] }]);
    expect(result.items.map((item) => said(item, false))).toEqual(["card place", "frame agents"]);
  });

  it("add_cards puts the cards it adds into a frame, beside what the frame holds", async () => {
    open({ frames: [frame("server", ["create-order"])] }, cards.slice(0, 1));
    const result = await edit([{ op: "add_cards", cards: [{ key: PLACE, frame: "server" }] }]);
    expect(result.items).toMatchObject([{ kind: "card", id: "order-service-place", status: "created", frame: "server" }]);
    expect(current().frames?.[0]?.members).toEqual(["create-order", "order-service-place"]);
    expect((await refusal([{ op: "add_cards", cards: [{ key: SLACK, frame: "nope" }] }])).message).toContain("There is no frame with id “nope”");
  });

  it("add_resources puts a resource card in a frame from no frame or the frame around it, never out of another frame", async () => {
    const orders: Resource = { id: "orders", kind: "queue", name: "orders", anchors: [{ key: CREATE }], at: [1600, 0], author: "agent:claude" };
    open({ frames: [frame("mine", ["orders"], { author: "person:maya" }), frame("server", ["create-order"])], resources: [orders] });
    const refused = await refusal([{ op: "add_resources", resources: [{ id: "orders", kind: "queue", name: "orders", frame: "server" }] }]);
    expect(refused).toMatchObject({ code: "invalid_op" });
    expect(refused.message).toContain("Can't put the queue “orders” (id orders) in the frame “server”: it is in the frame “mine” (id mine)");
    open({ frames: [frame("zone", ["server", "orders"], { look: "zone" }), frame("server", ["create-order"])], resources: [orders] });
    const result = await edit([{ op: "add_resources", resources: [{ id: "orders", kind: "queue", name: "orders", frame: "server" }] }]);
    expect(result.items).toMatchObject([{ kind: "resource", id: "orders", status: "updated", frame: "server" }]);
    expect(current().frames).toEqual([frame("zone", ["server"], { look: "zone" }), frame("server", ["create-order", "orders"])]);
  });

  it("add_cards places a card right of a resource card, a frame, or a note it is near", async () => {
    const at = async (near: string): Promise<readonly number[] | undefined> => {
      open(
        {
          frames: [frame("server", ["create-order"])],
          resources: [{ id: "orders", kind: "queue", name: "orders", anchors: [{ key: CREATE }], at: [1600, 600], author: "agent:claude" }],
          notes: [{ id: "n", text: "Read this first.", at: [1600, 1400], author: "agent:claude" }],
        },
        cards.slice(0, 1),
      );
      const result = await edit([{ op: "add_cards", cards: [{ key: PROCESS, near }] }]);
      return result.items[0]?.kind === "card" ? result.items[0].at : undefined;
    };
    const [x, y] = (await at("orders")) ?? [];
    expect(y).toBe(600);
    expect(x).toBeGreaterThan(1600);
    const [nx, ny] = (await at("n")) ?? [];
    expect(ny).toBe(1400);
    expect(nx).toBeGreaterThan(1600);
    // Right of the frame's box, level with its top: not right of everything.
    const [fx, fy] = (await at("server")) ?? [];
    expect(fx).toBeLessThan(1600);
    expect(fy).toBeLessThan(0);
  });

  it("arrange reports a frame's direction it writes, the default one too", async () => {
    open({ frames: [frame("server", ["create-order", "place"])] });
    const result = await edit([{ op: "arrange", frame: "server", direction: "right" }], "Arrange page");
    expect(result.items[0]).toMatchObject({ kind: "frame", id: "server", status: "updated", direction: "right" });
    expect(current().frames?.[0]?.direction).toBe("right");
    const again = await edit([{ op: "arrange", frame: "server", direction: "right" }], "Arrange page");
    expect(again.items.filter((item) => item.kind === "frame")).toEqual([]);
  });

  it("arrange lays out one frame in a direction it keeps, and moves a frame with what it holds", async () => {
    open({ frames: [frame("server", ["create-order", "place"]), frame("worker", ["process-order"])] });
    const result = await edit([{ op: "arrange", frame: "server", direction: "down" }], "Arrange page");
    expect(result.items[0]).toMatchObject({ kind: "frame", id: "server", status: "updated", direction: "down" });
    const [create, place] = ["create-order", "place"].map((id) => current().cards.find((item) => item.id === id)?.at ?? [0, 0]);
    expect(place?.[0]).toBe(create?.[0]);
    expect(place?.[1]).toBeGreaterThan(create?.[1] ?? 0);
    const worker = current().cards.find((item) => item.id === "process-order")?.at;
    await edit([{ op: "arrange", moves: [{ id: "worker", at: [5000, 5000] }] }], "Arrange page");
    const moved = current().cards.find((item) => item.id === "process-order")?.at;
    // The worker frame's box corner went to (5000, 5000): its card sits inside its padding.
    expect(moved).toEqual([5000 + 16, 5000 + 44]);
    expect(moved).not.toEqual(worker);
    expect((await refusal([{ op: "arrange", frame: "nope" }])).message).toContain("There is no frame with id “nope”");
  });
});
