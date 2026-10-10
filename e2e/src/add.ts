/**
 * The Add box (A, ⌘K, /): the map before typing, search across projects with kind tabs and a preview,
 * and commands after `>`. Focus never leaves its input, so keys go to it.
 */

import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { enterText } from "./window";

export function addBox(page: Page): Locator {
  return page.getByRole("dialog", { name: "Add" });
}

/** Add's input (a combobox over the results). */
export function addInput(page: Page): Locator {
  return addBox(page).getByRole("combobox", { name: "Find or run a command" });
}

/** Add's kind tabs (Entrances, Functions, Files, Text; All while searching). */
export function addTab(page: Page, kind: string): Locator {
  return addBox(page).getByRole("tablist", { name: "Kinds" }).getByRole("tab", { name: new RegExp(`^${kind}( |$)`) });
}

/** The results (or commands) Add lists while searching. */
export function addResults(page: Page): Locator {
  return addBox(page).getByRole("listbox");
}

/** The preview of the selected result. */
export function addPreview(page: Page): Locator {
  return addBox(page).getByRole("complementary", { name: "Preview" });
}

/** Opens Add with its button (the dock's on a board, the sidebar's search on Home). */
export async function openAdd(page: Page): Promise<void> {
  if ((await page.locator(".board-view").count()) > 0) {
    await page.locator(".board-view").getByRole("toolbar", { name: "Board" }).getByRole("button", { name: "Add", exact: true }).click();
  } else {
    await page.getByRole("complementary", { name: "Sidebar" }).getByRole("button", { name: "Add…" }).click();
  }
  await expect(addInput(page)).toBeFocused();
}

/** Types a query into Add (as one input event) and waits for its results to answer it. */
export async function search(page: Page, query: string): Promise<void> {
  await enterText(addInput(page), query);
  await expect(addResults(page)).toBeVisible();
}

/** The option of Add's results whose accessible name matches `name`. */
export function result(page: Page, name: string | RegExp): Locator {
  return addResults(page).getByRole("option", { name });
}

/** Moves Add's selection to `option` with ↓ (focus stays in the input; hover only tints). */
export async function selectResult(page: Page, option: Locator): Promise<void> {
  const id = await option.getAttribute("id");
  if (id === null) {
    throw new Error("the result has no id");
  }
  await expect
    .poll(async () => {
      if ((await addInput(page).getAttribute("aria-activedescendant")) === id) {
        return true;
      }
      await addInput(page).press("ArrowDown");
      return false;
    }, { intervals: [50] })
    .toBe(true);
}
