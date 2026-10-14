import { describe, expect, it, vi } from "vitest";
import cardCreateOrder from "../../../../../testdata/workspace/commands/card_create_order.json";
import codeTextCreateOrder from "../../../../../testdata/workspace/commands/code_text_create_order.json";
import flowTextCreateOrder from "../../../../../testdata/workspace/commands/flow_text_create_order.json";
import type { AgentEditRequest, AgentFailure, AgentItemResult, AgentPage, AgentShellEvent } from "../../../shared/agent-api";
import { pageRevision } from "../../agents/revision";
import type { CardView, CodeText, DrawnFlow, FlowText } from "../engine-types";
import { shapeCard } from "./card";
import { harness, PROJECT, recorded } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

// Keys as the recorded answers write them; the tool only looks them up.
const CREATE_ORDER = "src/routes/orders.ts#createOrder";
const IF = `${CREATE_ORDER}/if[0]`;
const PLACE_CALL = `${CREATE_ORDER}/call:orderService.place`;
const SLACK_CALL = `${CREATE_ORDER}/call:slack.post`;
const PLACE = "src/services/order-service.ts#OrderService.place";
const VALIDATE_CALL = `${PLACE}/call:validateOrder`;
const ORDER = "src/types.ts#Order";
const BOARD = { project: PROJECT, id: ".unfold/boards/order-placement.json" };
const BOARD_INFO = { title: "Order placement", revision: "r1" } as const;
const ENTRANCE_CARD = { id: "post-orders", ref: { entrance: "entrance:server/place-an-order" }, at: [-400, 0] };
const TYPE_CARD = { id: "order", ref: { block: ORDER }, at: [0, 600] };
const PAGE: AgentPage = {
  id: "page-1",
  title: "Placing an order",
  defaultView: "expanded",
  cards: [{ id: "create-order", ref: { block: CREATE_ORDER }, at: [0, 0] }, ENTRANCE_CARD, TYPE_CARD],
  names: [],
  combined: [],
  guards: [],
  links: [],
};
/** The page as the window saved it, shaped. */
const SHAPED: AgentPage = {
  ...PAGE,
  cards: [{ id: "create-order", ref: { block: CREATE_ORDER }, at: [0, 0], open: [[PLACE_CALL]] }, ENTRANCE_CARD, TYPE_CARD],
  names: [{ key: IF, text: "Asked to queue it?", author: "agent:claude", signature: "s" }],
  combined: [{ id: "tell-the-team", title: "Tell the team", members: [SLACK_CALL], collapsed: true, author: "agent:claude", signature: "s" }],
};
const ITEMS: readonly AgentItemResult[] = [
  { kind: "card", id: "create-order", status: "updated", key: CREATE_ORDER, name: "Create an order", view: "expanded", open: [[PLACE_CALL]] },
  { kind: "combined", id: "tell-the-team", status: "created", name: "Tell the team", members: [SLACK_CALL], collapsed: true },
];

/** createOrder's card as `card` draws it with `orderService.place` opened in place: the call box holds place's flow. */
function openedCard(): CardView {
  const card = recorded<CardView>(cardCreateOrder);
  if (card.type !== "definition") {
    throw new Error("the recorded card isn't a definition's");
  }
  const inset: DrawnFlow = { ...card.flow, header: { ...card.flow.header, key: PLACE, label: "OrderService.place" }, steps: { steps: [] }, routes: [] };
  const steps = card.flow.steps.steps.map((step) => (step.kind === "call" && step.key === PLACE_CALL ? { ...step, inset } : step));
  return { type: "definition", flow: { ...card.flow, steps: { ...card.flow.steps, steps } } };
}

/** The type Order's card: it shows fields, not a flow. */
function typeCard(): CardView {
  const card = recorded<CardView>(cardCreateOrder);
  if (card.type !== "definition") {
    throw new Error("the recorded card isn't a definition's");
  }
  return { type: "definition", flow: { ...card.flow, header: { ...card.flow.header, key: ORDER, kind: "type", label: "Order" }, steps: { steps: [] }, routes: [] } };
}

const PLACE_LABELS: CodeText = { text: "", labels: [{ label: "call:validateOrder", key: VALIDATE_CALL }] };

/** A window that holds the board with `page` and answers an edit with `answer` (the shaped page, by default). */
function app(answer: AgentFailure | null = null, page: AgentPage = PAGE) {
  const test = harness({
    engine: {
      code_text: ({ key }) => (key === CREATE_ORDER ? recorded<CodeText>(codeTextCreateOrder) : key === PLACE ? PLACE_LABELS : undefined),
      card: ({ key, open }) => (key === ORDER ? typeCard() : Array.isArray(open) && open.length > 0 ? openedCard() : recorded<CardView>(cardCreateOrder)),
      flow_text: () => recorded<FlowText>(flowTextCreateOrder),
    },
    window: (request: AgentShellEvent) => {
      switch (request.type) {
        case "agent_read":
          return { ok: true, page, board: { ...BOARD_INFO, unsaved: false } };
        case "agent_edit":
          return answer ?? { ok: true, page: SHAPED, board: BOARD_INFO, items: ITEMS, warnings: [] };
        default:
          return undefined;
      }
    },
  });
  test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
  return test;
}

