import { describe, expect, it, vi } from "vitest";
import type { AgentPage, AgentRepairRequest, AgentShellEvent } from "../../../shared/agent-api";
import { pageRevision } from "../../agents/revision";
import type { BoardHealthView, HealthItem } from "../engine-types";
import { finishWork, repair } from "./finish";
import { harness, PROJECT } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const BOARD = { project: PROJECT, id: ".unfold/boards/orders.json" };
const BOARD_INFO = { title: "Orders", revision: "r1" } as const;
const PLACE = "src/services/order-service.ts#class:OrderService/method:place";
const PAGE: AgentPage = {
  id: "page-1",
  title: "Placing an order",
  defaultView: "flow",
  cards: [
    { id: "place", ref: { block: PLACE }, at: [0, 0], description: { text: "Places it.", author: "person:lazar", signature: "s" } },
    { id: "charge", ref: { block: "src/pay.ts#function:charge" }, at: [400, 0], description: { text: "Charges.", author: "agent:claude", signature: "s" } },
  ],
  groups: [{ id: "pay", card: "charge", title: "Pay", members: ["src/pay.ts#function:charge/return[0]"], collapsed: false, author: "agent:claude" }],
  labels: [{ key: "src/pay.ts#function:charge/if[0]", text: "Declined?", author: "person:lazar", signature: "s" }],
  guards: [],
  links: [{ id: "save", from: "charge", to: "orders", kind: "writes", anchors: [{ key: PLACE }], author: "agent:claude", description: { text: "Saves it.", author: "agent:claude", signature: "s" } }],
  resources: [{ id: "orders", kind: "table", name: "orders", anchors: [{ key: "a" }, { key: "b" }], at: [0, 200], author: "agent:claude" }],
};

function health(...items: HealthItem[]): BoardHealthView {
  return { health: { items, missing: 0, outdated: 0, brokenGroups: 0, review: 0, brokenLinks: 0 }, outlines: [], label: null };
}

const OUTDATED_PLACE: HealthItem = { page: "page-1", kind: "outdated", target: { card: "place" }, message: "Changed.", repairs: ["resign", "removeDescription"] };
const OUTDATED_CHARGE: HealthItem = { page: "page-1", kind: "outdated", target: { card: "charge" }, message: "Changed.", repairs: ["resign", "removeDescription"] };
const REVIEW_CHARGE: HealthItem = { page: "page-1", kind: "review", target: { card: "charge" }, message: "Its kind changed.", repairs: ["accept", "reconnect", "removeCard"] };
const REVIEW_SAVE: HealthItem = { page: "page-1", kind: "review", target: { link: "save" }, key: PLACE, anchor: 0, message: "Its kind changed.", repairs: ["accept", "reconnect", "removeLink"] };
const REVIEW_PAY: HealthItem = { page: "page-1", kind: "review", target: { group: "pay" }, key: "src/pay.ts#function:charge/return[0]", message: "Its kind changed.", repairs: ["accept", "ungroup"] };
const MISSING_PLACE: HealthItem = { page: "page-1", kind: "missing", target: { card: "place" }, message: "Gone.", repairs: ["reconnect", "removeCard"] };
const ANCHOR_0: HealthItem = { page: "page-1", kind: "missing", target: { resource: "orders" }, key: "a", anchor: 0, message: "Anchor a is gone.", repairs: ["reconnect", "removeResource"] };
const ANCHOR_1: HealthItem = { page: "page-1", kind: "missing", target: { resource: "orders" }, key: "b", anchor: 1, message: "Anchor b is gone.", repairs: ["reconnect", "removeResource"] };

function app(...items: HealthItem[]) {
  const test = harness({
    engine: { board_health: () => health(...items) },
    window: (request: AgentShellEvent) => {
      switch (request.type) {
        case "agent_read":
          return { ok: true, page: PAGE, board: { ...BOARD_INFO, unsaved: false } };
        case "agent_repair":
          return { ok: true, page: { ...PAGE, title: "Placing an order" }, board: BOARD_INFO };
        default:
          return undefined;
      }
    },
  });
  test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
  return test;
}

function repairs(test: ReturnType<typeof harness>): AgentRepairRequest[] {
  return test.requests.filter((request): request is AgentShellEvent & AgentRepairRequest => request.type === "agent_repair");
}

const AT = { board: BOARD.id, project: "ts-orders", page: "page-1" };

