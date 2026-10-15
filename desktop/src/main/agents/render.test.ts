import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentBoardRef } from "../../shared/agent-api";
import type { RenderShellEvent } from "../../shared/render-api";
import { EngineRequestError } from "../engine-host";
import { createdWindows, FakeBrowserWindow, resetFakeElectron } from "../fake-electron";
import type { BoardHealthView, HealthItem } from "../mcp/engine-types";
import type { WindowEntry } from "../windows";
import { AgentBridge } from "./bridge";
import { imageFrame, PageRenderer } from "./render";
import { ViewTracker } from "./views";

vi.mock("electron", async () => (await import("../fake-electron")).electronModule);

const BOARD: AgentBoardRef = { project: "p-orders0000", id: ".unfold/boards/orders.json" };
const PAGE = { id: "page-1", title: "Placing an order", defaultView: "compact", cards: [], names: [], combined: [], guards: [], links: [] };
const INFO = { title: "Orders", revision: "r1", unsaved: false };
const OUTDATED: HealthItem = { page: "page-1", kind: "outdated", target: { card: "create-order" }, message: "The code changed.", repairs: ["resign"] };
const ELSEWHERE: HealthItem = { ...OUTDATED, page: "page-2" };
const HEALTH: BoardHealthView = {
  health: { items: [OUTDATED, ELSEWHERE], missing: 0, outdated: 2, review: 0, brokenCombined: 0, brokenLinks: 0 },
  label: "2 outdated",
};
const STRAYS = { kind: "strays", items: ["a"], message: "a has no line.", suggestion: "connect it." } as const;

/** The engine host as the renderer sees it: attaches and detaches the render window, and answers commands. */
class FakeEngine {
  readonly attached: number[] = [];
  readonly detached: number[] = [];
  readonly calls: [number, string, Readonly<Record<string, unknown>>][] = [];
  readonly answers = new Map<string, () => unknown>([
    ["board_health", () => HEALTH],
    ["read_board", () => ({ document: { title: "Orders", pages: [PAGE, { ...PAGE, id: "page-2" }] } })],
  ]);

  attach(entry: WindowEntry): void {
    this.attached.push(entry.id);
  }
  detach(entry: WindowEntry): void {
    this.detached.push(entry.id);
  }
  untrusted(windowId: number, command: string, args: Readonly<Record<string, unknown>>): Promise<unknown> {
    this.calls.push([windowId, command, args]);
    const answer = this.answers.get(command);
    return answer ? Promise.resolve().then(answer) : Promise.reject(new EngineRequestError({ code: "not_found", message: "board does not exist" }));
  }
}

function setup(options: { held?: boolean; timeoutMs?: number; idleMs?: number; showAllCalls?: () => boolean } = {}) {
  const engine = new FakeEngine();
  const views = new ViewTracker();
  if (options.held === true) {
    views.hold(BOARD);
  }
  const bridge = {
    views,
    read: vi.fn(({ page }: { page: string }) =>
      Promise.resolve(
        views.holds(BOARD)
          ? { ok: true as const, page: { ...PAGE, id: page, title: `Page ${page}` }, board: INFO, revision: "abcd1234", windowId: 7 }
          : { ok: false as const, code: "board_not_open", message: "Unfold's window doesn't hold this board." },
      ),
    ),
  };
  let ids = 40;
  const renderer = new PageRenderer({
    ui: { kind: "app", dir: "/ui" },
    bridge,
    engine,
    reserveId: () => ids++,
    showAllCalls: options.showAllCalls ?? (() => false),
    ...(options.timeoutMs !== undefined ? { timeoutMs: options.timeoutMs } : {}),
    idleMs: options.idleMs ?? 60_000,
  });
  return { engine, bridge, renderer };
}

/** The render window's `count`th request, once it has been sent. */
async function request(window: FakeBrowserWindow, count: number): Promise<RenderShellEvent> {
  await vi.waitFor(() => {
    expect(window.sent).toHaveLength(count);
  });
  return window.sent[count - 1] as RenderShellEvent;
}

