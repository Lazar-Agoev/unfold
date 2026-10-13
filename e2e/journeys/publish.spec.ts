/**
 * Drafts, publishing, and unpublished changes (docs/reader.md#files-drafts-publish-and-unpublished-changes, unpublished edits): a new board is a draft;
 * its first Publish names its file; edits to a published board stay on this machine until Publish;
 * Discard changes and Move back to drafts; a name that's taken; and a published file that changed
 * underneath the changes (a pull), resolved both ways. A trace's first save already has its cards in
 * place: leaving the board at once and publishing makes no unpublished changes later.
 */

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { boardMenu, boardView, expectCardsLoaded, layerRow, publishState, readBoard, showBoard, shownBoardRef, type BoardFile } from "../src/board";
import { expect, test } from "../src/fixtures";
import { launchCompiled, traceEntrance } from "../src/flows";
import { dialog, entranceRow, homeMain, showEntrances, showHome, sidebar, toast } from "../src/home";
import { boardFiles, describeSelected, DRAFTS, expectBoardFile, firstDescription, PUBLISHED } from "../src/lifecycle";

const DRAFT_ID = /^\.unfold\/local\/drafts\/board-\d+(-\d+)?\.json$/;
const PUBLISHED_ID = ".unfold/boards/post-orders.json";

/** Each card's id and position on a board file's first page. */
function places(board: BoardFile): string {
  return JSON.stringify(board.pages[0]?.cards.map((card) => [card.id, card.at]));
}

/** Rewrites a board file's title on disk, as a teammate's commit pulled in would. */
function retitle(folder: string, id: string, title: string): void {
  const path = join(folder, id);
  const board = JSON.parse(readFileSync(path, "utf8")) as BoardFile & Record<string, unknown>;
  writeFileSync(path, `${JSON.stringify({ ...board, title }, null, 2)}\n`);
}

