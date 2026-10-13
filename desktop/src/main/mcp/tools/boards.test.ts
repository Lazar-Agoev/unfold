import { describe, expect, it, vi } from "vitest";
import type { AgentBoardEditRequest, AgentCreateRequest, AgentPage, AgentShellEvent } from "../../../shared/agent-api";
import { pageRevision } from "../../agents/revision";
import type { BoardSummary } from "../engine-types";
import { createBoard, editPages, organizeBoards, show } from "./boards";
import { harness, PROJECT, projectRow, refusal } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const BOARD = { project: PROJECT, id: ".unfold/boards/order-placement.json" };
const INFO = { title: "Order placement", revision: "r1" } as const;
const PAGE: AgentPage = {
  id: "page-1",
  title: "Placing an order",
  defaultView: "diagram",
  cards: [{ id: "create-order", ref: { block: "src/routes/orders.ts#function:createOrder" }, at: [0, 0] }],
  groups: [],
  labels: [],
  guards: [],
  links: [],
};
const ENTRANCE = 'entrance:["express.route","http","POST","/orders","src/routes/orders.ts"]';

function summary(id: string, title: string): BoardSummary {
  return { project: PROJECT, id, name: id, title, folder: null, pages: 1, cards: 0, updatedAtMs: 0, label: null, error: null };
}

/** A window that holds what it's asked to, and answers as the UI does. */
function answers(request: AgentShellEvent): unknown {
  switch (request.type) {
    case "agent_create_board":
      return { ok: true, board: BOARD, info: INFO, folder: request.folder ?? null, pages: (request.pages ?? ["Page 1"]).map((title, index) => ({ id: `page-${index + 1}`, title })) };
    case "agent_read":
      return { ok: true, page: { ...PAGE, id: request.page }, board: { ...INFO, unsaved: false } };
    case "agent_open_board":
      return { ok: true };
    case "agent_edit":
      return {
        ok: true,
        page: PAGE,
        board: INFO,
        items: [
          { kind: "card", id: "post-orders", status: "created", key: ENTRANCE, name: "POST /orders" },
          { kind: "card", id: "create-order", status: "created", key: "src/routes/orders.ts#function:createOrder", name: "createOrder", stops: [{ reason: "external", call: "ordersQueue.add", line: 28, note: "bullmq" }] },
        ],
        warnings: [],
      };
    case "agent_show":
      return { ok: true, page: request.page ?? "page-1" };
    default:
      return undefined;
  }
}

describe("create_board", () => {
  it("has the window create the board, with its pages, and traces an entrance onto it", async () => {
    const app = harness({ window: answers, engine: { peek: () => ({ key: ENTRANCE }) } });
    const { text } = await app.call(createBoard, { project: "ts-orders", title: "Order placement", folder: "orders", pages: ["Placing an order", "Async path"], trace: ENTRANCE });
    const [created, edit] = app.requests;
    expect(created).toMatchObject({ type: "agent_create_board", project: PROJECT, title: "Order placement", folder: "orders", pages: ["Placing an order", "Async path"] } satisfies Partial<AgentCreateRequest> & { type: string });
    expect(edit).toMatchObject({ type: "agent_edit", board: BOARD, page: "page-1", title: "Trace", ops: [{ op: "add_cards", cards: [{ key: ENTRANCE, follow: { callees: 1 } }] }] });
    expect(app.views.holder(BOARD)).toBe(1);
    expect(text.split("\n")).toEqual([
      `Unfold · ts-orders › Order placement › Placing an order · rev ${pageRevision(PAGE)} · as Claude`,
      `Created the board “Order placement” (board id ${BOARD.id}) in the folder orders. Unfold loaded it; the person's view didn't move.`,
      "Pages:",
      "1. Placing an order (page id page-1)",
      "2. Async path (page id page-2)",
      `Traced POST /orders on the first page (rev ${pageRevision(PAGE)}):`,
      "- POST /orders (card id post-orders, created)",
      "- createOrder (card id create-order, created); stopped: ordersQueue.add (line 28) is external, bullmq",
      "Next: add_cards to build a page (pass the board and page ids), then arrange_page and check. show presents it when it's ready.",
    ]);
  });

  it("refuses an entrance the project doesn't have before creating anything, and teaches from the window's refusals", async () => {
    const app = harness({
      window: answers,
      engine: {
        peek: () => {
          throw refusal("not_found", "no such key");
        },
      },
    });
    const traced = await app.call(createBoard, { project: "ts-orders", title: "Orders", trace: 'entrance:["x"]' });
    expect(traced).toMatchObject({ isError: true, text: `Can't trace the entrance: ts-orders has no entrance with the key entrance:["x"]. list_entrances lists its entrances with their keys.` });
    expect(app.requests).toEqual([]);
    expect((await app.call(createBoard, { project: "ts-orders", title: "Orders", trace: "src/a.ts#function:a" })).isError).toBe(true);

    const late = harness({ window: () => ({ ok: false, code: "unconfirmed", message: "The Unfold window didn't answer within 30 s, so the change may still land." }) });
    expect((await late.call(createBoard, { project: "ts-orders", title: "Orders" })).text).toBe(
      "The Unfold window didn't answer within 30 s, so the change may still land. Read what you changed first (read_page, or list_boards for a board's pages and title), and try again only if it didn't land.",
    );
  });
});

