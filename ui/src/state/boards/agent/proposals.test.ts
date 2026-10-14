// @vitest-environment jsdom
import type { AgentEditSuccess, AgentFailure, AgentOp } from "@unfold/desktop/agent-api";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../../../ipc/client";
import type { BoardDocument, Card, Combined, Page, Proposal, SaveBoardArgs } from "../../../ipc/generated";
import { engineKey } from "../../../ipc/queries";
import { fixture, installFakeEngine, type FakeEngine } from "../../../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../../../test/workspace";
import { useShownBoardStore } from "../../shownBoard";
import * as store from "../store";
import { applyAgentEdit } from "./edit";
import { resetWorking } from "./working";

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
// Keys as the golden card writes them; nothing here takes them apart.
const CREATE = "src/routes/orders.ts#createOrder";
/** `orderService.place(…)`: a step a proposal can go after. */
const PLACE = `${CREATE}/call:orderService.place`;
/** `slack.post(…)`, the step after it. */
const SLACK = `${CREATE}/call:slack.post`;
/** The `if`'s branch: a proposal can go at the start of its body, not after it. */
const THEN = `${CREATE}/if[0]/then`;
const CLAUDE = { author: "agent:claude", name: "Claude" } as const;
const MAYA = "person:maya";

const createCard: Card = { id: "create-order", ref: { block: CREATE }, at: [0, 0] };

let engine: FakeEngine;
let saved: SaveBoardArgs[];

function open(overrides: Partial<Page> = {}, cards: Card[] = [createCard]): void {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", cards, overrides)])));
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
      card: ({ key }) => {
        if (key !== CREATE) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "No such key." };
        }
        return fixture("card", "card_create_order");
      },
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
  it("adds a proposed step after a block, signed as the agent, as one undo step, saved at once", async () => {
    open();
    const result = await edit([{ op: "propose", proposals: [{ after: PLACE, title: " Email a confirmation ", reason: "The queued path sends one; this one doesn't." }] }]);
    expect(result.items).toEqual([{ kind: "proposal", id: "proposal", status: "created", after: PLACE, title: "Email a confirmation", reason: "The queued path sends one; this one doesn't." }]);
    expect(onPage().proposals).toEqual([{ id: "proposal", after: PLACE, title: "Email a confirmation", reason: "The queued path sends one; this one doesn't.", author: "agent:claude" }]);
    expect(saved).toHaveLength(1);
    expect(store.getOpenBoard(REF)?.history.past.map((entry) => entry.name)).toEqual(["Claude · Propose steps"]);
    // The same call again finds it and changes nothing.
    const again = await edit([{ op: "propose", proposals: [{ after: PLACE, title: "Email a confirmation", reason: "The queued path sends one; this one doesn't." }] }]);
    expect(again.items).toMatchObject([{ id: "proposal", status: "kept" }]);
    expect(saved).toHaveLength(1);
  });

  it("adds one at the start of a part's body, where the drawn flow allows it, and refuses where it doesn't", async () => {
    open();
    const created = await edit([{ op: "propose", proposals: [{ id: "notify", start: THEN, title: "Notify the customer" }] }]);
    expect(created.items).toEqual([{ kind: "proposal", id: "notify", status: "created", start: THEN, title: "Notify the customer" }]);
    expect((await refusal([{ op: "propose", proposals: [{ after: THEN, title: "X" }] }])).message).toContain("can't go after “req.header(\"prefer\") === \"respond-async\"”");
    expect((await refusal([{ op: "propose", proposals: [{ start: PLACE, title: "X" }] }])).message).toContain("can't go at the start of “orderService.place”");
  });

  it("refuses a block no card on the page draws, and words or ids that don't fit", async () => {
    open();
    expect(await refusal([{ op: "propose", proposals: [{ after: "key-elsewhere", title: "X" }] }])).toMatchObject({ code: "code_unavailable" });
    expect((await refusal([{ op: "propose", proposals: [{ title: "X" }] }])).message).toContain("A new proposed step needs after");
    expect((await refusal([{ op: "propose", proposals: [{ after: PLACE }] }])).message).toContain("A new proposed step needs after");
    expect((await refusal([{ op: "propose", proposals: [{ after: PLACE, title: "x".repeat(121) }] }])).message).toContain("1 to 120 characters");
    expect((await refusal([{ op: "propose", proposals: [{ after: PLACE, title: "  " }] }])).message).toContain("1 to 120 characters");
    expect((await refusal([{ op: "propose", proposals: [{ id: "create-order", after: PLACE, title: "X" }] }])).message).toContain("can't name a new proposed step");
  });

  it("keeps a person's words unless asked, and makes new words the agent's; moving it keeps its author", async () => {
    const theirs: Proposal = { id: "email", after: PLACE, title: "Email them", reason: "Customers ask.", author: MAYA };
    open({ proposals: [theirs] });
    expect(await refusal([{ op: "propose", proposals: [{ id: "email", title: "Email a confirmation" }] }])).toMatchObject({ code: "people_text" });
    expect((await refusal([{ op: "propose", proposals: [{ id: "email", reason: null }] }])).message).toContain("the reason of the proposed step “Email them”");
    const moved = await edit([{ op: "propose", proposals: [{ id: "email", title: "Email them", after: SLACK }] }]);
    expect(moved.items).toMatchObject([{ id: "email", status: "updated", after: SLACK }]);
    expect(onPage().proposals?.[0]).toMatchObject({ after: SLACK, title: "Email them", author: MAYA });
    await edit([{ op: "propose", proposals: [{ id: "email", title: "Email a confirmation", reason: null }], replacePeopleText: true }]);
    expect(onPage().proposals?.[0]).toEqual({ id: "email", after: SLACK, title: "Email a confirmation", author: "agent:claude" });
  });

  it("repairs a placement whose anchor went missing, explicitly", async () => {
    open({ proposals: [{ id: "lost", after: "key-gone", title: "Retry", author: MAYA }] });
    // Its words alone can change without placing it again.
    expect((await edit([{ op: "propose", proposals: [{ id: "lost", title: "Retry" }] }])).items).toMatchObject([{ status: "kept" }]);
    const repaired = await edit([{ op: "propose", proposals: [{ id: "lost", start: THEN }] }]);
    expect(repaired.items).toMatchObject([{ id: "lost", status: "updated", start: THEN }]);
    expect(onPage().proposals?.[0]).toEqual({ id: "lost", start: THEN, title: "Retry", author: MAYA });
  });
});

