/**
 * The Add box (the dock's Add on a board): one list of names in the board's project. Focus never
 * leaves its input, so keys go to it.
 */

import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import { dock } from "./board";

export function addBox(page: Page): Locator {
  return page.getByRole("dialog", { name: "Add" });
}

/** Add's input (a combobox over the results). */
export function addInput(page: Page): Locator {
  return addBox(page).getByRole("combobox", { name: "Find" });
}

/** The results Add lists while searching. */
function addResults(page: Page): Locator {
  return addBox(page).getByRole("listbox");
}

/** Opens Add with the dock's button. */
export async function openAdd(page: Page): Promise<void> {
  await dock(page).getByRole("button", { name: "Add", exact: true }).click();
  await expect(addInput(page)).toBeFocused();
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
