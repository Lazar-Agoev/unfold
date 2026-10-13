import { describe, expect, it, vi } from "vitest";
import type { AgentEditRequest, AgentItemResult, AgentPage, AgentShellEvent } from "../../../shared/agent-api";
import { pageRevision } from "../../agents/revision";
import type { Page, ReadBoardResult } from "../engine-types";
import { addCards, arrangePage, removeItems, setView } from "./build";
import { harness, PROJECT } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const BOARD = { project: PROJECT, id: ".unfold/boards/order-placement.json" };
const BOARD_INFO = { title: "Order placement", revision: "r1" } as const;
const CREATE_ORDER = "src/routes/orders.ts#function:createOrder";
const PAGE: AgentPage = { id: "page-1", title: "Placing an order", defaultView: "diagram", cards: [], groups: [], labels: [], guards: [], links: [] };
const SAVED: AgentPage = { ...PAGE, cards: [{ id: "create-order", ref: { block: CREATE_ORDER }, at: [0, 0] }] };

/** A window that holds the board, reads `PAGE`, and saves an edit as `items`. */
function window(items: readonly AgentItemResult[], warnings: readonly string[] = []) {
  return (request: AgentShellEvent): unknown => {
    switch (request.type) {
      case "agent_read":
        return { ok: true, page: PAGE, board: { ...BOARD_INFO, unsaved: false } };
      case "agent_edit":
        return { ok: true, page: SAVED, board: BOARD_INFO, items, warnings };
      case "agent_open_board":
        return { ok: true };
      default:
        return undefined;
    }
  };
}

function inView(test: ReturnType<typeof harness>): void {
  test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
}

function edits(test: ReturnType<typeof harness>): AgentEditRequest[] {
  return test.requests.filter((request): request is AgentShellEvent & AgentEditRequest => request.type === "agent_edit");
}

describe("add_cards", () => {
  it("sends one edit to the page in view, and names what it placed and where following stopped", async () => {
    const test = harness({
      window: window(
        [
          { kind: "card", id: "create-order", status: "created", key: CREATE_ORDER, name: "createOrder", frame: "server", stops: [{ reason: "external", call: "ordersQueue.add", line: 28, note: "bullmq" }] },
          { kind: "card", id: "charge", status: "kept", key: "src/services/payment.ts#function:charge", name: "charge" },
        ],
        ["follow.types follows a type's fields, and charge isn't a type declaration or a class, so no types were added for it."],
      ),
    });
    inView(test);
    const { text } = await test.call(addCards, {
      cards: [
        { key: CREATE_ORDER, expanded: true, frame: "server", follow: { callees: 2 } },
        { key: "src/services/payment.ts#function:charge", view: "diagram", follow: { types: 1 } },
      ],
    });
    expect(edits(test)).toMatchObject([
      {
        board: BOARD,
        page: "page-1",
        agent: { author: "agent:claude", name: "Claude" },
        title: "Add cards",
        ops: [{ op: "add_cards", cards: [{ key: CREATE_ORDER, expanded: true, frame: "server", follow: { callees: 2 } }, { key: "src/services/payment.ts#function:charge", view: "diagram", follow: { types: 1 } }] }],
      },
    ]);
    expect(text.split("\n")).toEqual([
      `Unfold · ts-orders › Order placement › Placing an order · rev ${pageRevision(SAVED)} · as Claude`,
      "Cards (1 created, 1 kept):",
      "- createOrder (card id create-order, created, in frame server); stopped: ordersQueue.add (line 28) is external, bullmq",
      "- charge (card id charge, kept)",
      "follow.types follows a type's fields, and charge isn't a type declaration or a class, so no types were added for it.",
      `Ids: board ${BOARD.id}, page page-1. One undo step in Unfold, saved.`,
      "Next: set_view to expand the cards the page is about (and calls in place), arrange_page for a clean layout, then check.",
    ]);
  });

  it("checks the revision the agent read, and teaches from the window's refusals", async () => {
    const test = harness({ window: window([]) });
    inView(test);
    const stale = await test.call(addCards, { cards: [{ key: CREATE_ORDER }], expectRevision: "0123abcd" });
    expect(stale).toMatchObject({ isError: true, text: `The page changed since you read it (rev 0123abcd, now rev ${pageRevision(PAGE)}). Read it again, then make the change. read_page reads it, with its new rev.` });
    expect(edits(test)).toEqual([]);
    expect((await test.call(addCards, { cards: [{ key: CREATE_ORDER }], expectRevision: pageRevision(PAGE) })).isError).toBe(false);
    expect((await test.call(addCards, { cards: [{ key: CREATE_ORDER }], expectRevision: "rev 18" })).text).toContain("expectRevision is a page's revision");

    const refused = harness({ window: (request) => (request.type === "agent_edit" ? { ok: false, code: "page_changed", message: "The page changed while the edit was saving, and the edit no longer applies. Read the page again." } : undefined) });
    inView(refused);
    expect((await refused.call(addCards, { cards: [{ key: CREATE_ORDER }] })).text).toBe(
      "The page changed while the edit was saving, and the edit no longer applies. Read the page again. read_page reads it.",
    );
  });

  it("edits a board named without a page only when it has one page", async () => {
    const two: ReadBoardResult = {
      project: PROJECT,
      id: BOARD.id,
      name: "order-placement",
      document: { format: "unfold-board", version: 1, title: "Order placement", pages: [PAGE as unknown as Page, { ...(PAGE as unknown as Page), id: "page-2", title: "Async path" }] },
      revision: "r1",
    };
    const test = harness({ window: window([]), engine: { read_board: () => two } });
    const refused = await test.call(addCards, { board: BOARD.id, project: "ts-orders", cards: [{ key: CREATE_ORDER }] });
    expect(refused.text).toBe("Can't tell which page you mean: “Order placement” has 2 pages and none was named. Pass page, one of: Placing an order (page id page-1), Async path (page id page-2).");
    const one = harness({ window: window([]), engine: { read_board: () => ({ ...two, document: { ...two.document, pages: two.document.pages.slice(0, 1) } }) } });
    expect((await one.call(addCards, { board: BOARD.id, project: "ts-orders", cards: [{ key: CREATE_ORDER }] })).isError).toBe(false);
    expect(edits(one)[0]?.page).toBe("page-1");
  });
});

