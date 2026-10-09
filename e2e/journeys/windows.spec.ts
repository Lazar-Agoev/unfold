/**
 * Migration plan Step 10, the window journeys (U-37): two windows on one folder each with its own
 * board, both updating after a save; opening a board held by the other window focuses it; Move to
 * New Window keeps the undo history; three windows come back after quitting; and stopping the
 * engine host reconnects every window without losing an edit.
 */

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import type { UnfoldApp } from "../src/app";
import { expect, test } from "../src/fixtures";
import { openAndCompile, openPlace, PLACE, reopened } from "../src/flows";
import { editFile } from "../src/sandbox";
import {
  activeBoard,
  activeBoardId,
  card,
  cards,
  editorTabs,
  expectConnection,
  expectGraphCurrent,
  graphStatus,
  showView,
  windowInfo,
} from "../src/window";

interface BoardFile {
  readonly cards: { readonly id: string; readonly folded?: boolean }[];
}

function readBoard(folder: string, boardId: string): BoardFile {
  return JSON.parse(readFileSync(join(folder, boardId), "utf8")) as BoardFile;
}

/** Records which windows the main process focuses (`BrowserWindow#focus`). */
async function spyOnFocus(app: UnfoldApp): Promise<void> {
  await app.electron.evaluate(({ BrowserWindow }) => {
    const store = globalThis as unknown as { __focused?: number[] };
    if (store.__focused) {
      return;
    }
    const focused: number[] = [];
    store.__focused = focused;
    const prototype = BrowserWindow.prototype as unknown as { focus: (this: Electron.BrowserWindow) => void };
    const original = prototype.focus;
    prototype.focus = function focus(this: Electron.BrowserWindow) {
      focused.push(this.id);
      original.call(this);
    };
  });
}

async function focusedWindows(app: UnfoldApp): Promise<number[]> {
  return app.electron.evaluate(() => (globalThis as unknown as { __focused?: number[] }).__focused ?? []);
}

/** The window whose key is `key`. */
async function findWindow(pages: readonly Page[], key: string | undefined): Promise<Page> {
  for (const page of pages) {
    if ((await windowInfo(page)).key === key) {
      return page;
    }
  }
  throw new Error(`no window has key ${String(key)}`);
}

/** Opens `OrderService.place` from Search, on the window's own board. */
async function openPlaceFromSearch(page: Page): Promise<void> {
  await showView(page, "Search");
  await page.locator(".search-view").getByRole("searchbox").fill("OrderService.place");
  await page.getByRole("listbox", { name: "Search results" }).getByRole("option").filter({ hasText: "place" }).first().click();
  await expect(card(page, PLACE).locator(".code-view")).toBeVisible();
}

