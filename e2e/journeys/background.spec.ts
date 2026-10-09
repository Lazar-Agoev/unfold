/**
 * Migration plan Step 9, the background-compile journeys: break a file and fix it; switch branches
 * across 500 files, ending equal to a fresh compile; edit files while the app is closed; use a
 * read-only folder; delete the cache; rename a file by changing only the case of its name.
 */

import { cpSync, existsSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Page } from "@playwright/test";
import { expect, test } from "../src/fixtures";
import { openAndCompile, openPlace, PLACE, reopened } from "../src/flows";
import { editFile, writeFile } from "../src/sandbox";
import { writeSyntheticRepo } from "../src/synthetic";
import { xtask } from "../src/xtask";
import {
  card,
  eventMark,
  events,
  expectGraphCurrent,
  graphStatus,
  invoke,
  openEntrance,
  showView,
  waitForEvent,
} from "../src/window";

interface OutlineEntry {
  readonly name: string;
  readonly children: readonly OutlineEntry[];
}

interface IndexLog {
  readonly entries: readonly { readonly action: string; readonly files: number }[];
}

/** Every definition name in an indexed file's outline. */
async function outlineNames(page: Page, path: string): Promise<string[]> {
  const outline = await invoke<{ items: OutlineEntry[] }>(page, "file_outline", { path });
  const names: string[] = [];
  const walk = (items: readonly OutlineEntry[]) => {
    for (const item of items) {
      names.push(item.name);
      walk(item.children);
    }
  };
  walk(outline.items);
  return names;
}

