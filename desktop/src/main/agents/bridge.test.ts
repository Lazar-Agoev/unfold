import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentEditSuccess, AgentShellEvent, ViewReport } from "../../shared/agent-api";
import { FakeBrowserWindow, resetFakeElectron } from "../fake-electron";
import type { WindowEntry } from "../windows";
import { AppWindow } from "../windows";
import { AgentBridge, type AgentEditInput } from "./bridge";
import { pageRevision } from "./revision";
import { ViewTracker } from "./views";

vi.mock("electron", async () => (await import("../fake-electron")).electronModule);

type Entry = WindowEntry & { window: FakeBrowserWindow };

const BOARD = { project: "p-orders0000", id: ".unfold/boards/orders.json" };
const PAGE = { id: "page-1", title: "Placing an order", cards: [] };
const BOARD_INFO = { title: "Order placement", place: "draft", changes: 0, revision: "r2" } as const;
const EDIT: AgentEditInput = {
  board: BOARD,
  page: "page-1",
  agent: { author: "agent:claude", name: "Claude" },
  title: "Add cards",
  ops: [{ op: "add_cards", cards: [{ key: "src/a.ts#function:a" }] }],
};
const SAVED: AgentEditSuccess = { ok: true, page: PAGE, board: BOARD_INFO, items: [], warnings: [] };

function report(tabs: ViewReport["tabs"]): ViewReport {
  return { selection: { kind: "none" }, visible: [], tabs };
}

function setup(timeoutMs = 1_000) {
  const views = new ViewTracker();
  let bridge: AgentBridge | null = null;
  const appWindow = new AppWindow(
    { kind: "dev", url: "http://localhost:5173/" },
    {
      onLoaded: () => undefined,
      onPlacementChanged: () => undefined,
      onClosing: () => undefined,
      onClosed: (entry) => {
        bridge?.windowClosed(entry.id);
      },
    },
    false,
  );
  bridge = new AgentBridge({ windows: appWindow, views, timeoutMs });
  const open = (): Entry => {
    const entry = appWindow.open() as Entry;
    entry.window.finishLoad();
    return entry;
  };
  return { bridge, views, appWindow, open };
}

/** The agent requests a window received, in order. */
function requests(entry: Entry): AgentShellEvent[] {
  return entry.window.sent.filter((event): event is AgentShellEvent => typeof event === "object" && event !== null && "requestId" in event);
}

/** Waits for the window's `n`-th agent request (1-based) and returns it. */
async function request(entry: Entry, n: number): Promise<AgentShellEvent & { requestId: number }> {
  await vi.waitFor(() => {
    expect(requests(entry).length).toBeGreaterThanOrEqual(n);
  });
  return requests(entry)[n - 1] as AgentShellEvent & { requestId: number };
}

