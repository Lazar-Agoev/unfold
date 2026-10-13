/**
 * The Add box (the dock's Add on a board, the sidebar's search on Home): the entrance browser before
 * typing (LOOK IN, the scope's list, a preview), and search across projects with kind tabs and a
 * preview. Focus never leaves its input, so keys go to it.
 */

import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { enterText } from "./window";

export function addBox(page: Page): Locator {
  return page.getByRole("dialog", { name: "Add" });
}

/** Add's input (a combobox over the results). */
export function addInput(page: Page): Locator {
  return addBox(page).getByRole("combobox", { name: "Find" });
}

/** Add's kind tabs (Entrances, Functions, Files, Text; All while searching). */
export function addTab(page: Page, kind: string): Locator {
  return addBox(page).getByRole("tablist", { name: "Kinds" }).getByRole("tab", { name: new RegExp(`^${kind}( |$)`) });
}

/** The results Add lists while searching. */
export function addResults(page: Page): Locator {
  return addBox(page).getByRole("listbox");
}

/** The preview of the selected result. */
export function addPreview(page: Page): Locator {
  return addBox(page).getByRole("complementary", { name: "Preview" });
}

/** The entrance browser's LOOK IN. */
export function lookIn(page: Page): Locator {
  return addBox(page).getByRole("tree", { name: "Look in" });
}

/** A LOOK IN row by its name (All projects, a project, an application). */
export function lookInItem(page: Page, name: string): Locator {
  return lookIn(page).getByRole("treeitem", { name: new RegExp(`^${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*[\\d,]*$`) });
}

/** The entrance count a LOOK IN row shows, once it has loaded. */
export async function lookInCount(page: Page, name: string): Promise<number> {
  const count = lookInItem(page, name).locator(".add-nav-count");
  await expect(count).toHaveText(/^[\d,]+$/);
  return Number((await count.innerText()).replaceAll(",", ""));
}

/** The browser's list of the selected scope. */
export function browseList(page: Page): Locator {
  return addBox(page).getByRole("listbox", { name: "Entrances" });
}

/** A row of the browser's list by its accessible name (`POST /orders on page`, `Launch npm run start`). */
export function entranceRow(page: Page, name: string | RegExp): Locator {
  return browseList(page).getByRole("option", { name, exact: typeof name === "string" });
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
