/**
 * Reading a board (redesign §4.4): following calls and ⌥-click inline, On this page, Selected, the trail
 * with Back and Forward, the lens and a card's own view; pages, descriptions, groups, Flow labels, and
 * undo and redo across pages; Show callers, a drawn link, Arrange, the zoom and Fit, and the canvas and
 * card menus.
 */

import type { Page } from "@playwright/test";
import {
  boardView,
  callLink,
  camera,
  cameraSettled,
  card,
  cards,
  clickCanvas,
  dock,
  expectCardsLoaded,
  follow,
  layerRow,
  lens,
  onThisPage,
  pageRow,
  pageTitle,
  pagesList,
  selected,
  trail,
  zoomPercent,
} from "../src/board";
import { expect, test } from "../src/fixtures";
import { CREATE_ORDER, launchCompiled, openPlace, PLACE, POST_ORDERS, traceEntrance } from "../src/flows";
import { dialog, sidebar, toast } from "../src/home";
import { chooseOnCard, clickOnCanvas, dragCard, dragLines, expectSaved } from "../src/reader";
import { enterText } from "../src/window";

const REQUIRE_USER = "#function:requireUser";
const VALIDATE_BODY = "#function:validateBody";

/** Every block card on the active page shows `view` (`code-view` or `flow-view`). */
async function expectEveryBlockCardShows(page: Page, view: "code-view" | "flow-view"): Promise<void> {
  const blocks = boardView(page).locator(".block-card");
  const count = await blocks.count();
  expect(count).toBeGreaterThan(0);
  await expect(boardView(page).locator(`.block-card .${view}`)).toHaveCount(count);
}

