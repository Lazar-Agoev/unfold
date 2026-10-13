/** Multi-step flows several journeys start with. */

import { basename } from "node:path";
import type { Page } from "@playwright/test";
import { expect } from "@playwright/test";
import type { UnfoldApp } from "./app";
import { boardView, card, chooseLens, expectCardsLoaded, follow } from "./board";
import type { Fixtures } from "./fixtures";
import { addProjectFromSidebar, entranceRow, expectGraphCurrent, homeTitle, projectRow, showEntrances } from "./home";
import type { PlaygroundName, Sandbox } from "./sandbox";

export const CREATE_ORDER = "#function:createOrder";
export const PLACE = "#class:OrderService/method:place";
/** The end of `POST /orders`'s entrance key in `ts-orders`. */
export const POST_ORDERS = `"POST","/orders","requireUser"]`;

interface Launched {
  readonly app: UnfoldApp;
  readonly page: Page;
}

interface Compiled extends Launched {
  /** The sandbox copies of the playgrounds, in the order given. */
  readonly folders: readonly string[];
}

/**
 * Launches the app on an empty workspace and adds `folder` with Add Project…; Home shows it while its
 * first compile runs.
 */
export async function launchWithProject(launch: Fixtures["launch"], folder: string): Promise<Launched> {
  const app = await launch();
  const page = await app.firstWindow();
  await addProjectFromSidebar(app, page, folder);
  const name = basename(folder);
  await expect(projectRow(page, name)).toBeVisible();
  await expect(homeTitle(page)).toHaveText(name);
  return { app, page };
}

/**
 * Launches the app with copies of `playgrounds` added with Add Project… and compiled, in order. Home
 * ends on the last one.
 */
export async function launchCompiled(
  launch: Fixtures["launch"],
  sandbox: Sandbox,
  playgrounds: readonly PlaygroundName[] = ["ts-orders"],
): Promise<Compiled> {
  const app = await launch();
  const page = await app.firstWindow();
  const folders = playgrounds.map((name) => sandbox.playground(name));
  for (const [index, folder] of folders.entries()) {
    const name = playgrounds[index] ?? "";
    await addProjectFromSidebar(app, page, folder);
    await expect(projectRow(page, name)).toBeVisible();
    await expect(homeTitle(page)).toHaveText(name);
    await expectGraphCurrent(page);
  }
  return { app, page, folders };
}

/**
 * Traces an entrance from Home's How it starts onto a new draft board, which the window shows, and
 * waits for its cards. The new page opens in Diagram, with the entrance and its handlers expanded
 * (their written Flow).
 */
export async function traceEntrance(page: Page, kind: string, trigger: string): Promise<void> {
  await showEntrances(page);
  await entranceRow(page, kind, trigger).click();
  await expect(boardView(page).locator(".entrance-card")).toBeVisible();
  await expectCardsLoaded(page);
}

/**
 * On a compiled `ts-orders`: traces `POST /orders`, follows `→ OrderService.place` from `createOrder`'s
 * Flow, and switches the page to Code, where the callee's card shows its verified source.
 */
export async function openPlace(page: Page): Promise<void> {
  await traceEntrance(page, "POST", "/orders");
  await follow(card(page, CREATE_ORDER), "OrderService.place");
  await expect(card(page, PLACE)).toBeVisible();
  await chooseLens(page, "Code");
  await expect(card(page, PLACE).locator(".code-view")).toBeVisible();
}
