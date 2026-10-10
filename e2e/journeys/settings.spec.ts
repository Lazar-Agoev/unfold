/**
 * Settings (redesign §4.11): Project settings (Scope, Detectors, General's name, and the guard on
 * unsaved edits), the manual entrance editor (from the project menu, from Home, and from a card), and
 * Preferences (the theme, and a key binding stored by action id).
 */

import { join } from "node:path";
import type { Locator, Page } from "@playwright/test";
import { card, cardMenu } from "../src/board";
import { expect, test } from "../src/fixtures";
import { CREATE_ORDER, launchCompiled, traceEntrance } from "../src/flows";
import {
  dialog,
  entranceRow,
  expectGraphCurrent,
  homeMain,
  homeTitle,
  howItStarts,
  projectId,
  projectMenu,
  projectRow,
  showHome,
  sidebar,
  stateLine,
  toast,
  workspace,
} from "../src/home";
import { readFile } from "../src/sandbox";
import { enterText, eventMark, invoke, waitForEvent } from "../src/window";

interface SearchResults {
  readonly definitions: { readonly hits: readonly { readonly name: string }[] };
}

interface Preferences {
  readonly theme: string;
  readonly keyBindings: Readonly<Record<string, string>>;
}

/** The project settings dialog of the project named `name`. */
function settingsDialog(page: Page, name: string): Locator {
  return dialog(page, `${name} settings`);
}

/** Opens a section of the open settings dialog. */
async function showSection(settings: Locator, section: string): Promise<Locator> {
  await settings.getByRole("navigation", { name: "Sections" }).getByRole("button", { name: section, exact: true }).click();
  const pane = settings.getByRole("region", { name: section, exact: true });
  await expect(pane).toBeVisible();
  return pane;
}

/** Saves the open settings dialog and waits for it to close. */
async function saveSettings(page: Page, settings: Locator): Promise<void> {
  await settings.getByRole("button", { name: "Save", exact: true }).click();
  await expect(settings).toBeHidden();
  await dismiss(toast(page, /^Saved .* settings/));
}

/**
 * Waits for a toast and dismisses it. Toasts sit in the bottom-right corner over dialogs' footers, so
 * one left showing would cover the next dialog's Save until it hides itself.
 */
async function dismiss(shown: Locator): Promise<void> {
  await expect(shown).toBeVisible();
  await shown.hover();
  await shown.getByRole("button", { name: "Dismiss" }).click();
  await expect(shown).toBeHidden();
}

/**
 * Shows How it starts' "Not reached from a launch" (where a manual entrance no launch reaches is
 * listed) and waits for the row of `trigger` there.
 */
async function expectUnreached(page: Page, kind: string, trigger: string): Promise<void> {
  const section = howItStarts(page).getByRole("button", { name: /^Not reached from a launch · / });
  await expect(section).toBeVisible();
  if ((await section.getAttribute("aria-expanded")) !== "true") {
    await section.click();
  }
  await expect(section).toHaveAttribute("aria-expanded", "true");
  await expect(entranceRow(page, kind, trigger)).toBeVisible();
}

async function definitionNames(page: Page, project: string, query: string): Promise<string[]> {
  const found = await invoke<SearchResults>(page, "search", { query, kinds: ["definitions"], projects: [project] });
  return found.definitions.hits.map((hit) => hit.name);
}

