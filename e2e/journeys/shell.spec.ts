/**
 * Migration plan Step 10, the shell journeys: browse and outline, scope changes trigger updates,
 * search in every tab, follow a call from a source tab, add a definition with `⊕`, and Close Folder.
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import type { Locator, Page } from "@playwright/test";
import { expect, test } from "../src/fixtures";
import { CREATE_ORDER, openAndCompile, openPlace, PLACE } from "../src/flows";
import {
  activeBoard,
  card,
  editorTabs,
  eventMark,
  escapeRegExp,
  expectGraphCurrent,
  graphStatus,
  invoke,
  showView,
  waitForEvent,
  windowInfo,
} from "../src/window";

/** A row of a tree view by its exact label. */
function treeItem(tree: Locator, label: string): Locator {
  return tree.getByRole("treeitem").filter({ has: tree.page().locator(".tree-label", { hasText: new RegExp(`^${escapeRegExp(label)}$`) }) }).first();
}

/** Expands each folder of `path` in turn in the Files view and returns the last row. */
async function reveal(page: Page, path: readonly string[]): Promise<Locator> {
  await showView(page, "Files");
  const tree = page.getByRole("tree", { name: "Files" });
  let row = tree.getByRole("treeitem").first();
  for (const name of path) {
    row = treeItem(tree, name);
    await expect(row).toBeVisible();
    if ((await row.getAttribute("aria-expanded")) === "false") {
      await row.locator(".tree-twisty").click();
      await expect(row).toHaveAttribute("aria-expanded", "true");
    }
  }
  return row;
}

function searchField(page: Page): Locator {
  return page.locator(".search-view").getByRole("searchbox");
}

function results(page: Page): Locator {
  return page.getByRole("listbox", { name: "Search results" }).getByRole("option");
}

async function searchTab(page: Page, name: string): Promise<void> {
  await page.getByRole("tablist", { name: "Result kinds" }).getByRole("tab", { name: new RegExp(`^${name}`) }).click();
}

