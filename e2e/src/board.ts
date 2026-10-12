/**
 * The window in board mode: the active board's view (top bar, canvas, cards, dock) and its sidebar
 * (header, Pages, On this page, Selected), and the board files on disk. Only the active board tab is
 * mounted, so every locator here is the active board's.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { sidebar } from "./home";
import { escapeRegExp } from "./window";

/** The active board's view. */
export function boardView(page: Page): Locator {
  return page.locator(".board-view");
}

/** The active board, as the engine names it (`project`, `id`). */
export async function activeBoardRef(page: Page): Promise<{ readonly project: string; readonly id: string }> {
  const key = await boardView(page).getAttribute("data-board-key");
  const [project, id] = (key ?? "").split("\u0000");
  if (!project || !id) {
    throw new Error("no board is showing");
  }
  return { project, id };
}

/** A card on the active page, by the end of its key (`#function:createOrder`, `"/orders","requireUser"]`). */
export function card(page: Page, keySuffix: string): Locator {
  return boardView(page).locator(`.board-card[data-key$='${keySuffix}']`);
}

/** Every card on the active page. */
export function cards(page: Page): Locator {
  return boardView(page).locator(".board-card");
}

/** Waits until every card on the active page has its model. */
export async function expectCardsLoaded(page: Page): Promise<void> {
  await expect(boardView(page).locator(".board-card[data-loading]")).toHaveCount(0);
}

/** A resolved call link inside a card or the peek sheet, by its written text (`orderService.place`). */
export function callLink(from: Locator, text: string): Locator {
  return from.getByRole("button", { name: `Follow ${text}`, exact: true }).first();
}

/** The page's name in the canvas top bar. */
export function pageTitle(page: Page): Locator {
  return boardView(page).locator(".board-topbar-title");
}

/** The board's publish state in the top bar ("Draft", "Published · 1 unpublished change"). */
export function publishState(page: Page): Locator {
  return boardView(page).locator(".board-publish-text");
}

/** The top bar's Diagram | Code lens. */
export function lens(page: Page): Locator {
  return boardView(page).getByRole("radiogroup", { name: "Lens" });
}

/** The floating dock (Add, Back, Forward, the trail, Arrange, the zoom). */
export function dock(page: Page): Locator {
  return boardView(page).getByRole("toolbar", { name: "Board" });
}

/** The zoom the dock shows, in percent. */
export async function zoomPercent(page: Page): Promise<number> {
  const text = (await dock(page).locator(".board-dock-zoom").textContent()) ?? "";
  return Number.parseInt(text, 10);
}

/** The trail of focus moves in the dock, as its labels. */
export function trail(page: Page): Locator {
  return dock(page).getByRole("navigation", { name: "Trail" }).getByRole("button");
}

/** The canvas's camera transform (`translate(…) scale(…)`). */
export async function camera(page: Page): Promise<string> {
  return (await boardView(page).locator(".react-flow__viewport").getAttribute("style")) ?? "";
}

/**
 * Waits until the active page's camera has stopped moving (focusing a card pans with an animation):
 * its transform unchanged for ten consecutive frames.
 */
export async function cameraSettled(page: Page): Promise<void> {
  await boardView(page).evaluate(
    (view) =>
      new Promise<void>((resolve) => {
        const viewport = view.querySelector<HTMLElement>(".react-flow__viewport");
        let last = viewport?.style.transform ?? "";
        let still = 0;
        const tick = () => {
          const now = viewport?.style.transform ?? "";
          still = now === last ? still + 1 : 0;
          last = now;
          if (still >= 10) {
            resolve();
          } else {
            requestAnimationFrame(tick);
          }
        };
        requestAnimationFrame(tick);
      }),
  );
}

/** Whether the point at the middle of `target` hits it (nothing covers it, and it is on screen). */
async function isHittable(target: Locator): Promise<boolean> {
  return target.evaluate((element) => {
    const box = element.getBoundingClientRect();
    const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
    return hit !== null && (hit === element || element.contains(hit));
  });
}

/**
 * Pans the active page (a wheel scroll pans the canvas) until `target` can be pointed at: on screen
 * and not under the floating dock or top bar.
 */
export async function panUntilHittable(page: Page, target: Locator): Promise<void> {
  const pane = boardView(page).locator(".react-flow__pane");
  await expect
    .poll(
      async () => {
        if (await isHittable(target)) {
          return true;
        }
        const box = await target.boundingBox();
        const area = await pane.boundingBox();
        if (box && area) {
          await page.mouse.move(area.x + area.width / 2, area.y + area.height / 3);
          const dy = box.y + box.height / 2 - (area.y + area.height / 2);
          const dx = box.x < area.x ? box.x - area.x - 40 : box.x + box.width > area.x + area.width ? box.x + box.width - (area.x + area.width) + 40 : 0;
          await page.mouse.wheel(dx, dy);
          await cameraSettled(page);
        }
        return false;
      },
      { message: "pan the canvas until the target is on screen" },
    )
    .toBe(true);
}