describe("set_view", () => {
  const CALL = `${CREATE_ORDER}/variable:order/call[0]`;
  const PLACE_CALL = "src/services/order-service.ts#class:OrderService/method:place/expression_statement[1]/call[0]";

  it("sends expansion and calls to expand in place, and says how each card shows and what the page opens in", async () => {
    const card = { kind: "card", id: "create-order", status: "updated", key: CREATE_ORDER, name: "createOrder", expanded: true, expandedCalls: [[CALL], [CALL, PLACE_CALL]] } as const;
    const test = harness({ window: window([card, { kind: "card", id: "place", status: "updated", key: "k", name: "OrderService.place", expanded: false, expandedCalls: [] }]) });
    inView(test);
    const { text } = await test.call(setView, {
      items: [
        { id: "create-order", expanded: true, expandCalls: [[CALL, PLACE_CALL]], collapseCalls: [[`${CREATE_ORDER}/expression_statement[0]/call[0]`]] },
        { id: "place", view: "code" },
      ],
      defaultView: "code",
    });
    expect(edits(test)[0]?.ops).toEqual([
      {
        op: "set_view",
        items: [
          { id: "create-order", expanded: true, expandCalls: [[CALL, PLACE_CALL]], collapseCalls: [[`${CREATE_ORDER}/expression_statement[0]/call[0]`]] },
          { id: "place", view: "code" },
        ],
        defaultView: "code",
      },
    ]);
    expect(text.split("\n").slice(1, 6)).toEqual([
      "Items (2 updated):",
      `- createOrder (card id create-order, updated): expanded, calls expanded in place [${CALL}], [${CALL} › ${PLACE_CALL}]`,
      "- OrderService.place (card id place, updated): compact",
      'Code is a page-wide lens (defaultView "code"): view "code" expanded that card in Diagram.',
      "The page opens in Code, every card's source; expansions show in Diagram.",
    ]);
  });

  it("refuses a call path deeper than three levels before it reaches the window, saying what to do instead", async () => {
    const test = harness({ window: window([]) });
    inView(test);
    const deep = await test.call(setView, { items: [{ id: "create-order", expandCalls: [["a", "b", "c", "d"]] }] });
    expect(deep).toMatchObject({ isError: true, text: "A call path holds 1 to 3 keys: expand at most three levels, and follow a deeper call to its own card (add_cards)." });
    expect(edits(test)).toEqual([]);
  });

  it("takes item links of the page edited, and refuses links to another page", async () => {
    const test = harness({ window: window([{ kind: "card", id: "create-order", status: "updated", key: CREATE_ORDER, name: "createOrder" }]) });
    inView(test);
    const { text } = await test.call(setView, { items: [{ id: `unfold://ts-orders/${BOARD.id}?page=page-1&item=create-order`, view: "flow" }], defaultView: "diagram" });
    expect(edits(test)[0]?.ops).toEqual([{ op: "set_view", items: [{ id: "create-order", view: "flow" }], defaultView: "diagram" }]);
    expect(text.split("\n").slice(1, 4)).toEqual(["Items (1 updated):", "- createOrder (card id create-order, updated)", "The page opens in Diagram."]);
    const other = await test.call(setView, { page: "page-1", items: [{ id: `unfold://ts-orders/${BOARD.id}?page=page-2&item=x`, view: "flow" }] });
    expect(other.text).toBe(`Can't use the link: it names an item of another page (page-2 of ${BOARD.id}). Edit one page per call: pass ids of items on the page you're editing.`);
  });
});