test.describe("the shell", () => {
  test("browse with outlines, and scope changes trigger an update", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    const app = await launch();
    const page = await app.firstWindow();
    await openAndCompile(app, page, folder);
    await expect(graphStatus(page)).toHaveText("Graph current · 34 files · TypeScript");

    // Expanding an indexed file shows its definitions; `→4` marks its four entrances.
    const orders = await reveal(page, ["src", "routes", "orders.ts"]);
    await expect(orders).toContainText("→4");
    const tree = page.getByRole("tree", { name: "Files" });
    for (const name of ["listOrders", "getOrder", "createOrder", "cancelOrder"]) {
      await expect(treeItem(tree, name)).toBeVisible();
    }
    const service = await reveal(page, ["services", "order-service.ts", "OrderService"]);
    await expect(service).toHaveAttribute("aria-expanded", "true");
    await expect(treeItem(tree, "place")).toBeVisible();

    // Clicking a definition opens its card; the tree marks it as on the board.
    await treeItem(tree, "createOrder").click();
    await expect(card(page, CREATE_ORDER)).toBeVisible();
    await expect(treeItem(tree, "createOrder").locator(".mark-dot-accent")).toHaveCount(1);

    // Exclude from graph, from the context menu: config.toml changes and an update follows.
    const mark = await eventMark(page, "graph_updated");
    const jobs = treeItem(tree, "jobs");
    await jobs.click({ button: "right" });
    await page.getByRole("menuitem", { name: "Exclude from graph" }).click();
    const update = await waitForEvent(page, "graph_updated", mark, (event) => Array.isArray(event["removedFiles"]) && event["removedFiles"].length > 0);
    expect(update.event["removedFiles"]).toEqual(expect.arrayContaining(["src/jobs/cleanup.ts", "src/jobs/report.ts"]));
    await expect(graphStatus(page)).toHaveText("Graph current · 28 files · TypeScript");
    expect(readFileSync(join(folder, ".unfold/config.toml"), "utf8")).toContain("src/jobs");
    await expect(jobs).toContainText("not indexed");

    // Include in graph puts them back.
    await jobs.click({ button: "right" });
    await page.getByRole("menuitem", { name: "Include in graph" }).click();
    await expect(graphStatus(page)).toHaveText("Graph current · 34 files · TypeScript");
  });

  test("search in every tab", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    const app = await launch();
    const page = await app.firstWindow();
    await openAndCompile(app, page, folder);
    await showView(page, "Search");
    await searchField(page).fill("place");

    await searchTab(page, "All");
    await expect(results(page).filter({ hasText: "OrderService.place" }).first()).toBeVisible();

    await searchTab(page, "Definitions");
    await expect(results(page).first()).toBeVisible();
    await results(page).filter({ hasText: "OrderService.place" }).first().click();
    await expect(card(page, PLACE)).toBeVisible();

    await searchField(page).fill("orders");
    await searchTab(page, "Entrances");
    const entrance = page.getByRole("listbox", { name: "Search results" }).getByRole("option", { name: /^POST \/orders / });
    await expect(entrance).toBeVisible();
    await entrance.click();
    await expect(activeBoard(page).locator(".entrance-card").first()).toBeVisible();

    await searchField(page).fill("order-service");
    await searchTab(page, "Files");
    const file = results(page).filter({ hasText: "order-service.ts" }).first();
    await expect(file).toBeVisible();
    await file.click();
    await expect(editorTabs(page).filter({ hasText: "order-service.ts" })).toHaveAttribute("aria-selected", "true");
    await expect(page.locator(".source-tab:not([hidden]) .source-path")).toHaveText("src/services/order-service.ts");

    await searchField(page).fill("orderService.place(");
    await searchTab(page, "Text");
    const match = results(page).filter({ has: page.locator(".result-match") }).first();
    await expect(match).toBeVisible();
    await expect(match).toContainText("orderService.place(draft)");
    await match.click();
    await expect(editorTabs(page).filter({ hasText: "orders.ts" }).first()).toHaveAttribute("aria-selected", "true");
    await expect(page.locator(".source-tab:not([hidden]) .source-row-flash")).toContainText("orderService.place(draft)");
  });

  test("follow a call from a source tab, and add a definition with ⊕", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    const app = await launch();
    const page = await app.firstWindow();
    await openAndCompile(app, page, folder);

    // Double-clicking a file opens a read-only source tab, current with the graph.
    const orders = await reveal(page, ["src", "routes", "orders.ts"]);
    await orders.dblclick();
    const source = page.locator(".source-tab:not([hidden])");
    await expect(source.locator(".source-path")).toHaveText("src/routes/orders.ts");
    await expect(source.locator(".source-graph")).toHaveText("Graph current");

    // ⊕ beside createOrder adds it to the board, and ● replaces the ⊕.
    await source.getByRole("button", { name: "Add createOrder to the board" }).click();
    await expect(card(page, CREATE_ORDER)).toBeVisible();
    await editorTabs(page).filter({ hasText: "orders.ts" }).first().click();
    await expect(source.getByRole("button", { name: "Add createOrder to the board" })).toHaveCount(0);
    await expect(source.locator(".source-on-board")).toBeVisible();

    // A call in the source tab is a link: following it opens the callee's card.
    const link = source.locator(".source-row", { hasText: "orderService.place(draft)" }).getByRole("link").filter({ hasText: "place" });
    await link.click();
    await expect(card(page, PLACE)).toBeVisible();
    await expect(card(page, PLACE).locator(".code-view")).toBeVisible();
    await expectGraphCurrent(page);
  });

  test("Close Folder returns the window to the welcome screen; another window on the folder keeps it", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    const app = await launch();
    const page = await app.firstWindow();
    await openAndCompile(app, page, folder);
    await openPlace(page);
    const second = await app.nextWindow(() => page.keyboard.press("Meta+Shift+N"));
    await expect(second.locator(".workbench")).toBeVisible();
    await expectGraphCurrent(second);

    // Close Folder from the command palette: the window stays open and shows the welcome screen.
    await page.keyboard.press("Meta+Shift+P");
    await page.getByPlaceholder("Type a command").fill("Close Folder");
    await page.keyboard.press("Enter");
    await expect(page.locator(".welcome")).toBeVisible();
    await expect(page.locator(".welcome-recent-open").first()).toBeVisible();
    expect((await windowInfo(page)).folder).toBeNull();
    expect(app.windows()).toHaveLength(2);

    // The other window still has the folder's workspace.
    await expect(second.locator(".workbench")).toBeVisible();
    await expectGraphCurrent(second);
    await expect(invoke(second, "graph_status")).resolves.toMatchObject({ state: { kind: "current" } });
  });
});