test.describe("project settings", () => {
  test("Scope excludes a folder, Detectors turn one off, and General renames the project", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    const config = join(folder, ".unfold", "config.toml");
    const project = await projectId(page, "ts-orders");
    const settings = settingsDialog(page, "ts-orders");

    await test.step("Home's Settings opens the settings dialog on General", async () => {
      await homeMain(page).getByRole("button", { name: "Settings", exact: true }).click();
      await expect(settings).toBeVisible();
      await expect(settings.getByRole("region", { name: "General" })).toBeVisible();
    });

    await test.step("Scope: unchecking a folder in a lazily loaded level stages an exclude rule", async () => {
      const scope = await showSection(settings, "Scope");
      const tree = scope.getByRole("tree", { name: "Folders in ts-orders" });
      const src = tree.getByRole("treeitem", { name: /^Graph src / });
      await src.locator(".tree-twisty").click();
      await expect(src).toHaveAttribute("aria-expanded", "true");
      const jobs = tree.getByRole("checkbox", { name: "Graph src/jobs" });
      await expect(jobs).toBeChecked();
      await jobs.uncheck();
      await expect(scope.getByRole("region", { name: "Rules" }).locator(".scope-rule")).toContainText(["src/jobs"]);
    });

    await test.step("Save writes the rule to config.toml, keeping its comments, and a background update drops the folder", async () => {
      const mark = await eventMark(page, "graph_updated");
      await saveSettings(page, settings);
      const text = readFile(config);
      expect(text).toContain("exclude");
      expect(text).toContain("src/jobs");
      expect(text).toContain("# Unfold settings for the ts-orders playground.");
      await waitForEvent(page, "graph_updated", mark, (event) => event["project"] === project);
      await expect(stateLine(page)).toContainText("28 files", { timeout: 30_000 });
      await expect.poll(() => definitionNames(page, project, "cleanupExpiredOrders")).toEqual([]);
      expect(await definitionNames(page, project, "createOrder")).toContain("createOrder");
    });

    await test.step("Detectors: turning node-cron schedules off removes the schedules from How it starts", async () => {
      await expect(entranceRow(page, "SCHEDULE", "*/15 * * * *")).toBeVisible();
      await projectMenu(page, "ts-orders", "Settings…");
      const detectors = await showSection(settings, "Detectors");
      await detectors.getByRole("switch", { name: "node-cron schedules" }).click();
      await expect(detectors.getByRole("switch", { name: "node-cron schedules" })).toHaveAttribute("aria-checked", "false");
      await saveSettings(page, settings);
      expect(readFile(config)).toContain("node-cron.schedule");
      await expect(entranceRow(page, "SCHEDULE", "*/15 * * * *")).toHaveCount(0, { timeout: 30_000 });
      await expect(entranceRow(page, "POST", "/orders")).toBeVisible();
    });

    await test.step("Detectors: turning it back on brings the schedules back", async () => {
      await projectMenu(page, "ts-orders", "Settings…");
      const detectors = await showSection(settings, "Detectors");
      await detectors.getByRole("switch", { name: "node-cron schedules" }).click();
      await saveSettings(page, settings);
      await expect(entranceRow(page, "SCHEDULE", "*/15 * * * *")).toBeVisible({ timeout: 30_000 });
    });

    await test.step("closing with a staged edit asks to discard it", async () => {
      await projectMenu(page, "ts-orders", "Settings…");
      const general = await showSection(settings, "General");
      await enterText(general.getByRole("textbox", { name: "Name" }), "never-saved");
      await settings.getByRole("button", { name: "Cancel", exact: true }).click();
      const discard = dialog(page, "Discard changes?");
      await expect(discard).toBeVisible();
      await discard.getByRole("button", { name: "Discard changes" }).click();
      await expect(settings).toBeHidden();
      expect(readFile(config)).not.toContain("never-saved");
    });

    await test.step("General: a new shared name renames the project everywhere and writes [project] name", async () => {
      await homeMain(page).getByRole("button", { name: "Settings", exact: true }).click();
      const general = await showSection(settings, "General");
      await enterText(general.getByRole("textbox", { name: "Name" }), "orders-api");
      await saveSettings(page, settings);
      await expect(projectRow(page, "orders-api")).toBeVisible();
      await expect(projectRow(page, "ts-orders")).toHaveCount(0);
      await expect(homeTitle(page)).toHaveText("orders-api");
      expect((await workspace(page)).projects.map((row) => row.name)).toEqual(["orders-api"]);
      const text = readFile(config);
      expect(text).toContain("[project]");
      expect(text).toMatch(/name = "orders-api"/);
      await expectGraphCurrent(page);
    });
  });
});

