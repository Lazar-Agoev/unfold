/**
 * The keyboard map of redesign §8, each shortcut once with its effect; the dispatcher's rules (a
 * plain key never fires while typing, no window shortcut fires while a dialog is open); and the
 * application menu's Unfold items (§6 "Application menu").
 */

import type { Page } from "@playwright/test";
import { addBox, addInput, addTab } from "../src/add";
import { boardView, camera, card, cameraSettled, clickCanvas, dock, lens, onThisPage, pagesList, selected, zoomPercent } from "../src/board";
import { expect, test } from "../src/fixtures";
import { CREATE_ORDER, launchCompiled, traceEntrance } from "../src/flows";
import {
  compileButton,
  dialog,
  expectGraphCurrent,
  homeTab,
  homeTitle,
  projectId,
  projectRow,
  selectProject,
  showHome,
  sidebar,
  tabs,
  toast,
  workspace,
} from "../src/home";
import { clickAppMenu } from "../src/session";
import { eventMark, waitForEvent } from "../src/window";

/** The title of the active tab. */
function activeTab(page: Page) {
  return tabs(page).and(page.locator("[aria-selected='true']")).locator(".tab-label");
}

/** Opens Add with `key`, checks it opened as asked, and closes it with Escape. */
async function addOpensWith(page: Page, key: string, check: () => Promise<void> = () => Promise.resolve()): Promise<void> {
  await page.keyboard.press(key);
  await expect(addInput(page)).toBeFocused();
  await check();
  await page.keyboard.press("Escape");
  await expect(addBox(page)).toHaveCount(0);
}

/** Waits for a compile of `project` to start (its state turns to compiling). */
async function compileStarted(page: Page, project: string, mark: number): Promise<void> {
  await waitForEvent(page, "project_state", mark, (event) => event["project"] === project && (event["state"] as { kind: string }).kind === "compiling");
}

