/**
 * Several windows on one workspace (desktop/README.md#transport-and-windows, window restore) and the engine host
 * restarting (§11.11): New Window; two windows, each with its own board, both updating after a save; a
 * board held by another window focuses that window; Open in new window (from a tab and from the board
 * menu) keeps the board's undo history; windows and tabs come back after quitting; and a killed engine host reconnects
 * every window without losing an edit.
 */

import { join } from "node:path";
import type { Page } from "@playwright/test";
import { addInput, openAdd, search } from "../src/add";
import { activeBoardRef, boardMenu, card, chooseLens, clickCanvas, expectCardsLoaded, pagesList, readBoard, selected } from "../src/board";
import { expect, test } from "../src/fixtures";
import { launchCompiled, openPlace, PLACE, traceEntrance } from "../src/flows";
import { boardTab, expectGraphCurrent, homeTab, homeTitle, projectRow, showHome, sidebarBoard, tabs, toast } from "../src/home";
import { editFile } from "../src/sandbox";
import { focusedWindows, spyOnFocus, windowWithKey } from "../src/session";
import { enterText, expectConnection, invoke, windowInfo } from "../src/window";

/** Adds `OrderService.place` on a new draft from Home's Add (Enter makes the draft) and reads it in Code. */
async function placeOnNewDraft(page: Page): Promise<void> {
  const before = await tabs(page).count();
  await openAdd(page);
  await search(page, "OrderService.place");
  await expect(addInput(page)).toHaveAttribute("aria-activedescendant", /.+/);
  await addInput(page).press("Enter");
  await expect(tabs(page)).toHaveCount(before + 1);
  await expect(card(page, PLACE)).toBeVisible();
  await chooseLens(page, "Code");
  await expect(card(page, PLACE).locator(".code-view")).toBeVisible();
}

/** The window's tab titles, Home first (without the project monograms). */
function tabLabels(page: Page) {
  return tabs(page).locator(".tab-label");
}

/** The number of pages the active board has. */
function pages(page: Page) {
  return pagesList(page).getByRole("option");
}

