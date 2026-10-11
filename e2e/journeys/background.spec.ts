/**
 * Background updates (docs/engine.md#project-lifecycle, background updates): a saved file updates the graph and the open card; a
 * described card whose code changed asks for review on the card, its row, and its page; a file that
 * breaks shows its last version until it is fixed; a branch switch across 500 files is one update that
 * ends equal to a fresh compile; files edited while the app is closed are picked up on reopen; a
 * read-only folder keeps its graph in the fallback location; deleting the cache recovers; and a file
 * renamed by changing only the case of its name.
 */

import { cpSync, existsSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import { activeBoardRef, boardView, card, expectCardsLoaded, onThisPage, pagesList, readBoard, selected } from "../src/board";
import { expect, test } from "../src/fixtures";
import { launchCompiled, openPlace, PLACE, traceEntrance } from "../src/flows";
import { addProjectFromSidebar, expectGraphCurrent, firstCompile, homeMain, homeTitle, projectId, showHome, stateLine } from "../src/home";
import { editFile, writeFile } from "../src/sandbox";
import { writeSyntheticRepo } from "../src/synthetic";
import { enterText, eventMark, events, invoke, waitForEvent, whenTextShows } from "../src/window";

interface IndexLog {
  readonly entries: readonly { readonly action: string }[];
  readonly stats: Record<string, number> | null;
}

interface SearchResults {
  readonly definitions: { readonly hits: readonly { readonly name: string; readonly path: string }[] };
  readonly files: { readonly hits: readonly { readonly path: string }[] };
}

/** The definitions named exactly `name` in a project's graph, by path. */
async function definitionsNamed(page: Page, project: string, name: string): Promise<string[]> {
  const found = await invoke<SearchResults>(page, "search", { query: name, kinds: ["definitions"], projects: [project] });
  return found.definitions.hits.filter((hit) => hit.name === name).map((hit) => hit.path);
}

/** The indexed files of a project whose path contains `text`. */
async function filesLike(page: Page, project: string, text: string): Promise<string[]> {
  const found = await invoke<SearchResults>(page, "search", { query: text, kinds: ["files"], projects: [project] });
  return found.files.hits.map((hit) => hit.path).filter((path) => path.includes(text));
}

/** The committed graph's counts (its records, not the database's size on disk). */
async function graphCounts(page: Page, project: string): Promise<Record<string, number>> {
  const { stats } = await invoke<IndexLog>(page, "index_log", { project });
  const { databaseBytes: _bytes, ...counts } = stats ?? {};
  return counts;
}

/** The keys of the definitions that call `key`. */
async function callerKeys(page: Page, project: string, key: string): Promise<string[]> {
  const result = await invoke<{ callers: { key: string }[] }>(page, "callers", { project, key, limit: 100 });
  return result.callers.map((caller) => caller.key).sort();
}

const PLACE_CODE = `.board-view .board-card[data-key$="${PLACE}"] .code-view`;

test.describe("background updates", () => {
  test("a saved file updates the graph and the open card; a described card asks for review", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    const file = join(folder, "src/services/order-service.ts");
    await openPlace(page);
    const place = card(page, PLACE);

    await test.step("a save updates the graph and the open card (save → updated card)", async () => {
      const mark = await eventMark(page, "graph_updated");
      const shown = whenTextShows(page, PLACE_CODE, "// saved in the editor");
      const saved = Date.now();
      editFile(file, "this.payments.charge(draft);", "this.payments.charge(draft); // saved in the editor");
      const update = await waitForEvent(page, "graph_updated", mark, (event) => Array.isArray(event["changedFiles"]) && event["changedFiles"].includes("src/services/order-service.ts"));
      const elapsed = (await shown) - saved;
      test.info().annotations.push({ type: "save → updated card", description: `${elapsed} ms (graph_updated at ${update.at - saved} ms)` });
      expect(elapsed).toBeLessThan(1000);
    });

    await test.step("a description written in Selected is saved with the board", async () => {
      await place.locator(".card-header").click();
      await enterText(selected(page).getByRole("textbox", { name: "Description" }), "Charges the order and saves it.");
      await expect(place.locator(".card-description")).toContainText("Charges the order and saves it.");
      const { id } = await activeBoardRef(page);
      await expect
        .poll(() => readBoard(folder, id).pages[0]?.cards.find((entry) => entry.ref.block?.endsWith(PLACE))?.description?.text)
        .toBe("Charges the order and saves it.");
    });

    await test.step("its code changing shows the review banner, the row's mark, and the page's mark", async () => {
      editFile(file, "this.payments.charge(draft); // saved in the editor", "this.payments.charge(draft); // changed after it was described");
      await expect(place.locator(".card-review")).toContainText("Changed since it was described", { timeout: 30_000 });
      await expect(onThisPage(page).locator(".board-review-dot")).toHaveCount(1);
      await expect(pagesList(page).locator(".board-review-dot")).toHaveCount(1);
    });

    await test.step("Review explains the problem and a repair clears it", async () => {
      await place.locator(".card-review").getByRole("button", { name: "Review" }).click();
      const popover = page.getByRole("dialog", { name: "Review" });
      await expect(popover).toBeVisible();
      const repair = popover.locator(".repair-option").filter({ hasText: /Accept the current code|Keep the text/ }).first();
      await repair.getByRole("button", { name: "Apply" }).click();
      await expect(place.locator(".card-review")).toHaveCount(0);
      await expect(onThisPage(page).locator(".board-review-dot")).toHaveCount(0);
      await expect(pagesList(page).locator(".board-review-dot")).toHaveCount(0);
    });
  });

  test("a file that breaks shows its last version, dimmed, until it is fixed; Home's Health counts it", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const file = join(folders[0] ?? "", "src/services/order-service.ts");
    await openPlace(page);
    const place = card(page, PLACE);
    const original = readFileSync(file, "utf8");

    await test.step("the broken file's card shows its last verified code, dimmed", async () => {
      editFile(file, "this.payments.charge(draft);", "this.payments.charge(draft;");
      await expect(place.locator(".card-note")).toHaveText(/Doesn't parse right now · showing the last verified code/, { timeout: 30_000 });
      await expect(place.locator(".code-view")).toHaveClass(/\bdimmed\b/);
      await expect(place).toContainText("this.payments.charge(draft);");
    });

    await test.step("Home's state line and Health say a file doesn't parse", async () => {
      await showHome(page);
      await expect(stateLine(page)).toContainText("1 file doesn't parse");
      await expect(homeMain(page).getByRole("region", { name: "Health" })).toContainText("1 file doesn't parse");
    });

    await test.step("fixing the file brings the card back", async () => {
      writeFileSync(file, original);
      await expectGraphCurrent(page);
      await page.getByRole("tablist", { name: "Window tabs" }).getByRole("tab", { name: "POST /orders" }).click();
      await expect(place.locator(".code-view")).not.toHaveClass(/\bdimmed\b/);
      await expect(place.locator(".card-note")).toHaveCount(0);
    });
  });

  test("a branch switch across 500 files is one update that ends equal to a fresh compile", async ({ sandbox, launch }) => {
    test.setTimeout(300_000);
    const count = 520;
    const folder = sandbox.folder("synthetic");
    writeSyntheticRepo(folder, count, "main");
    const app = await launch({ args: [folder] });
    const page = await app.firstWindow();
    await expect(homeTitle(page)).toHaveText("synthetic");
    await firstCompile(page);
    await expect(stateLine(page)).toContainText(/Graph current\s*· 520 files · TypeScript/);
    const project = await projectId(page, "synthetic");
    const before = await invoke<IndexLog>(page, "index_log", { project });
    const mark = await eventMark(page, "graph_updated");

    await test.step("every file changes at once, and one background update follows", async () => {
      writeSyntheticRepo(folder, count, "feature");
      await waitForEvent(page, "graph_updated", mark, (event) => event["project"] === project);
      await expectGraphCurrent(page);
      await expect.poll(async () => (await invoke<IndexLog>(page, "index_log", { project })).entries.length).toBeGreaterThan(before.entries.length);
      const after = (await invoke<IndexLog>(page, "index_log", { project })).entries.slice(before.entries.length);
      expect(after.map((entry) => entry.action), "the branch switch is one background update").toEqual(["Background update"]);
      expect((await events(page, "graph_updated")).slice(mark).filter((entry) => entry.event["project"] === project)).toHaveLength(1);
      const twice = await invoke<{ definitions: { total: number } }>(page, "search", { query: "twice", kinds: ["definitions"], projects: [project] });
      expect(twice.definitions.total, "every file holds the feature branch's method").toBe(count);
    });

    await test.step("the updated graph equals a fresh compile of the same files", async () => {
      const copy = join(sandbox.root, "synthetic-copy");
      cpSync(folder, copy, { recursive: true, filter: (source) => !source.startsWith(join(folder, ".unfold")) });
      await addProjectFromSidebar(app, page, copy);
      await expect(homeTitle(page)).toHaveText("synthetic-copy");
      await firstCompile(page);
      const copyProject = await projectId(page, "synthetic-copy");
      const fresh = await graphCounts(page, copyProject);
      expect(fresh["definitions"]).toBeGreaterThan(0);
      expect(await graphCounts(page, project), "the updated graph's counts equal the fresh compile's").toEqual(fresh);
      // And its calls: who calls a sample of functions, after the switch rewired every import.
      for (const index of [0, 7, 99, 260, 519]) {
        const key = `src/m${String(Math.floor(index / 25)).padStart(3, "0")}/f${String(index).padStart(4, "0")}.ts#function:f${index}`;
        const callers = await callerKeys(page, copyProject, key);
        expect(callers).not.toEqual([]);
        expect(await callerKeys(page, project, key), `callers of f${index}`).toEqual(callers);
      }
    });
  });

  test("files edited while the app is closed are picked up on reopen", async ({ sandbox, launch }) => {
    const first = await launchCompiled(launch, sandbox);
    const folder = first.folders[0] ?? "";
    let page = first.page;
    const project = await projectId(page, "ts-orders");
    expect(await definitionsNamed(page, project, "addedWhileClosed")).toEqual([]);
    await first.app.quit();

    writeFileSync(join(folder, "src/lib/retry.ts"), `${readFileSync(join(folder, "src/lib/retry.ts"), "utf8")}\nexport function addedWhileClosed(): number {\n  return 1;\n}\n`);
    writeFile(join(folder, "src/lib/created-while-closed.ts"), "export function createdWhileClosed(): string {\n  return \"new\";\n}\n");
    rmSync(join(folder, "src/lib/calendar.ts"));

    const app = await launch();
    page = await app.firstWindow();

    await test.step("the project comes back on Home, and its graph catches up with the folder", async () => {
      await expect(homeTitle(page)).toHaveText("ts-orders");
      await expectGraphCurrent(page);
      await expect.poll(() => definitionsNamed(page, project, "addedWhileClosed")).toEqual(["src/lib/retry.ts"]);
      await expect.poll(() => definitionsNamed(page, project, "createdWhileClosed")).toEqual(["src/lib/created-while-closed.ts"]);
      await expect.poll(() => filesLike(page, project, "calendar")).toEqual([]);
    });
  });

  test("a read-only folder keeps its graph in the fallback location and everything works", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    sandbox.setReadOnly(folder, true);
    const app = await launch({ args: [folder] });
    const page = await app.firstWindow();
    await expect(homeTitle(page)).toHaveText("ts-orders");

    await test.step("the Compile works and Home's state line says it uses the fallback location", async () => {
      await firstCompile(page);
      await expect(stateLine(page)).toContainText("using fallback location");
    });

    await test.step("tracing and following make a draft, and nothing is written into the folder", async () => {
      await openPlace(page);
      const project = await projectId(page, "ts-orders");
      await expect.poll(async () => (await invoke<{ boards: { place: string }[] }>(page, "list_boards", { project })).boards.filter((board) => board.place === "draft").length).toBe(1);
      expect(existsSync(join(folder, ".unfold/local")), "nothing is written into the folder").toBe(false);
      expect(await definitionsNamed(page, project, "validateOrder")).not.toEqual([]);
    });
  });

  test("deleting the cache while the project is open recovers", async ({ sandbox, launch }) => {
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    await openPlace(page);
    const cache = join(folder, ".unfold/local/cache");
    expect(existsSync(join(cache, "graph.sqlite"))).toBe(true);

    await test.step("the next change recreates the graph and the card updates", async () => {
      rmSync(cache, { recursive: true, force: true });
      editFile(join(folder, "src/services/order-service.ts"), "this.payments.charge(draft);", "this.payments.charge(draft); // after the cache went");
      await expect(card(page, PLACE)).toContainText("// after the cache went", { timeout: 30_000 });
      await expect.poll(() => existsSync(join(cache, "graph.sqlite"))).toBe(true);
    });

    await test.step("the graph answers again: another entrance traces", async () => {
      await showHome(page);
      await expectGraphCurrent(page);
      await traceEntrance(page, "GET", "/health");
      await expect(boardView(page).locator(".entrance-card", { hasText: "/health" })).toBeVisible();
      await expectCardsLoaded(page);
    });
  });

  test("renaming a file by changing only the case of its name", async ({ sandbox, launch }) => {
    // On case-insensitive file systems the old name still finds the file; the engine's scan compares
    // names as spelled on disk, so only the new path stays in the graph.
    const { page, folders } = await launchCompiled(launch, sandbox);
    const folder = folders[0] ?? "";
    const project = await projectId(page, "ts-orders");
    const before = await definitionsNamed(page, project, "setLogSink");
    expect(before).toEqual(["src/lib/logger.ts"]);
    const mark = await eventMark(page, "graph_updated");

    await test.step("the update moves the file's definitions to the new name", async () => {
      renameSync(join(folder, "src/lib/logger.ts"), join(folder, "src/lib/Logger.ts"));
      const update = await waitForEvent(page, "graph_updated", mark, (event) => Array.isArray(event["changedFiles"]) && event["changedFiles"].includes("src/lib/Logger.ts"));
      expect(update.event["removedFiles"]).toContain("src/lib/logger.ts");
      await expect.poll(() => definitionsNamed(page, project, "setLogSink")).toEqual(["src/lib/Logger.ts"]);
      await expect.poll(() => filesLike(page, project, "ogger.ts")).toEqual(["src/lib/Logger.ts"]);
      await expectGraphCurrent(page);
    });
  });
});
