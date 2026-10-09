/**
 * Migration plan Step 10, the keyboard-only journey: open a folder from the recent list, Compile,
 * open an entrance, follow a call, and switch the lens, without the mouse.
 */

import type { Page } from "@playwright/test";
import { expect, test } from "../src/fixtures";
import { CREATE_ORDER, openAndCompile, PLACE } from "../src/flows";
import { activeBoard, card, expectConnection, expectGraphCurrent, invoke, windowCommand } from "../src/window";

/** Presses `key` until the focused element matches `selector` (at most `limit` presses). */
async function pressUntilFocused(page: Page, key: string, selector: string, limit = 400): Promise<void> {
  for (let presses = 0; presses < limit; presses++) {
    if (await page.evaluate((wanted) => document.activeElement?.matches(wanted) === true, selector)) {
      return;
    }
    await page.keyboard.press(key);
  }
  throw new Error(`${key} never reached ${selector}`);
}

/** The text of a tree's active row (it follows `aria-activedescendant`), and whether it is collapsed. */
function activeRow(page: Page, tree: string): Promise<{ text: string; collapsed: boolean } | null> {
  return page.evaluate((selector) => {
    const id = document.querySelector(selector)?.getAttribute("aria-activedescendant");
    const row = id ? document.getElementById(id) : null;
    return row ? { text: row.textContent, collapsed: row.getAttribute("aria-expanded") === "false" } : null;
  }, tree);
}

test("a keyboard-only journey: recent folder, Compile, an entrance, a call, the lens", async ({ sandbox, launch }) => {
  const folder = sandbox.playground("ts-orders");

  // Setup (not part of the journey): compile once, then quit with only a welcome window open, so
  // the next launch starts on the welcome screen with the folder in Recent.
  let app = await launch();
  let page = await app.firstWindow();
  await openAndCompile(app, page, folder);
  const welcome = await app.nextWindow(() => windowCommand(page, "new_window", { folder: "none" }));
  await expect(welcome.locator(".welcome-recent-open")).toBeVisible();
  await windowCommand(page, "close_window").catch(() => undefined);
  await expect.poll(() => app.windows().length).toBe(1);
  await app.quit();

  app = await launch();
  page = await app.firstWindow();
  await expect(page.locator(".welcome-recent-open")).toBeVisible();
  await expectConnection(page, "connected");

  // Open the folder from Recent.
  await pressUntilFocused(page, "Tab", ".welcome-recent-open", 40);
  await page.keyboard.press("Enter");
  await expect(page.locator(".workbench")).toBeVisible();
  await expectGraphCurrent(page);

  // Compile with ⇧⌘B.
  const before = (await invoke<{ entries: unknown[] }>(page, "index_log")).entries.length;
  await page.keyboard.press("Meta+Shift+B");
  await expect.poll(async () => (await invoke<{ entries: { action: string }[] }>(page, "index_log")).entries.slice(before).map((entry) => entry.action)).toContain("Compile");
  await expectGraphCurrent(page);

  // ⌘2, then walk the Entrances tree to POST /orders and open it.
  await page.keyboard.press("Meta+2");
  const tree = '[role="tree"][aria-label="Entrances"]';
  await expect(page.locator(tree)).toBeVisible();
  await pressUntilFocused(page, "Tab", tree, 60);
  let opened = false;
  for (let step = 0; step < 120 && !opened; step++) {
    const row = await activeRow(page, tree);
    if (row && row.text.includes("POST") && /\/orders(?!\/)/.test(row.text)) {
      await page.keyboard.press("Enter");
      opened = true;
    } else {
      await page.keyboard.press(row?.collapsed ? "ArrowRight" : "ArrowDown");
    }
  }
  expect(opened, "reached POST /orders in the Entrances tree").toBe(true);
  await expect(card(page, CREATE_ORDER)).toBeVisible();
  await expect(activeBoard(page).locator(".board-card[data-loading]")).toHaveCount(0);

  // Follow createOrder's call to place: Tab to its link, Enter.
  await pressUntilFocused(page, "Tab", `.board-card[data-key$="${CREATE_ORDER}"] .code-link.resolved[aria-label$=".place"]`);
  await page.keyboard.press("Enter");
  await expect(card(page, PLACE).locator(".code-view")).toBeVisible();

  // Switch the lens to Flow: focus the title bar's lens, then an arrow key.
  await pressUntilFocused(page, "Shift+Tab", '[role="radiogroup"][aria-label="Lens"] [role="radio"]');
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("radiogroup", { name: "Lens" }).getByRole("radio", { name: "Flow" })).toHaveAttribute("aria-checked", "true");
  const blockCards = activeBoard(page).locator(".block-card");
  await expect(activeBoard(page).locator(".block-card .flow-view")).toHaveCount(await blockCards.count());
});
