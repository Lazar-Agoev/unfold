/**
 * Explaining a card by hand (core-scope.html's Explaining frames): a block's menu, the canvas's menu
 * and Add resource…, drawing a link and picking its kind, and the board as its file holds it.
 */

import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { boardView } from "./board";
import { clickOnCanvas } from "./reader";
import { readFile } from "./sandbox";
import { enterText } from "./window";

/** A page of a board file, as the journeys read it. */
export interface SavedPage {
  readonly cards: readonly { readonly id: string; readonly description?: SavedText }[];
  readonly names?: readonly { readonly key: string; readonly text: string; readonly author: string }[];
  readonly combined?: readonly { readonly id: string; readonly title: string; readonly members: readonly string[]; readonly collapsed: boolean }[];
  readonly proposals?: readonly { readonly title: string; readonly reason?: string; readonly after?: string; readonly author: string }[];
  readonly resources?: readonly { readonly id: string; readonly kind: string; readonly name: string; readonly columns?: readonly { readonly name: string; readonly type?: string; readonly key?: string }[] }[];
  readonly links?: readonly {
    readonly id: string;
    readonly from: string;
    readonly to: string;
    readonly kind: string;
    readonly label?: SavedText;
    readonly fromColumn?: string;
    readonly toColumn?: string;
    readonly cardinality?: string;
  }[];
}

interface SavedText {
  readonly text: string;
  readonly author: string;
  readonly signature?: string;
}

/** The first page of the board file at `path`, as last written. */
export function savedPage(path: string): SavedPage {
  const board = JSON.parse(readFile(path)) as { readonly pages: readonly SavedPage[] };
  const first = board.pages[0];
  if (!first) {
    throw new Error(`${path} has no pages`);
  }
  return first;
}

/** Opens the menu of a block drawn in a card (right-click), once the camera holds still, and chooses `item`. */
export async function chooseOnBlock(page: Page, block: Locator, item: string): Promise<void> {
  const menu = page.getByRole("menu", { name: "Block actions" });
  await clickOnCanvas(page, block, { button: "right" });
  await menu.getByRole("menuitem", { name: item, exact: true }).click();
  await expect(menu).toBeHidden();
}

/**
 * Opens the canvas's menu on empty canvas with room below and to the right for a resource card (the
 * card goes where the menu opened), and chooses `item`.
 */
export async function chooseOnEmptyCanvas(page: Page, item: string): Promise<void> {
  const spot = await boardView(page)
    .locator(".react-flow__pane")
    .evaluate((pane) => {
      const area = pane.getBoundingClientRect();
      const free = (x: number, y: number) => document.elementFromPoint(x, y) === pane;
      const room = [
        [0, 0],
        [400, 0],
        [0, 260],
        [400, 260],
        [200, 130],
      ];
      for (let y = area.top + 80; y < area.bottom - 300; y += 40) {
        for (let x = area.left + 40; x < area.right - 440; x += 40) {
          if (room.every(([dx = 0, dy = 0]) => free(x + dx, y + dy))) {
            return { x, y };
          }
        }
      }
      return null;
    });
  if (!spot) {
    throw new Error("no empty canvas with room for a resource card");
  }
  const menu = page.getByRole("menu", { name: "Board actions" });
  await page.mouse.click(spot.x, spot.y, { button: "right" });
  await menu.getByRole("menuitem", { name: item, exact: true }).click();
  await expect(menu).toBeHidden();
}

/** A table's column as Add resource… takes it. */
interface ColumnInput {
  readonly name: string;
  readonly type: string;
  readonly key?: "primary" | "foreign" | "unique";
}

/** Adds a table with `columns` from the canvas's menu (Add resource…), and returns its card. */
export async function addTable(page: Page, name: string, columns: readonly ColumnInput[]): Promise<Locator> {
  await chooseOnEmptyCanvas(page, "Add resource…");
  const popover = page.getByRole("dialog", { name: "Add resource" });
  await expect(popover.getByRole("group", { name: "Kind" }).getByRole("button", { name: "Table" })).toHaveAttribute("aria-pressed", "true");
  await enterText(popover.getByRole("textbox", { name: "Name", exact: true }), name);
  for (const [index, column] of columns.entries()) {
    if (index > 0) {
      await popover.getByRole("button", { name: "+ Column" }).click();
    }
    await enterText(popover.getByRole("textbox", { name: "Column name", exact: true }).nth(index), column.name);
    await enterText(popover.getByRole("textbox", { name: "Column type", exact: true }).nth(index), column.type);
    if (column.key) {
      await popover.getByRole("combobox", { name: `Key mark of ${column.name}`, exact: true }).selectOption(column.key);
    }
  }
  await popover.getByRole("button", { name: "Add", exact: true }).click();
  await expect(popover).toBeHidden();
  const table = boardView(page).getByRole("article", { name: `table ${name}`, exact: true });
  await expect(table).toBeVisible();
  return table;
}

/**
 * Finishes Draw link… (already chosen from a menu): clicks the item it links to (at its header), and
 * gives the link its label.
 */
export async function finishLink(page: Page, to: Locator, label: string): Promise<void> {
  await expect(boardView(page).locator(".board-hint")).toHaveText("Click what it links to · esc cancels");
  await clickOnCanvas(page, to.locator(".card-header"));
  const dialog = page.getByRole("dialog", { name: "Draw link" });
  await enterText(dialog.getByRole("textbox", { name: "Label" }), label);
  await dialog.getByRole("button", { name: "Draw", exact: true }).click();
  await expect(dialog).toBeHidden();
}

/** A connection's chip, by what it says. */
export function connectionChip(page: Page, text: string | RegExp): Locator {
  return boardView(page).getByRole("button", { name: typeof text === "string" ? `Connection ${text}` : text, exact: typeof text === "string" });
}

/** Picks a connection's kind from Kind ▸ in its chip's menu. */
export async function setLinkKind(page: Page, chip: Locator, kind: string): Promise<void> {
  const menu = page.getByRole("menu", { name: "Connection actions" });
  await clickOnCanvas(page, chip, { button: "right" });
  await menu.getByRole("menuitem", { name: "Kind", exact: true }).click();
  const kinds = page.getByRole("menu", { name: "Kind" });
  await kinds.getByRole("menuitemcheckbox", { name: kind, exact: true }).click();
  await expect(menu).toBeHidden();
}
