/** Multi-step flows several journeys start with. */

import type { Page } from "@playwright/test";
import { expect } from "@playwright/test";
import type { UnfoldApp } from "./app";
import { boardView, card, expectCardsLoaded, follow } from "./board";
import type { Fixtures } from "./fixtures";
import { addProjectFromSidebar, entranceRow, firstCompile, homeTitle, projectRow, tabs } from "./home";
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
  await entranceRow(page, kind, trigger).click();
  await expect(tabs(page)).toHaveCount(before + 1);
  await expect(boardView(page).locator(".entrance-card")).toBeVisible();
  await expectCardsLoaded(page);
  await expectHandlersPlaced(page);
}

/** The canvas's gap between a card and the cards it opens (`FOLLOW_GAP` in ui/src/canvas/layout.ts). */
const FOLLOW_GAP = 80;

/**
 * Waits until a traced entrance's handler cards stand in their column beside it. The trace places
 * them once their cards are drawn and measured, as part of the same step: an edit made before that
 * (or leaving the board first) would meet them still in their first, estimated places.
 */
async function expectHandlersPlaced(page: Page): Promise<void> {
  await expect
    .poll(
      () =>
        boardView(page).evaluate((view, gap) => {
          const viewport = view.querySelector<HTMLElement>(".react-flow__viewport");
          const entrance = view.querySelector(".entrance-card")?.getBoundingClientRect();
          if (!viewport || !entrance) {
            return false;
          }
          const zoom = new DOMMatrix(getComputedStyle(viewport).transform).a;
          const handlers = [...view.querySelectorAll(".block-card")].map((card) => card.getBoundingClientRect());
          return handlers.every((handler) => Math.abs((handler.left - entrance.right) / zoom - gap) < 1);
        }, FOLLOW_GAP),
      { message: "the traced handlers are placed beside the entrance" },
    )
    .toBe(true);
}

/** On a compiled `ts-orders`: traces `POST /orders` and follows `orderService.place` from `createOrder`. */
export async function openPlace(page: Page): Promise<void> {
  await traceEntrance(page, "POST", "/orders");
  await follow(card(page, CREATE_ORDER), "orderService.place");
  await expect(card(page, PLACE).locator(".code-view")).toBeVisible();
}