test.describe("several windows", () => {
  test("New Window; two windows with their own boards both update; a held board focuses its window", async ({ sandbox, launch }) => {
    const { app, page: first, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await openPlace(first);

    const second = await test.step("⇧⌘N opens another window on the same workspace and project", async () => {
      const opened = await app.nextWindow(() => first.keyboard.press("Meta+Shift+N"));
      await expectConnection(opened, "connected");
      await expect(projectRow(opened, "ts-orders")).toBeVisible();
      await expect(homeTitle(opened)).toHaveText("ts-orders");
      await expect(homeTab(opened)).toHaveAttribute("aria-selected", "true");
      return opened;
    });

    await test.step("each window opens its own board", async () => {
      await placeOnNewDraft(second);
      expect((await activeBoardRef(second)).id).not.toBe((await activeBoardRef(first)).id);
    });

    await test.step("a save updates the card in both windows", async () => {
      editFile(join(folder, "src/services/order-service.ts"), "this.payments.charge(draft);", "this.payments.charge(draft); // seen by both windows");
      await expect(card(first, PLACE)).toContainText("// seen by both windows", { timeout: 30_000 });
      await expect(card(second, PLACE)).toContainText("// seen by both windows", { timeout: 30_000 });
    });

    await test.step("opening a board another window holds focuses that window and its tab", async () => {
      await spyOnFocus(app);
      await showHome(first);
      await expect(boardTab(first, "POST /orders")).toHaveAttribute("aria-selected", "false");
      await showHome(second);
      await sidebarBoard(second, "POST /orders").click();
      const firstId = await app.windowId(first);
      await expect.poll(() => focusedWindows(app)).toContain(firstId);
      await expect(boardTab(first, "POST /orders")).toHaveAttribute("aria-selected", "true");
      // The board stays where it was: the second window opened no tab for it.
      await expect(boardTab(second, "POST /orders")).toHaveCount(0);
    });
  });

  test("Open in new window, from a tab and from the board menu, keeps the board's undo history", async ({ sandbox, launch }) => {
    const { app, page: first } = await launchCompiled(launch, sandbox);
    await traceEntrance(first, "POST", "/orders");
    const board = await activeBoardRef(first);

    await test.step("an edit in the first window: a new page", async () => {
      await first.getByRole("complementary", { name: "Sidebar" }).getByRole("button", { name: "New page" }).click();
      await expect(pages(first)).toHaveCount(2);
    });

    const second = await test.step("the tab's Open in new window moves the board to a new window", async () => {
      await boardTab(first, "POST /orders").click({ button: "right" });
      const menu = first.getByRole("menu", { name: "Tab" });
      const opened = await app.nextWindow(() => menu.getByRole("menuitem", { name: "Open in new window" }).click());
      await expect(boardTab(opened, "POST /orders")).toHaveAttribute("aria-selected", "true");
      await expect(boardTab(first, "POST /orders")).toHaveCount(0);
      expect(await activeBoardRef(opened)).toEqual(board);
      await expect(pages(opened)).toHaveCount(2);
      return opened;
    });

    await test.step("⌘Z in the new window undoes the edit made in the first", async () => {
      await clickCanvas(second);
      await second.keyboard.press("Meta+Z");
      await expect(pages(second)).toHaveCount(1);
      await second.keyboard.press("Meta+Shift+Z");
      await expect(pages(second)).toHaveCount(2);
    });

    await test.step("the board menu's Open in new window moves it on, with its history", async () => {
      await second.getByRole("complementary", { name: "Sidebar" }).getByRole("button", { name: "New page" }).click();
      await expect(pages(second)).toHaveCount(3);
      const third = await app.nextWindow(() => boardMenu(second, "Open in new window"));
      await expect(boardTab(third, "POST /orders")).toHaveAttribute("aria-selected", "true");
      await expect(boardTab(second, "POST /orders")).toHaveCount(0);
      await expect(pages(third)).toHaveCount(3);
      await clickCanvas(third);
      await third.keyboard.press("Meta+Z");
      await expect(pages(third)).toHaveCount(2);
      await third.keyboard.press("Meta+Z");
      await expect(pages(third)).toHaveCount(1);
    });
  });

  test("quitting with two windows and relaunching restores both, with their tabs", async ({ sandbox, launch }) => {
    const { app, page: first } = await launchCompiled(launch, sandbox);
    await traceEntrance(first, "POST", "/orders");
    await showHome(first);
    await traceEntrance(first, "GET", "/health");
    await boardTab(first, "POST /orders").click();
    const second = await app.nextWindow(() => first.keyboard.press("Meta+Shift+N"));
    await expect(homeTitle(second)).toHaveText("ts-orders");
    await traceEntrance(second, "DELETE", "/orders/:id");
    const keys = [(await windowInfo(first)).key, (await windowInfo(second)).key];
    // The windows save their state 400 ms after a change, and at once as they close.
    await app.quit();

    const relaunched = await launch();
    await relaunched.firstWindow();

    await test.step("both windows come back", async () => {
      await expect.poll(() => relaunched.windows().length).toBe(2);
      const restoredKeys = await Promise.all(relaunched.windows().map(async (window) => (await windowInfo(window)).key));
      expect(restoredKeys.sort()).toEqual([...keys].sort());
    });

    await test.step("each window's tabs and active tab come back, and its boards reopen", async () => {
      const restoredFirst = await windowWithKey(relaunched, keys[0] ?? "");
      const restoredSecond = await windowWithKey(relaunched, keys[1] ?? "");
      await expect(tabLabels(restoredFirst)).toHaveText(["Home", "POST /orders", "GET /health"]);
      await expect(boardTab(restoredFirst, "POST /orders")).toHaveAttribute("aria-selected", "true");
      await expectCardsLoaded(restoredFirst);
      await expect(restoredFirst.locator(".board-view .entrance-card")).toBeVisible();
      await expect(tabLabels(restoredSecond)).toHaveText(["Home", "DELETE /orders/:id"]);
      await expect(boardTab(restoredSecond, "DELETE /orders/:id")).toHaveAttribute("aria-selected", "true");
      await expect(restoredSecond.locator(".board-view .entrance-card")).toBeVisible();
    });
  });

  test("a killed engine host reconnects every window without losing an edit", async ({ sandbox, launch }) => {
    const { app, page: first, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await openPlace(first);
    const second = await app.nextWindow(() => first.keyboard.press("Meta+Shift+N"));
    await expect(homeTitle(second)).toHaveText("ts-orders");
    const { id } = await activeBoardRef(first);

    await test.step("an edit is waiting to be saved when the engine host dies", async () => {
      await card(first, PLACE).locator(".card-header").click();
      await enterText(selected(first).getByRole("textbox", { name: "Description" }), "Written as the engine went away.");
      const pid = await app.killEngineHost();
      await expect(toast(first, "Reconnecting to the engine…")).toBeVisible();
      await expect.poll(() => app.engineHostPid()).not.toBe(pid);
    });

    await test.step("every window reconnects; the toast goes and the board stays open", async () => {
      for (const window of [first, second]) {
        await expectConnection(window, "connected");
        await expect(toast(window, "Reconnecting to the engine…")).toHaveCount(0, { timeout: 30_000 });
      }
      await expect(boardTab(first, "POST /orders")).toHaveAttribute("aria-selected", "true");
      await expect(card(first, PLACE).locator(".code-view")).toBeVisible();
      await expectGraphCurrent(second);
    });

    await test.step("the edit is saved after the reconnect, and the engine answers again", async () => {
      await expect
        .poll(() => readBoard(folder, id).pages[0]?.cards.find((entry) => entry.ref.block?.endsWith(PLACE))?.description?.text)
        .toBe("Written as the engine went away.");
      const project = (await activeBoardRef(first)).project;
      await expect.poll(async () => (await invoke<{ boards: { id: string; heldBy: number | null }[] }>(second, "list_boards", { project })).boards.find((board) => board.id === id)?.heldBy).toBe((await windowInfo(first)).windowId);
    });
  });
});
