// @vitest-environment jsdom
import type { AgentEditSuccess, AgentFailure, AgentOp } from "@unfold/desktop/agent-api";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../../../ipc/client";
import type { BoardDocument, Card, CardView, FlowNode, FlowRoute, Key, Page, Proposal, SaveBoardArgs } from "../../../ipc/generated";
import { engineKey } from "../../../ipc/queries";
import { fixture, installFakeEngine, type FakeEngine } from "../../../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../../../test/workspace";
import { useShownBoardStore } from "../../shownBoard";
import { keyDisplayName } from "../document";
import { stepPaths } from "../proposals";
import * as store from "../store";
import { applyAgentEdit } from "./edit";
import { resetWorking } from "./working";

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const CREATE = "src/routes/orders.ts#function:createOrder";
const IF = `${CREATE}/if[0]`;
const ORDER = `${CREATE}/variable:order`;
const CLAUDE = { author: "agent:claude", name: "Claude" } as const;
const MAYA = "person:maya";

const createCard: Card = { id: "create-order", ref: { block: CREATE }, at: [0, 0] };

let engine: FakeEngine;
let saved: SaveBoardArgs[];

/** createOrder's golden card, or a copy named for another key. */
function model(key: string): CardView {
  const card = fixture("card", "card_create_order");
  if (key === CREATE || card.type !== "block") {
    return card;
  }
  return { ...card, header: { ...card.header, key, name: keyDisplayName(key), qualifiedName: keyDisplayName(key) } };
}

function open(overrides: Partial<Page> = {}, cards: Card[] = [createCard]): void {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", cards, { defaultView: "diagram", ...overrides })])));
}

function onPage(): Page {
  const found = store.getOpenBoard(REF)?.document.pages[0];
  if (!found) {
    throw new Error("the page isn't open");
  }
  return found;
}

async function edit(ops: AgentOp[]): Promise<AgentEditSuccess> {
  const result = await applyAgentEdit({ requestId: 1, board: REF, page: "page-1", agent: CLAUDE, title: "Propose steps", ops });
  if (!result.ok) {
    throw new Error(`refused: ${result.code}: ${result.message}`);
  }
  return result;
}

/** A refused edit: nothing of it applies, nothing is saved, and the board keeps its history. */
async function refusal(ops: AgentOp[]): Promise<AgentFailure> {
  const before = store.getOpenBoard(REF)?.document;
  const saves = saved.length;
  const result = await applyAgentEdit({ requestId: 1, board: REF, page: "page-1", agent: CLAUDE, title: "Propose steps", ops });
  if (result.ok) {
    throw new Error("the edit applied");
  }
  expect(store.getOpenBoard(REF)?.document).toBe(before);
  expect(saved.length).toBe(saves);
  return result;
}

beforeEach(() => {
  store.resetBoards();
  resetWorking();
  queryClient.clear();
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  useShownBoardStore.setState({ board: null });
  saved = [];
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => model(key),
      save_board: (args) => {
        saved.push(args);
        return boardState(P, ID, boardDocument(structuredClone(args.board.pages) as BoardDocument["pages"], { title: args.board.title }), { revision: "r2" });
      },
    },
  });
});

afterEach(() => {
  engine.uninstall();
  store.resetBoards();
});

