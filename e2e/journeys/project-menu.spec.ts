/**
 * The project menu (`⋯` on a project's sidebar row and on its Home; docs/ui-shell.md#screens):
 * Compile again, Cancel, Open unfold.toml, and Remove from workspace….
 */

import { chmodSync, existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import type { Locator, Page } from "@playwright/test";
import { expect, test } from "../src/fixtures";
import { launchCompiled, launchWithProject } from "../src/flows";
import {
  chooseMenuItem,
  dialog,
  expectGraphCurrent,
  homeMain,
  homeTitle,
  projectId,
  projectMenu,
  projectRow,
  rowState,
  sidebar,
  stateLine,
  toast,
  workspace,
} from "../src/home";
import { readFile, writeFile } from "../src/sandbox";
import { writeSyntheticRepo } from "../src/synthetic";
import { enterText, eventMark, waitForEvent } from "../src/window";

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

/** Sets the editor command in the app's Settings (the sidebar's gear), saved on Enter. */
async function setEditorCommand(page: Page, command: string): Promise<void> {
  const settings = dialog(page, "Settings");
  await sidebar(page).getByRole("button", { name: "Settings", exact: true }).click();
  await expect(settings).toBeVisible();
  const editor = settings.getByRole("textbox", { name: "Editor command" });
  await enterText(editor, command);
  await editor.press("Enter");
  await page.keyboard.press("Escape");
  await expect(settings).toBeHidden();
}

test.describe("the project menu", () => {
  test("Compile again, and Open unfold.toml with the editor command", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    const config = join(folder, "unfold.toml");
    const project = await projectId(page, "ts-orders");
    // An editor that records what it was asked to open, one argument per line.
    const opened = join(sandbox.root, "opened.txt");
    const editor = join(sandbox.root, "editor.sh");
    writeFile(editor, `#!/bin/sh\nprintf '%s\\n' "$@" > "${opened}"\n`);
    chmodSync(editor, 0o755);

    await test.step("the menu on Home's header lists its four items, with Cancel disabled", async () => {
      const menu = await openHeaderMenu(page);
      await expect(menu.getByRole("menuitem")).toHaveText(["Compile again", "Cancel", "Open unfold.toml", "Remove from workspace…"]);
      await expect(menu.getByRole("menuitem", { name: "Cancel" })).toHaveAttribute("aria-disabled", "true");
      await page.keyboard.press("Escape");
      await expect(menu).toBeHidden();
    });

    await test.step("Compile again runs a compile", async () => {
      const mark = await eventMark(page, "project_state");
      await projectMenu(page, "ts-orders", "Compile again");
      await compiledAgain(page, project, mark);
    });

    await test.step("Open unfold.toml runs the editor command with the file's path", async () => {
      await setEditorCommand(page, `${editor} {path}`);
      await projectMenu(page, "ts-orders", "Open unfold.toml");
      await expect.poll(() => (existsSync(opened) ? readFile(opened) : "")).toBe(`${config}\n`);
      expect(readFile(config)).toContain("# Unfold settings for the ts-orders playground.");
    });

    await test.step("without an unfold.toml, Open unfold.toml creates an empty one first", async () => {
      rmSync(opened);
      rmSync(config);
      await homeMain(page).getByRole("button", { name: "Project menu" }).click();
      await chooseMenuItem(page, "Project menu", "Open unfold.toml");
      await expect.poll(() => (existsSync(opened) ? readFile(opened) : "")).toBe(`${config}\n`);
      expect(readFile(config)).toBe("");
    });

    await test.step("an editor that can't run shows why in a toast", async () => {
      await setEditorCommand(page, join(sandbox.root, "no-such-editor"));
      await projectMenu(page, "ts-orders", "Open unfold.toml");
      await expect(toast(page, "Couldn't open the file")).toBeVisible();
    });
  });

  test("Cancel stops the first compile, and Compile again finishes it", async ({ sandbox, launch }) => {
    test.setTimeout(300_000);
    const folder = sandbox.folder("synthetic");
    writeSyntheticRepo(folder, 8000, "main");
    const { page } = await launchWithProject(launch, folder);
    const project = await projectId(page, "synthetic");

    await test.step("while it compiles, the row shows progress and only Cancel applies", async () => {
      await expect(rowState(page, "synthetic")).toHaveAttribute("data-state", "compiling");
      const row = projectRow(page, "synthetic");
      await row.hover();
      await row.getByRole("button", { name: "Project menu" }).click();
      const menu = page.getByRole("menu", { name: "Project menu" });
      await expect(menu.getByRole("menuitem", { name: "Compile again" })).toHaveAttribute("aria-disabled", "true");
      await chooseMenuItem(page, "Project menu", "Cancel");
    });

    await test.step("Cancel leaves the project without a graph", async () => {
      await expect(rowState(page, "synthetic")).toHaveAttribute("data-state", "not_compiled");
      await expect(stateLine(page)).toContainText("Not compiled yet");
    });

    await test.step("Compile again compiles it", async () => {
      const mark = await eventMark(page, "project_state");
      await projectMenu(page, "synthetic", "Compile again");
      await compiledAgain(page, project, mark);
      await expect(stateLine(page)).toContainText("8,000 files");
    });
  });

  test("Remove from workspace… asks, keeps .unfold/, and a folder can be added again", async ({ sandbox, launch }) => {
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
      await expect(homeMain(page).getByRole("heading", { name: "Read a codebase by how it starts." })).toBeVisible();
    });

    await test.step("Add project adds a removed folder again, and its graph is current", async () => {
      await app.stubFolderDialog(orders);
      await homeMain(page).getByRole("button", { name: "Add project", exact: true }).click();
      await expect(projectRow(page, "ts-orders")).toBeVisible();
      await expect(homeTitle(page)).toHaveText("ts-orders");
      await expectGraphCurrent(page);
      expect((await workspace(page)).projects.map((row) => row.name)).toEqual(["ts-orders"]);
    });
  });
});