/** The `index`th render window, once it exists. */
function created(index = 0): Promise<FakeBrowserWindow> {
  return vi.waitFor(() => {
    const window = createdWindows[index];
    if (!window) {
      throw new Error("No render window yet.");
    }
    return window;
  });
}

/** The `index`th render window, once it exists, loaded. */
async function loaded(index = 0): Promise<FakeBrowserWindow> {
  const window = await created(index);
  window.finishLoad();
  return window;
}

beforeEach(() => {
  resetFakeElectron();
});

describe("imageFrame", () => {
  it("draws what fits at zoom 1, with padding", () => {
    expect(imageFrame({ x: 100, y: 50, width: 800, height: 400 }, 1)).toEqual({ width: 848, height: 448, camera: { x: -76, y: -26, zoom: 1 }, zoom: 1 });
  });

  it("zooms out to keep the long side within 4,096 pixels, and halves the room at scale 2", () => {
    const wide = imageFrame({ x: 0, y: 0, width: 10_000, height: 2000 }, 1);
    expect(wide.width).toBe(4096);
    expect(wide.zoom).toBeCloseTo(4048 / 10_000);
    const sharp = imageFrame({ x: 0, y: 0, width: 3000, height: 1000 }, 2);
    expect([sharp.width, sharp.zoom]).toEqual([2048, 2000 / 3000]);
  });

  it("fits a smaller room when the window couldn't take the size", () => {
    const frame = imageFrame({ x: 0, y: 0, width: 2000, height: 1000 }, 1, { width: 1048, height: 700 });
    expect(frame.zoom).toBe(0.5);
    expect([frame.width, frame.height]).toEqual([1048, 548]);
  });
});

