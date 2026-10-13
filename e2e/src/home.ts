/**
 * The window in Home mode: the sidebar's projects, Home's header and state line, its Boards, and How it
 * starts. Locators use what a person sees (roles and names); waits are on state.
 */

import type { Locator, Page } from "@playwright/test";
import { expect } from "@playwright/test";
import type { UnfoldApp } from "./app";
import { escapeRegExp, expectConnection, invoke } from "./window";

/** A row of `workspace`'s projects, as the journeys read it. */
interface ProjectRowInfo {
  readonly id: string;
  readonly name: string;
  readonly path: string;
  readonly live: boolean;
  readonly state: { readonly kind: string };
}

interface WorkspaceInfo {
  readonly projects: readonly ProjectRowInfo[];
}

export function workspace(page: Page): Promise<WorkspaceInfo> {
  return invoke<WorkspaceInfo>(page, "workspace");
}

/** The id of the project named `name` (its folder's name). */
export async function projectId(page: Page, name: string): Promise<string> {
  const row = (await workspace(page)).projects.find((project) => project.name === name);
  if (!row) {
    throw new Error(`no project named ${name}`);
  }
  return row.id;
}

// --- Sidebar ----------------------------------------------------------------------------------

export function sidebar(page: Page): Locator {
  return page.getByRole("complementary", { name: "Sidebar" });
}

/** Goes back to Home from the shown board (the chevron of the board sidebar's crumb). */
export async function showHome(page: Page): Promise<void> {
  await sidebar(page).getByRole("button", { name: "Home", exact: true }).click();
  await expect(homeMain(page)).toBeVisible();
}

/** The projects tree of Home's sidebar. */
function projectsTree(page: Page): Locator {
  return sidebar(page).getByRole("tree", { name: "Projects" });
}

/** A project's row in the sidebar, by its name (its folder's). */
export function projectRow(page: Page, name: string): Locator {
  return projectsTree(page).locator(".home-project-row").filter({ has: page.locator(".home-project-title", { hasText: new RegExp(`^${escapeRegExp(name)}$`) }) });
}

/** The state a project row's indicator shows (`current`, `compiling`, `problems`, …). */
export function rowState(page: Page, name: string): Locator {
  return projectRow(page, name).locator(".state-indicator");
}

/** A board's row under its project in the sidebar tree, by its title. */
export function sidebarBoard(page: Page, title: string): Locator {
  return projectsTree(page).locator(".home-tree-row[data-board-key]").filter({ has: page.locator(".home-tree-label", { hasText: new RegExp(`^${escapeRegExp(title)}$`) }) });
}

/** A folder's row in the sidebar tree, by its name. */
export function sidebarFolder(page: Page, name: string): Locator {
  return projectsTree(page).locator(".home-tree-row:not([data-board-key])").filter({ has: page.locator(".home-tree-label", { hasText: new RegExp(`^${escapeRegExp(name)}$`) }) });
}

/** Shows a project on Home (a click on its row). */
export async function selectProject(page: Page, name: string): Promise<void> {
  await projectRow(page, name).locator(".home-project-title").click();
  await expect(projectRow(page, name)).toHaveAttribute("aria-selected", "true");
  await expect(homeTitle(page)).toHaveText(name);
}

/** Opens the project menu (`⋯` on the project's sidebar row) and chooses `item`. */
export async function projectMenu(page: Page, name: string, item: string | RegExp): Promise<void> {
  const row = projectRow(page, name);
  await row.hover();
  await row.getByRole("button", { name: "Project menu" }).click();
  await chooseMenuItem(page, "Project menu", item);
}

/** Chooses an item of the open menu named `menu`. */
export async function chooseMenuItem(page: Page, menu: string, item: string | RegExp): Promise<void> {
  const open = page.getByRole("menu", { name: menu });
  await expect(open).toBeVisible();
  await open.getByRole("menuitem", { name: item, exact: true }).click();
  await expect(open).toBeHidden();
}

// --- Home's main area -------------------------------------------------------------------------

export function homeMain(page: Page): Locator {
  return page.locator(".home-main");
}

/** The project name Home shows (its 32px heading). */
export function homeTitle(page: Page): Locator {
  return homeMain(page).locator(".home-title");
}

/** Home's state line ("Graph current · 34 files · TypeScript · updated just now"). */
export function stateLine(page: Page): Locator {
  return homeMain(page).locator(".home-state-line");
}

/** Waits until Home's project reads "Graph current" (after a compile, an update, a reopen). */
export async function expectGraphCurrent(page: Page, timeout = 60_000): Promise<void> {
  await expect(stateLine(page)).toHaveAttribute("data-state", "current", { timeout });
}

/** The rows of Home's Boards: the project's boards, most recently edited first. */
export function homeBoardRows(page: Page): Locator {
  return homeMain(page).getByRole("group", { name: "Boards" }).locator(".home-board-row");
}

/** Adds `folder` as a project with Add Project… in the sidebar (the folder picker stubbed). */
export async function addProjectFromSidebar(app: UnfoldApp, page: Page, folder: string): Promise<void> {
  await app.stubFolderDialog(folder);
  await expectConnection(page, "connected");
  await sidebar(page).getByRole("button", { name: "Add Project…" }).click();
}

/** Home's How it starts section. */
export function howItStarts(page: Page): Locator {
  return homeMain(page).getByRole("region", { name: "How it starts" });
}

/** An application's row in How it starts (with its launches, and its entrances while expanded), by name. */
export function applicationRow(page: Page, name: string): Locator {
  return howItStarts(page).getByRole("region", { name, exact: true });
}

/** Expands `application` in How it starts, or every application with entrances besides its launches. */
export async function showEntrances(page: Page, application?: string): Promise<void> {
  const scope = application === undefined ? howItStarts(page) : applicationRow(page, application);
  await expect(howItStarts(page).locator(".home-launch-table")).toBeVisible();
  for (const toggle of await scope.getByRole("button", { name: /'s entrances$/ }).all()) {
    if ((await toggle.getAttribute("aria-expanded")) === "false") {
      await toggle.click();
      await expect(toggle).toHaveAttribute("aria-expanded", "true");
    }
  }
}

/**
 * An entrance's row in How it starts, by its kind column (`POST`, `COMMAND`, `LAUNCH`) and trigger,
 * optionally in one application. Rows show while their application is expanded (`showEntrances`) or
 * under "Not reached from a launch".
 */
export function entranceRow(page: Page, kind: string, trigger: string, application?: string): Locator {
  const scope = application === undefined ? howItStarts(page) : applicationRow(page, application);
  return scope.getByRole("button", { name: new RegExp(`^${escapeRegExp(kind)} ${escapeRegExp(trigger)}( |$)`) });
}

// --- Window-wide layers -----------------------------------------------------------------------

/** A toast in the window's bottom-right corner, by its text. */
export function toast(page: Page, text: string | RegExp): Locator {
  return page.getByRole("region", { name: "Notifications" }).locator(".toast").filter({ hasText: text });
}

/** An open dialog (or confirmation), by its title. */
export function dialog(page: Page, title: string | RegExp): Locator {
  return page.getByRole("dialog", { name: title }).or(page.getByRole("alertdialog", { name: title }));
}
