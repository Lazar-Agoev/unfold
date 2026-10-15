import { describe, expect, it, vi } from "vitest";
import type { AgentPage, AgentShellEvent } from "../../../shared/agent-api";
import type { PageChecks, ScreenshotRequest } from "../../agents/render";
import { pageRevision } from "../../../shared/revision";
import type { HealthItem } from "../engine-types";
import { check, getScreenshot } from "./review";
import { harness, PROJECT } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const BOARD = { project: PROJECT, id: ".unfold/boards/orders.json" };
const BOARD_INFO = { title: "Orders", revision: "r1", unsaved: false } as const;
const PLACE = "src/services/order-service.ts#OrderService.place";
const PAGE: AgentPage = {
  id: "page-1",
  title: "Placing an order",
  defaultView: "compact",
  cards: [{ id: "place", ref: { block: PLACE }, at: [0, 0], description: { text: "Places it.", author: "person:lazar", signature: "s" } }],
  names: [],
  combined: [],
  guards: [],
  links: [],
  frames: [{ id: "shared", title: "Shared", members: ["place"], look: "box", author: "agent:claude", description: { text: "Code both services call.", author: "agent:claude", signature: "s" } }],
};

const MISSING: HealthItem = { page: "page-1", kind: "missing", target: { card: "place" }, message: `${PLACE} is not in the graph.`, repairs: ["reconnect", "remove"] };
const OUTDATED_CARD: HealthItem = { page: "page-1", kind: "outdated", target: { card: "place" }, message: "The code changed since it was described.", repairs: ["resign", "removeDescription"] };
const OUTDATED_FRAME: HealthItem = { page: "page-1", kind: "outdated", target: { frame: "shared" }, message: "A member's code changed.", repairs: ["resign", "removeDescription"] };

const CHECKS: PageChecks = {
  page: "page-1",
  pageTitle: "Placing an order",
  health: [MISSING, OUTDATED_CARD, OUTDATED_FRAME],
  checks: [{ kind: "missingWords", items: ["place"], message: "OrderService.place has no description.", suggestion: "describe it." }],
};

function window(request: AgentShellEvent): unknown {
  return request.type === "agent_read" ? { ok: true, page: { ...PAGE, id: request.page }, board: BOARD_INFO } : undefined;
}

