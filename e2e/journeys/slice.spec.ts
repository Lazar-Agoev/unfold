/**
 * Migration plan Step 2, the first vertical slice: open `ts-orders`, Compile, open `POST /orders`,
 * follow `createOrder` into `OrderService.place`, open a second window on the folder, save a body
 * edit and watch the open card update in both windows, then quit, relaunch, and find the draft.
 */

import { join } from "node:path";
import { editFile } from "../src/sandbox";
import { expect, test } from "../src/fixtures";
import { CREATE_ORDER, PLACE } from "../src/flows";
import {
  activeBoardId,
  card,
  cards,
  expectCardsLoaded,
  expectConnection,
  expectGraphCurrent,
  firstCompile,
  follow,
  openEntrance,
  openFromWelcome,
  whenTextShows,
} from "../src/window";


test("the first vertical slice: compile, follow, two windows, a save, and a relaunch", async ({ sandbox, launch }) => {
  const folder = sandbox.playground("ts-orders");
  let app = await launch();
  const first = await app.firstWindow();
  await openFromWelcome(app, first, folder);
  await firstCompile(first);

  await openEntrance(first, "POST", "/orders");
  await expect(first.locator(".entrance-card")).toBeVisible();
  await expect(card(first, CREATE_ORDER)).toBeVisible();
  await expectCardsLoaded(first);

  await follow(card(first, CREATE_ORDER), "place");
  const place = card(first, PLACE);
  await expect(place).toBeVisible();
  await expect(place.locator(".code-row").first()).toBeVisible();
  await expect(place).toContainText("this.payments.charge(draft);");
  await expect(first.locator(".board-trail li")).toHaveCount(3);
  const boardId = await activeBoardId(first);
  const keysBefore = (await cards(first).evaluateAll((elements) => elements.map((element) => element.getAttribute("data-key")))).sort();

  // ⇧⌘N: a second window on the same folder. It shares the workspace and opens its own board.
  const second = await app.nextWindow(() => first.keyboard.press("Meta+Shift+N"));
  await expect(second.locator(".workbench")).toBeVisible();
  await expectConnection(second, "connected");
  await expectGraphCurrent(second);
  await openEntrance(second, "POST", "/orders");
  await expect(card(second, CREATE_ORDER)).toBeVisible();
  await follow(card(second, CREATE_ORDER), "place");
  await expect(card(second, PLACE)).toContainText("this.payments.charge(draft);");

  // A body edit to order-service.ts, saved on disk: the open card updates in both windows.
  const placeCode = `.editor-board:not([hidden]) .board-card[data-key$="${PLACE}"] .code-view`;
  const shown = Promise.all([whenTextShows(first, placeCode, "// charged once"), whenTextShows(second, placeCode, "// charged once")]);
  const saved = Date.now();
  editFile(join(folder, "src/services/order-service.ts"), "this.payments.charge(draft);", "this.payments.charge(draft); // charged once");
  const elapsed = Math.max(...(await shown)) - saved;
  await expect(card(first, PLACE)).toContainText("// charged once");
  await expect(card(second, PLACE)).toContainText("// charged once");
  test.info().annotations.push({ type: "save → updated card (both windows)", description: `${elapsed} ms` });
  // The budget is 1 s (Step 2, architecture §17), ratified on reference machines; shared CI runners
  // get twice that so a slow runner doesn't fail the build, and the measurement is reported.
  expect(elapsed, "save to updated card in both windows (target < 1 s)").toBeLessThan(process.env["CI"] ? 2_000 : 1_000);
  await expectGraphCurrent(first);

  // Quit with both windows open, relaunch: the folder's windows come back with the draft.
  await app.quit();
  app = await launch();
  const restored = await app.firstWindow();
  await expect(restored.locator(".welcome, .workbench").first()).toBeVisible();
  if (await restored.locator(".welcome").isVisible()) {
    await openFromWelcome(app, restored, folder);
  }
  await expectGraphCurrent(restored);
  await expect.poll(() => app.windows().length).toBe(2);
  const holder = await (async () => {
    for (const page of app.windows()) {
      await expect(page.locator(".workbench")).toBeVisible();
      if ((await page.locator(`.board-tab[data-board-id="${boardId}"]`).count()) > 0) {
        return page;
      }
    }
    return null;
  })();
  expect(holder, `a restored window shows the draft ${boardId}`).not.toBeNull();
  if (holder) {
    await holder.locator(".editor").getByRole("tab").first().waitFor();
    await expect(card(holder, PLACE)).toContainText("// charged once");
    const keysAfter = (await cards(holder).evaluateAll((elements) => elements.map((element) => element.getAttribute("data-key")))).sort();
    expect(keysAfter).toEqual(keysBefore);
  }
});