test.describe("publishing", () => {
  test("a draft, its first Publish, unpublished changes, Discard changes, and Move back to drafts", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";

    let traced = "";
    await test.step("a new board is a draft, on this machine only, saved with its cards in place", async () => {
      // Leave the board as soon as the trace draws it: nothing may move its cards after that step.
      await showEntrances(page);
      await entranceRow(page, "POST", "/orders").click();
      await expect(boardView(page).locator(".entrance-card")).toBeAttached();
      const { id } = await shownBoardRef(page);
      await showHome(page);
      expect(id).toMatch(DRAFT_ID);
      const draft = await expectBoardFile(folder, id, (board) => board.title === "POST /orders" && board.pages[0]?.cards.length === 4, "the draft is written with the traced cards");
      traced = places(draft);
      expect(boardFiles(folder, PUBLISHED)).toEqual([]);
    });

    await test.step("Drafts in the sidebar lists it", async () => {
      await sidebar(page).getByRole("button", { name: /^Drafts/ }).click();
      await expect(homeMain(page).getByRole("heading", { name: "Drafts", level: 1 })).toBeVisible();
      await expect(homeMain(page).getByRole("button", { name: "POST /orders" })).toBeVisible();
      await homeMain(page).getByRole("button", { name: "POST /orders" }).click();
      await expect(publishState(page)).toHaveText("Draft");
      await expectCardsLoaded(page);
    });

    await test.step("the first Publish names the file and writes it to .unfold/boards", async () => {
      await boardView(page).getByRole("button", { name: "Publish", exact: true }).click();
      const publish = dialog(page, "Publish POST /orders");
      await expect(publish).toBeVisible();
      await expect(publish.getByRole("textbox", { name: "File name" })).toHaveValue("post-orders");
      await expect(publish).toContainText(".unfold/boards/post-orders.json");
      await publish.getByRole("button", { name: "Publish", exact: true }).click();
      await expect(publish).toBeHidden();
      await expect(toast(page, "Published POST /orders")).toBeVisible();
      await expect(publishState(page)).toHaveText("Published");
      expect((await shownBoardRef(page)).id).toBe(PUBLISHED_ID);
      const published = readBoard(folder, PUBLISHED_ID);
      expect(published.title).toBe("POST /orders");
      expect(places(published), "the cards stand where the trace saved them").toBe(traced);
      expect(boardFiles(folder, DRAFTS)).toEqual([]);
    });

    await test.step("a published board without changes has nothing to publish, shown again too", async () => {
      await expect(boardView(page).getByRole("button", { name: "Publish", exact: true })).toBeDisabled();
      await showHome(page);
      await showBoard(page, "POST /orders");
      await expectCardsLoaded(page);
      // Long enough for an autosave (600 ms) of anything that moved after showing the board.
      await page.waitForTimeout(1_500);
      await expect(publishState(page)).toHaveText("Published");
      await expect(boardView(page).getByRole("button", { name: "Publish", exact: true })).toBeDisabled();
      expect(existsSync(join(folder, ".unfold/local/drafts/post-orders.changes.json"))).toBe(false);
    });

    await test.step("an edit is an unpublished change kept on this machine", async () => {
      const published = readFileSync(join(folder, PUBLISHED_ID), "utf8");
      await layerRow(page, "POST /orders").click();
      await describeSelected(page, "Takes an order from the web");
      await expect(publishState(page)).toHaveText("Published · 1 unpublished change");
      await expectBoardFile(folder, ".unfold/local/drafts/post-orders.changes.json", (board) => firstDescription(board) === "Takes an order from the web", "the changes copy holds the edit");
      expect(readFileSync(join(folder, PUBLISHED_ID), "utf8")).toBe(published);
      expect((await shownBoardRef(page)).id).toBe(PUBLISHED_ID);
    });

    await test.step("Publish (top bar) writes the changes over the published file", async () => {
      await boardView(page).getByRole("button", { name: "Publish", exact: true }).click();
      const publish = dialog(page, "Publish changes to POST /orders");
      await expect(publish).toContainText("1 change");
      await publish.getByRole("button", { name: "Publish", exact: true }).click();
      await expect(publish).toBeHidden();
      await expect(publishState(page)).toHaveText("Published");
      expect(firstDescription(readBoard(folder, PUBLISHED_ID))).toBe("Takes an order from the web");
      await expect.poll(() => existsSync(join(folder, ".unfold/local/drafts/post-orders.changes.json"))).toBe(false);
    });

    await test.step("Discard changes… asks, then returns to the published version", async () => {
      await describeSelected(page, "Something half done");
      await expect(publishState(page)).toHaveText("Published · 1 unpublished change");
      await expectBoardFile(folder, ".unfold/local/drafts/post-orders.changes.json", (board) => firstDescription(board) === "Something half done", "the changes copy holds the edit");
      await boardMenu(page, "Discard changes…");
      const confirm = dialog(page, "Discard changes to POST /orders?");
      await confirm.getByRole("button", { name: "Discard changes" }).click();
      await expect(confirm).toBeHidden();
      await expect(publishState(page)).toHaveText("Published");
      await expect(boardView(page).locator(".card-description")).toHaveText(/Takes an order from the web/);
      expect(existsSync(join(folder, ".unfold/local/drafts/post-orders.changes.json"))).toBe(false);
    });

    await test.step("Move back to drafts makes it a draft again under a new id", async () => {
      await boardMenu(page, "Move back to drafts");
      await expect(toast(page, "POST /orders is a draft again")).toBeVisible();
      await expect(publishState(page)).toHaveText("Draft");
      const { id } = await shownBoardRef(page);
      expect(id).toMatch(/^\.unfold\/local\/drafts\//);
      expect(existsSync(join(folder, PUBLISHED_ID))).toBe(false);
      expect(firstDescription(readBoard(folder, id))).toBe("Takes an order from the web");
    });
  });

  test("a file name that's taken gets a free one", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await traceEntrance(page, "POST", "/orders");
    await boardView(page).getByRole("button", { name: "Publish", exact: true }).click();
    await dialog(page, "Publish POST /orders").getByRole("button", { name: "Publish", exact: true }).click();
    await expect(publishState(page)).toHaveText("Published");

    await test.step("publishing a second board under the same name suggests another", async () => {
      await showHome(page);
      await traceEntrance(page, "POST", "/orders");
      await boardView(page).getByRole("button", { name: "Publish", exact: true }).click();
      const publish = dialog(page, "Publish POST /orders");
      const name = publish.getByRole("textbox", { name: "File name" });
      await expect(name).toHaveValue("post-orders");
      await publish.getByRole("button", { name: "Publish", exact: true }).click();
      await expect(publish).toContainText(".unfold/boards/post-orders.json already exists");
      await expect(name).not.toHaveValue("post-orders");
      const suggestion = await name.inputValue();
      await publish.getByRole("button", { name: "Publish", exact: true }).click();
      await expect(publish).toBeHidden();
      await expect(publishState(page)).toHaveText("Published");
      expect((await shownBoardRef(page)).id).toBe(`.unfold/boards/${suggestion}.json`);
      expect(boardFiles(folder, PUBLISHED)).toEqual(["post-orders.json", `${suggestion}.json`].sort());
    });
  });

  test("a published file that changed underneath: keep mine, then take theirs", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await traceEntrance(page, "POST", "/orders");
    await boardView(page).getByRole("button", { name: "Publish", exact: true }).click();
    await dialog(page, "Publish POST /orders").getByRole("button", { name: "Publish", exact: true }).click();
    await expect(publishState(page)).toHaveText("Published");

    /** Makes an unpublished change, then changes the published file on disk under it. */
    const conflict = async (description: string, theirs: string) => {
      await describeSelected(page, description);
      await expectBoardFile(folder, ".unfold/local/drafts/post-orders.changes.json", (board) => firstDescription(board) === description, "the changes copy holds the edit");
      retitle(folder, PUBLISHED_ID, theirs);
      await boardView(page).getByRole("button", { name: "Publish", exact: true }).click();
      const publish = dialog(page, /^Publish changes to /);
      await expect(publish).toBeVisible();
      // The session's last save came before the outside change, so Publish is what finds it.
      const box = publish.locator(".publish-conflict");
      await expect(box).toBeHidden();
      await publish.getByRole("button", { name: "Publish", exact: true }).click();
      await expect(box).toContainText("The published file changed since you started. Publishing replaces it.");
      return publish;
    };

    await test.step("Keep mine writes the changes over the changed file", async () => {
      const publish = await conflict("Mine wins", "Their title");
      await expect(publish.getByRole("radio", { name: "Keep mine" })).toBeChecked();
      await publish.getByRole("button", { name: "Publish", exact: true }).click();
      await expect(publish).toBeHidden();
      await expect(publishState(page)).toHaveText("Published");
      const written = readBoard(folder, PUBLISHED_ID);
      expect(firstDescription(written)).toBe("Mine wins");
      expect(written.title).toBe("POST /orders");
    });

    await test.step("Take theirs drops the changes for the file on disk", async () => {
      const publish = await conflict("Mine loses", "Their title");
      await publish.getByRole("radio", { name: "Take theirs and drop my changes" }).check();
      await publish.getByRole("button", { name: "Discard my changes" }).click();
      await expect(publish).toBeHidden();
      await expect(toast(page, /^Took the published/)).toBeVisible();
      await expect(publishState(page)).toHaveText("Published");
      await expect(boardView(page).locator(".board-topbar-title")).toBeVisible();
      await expect(sidebar(page).locator("h1.board-title")).toHaveText("Their title");
      expect(readBoard(folder, PUBLISHED_ID).title).toBe("Their title");
      expect(existsSync(join(folder, ".unfold/local/drafts/post-orders.changes.json"))).toBe(false);
    });
  });
});