describe("propose", () => {
  it("adds a proposed step after a real step, signed as the agent, as one undo step, saved at once", async () => {
    open();
    const result = await edit([{ op: "propose", card: "create-order", proposals: [{ after: ORDER, title: " Email a confirmation ", reason: "The queued path sends one; this one doesn't." }] }]);
    expect(result.items).toEqual([
      {
        kind: "proposal",
        id: "proposal",
        status: "created",
        card: "create-order",
        after: ORDER,
        title: "Email a confirmation",
        reason: "The queued path sends one; this one doesn't.",
        afterLabel: "OrderService.place",
        afterLine: 32,
      },
    ]);
    expect(onPage().proposals).toEqual([{ id: "proposal", card: "create-order", after: ORDER, title: "Email a confirmation", reason: "The queued path sends one; this one doesn't.", author: "agent:claude" }]);
    expect(saved).toHaveLength(1);
    expect(store.getOpenBoard(REF)?.history.past.map((entry) => entry.name)).toEqual(["Claude · Propose steps"]);
    // The same call again finds it and changes nothing.
    const again = await edit([{ op: "propose", card: "create-order", proposals: [{ after: ORDER, title: "Email a confirmation", reason: "The queued path sends one; this one doesn't." }] }]);
    expect(again.items).toMatchObject([{ id: "proposal", status: "kept" }]);
    expect(saved).toHaveLength(1);
  });

  it("takes a condition's path, and refuses a missing or wrong one, or one on a step with one way on", async () => {
    open();
    const created = await edit([{ op: "propose", card: "create-order", proposals: [{ id: "notify", after: IF, path: "no", title: "Notify the customer" }] }]);
    expect(created.items).toMatchObject([{ id: "notify", path: "no", afterLabel: "req.header?", afterLine: 27 }]);
    expect((await refusal([{ op: "propose", card: "create-order", proposals: [{ after: IF, title: "X" }] }])).message).toContain('has 2 paths: pass path "yes" or "no".');
    expect((await refusal([{ op: "propose", card: "create-order", proposals: [{ after: IF, path: "each", title: "X" }] }])).message).toContain('has no "each" path: pass path "yes" or "no".');
    expect((await refusal([{ op: "propose", card: "create-order", proposals: [{ after: ORDER, path: "yes", title: "X" }] }])).message).toContain("has one way on: leave path out");
  });

  it("refuses Start, End, steps that end their path, keys that aren't steps, and cards that aren't functions", async () => {
    open({}, [createCard, { id: "worker", ref: { entrance: "entrance:worker-orders" }, at: [800, 0] }, { id: "order", ref: { block: "src/types.ts#interface:Order" }, at: [0, 600] }]);
    const refused = async (after: Key, card = "create-order") => (await refusal([{ op: "propose", card, proposals: [{ after, title: "X" }] }])).message;
    expect(await refused(`${IF}/return[0]`)).toContain("is a return: nothing runs after it on its path");
    expect(await refused(CREATE)).toContain("is createOrder itself (its Start)");
    expect(await refused(`${CREATE}/if[9]`)).toContain("isn't a step of createOrder's Flow");
    expect(await refused(ORDER, "worker")).toContain("is an entrance card");
    expect(await refused(ORDER, "order")).toContain("is a type card");
    expect((await refusal([{ op: "propose", card: "create-order", proposals: [{ after: ORDER }] }])).message).toContain("A new proposed step needs after");
    expect((await refusal([{ op: "propose", card: "create-order", proposals: [{ after: ORDER, title: "x".repeat(121) }] }])).message).toContain("1 to 120 characters");
    expect((await refusal([{ op: "propose", card: "create-order", proposals: [{ after: ORDER, title: "  " }] }])).message).toContain("1 to 120 characters");
    expect((await refusal([{ op: "propose", card: "create-order", proposals: [{ id: "create-order", after: ORDER, title: "X" }] }])).message).toContain("can't name a new proposed step");
  });

  it("refuses to place a step while the card's Flow isn't available", async () => {
    open();
    queryClient.setQueryData(engineKey("card", { project: P, key: CREATE }), { ...model(CREATE), flow: null });
    expect(await refusal([{ op: "propose", card: "create-order", proposals: [{ after: ORDER, title: "X" }] }])).toMatchObject({ code: "code_unavailable" });
  });

  it("keeps a person's words unless asked, and makes new words the agent's", async () => {
    const theirs: Proposal = { id: "email", card: "create-order", after: ORDER, title: "Email them", reason: "Customers ask.", author: MAYA };
    open({ proposals: [theirs] });
    expect(await refusal([{ op: "propose", card: "create-order", proposals: [{ id: "email", title: "Email a confirmation" }] }])).toMatchObject({ code: "people_text" });
    expect((await refusal([{ op: "propose", card: "create-order", proposals: [{ id: "email", reason: null }] }])).message).toContain("the reason of the proposed step “Email them”");
    // The same words, and a placement change, are fine: moving it keeps its author.
    const moved = await edit([{ op: "propose", card: "create-order", proposals: [{ id: "email", title: "Email them", after: `${CREATE}/expression_statement[0]` }] }]);
    expect(moved.items).toMatchObject([{ id: "email", status: "updated", after: `${CREATE}/expression_statement[0]` }]);
    expect(onPage().proposals?.[0]).toMatchObject({ title: "Email them", author: MAYA });
    await edit([{ op: "propose", card: "create-order", proposals: [{ id: "email", title: "Email a confirmation", reason: null }], replacePeopleText: true }]);
    expect(onPage().proposals?.[0]).toEqual({ id: "email", card: "create-order", after: `${CREATE}/expression_statement[0]`, title: "Email a confirmation", author: "agent:claude" });
  });

  it("repairs a placement whose anchor went missing, explicitly, and checks a new path against the step it stays after", async () => {
    open({ proposals: [{ id: "lost", card: "create-order", after: `${CREATE}/if[3]`, path: "yes", title: "Retry", author: MAYA }] });
    // Its words alone can change without placing it again.
    expect((await edit([{ op: "propose", card: "create-order", proposals: [{ id: "lost", title: "Retry" }] }])).items).toMatchObject([{ status: "kept" }]);
    expect((await refusal([{ op: "propose", card: "create-order", proposals: [{ id: "lost", path: "no" }] }])).message).toContain("isn't a step of createOrder's Flow");
    const repaired = await edit([{ op: "propose", card: "create-order", proposals: [{ id: "lost", after: ORDER }] }]);
    expect(repaired.items).toMatchObject([{ id: "lost", status: "updated", after: ORDER, afterLine: 32 }]);
    expect(onPage().proposals?.[0]).toEqual({ id: "lost", card: "create-order", after: ORDER, title: "Retry", author: MAYA });
    await edit([{ op: "propose", card: "create-order", proposals: [{ id: "lost", after: IF, path: "yes" }] }]);
    expect((await edit([{ op: "propose", card: "create-order", proposals: [{ id: "lost", path: "no" }] }])).items).toMatchObject([{ after: IF, path: "no" }]);
  });

  it("refuses a proposal of another card", async () => {
    open({ proposals: [{ id: "email", card: "slack", after: ORDER, title: "Email", author: "agent:claude" }] }, [createCard, { id: "slack", ref: { block: "src/lib/slack.ts#variable:slack/method:post" }, at: [900, 0] }]);
    expect((await refusal([{ op: "propose", card: "create-order", proposals: [{ id: "email", title: "Email a confirmation" }] }])).message).toContain("is a proposed step of card slack");
  });
});

