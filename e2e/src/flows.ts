/** Multi-step flows several journeys start with. */

import type { Page } from "@playwright/test";
import { expect } from "@playwright/test";
import type { UnfoldApp } from "./app";
import { boardView, card, expectCardsLoaded, follow } from "./board";
import type { Fixtures } from "./fixtures";
import { addProjectFromSidebar, entranceRow, firstCompile, homeTitle, projectRow, showEntrances, tabs } from "./home";
import type { PlaygroundName, Sandbox } from "./sandbox";

export const CREATE_ORDER = "#function:createOrder";
export const PLACE = "#class:OrderService/method:place";
/** The end of `POST /orders`'s entrance key in `ts-orders`. */
export const POST_ORDERS = `"POST","/orders","requireUser"]`;

interface Compiled {
  readonly app: UnfoldApp;
  readonly page: Page;
  /** The sandbox copies of the playgrounds, in the order given. */
  readonly folders: readonly string[];
}

/**
 * Launches the app with copies of `playgrounds` connected and compiled: the first is passed on the
 * command line, the others are added with Add Project…. Home ends on the last one.
 */
export async function launchCompiled(
  launch: Fixtures["launch"],
  sandbox: Sandbox,
  playgrounds: readonly PlaygroundName[] = ["ts-orders"],
): Promise<Compiled> {
  const folders = playgrounds.map((name) => sandbox.playground(name));
  const first = folders[0];
  if (first === undefined) {
    throw new Error("launchCompiled needs a playground");
  }
  const app = await launch({ args: [first] });
  const page = await app.firstWindow();
  for (const [index, folder] of folders.entries()) {
    const name = playgrounds[index] ?? "";
    if (folder !== first) {
      await addProjectFromSidebar(app, page, folder);
    }
    await expect(projectRow(page, name)).toBeVisible();
    await expect(homeTitle(page)).toHaveText(name);
    await firstCompile(page);
  }
  return { app, page, folders };
}

/** Traces an entrance from Home's How it starts onto a new draft board and waits for its cards. */
export async function traceEntrance(page: Page, kind: string, trigger: string): Promise<void> {
  const before = await tabs(page).count();
  await showEntrances(page);
  await entranceRow(page, kind, trigger).click();
  await expect(tabs(page)).toHaveCount(before + 1);
  await expect(boardView(page).locator(".entrance-card")).toBeVisible();
  await expectCardsLoaded(page);
}

/** On a compiled `ts-orders`: traces `POST /orders` and follows `orderService.place` from `createOrder`. */
export async function openPlace(page: Page): Promise<void> {
  await traceEntrance(page, "POST", "/orders");
  await follow(card(page, CREATE_ORDER), "orderService.place");
  await expect(card(page, PLACE).locator(".code-view")).toBeVisible();
}
