/** Pointer work on the canvas: clicks that wait for the camera to hold still. */

import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { cameraSettled, panUntilHittable } from "./board";

/**
 * Clicks something on the canvas once the camera holds still: a focus move (Show callers, a follow)
 * animates the camera a moment after it is asked for, so a pan and click is retried until the click
 * lands on a still target.
 */
export async function clickOnCanvas(page: Page, target: Locator, options: { readonly modifiers?: ("Alt" | "Meta" | "Shift")[]; readonly button?: "right" } = {}): Promise<void> {
  await expect(async () => {
    await cameraSettled(page);
    await panUntilHittable(page, target);
    await target.click({ timeout: 2_000, ...options });
  }).toPass();
}

/** Opens a card's context menu once the camera holds still, and chooses `item`. */
export async function chooseOnCard(page: Page, card: Locator, item: string | RegExp): Promise<void> {
  const menu = page.getByRole("menu", { name: "Card actions" });
  await clickOnCanvas(page, card.locator(".card-header .card-title"), { button: "right" });
  await menu.getByRole("menuitem", { name: item }).click();
  await expect(menu).toBeHidden();
}