describe("screenshot", () => {
  it("draws the held page with its health in an offscreen render window, frames it, and captures it", async () => {
    const { engine, bridge, renderer } = setup({ held: true });
    const shot = renderer.screenshot({ board: BOARD, page: "page-1" });

    const window = await created();
    expect(window.url).toBe("app://unfold/render.html");
    expect(window.options).toMatchObject({ show: false, webPreferences: { sandbox: true, contextIsolation: true, nodeIntegration: false, devTools: false, offscreen: { deviceScaleFactor: 1 } } });
    // Nothing is sent before the window's UI has loaded; then it is attached to the engine.
    expect(window.sent).toEqual([]);
    window.finishLoad();
    expect(engine.attached).toEqual([40]);

    const page = await request(window, 1);
    expect(page).toEqual({
      type: "render_page",
      purpose: "image",
      board: BOARD,
      title: "Orders",
      page: { ...PAGE, title: "Page page-1" },
      health: HEALTH.health,
      showAllCalls: false,
      requestId: expect.any(Number) as number,
    });
    expect(bridge.read).toHaveBeenCalledWith({ board: BOARD, page: "page-1" });
    expect(engine.calls).toEqual([[0, "board_health", { project: BOARD.project, id: BOARD.id }]]);

    renderer.settle(page.requestId, { ok: true, bounds: { x: 0, y: 0, width: 800, height: 400 } });
    const frame = await request(window, 2);
    expect(frame).toEqual({ type: "render_frame", camera: { x: 24, y: 24, zoom: 1 }, width: 848, height: 448, requestId: expect.any(Number) as number });
    expect(window.contentSize).toEqual([848, 448]);

    renderer.settle(frame.requestId, { ok: true });
    const result = await shot;
    expect(result).toEqual({ ok: true, png: Buffer.from("png 848x448 at 1"), width: 848, height: 448 });
    expect(window.captures).toEqual([{ rect: { x: 0, y: 0, width: 848, height: 448 }, options: { stayHidden: true } }]);
    expect(renderer.owns(window.webContents as never)).toBe(true);
    expect(renderer.entries().map((entry) => entry.id)).toEqual([40]);
  });

  it("draws with Show all calls as the person's window shows it, the switch read at each request", async () => {
    let on = true;
    const { renderer } = setup({ held: true, showAllCalls: () => on });
    void renderer.screenshot({ board: BOARD, page: "page-1" });
    const window = await loaded();
    const image = await request(window, 1);
    expect(image).toMatchObject({ type: "render_page", purpose: "image", showAllCalls: true });
    renderer.settle(image.requestId, { ok: false, code: "empty_page", message: "Nothing to draw." });
    on = false;
    void renderer.check({ board: BOARD, page: "page-1" });
    expect(await request(window, 2)).toMatchObject({ type: "render_page", purpose: "check", showAllCalls: false });
  });

  it("reads a board no window holds from its file, with the health of its file", async () => {
    const { engine, renderer } = setup();
    engine.answers.set("read_board", () => ({ document: { title: "Orders", pages: [PAGE] } }));
    const shot = renderer.screenshot({ board: BOARD, page: "page-1", item: "create-order" });
    const window = await loaded();
    const page = await request(window, 1);
    expect(page).toMatchObject({ page: PAGE, health: HEALTH.health, item: "create-order" });
    expect(engine.calls).toEqual([
      [0, "read_board", { project: BOARD.project, id: BOARD.id }],
      [0, "board_health", { project: BOARD.project, id: BOARD.id }],
    ]);
    renderer.settle(page.requestId, { ok: true, bounds: { x: 0, y: 0, width: 100, height: 50 } });
    renderer.settle((await request(window, 2)).requestId, { ok: true });
    const result = await shot;
    expect(result.ok && result.note).toBeUndefined();
  });

  it("reads the board's file when the window closes as it reads, and says when the health can't be read", async () => {
    const { engine, bridge, renderer } = setup({ held: true });
    engine.answers.set("read_board", () => ({ document: { title: "Orders", pages: [PAGE] } }));
    engine.answers.set("board_health", () => {
      throw new EngineRequestError({ code: "engine_unavailable", message: "the Unfold engine could not be loaded" });
    });
    bridge.read.mockResolvedValueOnce({ ok: false, code: "window_closed", message: "Unfold's window closed before it answered." } as never);
    const shot = renderer.screenshot({ board: BOARD, page: "page-1" });
    const window = await loaded();
    const page = await request(window, 1);
    expect(page).toMatchObject({ page: PAGE, health: null });
    renderer.settle(page.requestId, { ok: true, bounds: { x: 0, y: 0, width: 100, height: 50 } });
    renderer.settle((await request(window, 2)).requestId, { ok: true });
    expect(await shot).toMatchObject({ ok: true, note: "No review banners: the board's health couldn't be read (the Unfold engine could not be loaded)." });
  });

  it("says when the board or the page isn't there", async () => {
    const { engine, renderer } = setup();
    engine.answers.delete("read_board");
    expect(await renderer.screenshot({ board: BOARD, page: "page-1" })).toEqual({ ok: false, code: "board_not_found", message: `The project has no board \`${BOARD.id}\`.` });
    engine.answers.set("read_board", () => ({ document: { title: "Orders", pages: [PAGE] } }));
    expect(await renderer.screenshot({ board: BOARD, page: "page-9" })).toEqual({
      ok: false,
      code: "page_not_found",
      message: "The board has no page `page-9`. Its pages: `page-1` (Placing an order).",
    });
    expect(createdWindows).toEqual([]);
  });

  it("passes on the render window's refusal", async () => {
    const { renderer } = setup({ held: true });
    const shot = renderer.screenshot({ board: BOARD, page: "page-1", item: "inner" });
    const window = await loaded();
    const refusal = { ok: false, code: "item_hidden", message: "`inner` is inside the collapsed frame `api`." };
    renderer.settle((await request(window, 1)).requestId, refusal);
    expect(await shot).toEqual(refusal);
  });

  it("scales a large page down to the cap, at scale 2 in a window of its own, and fits a window the screen kept smaller", async () => {
    const { engine, renderer } = setup({ held: true });
    FakeBrowserWindow.largest = { width: 1000, height: 1000 };
    const first = renderer.screenshot({ board: BOARD, page: "page-1" });
    const small = await loaded(0);
    renderer.settle((await request(small, 1)).requestId, { ok: true, bounds: { x: 0, y: 0, width: 400, height: 200 } });
    renderer.settle((await request(small, 2)).requestId, { ok: true });
    await first;

    const sharp = renderer.screenshot({ board: BOARD, page: "page-1", scale: 2 });
    const window = await loaded(1);
    expect(small.isDestroyed()).toBe(true);
    expect(engine.detached).toEqual([40]);
    expect(window.options.webPreferences?.["offscreen"]).toEqual({ deviceScaleFactor: 2 });
    renderer.settle((await request(window, 1)).requestId, { ok: true, bounds: { x: 0, y: 0, width: 5000, height: 1000 } });
    const frame = await request(window, 2);
    // 2,048 CSS pixels at most, but the window got only 1,000: the page fits that.
    expect(frame).toMatchObject({ camera: { zoom: 952 / 5000 }, width: 1000, height: 239 });
    renderer.settle(frame.requestId, { ok: true });
    expect(await sharp).toMatchObject({
      ok: true,
      png: Buffer.from("png 2000x478 at 2"),
      width: 2000,
      height: 478,
      note: "Drawn at 19% to fit 4,096 px on its long side; take a frame's or a card's image to read it closer.",
    });
  });

  it("runs requests one at a time in one window", async () => {
    const { renderer } = setup({ held: true });
    const first = renderer.screenshot({ board: BOARD, page: "page-1" });
    const second = renderer.screenshot({ board: BOARD, page: "page-2" });
    const window = await loaded();
    const page = await request(window, 1);
    expect(page).toMatchObject({ page: { id: "page-1" } });
    renderer.settle(page.requestId, { ok: true, bounds: { x: 0, y: 0, width: 10, height: 10 } });
    const frame = await request(window, 2);
    expect(frame.type).toBe("render_frame");
    renderer.settle(frame.requestId, { ok: true });
    await first;
    // The window is measuring again for the next page.
    expect(await request(window, 3)).toMatchObject({ type: "render_page", page: { id: "page-2" } });
    expect(window.contentSize).toEqual([1600, 1000]);
    expect(createdWindows).toHaveLength(1);
    renderer.settle((await request(window, 3)).requestId, { ok: false, code: "empty_page", message: "The page has nothing on it yet." });
    expect(await second).toMatchObject({ ok: false, code: "empty_page" });
  });

  it("gives up on a window that doesn't answer, closing it, and starts a new one for the next request", async () => {
    const { engine, renderer } = setup({ held: true, timeoutMs: 400 });
    const first = renderer.screenshot({ board: BOARD, page: "page-1" });
    const stuck = await loaded();
    expect(await first).toMatchObject({ ok: false, code: "render_timeout", message: expect.stringMatching(/^Unfold couldn't draw the page within/) as string });
    expect(stuck.isDestroyed()).toBe(true);
    expect(engine.detached).toEqual([40]);

    const second = renderer.screenshot({ board: BOARD, page: "page-1" });
    const fresh = await loaded(1);
    renderer.settle((await request(fresh, 1)).requestId, { ok: false, code: "empty_page", message: "Empty." });
    expect(await second).toMatchObject({ code: "empty_page" });
  });

  it("fails the request when the render window closes, ignores stray answers, and refuses malformed ones", async () => {
    const { renderer } = setup({ held: true });
    const closed = renderer.screenshot({ board: BOARD, page: "page-1" });
    const window = await loaded();
    await request(window, 1);
    window.emitContents("render-process-gone");
    expect(await closed).toMatchObject({ ok: false, code: "render_failed" });

    const malformed = renderer.screenshot({ board: BOARD, page: "page-1" });
    const next = await loaded(1);
    const page = await request(next, 1);
    renderer.settle(page.requestId + 100, { ok: true, bounds: { x: 0, y: 0, width: 1, height: 1 } });
    renderer.settle(page.requestId, { ok: true, bounds: { x: 0, y: 0, width: -1, height: 1 } });
    expect(await malformed).toEqual({ ok: false, code: "internal", message: "The render window answered with a malformed result." });
  });

  it("closes the render window once it has been idle", async () => {
    const { engine, renderer } = setup({ held: true, idleMs: 20 });
    const shot = renderer.screenshot({ board: BOARD, page: "page-1" });
    const window = await loaded();
    renderer.settle((await request(window, 1)).requestId, { ok: false, code: "empty_page", message: "Empty." });
    await shot;
    expect(window.isDestroyed()).toBe(false);
    await vi.waitFor(() => {
      expect(window.isDestroyed()).toBe(true);
    });
    expect(engine.detached).toEqual([40]);
    expect(renderer.entries()).toEqual([]);
  });
});

describe("check", () => {
  it("reads a board no window holds from its file, opening nothing, and returns the page's health items with its page checks", async () => {
    const { engine, renderer } = setup();
    const checked = renderer.check({ board: BOARD, page: "page-1" });
    const window = await loaded();
    const page = await request(window, 1);
    expect(page).toMatchObject({ type: "render_page", purpose: "check", page: PAGE, health: HEALTH.health });
    expect(engine.calls).toEqual([
      [0, "read_board", { project: BOARD.project, id: BOARD.id }],
      [0, "board_health", { project: BOARD.project, id: BOARD.id }],
    ]);
    renderer.settle(page.requestId, { ok: true, checks: [STRAYS] });
    expect(await checked).toEqual({ ok: true, title: "Orders", page: "page-1", pageTitle: "Placing an order", health: [OUTDATED], checks: [STRAYS] });
  });

  it("never opens a closed window: the bridge's read refuses, and the page comes from the file", async () => {
    const engine = new FakeEngine();
    const openWindow = vi.fn(() => Promise.reject(new Error("check opened the window")));
    // The app's window is closed: nothing is current, and no board is held.
    const closed = { get: () => undefined, current: () => undefined, deliver: () => undefined, focus: () => undefined };
    const bridge = new AgentBridge({ window: closed, openWindow, views: new ViewTracker() });
    const renderer = new PageRenderer({ ui: { kind: "app", dir: "/ui" }, bridge, engine, reserveId: () => 40, showAllCalls: () => false });
    const page = renderer.check({ board: BOARD, page: "page-1" });
    const board = renderer.checkBoard({ board: BOARD });
    const window = await loaded();
    renderer.settle((await request(window, 1)).requestId, { ok: true, checks: [] });
    expect(await page).toMatchObject({ ok: true, page: "page-1", health: [OUTDATED] });
    renderer.settle((await request(window, 2)).requestId, { ok: true, checks: [] });
    renderer.settle((await request(window, 3)).requestId, { ok: true, checks: [] });
    expect(await board).toMatchObject({ ok: true, pages: [{ page: "page-1", health: [OUTDATED] }, { page: "page-2", health: [ELSEWHERE] }] });
    expect(openWindow).not.toHaveBeenCalled();
  });

  it("checks every page of a board in order, each as the window that holds it has it", async () => {
    const { engine, renderer } = setup({ held: true });
    const checked = renderer.checkBoard({ board: BOARD });
    const window = await loaded();
    renderer.settle((await request(window, 1)).requestId, { ok: true, checks: [STRAYS] });
    const second = await request(window, 2);
    expect(second).toMatchObject({ purpose: "check", page: { id: "page-2" } });
    renderer.settle(second.requestId, { ok: true, checks: [] });
    expect(await checked).toEqual({
      ok: true,
      title: "Orders",
      pages: [
        { page: "page-1", pageTitle: "Page page-1", health: [OUTDATED], checks: [STRAYS] },
        { page: "page-2", pageTitle: "Page page-2", health: [ELSEWHERE], checks: [] },
      ],
    });
    // The health is read once for the board.
    expect(engine.calls.filter(([, command]) => command === "board_health")).toHaveLength(1);
  });

  it("passes on why the board couldn't be read or checked", async () => {
    const { engine, renderer } = setup();
    engine.answers.set("board_health", () => {
      throw new EngineRequestError({ code: "board_invalid", message: "The board file isn't a board." });
    });
    expect(await renderer.check({ board: BOARD, page: "page-1" })).toEqual({ ok: false, code: "board_invalid", message: "The board file isn't a board." });
    expect(await renderer.check({ board: BOARD, page: "page-9" })).toMatchObject({ ok: false, code: "page_not_found" });
    engine.answers.delete("read_board");
    expect(await renderer.checkBoard({ board: BOARD })).toMatchObject({ ok: false, code: "board_not_found" });
    expect(createdWindows).toEqual([]);
  });
});
