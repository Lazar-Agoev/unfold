/**
 * The app on the new model, end to end: a copy of `ts-orders` added to an empty workspace compiles,
 * a new board takes a definition through Add, and the card expands to draw its flow.
 */

import { addInput, openAdd, result, selectResult } from "../src/add";
import { boardView, card, expectCardsLoaded } from "../src/board";
import { expect, test } from "../src/fixtures";
import { launchCompiled } from "../src/flows";
import { homeMain } from "../src/home";
import { enterText } from "../src/window";

test("a playground compiles, and a board shows an expanded card with its flow", async ({ launch, sandbox }) => {
  const { app, page } = await launchCompiled(launch, sandbox);

  await homeMain(page).locator(".home-header").getByRole("button", { name: "New board" }).click();
  await expect(boardView(page)).toBeVisible();

  await openAdd(page);
  await enterText(addInput(page), "createOrder");
  await selectResult(page, result(page, /^function createOrder, /));
  await addInput(page).press("Enter");
  const createOrder = card(page, "createOrder");
  await expect(createOrder).toBeVisible();
  await expectCardsLoaded(page);

  await createOrder.getByRole("button", { name: "Expand createOrder" }).click();
  await expect(createOrder.getByRole("button", { name: "Collapse createOrder" })).toHaveAttribute("aria-expanded", "true");
  for (const call of ["orderService.place", "slack.post"]) {
    await expect(createOrder.locator(".flow-call-name", { hasText: call })).toBeVisible();
  }
  expect(app.logs.filter((line) => line.startsWith("[pageerror]"))).toEqual([]);
});