describe("stepPaths", () => {
  it("reads a decision's paths from its routes: an else if is no, a do-while's again is each", () => {
    const routes = (kinds: FlowRoute["kind"][]): FlowRoute[] => kinds.map((kind, index) => ({ from: 1, to: index + 2, kind }));
    expect(stepPaths({ routes: routes(["yes", "elseIf"]) }, 1)).toEqual(["yes", "no"]);
    expect(stepPaths({ routes: routes(["again", "done", "each"]) }, 1)).toEqual(["each", "done"]);
    expect(stepPaths({ routes: routes(["sequence", "return"]) }, 1)).toEqual([]);
    expect(stepPaths({ routes: routes(["yes", "no"]) }, 2)).toEqual([]);
  });
});

describe("remove_items of proposed steps", () => {
  const ours: Proposal = { id: "email", card: "create-order", after: ORDER, title: "Email a confirmation", author: "agent:claude" };
  const theirs: Proposal = { id: "notify", card: "create-order", after: IF, path: "yes", title: "Notify", author: MAYA };

  it("removes a proposal by id, a person's only when asked, and a card's with the card", async () => {
    open({ proposals: [ours, theirs] });
    const removed = await edit([{ op: "remove_items", ids: ["email"] }]);
    expect(removed.items).toEqual([{ kind: "proposal", id: "email", status: "removed", card: "create-order", after: ORDER, title: "Email a confirmation" }]);
    expect((await refusal([{ op: "remove_items", ids: ["notify"] }])).message).toContain("the proposed step “Notify”: Maya wrote it");
    // Removing the card takes its proposals, so a person's proposal guards the card too.
    expect(await refusal([{ op: "remove_items", ids: ["create-order"] }])).toMatchObject({ code: "people_text" });
    const all = await edit([{ op: "remove_items", ids: ["create-order"], replacePeopleText: true }]);
    expect(all.items.map((item) => `${item.kind} ${"id" in item ? item.id : ""}`)).toEqual(["card create-order", "proposal notify"]);
    expect(onPage().proposals).toBeUndefined();
  });
});

