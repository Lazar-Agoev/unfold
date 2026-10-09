/**
 * Migration plan Step 6, the reader journeys (the PoC's browser journeys, as desktop journeys):
 * 1. Open `POST /orders`, follow into `createOrder` and `OrderService.place`, switch Code and Flow,
 *    and go Back.
 * 2. Fit shows every card, zooming leaves card heights unchanged, and the top lens writes nothing.
 * 3. Label a Flow node, group lines, and fold a card; relaunch: the draft and the fold survived.
 * 4. Publish, and load a board written directly to disk (the sample board, healthy).
 */

import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import { expect, test } from "../src/fixtures";
import { CREATE_ORDER, PLACE } from "../src/flows";
import { boardsFixturesDir } from "../src/paths";
import {
  activeBoard,
  activeBoardId,
  cameraSettled,
  card,
  cards,
  enterText,
  expectCardsLoaded,
  expectGraphCurrent,
  firstCompile,
  follow,
  openEntrance,
  openFromWelcome,
  panUntilHittable,
  showView,
} from "../src/window";


interface BoardFile {
  readonly title: string;
  readonly cards: { readonly id: string; readonly ref: { readonly block?: string; readonly entrance?: string }; readonly view?: string; readonly folded?: boolean; readonly description?: { readonly text: string } }[];
  readonly labels?: Record<string, { readonly text: string }>;
  readonly groups: { readonly title: string; readonly card: string; readonly members: readonly string[] }[];
}

function readBoard(folder: string, boardId: string): BoardFile {
  return JSON.parse(readFileSync(join(folder, boardId), "utf8")) as BoardFile;
}

/** Card heights by card id, in CSS pixels (unscaled by the camera). */
function heights(page: Page): Promise<Record<string, number>> {
  return cards(page).evaluateAll((elements) => Object.fromEntries(elements.map((element) => [element.getAttribute("data-card-id") ?? "", (element as HTMLElement).offsetHeight])));
}

/** Whether every card lies inside the canvas's visible area. */
function everyCardInView(page: Page): Promise<boolean> {
  return activeBoard(page).evaluate((board) => {
    const pane = board.querySelector(".react-flow")?.getBoundingClientRect();
    if (!pane) {
      return false;
    }
    return [...board.querySelectorAll(".board-card")].every((element) => {
      const box = element.getBoundingClientRect();
      return box.left >= pane.left - 1 && box.right <= pane.right + 1 && box.top >= pane.top - 1 && box.bottom <= pane.bottom + 1;
    });
  });
}

async function focusCanvas(page: Page): Promise<void> {
  await activeBoard(page).locator(".react-flow__pane").click({ position: { x: 12, y: 12 } });
}

async function openInspector(page: Page): Promise<void> {
  if ((await page.locator("#workbench-inspector").count()) === 0) {
    await page.getByRole("button", { name: "Toggle Inspector" }).click();
  }
  await expect(page.locator("#workbench-inspector")).toBeVisible();
}