describe("organize_boards", () => {
  it("runs folder ops in the engine and board ops in the window, in order", async () => {
    const app = harness({
      window: (request) =>
        request.type === "agent_edit_board"
          ? { ok: true, board: { ...INFO, title: "Orders, end to end" }, pages: [{ id: "page-1", title: "Placing an order" }], results: [{ op: "set_title", status: "updated", title: "Orders, end to end" }] }
          : answers(request),
      engine: {
        create_folder: () => null,
        set_board_folder: (args) => ({ ...summary(String(args["id"]), "Order placement"), folder: args["folder"] }),
        list_boards: () => ({ project: PROJECT, boards: [summary(BOARD.id, "Order placement")], emptyFolders: [] }),
      },
    });
    const { text } = await app.call(organizeBoards, {
      project: "ts-orders",
      ops: [
        { op: "create_folder", path: "orders" },
        { op: "move", board: BOARD.id, folder: "orders" },
        { op: "rename", board: `unfold://ts-orders/${BOARD.id}`, title: "Orders, end to end" },
      ],
    });
    expect(text.split("\n").slice(1)).toEqual([
      "- Created the folder orders.",
      `- Moved “Order placement” (board id ${BOARD.id}) into orders.`,
      `- Renamed board ${BOARD.id} to “Orders, end to end” (one undo step in Unfold).`,
      "Next: list_boards shows the boards and folders now.",
    ]);
    const edit = app.requests.find((request) => request.type === "agent_edit_board") as AgentBoardEditRequest | undefined;
    expect(edit).toMatchObject({ board: BOARD, agent: { author: "agent:claude" }, title: "Rename board", ops: [{ op: "set_title", title: "Orders, end to end" }] });
  });

  it("says what was done before an op failed, and teaches what agents can't do", async () => {
    const app = harness({
      engine: {
        create_folder: () => null,
        delete_folder: () => {
          throw refusal("folder_not_empty", "the folder `flows` has boards");
        },
      },
    });
    const failed = await app.call(organizeBoards, { project: "ts-orders", ops: [{ op: "create_folder", path: "old" }, { op: "delete_folder", path: "flows" }, { op: "create_folder", path: "never" }] });
    expect(failed.isError).toBe(true);
    expect(failed.text).toBe(
      "The folder `flows` has boards. Move its boards out first (move); agents never delete boards.\nDone before it:\n- Created the folder old.\nThe ops after it weren't tried.",
    );
    expect(app.engineCalls.filter(([command]) => command === "create_folder")).toHaveLength(1);
    for (const op of ["delete", "duplicate"]) {
      const refused = await app.call(organizeBoards, { project: "ts-orders", ops: [{ op, board: BOARD.id }] });
      expect(refused.isError).toBe(true);
      expect(refused.text).toContain("op is one of create_folder, rename_folder, delete_folder, move, or rename. Agents can't delete boards");
    }
  });

  it("refuses a link to another project's board", async () => {
    const app = harness({ projects: [projectRow(PROJECT, "ts-orders"), projectRow("p-other00000", "py-orders")] });
    const missing = await app.call(organizeBoards, { project: "py-orders", ops: [{ op: "move", board: "unfold://ts-orders/.unfold/boards/x.json", folder: null }] });
    expect(missing.text).toBe("Can't organize the board: the link names a board of ts-orders, not py-orders. Organize each project's boards in its own call.");
  });
});