beforeEach(() => {
  resetFakeElectron();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("routing an edit", () => {
  it("goes to the window when it holds the board, and comes back with the page's revision", async () => {
    const { bridge, views, open } = setup();
    const holder = open();
    views.report(holder.id, report([BOARD]));

    const outcome = bridge.edit(EDIT);
    const sent = await request(holder, 1);
    expect(sent).toMatchObject({ type: "agent_edit", board: BOARD, page: "page-1", title: "Add cards", agent: EDIT.agent, ops: EDIT.ops });
    expect(sent).not.toHaveProperty("expectRevision");
    bridge.settle(holder.id, "edit", sent.requestId, SAVED);
    expect(await outcome).toEqual({ ...SAVED, revision: pageRevision(PAGE) });
  });

  it("opens a board the window doesn't hold in the background, then edits it there", async () => {
    const { bridge, views, open } = setup();
    const entry = open();
    views.report(entry.id, report([]));

    const outcome = bridge.edit(EDIT);
    const opening = await request(entry, 1);
    expect(opening).toMatchObject({ type: "agent_open_board", board: BOARD, page: "page-1" });
    bridge.settle(entry.id, "open", opening.requestId, { ok: true });
    const editing = await request(entry, 2);
    expect(editing.type).toBe("agent_edit");
    bridge.settle(entry.id, "edit", editing.requestId, SAVED);
    expect((await outcome).ok).toBe(true);
    expect(views.holder(BOARD)).toBe(entry.id);
  });

  it("opens the window when it is closed, and sends the request once its UI has loaded", async () => {
    const { bridge, appWindow } = setup();
    const outcome = bridge.open({ board: BOARD });
    const created = appWindow.current() as Entry | undefined;
    if (!created) {
      throw new Error("the window wasn't opened");
    }
    expect(requests(created)).toEqual([]);
    created.window.finishLoad();
    const opening = await request(created, 1);
    bridge.settle(created.id, "open", opening.requestId, { ok: true });
    expect(await outcome).toEqual({ ok: true, windowId: created.id });
  });

  it("passes on the window's refusal to open a board", async () => {
    const { bridge, views, open } = setup();
    const entry = open();
    const outcome = bridge.open({ board: BOARD });
    const opening = await request(entry, 1);
    const refusal = { ok: false, code: "too_many_tabs", message: "Close a board first." };
    bridge.settle(entry.id, "open", opening.requestId, refusal);
    expect(await outcome).toEqual(refusal);
    expect(views.holder(BOARD)).toBeUndefined();
  });

  it("opens the board again when the window's report still listed a board it closed", async () => {
    const { bridge, views, open } = setup();
    const entry = open();
    views.report(entry.id, report([BOARD]));

    const outcome = bridge.edit(EDIT);
    const first = await request(entry, 1);
    bridge.settle(entry.id, "edit", first.requestId, { ok: false, code: "board_not_open", message: "Not here." });
    const opening = await request(entry, 2);
    expect(opening.type).toBe("agent_open_board");
    bridge.settle(entry.id, "open", opening.requestId, { ok: true });
    const editing = await request(entry, 3);
    bridge.settle(entry.id, "edit", editing.requestId, SAVED);
    expect((await outcome).ok).toBe(true);
    expect(views.holder(BOARD)).toBe(entry.id);
  });

  it("passes on the window's refusal", async () => {
    const { bridge, views, open } = setup();
    const holder = open();
    views.report(holder.id, report([BOARD]));
    const outcome = bridge.edit(EDIT);
    const sent = await request(holder, 1);
    const refusal = { ok: false, code: "board_problem", message: "The key is gone." };
    bridge.settle(holder.id, "edit", sent.requestId, refusal);
    expect(await outcome).toEqual(refusal);
  });
});

describe("expected revisions", () => {
  it("reads the page first and refuses an edit when it changed since the agent read it", async () => {
    const { bridge, views, open } = setup();
    const holder = open();
    views.report(holder.id, report([BOARD]));

    const refused = bridge.edit({ ...EDIT, expectRevision: "00000000" });
    const read = await request(holder, 1);
    expect(read).toMatchObject({ type: "agent_read", board: BOARD, page: "page-1" });
    bridge.settle(holder.id, "read", read.requestId, { ok: true, page: PAGE, board: { ...BOARD_INFO, unsaved: false } });
    const outcome = await refused;
    expect(outcome).toMatchObject({ ok: false, code: "revision_mismatch" });
    expect(!outcome.ok && outcome.message).toContain(`now rev ${pageRevision(PAGE)}`);
    expect(requests(holder)).toHaveLength(1);

    const accepted = bridge.edit({ ...EDIT, expectRevision: pageRevision(PAGE) });
    const reread = await request(holder, 2);
    bridge.settle(holder.id, "read", reread.requestId, { ok: true, page: PAGE, board: { ...BOARD_INFO, unsaved: false } });
    const editing = await request(holder, 3);
    expect(editing.type).toBe("agent_edit");
    bridge.settle(holder.id, "edit", editing.requestId, SAVED);
    expect((await accepted).ok).toBe(true);
  });

  it("opens the board again when the window's report still listed a board it closed, then reads it", async () => {
    const { bridge, views, open } = setup();
    const entry = open();
    views.report(entry.id, report([BOARD]));

    const outcome = bridge.edit({ ...EDIT, expectRevision: pageRevision(PAGE) });
    const first = await request(entry, 1);
    expect(first.type).toBe("agent_read");
    bridge.settle(entry.id, "read", first.requestId, { ok: false, code: "board_not_open", message: "Not here." });
    const opening = await request(entry, 2);
    expect(opening.type).toBe("agent_open_board");
    bridge.settle(entry.id, "open", opening.requestId, { ok: true });
    const read = await request(entry, 3);
    bridge.settle(entry.id, "read", read.requestId, { ok: true, page: PAGE, board: { ...BOARD_INFO, unsaved: false } });
    const editing = await request(entry, 4);
    bridge.settle(entry.id, "edit", editing.requestId, SAVED);
    expect((await outcome).ok).toBe(true);
  });
});

describe("reading a held page", () => {
  it("reads from the window with the page's revision, and refuses a board it doesn't hold", async () => {
    const { bridge, views, open } = setup();
    const holder = open();
    expect(await bridge.read({ board: BOARD, page: "page-1" })).toMatchObject({ ok: false, code: "board_not_open" });
    expect(requests(holder)).toEqual([]);

    views.report(holder.id, report([BOARD]));
    const outcome = bridge.read({ board: BOARD, page: "page-1" });
    const sent = await request(holder, 1);
    const read = { ok: true, page: PAGE, board: { ...BOARD_INFO, unsaved: true } };
    bridge.settle(holder.id, "read", sent.requestId, read);
    expect(await outcome).toEqual({ ...read, revision: pageRevision(PAGE) });
  });

  it("forgets a board the window's report still listed after it closed it", async () => {
    const { bridge, views, open } = setup();
    const stale = open();
    views.report(stale.id, report([BOARD]));
    const outcome = bridge.read({ board: BOARD, page: "page-1" });
    bridge.settle(stale.id, "read", (await request(stale, 1)).requestId, { ok: false, code: "board_not_open", message: "Not here." });
    expect(await outcome).toMatchObject({ ok: false, code: "board_not_open" });
    expect(views.holder(BOARD)).toBeUndefined();
  });
});

describe("answers", () => {
  it("times out when the window doesn't answer, saying a change may still land, and ignores a late answer", async () => {
    vi.useFakeTimers();
    const { bridge, views, open } = setup(5_000);
    const holder = open();
    views.report(holder.id, report([BOARD]));
    const read = bridge.read({ board: BOARD, page: "page-1" });
    await vi.advanceTimersByTimeAsync(5_000);
    expect(await read).toEqual({ ok: false, code: "timeout", message: "The Unfold window didn't answer within 5 s." });
    const outcome = bridge.edit(EDIT);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(await outcome).toEqual({ ok: false, code: "unconfirmed", message: "The Unfold window didn't answer within 5 s, so the change may still land." });
    const sent = requests(holder).at(-1) as AgentShellEvent & { requestId: number };
    bridge.settle(holder.id, "edit", sent.requestId, SAVED);
  });

  it("fails the requests of a window that closes before it answers", async () => {
    const { bridge, views, open } = setup();
    const holder = open();
    views.report(holder.id, report([BOARD]));
    const outcome = bridge.edit(EDIT);
    await request(holder, 1);
    holder.window.close();
    expect(await outcome).toMatchObject({ ok: false, code: "window_closed" });
    expect(views.holder(BOARD)).toBeUndefined();
  });

  it("takes an answer only from the window asked, for the request's kind, and checks its shape", async () => {
    const { bridge, views, open } = setup();
    const holder = open();
    views.report(holder.id, report([BOARD]));
    const outcome = bridge.edit(EDIT);
    const sent = await request(holder, 1);
    bridge.settle(holder.id + 1, "edit", sent.requestId, SAVED);
    bridge.settle(holder.id, "read", sent.requestId, SAVED);
    bridge.settle(holder.id, "edit", "1", SAVED);
    bridge.settle(holder.id, "edit", sent.requestId, { ok: true, page: PAGE });
    expect(await outcome).toEqual({ ok: false, code: "internal", message: "The window answered with a malformed result." });
  });
});

describe("boards as a whole", () => {
  const CREATED = { ok: true, board: BOARD, info: BOARD_INFO, folder: null, pages: [{ id: "page-1", title: "Page 1" }] };

  it("has the window create a draft, which it then holds", async () => {
    const { bridge, views, open } = setup();
    const entry = open();
    const outcome = bridge.create({ project: BOARD.project, title: "Order placement", pages: ["Page 1"] });
    const sent = await request(entry, 1);
    expect(sent).toMatchObject({ type: "agent_create_board", project: BOARD.project, title: "Order placement", pages: ["Page 1"] });
    bridge.settle(entry.id, "create", sent.requestId, CREATED);
    expect(await outcome).toEqual({ ...CREATED, windowId: entry.id });
    expect(views.holder(BOARD)).toBe(entry.id);
  });

  it("copies a board the window holds", async () => {
    const { bridge, views, open } = setup();
    const holder = open();
    views.report(holder.id, report([BOARD]));
    const copy = { ...CREATED, board: { project: BOARD.project, id: ".unfold/local/drafts/orders-copy.json" } };
    const outcome = bridge.create({ project: BOARD.project, copyOf: BOARD.id });
    const sent = await request(holder, 1);
    expect(sent).toMatchObject({ type: "agent_create_board", copyOf: BOARD.id });
    bridge.settle(holder.id, "create", sent.requestId, copy);
    expect(await outcome).toMatchObject({ ok: true, windowId: holder.id });
    expect(views.holder(copy.board)).toBe(holder.id);
  });

  it("copies a board after opening it again when the window's report still listed it", async () => {
    const { bridge, views, open } = setup();
    const entry = open();
    views.report(entry.id, report([BOARD]));
    const copy = { ...CREATED, board: { project: BOARD.project, id: ".unfold/local/drafts/orders-copy.json" } };
    const outcome = bridge.create({ project: BOARD.project, copyOf: BOARD.id });
    bridge.settle(entry.id, "create", (await request(entry, 1)).requestId, { ok: false, code: "board_not_open", message: "This window doesn't hold the board to copy." });
    const opening = await request(entry, 2);
    expect(opening.type).toBe("agent_open_board");
    bridge.settle(entry.id, "open", opening.requestId, { ok: true });
    const copying = await request(entry, 3);
    expect(copying).toMatchObject({ type: "agent_create_board", copyOf: BOARD.id });
    bridge.settle(entry.id, "create", copying.requestId, copy);
    expect(await outcome).toMatchObject({ ok: true, windowId: entry.id });
    expect(views.holder(BOARD)).toBe(entry.id);
    expect(views.holder(copy.board)).toBe(entry.id);
  });

  it("edits a board's pages in the window, opening the board there first", async () => {
    const { bridge, open } = setup();
    const last = open();
    const edit = { board: BOARD, agent: EDIT.agent, title: "Edit pages", ops: [{ op: "add_page", title: "More" }] } as const;
    const outcome = bridge.editBoard(edit);
    const opening = await request(last, 1);
    bridge.settle(last.id, "open", opening.requestId, { ok: true });
    const sent = await request(last, 2);
    expect(sent).toMatchObject({ type: "agent_edit_board", ...edit });
    const done = { ok: true, board: BOARD_INFO, pages: [{ id: "page-1", title: "Page 1" }, { id: "page-2", title: "More" }], results: [{ op: "add_page", status: "created", page: "page-2", title: "More" }] };
    bridge.settle(last.id, "board", sent.requestId, done);
    expect(await outcome).toEqual(done);

    const malformed = bridge.editBoard(edit);
    const again = await request(last, 3);
    bridge.settle(last.id, "board", again.requestId, { ok: true, board: BOARD_INFO });
    expect(await malformed).toMatchObject({ ok: false, code: "internal" });
  });

  it("shows a board: the window switches to it and comes to the front, or opens it in the background only", async () => {
    const { bridge, views, open } = setup();
    const holder = open();
    views.report(holder.id, report([BOARD]));

    const shown = bridge.show({ board: BOARD, page: "page-1", item: "create-order" }, false);
    const sent = await request(holder, 1);
    expect(sent).toMatchObject({ type: "agent_show", board: BOARD, page: "page-1", item: "create-order" });
    bridge.settle(holder.id, "show", sent.requestId, { ok: true, page: "page-1" });
    expect(await shown).toEqual({ ok: true, windowId: holder.id, page: "page-1" });
    expect(FakeBrowserWindow.focused).toBe(holder.window);

    FakeBrowserWindow.focused = null;
    const refused = bridge.show({ board: BOARD, item: "nope" }, false);
    const second = await request(holder, 2);
    bridge.settle(holder.id, "show", second.requestId, { ok: false, code: "invalid_op", message: "No item nope." });
    expect(await refused).toMatchObject({ ok: false, code: "invalid_op" });
    expect(FakeBrowserWindow.focused).toBeNull();

    expect(await bridge.show({ board: BOARD }, true)).toEqual({ ok: true, windowId: holder.id });
    expect(requests(holder)).toHaveLength(2);
  });
});

describe("repairing", () => {
  it("has the window that holds the board apply the repair, and comes back with the page's revision", async () => {
    const { bridge, views, open } = setup();
    const holder = open();
    views.report(holder.id, report([BOARD]));
    const repair = { board: BOARD, page: "page-1", agent: EDIT.agent, repair: { page: "page-1", kind: "remove_card", card: "a" }, items: [] };
    const outcome = bridge.repair(repair);
    const sent = await request(holder, 1);
    expect(sent).toMatchObject({ type: "agent_repair", ...repair });
    bridge.settle(holder.id, "repair", sent.requestId, { ok: true, page: PAGE, board: BOARD_INFO });
    expect(await outcome).toEqual({ ok: true, page: PAGE, board: BOARD_INFO, revision: pageRevision(PAGE) });

    const refused = bridge.repair(repair);
    const again = await request(holder, 2);
    bridge.settle(holder.id, "repair", again.requestId, { ok: false, code: "board_problem", message: "No such card." });
    expect(await refused).toEqual({ ok: false, code: "board_problem", message: "No such card." });
  });
});

describe("finishing", () => {
  it("remembers the boards each agent edited or repaired until it finishes them", async () => {
    const { bridge, views, open } = setup();
    const holder = open();
    views.report(holder.id, report([BOARD]));
    const codex = { author: "agent:codex", name: "Codex" } as const;
    const edited = bridge.edit(EDIT);
    bridge.settle(holder.id, "edit", (await request(holder, 1)).requestId, SAVED);
    await edited;
    const refused = bridge.edit({ ...EDIT, board: { project: "p-orders0000", id: "other.json" }, agent: codex });
    bridge.settle(holder.id, "open", (await request(holder, 2)).requestId, { ok: false, code: "not_found", message: "No board." });
    await refused;

    expect(bridge.workedOn("agent:claude")).toEqual([BOARD]);
    expect(bridge.workedOn("agent:codex")).toEqual([]);
    bridge.finish({ board: BOARD, agent: EDIT.agent, summary: "Done" });
    expect(bridge.workedOn("agent:claude")).toEqual([]);
  });

  it("tells the window when it is open", () => {
    const { bridge, open } = setup();
    const finish = { board: BOARD, agent: EDIT.agent, summary: "Added the order flow" };
    bridge.finish(finish);

    const entry = open();
    bridge.finish(finish);
    expect(entry.window.sent).toEqual([{ type: "agent_finish", ...finish }]);
  });
});
