/**
 * One Diagram canvas on a compiled `ts-orders` (docs/user-guide.md#diagram-and-code, View code,
 * grouping, and Proposed steps): a traced entrance shows its ordered handlers and collapses to a
 * compact node, and Code and back keeps the expansions; View code (`C`) on createOrder's condition
 * opens its source with the condition's line and expression highlighted, and the highlight moves with
 * the condition when an edit above it compiles; Combine… makes two steps one collapsed block, which ⌘Z
 * undoes; Add proposed step after… puts a PROPOSED step on the card beside the compiled steps until
 * Remove proposal; OrderService.place expanded in place connects its own call to validateOrder's card;
 * and the emails worker's callback to withRetry expands in place to sendEmail's Flow, captioned with
 * the callback it goes through.
 */

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Locator, Page } from "@playwright/test";
import { boardView, card, clickCanvas, flowStep, lens, selected } from "../src/board";
import { expect, test } from "../src/fixtures";
import { CREATE_ORDER, launchCompiled, POST_ORDERS, traceEntrance } from "../src/flows";
import { dialog, showHome } from "../src/home";
import { clickOnCanvas } from "../src/reader";

const ORDERS_FILE = "src/routes/orders.ts";
const CONDITION = 'req.header("prefer") === "respond-async"';
const COMBINED_TITLE = "Place it and tell Slack";
const PROPOSED_TITLE = "Email a confirmation";

/** The peek sheet showing `path`. */
function sheet(page: Page, path: string): Locator {
  return page.getByRole("complementary", { name: `Source of ${path}`, exact: true });
}

/** The text the peek sheet marks as View code's exact expression (it may span several colored runs). */
async function focusText(peek: Locator): Promise<string> {
  return (await peek.locator(".peek-focus").allTextContents()).join("");
}

/** Clicks a Flow step on its label (the step's own buttons follow, expand, or open code). */
async function clickStep(page: Page, step: Locator, options: { readonly modifiers?: "Shift"[]; readonly button?: "right" } = {}): Promise<void> {
  await clickOnCanvas(page, step.locator(".flow-label"), options);
}

/** Chooses `item` in the step menu a Flow step's right-click opened. */
async function chooseStepItem(page: Page, item: string): Promise<void> {
  const menu = page.getByRole("menu", { name: "Card actions" });
  await menu.getByRole("menuitem", { name: item, exact: true }).click();
  await expect(menu).toBeHidden();
}