/** Follows a resolved call link inside a card (pans it into reach first). */
export async function follow(from: Locator, text: string): Promise<void> {
  const link = callLink(from, text);
  await panUntilHittable(from.page(), link);
  await link.click();
}

/** Clicks the empty canvas (top left of the pane), which clears the selection and takes focus. */
export async function clickCanvas(page: Page): Promise<void> {
  await boardView(page).locator(".react-flow__pane").click({ position: { x: 12, y: 12 } });
}

/** Opens a card's context menu and chooses `item`. */
export async function cardMenu(target: Locator, item: string | RegExp): Promise<void> {
  const page = target.page();
  await panUntilHittable(page, target.locator(".card-header"));
  await target.locator(".card-header").click({ button: "right" });
  const menu = page.getByRole("menu", { name: "Card actions" });
  await menu.getByRole("menuitem", { name: item }).click();
  await expect(menu).toBeHidden();
}

// --- The board's sidebar ----------------------------------------------------------------------

/** The board's title in the sidebar header. */
export function boardTitle(page: Page): Locator {
  return sidebar(page).locator("h1.board-title");
}

/** The board menu (`⋯` in the sidebar header): opens it and chooses `item` (a submenu path for Move to folder). */
export async function boardMenu(page: Page, ...path: readonly (string | RegExp)[]): Promise<void> {
  await sidebar(page).getByRole("button", { name: "Board menu" }).click();
  await chooseMenuPath(page, "Board menu", path);
}

/** A menu's item by its name (a string matches the whole name), checkable or not. */
export function menuItem(menu: Locator, name: string | RegExp): Locator {
  const options = { name, exact: true };
  return menu.getByRole("menuitem", options).or(menu.getByRole("menuitemcheckbox", options));
}

/** Chooses `path` in the open menu named `menu`, opening submenus on the way. */
export async function chooseMenuPath(page: Page, menu: string, path: readonly (string | RegExp)[]): Promise<void> {
  let current = page.getByRole("menu", { name: menu });
  await expect(current).toBeVisible();
  for (const [index, item] of path.entries()) {
    const entry = menuItem(current, item);
    if (index < path.length - 1) {
      await entry.click();
      current = page.getByRole("menu", { name: item, exact: true });
      await expect(current).toBeVisible();
    } else {
      await entry.click();
    }
  }
}

/** The Pages section's list of pages. */
export function pagesList(page: Page): Locator {
  return sidebar(page).getByRole("listbox", { name: "Pages of the board" });
}

/** A page's row in Pages, by its title. */
export function pageRow(page: Page, title: string): Locator {
  return pagesList(page).getByRole("option").filter({ has: page.locator(".list-row-label", { hasText: new RegExp(`^${escapeRegExp(title)}$`) }) });
}

/** On this page: the active page's cards, groups, and links in follow order. */
export function onThisPage(page: Page): Locator {
  return sidebar(page).getByRole("listbox", { name: "Cards on this page" });
}

/** A row of On this page, by its label. */
export function layerRow(page: Page, label: string): Locator {
  return onThisPage(page).getByRole("option", { name: label, exact: true });
}

/** The Selected section at the bottom of the board's sidebar. */
export function selected(page: Page): Locator {
  return sidebar(page).getByRole("region", { name: "Selected" });
}

// --- Board files ------------------------------------------------------------------------------

/** A board file, as the journeys read it. */
export interface BoardFile {
  readonly title?: string;
  readonly folder?: string;
  readonly projects?: readonly string[];
  readonly publishes?: string;
  readonly pages: readonly {
    readonly id: string;
    readonly title: string;
    readonly cards: readonly {
      readonly id: string;
      readonly ref: { readonly block?: string; readonly entrance?: string; readonly project?: string };
      readonly at: readonly [number, number];
      readonly folded?: boolean;
      readonly view?: string;
      readonly description?: { readonly text: string };
    }[];
    readonly groups: readonly { readonly id: string; readonly title: string }[];
    readonly labels: readonly { readonly key: string; readonly text: string }[];
    readonly links?: readonly {
      readonly id: string;
      readonly from: string;
      readonly to: string;
      readonly kind?: string;
      readonly author?: string;
      readonly channel?: string;
      readonly label?: { readonly text: string };
    }[];
    readonly frames?: readonly { readonly id: string; readonly title: string; readonly members: readonly string[]; readonly look: string; readonly collapsed?: boolean; readonly author: string }[];
    readonly resources?: readonly { readonly id: string; readonly kind: string; readonly name: string; readonly type?: string; readonly author: string }[];
    readonly notes?: readonly { readonly id: string; readonly text: string; readonly attach?: string; readonly author: string }[];
  }[];
}

/** Reads a board file (`id` is the project-relative path, such as `.unfold/local/drafts/board-1.json`). */
export function readBoard(folder: string, id: string): BoardFile {
  return JSON.parse(readFileSync(join(folder, id), "utf8")) as BoardFile;
}