describe("repair", () => {
  it("applies one repair a health item offers, through the window that holds the board", async () => {
    const test = app(MISSING_PLACE);
    const { text } = await test.call(repair, { ...AT, item: "place", repair: "reconnect", to: "src/services/order-service.ts#class:OrderService/method:placeOrder" });
    expect(test.windowCalls).toEqual([[1, "board_health", { project: PROJECT, id: BOARD.id }]]);
    expect(repairs(test)).toMatchObject([
      {
        board: BOARD,
        page: "page-1",
        agent: { author: "agent:claude" },
        repair: { page: "page-1", kind: "reconnect", from: PLACE, to: "src/services/order-service.ts#class:OrderService/method:placeOrder" },
        items: ["place"],
      },
    ]);
    expect(text.split("\n")).toEqual([
      `Unfold · ts-orders › Orders › Placing an order · rev ${pageRevision(PAGE)} · as Claude`,
      "Reconnected the card OrderService.place to src/services/order-service.ts#class:OrderService/method:placeOrder.",
      `Ids: board ${BOARD.id}, page page-1. One undo step in Unfold, saved.`,
      "Next: check again to see what's left.",
    ]);
    const removed = await test.call(repair, { ...AT, item: "place", repair: "remove_card" });
    expect(removed.text).toContain("Removed, with its step groups and links, the card OrderService.place.");
    expect(repairs(test)[1]).toMatchObject({ repair: { page: "page-1", kind: "remove_card", card: "place" }, items: [] });
  });

  it("re-signs and accepts only what agents wrote (D6): a person's words are theirs to sign off", async () => {
    const test = app(OUTDATED_PLACE, OUTDATED_CHARGE, REVIEW_CHARGE);
    const person = await test.call(repair, { ...AT, item: "place", repair: "resign" });
    expect(person).toMatchObject({
      isError: true,
      text: "Can't re-sign the card OrderService.place: Lazar (a person) wrote the description of OrderService.place, so signing off on it is theirs to do. Tell them in the chat what changed in the code, and whether the text still reads right to you.",
    });
    const own = await test.call(repair, { ...AT, item: "charge", repair: "resign" });
    expect(own.isError).toBe(false);
    expect(repairs(test)).toMatchObject([{ repair: { kind: "resign", target: { card: "charge" } }, items: ["charge"] }]);
    // Accepting a card's new code vouches for every text on it, a person's Flow label too.
    const accept = await test.call(repair, { ...AT, item: "charge", repair: "accept" });
    expect(accept.text).toBe(
      "Can't accept the current code for the card charge: Lazar (a person) wrote the Flow label “Declined?”, so signing off on it is theirs to do. Tell them in the chat what changed in the code, and whether the text still reads right to you.",
    );
  });

  it("accepts a key only when agents wrote everything its guard holds: the guard is the page's, shared by every item citing the key", async () => {
    const test = app(REVIEW_SAVE, REVIEW_PAY);
    // The agent's connection cites the card a person described: accepting its anchor accepts the card's code too.
    const shared = await test.call(repair, { ...AT, item: "save", repair: "accept" });
    expect(shared.text).toBe(
      "Can't accept the current code for the writes connection charge → orders: Lazar (a person) wrote the description of OrderService.place, so signing off on it is theirs to do. Tell them in the chat what changed in the code, and whether the text still reads right to you.",
    );
    const own = await test.call(repair, { ...AT, item: "pay", repair: "accept" });
    expect(own.isError).toBe(false);
    expect(repairs(test)).toMatchObject([{ repair: { kind: "accept", key: "src/pay.ts#function:charge/return[0]" }, items: ["pay"] }]);
  });

  it("says which repairs an item offers, and which anchor to pick", async () => {
    const test = app(MISSING_PLACE, ANCHOR_0, ANCHOR_1);
    expect((await test.call(repair, { ...AT, item: "place", repair: "resign" })).text).toBe(
      "Can't resign the card OrderService.place: its problems offer reconnect, remove_card. Pass one of those; check shows each problem.",
    );
    expect((await test.call(repair, { ...AT, item: "nothing", repair: "resign" })).text).toBe(
      "Can't repair “nothing”: nothing on the page “Placing an order” with that id has a health problem. check lists the page's problems, each with its item and repairs.",
    );
    expect((await test.call(repair, { ...AT, item: "orders", repair: "reconnect", to: "c" })).text).toBe(
      "Can't reconnect the table “orders”: 2 of its anchors have problems. Pass anchor, one of: 0 (Anchor a is gone.); 1 (Anchor b is gone.).",
    );
    expect((await test.call(repair, { ...AT, item: "place", repair: "reconnect" })).text).toBe(
      "Can't reconnect the card OrderService.place: to is missing. Pass to: the code's new key (search or get_relations finds it), or for an anchor an evidence file's lines.",
    );
    const anchored = await test.call(repair, { ...AT, item: "orders", repair: "reconnect", anchor: 1, to: { file: "db/schema.sql", lines: [3, 9] } });
    expect(anchored.isError).toBe(false);
    expect(repairs(test)).toMatchObject([{ repair: { page: "page-1", kind: "reconnect_anchor", item: "orders", anchor: 1, to: { file: "db/schema.sql", lines: [3, 9] } }, items: ["orders"] }]);
  });
});

describe("finish_work", () => {
  it("finishes every board the agent changed since it last finished, else the board in view", async () => {
    const test = app(MISSING_PLACE);
    await test.call(repair, { ...AT, item: "place", repair: "remove_card" });
    const read = { read_board: () => ({ document: { title: "Orders" } }) };
    const finishing = harness({ engine: read, window: () => undefined });
    expect((await finishing.call(finishWork, { summary: "Nothing" })).text).toBe(
      "Unfold · as Claude\nNothing to finish: you haven't changed a board since you last finished, and no board is in view.\nNext: tell the person in the chat what you found.",
    );

    const { text } = await test.call(finishWork, { summary: "Fixed the boards after the rename" });
    expect(test.requests.filter((request) => request.type === "agent_finish")).toEqual([{ type: "agent_finish", board: BOARD, agent: { author: "agent:claude", name: "Claude" }, summary: "Fixed the boards after the rename" }]);
    expect(text.split("\n").slice(1)).toEqual([
      `Finished on “${BOARD.id}” (board id ${BOARD.id}): its working marks are cleared, and “Claude · Fixed the boards after the rename” shows in its window.`,
      "Next: tell the person in the chat what you made, and which parts go beyond what Unfold compiled.",
    ]);
    // Finished: the next call falls back to the board in view.
    await test.call(finishWork, { summary: "Again" });
    expect(test.requests.filter((request) => request.type === "agent_finish")).toHaveLength(2);
  });
});