describe("arrange_page", () => {
  it("sends the frame, direction, and moves, and says where cards went", async () => {
    const test = harness({ window: window([{ kind: "card", id: "create-order", status: "updated", key: CREATE_ORDER, name: "createOrder", at: [80, 120] }]) });
    inView(test);
    const { text } = await test.call(arrangePage, { frame: "server", direction: "down", moves: [{ id: "a", rightOf: "b" }, { id: "c", at: [1, 2] }] });
    expect(edits(test)[0]?.ops).toEqual([{ op: "arrange", frame: "server", direction: "down", moves: [{ id: "a", rightOf: "b" }, { id: "c", at: [1, 2] }] }]);
    expect(text).toContain("- createOrder (card id create-order, updated) at [80, 120]");
    const twice = await test.call(arrangePage, { moves: [{ id: "a", rightOf: "b", below: "c" }] });
    expect(twice).toMatchObject({ isError: true, text: "Each move takes exactly one of at, rightOf, or below." });
  });
});

describe("remove_items", () => {
  it("takes a proposed step's link (Copy link on a proposal), and says the proposal went, not code", async () => {
    const after = `${CREATE_ORDER}/variable:order`;
    const test = harness({ window: window([{ kind: "proposal", id: "email-a-confirmation", status: "removed", card: "create-order", after, title: "Email a confirmation" }]) });
    inView(test);
    const { text } = await test.call(removeItems, { ids: [`unfold://ts-orders/${BOARD.id}?page=page-1&item=email-a-confirmation`] });
    expect(edits(test)[0]?.ops).toEqual([{ op: "remove_items", ids: ["email-a-confirmation"] }]);
    expect(text.split("\n").slice(1, 3)).toEqual(["Items (1 removed):", `- proposed step “Email a confirmation” (proposal id email-a-confirmation; removed) in card create-order, after the step ${after}`]);
  });

  it("is destructive, passes keepMembers and replacePeopleText, and passes on a refusal to remove a person's words", async () => {
    expect(removeItems.annotations).toEqual({ readOnlyHint: false, destructiveHint: true, idempotentHint: false });
    expect(addCards.annotations).toEqual({ readOnlyHint: false, destructiveHint: false, idempotentHint: true });
    const test = harness({
      window: (request) =>
        request.type === "agent_edit"
          ? { ok: false, code: "people_text", message: "Can't change the description of createOrder: Lazar wrote it. People's words are safe; pass replacePeopleText only when the person asked for that change." }
          : undefined,
    });
    inView(test);
    const { text, isError } = await test.call(removeItems, { ids: ["create-order", "server"], keepMembers: false });
    expect(isError).toBe(true);
    expect(text).toBe("Can't change the description of createOrder: Lazar wrote it. People's words are safe; pass replacePeopleText only when the person asked for that change.");
    expect(edits(test)[0]?.ops).toEqual([{ op: "remove_items", ids: ["create-order", "server"], keepMembers: false }]);
  });
});