test.describe("the keyboard", () => {
  test("every shortcut of the keyboard map, once", async ({ sandbox, launch }) => {
    const { app, page } = await launchCompiled(launch, sandbox);
    const python = sandbox.playground("py-orders");
    const orders = await projectId(page, "ts-orders");

    await test.step("⌘O adds a project; ⇧⌘B compiles it from its not-compiled Home", async () => {
      await app.stubFolderDialog(python);
      await page.keyboard.press("Meta+O");
      await expect(homeTitle(page)).toHaveText("py-orders");
      await expect(compileButton(page)).toBeVisible();
      await page.keyboard.press("Meta+Shift+B");
      await expectGraphCurrent(page);
      await selectProject(page, "ts-orders");
    });

    await test.step("A, ⌘K, and / open Add; ⇧⌘P opens it on commands; ⌘P on Files; ⌘T on Functions", async () => {
      await homeTab(page).focus();
      await addOpensWith(page, "a");
      await addOpensWith(page, "Meta+K");
      await addOpensWith(page, "/");
      await addOpensWith(page, "Meta+Shift+P", async () => {
        await expect(addInput(page)).toHaveValue(/^>/);
      });
      await addOpensWith(page, "Meta+P", async () => {
        await expect(addTab(page, "Files")).toHaveAttribute("aria-selected", "true");
      });
      await addOpensWith(page, "Meta+T", async () => {
        await expect(addTab(page, "Functions")).toHaveAttribute("aria-selected", "true");
      });
    });

    await test.step("⌘N makes a new board; ⌘0 shows Home; ⌘W closes a tab", async () => {
      await page.keyboard.press("Meta+N");
      await expect(tabs(page)).toHaveCount(2);
      await expect(homeTab(page)).toHaveAttribute("aria-selected", "false");
      await page.keyboard.press("Meta+0");
      await expect(homeTab(page)).toHaveAttribute("aria-selected", "true");
      await tabs(page).nth(1).click();
      await page.keyboard.press("Meta+W");
      await expect(tabs(page)).toHaveCount(1);
      await expect(homeTab(page)).toHaveAttribute("aria-selected", "true");
    });

    await test.step("⌃Tab and ⌃⇧Tab, ⌘⇧] and ⌘⇧[ move between tabs", async () => {
      await traceEntrance(page, "GET", "/health");
      await showHome(page);
      await traceEntrance(page, "POST", "/orders");
      await expect(activeTab(page)).toHaveText("POST /orders");
      await page.keyboard.press("Control+Tab");
      await expect(activeTab(page)).toHaveText("Home");
      await page.keyboard.press("Control+Shift+Tab");
      await expect(activeTab(page)).toHaveText("POST /orders");
      await page.keyboard.press("Meta+Shift+BracketLeft");
      await expect(activeTab(page)).toHaveText("GET /health");
      await page.keyboard.press("Meta+Shift+BracketRight");
      await expect(activeTab(page)).toHaveText("POST /orders");
    });

    await test.step("⌥⌘N adds a page; ⌘Z and ⇧⌘Z undo and redo it", async () => {
      await clickCanvas(page);
      await page.keyboard.press("Alt+Meta+N");
      await expect(pagesList(page).getByRole("option")).toHaveCount(2);
      await clickCanvas(page);
      await page.keyboard.press("Meta+Z");
      await expect(pagesList(page).getByRole("option")).toHaveCount(1);
      await page.keyboard.press("Meta+Shift+Z");
      await expect(pagesList(page).getByRole("option")).toHaveCount(2);
      await pagesList(page).getByRole("option").first().click();
    });

    await test.step("⌘1 and ⌘2 set the page's lens to Code and Flow", async () => {
      await clickCanvas(page);
      await page.keyboard.press("Meta+2");
      await expect(lens(page).getByRole("radio", { name: "Flow" })).toHaveAttribute("aria-checked", "true");
      await expect(card(page, CREATE_ORDER).locator(".flow-view")).toBeVisible();
      await page.keyboard.press("Meta+1");
      await expect(lens(page).getByRole("radio", { name: "Code" })).toHaveAttribute("aria-checked", "true");
      await expect(card(page, CREATE_ORDER).locator(".code-view")).toBeVisible();
    });

    await test.step("⌘B hides and shows the sidebar", async () => {
      await page.keyboard.press("Meta+B");
      await expect(sidebar(page)).toHaveCount(0);
      await page.keyboard.press("Meta+B");
      await expect(sidebar(page)).toBeVisible();
    });

    await test.step("⌘[ and ⌘] go back and forward", async () => {
      await onThisPage(page).getByRole("option", { name: "createOrder", exact: true }).click();
      await cameraSettled(page);
      const forward = dock(page).getByRole("button", { name: "Forward" });
      await expect(forward).toBeDisabled();
      await clickCanvas(page);
      await page.keyboard.press("Meta+BracketLeft");
      await expect(forward).toBeEnabled();
      await page.keyboard.press("Meta+BracketRight");
      await expect(forward).toBeDisabled();
    });

    await test.step("⇧1 fits; ⌘= and ⌘- zoom in and out", async () => {
      /** Presses a zoom key and waits for the camera to move and settle (a move starts a frame or two later). */
      const zoomWith = async (key: string): Promise<number> => {
        const before = await camera(page);
        await page.keyboard.press(key);
        await expect.poll(() => camera(page)).not.toBe(before);
        await cameraSettled(page);
        return zoomPercent(page);
      };
      // Zoomed in first, so Fit has a move to make.
      const start = await zoomPercent(page);
      expect(await zoomWith("Meta+Equal")).toBeGreaterThan(start);
      const fitted = await zoomWith("Shift+Digit1");
      const zoomed = await zoomWith("Meta+Equal");
      expect(zoomed).toBeGreaterThan(fitted);
      expect(await zoomWith("Meta+Minus")).toBeLessThan(zoomed);
      expect(await zoomWith("Meta+Equal")).toBeGreaterThan(fitted);
      expect(await zoomWith("Shift+Digit1")).toBe(fitted);
    });

    await test.step("⇧A arranges the page by call depth", async () => {
      const header = card(page, CREATE_ORDER).locator(".card-header");
      const box = await header.boundingBox();
      if (!box) {
        throw new Error("createOrder isn't on screen");
      }
      await page.mouse.move(box.x + 20, box.y + box.height / 2);
      await page.mouse.down();
      await page.mouse.move(box.x - 200, box.y + 260, { steps: 8 });
      await page.mouse.up();
      await clickCanvas(page);
      await page.keyboard.press("Shift+A");
      await expect(toast(page, /^Arranged \d+ cards? by call depth\./)).toBeVisible();
    });

    await test.step("⇧⌘B compiles the board's project", async () => {
      const mark = await eventMark(page, "project_state");
      await page.keyboard.press("Meta+Shift+B");
      await compileStarted(page, orders, mark);
      await waitForEvent(page, "project_state", mark, (event) => event["project"] === orders && (event["state"] as { kind: string }).kind === "current");
    });

    await test.step("⇧⌘E opens Publish; ⌘, opens Preferences", async () => {
      await page.keyboard.press("Meta+Shift+E");
      await expect(dialog(page, "Publish POST /orders")).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(dialog(page, "Publish POST /orders")).toHaveCount(0);
      await page.keyboard.press("Meta+Comma");
      await expect(dialog(page, "Preferences")).toBeVisible();
    });

    await test.step("no window shortcut fires while a dialog is open", async () => {
      await page.keyboard.press("Meta+B");
      await page.keyboard.press("Meta+0");
      await expect(sidebar(page)).toBeVisible();
      await expect(homeTab(page)).toHaveAttribute("aria-selected", "false");
      await page.keyboard.press("Escape");
      await expect(dialog(page, "Preferences")).toHaveCount(0);
    });

    await test.step("a plain key doesn't fire while typing in a field", async () => {
      await card(page, CREATE_ORDER).locator(".card-header").click();
      const description = selected(page).getByRole("textbox", { name: "Description" });
      await description.click();
      await page.keyboard.type("a");
      await expect(description).toHaveValue("a");
      await expect(addBox(page)).toHaveCount(0);
    });

    await test.step("⇧⌘N opens a new window", async () => {
      const second = await app.nextWindow(() => page.keyboard.press("Meta+Shift+N"));
      await expect(homeTitle(second)).toHaveText("ts-orders");
      expect(app.windows()).toHaveLength(2);
    });
    await expect(boardView(page)).toBeVisible();
  });

  test("the application menu's File, View, and Go items", async ({ sandbox, launch }) => {
    const { app, page } = await launchCompiled(launch, sandbox);
    const python = sandbox.playground("py-orders");
    await traceEntrance(page, "POST", "/orders");

    await test.step("View › Home shows Home; View › Toggle Sidebar hides and shows the sidebar", async () => {
      await clickAppMenu(app, page, ["View", "Home"]);
      await expect(homeTab(page)).toHaveAttribute("aria-selected", "true");
      await clickAppMenu(app, page, ["View", "Toggle Sidebar"]);
      await expect(sidebar(page)).toHaveCount(0);
      await clickAppMenu(app, page, ["View", "Toggle Sidebar"]);
      await expect(sidebar(page)).toBeVisible();
    });

    await test.step("Go › Add opens Add", async () => {
      await clickAppMenu(app, page, ["Go", "Add"]);
      await expect(addInput(page)).toBeFocused();
      await page.keyboard.press("Escape");
      await expect(addBox(page)).toHaveCount(0);
    });

    await test.step("File › Add Project… adds the chosen folder", async () => {
      await app.stubFolderDialog(python);
      await clickAppMenu(app, page, ["File", "Add Project…"]);
      await expect(projectRow(page, "py-orders")).toBeVisible();
      await expect(homeTitle(page)).toHaveText("py-orders");
    });

    await test.step("File › Remove Project asks, then removes the project Home shows", async () => {
      await app.stubMessageBox(0);
      await clickAppMenu(app, page, ["File", "Remove Project"]);
      await expect(projectRow(page, "py-orders")).toHaveCount(0);
      expect(await app.messageBoxes()).toEqual(["Remove py-orders from the workspace?"]);
      expect((await workspace(page)).projects.map((row) => row.name)).toEqual(["ts-orders"]);
    });
  });
});