test.describe("background compile", () => {
  test("a file that breaks shows its last version, dimmed, until it is fixed", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    const app = await launch();
    const page = await app.firstWindow();
    await openAndCompile(app, page, folder);
    await openPlace(page);

    const file = join(folder, "src/services/order-service.ts");
    const original = readFileSync(file, "utf8");
    editFile(file, "this.payments.charge(draft);", "this.payments.charge(draft;");
    await expect(graphStatus(page)).toHaveText("1 file doesn't parse · showing its last version that parsed");
    const place = card(page, PLACE);
    await expect(place.locator(".card-note")).toHaveText(/Doesn't parse right now · showing the last verified code/);
    await expect(place.locator(".code-view")).toHaveClass(/\bdimmed\b/);
    await expect(place).toContainText("this.payments.charge(draft);");
    await expect(page.locator(".statusbar-problems")).toBeVisible();

    writeFileSync(file, original);
    await expectGraphCurrent(page);
    await expect(place.locator(".code-view")).not.toHaveClass(/\bdimmed\b/);
    await expect(place.locator(".card-note")).toHaveCount(0);
  });

  test("a branch switch across 500 files is one update that ends equal to a fresh compile", async ({ sandbox, launch }) => {
    test.setTimeout(300_000);
    const count = 520;
    const folder = sandbox.folder("synthetic");
    writeSyntheticRepo(folder, count, "main");
    const app = await launch();
    const page = await app.firstWindow();
    await openAndCompile(app, page, folder);
    await expect(graphStatus(page)).toHaveText(`Graph current · ${count} files · TypeScript`);
    const logBefore = await invoke<IndexLog>(page, "index_log");
    const mark = await eventMark(page, "graph_updated");

    writeSyntheticRepo(folder, count, "feature");
    await waitForEvent(page, "graph_updated", mark, (event) => event["everything"] === true || (Array.isArray(event["changedFiles"]) && event["changedFiles"].length > 0));
    await expectGraphCurrent(page);
    await expect.poll(async () => (await invoke<IndexLog>(page, "index_log")).entries.length).toBeGreaterThan(logBefore.entries.length);
    const after = (await invoke<IndexLog>(page, "index_log")).entries.slice(logBefore.entries.length);
    expect(after.map((entry) => entry.action), "the branch switch is one background update").toEqual(["Background update"]);
    expect(await events(page, "graph_updated").then((list) => list.length - mark)).toBe(1);

    // The committed graph equals a fresh compile of an identical copy of the same bytes.
    const copy = join(sandbox.root, "synthetic-copy");
    cpSync(folder, copy, { recursive: true, filter: (source) => !source.startsWith(join(folder, ".unfold")) });
    const fromApp = join(sandbox.root, "app.jsonl");
    const fresh = join(sandbox.root, "fresh.jsonl");
    xtask(["export-store", folder, "-o", fromApp]);
    xtask(["export", copy, "-o", fresh]);
    const appExport = readFileSync(fromApp, "utf8");
    expect(appExport.length).toBeGreaterThan(0);
    expect(appExport.includes("twice"), "the export holds the feature branch").toBe(true);
    expect(appExport === readFileSync(fresh, "utf8"), "the app's graph equals a fresh compile").toBe(true);
  });

  test("files edited while the app is closed are picked up on reopen", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    let app = await launch();
    let page = await app.firstWindow();
    await openAndCompile(app, page, folder);
    expect(await outlineNames(page, "src/lib/retry.ts")).not.toContain("addedWhileClosed");
    await app.quit();

    writeFileSync(join(folder, "src/lib/retry.ts"), `${readFileSync(join(folder, "src/lib/retry.ts"), "utf8")}\nexport function addedWhileClosed(): number {\n  return 1;\n}\n`);
    writeFile(join(folder, "src/lib/created-while-closed.ts"), "export function createdWhileClosed(): string {\n  return \"new\";\n}\n");
    rmSync(join(folder, "src/lib/calendar.ts"));

    app = await launch();
    page = await app.firstWindow();
    await reopened(app, page, folder);
    await expect.poll(() => outlineNames(page, "src/lib/retry.ts")).toContain("addedWhileClosed");
    await expect.poll(() => outlineNames(page, "src/lib/created-while-closed.ts").catch(() => [])).toContain("createdWhileClosed");
    // The deleted file left the graph: its outline has no language or state any more.
    await expect.poll(async () => (await invoke<{ state: string | null }>(page, "file_outline", { path: "src/lib/calendar.ts" }).catch(() => ({ state: null }))).state).toBeNull();
  });

  test("a read-only folder keeps its graph in the fallback location and everything works", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    sandbox.setReadOnly(folder, true);
    const app = await launch();
    const page = await app.firstWindow();
    await openAndCompile(app, page, folder);
    await expect(page.locator(".statusbar-note", { hasText: "using fallback location" })).toBeVisible();
    await openPlace(page);
    await expect.poll(async () => (await invoke<{ drafts: unknown[] }>(page, "list_boards")).drafts.length).toBe(1);
    expect(existsSync(join(folder, ".unfold/local")), "nothing is written into the folder").toBe(false);
    await showView(page, "Search");
    await page.locator(".sidebar").getByRole("searchbox").first().fill("validateOrder");
    await expect(page.locator(".sidebar").getByRole("option", { name: /validateOrder/ }).first()).toBeVisible();
  });

  test("deleting the cache while the folder is open recovers", async ({ sandbox, launch }) => {
    const folder = sandbox.playground("ts-orders");
    const app = await launch();
    const page = await app.firstWindow();
    await openAndCompile(app, page, folder);
    await openPlace(page);
    const cache = join(folder, ".unfold/local/cache");
    expect(existsSync(join(cache, "graph.sqlite"))).toBe(true);
    rmSync(cache, { recursive: true, force: true });

    // The next change finds the database gone, recreates it, and compiles everything again.
    editFile(join(folder, "src/services/order-service.ts"), "this.payments.charge(draft);", "this.payments.charge(draft); // after the cache went");
    await expect(card(page, PLACE)).toContainText("// after the cache went", { timeout: 30_000 });
    await expectGraphCurrent(page);
    expect(existsSync(join(cache, "graph.sqlite"))).toBe(true);
    await openEntrance(page, "GET", "/health");
    await expect(page.locator(".editor-board:not([hidden]) .entrance-card", { hasText: "/health" }).first()).toBeVisible();
  });

  test("renaming a file by changing only the case of its name", async ({ sandbox, launch }) => {
    // On case-insensitive file systems the old name still finds the file; the engine's scan
    // compares names as spelled on disk, so only the new path stays in the graph.
    const folder = sandbox.playground("ts-orders");
    const app = await launch();
    const page = await app.firstWindow();
    await openAndCompile(app, page, folder);
    const before = await outlineNames(page, "src/lib/logger.ts");
    expect(before.length).toBeGreaterThan(0);
    const mark = await eventMark(page, "graph_updated");

    renameSync(join(folder, "src/lib/logger.ts"), join(folder, "src/lib/Logger.ts"));
    const update = await waitForEvent(page, "graph_updated", mark, (event) => Array.isArray(event["changedFiles"]) && event["changedFiles"].includes("src/lib/Logger.ts"));
    expect(update.event["removedFiles"]).toContain("src/lib/logger.ts");
    await expect.poll(() => outlineNames(page, "src/lib/Logger.ts")).toEqual(before);
    const listing = await invoke<{ entries: { name: string }[] }>(page, "list_dir", { path: "src/lib" });
    expect(listing.entries.map((entry) => entry.name)).toContain("Logger.ts");
    expect(listing.entries.map((entry) => entry.name)).not.toContain("logger.ts");
    await expect(graphStatus(page)).not.toHaveText(/Updating/);
  });
});