describe("check", () => {
  it("lists a page's health with the repair calls it allows, and the page checks with their fixes", async () => {
    const asked: unknown[] = [];
    const test = harness({
      window,
      renderer: {
        check: (request) => {
          asked.push(request);
          return Promise.resolve({ ok: true, title: "Orders", ...CHECKS });
        },
      },
    });
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
    const { text } = await test.call(check);
    expect(asked).toEqual([{ board: BOARD, page: "page-1" }]);
    expect(text.split("\n")).toEqual([
      `Unfold · ts-orders › Orders › Placing an order · rev ${pageRevision(PAGE)} · as Claude`,
      `Ids: board ${BOARD.id}, page page-1.`,
      "Health (3):",
      `- missing: the card place (item place): ${PLACE} is not in the graph.`,
      `  Fix: repair({board: "${BOARD.id}", page: "page-1", item: "place", repair: "reconnect", to: <the new key>}) · repair({board: "${BOARD.id}", page: "page-1", item: "place", repair: "remove"})`,
      "- outdated: the card place (item place): The code changed since it was described.",
      `  Fix: resign is Lazar (a person)'s to do: they wrote the description of the card place · repair({board: "${BOARD.id}", page: "page-1", item: "place", repair: "remove_description"})`,
      "- outdated: the frame “Shared” (item shared): A member's code changed.",
      `  Fix: repair({board: "${BOARD.id}", page: "page-1", item: "shared", repair: "resign"}) · repair({board: "${BOARD.id}", page: "page-1", item: "shared", repair: "remove_description"})`,
      "Page checks (1):",
      "- missing words: OrderService.place has no description. (ids place)",
      "  Fix: describe it.",
      "Next: fix with the calls above, then check again and get_screenshot.",
    ]);
  });

  it("says a page with too many of an agent's words in words", async () => {
    const wordy = { kind: "wordy", items: ["n1", "n2", "n3"], message: "An agent left 3 notes on the page; about 2 is the most a page reads well with.", suggestion: "remove_items the notes that restate a card." } as const;
    const test = harness({ window, renderer: { check: () => Promise.resolve({ ok: true, title: "Orders", ...CHECKS, health: [], checks: [wordy] }) } });
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
    const { text } = await test.call(check);
    expect(text.split("\n").slice(2, 6)).toEqual([
      "Health: no problems.",
      "Page checks (1):",
      "- too many words: An agent left 3 notes on the page; about 2 is the most a page reads well with. (ids n1, n2, n3)",
      "  Fix: remove_items the notes that restate a card.",
    ]);
  });

  it("checks every page of a board named without a page", async () => {
    const test = harness({
      window,
      renderer: {
        checkBoard: () =>
          Promise.resolve({ ok: true, title: "Orders", pages: [{ page: "page-1", pageTitle: "Placing an order", health: [], checks: [] }, { ...CHECKS, page: "page-2", pageTitle: "Async path", health: [MISSING] }] }),
      },
    });
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
    const { text } = await test.call(check, { board: BOARD.id, project: "ts-orders" });
    const lines = text.split("\n");
    expect(lines.slice(0, 4)).toEqual([
      "Unfold · ts-orders › Orders · as Claude",
      `Board id ${BOARD.id}: 2 pages, 1 page with something to fix.`,
      `Page “Placing an order” (page id page-1, rev ${pageRevision(PAGE)}):`,
      "Nothing to fix.",
    ]);
    expect(lines[4]).toBe(`Page “Async path” (page id page-2, rev ${pageRevision({ ...PAGE, id: "page-2" })}):`);
    expect(lines).toContain(`  Fix: repair({board: "${BOARD.id}", page: "page-2", item: "place", repair: "reconnect", to: <the new key>}) · repair({board: "${BOARD.id}", page: "page-2", item: "place", repair: "remove"})`);

    // A link with a page checks that page only.
    const linked = harness({ window, renderer: { check: () => Promise.resolve({ ok: true, title: "Orders", page: "page-1", pageTitle: "Placing an order", health: [], checks: [] }) } });
    linked.report({ selection: { kind: "none" }, visible: [], held: [BOARD] });
    const one = await linked.call(check, { board: `unfold://ts-orders/${BOARD.id}?page=page-1` });
    expect(one.text.split("\n").slice(1)).toEqual([`Ids: board ${BOARD.id}, page page-1.`, "Nothing to fix: no health problems, and the page checks found nothing.", "Next: get_screenshot, to look at it the way the person will."]);
  });

  it("pages a whole board's check by page, with a cursor, under the size guard", async () => {
    const titles = Array.from({ length: 8 }, (_, index) => `Page ${index + 1}`);
    const many = (page: string): HealthItem[] =>
      Array.from({ length: 25 }, () => ({ ...MISSING, page, message: `${PLACE} is not in the graph: it was renamed or moved, or its file was excluded from the graph.` }));
    const asked: unknown[] = [];
    const test = harness({
      window,
      renderer: {
        checkBoard: (request) => {
          asked.push(request);
          return Promise.resolve({ ok: true, title: "Orders", pages: titles.map((title, index) => ({ ...CHECKS, page: `page-${index + 1}`, pageTitle: title, health: many(`page-${index + 1}`) })) });
        },
      },
    });
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
    const seen: string[] = [];
    let cursor: string | undefined;
    let calls = 0;
    do {
      const { text } = await test.call(check, { board: BOARD.id, project: "ts-orders", ...(cursor !== undefined ? { cursor } : {}) });
      calls += 1;
      expect(text).not.toContain("Cut here");
      expect(text.split("\n")[1]).toBe(`Board id ${BOARD.id}: 8 pages, 8 pages with something to fix.`);
      expect(text.split("\n").at(-1)).toBe("Next: fix with the calls above, then check again and get_screenshot.");
      seen.push(...[...text.matchAll(/^Page “([^”]+)”/gm)].map(([, title]) => title ?? ""));
      cursor = /More: check with board and cursor "(\d+)"/.exec(text)?.[1];
    } while (cursor !== undefined && calls < 10);
    expect(calls).toBeGreaterThan(1);
    expect(seen).toEqual(titles);
    expect(asked).toHaveLength(calls);
    expect(await test.call(check, { board: BOARD.id, project: "ts-orders", cursor: "8" })).toMatchObject({ isError: true });
    expect(await test.call(check, { page: "page-1", cursor: "1" })).toMatchObject({
      isError: true,
      text: "Can't page this check: cursor pages a whole board's check. Pass board alone, or leave cursor out.",
    });
  });

  it("teaches from the render window's refusals", async () => {
    const test = harness({ window, renderer: { check: () => Promise.resolve({ ok: false, code: "empty_page", message: "The page has nothing to draw." }) } });
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
    expect(await test.call(check)).toMatchObject({ isError: true, text: "The page has nothing to draw. add_cards puts code on it first." });
  });
});

describe("get_screenshot", () => {
  it("returns the PNG as image content after a one-line header, never moving the view", async () => {
    const asked: ScreenshotRequest[] = [];
    const test = harness({
      window,
      renderer: {
        screenshot: (request) => {
          asked.push(request);
          return Promise.resolve({ ok: true, png: Buffer.from([137, 80, 78, 71]), width: 640, height: 320, note: "Drawn at 50% to fit." });
        },
      },
    });
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
    const { result } = await test.call(getScreenshot, { item: `unfold://ts-orders/${BOARD.id}?page=page-1&item=place`, scale: 2 });
    expect(asked).toEqual([{ board: BOARD, page: "page-1", item: "place", scale: 2 }]);
    expect(result.content).toEqual([
      { type: "text", text: `Unfold · ts-orders › Orders › Placing an order · rev ${pageRevision(PAGE)} · as Claude\nThe item place on page page-1, 640×320 px. Drawn at 50% to fit.\nNext: look at it as the person will, give a one-line verdict in the chat, and fix what you see with targeted edits.` },
      { type: "image", data: Buffer.from([137, 80, 78, 71]).toString("base64"), mimeType: "image/png" },
    ]);
    expect(test.requests.map((request) => request.type)).toEqual(["agent_read"]);
    expect(test.focused).toEqual([]);
  });

  it("refuses a hidden item, saying what to do", async () => {
    const test = harness({ window, renderer: { screenshot: () => Promise.resolve({ ok: false, code: "item_hidden", message: "place is inside the collapsed frame “Shared”." }) } });
    test.report({ board: BOARD, page: "page-1", selection: { kind: "none" }, visible: [], held: [BOARD] });
    expect(await test.call(getScreenshot, { item: "place" })).toMatchObject({
      isError: true,
      text: "place is inside the collapsed frame “Shared”. set_view expands the frame that holds it, or take that frame's image.",
    });
    expect((await test.call(getScreenshot, { scale: 3 })).isError).toBe(true);
  });
});
