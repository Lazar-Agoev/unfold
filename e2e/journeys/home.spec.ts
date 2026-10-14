/**
 * Compile and Home (docs/ui-shell.md#home), end to end on a copy
 * of `ts-orders` added to an empty workspace: it compiles at once; Home shows the state line, How it
 * starts as `unfold.toml` writes it, Boards, and Health; the deliberately missing key shows its dot,
 * and Fix rewrites its one line; the project menu opens `unfold.toml` in the editor; Settings opens
 * from the gear; and a killed engine host reconnects the window with the same state, an edit not lost.
 */

import { chmodSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { addInput, openAdd, result, selectResult } from "../src/add";
import { openSettings } from "../src/agents";
import { boardTitle, boardView, card, expectCardsLoaded, selected, shownBoardRef } from "../src/board";
import { expect, test } from "../src/fixtures";
import { launchCompiled } from "../src/flows";
import {
  expectGraphCurrent,
  healthLine,
  homeBoardRows,
  homeMain,
  howItStarts,
  projectMenu,
  rowState,
  showHome,
  startRow,
  stateLine,
  toast,
} from "../src/home";
import { readFile, writeFile } from "../src/sandbox";
import { enterText, expectConnection, invoke } from "../src/window";

const MISSING = "src/server.ts#adminStats";
const MOVED = "src/routes/admin.ts#adminStats";

/** How it starts as `unfold.toml` writes it: each app (`name`, `run`), then its entrances (`name`, `on`). */
function writtenStarts(toml: string): { readonly kind: "app" | "entrance"; readonly name: string; readonly command: string }[] {
  const rows: { kind: "app" | "entrance"; name: string; command: string }[] = [];
  for (const line of toml.split("\n")) {
    const app = /^name = "([^"]+)"$/.exec(line);
    const run = /^run = "([^"]+)"$/.exec(line);
    const entrance = /^\s+\{ name = "([^"]+)", on = "([^"]+)"/.exec(line);
    if (app?.[1]) {
      rows.push({ kind: "app", name: app[1], command: "" });
    } else if (run?.[1] && rows.length > 0) {
      const last = rows[rows.length - 1];
      if (last) {
        last.command = run[1];
      }
    } else if (entrance?.[1] && entrance[2]) {
      rows.push({ kind: "entrance", name: entrance[1], command: entrance[2] });
    }
  }
  return rows;
}

function readFileOrNull(path: string): string | null {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return null;
  }
}

