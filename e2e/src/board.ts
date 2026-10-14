/**
 * The window in board mode: the shown board's view (top bar, canvas, cards, arrows, dock) and its
 * sidebar (header, Pages, Selected). Only the shown board is mounted, so every locator here is the
 * shown board's.
 */

import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { sidebar, sidebarBoard } from "./home";

/** The shown board's view. */
export function boardView(page: Page): Locator {
  return page.locator(".board-view");
}

/** The shown board, as the engine names it (`project`, `id`). */
export async function shownBoardRef(page: Page): Promise<{ readonly project: string; readonly id: string }> {
  const key = await boardView(page).getAttribute("data-board-key");
  const [project, id] = (key ?? "").split("\u0000");
  if (!project || !id) {
    throw new Error("no board is showing");
  }
  return { project, id };
}

/** A card on the active page, by the name its header shows (`createOrder`, `place`). */
export function card(page: Page, name: string): Locator {
  return boardView(page).getByRole("article", { name, exact: true });
}

/** Every card on the active page. */
export function cards(page: Page): Locator {
  return boardView(page).locator(".board-card");
}

/** Waits until every card on the active page has its model. */
export async function expectCardsLoaded(page: Page): Promise<void> {
  await expect(boardView(page).locator(".board-card[data-loading]")).toHaveCount(0);
}

/**
 * The arrow from card `from` to card `to` (its accessible name is "Call", or "3 calls" from a compact
 * card), the stroke that takes its click.
 */
export async function arrow(from: Locator, to: Locator): Promise<Locator> {
  const source = (await from.getAttribute("data-card-id")) ?? "";
  const target = (await to.getAttribute("data-card-id")) ?? "";
  return boardView(from.page()).locator(`.board-arrow-hit[data-arrow^='${source}|'][data-arrow$='|${target}']`);
}

/**
 * Every arrow drawn from card `from` to card `to`, the arrows that can't be selected (an app's or
 * entrance's, to its code) included: each one's React Flow edge.
 */
export async function drawnArrows(from: Locator, to: Locator): Promise<Locator> {
  const source = (await from.getAttribute("data-card-id")) ?? "";
  const target = (await to.getAttribute("data-card-id")) ?? "";
  return boardView(from.page()).locator(`.react-flow__edge[data-id^='${source}|'][data-id$='|${target}']`);
}

/** The floating dock (Add, Arrange, the zoom). */
export function dock(page: Page): Locator {
  return boardView(page).getByRole("toolbar", { name: "Board" });
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

// --- The board's sidebar ----------------------------------------------------------------------

/** The board's title in the sidebar header. */
export function boardTitle(page: Page): Locator {
  return sidebar(page).locator("h1.board-title");
}

/** Shows a board from Home's sidebar tree (a click on its row), by its title. */
export async function showBoard(page: Page, title: string): Promise<void> {
  await sidebarBoard(page, title).click();
  await expect(boardTitle(page)).toHaveText(title);
}

/** The Selected section at the bottom of the board's sidebar. */
export function selected(page: Page): Locator {
  return sidebar(page).getByRole("region", { name: "Selected" });
}