function edits(test: ReturnType<typeof harness>): AgentEditRequest[] {
  return test.requests.filter((request): request is AgentShellEvent & AgentEditRequest => request.type === "agent_edit");
}

describe("shape_card", () => {
  it("maps short labels to keys by lookup, applies the shaping as one undo step, and answers with the shaped flow", async () => {
    const test = app();
    const { text, isError } = await test.call(shapeCard, {
      card: "create-order",
      title: "Create an order",
      description: "Places an order now, or queues it when the client asks.",
      names: { "if[0]": "Asked to queue it?", [SLACK_CALL]: "Tell the team" },
      combine: [{ steps: ["call:slack.post"], title: "Tell the team" }],
      open: [["call:orderService.place", "call:validateOrder"]],
    });
    expect(isError).toBe(false);
    const sent = edits(test);
    expect(sent).toHaveLength(1);
    expect(sent[0]?.title).toBe("Shape card");
    expect(sent[0]?.ops).toEqual([
      {
        op: "shape_card",
        card: "create-order",
        title: "Create an order",
        description: "Places an order now, or queues it when the client asks.",
        names: [
          { key: IF, text: "Asked to queue it?" },
          { key: SLACK_CALL, text: "Tell the team" },
        ],
        combine: [{ members: [SLACK_CALL], title: "Tell the team" }],
        // The second block is looked up in what the first opens: place's own labels.
        open: [[PLACE_CALL, VALIDATE_CALL]],
      },
    ]);
    // The result's flow is the shaped page's: its overlay and the card's open paths.
    expect(test.engineCalls.filter(([command]) => command === "flow_text")).toEqual([
      [
        "flow_text",
        {
          project: PROJECT,
          key: CREATE_ORDER,
          overlay: { names: [{ key: IF, text: "Asked to queue it?" }], combined: [{ id: "tell-the-team", title: "Tell the team", members: [SLACK_CALL], collapsed: true }], proposed: [] },
          open: [[PLACE_CALL]],
        },
      ],
    ]);
    const lines = text.split("\n");
    expect(lines.slice(0, 5)).toEqual([
      `Unfold · ts-orders › Order placement › Placing an order · rev ${pageRevision(SHAPED)} · as Claude`,
      "Shaped (1 created, 1 updated):",
      `- Create an order (card id create-order, updated): expanded, calls open in place [${PLACE_CALL}]`,
      `- “Tell the team” (combined step id tell-the-team; collapsed; created): ${SLACK_CALL}`,
      "The card as the person sees it now:",
    ]);
    expect(text).toContain(flowTextCreateOrder.text.trimEnd());
    expect(lines.slice(-2)).toEqual([
      `Ids: board ${BOARD.id}, page page-1. One undo step in Unfold, saved.`,
      "Next: shape_card the next card the page is about, then frame_cards, add_resources, and connect to compose the page; arrange_page and check.",
    ]);
  });

  it("passes keys as written, and a person's words refusal as it is", async () => {
    const test = app({
      ok: false,
      code: "people_text",
      message: "Can't change the name “Queued?”: Maya wrote it. People's words are safe; pass replacePeopleText only when the person asked for that change.",
    });
    const refused = await test.call(shapeCard, { card: "create-order", names: { [IF]: "Asked to queue it?" } });
    expect(refused).toMatchObject({ isError: true, text: "Can't change the name “Queued?”: Maya wrote it. People's words are safe; pass replacePeopleText only when the person asked for that change." });
    expect(edits(test)[0]?.ops).toEqual([{ op: "shape_card", card: "create-order", names: [{ key: IF, text: "Asked to queue it?" }] }]);

    await test.call(shapeCard, { card: "create-order", names: { [IF]: null }, replacePeopleText: true });
    expect(edits(test)[1]?.ops).toEqual([{ op: "shape_card", card: "create-order", names: [{ key: IF, text: null }], replacePeopleText: true }]);
  });

  it("refuses with what failed and the blocks the agent can choose from instead", async () => {
    const test = app({
      ok: false,
      code: "board_problem",
      message: "Can't save the page: `src/routes/orders.ts#createOrder/if[0]/then/return[0]` is not in the body of `src/routes/orders.ts#createOrder/if[0]`: combined steps are consecutive steps of one body.",
    });
    const refused = await test.call(shapeCard, { card: "create-order", combine: [{ steps: ["if[0]", "if[0]/then/return[0]"], title: "Stop early" }] });
    expect(refused.isError).toBe(true);
    expect(refused.text.split("\n")).toEqual([
      "Can't save the page: `src/routes/orders.ts#createOrder/if[0]/then/return[0]` is not in the body of `src/routes/orders.ts#createOrder/if[0]`: combined steps are consecutive steps of one body.",
      "The blocks of createOrder you can choose from, by short label:",
      `- names: any block, such as ${codeTextCreateOrder.labels.map((label) => label.label).join(", ")}; title names the card itself`,
      "- combine: consecutive steps of one body: in its own body: if[0], call:orderService.place, call:slack.post, call:res.status().json; in if[0]/then: if[0]/then/call:ordersQueue.add, if[0]/then/call:res.status().json, if[0]/then/return[0]",
      "- open: call:orderService.place, call:slack.post (a path's next blocks are calls or callbacks of what the one before opens)",
    ]);
  });

  it("passes a refusal that isn't about a key or a rule as it is, without the blocks", async () => {
    const message = "The name for “src/routes/orders.ts#createOrder/if[0]” is blank. Write it in a few words, or leave it out.";
    const refused = await app({ ok: false, code: "invalid_op", message }).call(shapeCard, { card: "create-order", names: { "if[0]": "Asked to queue it?" } });
    expect(refused).toMatchObject({ isError: true, text: message });
  });

  it("refuses before editing when there is nothing to do, no such card, or more than a description on a card with no flow", async () => {
    const test = app();
    expect((await test.call(shapeCard, { card: "create-order" })).text).toBe("Can't shape the card: nothing to change was given. Pass title, description, names, combine, or open.");
    expect((await test.call(shapeCard, { card: "nope", title: "X" })).text).toBe(
      "Can't shape the card: there is no card with id “nope” on the page “Placing an order”. read_page lists the page's cards with their ids.",
    );
    expect((await test.call(shapeCard, { card: "post-orders", title: "X" })).text).toBe(
      "Can't shape the card: post-orders is an app's or entrance's card, which draws no flow of its own. On it, shape_card writes only description; shape the cards of its code for the rest.",
    );
    expect((await test.call(shapeCard, { card: "order", description: "An order.", names: { x: "Y" } })).text).toBe(
      "Can't shape the card: order is a type card, which shows fields, not a flow. On it, shape_card writes only description; shape a function's, a class's, or a module's card for the rest.",
    );
    expect((await test.call(shapeCard, { card: "create-order", open: [["a", "b", "c", "d"]] })).isError).toBe(true);
    expect(edits(test)).toEqual([]);
  });

  it("writes the description of an app's, entrance's, or type card", async () => {
    const test = app();
    for (const card of ["post-orders", "order"]) {
      expect((await test.call(shapeCard, { card, description: "Takes an order from the client." })).isError).toBe(false);
    }
    expect(edits(test).map((edit) => edit.ops)).toEqual([
      [{ op: "shape_card", card: "post-orders", description: "Takes an order from the client." }],
      [{ op: "shape_card", card: "order", description: "Takes an order from the client." }],
    ]);
  });

  it("refuses a short label of a call the card opens in place, naming the block's full key", async () => {
    const advice = `Can't shape the card: “call:validateOrder” is a short label of ${PLACE}, a call the card opens in place, not of the card's own code. Short labels name the card's own blocks; name a block of an opened call by its full key: ${VALIDATE_CALL}.`;
    // Opened in this call.
    const test = app();
    expect((await test.call(shapeCard, { card: "create-order", names: { "call:validateOrder": "Check the order" }, open: [["call:orderService.place"]] })).text).toBe(advice);
    // Opened already, combined.
    expect((await test.call(shapeCard, { card: "create-order", combine: [{ steps: ["call:validateOrder"], title: "Check the order" }] })).text).not.toBe(advice);
    const opened = app(null, SHAPED);
    expect((await opened.call(shapeCard, { card: "create-order", combine: [{ steps: ["call:validateOrder"], title: "Check the order" }] })).text).toBe(advice);
    expect(edits(opened)).toEqual([]);
    // By its full key, it goes to the window.
    await opened.call(shapeCard, { card: "create-order", names: { [VALIDATE_CALL]: "Check the order" } });
    expect(edits(opened)[0]?.ops).toEqual([{ op: "shape_card", card: "create-order", names: [{ key: VALIDATE_CALL, text: "Check the order" }] }]);
  });

  it("passes an open path's later blocks as written when what the one before opens can't be read", async () => {
    const test = app();
    await test.call(shapeCard, { card: "create-order", open: [["call:slack.post", "call:send"]] });
    // slack.post isn't opened in the drawn card, so `call:send` can't be looked up: the save decides.
    expect(edits(test)[0]?.ops).toEqual([{ op: "shape_card", card: "create-order", open: [[SLACK_CALL, "call:send"]] }]);
  });
});