test.describe("the reader", () => {
  test("follow, Code and Flow, Back; Fit, zoom, and a lens that writes nothing", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    const app = await launch();
    const page = await app.firstWindow();
    await openFromWelcome(app, page, folder);
    await firstCompile(page);

    // 1. Open POST /orders, follow into createOrder and OrderService.place.
    await openEntrance(page, "POST", "/orders");
    await expect(card(page, CREATE_ORDER)).toBeVisible();
    await expectCardsLoaded(page);
    await follow(card(page, CREATE_ORDER), "place");
    const place = card(page, PLACE);
    await expect(place.locator(".code-view")).toBeVisible();
    await expect(page.locator(".board-trail li")).toHaveCount(3);
    const arrows = await activeBoard(page).locator(".react-flow__edge").count();
    expect(arrows, "an arrow for each opened call").toBeGreaterThanOrEqual(2);

    // The card's own switch: Flow, then Code again.
    const view = place.getByRole("radiogroup", { name: /^View of/ });
    await view.getByRole("radio", { name: "Flow" }).click();
    await expect(place.locator(".flow-view")).toBeVisible();
    await expect(place.locator(".flow-caption")).toHaveText("Paths from written structure");
    expect(await place.locator("[data-flow-node]").count()).toBeGreaterThan(2);
    await view.getByRole("radio", { name: "Code" }).click();
    await expect(place.locator(".code-view")).toBeVisible();

    // Back returns along the trail.
    const trailBefore = await page.locator(".board-trail li").allTextContents();
    await page.locator(".board-back").click();
    await expect(page.locator(".board-trail li")).not.toHaveText(trailBefore);
    await expect(page.locator(".board-trail li.current")).not.toContainText("place");

    // 2. Fit shows every card; zooming changes the camera, never a card's height.
    await focusCanvas(page);
    await page.getByRole("button", { name: "Fit Board" }).click();
    await expect.poll(() => everyCardInView(page)).toBe(true);
    const before = await heights(page);
    const transform = () => activeBoard(page).locator(".react-flow__viewport").getAttribute("style");
    const fitTransform = await transform();
    await page.getByRole("button", { name: "Zoom In" }).click();
    await page.getByRole("button", { name: "Zoom In" }).click();
    await expect.poll(transform).not.toBe(fitTransform);
    expect(await heights(page)).toEqual(before);
    await page.getByRole("button", { name: "Zoom Out" }).click();
    expect(await heights(page)).toEqual(before);
    await page.keyboard.press("Shift+Digit1");
    await expect.poll(() => everyCardInView(page)).toBe(true);

    // The top lens shows every card in Flow without editing the board. Proof: the next real edit
    // (a fold) saves the document, and it differs from the saved one by that fold alone.
    const boardId = await activeBoardId(page);
    const onCanvas = await cards(page).count();
    await expect.poll(() => existsSync(join(folder, boardId)) && readBoard(folder, boardId).cards.length === onCanvas).toBe(true);
    await expect(page.locator(".board-saving")).toHaveCount(0);
    const saved = readBoard(folder, boardId);
    const savedBytes = readFileSync(join(folder, boardId));
    const lens = page.getByRole("radiogroup", { name: "Lens" });
    await lens.getByRole("radio", { name: "Flow" }).click();
    await expect(activeBoard(page).locator(".block-card .flow-view")).toHaveCount(await activeBoard(page).locator(".block-card").count());
    expect(readFileSync(join(folder, boardId)).equals(savedBytes), "switching the lens writes nothing").toBe(true);
    const middleware = (await cards(page).evaluateAll((elements) => elements.map((element) => element.getAttribute("data-card-id")))).find((id) => id !== null && !saved.cards.find((entry) => entry.id === id)?.ref.entrance);
    expect(middleware).toBeDefined();
    await activeBoard(page).locator(`.board-card[data-card-id="${middleware ?? ""}"] .card-fold`).click();
    await expect.poll(() => readBoard(folder, boardId).cards.find((entry) => entry.id === middleware)?.folded ?? false).toBe(true);
    const after = readBoard(folder, boardId);
    expect(after.cards.map((entry) => ({ ...entry, folded: entry.id === middleware ? undefined : entry.folded }))).toEqual(
      saved.cards.map((entry) => ({ ...entry, folded: entry.id === middleware ? undefined : entry.folded })),
    );
    await lens.getByRole("radio", { name: "Code" }).click();
    await expect(place.locator(".code-view")).toBeVisible();
  });

  test("label a Flow node, group lines, fold a card; relaunch; publish; a board written to disk", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    let app = await launch();
    let page = await app.firstWindow();
    await openFromWelcome(app, page, folder);
    await firstCompile(page);
    await openEntrance(page, "POST", "/orders");
    await expect(card(page, CREATE_ORDER)).toBeVisible();
    await expectCardsLoaded(page);
    await follow(card(page, CREATE_ORDER), "place");
    await expect(card(page, PLACE).locator(".code-view")).toBeVisible();
    const boardId = await activeBoardId(page);

    // Label a Flow node: the Boards view steps to a node that still shows a code name.
    await openInspector(page);
    await showView(page, "Boards");
    const stepper = page.locator(".boards-unlabeled");
    await expect(stepper).toBeVisible();
    const unlabeledBefore = await stepper.textContent();
    await stepper.click();
    const labelField = page.getByRole("textbox", { name: "Flow label" });
    await expect(labelField).toBeVisible();
    await enterText(labelField, "Read the order draft");
    await expect(stepper).not.toHaveText(unlabeledBefore ?? "");
    await expect.poll(() => Object.values(readBoard(folder, boardId).labels ?? {}).map((label) => label.text)).toContain("Read the order draft");

    // Group lines from the gutter of createOrder (the synchronous placement, lines 32-34). The
    // outline focuses its card, so its gutter is on screen.
    await page.locator(".boards-outline-row", { hasText: "createOrder" }).first().click();
    const createOrder = card(page, CREATE_ORDER);
    await expect(createOrder).toHaveClass(/\bselected\b/);
    await cameraSettled(page);
    const createOrderView = createOrder.getByRole("radiogroup", { name: /^View of/ });
    if ((await createOrderView.getByRole("radio", { name: "Code" }).getAttribute("aria-checked")) !== "true") {
      await createOrderView.getByRole("radio", { name: "Code" }).click();
    }
    const numbers = createOrder.locator(".code-number");
    await panUntilHittable(page, numbers.filter({ hasText: /^34$/ }));
    await panUntilHittable(page, numbers.filter({ hasText: /^32$/ }));
    const from = await numbers.filter({ hasText: /^32$/ }).boundingBox();
    const to = await numbers.filter({ hasText: /^34$/ }).boundingBox();
    expect(from && to, "lines 32 and 34 of createOrder are drawn").toBeTruthy();
    if (from && to) {
      await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
      await page.mouse.down();
      await page.mouse.move(to.x + to.width / 2, to.y + to.height / 2, { steps: 6 });
      await page.mouse.up();
    }
    const rail = createOrder.locator(".code-group-input");
    await expect(rail).toBeVisible();
    await enterText(rail, "Place and acknowledge");
    await rail.press("Enter");
    await expect(createOrder.locator(".code-group-title")).toHaveText("Place and acknowledge");

    // Fold a card.
    const placeId = await card(page, PLACE).getAttribute("data-card-id");
    await card(page, PLACE).locator(".card-fold").click();
    await expect(card(page, PLACE)).toHaveClass(/\bfolded\b/);
    await expect
      .poll(() => {
        const document = readBoard(folder, boardId);
        return document.groups.some((group) => group.title === "Place and acknowledge") && document.cards.find((entry) => entry.id === placeId)?.folded === true;
      })
      .toBe(true);

    // Relaunch: the draft, its label, group, and fold survived.
    await app.quit();
    app = await launch();
    page = await app.firstWindow();
    await expect(page.locator(".workbench, .welcome").first()).toBeVisible();
    if (await page.locator(".welcome").isVisible()) {
      await openFromWelcome(app, page, folder);
    }
    await expectGraphCurrent(page);
    await expect(activeBoard(page).locator(`.board-tab[data-board-id="${boardId}"]`)).toBeVisible();
    await expect(card(page, PLACE)).toHaveClass(/\bfolded\b/);
    await expect(card(page, CREATE_ORDER).locator(".code-group-title")).toHaveText("Place and acknowledge");
    await page.getByRole("radiogroup", { name: "Lens" }).getByRole("radio", { name: "Flow" }).click();
    await expect(activeBoard(page).locator(".flow-label", { hasText: "Read the order draft" })).toHaveCount(1);
    await page.getByRole("radiogroup", { name: "Lens" }).getByRole("radio", { name: "Code" }).click();

    // Publish: the board is written to .unfold/boards/ and its draft removed.
    await page.locator(".titlebar-publish").click();
    const dialog = page.getByRole("dialog", { name: "Publish board" });
    await expect(dialog).toContainText(".unfold/boards/");
    await dialog.getByRole("button", { name: "Publish" }).click();
    await expect(page.locator(".titlebar .board-path")).toHaveText(/^\.unfold\/boards\/.+\.json$/);
    const publishedPath = (await page.locator(".titlebar .board-path").textContent()) ?? "";
    expect(existsSync(join(folder, publishedPath))).toBe(true);
    expect(existsSync(join(folder, boardId))).toBe(false);
    const published = readBoard(folder, publishedPath);
    expect(published.groups.map((group) => group.title)).toContain("Place and acknowledge");

    // A board written directly to disk appears, and loads with every description and label
    // attached and healthy.
    const sample = JSON.parse(readFileSync(join(boardsFixturesDir, "order-placement.json"), "utf8")) as BoardFile;
    mkdirSync(join(folder, ".unfold/boards"), { recursive: true });
    copyFileSync(join(boardsFixturesDir, "order-placement.json"), join(folder, ".unfold/boards/order-placement.json"));
    expect(readdirSync(join(folder, ".unfold/boards"))).toContain("order-placement.json");
    await showView(page, "Boards");
    const row = page.locator(".board-row", { hasText: sample.title });
    await expect(row).toBeVisible();
    await expect(row.locator(".board-label")).toHaveCount(0);
    await row.click();
    await expect(activeBoard(page).locator('.board-tab[data-board-id=".unfold/boards/order-placement.json"]')).toBeVisible();
    await expect(cards(page)).toHaveCount(sample.cards.length);
    await expectCardsLoaded(page);
    const described = sample.cards.filter((entry) => entry.description !== undefined);
    await expect(activeBoard(page).locator(".card-description")).toHaveCount(described.length);
    for (const entry of described) {
      await expect(activeBoard(page).locator(`.board-card[data-card-id="${entry.id}"] .card-description`)).toContainText(entry.description?.text.slice(0, 40) ?? "");
    }
    await expect(page.locator(".boards-issue")).toHaveCount(0);
    for (const group of sample.groups) {
      await expect(activeBoard(page).locator(".code-group-title", { hasText: group.title })).toHaveCount(1);
    }
    await page.getByRole("radiogroup", { name: "Lens" }).getByRole("radio", { name: "Flow" }).click();
    for (const label of Object.values(sample.labels ?? {})) {
      await expect(activeBoard(page).locator(".flow-label:not(.fallback)", { hasText: label.text }).first()).toBeVisible();
    }
  });
});
