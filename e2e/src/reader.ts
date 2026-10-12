/** Helpers for the reader journeys on the board: the saved draft, line-number drags, and dragging a card. */

import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { activeBoardRef, cameraSettled, panUntilHittable, readBoard, type BoardFile } from "./board";

/** Waits until the active board's saved file satisfies `check` (autosave runs after a short pause). */
export async function expectSaved(page: Page, folder: string, check: (board: BoardFile) => boolean, message: string): Promise<BoardFile> {
  const { id } = await activeBoardRef(page);
  let saved: BoardFile | undefined;
  await expect
    .poll(
      () => {
        try {
          saved = readBoard(folder, id);
          return check(saved);
        } catch {
          return false;
        }
      },
      { message },
    )
    .toBe(true);
  if (!saved) {
    throw new Error(message);
  }
  return saved;
}

/** A line number in a card's code gutter. */
function lineNumber(card: Locator, line: number): Locator {
  return card.locator(".code-number", { hasText: new RegExp(`^${line}$`) });
}

/** Drags down a card's line numbers from `from` to `to`, which proposes a group of those lines. */
export async function dragLines(page: Page, card: Locator, from: number, to: number): Promise<void> {
  await cameraSettled(page);
  await panUntilHittable(page, lineNumber(card, to));
  const start = await lineNumber(card, from).boundingBox();
  const end = await lineNumber(card, to).boundingBox();
  if (!start || !end) {
    throw new Error("the line numbers aren't on screen");
  }
  await page.mouse.move(start.x + start.width / 2, start.y + start.height / 2);
  await page.mouse.down();
  await page.mouse.move(end.x + end.width / 2, end.y + end.height / 2, { steps: 5 });
  await page.mouse.up();
}

/** Drags a card by its header by `dx`, `dy` screen pixels. */
export async function dragCard(page: Page, card: Locator, dx: number, dy: number): Promise<void> {
  await cameraSettled(page);
  await panUntilHittable(page, card.locator(".card-header .card-title"));
  const header = await card.locator(".card-header .card-title").boundingBox();
  if (!header) {
    throw new Error("the card's header isn't on screen");
  }
  const x = header.x + header.width / 2;
  const y = header.y + header.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + dx, y + dy, { steps: 10 });
  await page.mouse.up();
}

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