describe("remove_items of proposed steps", () => {
  const ours: Proposal = { id: "email", after: PLACE, title: "Email a confirmation", author: "agent:claude" };
  const theirs: Proposal = { id: "notify", start: THEN, title: "Notify", author: MAYA };

  it("removes a proposal by id, a person's only when asked; removing a card keeps them, as they apply page-wide", async () => {
    open({ proposals: [ours, theirs] });
    const removed = await edit([{ op: "remove_items", ids: ["email"] }]);
    expect(removed.items).toEqual([{ kind: "proposal", id: "email", status: "removed", after: PLACE, title: "Email a confirmation" }]);
    expect((await refusal([{ op: "remove_items", ids: ["notify"] }])).message).toContain("the proposed step “Notify”: Maya wrote it");
    const card = await edit([{ op: "remove_items", ids: ["create-order"] }]);
    expect(card.items.map((item) => `${item.kind} ${item.id}`)).toEqual(["card create-order"]);
    expect(onPage().proposals).toEqual([theirs]);
  });
});

describe("combined steps", () => {
  const checks: Combined = { id: "checks", title: "Checks", members: [`${CREATE}/if[0]`], collapsed: false, author: "agent:claude", signature: "" };

  it("set_view collapses and opens a combined step, and refuses a view for it", async () => {
    open({ combined: [checks] });
    const result = await edit([{ op: "set_view", items: [{ id: "checks", collapsed: true }] }]);
    expect(result.items).toEqual([{ kind: "combined", id: "checks", status: "updated", name: "Checks", members: [`${CREATE}/if[0]`], collapsed: true }]);
    expect(onPage().combined[0]?.collapsed).toBe(true);
    expect((await refusal([{ op: "set_view", items: [{ id: "checks", view: "expanded" }] }])).message).toContain("is a combined step: it takes collapsed");
  });

  it("describe writes a combined step's description, a person's only when asked", async () => {
    open({ combined: [checks, { ...checks, id: "mine", members: [PLACE], author: MAYA, description: { text: "Mine.", author: MAYA, signature: "" } }] });
    const result = await edit([{ op: "describe", items: [{ id: "checks", text: "Stops early." }] }]);
    expect(result.items).toMatchObject([{ kind: "combined", id: "checks", status: "created" }]);
    expect(onPage().combined[0]?.description).toEqual({ text: "Stops early.", author: "agent:claude", signature: "" });
    expect((await refusal([{ op: "describe", items: [{ id: "mine", text: "Theirs." }] }])).message).toContain("the description of the combined step “Checks”");
  });

  it("remove_items removes a combined step; a person's only when asked", async () => {
    open({ combined: [checks, { ...checks, id: "mine", title: "Mine", members: [PLACE], author: MAYA }] });
    const result = await edit([{ op: "remove_items", ids: ["checks"] }]);
    expect(result.items).toEqual([{ kind: "combined", id: "checks", status: "removed", name: "Checks", members: [`${CREATE}/if[0]`], collapsed: false }]);
    expect(onPage().combined.map((item) => item.id)).toEqual(["mine"]);
    expect((await refusal([{ op: "remove_items", ids: ["mine"] }])).message).toContain("the combined step “Mine”: Maya wrote it");
  });
});
