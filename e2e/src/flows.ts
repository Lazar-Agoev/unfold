/** Multi-step flows several journeys start with. */

import type { Page } from "@playwright/test";
import { expect } from "@playwright/test";
import type { UnfoldApp } from "./app";
import { card, expectCardsLoaded, expectGraphCurrent, firstCompile, follow, openEntrance, openFromWelcome } from "./window";

export const CREATE_ORDER = "#function:createOrder";
export const PLACE = "#class:OrderService/method:place";

/** Opens `folder` from the welcome screen and compiles it for the first time. */
export async function openAndCompile(app: UnfoldApp, page: Page, folder: string): Promise<void> {
  await openFromWelcome(app, page, folder);
  await firstCompile(page);
}

/** On a compiled `ts-orders`: opens `POST /orders` and follows `createOrder` into `OrderService.place`. */
export async function openPlace(page: Page): Promise<void> {
  await openEntrance(page, "POST", "/orders");
  await expect(card(page, CREATE_ORDER)).toBeVisible();
  await expectCardsLoaded(page);
  await follow(card(page, CREATE_ORDER), "place");
  await expect(card(page, PLACE).locator(".code-view")).toBeVisible();
}

/**
 * After a relaunch: the folder's windows come back by themselves; if the first window shows the
 * welcome screen instead, opens `folder` there. Waits for a current graph.
 */
export async function reopened(app: UnfoldApp, page: Page, folder: string): Promise<void> {
  await expect(page.locator(".workbench, .welcome, .first-compile").first()).toBeVisible();
  if (await page.locator(".welcome").isVisible()) {
    await openFromWelcome(app, page, folder);
  }
  await expectGraphCurrent(page);
}