describe("set_view: calls expanded in place", () => {
  const HANDLER = "src/queues/email-worker.ts#variable:emailWorker/new[0]/arrow_function[0]";
  const RETRY_STEP = `${HANDLER}/expression_statement[0]`;
  const RETRY_CALL = `${RETRY_STEP}/call[0]`;
  const CALLBACK = `${RETRY_STEP}/call[0]/arrow_function[0]`;
  const SEND = "src/jobs/send-email.ts#function:sendEmail";
  const WITH_RETRY = "src/lib/retry.ts#function:withRetry";
  const LOG = "src/lib/logger.ts#variable:logger/method:info";
  const LOG_CALL = `${SEND}/expression_statement[2]/call[0]`;
  const ADD_CALL = `${SEND}/expression_statement[0]/call[0]`;
  const PLACE = "src/services/order-service.ts#class:OrderService/method:place";
  const ORDER_CALL = `${ORDER}/call[0]`;

  /** A block's model with these steps between Start and End. */
  function flowModel(key: Key, steps: readonly Omit<FlowNode, "id" | "lane">[], routes: readonly FlowRoute[] = []): CardView {
    const base = model(key);
    if (base.type !== "block" || !base.flow) {
      throw new Error("createOrder's card has a Flow");
    }
    const nodes: FlowNode[] = [
      { id: 0, key, kind: "start", label: keyDisplayName(key), lane: 0 },
      ...steps.map((step, index) => ({ ...step, id: index + 1, lane: 0 })),
      { id: steps.length + 1, kind: "end", label: "End", lane: 0 },
    ];
    return { ...base, flow: { ...base.flow, key, nodes, routes: [...routes], tree: [], start: 0, end: steps.length + 1, lanes: 1 } };
  }

  /** A call step whose main call goes to `target` (absent: an external call). */
  function call(key: Key, source: Key, name: string, target?: Key, extra: Partial<FlowNode> = {}): Omit<FlowNode, "id" | "lane"> {
    const chip = target !== undefined ? { resolution: "resolved" as const, target } : { resolution: "external" as const, note: "a library" };
    return { key, kind: "call", label: name, startLine: 9, chips: [{ source, seq: 0, kind: "calls", name, line: 9, ...chip }], call: 0, ...extra };
  }

  function models(entries: Readonly<Record<Key, CardView>>): void {
    for (const [key, value] of Object.entries(entries)) {
      queryClient.setQueryData(engineKey("card", { project: P, key }), value);
    }
  }

  const callback = { key: CALLBACK, label: "callback to sendEmail", lines: 1, throws: false };

  function worker(): void {
    open({}, [createCard, { id: "handler", ref: { block: HANDLER }, at: [0, 600], view: "flow" }, { id: "order", ref: { block: "src/types.ts#interface:Order" }, at: [0, 1200] }, { id: "worker", ref: { entrance: "entrance:worker-emails" }, at: [900, 0] }]);
    models({
      [HANDLER]: flowModel(HANDLER, [call(RETRY_STEP, RETRY_CALL, "withRetry", WITH_RETRY, { callbacks: [callback] })]),
      // `() => sendEmail(job.data)`: one call, so it shows sendEmail.
      [CALLBACK]: flowModel(CALLBACK, [call(`${CALLBACK}/expression[0]`, `${CALLBACK}/call[0]`, "sendEmail", SEND)]),
      [SEND]: flowModel(SEND, [call(`${SEND}/expression_statement[0]`, ADD_CALL, "outbox.add"), call(`${SEND}/expression_statement[2]`, LOG_CALL, "logger.info", LOG)]),
      [WITH_RETRY]: flowModel(WITH_RETRY, [call(`${WITH_RETRY}/return[0]`, `${WITH_RETRY}/return[0]/call[0]`, "fn", HANDLER)]),
    });
  }

  it("expands a resolved call, and through a one-call callback its callee, with the levels above, and reports them", async () => {
    worker();
    const result = await edit([{ op: "set_view", items: [{ id: "handler", expandCalls: [[CALLBACK, LOG_CALL]] }] }]);
    expect(result.items).toEqual([
      { kind: "card", id: "handler", status: "updated", key: HANDLER, name: keyDisplayName(HANDLER), expanded: true, expandedCalls: [[CALLBACK], [CALLBACK, LOG_CALL]] },
    ]);
    expect(onPage().cards[1]?.expanded).toEqual([[CALLBACK], [CALLBACK, LOG_CALL]]);
    // A callback that makes more than one call leads to itself: its own steps are the next level.
    models({ [CALLBACK]: flowModel(CALLBACK, [call(`${CALLBACK}/expression[0]`, `${CALLBACK}/call[0]`, "sendEmail", SEND), call(`${CALLBACK}/expression[1]`, `${CALLBACK}/call[1]`, "logger.info", LOG)]) });
    expect((await refusal([{ op: "set_view", items: [{ id: "handler", expandCalls: [[CALLBACK, LOG_CALL]] }] }])).message).toContain(`“${LOG_CALL}” isn't a call step's main call or a callback in`);
    await edit([{ op: "set_view", items: [{ id: "handler", expandCalls: [[CALLBACK, `${CALLBACK}/call[1]`]] }] }]);
    expect(onPage().cards[1]?.expanded).toContainEqual([CALLBACK, `${CALLBACK}/call[1]`]);
  });

  it("refuses external and nested calls, unknown keys, a repeated ancestor, and more than three levels, saying what to pass", async () => {
    worker();
    const refused = async (path: Key[], card = "handler") => (await refusal([{ op: "set_view", items: [{ id: card, expandCalls: [path] }] }])).message;
    expect(await refused([`${IF}/variable:job/call[0]`], "create-order")).toContain("the call ordersQueue.add (line 28) in createOrder is external (bullmq), so it has no code to expand in place");
    expect(await refused([`${CREATE}/expression_statement[1]/call[0]/call[0]`], "create-order")).toContain("is written inside another call");
    expect(await refused([`${CREATE}/if[0]/call[9]`], "create-order")).toContain("isn't a call step's main call or a callback");
    // withRetry calls the handler back: the handler already shows above.
    expect(await refused([RETRY_CALL, `${WITH_RETRY}/return[0]/call[0]`])).toContain("which already shows above it (a repeated ancestor)");
    expect(await refused([CALLBACK, ADD_CALL])).toContain("the call outbox.add (line 9) in sendEmail is external (a library)");
    expect(await refused([RETRY_CALL, RETRY_CALL, RETRY_CALL, RETRY_CALL])).toContain("a call path holds 1 to 3 keys");
    expect(onPage().cards[1]?.expanded).toBeUndefined();
  });

  it("refuses more than 64 calls a card, and calls or expansion on cards that don't take them", async () => {
    worker();
    models({ [PLACE]: flowModel(PLACE, []) });
    const many = Array.from({ length: 64 }, (_, index) => [`${CREATE}/gone[${index}]/call[0]`]);
    open({}, [{ ...createCard, expanded: many }, { id: "order", ref: { block: "src/types.ts#interface:Order" }, at: [0, 1200] }, { id: "worker", ref: { entrance: "entrance:worker-emails" }, at: [900, 0] }]);
    expect((await refusal([{ op: "set_view", items: [{ id: "create-order", expandCalls: [[ORDER_CALL]] }] }])).message).toContain("a card expands at most 64");
    expect((await refusal([{ op: "set_view", items: [{ id: "order", expanded: true }] }])).message).toContain("is a type card: it keeps its fields in every view");
    open({ resources: [{ id: "orders", kind: "table", name: "orders", anchors: [], at: [0, 900], author: "agent:claude" }] });
    expect((await refusal([{ op: "set_view", items: [{ id: "orders", expanded: true }] }])).message).toContain("is a resource card: it keeps its columns in every view");
    open({}, [{ ...createCard, expanded: many }, { id: "order", ref: { block: "src/types.ts#interface:Order" }, at: [0, 1200] }, { id: "worker", ref: { entrance: "entrance:worker-emails" }, at: [900, 0] }]);
    expect((await refusal([{ op: "set_view", items: [{ id: "worker", expandCalls: [[ORDER_CALL]] }] }])).message).toContain("is an entrance card: its targets aren't calls to expand in place");
    // A type card still takes the older view "diagram", and an entrance expands.
    const result = await edit([{ op: "set_view", items: [{ id: "order", view: "diagram" }, { id: "worker", expanded: true }] }]);
    expect(result.items).toMatchObject([{ id: "order", status: "updated" }, { id: "worker", status: "updated", expanded: true, expandedCalls: [] }]);
    expect(result.items[0]).not.toHaveProperty("expanded");
  });

  it("collapses a call with what it holds, takes a legacy view as expansion, and warns when the card is compact", async () => {
    open({}, [{ ...createCard, view: "diagram" }]);
    models({ [PLACE]: flowModel(PLACE, [call(`${PLACE}/expression_statement[1]`, `${PLACE}/expression_statement[1]/call[0]`, "insertOrder", "src/db/orders.ts#function:insertOrder")]) });
    const compact = await edit([{ op: "set_view", items: [{ id: "create-order", expandCalls: [[ORDER_CALL, `${PLACE}/expression_statement[1]/call[0]`]] }] }]);
    expect(compact.warnings).toEqual(["createOrder is compact: the calls it expands in place show once it's expanded (expanded: true)."]);
    expect(compact.items).toMatchObject([{ expanded: false, expandedCalls: [[ORDER_CALL], [ORDER_CALL, `${PLACE}/expression_statement[1]/call[0]`]] }]);
    const coded = await edit([{ op: "set_view", items: [{ id: "create-order", view: "code", collapseCalls: [[ORDER_CALL]] }] }]);
    expect(coded.items).toMatchObject([{ expanded: true, expandedCalls: [] }]);
    expect(onPage().cards[0]).toEqual({ ...createCard, view: "flow" });
  });
});
