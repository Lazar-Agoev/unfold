/**
 * Typed lines (docs/reader.md) on the `typed-lines` fixture: a line between cards
 * carries its call's signature at rest; clicking the label opens the call's contract in its place,
 * with what it sends, returns, and throws; a written type opens its definition under its row, and
 * "Used in" lists where it is used; Escape closes the contract. The same in the dark theme.
 */

import type { Locator, Page } from "@playwright/test";
import { addInput, openAdd, result, selectResult } from "../src/add";
import { boardView, cameraSettled, card, chooseLens, dock, expectCardsLoaded, follow, panUntilHittable } from "../src/board";
import { expect, test } from "../src/fixtures";
import { firstCompile, homeTitle, projectRow } from "../src/home";
import { enterText } from "../src/window";

const CREATE_ORDER = "src/orders/create.ts#function:createOrder";
const RESERVE = "src/inventory.ts#function:reserveInventory";
const CHARGE = "src/payments.ts#function:chargePayment";

function label(page: Page, text: string): Locator {
  return boardView(page).getByRole("button", { name: `Line ${text}`, exact: true });
}

function contract(page: Page): Locator {
  return boardView(page).getByRole("region", { name: "Call to chargePayment" });
}

/**
 * The pairs of a drawn label and a card whose boxes meet, as "label × card" (none, when every label
 * keeps clear of the cards), and how many labels are drawn.
 */
async function labelsOverCards(page: Page): Promise<{ drawn: number; overlaps: string[] }> {
  return boardView(page).evaluate((view) => {
    const labels = [...view.querySelectorAll<HTMLElement>(".line-label")];
    const cards = [...view.querySelectorAll<HTMLElement>(".board-card")];
    const overlaps: string[] = [];
    for (const label of labels) {
      const a = label.getBoundingClientRect();
      for (const card of cards) {
        const b = card.getBoundingClientRect();
        if (a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom) {
          overlaps.push(`${label.textContent} × ${card.dataset["key"] ?? ""}`);
        }
      }
    }
    return { drawn: labels.length, overlaps };
  });
}

/** The contract's rows as their three columns' text. */
async function rows(section: Locator): Promise<string[][]> {
  return section.locator(".line-contract-row").evaluateAll((elements) => elements.map((row) => [...row.children].map((cell) => cell.textContent)));
}