test("a playground compiles; Home shows how it starts, fixes a key, and survives the engine host", async ({ launch, sandbox }) => {
  const { app, page, folders } = await launchCompiled(launch, sandbox);
  const folder = folders[0] ?? "";
  const config = join(folder, "unfold.toml");
  const written = readFile(config);
  const admin = startRow(page, "Admin stats");
  const notFound = (count: number) => healthLine(page).locator(".home-health-item", { hasText: `${count} entrances' code isn't found` });

  await test.step("an empty workspace adds a copy of ts-orders, which compiles at once", async () => {
    await expect(rowState(page, "ts-orders")).toHaveAttribute("data-state", "current");
    await expect(stateLine(page)).toContainText(/Graph current\s*· 34 files · TypeScript · updated just now/);
  });

  await test.step("Home shows the state line, How it starts as written, Boards, and Health", async () => {
    const rows = howItStarts(page).locator(".home-start-row");
    const shown = await rows.evaluateAll((all) =>
      all.map((row) => ({
        kind: row.classList.contains("app-row") ? "app" : "entrance",
        name: row.querySelector(".home-start-name")?.textContent ?? "",
        command: row.querySelector(".home-start-command")?.textContent ?? "",
      })),
    );
    expect(shown).toEqual(writtenStarts(written));
    // Apps are bold, their entrances indented under them, and what starts each is on the right in mono.
    const server = startRow(page, "Server");
    const place = startRow(page, "Place an order");
    await expect(server.locator(".home-start-name")).toHaveCSS("font-weight", "600");
    await expect(place.locator(".home-start-name")).toHaveCSS("font-weight", "400");
    const serverName = await server.locator(".home-start-name").boundingBox();
    const placeName = await place.locator(".home-start-name").boundingBox();
    expect(placeName?.x ?? 0).toBeGreaterThan(serverName?.x ?? 0);
    const command = place.locator(".home-start-command");
    await expect(command.locator("b")).toHaveText("POST");
    expect((await command.evaluate((element) => getComputedStyle(element).fontFamily)).toLowerCase()).toMatch(/mono/);
    // An app's `run` and an entrance's `on` end in the same right column.
    const right = async (row: typeof place) => {
      const box = await row.locator(".home-start-command").boundingBox();
      return (box?.x ?? 0) + (box?.width ?? 0);
    };
    expect(Math.abs((await right(server)) - (await right(place)))).toBeLessThan(1);
    expect((await command.boundingBox())?.x ?? 0).toBeGreaterThan((placeName?.x ?? 0) + (placeName?.width ?? 0));

    const boards = homeMain(page).getByRole("region", { name: "Boards" });
    await expect(boards.getByText("No boards yet")).toBeVisible();
    await expect(healthLine(page).locator(".home-health-item")).toHaveText(["All files parse", "3 entrances' code isn't found"]);
  });

  await test.step("a missing key shows its dot; Fix rewrites its one line, and the dot goes", async () => {
    await expect(admin.getByRole("img", { name: "Needs a look" })).toBeVisible();
    await expect(startRow(page, "Place an order").getByRole("img", { name: "Needs a look" })).toHaveCount(0);
    await admin.hover();
    await expect(admin.locator(".home-start-why")).toHaveText("Moved to src/routes/admin.ts");
    await admin.getByRole("button", { name: "Fix", exact: true }).click();
    await expect(admin.getByRole("img", { name: "Needs a look" })).toHaveCount(0);
    await expect(notFound(2)).toBeVisible();

    const before = written.split("\n");
    const after = readFile(config).split("\n");
    expect(after).toHaveLength(before.length);
    const changed = after.flatMap((line, index) => (line === before[index] ? [] : [index]));
    expect(changed).toHaveLength(1);
    const [at = -1] = changed;
    expect(before[at]).toContain(`code = "${MISSING}"`);
    expect(after[at]).toBe(before[at]?.replace(MISSING, MOVED));
  });

  const opened = join(sandbox.root, "opened.txt");
  const stub = join(sandbox.root, "editor.sh");
  const editorCommand = `${stub} {path} {line}`;

  await test.step("the project menu's Open unfold.toml runs the editor command", async () => {
    writeFile(stub, `#!/bin/sh\nprintf '%s\\n' "$@" > '${opened}'\n`);
    chmodSync(stub, 0o755);
    const settings = await openSettings(page);
    await enterText(settings.getByRole("textbox", { name: "Editor command" }), editorCommand);
    await page.keyboard.press("Enter");
    await expect.poll(async () => (await invoke<{ readonly editorCommand?: string }>(page, "get_settings")).editorCommand).toBe(editorCommand);
    await page.keyboard.press("Escape");
    await expect(settings).toBeHidden();

    await projectMenu(page, "ts-orders", "Open unfold.toml");
    await expect.poll(() => readFileOrNull(opened)).toBe(`${config}\n1\n`);
  });

  await test.step("Settings opens from the gear, with the saved editor command", async () => {
    const settings = await openSettings(page);
    await expect(settings.getByRole("textbox", { name: "Editor command" })).toHaveValue(editorCommand);
    await page.keyboard.press("Escape");
    await expect(settings).toBeHidden();
  });

  await test.step("a killed engine host reconnects the window with the same state, an edit not lost", async () => {
    await homeMain(page).locator(".home-header").getByRole("button", { name: "New board" }).click();
    await expect(boardView(page)).toBeVisible();
    await openAdd(page);
    await enterText(addInput(page), "createOrder");
    await selectResult(page, result(page, /^function createOrder, /));
    await addInput(page).press("Enter");
    const createOrder = card(page, "createOrder");
    await expect(createOrder).toBeVisible();
    await expectCardsLoaded(page);
    const { id } = await shownBoardRef(page);
    const title = await boardTitle(page).innerText();

    // An edit is waiting to be saved when the engine host dies.
    await createOrder.locator(".card-header .card-title").click();
    const description = "Written as the engine went away.";
    await enterText(selected(page).getByRole("textbox", { name: "Description" }), description);
    const pid = await app.killEngineHost();
    await expect(toast(page, "Reconnecting to the engine…")).toBeVisible();
    await expect.poll(() => app.engineHostPid()).not.toBe(pid);

    await expectConnection(page, "connected");
    await expect(toast(page, "Reconnecting to the engine…")).toHaveCount(0);
    await expect(boardTitle(page)).toHaveText(title);
    await expect(createOrder).toBeVisible();
    await expectCardsLoaded(page);
    await expect
      .poll(() => {
        const board = JSON.parse(readFile(join(folder, id))) as { readonly pages: readonly { readonly cards: readonly { readonly description?: { readonly text?: string } }[] }[] };
        return board.pages[0]?.cards.map((entry) => entry.description?.text);
      })
      .toEqual([description]);

    await showHome(page);
    await expectGraphCurrent(page);
    await expect(startRow(page, "Admin stats")).toBeVisible();
    await expect(howItStarts(page).getByRole("img", { name: "Needs a look" })).toHaveCount(2);
    await expect(homeBoardRows(page).locator(".home-board-title")).toHaveText([title]);
    await expect(notFound(2)).toBeVisible();
  });

  expect(app.logs.filter((line) => line.startsWith("[pageerror]"))).toEqual([]);
});