test.describe("the entrance editor", () => {
  test("add a manual entrance from the project menu, edit and remove it from Home, and start one from a card", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const config = join(folders[0] ?? "", ".unfold", "config.toml");
    const editor = dialog(page, "Add entrance");

    await test.step("project menu › Add entrance… opens the editor; a command entrance is saved to config.toml", async () => {
      await projectMenu(page, "ts-orders", "Add entrance…");
      await expect(editor).toBeVisible();
      await expect(editor.getByRole("radio", { name: "Command" })).toBeChecked();
      await enterText(editor.getByRole("textbox", { name: "Trigger" }), "metrics");
      await enterText(editor.getByRole("textbox", { name: "Handler 1" }), "src/jobs/report.ts#reportMetrics");
      await editor.getByRole("button", { name: "Add entrance", exact: true }).click();
      await expect(editor).toBeHidden();
      await dismiss(toast(page, "Added the entrance in .unfold/config.toml"));
      expect(readFile(config)).toContain('trigger = "metrics"');
    });

    await test.step("the manual entrance shows on Home under How it starts (not reached from a launch)", async () => {
      await expectUnreached(page, "COMMAND", "metrics");
    });

    await test.step("its row's menu Edit… changes it", async () => {
      await entranceRow(page, "COMMAND", "metrics").click({ button: "right" });
      await page.getByRole("menu", { name: "Entrance menu" }).getByRole("menuitem", { name: "Edit…" }).click();
      const edit = dialog(page, "Edit entrance");
      await expect(edit).toBeVisible();
      await expect(edit.getByRole("textbox", { name: "Trigger" })).toHaveValue("metrics");
      await expect(edit.getByRole("textbox", { name: "Handler 1" })).toHaveValue("src/jobs/report.ts#reportMetrics");
      await enterText(edit.getByRole("textbox", { name: "Trigger" }), "stats");
      await edit.getByRole("button", { name: "Save", exact: true }).click();
      await expect(edit).toBeHidden();
      await dismiss(toast(page, "Saved the entrance in .unfold/config.toml"));
      await expectUnreached(page, "COMMAND", "stats");
      await expect(entranceRow(page, "COMMAND", "metrics")).toHaveCount(0);
      const text = readFile(config);
      expect(text).toContain('trigger = "stats"');
      expect(text).not.toContain('trigger = "metrics"');
    });

    await test.step("its row's menu Remove… removes it after asking", async () => {
      await entranceRow(page, "COMMAND", "stats").click({ button: "right" });
      await page.getByRole("menu", { name: "Entrance menu" }).getByRole("menuitem", { name: "Remove…" }).click();
      const confirm = dialog(page, "Remove entrance");
      await expect(confirm).toBeVisible();
      await confirm.getByRole("button", { name: "Remove", exact: true }).click();
      await expect(entranceRow(page, "COMMAND", "stats")).toHaveCount(0);
      await expect.poll(() => readFile(config)).not.toContain('trigger = "stats"');
      // The playground's own manual entrances stay.
      expect(readFile(config)).toContain('trigger = "import"');
    });

    await test.step("Home's Add entrance opens the editor too", async () => {
      await howItStarts(page).getByRole("button", { name: "Add entrance", exact: true }).click();
      await expect(editor).toBeVisible();
      await editor.getByRole("button", { name: "Cancel", exact: true }).click();
      await expect(editor).toBeHidden();
    });

    await test.step("a card's Add as entrance… prefills the handler with the card's definition", async () => {
      await traceEntrance(page, "POST", "/orders");
      await cardMenu(card(page, CREATE_ORDER), "Add as entrance…");
      await expect(editor).toBeVisible();
      await expect(editor.getByRole("textbox", { name: "Handler 1" })).toHaveValue("src/routes/orders.ts#createOrder");
      await editor.getByRole("button", { name: "Cancel", exact: true }).click();
      await expect(editor).toBeHidden();
      await showHome(page);
    });
  });
});