test.describe("typed lines", () => {
  test("a line's label, its contract, a type's definition and uses, and Escape, in light and dark", async ({ sandbox, launch }) => {
    const folder = sandbox.fixture("typed-lines/ts", "typed-lines");
    const app = await launch({ args: [folder] });
    const page = await app.firstWindow();
    await expect(projectRow(page, "typed-lines")).toBeVisible();
    await expect(homeTitle(page)).toHaveText("typed-lines");
    await firstCompile(page);

    await test.step("createOrder and the two functions it calls go on a new board", async () => {
      await openAdd(page);
      await enterText(addInput(page), "createOrder");
      const option = result(page, "Function createOrder, src/orders/create.ts");
      await expect(option).toBeVisible();
      await selectResult(page, option);
      await addInput(page).press("Enter");
      await expect(card(page, CREATE_ORDER)).toBeVisible();
      await expectCardsLoaded(page);
      // Read in Code: its call links follow the calls, and a contract marks the lines it reads.
      await chooseLens(page, "Code");
      await follow(card(page, CREATE_ORDER), "reserveInventory");
      await expect(card(page, RESERVE)).toBeVisible();
      await expectCardsLoaded(page);
      // Following centers the callee; Fit brings the caller back into view.
      await dock(page).getByRole("button", { name: "Fit", exact: true }).click();
      await cameraSettled(page);
      await follow(card(page, CREATE_ORDER), "chargePayment");
      await expect(card(page, CHARGE)).toBeVisible();
      await expectCardsLoaded(page);
      // Arrange lays the callees out in a column to the right, as the design does (and fits them).
      await dock(page).getByRole("button", { name: "Arrange", exact: true }).click();
      await cameraSettled(page);
    });

    await test.step("a line carries its call's signature at rest", async () => {
      await expect(label(page, "(CartItem[]) → Promise<Hold>")).toBeVisible();
      await expect(label(page, "(Cart, User) → Promise<Charge>")).toBeVisible();
      // Arrange leaves 120px between the columns, less than either full label: each label still keeps
      // clear of every card (in a shorter form, beside the cards, or not drawn at rest).
      const placed = await labelsOverCards(page);
      expect(placed.overlaps).toEqual([]);
      expect(placed.drawn).toBeGreaterThan(0);
      await test.info().attach("board, light", { body: await boardView(page).screenshot(), contentType: "image/png" });
    });

    await test.step("clicking a label opens the call's contract in its place", async () => {
      const charge = label(page, "(Cart, User) → Promise<Charge>");
      await panUntilHittable(page, charge);
      await charge.click();
      await expect(contract(page)).toBeVisible();
      await expect(charge).toBeHidden();
      expect(await rows(contract(page))).toEqual([
        ["Sends", "cart", "cart: Cart"],
        ["", "req.user", "user: User"],
        ["Returns", "charge", "Promise<Charge>"],
        ["Throws", "catch", "CardDeclinedError"],
      ]);
      // The callee marks the lines the contract reads; the caller keeps its calling line selected.
      await expect(card(page, CHARGE).locator(".code-row.marked")).toHaveCount(2);
      await expect(card(page, CHARGE).locator(".code-row.marked[data-line='17']")).toBeVisible();
      await expect(card(page, CREATE_ORDER).locator(".code-row.selected[data-line='27']")).toBeVisible();
    });

    await test.step("a written type opens its definition under its row", async () => {
      await contract(page).getByRole("button", { name: "Cart", exact: true }).click();
      const definition = contract(page).getByRole("group", { name: "interface Cart" });
      await expect(definition).toBeVisible();
      await expect(definition.getByText("cart.ts:12")).toBeVisible();
      await expect(definition.getByText("coupon?")).toBeVisible();
      await expect(definition.getByRole("button", { name: "CartItem", exact: true })).toBeVisible();
      await test.info().attach("type opened, light", { body: await boardView(page).screenshot(), contentType: "image/png" });
    });

    await test.step("Used in lists the places a type is used", async () => {
      await contract(page).getByRole("button", { name: /^Used in \d+ places?$/ }).click();
      const places = page.getByRole("dialog", { name: "Where Cart is used" });
      await expect(places).toBeVisible();
      await expect(places.getByRole("listitem").filter({ hasText: "chargePayment" })).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(places).toBeHidden();
      await expect(contract(page)).toBeVisible();
    });

    await test.step("Escape closes the contract and the label comes back", async () => {
      await page.keyboard.press("Escape");
      await expect(contract(page)).toBeHidden();
      await expect(label(page, "(Cart, User) → Promise<Charge>")).toBeVisible();
      await expect(card(page, CHARGE).locator(".code-row.marked")).toHaveCount(0);
    });

    await test.step("the dark theme draws the label and the contract with its own tokens", async () => {
      await page.evaluate(() => {
        document.documentElement.setAttribute("data-theme", "dark");
      });
      const charge = label(page, "(Cart, User) → Promise<Charge>");
      // The label sits on the canvas color; the contract on the surface.
      await expect(charge).toHaveCSS("background-color", "rgb(21, 22, 25)");
      await charge.click();
      await expect(contract(page)).toHaveCSS("background-color", "rgb(29, 31, 35)");
      await contract(page).getByRole("button", { name: "Cart", exact: true }).click();
      await expect(contract(page).getByRole("group", { name: "interface Cart" })).toBeVisible();
      await test.info().attach("type opened, dark", { body: await boardView(page).screenshot(), contentType: "image/png" });
      await page.keyboard.press("Escape");
      await expect(contract(page)).toBeHidden();
    });
  });
});
