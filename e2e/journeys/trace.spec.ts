/**
 * Tracing an entrance (docs/ui-shell.md#home), end to end on a
 * copy of `py-orders` (Python, so its flows have builtins to hide): clicking an entrance on Home puts it
 * and its code, expanded, on a new board with arrows to the code; a call opens its callee's flow in
 * place; Show callers adds the calling definition with its arrow; Show all calls reveals the builtins;
 * Add puts a definition on the page by name; and a source file edited on disk updates its card.
 */

import { join } from "node:path";
import { addInput, openAdd, result, selectResult } from "../src/add";
import { arrow, boardTitle, boardView, card, cards, drawnArrows, expectCardsLoaded } from "../src/board";
import { expect, test } from "../src/fixtures";
import { launchCompiled } from "../src/flows";
import { startRow } from "../src/home";
import { chooseOnCard, clickOnCanvas } from "../src/reader";
import { editFile } from "../src/sandbox";
import { enterText, escapeRegExp } from "../src/window";

const IMPORT_ORDERS = "src/orders_playground/jobs/import_orders.py";
const VALIDATE_ORDER = "src/orders_playground/lib/validate.py#validate_order";

test("an entrance traces onto a board that follows its calls and keeps up with the code", async ({ launch, sandbox }) => {
  const { app, page, folders } = await launchCompiled(launch, sandbox, ["py-orders"]);
  const folder = folders[0] ?? "";
  const entrance = card(page, "Orders CLI · Import orders");
  const importOrders = card(page, "import_orders");
  const callName = (name: string) => importOrders.locator(".flow-call-name", { hasText: new RegExp(`^${escapeRegExp(name)}$`) });

  await test.step("clicking an entrance shows it and its code, expanded, on a new board, with arrows to the code", async () => {
    await startRow(page, "Import orders").click();
    await expect(boardTitle(page)).toHaveText("Import orders");
    await expect(cards(page)).toHaveCount(2);
    await expect(entrance).toBeVisible();
    await expect(importOrders).toBeVisible();
    await expectCardsLoaded(page);
    await expect(importOrders.getByRole("button", { name: "Collapse import_orders" })).toHaveAttribute("aria-expanded", "true");
    for (const call of ["validate_order", "order_service.place", "logger.info"]) {
      await expect(callName(call)).toBeVisible();
    }
    await expect(await drawnArrows(entrance, importOrders)).toHaveCount(1);
  });

  await test.step("a call opens its callee's flow in place", async () => {
    const open = importOrders.getByRole("button", { name: "Open validate_order in place", exact: true });
    await clickOnCanvas(page, open);
    await expect(importOrders.getByRole("button", { name: "Close validate_order", exact: true })).toHaveAttribute("aria-expanded", "true");
    await expect(importOrders.locator(".flow-inset-name", { hasText: /^validate_order$/ })).toBeVisible();
    // validate_order's own blocks are drawn inside import_orders' card.
    await expect(importOrders.locator(`[data-key^='${VALIDATE_ORDER}/']`).first()).toBeVisible();
  });

  const main = card(page, "main");

  await test.step("Show callers adds the calling definition, with its arrow", async () => {
    await chooseOnCard(page, importOrders, "Show callers");
    await expect(main).toBeVisible();
    await expect(cards(page)).toHaveCount(3);
    await expectCardsLoaded(page);
    await expect(await arrow(main, importOrders)).toHaveCount(1);
  });

  await test.step("Show all calls reveals the builtins, and hides them again", async () => {
    const showAll = boardView(page).getByRole("switch", { name: "Show all calls" });
    const len = importOrders.locator(".flow-thin-item.builtin", { hasText: /^len/ });
    await expect(showAll).toHaveAttribute("aria-checked", "false");
    await expect(len).toHaveCount(0);
    await showAll.click();
    await expect(showAll).toHaveAttribute("aria-checked", "true");
    await expect(len.first()).toBeVisible();
    await showAll.click();
    await expect(showAll).toHaveAttribute("aria-checked", "false");
    await expect(len).toHaveCount(0);
  });

  const validateOrder = card(page, "validate_order");

  await test.step("Add puts a definition on the page by name", async () => {
    await openAdd(page);
    await enterText(addInput(page), "validate_order");
    await selectResult(page, result(page, /^function validate_order, /));
    await addInput(page).press("Enter");
    await expect(validateOrder).toBeVisible();
    await expect(cards(page)).toHaveCount(4);
    await expectCardsLoaded(page);
    await expect(await arrow(importOrders, validateOrder)).toHaveCount(1);
  });

  await test.step("a source file edited on disk updates its card", async () => {
    editFile(join(folder, IMPORT_ORDERS), "order_service.place(draft)", "order_service.cancel(draft)");
    await expect(callName("order_service.cancel")).toBeVisible();
    await expect(callName("order_service.place")).toHaveCount(0);
  });

  expect(app.logs.filter((line) => line.startsWith("[pageerror]"))).toEqual([]);
});
