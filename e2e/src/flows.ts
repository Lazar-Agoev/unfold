/** Multi-step flows journeys start with. */

import type { Page } from "@playwright/test";
import { expect } from "@playwright/test";
import type { UnfoldApp } from "./app";
import type { Fixtures } from "./fixtures";
import { addProjectFromSidebar, expectGraphCurrent, homeTitle, projectRow } from "./home";
import type { PlaygroundName, Sandbox } from "./sandbox";

interface Compiled {
  readonly app: UnfoldApp;
  readonly page: Page;
  /** The sandbox copies of the playgrounds, in the order given. */
  readonly folders: readonly string[];
}

/**
 * Launches the app on an empty workspace and adds copies of `playgrounds` with Add Project…, each
 * compiled before the next. Home ends on the last one.
 */
export async function launchCompiled(
  launch: Fixtures["launch"],
  sandbox: Sandbox,
  playgrounds: readonly PlaygroundName[] = ["ts-orders"],
): Promise<Compiled> {
  const app = await launch();
  const page = await app.firstWindow();
  const folders = playgrounds.map((name) => sandbox.playground(name));
  for (const [index, folder] of folders.entries()) {
    const name = playgrounds[index] ?? "";
    await addProjectFromSidebar(app, page, folder);
    await expect(projectRow(page, name)).toBeVisible();
    await expect(homeTitle(page)).toHaveText(name);
    await expectGraphCurrent(page);
  }
  return { app, page, folders };
}
