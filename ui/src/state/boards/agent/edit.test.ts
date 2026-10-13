// @vitest-environment jsdom
import type { AgentEditRequest, AgentEditSuccess, AgentFailure, AgentOp } from "@unfold/desktop/agent-api";
import { renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { STACK_GAP, type Rect } from "../../../canvas/layout";
import { measureText } from "../../../canvas/measure";
import { estimatedBlockSize } from "../../../canvas/sizing";
import { queryClient } from "../../../ipc/client";
import type { BoardDocument, Card, CardView, Group, LabelEntry, Link, Page, Relations, SaveBoardArgs } from "../../../ipc/generated";
import { engineKey } from "../../../ipc/queries";
import { fixture, installFakeEngine, type FakeEngine } from "../../../test/engine";
import { boardDocument, boardState, page, projectRow, workspaceView } from "../../../test/workspace";
import { getShownBoard, showBoard, useShownBoardStore } from "../../shownBoard";
import { displayedRects } from "../api";
import { keyDisplayName } from "../document";
import { addNote } from "../notes";
import * as store from "../store";
import { applyAgentEdit, finishAgentWork, openBoardForAgent, readAgentPage } from "./edit";
import { resetWorking, useWorkingAgents, useWorkingItems } from "./working";

const toast = vi.hoisted(() => ({ showToast: vi.fn() }));
vi.mock("../../../components/Toast", () => toast);

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const REF = { project: P, id: ID };
const TARGET = { board: REF, page: "page-1" };
const CREATE = "src/routes/orders.ts#function:createOrder";
const PLACE = "src/services/order-service.ts#class:OrderService/method:place";
const SLACK = "src/lib/slack.ts#variable:slack/method:post";
const INSERT = "src/db/orders.ts#function:insertOrder";
const FILE = "src/routes/orders.ts#";
const MISSING = "src/gone.ts#function:gone";
const IF = `${CREATE}/if[0]`;
const CLAUDE = { author: "agent:claude", name: "Claude" } as const;

const createCard: Card = { id: "create-order", ref: { block: CREATE }, at: [0, 0] };

let engine: FakeEngine;
let saved: SaveBoardArgs[];
let refuseSave: { code: string; message: string } | null;
/** Holds the next save until it settles. */
let holdSave: Promise<void> | null;

/** createOrder's golden card, or a copy named for another key. */
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

function open(pageOverrides: Partial<Page> = {}, cards: Card[] = [createCard]): void {
  store.adoptBoardState(boardState(P, ID, boardDocument([page("page-1", cards, pageOverrides)])));
}

function onPage(): Page {
  const found = store.getOpenBoard(REF)?.document.pages[0];
  if (!found) {
    throw new Error("the page isn't open");
  }
  return found;
}

function request(ops: AgentOp[], title = "Add cards"): AgentEditRequest {
  return { requestId: 1, board: REF, page: "page-1", agent: CLAUDE, title, ops };
}

async function edit(ops: AgentOp[]): Promise<AgentEditSuccess> {
  const result = await applyAgentEdit(request(ops));
  if (!result.ok) {
    throw new Error(`refused: ${result.code}: ${result.message}`);
  }
  return result;
}

/** Applies a refused edit: nothing of it applies, and nothing of it is an undo step or a save. */
async function refusal(ops: AgentOp[]): Promise<AgentFailure> {
  const before = store.getOpenBoard(REF)?.document;
  const saves = saved.length;
  const result = await applyAgentEdit(request(ops));
  if (result.ok) {
    throw new Error("the edit applied");
  }
  expect(store.getOpenBoard(REF)?.document).toBe(before);
  expect(saved.length).toBe(saves);
  return result;
}

/** The working marks of the test page, read through their hooks. */
function readWorking(): { agents: string[]; items: string[] } {
  const { result } = renderHook(() => ({ agents: useWorkingAgents(TARGET), items: useWorkingItems(TARGET) }));
  return { agents: [...result.current.agents], items: [...result.current.items] };
}

function overlap(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

beforeEach(() => {
  store.resetBoards();
  resetWorking();
  queryClient.clear();
  queryClient.setQueryData(engineKey("workspace", {}), workspaceView([projectRow(P, "orders")]));
  toast.showToast.mockClear();
  useShownBoardStore.setState({ board: null });
  saved = [];
  refuseSave = null;
  holdSave = null;
  engine = installFakeEngine({
    handlers: {
      card: ({ key }) => {
        if (key === MISSING) {
          // Rejections are plain {code, message} data, as from the real preload.
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { code: "not_found", message: "No such key." };
        }
        return model(key);
      },
      relations: ({ key }) => {
        if (key === CREATE) {
          return fixture("relations", "relations_create_order");
        }
        if (key === PLACE) {
          const relations = noCalls(key);
          return {
            ...relations,
            outgoing: { ...relations.outgoing, openable: [{ source: `${PLACE}/call[0]`, seq: 0, kind: "calls", resolution: "resolved", target: INSERT, text: "insertOrder", line: 12 }] },
          };
        }
        return noCalls(key);
      },
      callers: ({ key }) => (key === CREATE ? { ...fixture("callers", "callers_create_order"), total: 3 } : { project: P, key, total: 0, callers: [] }),
      save_board: async (args) => {
        const held = holdSave;
        holdSave = null;
        await held;
        if (refuseSave) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw refuseSave;
        }
        saved.push(args);
        // The engine signs what it saves: the answer, not the request, is the page as saved.
        const pages = structuredClone(args.board.pages) as BoardDocument["pages"];
        for (const card of pages.flatMap((item) => item.cards)) {
          if (card.description?.signature === "") {
            card.description.signature = "sha256:signed";
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

describe("an agent's edit", () => {
  it("is one undo step named for the agent and the tool, saved at once, answering with the page as saved", async () => {
    open({}, []);
    const result = await edit([{ op: "add_cards", cards: [{ key: CREATE }] }, { op: "describe", items: [{ id: "create-order", text: "Takes an order." }] }]);
    expect(saved).toHaveLength(1);
    expect(result.page.id).toBe("page-1");
    expect((result.page as unknown as Page).cards[0]?.description).toEqual({ text: "Takes an order.", author: "agent:claude", signature: "sha256:signed" });
    expect(result.board).toEqual({ title: "Order placement", revision: "r2" });
    expect(result.items).toEqual([
      { kind: "card", id: "create-order", status: "created", key: CREATE, name: "createOrder", at: [0, 0] },
      { kind: "card", id: "create-order", status: "created", key: CREATE, name: "createOrder" },
    ]);
    const board = store.getOpenBoard(REF);
    expect(board?.history.past.map((entry) => entry.name)).toEqual(["Claude · Add cards"]);
    expect(board && store.isDirty(board)).toBe(false);
    store.undo(REF);
    expect(onPage().cards).toEqual([]);
  });

  it("leaves the board as it was and answers with the engine's code and message when the save is refused", async () => {
    open();
    refuseSave = { code: "board_problem", message: "The key src/x.ts#function:x is not in the graph." };
    expect(await refusal([{ op: "describe", items: [{ id: "create-order", text: "Takes an order." }] }])).toEqual({ ok: false, ...refuseSave });
    expect(store.getOpenBoard(REF)?.history).toMatchObject({ past: [], future: [] });
  });

  it("answers with what it applied over a person's edit made while it saved, as the save left it", async () => {
    open();
    let release: () => void = () => undefined;
    holdSave = new Promise((resolve) => {
      release = resolve;
    });
    const pending = applyAgentEdit(request([{ op: "add_notes", notes: [{ text: "Read createOrder first." }] }], "Add notes"));
    await vi.waitFor(() => {
      expect(engine.callsOf("save_board")).toHaveLength(1);
    });
    // The person's note takes the id the agent's note had.
    store.editPage(TARGET, (draft) => {
      addNote(draft, { text: "Mine.", at: [0, 900], author: "person:maya" });
    });
    release();
    const result = await pending;
    expect(result).toMatchObject({ ok: true, items: [{ kind: "note", id: "note-2", status: "created" }] });
    expect(result.ok && (result.page as unknown as Page).notes?.map((note) => [note.id, note.author])).toEqual([
      ["note", "person:maya"],
      ["note-2", "agent:claude"],
    ]);
    expect(saved.map((args) => args.board.pages[0]?.notes?.map((note) => note.id))).toEqual([["note"], ["note", "note-2"]]);
    expect(readWorking().items).toEqual(["note-2"]);
  });

  it("refuses a board the window doesn't hold, a page it doesn't have, and a person's identity", async () => {
    expect(await applyAgentEdit(request([]))).toMatchObject({ ok: false, code: "board_not_open" });
    open();
    expect(await applyAgentEdit({ ...request([]), page: "page-9" })).toMatchObject({ ok: false, code: "page_not_found" });
    const person = { ...request([]), agent: { author: "person:maya", name: "Maya" } } as unknown as AgentEditRequest;
    expect(await applyAgentEdit(person)).toMatchObject({ ok: false, code: "invalid_op" });
  });

  it("changes nothing, and saves nothing, when everything is already as asked", async () => {
    open({}, [{ ...createCard, description: { text: "Takes an order.", author: "person:maya", signature: "s" } }]);
    const result = await edit([{ op: "describe", items: [{ id: "create-order", text: "Takes an order." }] }]);
    expect(result.items).toMatchObject([{ id: "create-order", status: "kept" }]);
    expect(saved).toEqual([]);
    expect(store.getOpenBoard(REF)?.history.past).toEqual([]);
    expect(readWorking().items).toEqual([]);
  });
});

describe("working marks", () => {
  it("mark what the agent changed until it finishes, then show its summary", async () => {
    open();
    await edit([{ op: "describe", items: [{ id: "create-order", text: "Takes an order." }] }]);
    expect(readWorking()).toEqual({ agents: ["Claude"], items: ["create-order"] });
    finishAgentWork({ board: REF, agent: CLAUDE, summary: "Described createOrder" });
    expect(readWorking()).toEqual({ agents: [], items: [] });
    expect(toast.showToast).toHaveBeenCalledWith({ title: "Claude · Described createOrder" });
  });

  it("aren't saved and aren't undo steps", async () => {
    open();
    await edit([{ op: "describe", items: [{ id: "create-order", text: "Takes an order." }] }]);
    expect(JSON.stringify(saved)).not.toContain("working");
    expect(store.getOpenBoard(REF)?.history.past).toHaveLength(1);
  });
});

describe("add_cards", () => {
  it("keeps a card already on the page, and changes only its view when asked", async () => {
    open();
    const kept = await edit([{ op: "add_cards", cards: [{ key: CREATE }] }]);
    expect(kept.items).toEqual([{ kind: "card", id: "create-order", status: "kept", key: CREATE, name: "createOrder" }]);
    const viewed = await edit([{ op: "add_cards", cards: [{ key: CREATE, view: "diagram" }] }]);
    expect(viewed.items).toMatchObject([{ id: "create-order", status: "updated" }]);
    expect(onPage().cards).toEqual([{ ...createCard, view: "diagram" }]);
  });

  it("places a card beside the card that calls it, level with the calling line, never covering a card", async () => {
    open({}, [createCard, { id: "in-the-way", ref: { block: SLACK }, at: [600, 220] }]);
    const result = await edit([{ op: "add_cards", cards: [{ key: PLACE }] }]);
    const added = onPage().cards.find((card) => "block" in card.ref && card.ref.block === PLACE);
    expect(result.items).toMatchObject([{ status: "created", key: PLACE, name: "OrderService.place" }]);
    const board = store.getOpenBoard(REF);
    const rects = board ? displayedRects(board, "page-1") : new Map<string, Rect>();
    const create = rects.get("create-order");
    const placed = added ? rects.get(added.id) : undefined;
    expect(placed && create ? placed.x : 0).toBeGreaterThan((create?.x ?? 0) + (create?.width ?? 0));
    for (const [id, rect] of rects) {
      if (id !== added?.id && placed) {
        expect(overlap(placed, rect)).toBe(false);
      }
    }
  });

  it("places a card near the item it names, and refuses an item that isn't there", async () => {
    open({}, [createCard, { id: "far", ref: { block: "src/misc.ts#function:misc" }, at: [3000, 3000] }]);
    await edit([{ op: "add_cards", cards: [{ key: INSERT, near: "far" }] }]);
    const added = onPage().cards.at(-1);
    expect(added?.at[0]).toBeGreaterThan(3000);
    expect(added?.at[1]).toBe(3000);
    expect((await refusal([{ op: "add_cards", cards: [{ key: MISSING.replace("gone", "other"), near: "nowhere" }] }])).message).toContain("no item with id “nowhere”");
  });

  it("refuses a key that isn't in the graph", async () => {
    open();
    expect(await refusal([{ op: "add_cards", cards: [{ key: MISSING }] }])).toMatchObject({ code: "invalid_op" });
  });

  it("follows resolved calls to the depth asked for, and says where following stopped", async () => {
    open({}, []);
    const result = await edit([{ op: "add_cards", cards: [{ key: CREATE, follow: { callees: 1 } }] }]);
    expect(result.items.map((item) => (item.kind === "card" ? [item.name, item.status, item.via?.depth] : []))).toEqual([
      ["createOrder", "created", undefined],
      ["OrderService.place", "created", 1],
      ["slack.post", "created", 1],
    ]);
    const [create, place] = result.items;
    expect(create?.kind === "card" ? create.stops : undefined).toEqual([
      { reason: "external", call: "req.header", line: 27, note: "express" },
      { reason: "external", call: "ordersQueue.add", line: 28, note: "bullmq" },
      { reason: "unresolved", call: "res.status().json", line: 29, note: "receiver type unknown" },
      { reason: "external", call: "res.status", line: 29, note: "express" },
    ]);
    // OrderService.place calls insertOrder: one resolved call past the depth asked for.
    expect(place?.kind === "card" ? place.stops : undefined).toEqual([{ reason: "depth", count: 1 }]);
    expect(onPage().cards.map((card) => card.id)).toEqual(["create-order", "order-service-place", "slack-post"]);

    const deeper = await edit([{ op: "add_cards", cards: [{ key: CREATE, follow: { callees: 2 } }] }]);
    expect(deeper.items.map((item) => (item.kind === "card" ? `${item.name} ${item.status}` : ""))).toEqual([
      "createOrder kept",
      "OrderService.place kept",
      "slack.post kept",
      "insertOrder created",
    ]);
  });

  it("follows callers, to the left, saying how many it didn't list", async () => {
    open();
    const result = await edit([{ op: "add_cards", cards: [{ key: CREATE, follow: { callers: 1 } }] }]);
    expect(result.items).toMatchObject([
      { id: "create-order", status: "kept", stops: [{ reason: "limit", count: 2 }] },
      { key: FILE, status: "created", via: { from: "create-order", direction: "callers", depth: 1 } },
    ]);
    expect(onPage().cards[1]?.at[0]).toBeLessThan(0);
  });
});

describe("set_view", () => {
  const group: Group = { id: "checks", card: "create-order", title: "Checks", members: [IF], collapsed: false };

  it("sets cards' views and folds, step groups' collapse, and the page's default view", async () => {
    open({ groups: [group] });
    const result = await edit([
      { op: "set_view", items: [{ id: "create-order", view: "flow", folded: true }, { id: "checks", collapsed: true }], defaultView: "diagram" },
    ]);
    expect(result.items).toMatchObject([
      { kind: "card", id: "create-order", status: "updated" },
      { kind: "group", id: "checks", status: "updated" },
    ]);
    expect(onPage()).toMatchObject({ defaultView: "diagram", cards: [{ view: "flow", folded: true }], groups: [{ collapsed: true }] });
  });

  it("refuses what an item doesn't take, and ids that aren't on the page", async () => {
    open({ groups: [group] });
    expect((await refusal([{ op: "set_view", items: [{ id: "checks", view: "flow" }] }])).message).toContain("is a step group");
    expect((await refusal([{ op: "set_view", items: [{ id: "create-order", collapsed: true }] }])).message).toContain("is a card");
    expect((await refusal([{ op: "set_view", items: [{ id: "nope", folded: true }] }])).code).toBe("invalid_op");
  });
});

describe("arrange", () => {
  it("lays the page out by call depth, then applies the moves", async () => {
    open({}, [
      { id: "slack-post", ref: { block: SLACK }, at: [0, 0] },
      { id: "create-order", ref: { block: CREATE }, at: [2000, 2000] },
      { id: "insert-order", ref: { block: INSERT }, at: [5000, 0] },
    ]);
    const result = await edit([{ op: "arrange", moves: [{ id: "insert-order", below: "create-order" }] }]);
    const cards = new Map(onPage().cards.map((card) => [card.id, card.at]));
    // createOrder calls slack.post, so slack.post sits in the next column.
    expect(cards.get("slack-post")?.[0]).toBeGreaterThan(cards.get("create-order")?.[0] ?? 0);
    // The page was never drawn: sizes are estimated from the cards' models.
    const create = cards.get("create-order") ?? [0, 0];
    expect(cards.get("insert-order")).toEqual([create[0], create[1] + estimatedBlockSize(model(CREATE), measureText).height + STACK_GAP]);
    expect(result.items.map((item) => (item.kind === "card" ? item.id : "")).sort()).toEqual(["create-order", "insert-order", "slack-post"]);
    expect((await refusal([{ op: "arrange", moves: [{ id: "nope", at: [0, 0] }] }])).code).toBe("invalid_op");
  });
});

describe("people's words are safe", () => {
  const described = { ...createCard, description: { text: "Places the order.", author: "person:maya", signature: "s" } };

  it("describe won't replace a person's description unless asked, and signs what it writes as the agent", async () => {
    open({}, [described]);
    const refused = await refusal([{ op: "describe", items: [{ id: "create-order", text: "Takes an order." }] }]);
    expect(refused).toMatchObject({ code: "people_text" });
    expect(refused.message).toContain("Maya wrote it");
    await edit([{ op: "describe", items: [{ id: "create-order", text: "Takes an order." }], replacePeopleText: true }]);
    expect(onPage().cards[0]?.description).toMatchObject({ text: "Takes an order.", author: "agent:claude" });
  });

  it("an agent replaces an agent's words, and a person editing them makes them theirs", async () => {
    open({}, [{ ...createCard, description: { text: "Old.", author: "agent:codex", signature: "s" } }]);
    const result = await edit([{ op: "describe", items: [{ id: "create-order", text: "New." }] }]);
    expect(result.items).toMatchObject([{ status: "updated" }]);
    store.editPage(TARGET, (draft) => {
      const card = draft.cards[0];
      if (card?.description) {
        card.description = { text: "Mine.", author: "person:maya", signature: "" };
      }
    });
    await store.flushBoard(REF);
    expect((await refusal([{ op: "describe", items: [{ id: "create-order", text: "Again." }] }])).code).toBe("people_text");
  });

  it("remove_items won't remove a person's description, group, or label unless asked", async () => {
    const link: Link = { id: "to-slack", from: "create-order", to: "slack", label: { text: "notifies", author: "person:maya", signature: "" } };
    const slack: Card = { id: "slack", ref: { block: SLACK }, at: [900, 0] };
    open({ links: [link], groups: [{ id: "checks", card: "create-order", title: "Checks", members: [IF], collapsed: false, author: "agent:claude" }] }, [described, slack]);
    expect((await refusal([{ op: "remove_items", ids: ["create-order"] }])).code).toBe("people_text");
    expect((await refusal([{ op: "remove_items", ids: ["to-slack"] }])).message).toContain("the label “notifies”");
    const removed = await edit([{ op: "remove_items", ids: ["checks"] }]);
    expect(removed.items).toEqual([{ kind: "group", id: "checks", status: "removed", card: "create-order", name: "Checks" }]);
    const all = await edit([{ op: "remove_items", ids: ["create-order", "to-slack"], replacePeopleText: true }]);
    expect(all.items.map((item) => `${item.kind} ${item.status}`)).toEqual(["card removed", "link removed"]);
    expect(onPage()).toMatchObject({ cards: [slack], links: [], groups: [] });
    expect((await refusal([{ op: "remove_items", ids: ["nope"] }])).code).toBe("invalid_op");
  });

  it("remove_items counts a drawn link and a person's connection as theirs, also when they would go with a card", async () => {
    const slack: Card = { id: "slack", ref: { block: SLACK }, at: [900, 0] };
    open({ links: [{ id: "drawn", from: "create-order", to: "slack" }] }, [createCard, slack]);
    expect((await refusal([{ op: "remove_items", ids: ["drawn"] }])).message).toContain("Can't change the link a person drew: a person wrote it.");
    expect((await refusal([{ op: "remove_items", ids: ["slack"] }])).code).toBe("people_text");
    open({ links: [{ id: "posts", kind: "http", from: "create-order", to: "slack", author: "person:maya" }] }, [createCard, slack]);
    expect((await refusal([{ op: "remove_items", ids: ["slack"] }])).message).toContain("Can't change the http connection: Maya wrote it.");
    const removed = await edit([{ op: "remove_items", ids: ["slack"], replacePeopleText: true }]);
    expect(removed.items).toMatchObject([
      { kind: "card", id: "slack", status: "removed" },
      { kind: "link", id: "posts", status: "removed" },
    ]);
  });

  it("a step group titled before titles were attributed counts as a person's", async () => {
    open({ groups: [{ id: "checks", card: "create-order", title: "Checks", members: [IF], collapsed: false }] });
    expect((await refusal([{ op: "remove_items", ids: ["checks"] }])).message).toContain("a person wrote it");
  });
});

describe("group_steps", () => {
  const job = `${IF}/variable:job`;
  const json = `${IF}/expression_statement[0]`;

  it("keeps the card's group of the same statements when the call comes again, and retitles it", async () => {
    open();
    const queue: AgentOp = { op: "group_steps", card: "create-order", groups: [{ title: "Queue it", lines: [28, 29] }] };
    await edit([queue]);
    expect((await edit([queue])).items).toMatchObject([{ kind: "group", id: "queue-it", status: "kept" }]);
    const retitled = await edit([{ op: "group_steps", card: "create-order", groups: [{ title: "Hand off", steps: [job, json] }] }]);
    expect(retitled.items).toMatchObject([{ kind: "group", id: "queue-it", status: "updated", name: "Hand off" }]);
    expect(onPage().groups).toMatchObject([{ id: "queue-it", title: "Hand off", members: [job, json] }]);
  });

  it("groups lines into whole statements of one branch, and names the statements it took", async () => {
    open();
    const result = await edit([{ op: "group_steps", card: "create-order", groups: [{ title: "Queue it", lines: [28, 29], description: "Hands it to a worker." }] }]);
    expect(result.items).toEqual([
      {
        kind: "group",
        id: "queue-it",
        status: "created",
        card: "create-order",
        name: "Queue it",
        lines: [28, 29],
        statements: [
          { key: job, kind: "variable", lines: [28, 28] },
          { key: json, kind: "expression_statement", lines: [29, 29] },
        ],
      },
    ]);
    expect(result.warnings).toEqual([]);
    expect(onPage().groups).toEqual([
      { id: "queue-it", card: "create-order", title: "Queue it", members: [job, json], collapsed: false, author: "agent:claude", description: { text: "Hands it to a worker.", author: "agent:claude", signature: "" } },
    ]);
  });

  it("says when a range took more lines than asked, and why a range breaks the rule", async () => {
    open();
    const widened = await edit([{ op: "group_steps", card: "create-order", groups: [{ title: "Guard", lines: [27, 30] }] }]);
    expect(widened.items).toMatchObject([{ lines: [27, 31], statements: [{ key: IF, kind: "if" }] }]);
    expect(widened.warnings).toEqual(["“Guard” took whole statements: lines 27–31 of createOrder, not lines 27–30."]);
    const header = await refusal([{ op: "group_steps", card: "create-order", groups: [{ title: "Header", lines: [25, 25] }] }]);
    expect(header.message).toContain("Can't make the step group “Header” from line 25 of createOrder");
    expect(header.message).toContain("A step group is consecutive statements in one branch.");
    const overlapping = await refusal([{ op: "group_steps", card: "create-order", groups: [{ title: "Again", lines: [28, 28] }] }]);
    expect(overlapping.message).toContain("overlap the group \"Guard\"");
  });

  it("takes step keys that are consecutive in one branch, and refuses ones that aren't", async () => {
    open();
    const result = await edit([{ op: "group_steps", card: "create-order", groups: [{ id: "queue", title: "Queue it", steps: [json, job] }] }]);
    expect(result.items).toMatchObject([{ id: "queue", lines: [28, 29] }]);
    const gap = await refusal([
      { op: "group_steps", card: "create-order", groups: [{ title: "Gap", steps: [`${CREATE}/variable:draft`, `${CREATE}/variable:order`] }] },
    ]);
    expect(gap.message).toContain("aren't consecutive statements of one branch");
    expect(gap.message).toContain("The statements there:");
    expect((await refusal([{ op: "group_steps", card: "create-order", groups: [{ title: "X", steps: ["nope"] }] }])).message).toContain("isn't a statement of createOrder");
    expect((await refusal([{ op: "group_steps", card: "create-order", groups: [{ title: "X" }] }])).message).toContain("either lines or steps");
  });

  it("updates a group by id, but not a person's title unless asked", async () => {
    open({ groups: [{ id: "checks", card: "create-order", title: "Checks", members: [IF], collapsed: false, author: "person:maya" }] });
    expect((await refusal([{ op: "group_steps", card: "create-order", groups: [{ id: "checks", title: "Guard", lines: [27, 31] }] }])).code).toBe("people_text");
    const kept = await edit([{ op: "group_steps", card: "create-order", groups: [{ id: "checks", title: "Checks", lines: [27, 31], collapsed: true }] }]);
    expect(kept.items).toMatchObject([{ id: "checks", status: "updated" }]);
    expect(onPage().groups[0]).toMatchObject({ title: "Checks", author: "person:maya", collapsed: true });
    await edit([{ op: "group_steps", card: "create-order", groups: [{ id: "checks", title: "Guard", lines: [27, 31] }], replacePeopleText: true }]);
    expect(onPage().groups[0]).toMatchObject({ title: "Guard", author: "agent:claude" });
  });

  it("refuses a card whose code isn't available", async () => {
    open();
    queryClient.setQueryData(engineKey("card", { project: P, key: CREATE }), { ...model(CREATE), code: null });
    expect(await refusal([{ op: "group_steps", card: "create-order", groups: [{ title: "X", lines: [28, 29] }] }])).toMatchObject({ code: "code_unavailable" });
  });
});

describe("label_steps", () => {
  const job = `${IF}/variable:job`;

  it("labels a card's Flow steps as the agent, and keeps a person's label unless asked", async () => {
    const theirs: LabelEntry = { key: IF, text: "Wants it later?", author: "person:maya", signature: "" };
    open({ labels: [theirs] });
    const result = await edit([{ op: "label_steps", card: "create-order", labels: [{ step: job, text: "Queue the order" }] }]);
    expect(result.items).toEqual([{ kind: "label", status: "created", card: "create-order", step: job, name: "Queue the order" }]);
    expect(onPage().labels).toEqual([theirs, { key: job, text: "Queue the order", author: "agent:claude", signature: "" }]);
    expect(readWorking().items).toEqual(["create-order"]);
    expect((await refusal([{ op: "label_steps", card: "create-order", labels: [{ step: IF, text: "Later?" }] }])).code).toBe("people_text");
  });

  it("refuses a key that isn't one of the card's Flow steps, and blank text", async () => {
    open();
    expect((await refusal([{ op: "label_steps", card: "create-order", labels: [{ step: "src/x.ts#function:x", text: "X" }] }])).message).toContain("isn't a Flow step of createOrder");
    expect((await refusal([{ op: "label_steps", card: "create-order", labels: [{ step: job, text: "  " }] }])).message).toContain("is blank");
  });
});

describe("reading and opening for an agent", () => {
  it("reads the page as the window holds it, unsaved edits included", () => {
    expect(readAgentPage({ requestId: 1, board: REF, page: "page-1" })).toMatchObject({ ok: false, code: "board_not_open" });
    open();
    store.editPage(TARGET, (draft) => {
      draft.title = "Placing an order";
    });
    store.updatePageView(TARGET, (view) => {
      view.sizes["create-order"] = { width: 520, height: 310 };
    });
    expect(readAgentPage({ requestId: 1, board: REF, page: "page-1" })).toMatchObject({
      ok: true,
      page: { id: "page-1", title: "Placing an order" },
      board: { title: "Order placement", revision: "r1", unsaved: true },
      sizes: { "create-order": { width: 520, height: 310 } },
    });
    expect(readAgentPage({ requestId: 1, board: REF, page: "page-2" })).toMatchObject({ ok: false, code: "page_not_found" });
  });

  it("opens a board without showing it, and passes on the engine's refusal", async () => {
    const other = { project: P, id: ".unfold/boards/other.json" };
    engine.uninstall();
    engine = installFakeEngine({
      handlers: {
        open_board: ({ id }) => {
          if (id !== other.id) {
            // eslint-disable-next-line @typescript-eslint/only-throw-error
            throw { code: "not_found", message: "No such board." };
          }
          return boardState(P, other.id);
        },
      },
    });
    open();
    showBoard(REF);
    expect(await openBoardForAgent({ requestId: 1, board: other })).toEqual({ ok: true });
    expect(store.getOpenBoard(other)).toBeDefined();
    expect(getShownBoard()).toEqual(REF);
    expect(await openBoardForAgent({ requestId: 2, board: { project: P, id: "third.json" } })).toMatchObject({ ok: false, code: "not_found" });
    expect(engine.windowCalls.filter((call) => call.command !== "report_shown_board")).toEqual([]);
  });
});
