/**
 * The project menu (`⋯` on a project's sidebar row and on its Home; docs/ui-shell.md#screens):
 * Compile again, Rebuild from scratch, Cancel, Pause and Resume background updates, Settings…,
 * Add entrance…, Health and activity, Reveal in Finder, and Remove from workspace….
 */

import { existsSync } from "node:fs";
import { join } from "node:path";
import type { Locator, Page } from "@playwright/test";
import { card, chooseLens, showBoard } from "../src/board";
import { expect, test } from "../src/fixtures";
import { CREATE_ORDER, launchCompiled, launchWithProject, traceEntrance } from "../src/flows";
import {
  chooseMenuItem,
  dialog,
  expectGraphCurrent,
  firstCompile,
  homeMain,
  homeTitle,
  projectId,
  projectMenu,
  projectRow,
  rowState,
  showHome,
  stateLine,
  workspace,
} from "../src/home";
import { editFile } from "../src/sandbox";
import { writeSyntheticRepo } from "../src/synthetic";
import { eventMark, events, invoke, waitForEvent } from "../src/window";

interface IndexLog {
  readonly entries: readonly { readonly action: string }[];
}

async function logActions(page: Page, project: string): Promise<string[]> {
  return (await invoke<IndexLog>(page, "index_log", { project })).entries.map((entry) => entry.action);
}

/** Opens the project menu on Home's header and returns it. */
async function openHeaderMenu(page: Page): Promise<Locator> {
  await homeMain(page).getByRole("button", { name: "Project menu" }).click();
  const menu = page.getByRole("menu", { name: "Project menu" });
  await expect(menu).toBeVisible();
  return menu;
}

/** Waits for the project to go through `compiling` and come back to current, from `mark`. */
async function compiledAgain(page: Page, project: string, mark: number): Promise<void> {
  await waitForEvent(page, "project_state", mark, (event) => event["project"] === project && (event["state"] as { kind: string }).kind === "compiling");
  await waitForEvent(page, "project_state", mark, (event) => event["project"] === project && (event["state"] as { kind: string }).kind === "current");
  await expectGraphCurrent(page);
}