test.describe("several windows", () => {
  test("two windows, each with its own board, both update after a save; a held board focuses its window", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    const app = await launch();
    const first = await app.firstWindow();
    await openAndCompile(app, first, folder);
    await openPlace(first);
    const firstBoard = await activeBoardId(first);

    const second = await app.nextWindow(() => first.keyboard.press("Meta+Shift+N"));
    await expectConnection(second, "connected");
    await expectGraphCurrent(second);
    await openPlaceFromSearch(second);
    const secondBoard = await activeBoardId(second);
    expect(secondBoard).not.toBe(firstBoard);

    editFile(join(folder, "src/services/order-service.ts"), "this.repo.insert(order);", "this.repo.insert(order); // saved once");
    await expect(card(first, PLACE)).toContainText("// saved once");
    await expect(card(second, PLACE)).toContainText("// saved once");
    await expect(graphStatus(first)).toHaveText(await graphStatus(second).textContent() ?? "");

    // The second window opens the first window's board: it stays in the first window, which is focused.
    await spyOnFocus(app);
    const firstId = (await windowInfo(first)).windowId;
    await showView(second, "Boards");
    const tabsBefore = await editorTabs(second).count();
    await second.locator(".board-row", { hasText: "POST /orders" }).click();
    await expect.poll(() => focusedWindows(app)).toContain(firstId);
    expect(await editorTabs(second).count()).toBe(tabsBefore);
    await expect(first.locator(`.board-tab[data-board-id="${firstBoard}"]`)).toHaveCount(1);
  });

  test("Move to New Window keeps the board's undo history", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    const app = await launch();
    const first = await app.firstWindow();
    await openAndCompile(app, first, folder);
    await openPlace(first);
    const boardId = await activeBoardId(first);
    const count = await cards(first).count();

    // An undoable edit before the move: fold the place card.
    await card(first, PLACE).locator(".card-fold").click();
    await expect(card(first, PLACE)).toHaveClass(/\bfolded\b/);

    const boardTab = editorTabs(first).filter({ hasText: "POST /orders" });
    await boardTab.click({ button: "right" });
    const moved = await app.nextWindow(() => first.getByRole("menuitem", { name: "Move to New Window" }).click());
    await expect(moved.locator(`.board-tab[data-board-id="${boardId}"]`)).toBeVisible();
    await expect(first.locator(`.board-tab[data-board-id="${boardId}"]`)).toHaveCount(0);
    await expect(cards(moved)).toHaveCount(count);
    await expect(card(moved, PLACE)).toHaveClass(/\bfolded\b/);

    // Undo in the new window reverts the edit made in the old one.
    await activeBoard(moved).locator(".react-flow__pane").click({ position: { x: 12, y: 12 } });
    await moved.keyboard.press("Meta+Z");
    await expect(card(moved, PLACE)).not.toHaveClass(/\bfolded\b/);
    await expect.poll(() => readBoard(folder, boardId).cards.some((entry) => entry.folded === true)).toBe(false);
    await moved.keyboard.press("Meta+Shift+Z");
    await expect(card(moved, PLACE)).toHaveClass(/\bfolded\b/);
  });

  test("quitting with three windows open and relaunching restores all three", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    let app = await launch();
    const first = await app.firstWindow();
    await openAndCompile(app, first, folder);
    await openPlace(first);
    const second = await app.nextWindow(() => first.keyboard.press("Meta+Shift+N"));
    await expectGraphCurrent(second);
    const third = await app.nextWindow(() => second.keyboard.press("Meta+Shift+N"));
    await expectGraphCurrent(third);
    const keys = await Promise.all([first, second, third].map(async (page) => (await windowInfo(page)).key));
    expect(new Set(keys).size).toBe(3);
    // Each window saves its bounds and its state when its workspace opens.
    await expect.poll(() => existsSync(join(folder, ".unfold/local/state.json")) && Object.keys((JSON.parse(readFileSync(join(folder, ".unfold/local/state.json"), "utf8")) as { windows?: Record<string, unknown> }).windows ?? {}).length).toBe(3);
    // Resized right before quitting, inside the bounds-save debounce: the save is flushed on close.
    const resizedKey = keys[2];
    await app.resize(third, 1100, 760);
    await app.quit();

    app = await launch();
    const restored = await app.firstWindow();
    await reopened(app, restored, folder);
    await expect.poll(() => app.windows().length).toBe(3);
    const restoredKeys = await Promise.all(app.windows().map(async (page) => (await windowInfo(page)).key));
    expect(restoredKeys.sort()).toEqual([...keys].sort());
    const resized = await findWindow(app.windows(), resizedKey);
    const [width = 0, height = 0] = await app.windowSize(resized);
    expect(Math.abs(width - 1100)).toBeLessThanOrEqual(4);
    expect(Math.abs(height - 760)).toBeLessThanOrEqual(4);
    for (const page of app.windows()) {
      await expect(page.locator(".workbench")).toBeVisible();
    }
    const withBoard = await Promise.all(app.windows().map((page) => page.locator(".board-tab").count()));
    expect(withBoard.reduce((sum, value) => sum + value, 0)).toBeGreaterThanOrEqual(1);
  });

  test("stopping the engine host reconnects every window without losing an edit", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    const app = await launch();
    const first = await app.firstWindow();
    await openAndCompile(app, first, folder);
    await openPlace(first);
    const boardId = await activeBoardId(first);
    const second = await app.nextWindow(() => first.keyboard.press("Meta+Shift+N"));
    await expectGraphCurrent(second);
    await expect.poll(() => existsSync(join(folder, boardId)) && readBoard(folder, boardId).cards.length === 5).toBe(true);

    // An edit, then the engine host dies before the autosave can reach it.
    const placeId = (await card(first, PLACE).getAttribute("data-card-id")) ?? "";
    await card(first, PLACE).locator(".card-fold").click();
    const killed = await app.killEngineHost();
    const stopped = Date.now();
    await expect(first.locator("[data-connection='reconnecting']")).toBeVisible();
    await expectConnection(first, "connected");
    await expectConnection(second, "connected");
    await expectGraphCurrent(first);
    await expectGraphCurrent(second);
    test.info().annotations.push({ type: "engine-host restart until both windows are usable", description: `${Date.now() - stopped} ms` });
    expect(await app.engineHostPid()).not.toBe(killed);

    // The edit made before the restart is saved after it, and both windows keep working.
    await expect(card(first, PLACE)).toHaveClass(/\bfolded\b/);
    await expect.poll(() => readBoard(folder, boardId).cards.find((entry) => entry.id === placeId)?.folded ?? false).toBe(true);
    await expect(first.locator(".board-banner")).toHaveCount(0);
    editFile(join(folder, "src/services/order-service.ts"), "this.repo.insert(order);", "this.repo.insert(order); // after the restart");
    await openPlaceFromSearch(second);
    await expect(card(second, PLACE)).toContainText("// after the restart");
  });
});
