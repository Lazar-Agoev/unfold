/**
 * Home of a compiled project (docs/ui-shell.md#screens): the header and its state line, How it starts
 * (applications by category, an expanded application, Not reached) and Trace, the Boards list (by last
 * edit), the Health line, and the sidebar's tree; then Health for a project with a file that doesn't
 * parse; the engine host restarting (§11.11) without losing an edit; and the app's Settings (the
 * editor command, and the commands that connect an agent).
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { AGENT_ADDRESS } from "../src/agents";
import { boardTitle, boardView, card, readBoard, selected, showBoard, shownBoardRef } from "../src/board";
import { expect, test } from "../src/fixtures";
import { launchCompiled, launchWithProject, openPlace, PLACE, traceEntrance } from "../src/flows";
import {
  applicationRow,
  dialog,
  entranceRow,
  expectGraphCurrent,
  homeBoardRows,
  homeMain,
  howItStarts,
  projectRow,
  rowState,
  showHome,
  sidebar,
  showEntrances,
  sidebarBoard,
  stateLine,
  toast,
} from "../src/home";
import { writeFile } from "../src/sandbox";
import { enterText, expectConnection, invoke } from "../src/window";

interface AppSettings {
  readonly editorCommand?: string;
}

test.describe("Home", () => {
  test("a compiled project's Home: header, How it starts, Trace, Boards, Health, and the sidebar tree", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    // A command entrance no launch reaches, for "Not reached from a launch".
    const config = join(folder, "unfold.toml");
    writeFile(config, `${readFileSync(config, "utf8")}\n[[entrance]]\nid = "metrics"\nkind = "command"\ntrigger = "metrics"\nhandler = "src/jobs/report.ts#reportMetrics"\n`);
    const { page } = await launchWithProject(launch, folder);
    await expectGraphCurrent(page);

    await test.step("the header shows the path, the name, the project menu, New board, and the graph's state in words", async () => {
      const header = homeMain(page).locator(".home-header");
      await expect(header.locator(".home-path")).toContainText("ts-orders");
      await expect(header.getByRole("button", { name: "Project menu" })).toBeVisible();
      await expect(header.getByRole("button", { name: "New board" })).toBeVisible();
      await expect(stateLine(page)).toContainText(/Graph current\s*· 34 files · TypeScript · updated just now/);
      await expect(rowState(page, "ts-orders")).toHaveAttribute("data-state", "current");
    });

    await test.step("How it starts lists the applications by category, each launch as folder $ command", async () => {
      await expect(howItStarts(page).getByText(/^\d+ applications · 7 launches · \d+ routes\. Hover a row to trace it on a new board\.$/)).toBeVisible();
      for (const band of ["Services", "Workers", "Scheduled"]) {
        await expect(howItStarts(page).getByRole("heading", { name: band, level: 3 })).toBeVisible();
      }
      for (const application of ["cli", "scheduler", "server", "worker"]) {
        await expect(applicationRow(page, application)).toBeVisible();
      }
      const server = applicationRow(page, "server");
      await expect(server.locator(".home-launch-folder").first()).toHaveText("ts-orders $");
      await expect(server).toContainText("npm run start");
      await expect(server).toContainText("npm run dev");
      await expect(server.getByText(/^\d+ routes$/)).toHaveCount(2);
      // A launch into code that isn't indexed keeps its row, and says so.
      await expect(howItStarts(page).locator(".home-launch").filter({ hasText: "npm run legacy:export" })).toContainText("not indexed");
    });

    await test.step("expanding an application lists its launches, then its routes by path", async () => {
      const server = applicationRow(page, "server");
      await expect(server.getByRole("button", { name: /^LAUNCH / })).toHaveCount(0);
      await showEntrances(page, "server");
      await expect(server.getByRole("button", { name: /^LAUNCH / })).toHaveCount(2);
      await expect(entranceRow(page, "POST", "/orders", "server")).toBeVisible();
      await expect(entranceRow(page, "GET", "/health/live", "server")).toBeVisible();
      await expect(entranceRow(page, "DELETE", "/orders/:id", "server")).toBeVisible();
      await expect(server.locator(".home-scope-heading-title", { hasText: "/orders" })).toBeVisible();
    });

    await test.step("Not reached from a launch expands to the entrances no application serves", async () => {
      const unreached = howItStarts(page).getByRole("button", { name: /^Not reached from a launch · \d+$/ });
      await expect(unreached).toHaveAttribute("aria-expanded", "false");
      await unreached.click();
      await expect(unreached).toHaveAttribute("aria-expanded", "true");
      await expect(entranceRow(page, "COMMAND", "metrics")).toBeVisible();
    });

    await test.step("an entrance's menu holds only Trace on a New Board", async () => {
      await entranceRow(page, "COMMAND", "metrics").click({ button: "right" });
      const menu = page.getByRole("menu", { name: "Entrance menu" });
      await expect(menu.getByRole("menuitem")).toHaveText(["Trace on a New Board"]);
      await page.keyboard.press("Escape");
      await expect(menu).toBeHidden();
    });

    await test.step("the Health line says every file parses", async () => {
      const health = homeMain(page).getByRole("region", { name: "Health", exact: true });
      await expect(health.locator(".home-health-item")).toHaveText(["All files parse"]);
      await expect(health.getByRole("button")).toHaveCount(0);
    });

    await test.step("Trace opens an entrance on a new page of a new board", async () => {
      await traceEntrance(page, "POST", "/orders");
      await expect(boardTitle(page)).toHaveText("POST /orders");
      await expect(boardView(page).getByRole("article", { name: "POST /orders" })).toBeVisible();
    });

    const rows = homeBoardRows(page);

    await test.step("Home's Boards lists the board with when it was last edited", async () => {
      await showHome(page);
      await expect(rows).toHaveCount(1);
      await expect(rows.locator(".home-board-title")).toHaveText(["POST /orders"]);
      await expect(rows.locator(".home-board-edited")).toHaveText(["edited just now"]);
    });

    await test.step("New board on Home makes an empty board and opens it", async () => {
      await homeMain(page).locator(".home-header").getByRole("button", { name: "New board" }).click();
      await expect(boardTitle(page)).toHaveText("Untitled board");
      await expect(boardView(page).getByText("Open an entrance or a definition to start this page.")).toBeVisible();
      await showHome(page);
      await expect(rows.locator(".home-board-title")).toHaveText(["Untitled board", "POST /orders"]);
    });

    await test.step("the sidebar lists the project's boards, and a board row opens its board", async () => {
      await expect(sidebarBoard(page, "POST /orders")).toBeVisible();
      await expect(sidebarBoard(page, "Untitled board")).toBeVisible();
      await showBoard(page, "POST /orders");
      await expect(boardView(page).getByRole("article", { name: "POST /orders" })).toBeVisible();
    });

    await test.step("an edited board moves to the top of Home's Boards, and a row opens its board", async () => {
      await boardTitle(page).dblclick();
      const field = sidebar(page).getByRole("textbox", { name: "Board title" });
      await enterText(field, "Placing an order");
      await field.press("Enter");
      await expect(boardTitle(page)).toHaveText("Placing an order");
      await showHome(page);
      await expect(rows.locator(".home-board-title")).toHaveText(["Placing an order", "Untitled board"]);
      await rows.filter({ hasText: "Untitled board" }).click();
      await expect(boardTitle(page)).toHaveText("Untitled board");
    });

    await test.step("a project row shows the project's overview on Home", async () => {
      await showHome(page);
      await projectRow(page, "ts-orders").locator(".home-project-title").click();
      await expect(howItStarts(page)).toBeVisible();
    });
  });

  test("Health: a file that doesn't parse, the Health line, and the Health view", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    writeFile(join(folder, "src", "broken.ts"), "export function broken( {\n  return 1;\n");
    const { page } = await launchWithProject(launch, folder);

    await test.step("the project row and the state line say a file doesn't parse", async () => {
      await expect(rowState(page, "ts-orders")).toHaveAttribute("data-state", "problems", { timeout: 60_000 });
      await expect(stateLine(page)).toContainText("1 file doesn't parse");
    });

    await test.step("the Health line shows it in amber, as a button to the Health view", async () => {
      const health = homeMain(page).getByRole("region", { name: "Health", exact: true });
      await expect(health.locator(".home-health-item")).toHaveText(["1 file doesn't parse"]);
      await health.getByRole("button", { name: "1 file doesn't parse" }).click();
    });

    const view = homeMain(page).locator(".home-health");

    await test.step("Health lists the file that doesn't parse, with Open", async () => {
      await expect(view.getByRole("heading", { name: "Health" })).toBeVisible();
      const row = view.locator(".home-health-row").filter({ hasText: "src/broken.ts" });
      await expect(row).toBeVisible();
      await expect(row.getByRole("button", { name: "Open" })).toBeVisible();
    });

    await test.step("Back to overview returns to How it starts", async () => {
      await view.getByRole("button", { name: "Back to overview" }).click();
      await expect(howItStarts(page)).toBeVisible();
    });
  });

  test("a killed engine host reconnects the window without losing an edit", async ({ sandbox, launch }) => {
    const { app, page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await openPlace(page);
    const { project, id } = await shownBoardRef(page);

    await test.step("an edit is waiting to be saved when the engine host dies", async () => {
      await card(page, PLACE).locator(".card-header").click();
      await enterText(selected(page).getByRole("textbox", { name: "Description" }), "Written as the engine went away.");
      const pid = await app.killEngineHost();
      await expect(toast(page, "Reconnecting to the engine…")).toBeVisible();
      await expect.poll(() => app.engineHostPid()).not.toBe(pid);
    });

    await test.step("the window reconnects; the toast goes and the board stays shown", async () => {
      await expectConnection(page, "connected");
      await expect(toast(page, "Reconnecting to the engine…")).toHaveCount(0, { timeout: 30_000 });
      await expect(boardTitle(page)).toHaveText("POST /orders");
      await expect(card(page, PLACE).locator(".code-view")).toBeVisible();
    });

    await test.step("the edit is saved after the reconnect, and the engine answers again", async () => {
      await expect
        .poll(() => readBoard(folder, id).pages[0]?.cards.find((entry) => entry.ref.block?.endsWith(PLACE))?.description?.text)
        .toBe("Written as the engine went away.");
      await expect.poll(async () => (await invoke<{ boards: { id: string }[] }>(page, "list_boards", { project })).boards.map((board) => board.id)).toContain(id);
      await showHome(page);
      await expectGraphCurrent(page);
    });
  });
});

test.describe("app settings", () => {
  test("the sidebar's gear opens Settings: the editor command is saved, and an agent's command is copied", async ({ sandbox, launch }) => {
    const { app, page } = await launchCompiled(launch, sandbox);
    const settings = dialog(page, "Settings");
    const editor = settings.getByRole("textbox", { name: "Editor command" });

    await test.step("the editor command is saved on Enter", async () => {
      await sidebar(page).getByRole("button", { name: "Settings", exact: true }).click();
      await expect(settings).toBeVisible();
      await expect(editor).toHaveValue("");
      await enterText(editor, "code -g {path}:{line}");
      await editor.press("Enter");
      await expect.poll(async () => (await invoke<AppSettings>(page, "get_settings")).editorCommand).toBe("code -g {path}:{line}");
    });

    await test.step("the Codex command is copied with the app's address", async () => {
      await settings.getByRole("button", { name: "Copy the Codex command", exact: true }).click();
      await expect(toast(page, "Copied the Codex command")).toBeVisible();
      await expect.poll(() => app.clipboardText()).toBe(`codex mcp add unfold --url ${AGENT_ADDRESS}`);
    });

    await test.step("Escape closes Settings, and it opens again with the saved command", async () => {
      await page.keyboard.press("Escape");
      await expect(settings).toBeHidden();
      await sidebar(page).getByRole("button", { name: "Settings", exact: true }).click();
      await expect(editor).toHaveValue("code -g {path}:{line}");
    });

    await test.step("an emptied command is saved as none when Settings closes", async () => {
      await enterText(editor, "");
      await page.keyboard.press("Escape");
      await expect(settings).toBeHidden();
      await expect.poll(async () => (await invoke<AppSettings>(page, "get_settings")).editorCommand).toBeUndefined();
    });
  });
});
