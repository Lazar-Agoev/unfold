/**
 * The source peek sheet (redesign §4.6, R-12): a whole file, read-only, sliding in from the right
 * over the canvas, with folds, call links, and a `⊕` beside each definition that adds it to the page.
 * A pin keeps it open and narrows the canvas instead; Escape closes it. It replaces V1's source tabs.
 */

import type { Locator, Page } from "@playwright/test";
import { addBox, addInput, addTab, openAdd, result, selectResult } from "../src/add";
import { boardView, card, cardMenu, clickCanvas } from "../src/board";
import { expect, test } from "../src/fixtures";
import { CREATE_ORDER, launchCompiled, PLACE, traceEntrance } from "../src/flows";
import { enterText } from "../src/window";

const SERVICE = "src/services/order-service.ts";

function sheet(page: Page, path: string): Locator {
  return page.getByRole("complementary", { name: `Source of ${path}`, exact: true });
}

async function canvasWidth(page: Page): Promise<number> {
  return (await boardView(page).boundingBox())?.width ?? 0;
}

test.describe("the peek sheet", () => {
  test("a file from Add: folds, ⊕ and call links add to the page, the pin, the menu, and Escape", async ({ sandbox, launch }) => {
    const { page } = await launchCompiled(launch, sandbox);
    await traceEntrance(page, "POST", "/orders");
    const fullWidth = await canvasWidth(page);
    const peek = sheet(page, SERVICE);

    await test.step("a file chosen in Add opens in the sheet, whole and read-only", async () => {
      await openAdd(page);
      await addTab(page, "Files").click();
      await enterText(addInput(page), "order-service");
      const file = result(page, `File ${SERVICE}`);
      await expect(file).toBeVisible();
      await selectResult(page, file);
      await addInput(page).press("Enter");
      await expect(addBox(page)).toBeHidden();
      await expect(peek).toBeVisible();
      await expect(peek.getByText("48 lines · TypeScript")).toBeVisible();
      await expect(peek.getByRole("document", { name: `${SERVICE}, read-only` })).toBeVisible();
      await expect(peek.getByText("adds a function to")).toBeVisible();
      // Floating, it covers the canvas without narrowing it.
      expect(await canvasWidth(page)).toBe(fullWidth);
    });

    await test.step("a definition folds and unfolds", async () => {
      const line14 = peek.locator(".peek-row[data-line='14']");
      await expect(line14).toBeVisible();
      await peek.getByRole("button", { name: "Fold lines 13–25" }).click();
      await expect(peek.getByRole("button", { name: "Unfold lines 13–25" })).toHaveAttribute("aria-expanded", "false");
      await expect(line14).toHaveCount(0);
      await peek.getByRole("button", { name: "Unfold lines 13–25" }).click();
      await expect(line14).toBeVisible();
    });

    await test.step("⊕ beside a definition adds it to the page", async () => {
      await peek.getByRole("button", { name: "Add OrderService.place to the page" }).click();
      await expect(card(page, PLACE)).toBeVisible();
      await expect(peek.getByRole("img", { name: "OrderService.place is on the page" })).toBeVisible();
    });

    await test.step("a call link in the sheet adds its callee", async () => {
      await peek.getByRole("button", { name: "Follow validateOrder", exact: true }).click();
      await expect(card(page, "src/lib/validate.ts#function:validateOrder")).toBeVisible();
    });

    await test.step("the sheet's menu offers Open in editor and Reveal in Finder", async () => {
      await peek.getByRole("button", { name: "More" }).click();
      const menu = page.getByRole("menu", { name: "File actions" });
      await expect(menu.getByRole("menuitem", { name: /^Open in editor/ })).toBeVisible();
      await expect(menu.getByRole("menuitem", { name: "Reveal in Finder" })).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(menu).toBeHidden();
      await expect(peek).toBeVisible();
    });

    await test.step("Escape in the sheet closes it", async () => {
      await peek.getByRole("document").click();
      await page.keyboard.press("Escape");
      await expect(peek).toBeHidden();
    });

    await test.step("pinned, the sheet narrows the canvas instead of covering it, and Escape still closes it", async () => {
      await openAdd(page);
      await addTab(page, "Files").click();
      await enterText(addInput(page), "order-service");
      await expect(result(page, `File ${SERVICE}`)).toBeVisible();
      await addInput(page).press("Enter");
      await expect(peek).toBeVisible();
      await peek.getByRole("button", { name: "Pin" }).click();
      await expect(peek.getByRole("button", { name: "Unpin" })).toHaveAttribute("aria-pressed", "true");
      await expect.poll(() => canvasWidth(page)).toBeLessThan(fullWidth - 400);
      await clickCanvas(page);
      await expect(peek).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(peek).toBeHidden();
      await expect.poll(() => canvasWidth(page)).toBe(fullWidth);
    });
  });

  test("a text match opens the sheet at its line; Open source on a card opens its file", async ({ sandbox, launch }) => {
    const { page } = await launchCompiled(launch, sandbox);
    await traceEntrance(page, "POST", "/orders");

    await test.step("a text match opens the sheet at its line", async () => {
      await openAdd(page);
      await enterText(addInput(page), "withRetry");
      await addTab(page, "Text").click();
      const match = result(page, /^src\/lib\/retry\.ts line 3: /);
      await expect(match).toBeVisible();
      await selectResult(page, match);
      await addInput(page).press("Enter");
      const peek = sheet(page, "src/lib/retry.ts");
      await expect(peek).toBeVisible();
      await expect(peek.locator(".peek-row.current")).toHaveAttribute("data-line", "3");
      await page.keyboard.press("Escape");
      await expect(peek).toBeHidden();
    });

    await test.step("Open source in a card's menu opens its file in the sheet", async () => {
      await cardMenu(card(page, CREATE_ORDER), "Open source");
      await expect(sheet(page, "src/routes/orders.ts")).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(sheet(page, "src/routes/orders.ts")).toBeHidden();
    });
  });
});
