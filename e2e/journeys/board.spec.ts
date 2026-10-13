/**
 * Reading a board (docs/reader.md#boards): following calls and ⌥-click inline, Selected, the Diagram | Code lens
 * and a card's expansion; pages, descriptions, Flow labels, and undo and redo across pages; an arrow's call in
 * Selected, Show callers, a drawn link, Arrange, the zoom and Fit, and the canvas and card menus.
 */

import type { Locator, Page } from "@playwright/test";
import {
  arrow,
  boardView,
  callLink,
  camera,
  cameraSettled,
  card,
  cards,
  chooseLens,
  clickArrow,
  clickCanvas,
  dock,
  expectCardsLoaded,
  follow,
  pageRow,
  pageTitle,
  pagesList,
  selected,
  zoomPercent,
} from "../src/board";
import { expect, test } from "../src/fixtures";
import { CREATE_ORDER, launchCompiled, openPlace, PLACE, POST_ORDERS, traceEntrance } from "../src/flows";
import { dialog, sidebar, toast } from "../src/home";
import { chooseOnCard, clickOnCanvas, dragCard, expectSaved } from "../src/reader";
import { enterText } from "../src/window";

const REQUIRE_USER = "#function:requireUser";
const VALIDATE_BODY = "#function:validateBody";
const VALIDATE_ORDER = "#function:validateOrder";

/** Every block card on the active page shows `view` (`code-view` or `flow-view`). */
async function expectEveryBlockCardShows(page: Page, view: "code-view" | "flow-view"): Promise<void> {
  const blocks = boardView(page).locator(".block-card");
  const count = await blocks.count();
  expect(count).toBeGreaterThan(0);
  await expect(boardView(page).locator(`.block-card .${view}`)).toHaveCount(count);
}

/** Selected's label and value rows, as their two columns' text. */
async function relationRows(region: Locator): Promise<(string | null)[][]> {
  return region.locator(".relation-row").evaluateAll((rows) => rows.map((row) => [...row.children].map((cell) => cell.textContent)));
}