test.describe("the project menu", () => {
  test("Compile again, Rebuild from scratch, Health and activity, Settings…, Add entrance…, Reveal in Finder", async ({ sandbox, launch }) => {
    const { page } = await launchCompiled(launch, sandbox);
    const project = await projectId(page, "ts-orders");

    await test.step("the menu on Home's header lists every item, with Cancel disabled", async () => {
      const menu = await openHeaderMenu(page);
      const names = await menu.getByRole("menuitem").allInnerTexts();
      expect(names.map((name) => name.split("\n")[0]?.trim())).toEqual([
        "Compile again",
        "Rebuild from scratch",
        "Cancel",
        "Pause background updates",
        "Settings…",
        "Add entrance…",
        "Health and activity",
        "Reveal in Finder",
        "Remove from workspace…",
      ]);
      await expect(menu.getByRole("menuitem", { name: "Cancel" })).toHaveAttribute("aria-disabled", "true");
      await expect(menu.getByRole("menuitem", { name: "Reveal in Finder" })).not.toHaveAttribute("aria-disabled", "true");
      // Reveal in Finder isn't chosen: Rust opens the real file manager.
      await page.keyboard.press("Escape");
      await expect(menu).toBeHidden();
    });

    await test.step("Compile again runs a compile that the Activity log records", async () => {
      const before = await logActions(page, project);
      const mark = await eventMark(page, "project_state");
      await projectMenu(page, "ts-orders", /^Compile again/);
      await compiledAgain(page, project, mark);
      await expect.poll(() => logActions(page, project)).toHaveLength(before.length + 1);
      expect((await logActions(page, project)).at(-1)).toBe("Compile");
    });

    await test.step("Rebuild from scratch rebuilds the graph", async () => {
      const before = await logActions(page, project);
      const mark = await eventMark(page, "project_state");
      await projectMenu(page, "ts-orders", "Rebuild from scratch");
      await compiledAgain(page, project, mark);
      await expect.poll(() => logActions(page, project)).toHaveLength(before.length + 1);
      expect((await logActions(page, project)).at(-1)).toBe("Rebuild from scratch");
    });

    await test.step("Health and activity opens Home's Health view; Activity lists the rebuild", async () => {
      await projectMenu(page, "ts-orders", "Health and activity");
      const health = homeMain(page).getByRole("region", { name: "Health and activity" });
      await expect(health).toBeVisible();
      await health.getByRole("radio", { name: /^Activity/ }).click();
      const log = health.getByRole("table", { name: "Activity" });
      await expect(log.getByRole("row").filter({ hasText: "Rebuild" })).not.toHaveCount(0);
      await expect(log.getByRole("row").filter({ hasText: "Compile" })).not.toHaveCount(0);
    });

    await test.step("Settings… opens the project's settings", async () => {
      await projectMenu(page, "ts-orders", "Settings…");
      const settings = dialog(page, "ts-orders settings");
      await expect(settings).toBeVisible();
      await settings.getByRole("button", { name: "Cancel", exact: true }).click();
      await expect(settings).toBeHidden();
    });

    await test.step("Add entrance… opens the launch editor", async () => {
      await projectMenu(page, "ts-orders", "Add entrance…");
      const editor = dialog(page, "Add launch");
      await expect(editor).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(editor).toBeHidden();
    });
  });

  test("Cancel stops a Rebuild from scratch before it commits", async ({ sandbox, launch }) => {
    test.setTimeout(300_000);
    const folder = sandbox.folder("synthetic");
    writeSyntheticRepo(folder, 8000, "main");
    const { page } = await launchWithProject(launch, folder);
    await firstCompile(page);
    const project = await projectId(page, "synthetic");
    const before = await logActions(page, project);

    await test.step("while it rebuilds, the row shows progress and only Cancel applies", async () => {
      await projectMenu(page, "synthetic", "Rebuild from scratch");
      await expect(rowState(page, "synthetic")).toHaveAttribute("data-state", "compiling");
      const row = projectRow(page, "synthetic");
      await row.hover();
      await row.getByRole("button", { name: "Project menu" }).click();
      const menu = page.getByRole("menu", { name: "Project menu" });
      await expect(menu.getByRole("menuitem", { name: /^Compile again/ })).toHaveAttribute("aria-disabled", "true");
      await expect(menu.getByRole("menuitem", { name: "Rebuild from scratch" })).toHaveAttribute("aria-disabled", "true");
      await chooseMenuItem(page, "Project menu", "Cancel");
    });

    await test.step("Cancel returns the project to its graph, and nothing is committed", async () => {
      await expect(rowState(page, "synthetic")).toHaveAttribute("data-state", "current");
      await expectGraphCurrent(page);
      await expect(stateLine(page)).toContainText("8,000 files");
      expect(await logActions(page, project)).toEqual(before);
    });
  });

  test("Pause background updates holds an edit back until Resume", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    const project = await projectId(page, "ts-orders");
    await traceEntrance(page, "POST", "/orders");
    // Read createOrder's source in Code, where the edit will show.
    await chooseLens(page, "Code");
    await expect(card(page, CREATE_ORDER)).toContainText("placed for");
    await showHome(page);

    await test.step("Pause background updates: the row shows the pause glyph and Home says so", async () => {
      await projectMenu(page, "ts-orders", "Pause background updates");
      await expect(rowState(page, "ts-orders")).toHaveAttribute("data-state", "paused");
      await expect(stateLine(page)).toHaveAttribute("data-state", "paused");
      await expect(stateLine(page)).toContainText("background updates are off");
    });

    await test.step("an edit while paused is counted but not compiled", async () => {
      const mark = await eventMark(page, "graph_updated");
      editFile(join(folder, "src/routes/orders.ts"), "placed for ${order.customerId}", "was placed for ${order.customerId}");
      await expect(stateLine(page)).toContainText("1 file changed since");
      expect((await events(page, "graph_updated")).slice(mark)).toEqual([]);
    });

    await test.step("the project menu now offers Resume background updates; Resume compiles the edit", async () => {
      const mark = await eventMark(page, "graph_updated");
      await projectMenu(page, "ts-orders", "Resume background updates");
      await waitForEvent(page, "graph_updated", mark, (event) => event["project"] === project);
      await expect(rowState(page, "ts-orders")).toHaveAttribute("data-state", "current");
      await expectGraphCurrent(page);
    });

    await test.step("the card shows the edited code", async () => {
      await showBoard(page, "POST /orders");
      await expect(card(page, CREATE_ORDER)).toContainText("was placed for");
    });
  });

  test("Remove from workspace… asks, keeps .unfold/, and the folders can be added back", async ({ sandbox, launch }) => {
    const { app, page, folders } = await launchCompiled(launch, sandbox, ["ts-orders", "py-orders"]);
    const [orders = "", python = ""] = folders;

    await test.step("answering Cancel keeps the project", async () => {
      await app.stubMessageBox(1);
      await projectMenu(page, "py-orders", "Remove from workspace…");
      await expect.poll(() => app.messageBoxes()).toEqual(["Remove py-orders from the workspace?"]);
      await expect(projectRow(page, "py-orders")).toBeVisible();
    });

    await test.step("answering Remove takes it out of the workspace; its .unfold/ stays; Home shows the other project", async () => {
      await app.stubMessageBox(0);
      await projectMenu(page, "py-orders", "Remove from workspace…");
      await expect(projectRow(page, "py-orders")).toHaveCount(0);
      expect(await app.messageBoxes()).toEqual(["Remove py-orders from the workspace?"]);
      expect(existsSync(join(python, ".unfold", "local"))).toBe(true);
      await expect(homeTitle(page)).toHaveText("ts-orders");
      expect((await workspace(page)).projects.map((row) => row.name)).toEqual(["ts-orders"]);
    });

    await test.step("removing the last project leaves an empty workspace", async () => {
      await projectMenu(page, "ts-orders", "Remove from workspace…");
      await expect(projectRow(page, "ts-orders")).toHaveCount(0);
      expect(await app.messageBoxes()).toEqual(["Remove py-orders from the workspace?", "Remove ts-orders from the workspace?"]);
      expect(existsSync(join(orders, ".unfold", "local"))).toBe(true);
    });

    await test.step("first launch lists both folders under Opened before; Add brings one back, still compiled", async () => {
      await expect(homeMain(page).getByRole("heading", { name: "Read a codebase by how it starts." })).toBeVisible();
      const before = homeMain(page).locator(".home-first-before");
      await expect(before.getByRole("heading", { name: "Opened before" })).toBeVisible();
      await expect(before.getByRole("button", { name: "Add ts-orders" })).toBeVisible();
      await expect(before.getByRole("button", { name: "Add py-orders" })).toBeVisible();
      await before.getByRole("button", { name: "Add ts-orders" }).click();
      await expect(projectRow(page, "ts-orders")).toBeVisible();
      await expect(homeTitle(page)).toHaveText("ts-orders");
      await expectGraphCurrent(page);
      expect((await workspace(page)).removed.map((row) => row.name)).toEqual(["py-orders"]);
    });
  });
});