describe("edit_pages", () => {
  it("sends one board edit to the window, for the board in view by default, and lists the pages after", async () => {
    const app = harness({
      window: (request) =>
        request.type === "agent_edit_board"
          ? {
              ok: true,
              board: INFO,
              pages: [
                { id: "page-1", title: "Placing an order" },
                { id: "page-3", title: "Async path" },
              ],
              results: [
                { op: "add_page", status: "created", page: "page-3", title: "Async path" },
                { op: "delete_page", status: "removed", page: "page-2", title: "Scratch" },
              ],
            }
          : answers(request),
    });
    app.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
    const { text } = await app.call(editPages, { ops: [{ op: "add", title: "Async path" }, { op: "delete", page: "page-2" }], replacePeopleText: true });
    expect(app.requests.at(-1)).toMatchObject({
      type: "agent_edit_board",
      board: BOARD,
      title: "Edit pages",
      ops: [{ op: "add_page", title: "Async path" }, { op: "delete_page", page: "page-2" }],
      replacePeopleText: true,
    });
    expect(text.split("\n")).toEqual([
      "Unfold · ts-orders › Order placement · as Claude",
      `Board id ${BOARD.id}: one undo step in Unfold.`,
      "- Added the page “Async path” (page id page-3).",
      "- Deleted the page “Scratch” (page id page-2).",
      "Pages now:",
      "1. Placing an order (page id page-1)",
      "2. Async path (page id page-3)",
      "Next: add_cards with a page's id builds it.",
    ]);
  });

  it("passes the window's refusal on, and needs a board", async () => {
    const app = harness({ window: (request) => (request.type === "agent_edit_board" ? { ok: false, code: "people_text", message: "Can't change the label “Queue it” on the page “Scratch”: Lazar wrote it." } : answers(request)) });
    app.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
    expect(await app.call(editPages, { ops: [{ op: "delete", page: "page-2" }] })).toMatchObject({ isError: true, text: "Can't change the label “Queue it” on the page “Scratch”: Lazar wrote it." });
    expect((await app.call(editPages, { ops: [{ op: "duplicate", page: "page-1" }] })).isError).toBe(true);
    const none = harness();
    expect((await none.call(editPages, { ops: [{ op: "add", title: "More" }] })).text).toBe(
      "Can't tell which board you mean: no board is in view in Unfold, and none was named. Pass board (an id from list_boards) or a link the person pasted, or ask them to open the board.",
    );
  });
});

describe("show", () => {
  it("brings an item into view and the window forward", async () => {
    const app = harness({ window: answers });
    const { text } = await app.call(show, { item: `unfold://ts-orders/${BOARD.id}?page=page-1&item=create-order` });
    expect(app.requests.map((request) => request.type)).toEqual(["agent_open_board", "agent_show", "agent_read"]);
    expect(app.requests[1]).toMatchObject({ board: BOARD, page: "page-1", item: "create-order" });
    expect(app.focused).toEqual([1]);
    expect(text.split("\n").slice(1)).toEqual([
      "Showed the card createOrder on the page “Placing an order” of “Order placement”, selected and centered; its window came to the front.",
      `Ids: board ${BOARD.id}, page page-1.`,
    ]);
  });

  it("only loads the board in the background, and won't show an item there", async () => {
    const app = harness({ window: answers });
    app.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [] });
    const { text } = await app.call(show, { mode: "background" });
    expect(app.requests.map((request) => request.type)).toEqual(["agent_open_board", "agent_read"]);
    expect(app.focused).toEqual([]);
    expect(text).toContain("Loaded “Order placement” in Unfold; the person's view didn't move.");
    expect((await app.call(show, { mode: "background", item: "create-order" })).isError).toBe(true);
    const refusedShow = harness({ window: (request) => (request.type === "agent_show" ? { ok: false, code: "invalid_op", message: "There is no item with id “nope” on the page “Placing an order”." } : answers(request)) });
    refusedShow.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
    expect(await refusedShow.call(show, { item: "nope" })).toMatchObject({ isError: true, text: "There is no item with id “nope” on the page “Placing an order”." });
  });
});
