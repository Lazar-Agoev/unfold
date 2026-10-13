/**
 * First launch and connecting projects (docs/ui-shell.md#screens): an empty workspace; Add project with the
 * picker, from the sidebar, and from File › Add Project…, each compiling the folder at once; and a
 * folder Unfold can't write `.unfold/` into, which is refused.
 */

import { existsSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "../src/fixtures";
import { expectGraphCurrent, homeMain, homeTitle, howItStarts, projectId, projectRow, rowState, sidebar, stateLine, toast, workspace } from "../src/home";
import { eventMark, expectConnection, waitForEvent } from "../src/window";

test.describe("first launch", () => {
  test("an empty workspace; Add project, the sidebar's Add Project…, and File › Add Project… compile at once", async ({ sandbox, launch }) => {
    const orders = sandbox.playground("ts-orders");
    const python = sandbox.playground("py-orders");
    const later = sandbox.playground("py-orders", "later-orders");
    const app = await launch();
    const page = await app.firstWindow();
    await expectConnection(page, "connected");

    await test.step("the first launch shows an empty workspace: a headline, one line, and Add project", async () => {
      await expect(homeMain(page).getByRole("heading", { name: "Read a codebase by how it starts." })).toBeVisible();
      await expect(homeMain(page).getByText("Add a folder. Unfold reads its TypeScript and Python, and never runs it.")).toBeVisible();
      await expect(homeMain(page).getByRole("button")).toHaveText(["Add project"]);
      await expect(sidebar(page).getByText("No projects yet")).toBeVisible();
      expect((await workspace(page)).projects).toEqual([]);
    });

    await test.step("the window is one sidebar and the content: no tabs, activity bar, inspector, panel, or status bar", async () => {
      await expect(page.locator("body > #root > .app > *:not([hidden])")).toHaveCount(2);
      await expect(page.getByRole("tablist")).toHaveCount(0);
      await expect(page.getByRole("complementary")).toHaveCount(1);
      await expect(page.getByRole("main")).toHaveCount(1);
      await expect(page.getByRole("contentinfo")).toHaveCount(0);
    });

    await test.step("Add project adds the chosen folder and compiles it at once", async () => {
      const mark = await eventMark(page, "project_state");
      await app.stubFolderDialog(orders);
      await homeMain(page).getByRole("button", { name: "Add project", exact: true }).click();
      await expect(projectRow(page, "ts-orders")).toBeVisible();
      await expect(homeTitle(page)).toHaveText("ts-orders");
      const project = await projectId(page, "ts-orders");
      await waitForEvent(page, "project_state", mark, (event) => event["project"] === project && (event["state"] as { kind: string }).kind === "compiling");
      await expectGraphCurrent(page);
      await expect(stateLine(page)).toContainText(/Graph current\s*· 34 files · TypeScript · updated/);
      await expect(rowState(page, "ts-orders")).toHaveAttribute("data-state", "current");
      await expect(howItStarts(page).getByRole("region", { name: "server" })).toBeVisible();
      expect(existsSync(join(orders, ".unfold", "local"))).toBe(true);
    });

    await test.step("the sidebar's Add Project… adds another", async () => {
      await app.stubFolderDialog(python);
      await sidebar(page).getByRole("button", { name: "Add Project…" }).click();
      await expect(projectRow(page, "py-orders")).toBeVisible();
      await expect(homeTitle(page)).toHaveText("py-orders");
      await expectGraphCurrent(page);
    });

    await test.step("File › Add Project… in the application menu adds one too", async () => {
      await app.stubFolderDialog(later);
      await app.clickMenu(page, ["File", "Add Project…"]);
      await expect(projectRow(page, "later-orders")).toBeVisible();
      await expect(homeTitle(page)).toHaveText("later-orders");
      await expectGraphCurrent(page);
      expect((await workspace(page)).projects.map((row) => row.name)).toEqual(["ts-orders", "py-orders", "later-orders"]);
    });
  });

  test("a folder Unfold can't write .unfold/ into is refused with the engine's reason", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    sandbox.setReadOnly(folder, true);
    const app = await launch();
    const page = await app.firstWindow();
    await expectConnection(page, "connected");

    await test.step("Add project shows why, and the workspace stays empty", async () => {
      await app.stubFolderDialog(folder);
      await homeMain(page).getByRole("button", { name: "Add project", exact: true }).click();
      const refused = toast(page, "Couldn't add the folder");
      await expect(refused).toBeVisible();
      await expect(refused).toContainText("the folder is read-only");
      expect((await workspace(page)).projects).toEqual([]);
      await expect(sidebar(page).getByText("No projects yet")).toBeVisible();
      expect(existsSync(join(folder, ".unfold", "local")), "nothing is written into the folder").toBe(false);
    });
  });
});