test.describe("preferences", () => {
  test("the theme applies at once and is saved; a key binding is stored by action id", async ({ sandbox, launch }) => {
    const { page } = await launchCompiled(launch, sandbox);
    const preferences = dialog(page, "Preferences");
    const root = page.locator("html");

    await test.step("the sidebar's gear opens Preferences; Dark applies at once and Save keeps it", async () => {
      await sidebar(page).getByRole("button", { name: "Preferences…" }).click();
      await expect(preferences).toBeVisible();
      await expect(root).not.toHaveAttribute("data-theme", /./);
      await preferences.getByRole("radiogroup", { name: "Theme" }).getByRole("radio", { name: "Dark" }).click();
      await expect(root).toHaveAttribute("data-theme", "dark");
      await preferences.getByRole("button", { name: "Save", exact: true }).click();
      await expect(preferences).toBeHidden();
      await expect(root).toHaveAttribute("data-theme", "dark");
      expect((await invoke<Preferences>(page, "get_preferences")).theme).toBe("dark");
    });

    await test.step("⌘, opens Preferences; Light previews, Cancel returns to the saved theme", async () => {
      await page.keyboard.press("Meta+Comma");
      await expect(preferences).toBeVisible();
      await preferences.getByRole("radiogroup", { name: "Theme" }).getByRole("radio", { name: "Light" }).click();
      await expect(root).toHaveAttribute("data-theme", "light");
      await preferences.getByRole("button", { name: "Cancel", exact: true }).click();
      await expect(preferences).toBeHidden();
      await expect(root).toHaveAttribute("data-theme", "dark");
    });

    await test.step("System follows the computer again", async () => {
      await page.keyboard.press("Meta+Comma");
      await preferences.getByRole("radiogroup", { name: "Theme" }).getByRole("radio", { name: "System" }).click();
      await preferences.getByRole("button", { name: "Save", exact: true }).click();
      await expect(preferences).toBeHidden();
      await expect(root).not.toHaveAttribute("data-theme", /./);
      expect((await invoke<Preferences>(page, "get_preferences")).theme).toBe("system");
    });

    await test.step("Keyboard: Toggle Sidebar rebound to ⇧⌘Y is stored under view.toggleSidebar", async () => {
      await page.keyboard.press("Meta+Comma");
      const record = preferences.getByRole("button", { name: "Shortcut for Toggle Sidebar" });
      await record.click();
      await expect(record).toHaveAttribute("aria-pressed", "true");
      await record.press("Meta+Shift+Y");
      await expect(record).toHaveAttribute("aria-pressed", "false");
      await expect(preferences.getByRole("button", { name: "Reset Toggle Sidebar" })).toBeEnabled();
      await preferences.getByRole("button", { name: "Save", exact: true }).click();
      await expect(preferences).toBeHidden();
      const bindings = (await invoke<Preferences>(page, "get_preferences")).keyBindings;
      expect(Object.keys(bindings)).toEqual(["view.toggleSidebar"]);
      expect(bindings["view.toggleSidebar"]).toMatch(/Shift\+Y$/);
    });

    await test.step("the new key toggles the sidebar and the old one no longer does", async () => {
      await expect(sidebar(page)).toBeVisible();
      await page.keyboard.press("Meta+Shift+Y");
      await expect(sidebar(page)).toBeHidden();
      await page.keyboard.press("Meta+Shift+Y");
      await expect(sidebar(page)).toBeVisible();
      await page.keyboard.press("Meta+B");
      // ⌘B does nothing now: Add opened with A afterwards proves the key went through the dispatcher.
      await page.keyboard.press("a");
      await expect(page.getByRole("dialog", { name: "Add" })).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(sidebar(page)).toBeVisible();
    });

    await test.step("Reset returns the action to its default and drops the stored binding", async () => {
      await page.keyboard.press("Meta+Comma");
      await preferences.getByRole("button", { name: "Reset Toggle Sidebar" }).click();
      await preferences.getByRole("button", { name: "Save", exact: true }).click();
      await expect(preferences).toBeHidden();
      expect((await invoke<Preferences>(page, "get_preferences")).keyBindings).toEqual({});
      await page.keyboard.press("Meta+B");
      await expect(sidebar(page)).toBeHidden();
      await page.keyboard.press("Meta+B");
      await expect(sidebar(page)).toBeVisible();
    });
  });
});
