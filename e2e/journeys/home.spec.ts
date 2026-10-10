/**
 * Home of a compiled project (redesign §4.3): the header with the branch and worktree switcher, How it
 * starts and Trace, the Boards section, the sidebar's tree, Recents and Drafts; then Health and
 * activity for a project with a file that doesn't parse.
 */

import { join } from "node:path";
import { boardView } from "../src/board";
import { expect, test } from "../src/fixtures";
import { traceEntrance } from "../src/flows";
import {
  boardTab,
  compileButton,
  entranceRow,
  expectGraphCurrent,
  homeMain,
  homeTab,
  homeTitle,
  howItStarts,
  projectRow,
  rowState,
  showHome,
  sidebar,
  sidebarBoard,
  stateLine,
  tabs,
} from "../src/home";
import { writeFile } from "../src/sandbox";
import { enterText } from "../src/window";

test.describe("Home", () => {
  test("a compiled project's Home: header, How it starts, Trace, Boards, the sidebar tree, Recents and Drafts", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    // A repository on a feature branch: Unfold reads Git's files (it never runs Git).
    writeFile(join(folder, ".git", "HEAD"), "ref: refs/heads/feature-x\n");
    const app = await launch({ args: [folder] });
    const page = await app.firstWindow();
    await expect(homeTitle(page)).toHaveText("ts-orders");
    await compileButton(page).click();
    await expectGraphCurrent(page);

    await test.step("the header shows the path, the name, the branch, and the graph's state in words", async () => {
      await expect(homeMain(page).locator(".home-path")).toContainText("ts-orders");
      await expect(stateLine(page)).toContainText(/Graph current\s*· 34 files · TypeScript · updated/);
      await expect(projectRow(page, "ts-orders").locator(".home-project-branch")).toHaveText("feature-x");
      await expect(rowState(page, "ts-orders")).toHaveAttribute("data-state", "current");
    });

    await test.step("the worktree switcher lists the worktrees and says what is per worktree", async () => {
      await homeMain(page).getByRole("button", { name: "Branch feature-x. Switch worktree" }).click();
      const worktrees = page.getByRole("menu", { name: "Worktrees" });
      await expect(worktrees.getByRole("menuitemradio", { checked: true })).toContainText("feature-x");
      await expect(page.getByText("Drafts and the graph are per worktree.")).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(worktrees).toBeHidden();
    });

    await test.step("How it starts groups the entrances by application", async () => {
      await expect(howItStarts(page).getByText("18 entrances in 4 applications.", { exact: false })).toBeVisible();
      for (const application of ["cli", "scheduler", "server", "worker"]) {
        await expect(howItStarts(page).getByRole("region", { name: application, exact: true })).toBeVisible();
      }
      await expect(entranceRow(page, "COMMAND", "import", "cli")).toBeVisible();
      await expect(entranceRow(page, "WORKER", "emails", "worker")).toBeVisible();
    });

    await test.step("an application with more than eight entrances shows eight, then N more · Filter", async () => {
      const server = howItStarts(page).getByRole("region", { name: "server", exact: true });
      await expect(server.locator(".home-entrance-row")).toHaveCount(8);
      await expect(server.getByText("1 more route")).toBeVisible();
      await server.getByRole("button", { name: "Filter", exact: true }).click();
      await expect(server.locator(".home-entrance-row")).toHaveCount(9);
      await enterText(server.getByRole("searchbox", { name: "Filter server's entrances" }), "health");
      await expect(server.locator(".home-entrance-row")).toHaveCount(2);
      await expect(entranceRow(page, "GET", "/health/live", "server")).toBeVisible();
      await server.getByRole("button", { name: "Show fewer" }).click();
      await expect(server.locator(".home-entrance-row")).toHaveCount(8);
    });

    await test.step("Launches into code that isn't indexed expands from collapsed", async () => {
      const launches = howItStarts(page).getByRole("button", { name: /^Launches into code that isn't indexed · 1/ });
      await expect(launches).toHaveAttribute("aria-expanded", "false");
      await launches.click();
      await expect(launches).toHaveAttribute("aria-expanded", "true");
      await expect(howItStarts(page).locator(".home-collapsed-body .home-launch-row")).toHaveCount(1);
    });

    await test.step("Trace opens an entrance on a new page of a new draft board", async () => {
      await traceEntrance(page, "POST", "/orders");
      await expect(boardTab(page, "POST /orders")).toHaveAttribute("aria-selected", "true");
      await expect(boardView(page).locator(".board-publish-text")).toHaveText("Draft");
      await expect(boardView(page).getByRole("article", { name: "POST /orders" })).toBeVisible();
    });

    const boards = homeMain(page).getByRole("region", { name: "Boards" });

    await test.step("Home's Boards shows the board as a card with a thumbnail of its first page", async () => {
      await showHome(page);
      const board = boards.getByRole("button", { name: "POST /orders" });
      await expect(board).toBeVisible();
      // Drawn from card positions: the entrance and its three handlers.
      await expect(board.locator(".home-thumb-card")).toHaveCount(4);
    });

    await test.step("New board on Home makes an empty draft and opens it", async () => {
      await homeMain(page).locator(".home-header").getByRole("button", { name: "New board" }).click();
      await expect(boardTab(page, "Untitled board")).toHaveAttribute("aria-selected", "true");
      await expect(boardView(page).getByText("Open an entrance or a definition to start this page.")).toBeVisible();
      await showHome(page);
    });

    await test.step("Boards filters All, Published, and Drafts, and sorts by last edited or title", async () => {
      const titles = boards.locator(".home-board-title");
      await expect(titles).toHaveText(["Untitled board", "POST /orders"]);
      const show = boards.getByRole("radiogroup", { name: "Show" });
      await show.getByRole("radio", { name: "Published" }).click();
      await expect(boards.getByText("No published boards")).toBeVisible();
      await show.getByRole("radio", { name: "Drafts" }).click();
      await expect(titles).toHaveText(["Untitled board", "POST /orders"]);
      await show.getByRole("radio", { name: "All" }).click();
      await boards.getByRole("button", { name: "Last edited" }).click();
      await page.getByRole("menu", { name: "Sort boards" }).getByRole("menuitemcheckbox", { name: "Title" }).click();
      await expect(titles).toHaveText(["POST /orders", "Untitled board"]);
    });

    await test.step("the sidebar lists the project's boards, and a board row opens its board", async () => {
      await expect(sidebarBoard(page, "POST /orders")).toBeVisible();
      await expect(sidebarBoard(page, "Untitled board")).toBeVisible();
      // Close its tab, so the row opens the board again (and records it as a recent board).
      await boardTab(page, "POST /orders").hover();
      await page.getByRole("button", { name: "Close POST /orders" }).click();
      await expect(tabs(page)).toHaveCount(2);
      await sidebarBoard(page, "POST /orders").click();
      await expect(boardTab(page, "POST /orders")).toHaveAttribute("aria-selected", "true");
      await expect(boardView(page).getByRole("article", { name: "POST /orders" })).toBeVisible();
      await expect(tabs(page)).toHaveCount(3);
      await homeTab(page).click();
    });

    await test.step("Recents lists the boards opened lately, across projects", async () => {
      await sidebar(page).getByRole("button", { name: "Recents" }).click();
      await expect(homeMain(page).getByRole("heading", { name: "Recents", level: 1 })).toBeVisible();
      const recents = homeMain(page).getByRole("group", { name: "Recent boards" });
      await expect(recents.getByRole("button", { name: "POST /orders" })).toContainText(/ts-orders ·.*1 page · Draft · opened/);
    });

    await test.step("Drafts lists every project's drafts, and a card opens its board", async () => {
      await sidebar(page).getByRole("button", { name: "Drafts" }).click();
      await expect(homeMain(page).getByRole("heading", { name: "Drafts", level: 1 })).toBeVisible();
      const drafts = homeMain(page).getByRole("group", { name: "Drafts" });
      await expect(drafts.getByRole("button")).toHaveCount(2);
      await drafts.getByRole("button", { name: "Untitled board" }).click();
      await expect(boardTab(page, "Untitled board")).toHaveAttribute("aria-selected", "true");
    });

    await test.step("a project row shows the project's overview on Home", async () => {
      await homeTab(page).click();
      await projectRow(page, "ts-orders").locator(".home-project-title").click();
      await expect(howItStarts(page)).toBeVisible();
    });
  });

  test("Health and activity: a file that doesn't parse, the Health list, and the Activity log", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    writeFile(join(folder, "src", "broken.ts"), "export function broken( {\n  return 1;\n");
    const app = await launch({ args: [folder] });
    const page = await app.firstWindow();
    await expect(homeTitle(page)).toHaveText("ts-orders");
    await compileButton(page).click();

    await test.step("the project row and the state line say a file doesn't parse", async () => {
      await expect(rowState(page, "ts-orders")).toHaveAttribute("data-state", "problems", { timeout: 60_000 });
      await expect(stateLine(page)).toContainText("1 file doesn't parse");
    });

    const health = homeMain(page).getByRole("region", { name: "Health", exact: true });

    await test.step("the Health line sums the problems up, each with its action", async () => {
      await expect(health).toContainText("1 file doesn't parse");
      await expect(health).toContainText("1 launch into code that isn't indexed");
      await expect(health.getByRole("button", { name: "See Health" })).toBeVisible();
    });

    const section = homeMain(page).getByRole("region", { name: "Health and activity" });
    const show = section.getByRole("radiogroup", { name: "Show" });

    await test.step("Health and activity lists the file that doesn't parse, with Open", async () => {
      await health.getByRole("button", { name: "Health and activity" }).click();
      await expect(show.getByRole("radio", { name: /^Health/ })).toHaveAttribute("aria-checked", "true");
      const row = section.locator(".home-health-row").filter({ hasText: "src/broken.ts" });
      await expect(row).toBeVisible();
      await expect(row.getByRole("button", { name: "Open" })).toBeVisible();
    });

    await test.step("Activity shows the index log with the Compile", async () => {
      await show.getByRole("radio", { name: "Activity" }).click();
      const log = section.getByRole("table", { name: "Activity" });
      await expect(log.getByRole("cell", { name: "Compile", exact: true })).toBeVisible();
      await expect(section.getByRole("button", { name: "Copy diagnostics" })).toBeVisible();
    });

    await test.step("Back to overview returns to How it starts", async () => {
      await section.getByRole("button", { name: "Back to overview" }).click();
      await expect(howItStarts(page)).toBeVisible();
    });
  });
});