test.describe("the board", () => {
  test("follow a call, On this page, Selected, the trail with Back and Forward, the lens, and ⌥-click inline", async ({ sandbox, launch }) => {
    const { page } = await launchCompiled(launch, sandbox);
    await traceEntrance(page, "POST", "/orders");

    await test.step("On this page lists the traced cards in follow order", async () => {
      await expect(onThisPage(page).locator(".layer-label")).toHaveText(["POST /orders", "requireUser", "validateBody", "createOrder"]);
      await expect(card(page, POST_ORDERS)).toHaveClass(/entrance-card/);
    });

    await test.step("following a call adds its card with an arrow, and the trail grows", async () => {
      const arrows = await boardView(page).locator(".react-flow__edge").count();
      await follow(card(page, CREATE_ORDER), "orderService.place");
      await expect(card(page, PLACE).locator(".code-view")).toBeVisible();
      await expect(boardView(page).locator(".react-flow__edge")).toHaveCount(arrows + 1);
      await expect(callLink(card(page, CREATE_ORDER), "orderService.place")).toHaveClass(/opened/);
      await expect(trail(page).last()).toHaveText("OrderService.place");
      await expect(onThisPage(page).locator(".layer-label")).toHaveText(["POST /orders", "requireUser", "validateBody", "createOrder", "OrderService.place"]);
    });

    await test.step("Selected shows the card's name, location, Called by, Reached from, and calls not opened", async () => {
      await clickOnCanvas(page, card(page, PLACE).locator(".card-title"));
      const region = selected(page);
      await expect(region.getByRole("heading", { name: "OrderService.place" })).toBeVisible();
      await expect(region.getByText("src/services/order-service.ts:13")).toBeVisible();
      await expect(region.locator(".relation-label", { hasText: "Called by" })).toBeVisible();
      await expect(region.locator(".relation-label", { hasText: "Reached from" })).toBeVisible();
      await expect(region.getByRole("button", { name: "POST /orders", exact: true })).toBeVisible();
      await expect(region.locator(".relation-label", { hasText: "Not opened" })).toBeVisible();
    });

    await test.step("+ Open in Selected opens a call that isn't on the page", async () => {
      const before = await cards(page).count();
      await selected(page).getByRole("button", { name: "Open validateOrder on the page" }).click();
      await expect(cards(page)).toHaveCount(before + 1);
      await expect(card(page, "#function:validateOrder")).toBeVisible();
      await expectCardsLoaded(page);
    });

    await test.step("a row of On this page brings its card into view and adds to the trail", async () => {
      await layerRow(page, "requireUser").click();
      await expect(layerRow(page, "requireUser")).toHaveAttribute("aria-selected", "true");
      await expect(card(page, REQUIRE_USER)).toHaveClass(/selected/);
      await expect(trail(page).last()).toHaveText("requireUser");
    });

    await test.step("Back and Forward in the dock move along the trail", async () => {
      const back = dock(page).getByRole("button", { name: "Back" });
      const forward = dock(page).getByRole("button", { name: "Forward" });
      await expect(forward).toBeDisabled();
      await back.click();
      await expect(forward).toBeEnabled();
      await forward.click();
      await expect(forward).toBeDisabled();
    });

    await test.step("the lens in the top bar shows every card in Code or Flow, and ⌘1 / ⌘2 switch it", async () => {
      await lens(page).getByRole("radio", { name: "Flow" }).click();
      await expectEveryBlockCardShows(page, "flow-view");
      await lens(page).getByRole("radio", { name: "Code" }).click();
      await expectEveryBlockCardShows(page, "code-view");
      await clickCanvas(page);
      await page.keyboard.press("Meta+2");
      await expect(lens(page).getByRole("radio", { name: "Flow" })).toHaveAttribute("aria-checked", "true");
      await expectEveryBlockCardShows(page, "flow-view");
      await page.keyboard.press("Meta+1");
      await expect(lens(page).getByRole("radio", { name: "Code" })).toHaveAttribute("aria-checked", "true");
      await expectEveryBlockCardShows(page, "code-view");
    });

    await test.step("a card's own Code | Flow switch changes only that card", async () => {
      const requireUser = card(page, REQUIRE_USER);
      await clickOnCanvas(page, requireUser.getByRole("radiogroup", { name: "View of requireUser" }).getByRole("radio", { name: "Flow" }));
      await expect(requireUser.locator(".flow-view")).toBeVisible();
      await expect(card(page, VALIDATE_BODY).locator(".code-view")).toBeVisible();
    });

    await test.step("⌥-click on a call expands its callee inline, and it closes again", async () => {
      const createOrder = card(page, CREATE_ORDER);
      const before = await cards(page).count();
      const link = callLink(createOrder, "slack.post");
      await clickOnCanvas(page, link, { modifiers: ["Alt"] });
      const inline = createOrder.locator(".code-inline");
      await expect(inline).toBeVisible();
      await expect(inline).toContainText("post");
      await expect(cards(page)).toHaveCount(before);
      await inline.getByRole("button", { name: "Close the expanded callee" }).click();
      await expect(inline).toHaveCount(0);
    });
  });

  test("pages, descriptions, groups, Flow labels, and undo and redo across pages", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await traceEntrance(page, "POST", "/orders");
    const createOrder = card(page, CREATE_ORDER);

    await test.step("a description written in Selected shows on the card and is saved in the draft", async () => {
      await clickOnCanvas(page, createOrder.locator(".card-title"));
      await expect(selected(page).getByRole("heading", { name: "createOrder" })).toBeVisible();
      await enterText(selected(page).getByRole("textbox", { name: "Description" }), "Places the order, or queues it.");
      await expect(createOrder.locator(".card-description")).toContainText("Places the order, or queues it.");
      await expectSaved(
        page,
        folder,
        (board) => board.pages[0]?.cards.some((item) => item.description?.text === "Places the order, or queues it.") ?? false,
        "the description is saved",
      );
    });

    await test.step("double-clicking the description on the card edits it in place", async () => {
      await createOrder.locator(".card-description p").dblclick();
      const field = createOrder.getByRole("textbox", { name: "Description" });
      await enterText(field, "Places the order.");
      await field.press("Enter");
      await expect(createOrder.locator(".card-description p")).toHaveText("Places the order.");
      await expect(selected(page).getByRole("textbox", { name: "Description" })).toHaveValue("Places the order.");
    });

    await test.step("dragging down line numbers makes a group, named on its rail", async () => {
      await dragLines(page, createOrder, 28, 30);
      const title = createOrder.getByRole("textbox", { name: "Group title" });
      await expect(title).toBeFocused();
      await title.pressSequentially("Queue the order");
      await title.press("Enter");
      await expect(createOrder.locator(".code-group-title")).toHaveText("Queue the order");
      await expect(layerRow(page, "Queue the order")).toBeVisible();
      await expectSaved(page, folder, (board) => board.pages[0]?.groups.some((group) => group.title === "Queue the order") ?? false, "the group is saved");
    });

    await test.step("a group collapses to its title and expands again", async () => {
      await createOrder.getByRole("button", { name: "Collapse Queue the order" }).click();
      await expect(createOrder.locator(".code-group.collapsed .code-fold-badge")).toHaveText("3 lines");
      await createOrder.getByRole("button", { name: "Expand Queue the order" }).click();
      await expect(createOrder.locator(".code-group.collapsed")).toHaveCount(0);
    });

    await test.step("a group selected from On this page shows in Selected, where Ungroup removes it", async () => {
      await layerRow(page, "Queue the order").click();
      await expect(selected(page).getByRole("textbox", { name: "Group title" })).toHaveValue("Queue the order");
      await selected(page).getByRole("button", { name: "Ungroup" }).click();
      await expect(createOrder.locator(".code-group-title")).toHaveCount(0);
      await expect(layerRow(page, "Queue the order")).toHaveCount(0);
    });

    await test.step("Label them steps to an unlabeled Flow node and focuses its label in Selected", async () => {
      const line = sidebar(page).locator(".board-unlabeled");
      const count = Number.parseInt((await line.textContent()) ?? "", 10);
      expect(count).toBeGreaterThan(0);
      await line.getByRole("button", { name: "Label them" }).click();
      const field = selected(page).getByRole("textbox", { name: "Flow label" });
      await expect(field).toBeFocused();
      await field.pressSequentially("Check who is asking");
      await expect(line).toContainText(`${count - 1} Flow step`);
      await lens(page).getByRole("radio", { name: "Flow" }).click();
      await expect(boardView(page).locator(".flow-label", { hasText: "Check who is asking" })).toBeVisible();
      await expectSaved(page, folder, (board) => board.pages[0]?.labels.some((label) => label.text === "Check who is asking") ?? false, "the label is saved");
      await lens(page).getByRole("radio", { name: "Code" }).click();
    });

    await test.step("+ New page adds an empty page, and a double-click renames it", async () => {
      await sidebar(page).getByRole("button", { name: "New page" }).click();
      await expect(pageRow(page, "Page 2")).toHaveAttribute("aria-selected", "true");
      await expect(boardView(page).getByText("Open an entrance or a definition to start this page.")).toBeVisible();
      await pageRow(page, "Page 2").dblclick();
      const field = pagesList(page).getByRole("textbox", { name: "Page title" });
      await field.fill("Refunds");
      await field.press("Enter");
      await expect(pageRow(page, "Refunds")).toBeVisible();
      await expect(pageTitle(page)).toHaveText("Refunds");
    });

    await test.step("switching pages shows each page's cards", async () => {
      await pageRow(page, "Page 1").click();
      await expect(pageTitle(page)).toHaveText("Page 1");
      await expect(card(page, CREATE_ORDER)).toBeVisible();
    });

    await test.step("the page menu duplicates a page and moves it", async () => {
      await pageRow(page, "Page 1").hover();
      await pagesList(page).getByRole("button", { name: "Page 1 actions" }).click();
      await page.getByRole("menu", { name: "Page 1 actions" }).getByRole("menuitem", { name: "Duplicate page" }).click();
      await expect(pagesList(page).getByRole("option")).toHaveText(["Page 1", "Page 1 copy", "Refunds"]);
      await pageRow(page, "Refunds").hover();
      await pagesList(page).getByRole("button", { name: "Refunds actions" }).click();
      await page.getByRole("menu", { name: "Refunds actions" }).getByRole("menuitem", { name: "Move up" }).click();
      await expect(pagesList(page).getByRole("option")).toHaveText(["Page 1", "Refunds", "Page 1 copy"]);
    });

    await test.step("undo and redo go back to the page an edit changed", async () => {
      await pageRow(page, "Page 1 copy").click();
      await expect(pageTitle(page)).toHaveText("Page 1 copy");
      await expectCardsLoaded(page);
      await chooseOnCard(page, card(page, REQUIRE_USER), "Remove from page");
      await expect(card(page, REQUIRE_USER)).toHaveCount(0);
      await pageRow(page, "Page 1").click();
      await expect(card(page, REQUIRE_USER)).toBeVisible();
      await page.keyboard.press("Meta+Z");
      await expect(pageTitle(page)).toHaveText("Page 1 copy");
      await expect(card(page, REQUIRE_USER)).toBeVisible();
      await pageRow(page, "Page 1").click();
      await page.keyboard.press("Meta+Shift+Z");
      await expect(pageTitle(page)).toHaveText("Page 1 copy");
      await expect(card(page, REQUIRE_USER)).toHaveCount(0);
    });

    await test.step("Delete page… asks first, then deletes the page", async () => {
      await pageRow(page, "Page 1 copy").hover();
      await pagesList(page).getByRole("button", { name: "Page 1 copy actions" }).click();
      await page.getByRole("menu", { name: "Page 1 copy actions" }).getByRole("menuitem", { name: "Delete page…" }).click();
      const confirm = dialog(page, "Delete “Page 1 copy”?");
      await expect(confirm).toBeVisible();
      await confirm.getByRole("button", { name: "Delete page" }).click();
      await expect(pagesList(page).getByRole("option")).toHaveText(["Page 1", "Refunds"]);
      await expectSaved(page, folder, (board) => board.pages.map((item) => item.title).join() === "Page 1,Refunds", "the pages are saved");
    });
  });

  test("Show callers, a drawn link, Arrange, the zoom and Fit, and the canvas and card menus", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await openPlace(page);
    await expectCardsLoaded(page);

    await test.step("Show callers on a card adds the cards that call it", async () => {
      await chooseOnCard(page, card(page, PLACE), "Show callers");
      await expect(card(page, "#function:importOrders")).toBeVisible();
      await expect(card(page, "#function:processOrder")).toBeVisible();
      await expectCardsLoaded(page);
    });

    await test.step("Draw link… joins two cards with a drawn link, labeled in Selected", async () => {
      await chooseOnCard(page, card(page, REQUIRE_USER), "Draw link…");
      await expect(boardView(page).getByRole("status").filter({ hasText: "Click the card to link to" })).toBeVisible();
      await clickOnCanvas(page, card(page, VALIDATE_BODY).locator(".card-title"));
      await expect(boardView(page).locator(".board-link")).toHaveCount(1);
      await expect(selected(page).getByRole("heading", { name: "Link" })).toBeVisible();
      const label = selected(page).getByRole("textbox", { name: "Label" });
      await expect(label).toBeFocused();
      await label.pressSequentially("then checks the body");
      await expect(boardView(page).locator(".board-link-label")).toContainText("then checks the body");
      await expect(layerRow(page, "then checks the body")).toBeVisible();
      await expectSaved(page, folder, (board) => board.pages[0]?.links?.some((link) => link.label?.text === "then checks the body") ?? false, "the link is saved");
    });

    await test.step("Remove link in Selected removes it", async () => {
      await selected(page).getByRole("button", { name: "Remove link" }).click();
      await expect(boardView(page).locator(".board-link")).toHaveCount(0);
    });

    await test.step("Arrange orders the cards by call depth, with Undo in its toast", async () => {
      await dragCard(page, card(page, REQUIRE_USER), 0, 260);
      await dock(page).getByRole("button", { name: "Arrange" }).click();
      const arranged = toast(page, /^Arranged \d+ cards? by call depth\./);
      await expect(arranged).toBeVisible();
      await arranged.getByRole("button", { name: "Undo" }).click();
      await expect(arranged).toHaveCount(0);
    });

    await test.step("the dock's zoom buttons change the zoom, and Fit returns to the whole page", async () => {
      // A button moves the camera with an animation that starts on the next render: wait for it to
      // start (the transform changes) where it must, then for it to end.
      const press = async (name: string, moves: boolean) => {
        const before = await camera(page);
        await dock(page).getByRole("button", { name, exact: true }).click();
        if (moves) {
          await expect.poll(() => camera(page)).not.toBe(before);
        }
        await cameraSettled(page);
        return zoomPercent(page);
      };
      await press("Zoom in", true);
      const fitted = await press("Fit", true);
      const zoomed = await press("Zoom in", true);
      expect(zoomed).toBeGreaterThan(fitted);
      expect(await press("Zoom out", true)).toBeLessThan(zoomed);
      await press("Zoom in", true);
      expect(await press("Fit", true)).toBe(fitted);
    });

    await test.step("the canvas menu folds and unfolds every card", async () => {
      await boardView(page).locator(".react-flow__pane").click({ button: "right", position: { x: 12, y: 12 } });
      await page.getByRole("menu", { name: "Board actions" }).getByRole("menuitem", { name: "Fold all cards", exact: true }).click();
      const total = await cards(page).count();
      await expect(boardView(page).locator(".board-card.folded")).toHaveCount(total);
      await boardView(page).locator(".react-flow__pane").click({ button: "right", position: { x: 12, y: 12 } });
      await page.getByRole("menu", { name: "Board actions" }).getByRole("menuitem", { name: "Unfold all cards", exact: true }).click();
      await expect(boardView(page).locator(".board-card.folded")).toHaveCount(0);
    });

    await test.step("the card menu folds a card and removes it from the page", async () => {
      await chooseOnCard(page, card(page, PLACE), /^Fold card$/);
      await expect(card(page, PLACE)).toHaveClass(/folded/);
      await chooseOnCard(page, card(page, PLACE), /^Unfold card$/);
      await expect(card(page, PLACE)).not.toHaveClass(/folded/);
      await chooseOnCard(page, card(page, VALIDATE_BODY), "Remove from page");
      await expect(card(page, VALIDATE_BODY)).toHaveCount(0);
      await expect(layerRow(page, "validateBody")).toHaveCount(0);
    });
  });
});
