// @vitest-environment jsdom
import type { AgentEditSuccess, AgentFailure, AgentOp } from "@unfold/desktop/agent-api";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { queryClient } from "../../../ipc/client";
import type { BoardDocument, Card, Combined, NameEntry, Page, SaveBoardArgs } from "../../../ipc/generated";
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
const IF = `${CREATE}/if[0]`;
const PLACE = `${CREATE}/call:orderService.place`;
const SLACK = `${CREATE}/call:slack.post`;
const CLAUDE = { author: "agent:claude", name: "Claude" } as const;
const MAYA = "person:maya";

const createCard: Card = { id: "create-order", ref: { block: CREATE }, at: [0, 0], view: "expanded" };
const entranceCard: Card = { id: "post-orders", ref: { entrance: "entrance:server/place-an-order" }, at: [-400, 0] };

let engine: FakeEngine;
let saved: SaveBoardArgs[];
/** A message the next save is refused with, as the board's checks refuse one. */
let refuseSave: string | null;

function open(overrides: Partial<Page> = {}, cards: Card[] = [createCard, entranceCard]): void {
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
  const result = await applyAgentEdit({ requestId: 1, board: REF, page: "page-1", agent: CLAUDE, title: "Shape card", ops });
  if (!result.ok) {
    throw new Error(`refused: ${result.code}: ${result.message}`);
  }
  return result;
}

/** A refused edit: nothing of it applies, nothing is saved, and the board keeps its history. */
async function refusal(ops: AgentOp[]): Promise<AgentFailure> {
  const before = store.getOpenBoard(REF)?.document;
  const saves = saved.length;
  const result = await applyAgentEdit({ requestId: 1, board: REF, page: "page-1", agent: CLAUDE, title: "Shape card", ops });
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
  refuseSave = null;
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => (key === CREATE ? fixture("card", "card_create_order") : fixture("card", "card_place_an_order")),
      save_board: (args) => {
        if (refuseSave !== null) {
          throw Object.assign(new Error(refuseSave), { code: "board_problem" });
        }
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

describe("shape_card", () => {
  it("titles, describes, names, combines, and opens calls in one undo step, saved at once and signed as the agent", async () => {
    open();
    const result = await edit([
      {
        op: "shape_card",
        card: "create-order",
        title: " Create an order ",
        description: "Places an order now, or queues it when the client asks.",
        names: [{ key: IF, text: "Asked to queue it?" }],
        combine: [{ members: [SLACK], title: "Tell the team" }],
        open: [[PLACE]],
      },
    ]);
    expect(result.items).toEqual([
      { kind: "card", id: "create-order", status: "updated", key: CREATE, name: "createOrder", view: "expanded", open: [[PLACE]] },
      { kind: "combined", id: "tell-the-team", status: "created", name: "Tell the team", members: [SLACK], collapsed: true },
    ]);
    const shaped = onPage();
    expect(shaped.names).toEqual([
      { key: CREATE, text: "Create an order", author: "agent:claude", signature: "" },
      { key: IF, text: "Asked to queue it?", author: "agent:claude", signature: "" },
    ]);
    expect(shaped.cards[0]).toEqual({ ...createCard, open: [[PLACE]], description: { text: "Places an order now, or queues it when the client asks.", author: "agent:claude", signature: "" } });
    expect(shaped.combined).toEqual([{ id: "tell-the-team", title: "Tell the team", members: [SLACK], collapsed: true, author: "agent:claude", signature: "" }]);
    expect(saved).toHaveLength(1);
    expect(store.getOpenBoard(REF)?.history.past.map((entry) => entry.name)).toEqual(["Claude · Shape card"]);
  });

  it("updates a combined step with the same blocks, takes names off with null, and closes every opened call with []", async () => {
    const ours: Combined = { id: "tell", title: "Tell", members: [SLACK], collapsed: true, author: "agent:claude", signature: "" };
    const name: NameEntry = { key: IF, text: "Queued?", author: "agent:claude", signature: "" };
    open({ combined: [ours], names: [name] }, [{ ...createCard, open: [[PLACE]] }]);
    const result = await edit([{ op: "shape_card", card: "create-order", names: [{ key: IF, text: null }], combine: [{ members: [SLACK], title: "Tell the team", collapsed: false }], open: [] }]);
    expect(result.items).toMatchObject([
      { kind: "card", status: "updated", open: [] },
      { kind: "combined", id: "tell", status: "updated", name: "Tell the team", collapsed: false },
    ]);
    expect(onPage().names).toEqual([]);
    expect(onPage().cards[0]?.open).toBeUndefined();
    // The same call again changes nothing.
    const again = await edit([{ op: "shape_card", card: "create-order", combine: [{ members: [SLACK], title: "Tell the team" }] }]);
    expect(again.items).toMatchObject([{ status: "kept" }, { status: "kept" }]);
  });

  it("keeps a person's name, title, combined step, and description unless the person asked", async () => {
    const theirs: Combined = { id: "tell", title: "Tell", members: [SLACK], collapsed: true, author: MAYA, signature: "" };
    open({ combined: [theirs], names: [{ key: IF, text: "Queued?", author: MAYA, signature: "" }] }, [{ ...createCard, description: { text: "Mine.", author: MAYA, signature: "" } }]);
    expect((await refusal([{ op: "shape_card", card: "create-order", names: [{ key: IF, text: "Asked to queue it?" }] }])).message).toContain("Can't change the name “Queued?”: Maya wrote it");
    expect((await refusal([{ op: "shape_card", card: "create-order", names: [{ key: IF, text: null }] }])).code).toBe("people_text");
    expect((await refusal([{ op: "shape_card", card: "create-order", combine: [{ members: [SLACK], title: "Tell the team" }] }])).message).toContain("the combined step “Tell”");
    expect((await refusal([{ op: "shape_card", card: "create-order", description: "Ours." }])).message).toContain("the description of createOrder");
    // The same words are kept, whoever wrote them; asked, the agent's words replace them.
    await edit([{ op: "shape_card", card: "create-order", names: [{ key: IF, text: "Queued?" }] }]);
    await edit([{ op: "shape_card", card: "create-order", names: [{ key: IF, text: "Asked to queue it?" }], replacePeopleText: true }]);
    expect(onPage().names).toEqual([{ key: IF, text: "Asked to queue it?", author: "agent:claude", signature: "" }]);
  });

  it("refuses a card that isn't on the page, or draws no flow, blank words, and what the board's save refuses, applying nothing", async () => {
    open();
    expect((await refusal([{ op: "shape_card", card: "nope", title: "X" }])).message).toContain("There is no card with id “nope”");
    expect((await refusal([{ op: "shape_card", card: "post-orders", title: "X" }])).message).toContain("is an app's or entrance's card: it draws no flow of its own");
    expect((await refusal([{ op: "shape_card", card: "create-order", title: "  " }])).message).toContain("is blank");
    refuseSave = "`src/routes/orders.ts#createOrder/if[0]/then/return[0]` is not in the body of `src/routes/orders.ts#createOrder/if[0]`: combined steps are consecutive steps of one body";
    expect(await refusal([{ op: "shape_card", card: "create-order", combine: [{ members: [IF, `${IF}/then/return[0]`], title: "Stop early" }] }])).toMatchObject({ code: "board_problem", message: refuseSave });
  });

  it("warns that a compact card shows its opened calls once expanded", async () => {
    open({}, [{ ...createCard, view: "compact" }]);
    const result = await edit([{ op: "shape_card", card: "create-order", open: [[PLACE]] }]);
    expect(result.warnings).toEqual(["createOrder is compact, so the calls it opens in place show once set_view expands it."]);
  });
});