test.describe("diagrams", () => {
  test("expand and collapse, View code, Combine, a proposed step, and a callback expanded in place", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    await traceEntrance(page, "POST", "/orders");
    const entrance = card(page, POST_ORDERS);
    const createOrder = card(page, CREATE_ORDER);

    await test.step("a traced entrance shows its handlers in order; its chevron collapses it to a compact node, and Code and back keeps that", async () => {
      await expect(entrance.locator(".ecard-heading")).toHaveText("Handlers, in order");
      await expect(entrance.locator(".ecard-handler-name")).toHaveText(["requireUser", "validateBody", "createOrder"]);
      await expect(createOrder.locator(".flow-view")).toBeVisible();
      await clickOnCanvas(page, entrance.getByRole("button", { name: "Collapse POST /orders", exact: true }));
      await expect(entrance).toHaveClass(/diagram-card/);
      await expect(entrance.locator(".ecard-handler")).toHaveCount(0);
      await expect(entrance.getByRole("button", { name: "Expand POST /orders", exact: true })).toBeVisible();
      await clickCanvas(page);
      await page.keyboard.press("Meta+2");
      await expect(lens(page).getByRole("radio", { name: "Code" })).toHaveAttribute("aria-checked", "true");
      await expect(createOrder.locator(".code-view")).toBeVisible();
      // Code shows an entrance whole: its handlers, without a chevron.
      await expect(entrance.locator(".ecard-handler-name")).toHaveText(["requireUser", "validateBody", "createOrder"]);
      await page.keyboard.press("Meta+1");
      await expect(lens(page).getByRole("radio", { name: "Diagram" })).toHaveAttribute("aria-checked", "true");
      await expect(entrance).toHaveClass(/diagram-card/);
      await expect(entrance.locator(".ecard-handler")).toHaveCount(0);
      await expect(createOrder.locator(".flow-view")).toBeVisible();
      await expect(createOrder.locator(".code-view")).toHaveCount(0);
    });

    await test.step("C on createOrder's condition opens orders.ts in the peek, its line and expression highlighted", async () => {
      await clickOnCanvas(page, createOrder.locator(".flow-node.kind-condition .flow-label"));
      await expect(createOrder.locator(".flow-node.kind-condition")).toHaveClass(/selected/);
      await page.keyboard.press("c");
      const peek = sheet(page, ORDERS_FILE);
      await expect(peek).toBeVisible();
      await expect(peek.locator(".peek-row.focus")).toHaveCount(1);
      await expect(peek.locator('.peek-row.focus[data-line="27"]')).toBeVisible();
      await expect.poll(() => focusText(peek)).toBe(CONDITION);
      // The diagram keeps its selection while the sheet shows the source.
      await expect(createOrder.locator(".flow-node.kind-condition")).toHaveClass(/selected/);
      // Ten lines inserted above it: once that compiles, the highlight is on the condition, now line 37.
      const file = join(folders[0] ?? "", ORDERS_FILE);
      const original = readFileSync(file, "utf8");
      writeFileSync(file, `${"\n".repeat(10)}${original}`);
      await expect(peek.locator('.peek-row.focus[data-line="37"]')).toBeVisible({ timeout: 30_000 });
      await expect(peek.locator(".peek-row.focus")).toHaveCount(1);
      await expect.poll(() => focusText(peek)).toBe(CONDITION);
      writeFileSync(file, original);
      await expect(peek.locator('.peek-row.focus[data-line="27"]')).toBeVisible({ timeout: 30_000 });
      await expect(peek.locator(".peek-row.focus")).toHaveCount(1);
      await page.keyboard.press("Escape");
      await expect(peek).toHaveCount(0);
    });

    await test.step("Shift-selecting two steps and Combine… makes one collapsed COMBINED block; ⌘Z removes it", async () => {
      const place = flowStep(createOrder, "OrderService.place");
      const slack = flowStep(createOrder, "slack.post");
      await clickStep(page, place);
      await clickStep(page, slack, { modifiers: ["Shift"] });
      await expect(place).toHaveClass(/selected/);
      await expect(slack).toHaveClass(/selected/);
      await clickStep(page, slack, { button: "right" });
      await chooseStepItem(page, "Combine…");
      const combine = dialog(page, "Combine 2 steps");
      await expect(combine).toBeVisible();
      const title = combine.getByRole("textbox", { name: "Title" });
      await expect(title).toBeFocused();
      await title.pressSequentially(COMBINED_TITLE);
      await combine.getByRole("button", { name: "Combine", exact: true }).click();
      await expect(combine).toBeHidden();
      const block = createOrder.locator(".flow-node.kind-group").filter({ hasText: COMBINED_TITLE });
      await expect(block).toBeVisible();
      await expect(block).toContainText("COMBINED");
      await expect(block).toContainText("2 steps");
      await expect(block.getByRole("button", { name: `Expand step group ${COMBINED_TITLE}` })).toBeVisible();
      // Collapsed, the block stands in for its steps.
      await expect(place).toHaveCount(0);
      await expect(slack).toHaveCount(0);
      await clickCanvas(page);
      await page.keyboard.press("Meta+Z");
      await expect(createOrder.locator(".flow-node.kind-group")).toHaveCount(0);
      await expect(place).toBeVisible();
      await expect(slack).toBeVisible();
    });

    await test.step("Add proposed step after… puts a PROPOSED step after OrderService.place beside the compiled steps; Remove proposal removes it", async () => {
      const place = flowStep(createOrder, "OrderService.place");
      const steps = await createOrder.locator(".flow-node").count();
      await clickStep(page, place, { button: "right" });
      await chooseStepItem(page, "Add proposed step after…");
      // Its placeholder title is selected in Selected, for the person to type over.
      const field = selected(page).getByRole("textbox", { name: "Proposed step title" });
      await expect(field).toBeFocused();
      await expect(field).toHaveValue("Proposed step");
      await field.pressSequentially(PROPOSED_TITLE);
      await expect(field).toHaveValue(PROPOSED_TITLE);
      const proposed = createOrder.getByRole("button", { name: `Proposed step: ${PROPOSED_TITLE}`, exact: true });
      await expect(proposed).toBeVisible();
      await expect(proposed).toContainText("PROPOSED");
      await expect(proposed).toHaveClass(/selected/);
      // The compiled path stays as it is: every step still there, the proposal beside them.
      await expect(createOrder.locator(".flow-node")).toHaveCount(steps);
      await expect(place).toBeVisible();
      await expect(flowStep(createOrder, "slack.post")).toBeVisible();
      await selected(page).getByRole("button", { name: "Remove proposal", exact: true }).click();
      await expect(createOrder.locator(".flow-proposal")).toHaveCount(0);
      await expect(createOrder.locator(".flow-node")).toHaveCount(steps);
    });

    await test.step("OrderService.place expanded in place: following its validateOrder call connects that step to the new card", async () => {
      await clickOnCanvas(page, createOrder.getByRole("button", { name: "Expand OrderService.place here", exact: true }));
      const inset = createOrder.getByRole("region", { name: "OrderService.place, expanded in place", exact: true });
      await expect(inset.locator(".flow-node").first()).toBeVisible();
      await clickOnCanvas(page, inset.getByRole("button", { name: "Follow validateOrder", exact: true }));
      await expect(card(page, "#function:validateOrder")).toBeVisible();
      // Its own port on the inset's step, and the compiler arrow from it (apart from validateBody's).
      await expect(inset.locator('[data-handleid^="inset:"]')).toHaveCount(1);
      await expect(boardView(page).locator('.react-flow__edge[data-id*="|inset:"]')).toHaveCount(1);
    });

    await test.step("the emails worker's callback to withRetry expands in place to sendEmail's Flow, via the callback", async () => {
      await showHome(page);
      await traceEntrance(page, "WORKER", "emails");
      const expand = boardView(page).getByRole("button", { name: "Expand callback to withRetry here", exact: true });
      await clickOnCanvas(page, expand);
      const inset = boardView(page).getByRole("region", { name: "sendEmail, expanded in place", exact: true });
      await expect(inset).toBeVisible();
      await expect(inset.locator(".flow-inset-name")).toHaveText("sendEmail");
      await expect(inset.locator(".flow-inset-via")).toHaveText("via callback to withRetry");
      await expect(inset.getByRole("button", { name: "Collapse callback to withRetry here", exact: true })).toBeVisible();
      await expect(inset.locator(".flow-node").first()).toBeVisible();
    });
  });
});
