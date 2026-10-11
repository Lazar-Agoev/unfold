/**
 * A board's life outside publishing (architecture §11.8, decision U-47): renaming in place,
 * the board menu (Duplicate, Share as Mermaid and PNG, Fold all, Delete), folders and Move to folder, and edits
 * made to a board's file outside the app (a clean board reloads; a deleted file shows the banner), and a
 * save that fails (the banner's Retry).
 */

import { chmodSync, existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { activeBoardRef, boardMenu, boardTitle, boardView, cards, chooseMenuPath, layerRow, menuItem, readBoard, type BoardFile } from "../src/board";
import { expect, test } from "../src/fixtures";
import { launchCompiled, traceEntrance } from "../src/flows";
import { boardTab, dialog, showHome, sidebar, sidebarBoard, sidebarFolder, tabs, toast } from "../src/home";
import { describeSelected, DRAFTS, expectBoardFile, firstDescription } from "../src/lifecycle";
import { enterText } from "../src/window";

test.describe("boards", () => {
  test("the board menu: rename in place, Duplicate, Share as Mermaid, Fold all, and Delete", async ({ sandbox, launch }) => {
    const { app, page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await traceEntrance(page, "POST", "/orders");
    const { id } = await activeBoardRef(page);

    await test.step("double-clicking the title in the sidebar header renames the board in place", async () => {
      await boardTitle(page).dblclick();
      const field = sidebar(page).getByRole("textbox", { name: "Board title" });
      await enterText(field, "Placing an order");
      await field.press("Enter");
      await expect(boardTitle(page)).toHaveText("Placing an order");
      await expect(boardTab(page, "Placing an order")).toHaveAttribute("aria-selected", "true");
      await expectBoardFile(folder, id, (board) => board.title === "Placing an order", "the new title is saved");
    });

    await test.step("Rename… in the board menu renames it in the header too", async () => {
      await boardMenu(page, "Rename…");
      const field = sidebar(page).getByRole("textbox", { name: "Board title" });
      await expect(field).toBeFocused();
      await enterText(field, "Orders");
      await field.press("Enter");
      await expect(boardTitle(page)).toHaveText("Orders");
      await expectBoardFile(folder, id, (board) => board.title === "Orders", "the new title is saved");
    });

    await test.step("Duplicate makes a draft copy, which opens from the toast", async () => {
      await boardMenu(page, "Duplicate");
      const made = toast(page, "Made Orders copy");
      await made.getByRole("button", { name: "Open" }).click();
      await expect(boardTab(page, "Orders copy")).toHaveAttribute("aria-selected", "true");
      const copy = await activeBoardRef(page);
      expect(copy.id).not.toBe(id);
      expect(readBoard(folder, copy.id).title).toBe("Orders copy");
      await boardTab(page, "Orders").click();
      await expect(boardTitle(page)).toHaveText("Orders");
    });

    await test.step("Share › Export Mermaid… writes the page through the save dialog", async () => {
      const file = join(sandbox.root, "orders.mmd");
      await app.stubSaveDialog(file);
      await boardMenu(page, "Share", "Export Mermaid…");
      await expect(toast(page, "Exported Mermaid")).toBeVisible();
      expect(readFileSync(file, "utf8")).toMatch(/^flowchart /);
    });

    await test.step("Share › Copy Mermaid puts the same page on the clipboard", async () => {
      await boardMenu(page, "Share", "Copy Mermaid");
      await expect(toast(page, "Copied the page as Mermaid")).toBeVisible();
      expect(await app.clipboardText()).toBe(readFileSync(join(sandbox.root, "orders.mmd"), "utf8"));
    });

    await test.step("Share › Export PNG… draws the page in the render window and writes it through the save dialog", async () => {
      const file = join(sandbox.root, "orders.png");
      await app.stubSaveDialog(file);
      await boardMenu(page, "Share", "Export PNG…");
      await expect(toast(page, "Exported PNG")).toBeVisible({ timeout: 30_000 });
      // A PNG's signature, then its header's width and height.
      const png = readFileSync(file);
      expect([...png.subarray(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
      expect(png.readUInt32BE(16)).toBeGreaterThan(100);
      expect(png.readUInt32BE(20)).toBeGreaterThan(100);
    });

    await test.step("Fold all cards and Unfold all cards", async () => {
      const count = await cards(page).count();
      await boardMenu(page, /^Fold all cards$/);
      await expect(boardView(page).locator(".board-card.folded")).toHaveCount(count);
      await expectBoardFile(folder, id, (board) => board.pages[0]?.cards.every((card) => card.folded === true) ?? false, "the cards are saved folded");
      await boardMenu(page, /^Unfold all cards$/);
      await expect(boardView(page).locator(".board-card.folded")).toHaveCount(0);
    });

    await test.step("Delete… asks, then deletes the file and closes the tab", async () => {
      await boardMenu(page, "Delete…");
      const confirm = dialog(page, "Delete “Orders”?");
      await confirm.getByRole("button", { name: "Delete", exact: true }).click();
      await expect(toast(page, "Deleted Orders")).toBeVisible();
      await expect(boardTab(page, "Orders")).toHaveCount(0);
      await expect(tabs(page)).toHaveCount(2);
      expect(existsSync(join(folder, id))).toBe(false);
    });
  });

  test("folders: Move to folder, the sidebar tree, and the folder menu", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await traceEntrance(page, "POST", "/orders");
    const { id } = await activeBoardRef(page);

    await test.step("Move to folder › New folder… moves the board into a new folder", async () => {
      await boardMenu(page, "Move to folder", "New folder…");
      const prompt = dialog(page, "New folder");
      await enterText(prompt.getByRole("textbox", { name: "Name" }), "flows");
      await prompt.getByRole("button", { name: "Move" }).click();
      await expect(prompt).toBeHidden();
      await expect(sidebar(page).locator(".board-crumb-text")).toHaveText("ts-orders / flows");
      await expectBoardFile(folder, id, (board) => board.folder === "flows", "the folder is saved");
    });

    await test.step("the sidebar tree shows the folder with the board inside", async () => {
      await showHome(page);
      const flows = sidebarFolder(page, "flows");
      await expect(flows).toBeVisible();
      if ((await flows.getAttribute("aria-expanded")) !== "true") {
        await flows.click();
      }
      await expect(flows).toHaveAttribute("aria-expanded", "true");
      await expect(sidebarBoard(page, "POST /orders")).toHaveAttribute("aria-level", "3");
    });

    await test.step("the folder menu's New folder… makes an empty folder inside, which Delete removes", async () => {
      await sidebarFolder(page, "flows").click({ button: "right" });
      await chooseMenuPath(page, "Sidebar menu", ["New Folder…"]);
      const prompt = dialog(page, "New folder");
      await expect(prompt).toContainText("Inside flows.");
      await enterText(prompt.getByRole("textbox", { name: "Name" }), "later");
      await prompt.getByRole("button", { name: "Create" }).click();
      await expect(sidebarFolder(page, "later")).toBeVisible();
      await sidebarFolder(page, "later").click({ button: "right" });
      await chooseMenuPath(page, "Sidebar menu", ["Delete"]);
      await expect(sidebarFolder(page, "later")).toHaveCount(0);
    });

    await test.step("a folder that holds a board can't be deleted", async () => {
      await sidebarFolder(page, "flows").click({ button: "right" });
      const menu = page.getByRole("menu", { name: "Sidebar menu" });
      await expect(menuItem(menu, "Delete")).toHaveAttribute("aria-disabled", "true");
      await page.keyboard.press("Escape");
      await expect(menu).toBeHidden();
    });

    await test.step("Rename… on a folder moves its boards with it", async () => {
      await sidebarFolder(page, "flows").click({ button: "right" });
      await chooseMenuPath(page, "Sidebar menu", ["Rename…"]);
      const prompt = dialog(page, "Rename folder");
      await enterText(prompt.getByRole("textbox", { name: "Name" }), "journeys");
      await prompt.getByRole("button", { name: "Rename" }).click();
      await expect(sidebarFolder(page, "journeys")).toBeVisible();
      await expect(sidebarFolder(page, "flows")).toHaveCount(0);
      await expectBoardFile(folder, id, (board) => board.folder === "journeys", "the board moved with its folder");
    });

    await test.step("Move to folder › Top level moves the board out again", async () => {
      await boardTab(page, "POST /orders").click();
      await expect(sidebar(page).locator(".board-crumb-text")).toHaveText("ts-orders / journeys");
      await boardMenu(page, "Move to folder", "Top level");
      await expectBoardFile(folder, id, (board) => board.folder === undefined, "the board is at the top level");
      await expect(sidebar(page).locator(".board-crumb-text")).toHaveText("ts-orders");
      await showHome(page);
      await expect(sidebarBoard(page, "POST /orders")).toHaveAttribute("aria-level", "2");
    });
  });

  test("a board's file edited outside the app: a clean board reloads, a deleted file shows the banner; a failed save offers Retry", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await traceEntrance(page, "POST", "/orders");
    const { id } = await activeBoardRef(page);
    // Wait for the trace's cards to be saved: an outside edit then meets a board without unsaved changes.
    await expectBoardFile(folder, id, (board) => board.pages[0]?.cards.length === 4, "the traced cards are saved");

    await test.step("an outside edit to a board without unsaved changes reloads it", async () => {
      const board = JSON.parse(readFileSync(join(folder, id), "utf8")) as BoardFile & Record<string, unknown>;
      writeFileSync(join(folder, id), `${JSON.stringify({ ...board, title: "Edited elsewhere" }, null, 2)}\n`);
      await expect(boardTitle(page)).toHaveText("Edited elsewhere");
      await expect(boardTab(page, "Edited elsewhere")).toBeVisible();
    });

    await test.step("deleting the open board's file shows the banner at the top of the canvas", async () => {
      rmSync(join(folder, id));
      const banner = boardView(page).getByRole("alert").filter({ hasText: "This board's file was deleted outside Unfold" });
      await expect(banner).toBeVisible();
      await expect(banner.getByRole("button", { name: "Close the board" })).toBeVisible();
    });

    await test.step("Keep mine on the banner writes the board again", async () => {
      const banner = boardView(page).getByRole("alert");
      await banner.getByRole("button", { name: "Keep mine (write it again)" }).click();
      await expect(banner).toBeHidden();
      await expectBoardFile(folder, id, (board) => board.title === "Edited elsewhere", "the board is written again");
    });

    await test.step("a save that fails shows Not saved with Retry, and Retry saves once it can", async () => {
      const drafts = join(folder, DRAFTS);
      chmodSync(drafts, 0o555);
      try {
        await layerRow(page, "POST /orders").click();
        await describeSelected(page, "Checks the user, then places the order.");
        await expect(boardView(page).getByRole("status").filter({ hasText: /^Not saved:/ })).toBeVisible();
      } finally {
        chmodSync(drafts, 0o755);
      }
      const banner = boardView(page).getByRole("status").filter({ hasText: /^Not saved:/ });
      await banner.getByRole("button", { name: "Retry" }).click();
      await expect(banner).toBeHidden();
      await expectBoardFile(folder, id, (board) => firstDescription(board) === "Checks the user, then places the order.", "the description is saved");
    });
  });
});
