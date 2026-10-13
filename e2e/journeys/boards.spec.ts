/**
 * Boards are files (docs/reader.md#boards, board folder metadata): renaming in place, the board menu (Rename…,
 * Move to folder, Export PNG…, Copy link, Delete…), one-level folders through Move to folder, and the board's
 * file changed outside the app (the board reloads and its undo history starts over; a deleted file is written
 * back), and a save that fails ("Not saved · Retry").
 */

import { chmodSync, existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { boardMenu, boardTitle, boardView, card, chooseMenuPath, clickCanvas, layerRow, menuItem, showBoard, shownBoardRef, type BoardFile } from "../src/board";
import { expect, test } from "../src/fixtures";
import { launchCompiled, traceEntrance } from "../src/flows";
import { dialog, homeBoardRows, homeMain, showHome, sidebar, sidebarBoard, sidebarFolder, toast } from "../src/home";
import { BOARDS, describeSelected, expectBoardFile, firstDescription } from "../src/lifecycle";
import { enterText, eventMark, waitForEvent } from "../src/window";

/** Rewrites a board's file the way another program does, with `change` applied to what it holds. */
function editOutside(folder: string, id: string, change: Partial<BoardFile>): void {
  const board = JSON.parse(readFileSync(join(folder, id), "utf8")) as BoardFile & Record<string, unknown>;
  writeFileSync(join(folder, id), `${JSON.stringify({ ...board, ...change }, null, 2)}\n`);
}

test.describe("boards", () => {
  test("the board menu: rename in place, Export PNG…, Copy link, and Delete…", async ({ sandbox, launch }) => {
    const { app, page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await traceEntrance(page, "POST", "/orders");
    const { id } = await shownBoardRef(page);

    await test.step("the board is a file in .unfold/boards, and the menu holds Rename…, Move to folder, Export PNG…, Copy link, Delete…", async () => {
      expect(id).toMatch(/^\.unfold\/boards\/[^/]+\.json$/);
      await expectBoardFile(folder, id, (board) => board.title === "POST /orders", "the traced board is saved");
      await sidebar(page).getByRole("button", { name: "Board menu" }).click();
      const menu = page.getByRole("menu", { name: "Board menu" });
      await expect(menu.getByRole("menuitem")).toHaveText(["Rename…", "Move to folder", "Export PNG…", "Copy link", "Delete…"]);
      await page.keyboard.press("Escape");
      await expect(menu).toBeHidden();
    });

    await test.step("double-clicking the title in the sidebar header renames the board in place", async () => {
      await boardTitle(page).dblclick();
      const field = sidebar(page).getByRole("textbox", { name: "Board title" });
      await enterText(field, "Placing an order");
      await field.press("Enter");
      await expect(boardTitle(page)).toHaveText("Placing an order");
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

    await test.step("Export PNG… draws the page in the render window and writes it through the save dialog", async () => {
      const file = join(sandbox.root, "orders.png");
      await app.stubSaveDialog(file);
      await boardMenu(page, "Export PNG…");
      await expect(toast(page, "Exported PNG")).toBeVisible({ timeout: 30_000 });
      // A PNG's signature, then its header's width and height.
      const png = readFileSync(file);
      expect([...png.subarray(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
      expect(png.readUInt32BE(16)).toBeGreaterThan(100);
      expect(png.readUInt32BE(20)).toBeGreaterThan(100);
    });

    await test.step("Copy link puts the board's link, named by the project's folder, on the clipboard", async () => {
      await boardMenu(page, "Copy link");
      await expect(toast(page, "Copied the link")).toBeVisible();
      await expect.poll(() => app.clipboardText()).toBe(`unfold://ts-orders/${id}`);
    });

    await test.step("Delete… asks, then deletes the file and shows Home", async () => {
      await boardMenu(page, "Delete…");
      const confirm = dialog(page, "Delete “Orders”?");
      await confirm.getByRole("button", { name: "Delete", exact: true }).click();
      await expect(toast(page, "Deleted Orders")).toBeVisible();
      await expect(homeMain(page)).toBeVisible();
      await expect(sidebarBoard(page, "Orders")).toHaveCount(0);
      await expect(homeBoardRows(page)).toHaveCount(0);
      expect(existsSync(join(folder, id))).toBe(false);
    });
  });

  test("folders are one level: Move to folder, the sidebar tree, and the folder menu", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await traceEntrance(page, "POST", "/orders");
    const { id } = await shownBoardRef(page);

    await test.step("Move to folder › New folder… refuses a folder inside a folder", async () => {
      await boardMenu(page, "Move to folder", "New folder…");
      const prompt = dialog(page, "New folder");
      await enterText(prompt.getByRole("textbox", { name: "Name" }), "flows/orders");
      await prompt.getByRole("button", { name: "Move" }).click();
      await expect(prompt).toContainText("A folder's name can't contain “/”: folders hold boards, not other folders.");
      await expect(prompt).toBeVisible();
    });

    await test.step("a folder's name moves the board into the new folder", async () => {
      const prompt = dialog(page, "New folder");
      await enterText(prompt.getByRole("textbox", { name: "Name" }), "flows");
      await prompt.getByRole("button", { name: "Move" }).click();
      await expect(prompt).toBeHidden();
      await expect(sidebar(page).locator(".board-crumb-text")).toHaveText("ts-orders / flows");
      await expectBoardFile(folder, id, (board) => board.folder === "flows", "the folder is saved");
    });

    await test.step("the sidebar tree shows the folder with the board inside, and Home's row names the folder", async () => {
      await showHome(page);
      const flows = sidebarFolder(page, "flows");
      await expect(flows).toBeVisible();
      if ((await flows.getAttribute("aria-expanded")) !== "true") {
        await flows.click();
      }
      await expect(flows).toHaveAttribute("aria-expanded", "true");
      await expect(sidebarBoard(page, "POST /orders")).toHaveAttribute("aria-level", "3");
      await expect(homeBoardRows(page).locator(".home-board-folder")).toHaveText(["flows"]);
    });

    await test.step("the folder menu holds New Board Here, Rename…, and Delete, which a folder holding a board can't use", async () => {
      await sidebarFolder(page, "flows").click({ button: "right" });
      const menu = page.getByRole("menu", { name: "Sidebar menu" });
      await expect(menu.getByRole("menuitem")).toHaveText(["New Board Here", "Rename…", "Delete"]);
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
      const journeys = sidebarFolder(page, "journeys");
      if ((await journeys.getAttribute("aria-expanded")) !== "true") {
        await journeys.click();
      }
      await expect(sidebarBoard(page, "POST /orders")).toHaveAttribute("aria-level", "3");
    });

    await test.step("Move to folder › Top level moves the board out again", async () => {
      await showBoard(page, "POST /orders");
      await expect(sidebar(page).locator(".board-crumb-text")).toHaveText("ts-orders / journeys");
      await boardMenu(page, "Move to folder", "Top level");
      await expectBoardFile(folder, id, (board) => board.folder === undefined, "the board is at the top level");
      await expect(sidebar(page).locator(".board-crumb-text")).toHaveText("ts-orders");
      await showHome(page);
      await expect(sidebarBoard(page, "POST /orders")).toHaveAttribute("aria-level", "2");
    });
  });

  test("a board's file changed outside the app: the board reloads and its undo starts over, a deleted file is written back, a failed save offers Retry", async ({ sandbox, launch }) => {
    const { app, page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await traceEntrance(page, "POST", "/orders");
    const { id } = await shownBoardRef(page);
    const createOrder = card(page, "#function:createOrder");

    await test.step("an edit in the app is saved, and is a step to undo", async () => {
      await layerRow(page, "createOrder").click();
      await describeSelected(page, "Places the order.");
      await expect(createOrder.locator(".card-description")).toContainText("Places the order.");
      await expectBoardFile(folder, id, (board) => firstDescription(board) === "Places the order.", "the description is saved");
    });

    await test.step("an outside edit reloads the board", async () => {
      const reloaded = await eventMark(page, "board_reloaded");
      editOutside(folder, id, { title: "Edited elsewhere" });
      await waitForEvent(page, "board_reloaded", reloaded);
      await expect(boardTitle(page)).toHaveText("Edited elsewhere");
      await expect(page).toHaveTitle("Edited elsewhere — Unfold");
    });

    await test.step("Undo has nothing to take back after the reload", async () => {
      await clickCanvas(page);
      const undone = await eventMark(page, "menu_command");
      await app.clickMenu(page, ["Edit", "Undo"]);
      await waitForEvent(page, "menu_command", undone);
      await expect(boardTitle(page)).toHaveText("Edited elsewhere");
      await expect(createOrder.locator(".card-description")).toContainText("Places the order.");
    });

    await test.step("deleting the board's file keeps the board, and the next edit writes the file back", async () => {
      const deleted = await eventMark(page, "board_reloaded");
      rmSync(join(folder, id));
      await waitForEvent(page, "board_reloaded", deleted);
      await expect(boardTitle(page)).toHaveText("Edited elsewhere");
      await expect(boardView(page).getByRole("alert")).toHaveCount(0);
      await layerRow(page, "createOrder").click();
      await describeSelected(page, "Places the order, or queues it.");
      await expectBoardFile(
        folder,
        id,
        (board) => board.title === "Edited elsewhere" && firstDescription(board) === "Places the order, or queues it.",
        "the board is written back",
      );
    });

    await test.step("a save that fails shows Not saved · Retry, and the board doesn't reload while it fails", async () => {
      const boards = join(folder, BOARDS);
      const notSaved = boardView(page).getByRole("status").filter({ hasText: "Not saved ·" });
      chmodSync(boards, 0o555);
      try {
        await describeSelected(page, "Checks the user, then places the order.");
        await expect(notSaved).toBeVisible();
        await expect(notSaved.locator(".board-banner-text")).toHaveAttribute("title", /\S/);
        await expect(notSaved.getByRole("button", { name: "Retry" })).toBeVisible();
        const changed = await eventMark(page, "board_reloaded");
        editOutside(folder, id, { title: "Changed while failing" });
        await waitForEvent(page, "board_reloaded", changed);
        await expect(boardTitle(page)).toHaveText("Edited elsewhere");
      } finally {
        chmodSync(boards, 0o755);
      }
    });

    await test.step("Retry writes the person's version over the outside change", async () => {
      const notSaved = boardView(page).getByRole("status").filter({ hasText: "Not saved ·" });
      await notSaved.getByRole("button", { name: "Retry" }).click();
      await expect(notSaved).toBeHidden();
      await expectBoardFile(
        folder,
        id,
        (board) => board.title === "Edited elsewhere" && firstDescription(board) === "Checks the user, then places the order.",
        "the person's version is saved",
      );
    });
  });
});
