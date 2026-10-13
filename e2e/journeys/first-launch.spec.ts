/**
 * First launch and connecting projects (docs/ui-shell.md#screens): an empty workspace, Add project with the
 * picker, from the sidebar, and from File › Add Project…; then the not-compiled Home with its scope tree
 * and the first Compile.
 */

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "../src/fixtures";
import {
  compileButton,
  expectGraphCurrent,
  homeMain,
  homeTitle,
  howItStarts,
  projectId,
  projectRow,
  rowState,
  sidebar,
  stateLine,
  workspace,
} from "../src/home";
import { launchWithProject } from "../src/flows";
import { writeFile } from "../src/sandbox";
import { eventMark, expectConnection, invoke, waitForEvent } from "../src/window";

interface SearchResults {
  readonly definitions: { readonly hits: readonly { readonly name: string }[] };
}

test.describe("first launch", () => {
  test("an empty workspace; Add project…, the sidebar's Add Project…, and File › Add Project…", async ({ sandbox, launch }) => {
    const orders = sandbox.playground("ts-orders");
    const python = sandbox.playground("py-orders");
    const later = sandbox.playground("py-orders", "later-orders");
    const app = await launch();
    const page = await app.firstWindow();
    await expectConnection(page, "connected");

    await test.step("the first launch shows an empty workspace", async () => {
      await expect(homeMain(page).getByRole("heading", { name: "Read a codebase by how it starts." })).toBeVisible();
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

    await test.step("Add project… adds the chosen folder and shows it, not compiled, writing nothing", async () => {
      await app.stubFolderDialog(orders);
      await homeMain(page).getByRole("button", { name: /^Add project…/ }).click();
      await expect(projectRow(page, "ts-orders")).toBeVisible();
      await expect(homeTitle(page)).toHaveText("ts-orders");
      await expect(rowState(page, "ts-orders")).toHaveAttribute("data-state", "not_compiled");
      await expect(stateLine(page)).toContainText("Not compiled yet");
      // Connecting a project writes nothing into its folder.
      expect(existsSync(join(orders, ".unfold", "local"))).toBe(false);
    });

    await test.step("the sidebar's Add Project… adds another", async () => {
      await app.stubFolderDialog(python);
      await sidebar(page).getByRole("button", { name: "Add Project…" }).click();
      await expect(projectRow(page, "py-orders")).toBeVisible();
      await expect(homeTitle(page)).toHaveText("py-orders");
    });

    await test.step("File › Add Project… in the application menu adds one too", async () => {
      await app.stubFolderDialog(later);
      await app.clickMenu(page, ["File", "Add Project…"]);
      await expect(projectRow(page, "later-orders")).toBeVisible();
      await expect(homeTitle(page)).toHaveText("later-orders");
      expect((await workspace(page)).projects.map((row) => row.name)).toEqual(["ts-orders", "py-orders", "later-orders"]);
    });
  });

  test("the not-compiled Home: what it reads and skips, the scope tree, and the first Compile", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    writeFile(join(folder, "extra", "extra.ts"), "export function extraThing(): number {\n  return 1;\n}\n");
    const { page } = await launchWithProject(launch, folder);

    await test.step("the added folder shows, not compiled, with what Unfold will read", async () => {
      await expect(homeMain(page).getByText("Unfold will read 35 TypeScript files.")).toBeVisible();
    });

    await test.step("Skipped says what isn't read and why", async () => {
      const skipped = homeMain(page).getByRole("region", { name: "Skipped" });
      await expect(skipped).toContainText("JavaScript isn't graphed");
    });

    await test.step("unchecking a top-level folder in the scope tree leaves it out", async () => {
      const scope = homeMain(page).getByRole("region", { name: "What to graph" });
      await expect(scope.getByText("35 of 35 files")).toBeVisible();
      const extra = scope.getByRole("checkbox", { name: "extra" });
      await expect(extra).toBeChecked();
      await extra.uncheck();
      await expect(scope.getByText("34 of 35 files")).toBeVisible();
      // Nothing is written before Compile.
      expect(readFileSync(join(folder, "unfold.toml"), "utf8")).not.toContain("extra");
    });

    await test.step("the promise says what Compile does", async () => {
      await expect(homeMain(page).getByText(/^Reads files only\. Runs nothing and installs nothing\. Creates \.unfold\/ in this folder/)).toBeVisible();
    });

    await test.step("Compile writes the scope, shows progress, and fills Home in", async () => {
      const project = await projectId(page, "ts-orders");
      const mark = await eventMark(page, "project_state");
      await compileButton(page).click();
      await waitForEvent(page, "project_state", mark, (event) => event["project"] === project && (event["state"] as { kind: string }).kind === "compiling");
      await expectGraphCurrent(page);
      await expect(stateLine(page)).toContainText(/Graph current\s*· 34 files · TypeScript/);
      await expect(rowState(page, "ts-orders")).toHaveAttribute("data-state", "current");
      await expect(howItStarts(page).getByRole("region", { name: "server" })).toBeVisible();
      expect(readFileSync(join(folder, "unfold.toml"), "utf8")).toContain("extra");
      const found = await invoke<SearchResults>(page, "search", { query: "extraThing", kinds: ["definitions"] });
      expect(found.definitions.hits).toEqual([]);
      expect(existsSync(join(folder, ".unfold", "local"))).toBe(true);
    });

    await test.step("after the first Compile, Compile lives in the project menu", async () => {
      await expect(compileButton(page)).toHaveCount(0);
    });
  });
});