test.describe("the board", () => {
  test("follow a call, Selected, the lens, and ⌥-click inline", async ({ sandbox, launch }) => {
    const { page } = await launchCompiled(launch, sandbox);
    await traceEntrance(page, "POST", "/orders");

    await test.step("the traced handlers stand in a column beside the entrance, the first level with its row", async () => {
      const geometry = await boardView(page).evaluate((view, key) => {
        const viewport = view.querySelector<HTMLElement>(".react-flow__viewport");
        const entrance = view.querySelector(".entrance-card");
        const row = entrance?.querySelector(`.ecard-handler[data-target-key$='${key}']`)?.getBoundingClientRect();
        const handler = view.querySelector(`.block-card[data-key$='${key}']`)?.getBoundingClientRect();
        if (!viewport || !entrance || !row || !handler) {
          return null;
        }
        const zoom = new DOMMatrix(getComputedStyle(viewport).transform).a;
        const right = entrance.getBoundingClientRect().right;
        const columns = [...view.querySelectorAll(".block-card")].map((card) => (card.getBoundingClientRect().left - right) / zoom);
        return { columns, rise: (handler.top - (row.top + row.height / 2)) / zoom };
      }, REQUIRE_USER);
      expect(geometry?.columns).toHaveLength(3);
      // FOLLOW_GAP (ui/src/canvas/layout.ts) to the right of the entrance card.
      for (const gap of geometry?.columns ?? []) {
        expect(Math.abs(gap - 80)).toBeLessThan(1);
      }
      // The handler's header center (HEADER_PORT, 22 px below its top) is level with its row's center.
      expect(Math.abs((geometry?.rise ?? Number.NaN) + 22)).toBeLessThan(2);
    });

    await test.step("following a call adds its card with an arrow", async () => {
      await expect(card(page, POST_ORDERS)).toHaveClass(/entrance-card/);
      const arrows = await boardView(page).locator(".react-flow__edge").count();
      // The traced createOrder shows its Flow: its `→ OrderService.place` follows the call.
      await follow(card(page, CREATE_ORDER), "OrderService.place");
      // A followed card takes the new page's Diagram default: a compact node, once its model is in.
      await expect(card(page, PLACE)).toHaveClass(/diagram-card/);
      await expectCardsLoaded(page);
      await expect(card(page, PLACE).locator(".card-location")).toHaveText("services/order-service.ts:13");
      await expect(boardView(page).locator(".react-flow__edge")).toHaveCount(arrows + 1);
    });

    await test.step("Selected shows the card's name, location, Called by, and Reached from", async () => {
      await clickOnCanvas(page, card(page, PLACE).locator(".card-title"));
      const region = selected(page);
      await expect(region.getByRole("heading", { name: "OrderService.place" })).toBeVisible();
      await expect(region.getByText("src/services/order-service.ts:13")).toBeVisible();
      await expect(region.locator(".relation-label", { hasText: "Called by" })).toBeVisible();
      await expect(region.locator(".relation-label", { hasText: "Reached from" })).toBeVisible();
      await expect(region.getByRole("button", { name: "POST /orders", exact: true })).toBeVisible();
    });

    await test.step("the lens in the top bar switches the page between Diagram and Code", async () => {
      await chooseLens(page, "Code");
      await expectEveryBlockCardShows(page, "code-view");
      // In Code, the call followed from createOrder's Flow is an opened link: its card is on the page.
      await expect(callLink(card(page, CREATE_ORDER), "orderService.place")).toHaveClass(/opened/);
      await chooseLens(page, "Diagram");
      await expect(boardView(page).locator(".block-card .code-view")).toHaveCount(0);
    });

    await test.step("a card's chevron expands and collapses only that card in Diagram, and it survives a trip through Code", async () => {
      const requireUser = card(page, REQUIRE_USER);
      // Tracing opened the handlers expanded (their written Flow); the followed OrderService.place is compact.
      await expect(requireUser.locator(".flow-view")).toBeVisible();
      await expect(card(page, PLACE)).toHaveClass(/diagram-card/);
      await clickOnCanvas(page, requireUser.getByRole("button", { name: "Collapse requireUser" }));
      await expect(requireUser).toHaveClass(/diagram-card/);
      await expect(card(page, VALIDATE_BODY).locator(".flow-view")).toBeVisible();
      await chooseLens(page, "Code");
      await expectEveryBlockCardShows(page, "code-view");
      await chooseLens(page, "Diagram");
      await expect(requireUser).toHaveClass(/diagram-card/);
      await expect(card(page, VALIDATE_BODY).locator(".flow-view")).toBeVisible();
      await clickOnCanvas(page, requireUser.getByRole("button", { name: "Expand requireUser" }));
      await expect(requireUser.locator(".flow-view")).toBeVisible();
      // Back to Code for reading the source inline.
      await chooseLens(page, "Code");
      await expectEveryBlockCardShows(page, "code-view");
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

  test("pages, descriptions, Flow labels, and undo and redo across pages", async ({ sandbox, launch }) => {
    const { app, page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await traceEntrance(page, "POST", "/orders");
    const createOrder = card(page, CREATE_ORDER);

    await test.step("a description written in Selected shows on the card and is saved in the board's file", async () => {
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

    await test.step("a Flow step's label written in Selected shows in its Flow and is saved", async () => {
      // The traced cards are expanded: createOrder shows its Flow.
      await clickOnCanvas(page, createOrder.locator(".flow-node.kind-condition .flow-label"));
      await enterText(selected(page).getByRole("textbox", { name: "Flow label" }), "Did the client ask for an async reply?");
      await expect(createOrder.locator(".flow-node.kind-condition .flow-label")).toHaveText("Did the client ask for an async reply?");
      await expectSaved(page, folder, (board) => board.pages[0]?.labels.some((label) => label.text === "Did the client ask for an async reply?") ?? false, "the label is saved");
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

    await test.step("the page menu holds Rename, Copy link, Move up, Move down, and Delete page…, and moves a page", async () => {
      await pageRow(page, "Refunds").hover();
      await pagesList(page).getByRole("button", { name: "Refunds actions" }).click();
      const menu = page.getByRole("menu", { name: "Refunds actions" });
      await expect(menu.getByRole("menuitem")).toHaveText(["Rename", "Copy link", "Move up", "Move down", "Delete page…"]);
      await menu.getByRole("menuitem", { name: "Move up" }).click();
      await expect(pagesList(page).getByRole("option")).toHaveText(["Refunds", "Page 1"]);
      await expectSaved(page, folder, (board) => board.pages.map((item) => item.title).join() === "Refunds,Page 1", "the page order is saved");
    });

    await test.step("undo and redo go back to the page an edit changed", async () => {
      await pageRow(page, "Page 1").click();
      await expect(pageTitle(page)).toHaveText("Page 1");
      await expectCardsLoaded(page);
      await chooseOnCard(page, card(page, REQUIRE_USER), "Remove from page");
      await expect(card(page, REQUIRE_USER)).toHaveCount(0);
      await pageRow(page, "Refunds").click();
      await expect(pageTitle(page)).toHaveText("Refunds");
      await app.clickMenu(page, ["Edit", "Undo"]);
      await expect(pageTitle(page)).toHaveText("Page 1");
      await expect(card(page, REQUIRE_USER)).toBeVisible();
      await pageRow(page, "Refunds").click();
      await expect(pageTitle(page)).toHaveText("Refunds");
      await app.clickMenu(page, ["Edit", "Redo"]);
      await expect(pageTitle(page)).toHaveText("Page 1");
      await expect(card(page, REQUIRE_USER)).toHaveCount(0);
    });

    await test.step("Delete page… asks first, then deletes the page", async () => {
      await pageRow(page, "Refunds").hover();
      await pagesList(page).getByRole("button", { name: "Refunds actions" }).click();
      await page.getByRole("menu", { name: "Refunds actions" }).getByRole("menuitem", { name: "Delete page…" }).click();
      const confirm = dialog(page, "Delete “Refunds”?");
      await expect(confirm).toBeVisible();
      await confirm.getByRole("button", { name: "Delete page" }).click();
      await expect(pagesList(page).getByRole("option")).toHaveText(["Page 1"]);
      await expectSaved(page, folder, (board) => board.pages.map((item) => item.title).join() === "Page 1", "the pages are saved");
    });
  });

  test("an arrow's call in Selected, Show callers, a drawn link, Arrange, the zoom and Fit, and the canvas and card menus", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await openPlace(page);
    await expectCardsLoaded(page);

    await test.step("clicking an arrow shows its call in Selected: what it sends, what comes back, and what it throws", async () => {
      // OrderService.place reads in Code: following its call adds validateOrder with an arrow.
      await follow(card(page, PLACE), "validateOrder");
      await expect(card(page, VALIDATE_ORDER)).toBeVisible();
      await expectCardsLoaded(page);
      const line = await arrow(card(page, PLACE), card(page, VALIDATE_ORDER));
      await expect(line).toHaveAccessibleName("Call");
      await clickArrow(page, line);
      const region = selected(page);
      await expect(region.getByRole("heading", { name: "validateOrder(input)" })).toBeVisible();
      await expect(region.getByText("Call · line 14")).toBeVisible();
      // validateOrder's own throws (not those of the callback it passes to map), none caught here.
      await expect
        .poll(() => relationRows(region))
        .toEqual([
          ["Sends", "input→input"],
          ["Returns", "OrderDraft→draft"],
          ["Throws", "Error"],
          ["", "Error"],
          ["", "Error"],
          ["", "Error"],
        ]);
    });

    await test.step("Escape lets go of the arrow, and so does a click on the canvas", async () => {
      const line = await arrow(card(page, PLACE), card(page, VALIDATE_ORDER));
      await page.keyboard.press("Escape");
      await expect(line).toHaveAttribute("aria-pressed", "false");
      await expect(selected(page).getByText(/^Select a card/)).toBeVisible();
      await clickArrow(page, line);
      await clickCanvas(page);
      await expect(line).toHaveAttribute("aria-pressed", "false");
      await expect(selected(page).getByText(/^Select a card/)).toBeVisible();
    });

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

    await test.step("the dock's zoom buttons change the zoom, and Fit in the dock or the canvas menu returns to the whole page", async () => {
      // A button moves the camera with an animation that starts on the next render: wait for it to
      // start (the transform changes) where it must, then for it to end.
      const moved = async (act: () => Promise<void>) => {
        const before = await camera(page);
        await act();
        await expect.poll(() => camera(page)).not.toBe(before);
        await cameraSettled(page);
        return zoomPercent(page);
      };
      const press = (name: string) =>
        moved(async () => {
          await dock(page).getByRole("button", { name, exact: true }).click();
        });
      await press("Zoom in");
      const fitted = await press("Fit");
      const zoomed = await press("Zoom in");
      expect(zoomed).toBeGreaterThan(fitted);
      expect(await press("Zoom out")).toBeLessThan(zoomed);
      await press("Zoom in");
      expect(await press("Fit")).toBe(fitted);
      await press("Zoom in");
      const fit = await moved(async () => {
        await boardView(page).locator(".react-flow__pane").click({ button: "right", position: { x: 12, y: 12 } });
        await page.getByRole("menu", { name: "Board actions" }).getByRole("menuitem", { name: "Fit", exact: true }).click();
      });
      expect(fit).toBe(fitted);
    });

    await test.step("the card menu removes a card from the page", async () => {
      await chooseOnCard(page, card(page, VALIDATE_BODY), "Remove from page");
      await expect(card(page, VALIDATE_BODY)).toHaveCount(0);
    });
  });
});
