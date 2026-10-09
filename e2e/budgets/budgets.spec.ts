/**
 * Architecture §17's budgets, measured on the built app on this machine (run with the release
 * addon: `just e2e-budgets`). Each test measures one row and records it in
 * `test-results/budgets.json`; docs/testing.md keeps the table.
 *
 * The 602-file corpus is TypeScript 6.0.3's `src` (testdata/corpora.lock.toml), read from
 * `UNFOLD_TS_CORPUS` or the proof of concept's vendored copy, and copied into the sandbox: the
 * source is never written. Tests that need it are skipped when it isn't there.
 */

import { cpSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import type { Page } from "@playwright/test";
import { UnfoldApp } from "../src/app";
import { expect, test } from "../src/fixtures";
import { openAndCompile } from "../src/flows";
import type { Sandbox } from "../src/sandbox";
import { writeFile } from "../src/sandbox";
import { syntheticFile, syntheticPath, writeSyntheticRepo } from "../src/synthetic";
import {
  activeBoard,
  card,
  cards,
  eventMark,
  expectConnection,
  expectGraphCurrent,
  firstCompile,
  graphStatus,
  openFromWelcome,
  showView,
  waitForEvent,
  whenTextShows,
  windowCommand,
} from "../src/window";

const CORPUS = process.env["UNFOLD_TS_CORPUS"] ?? "/Users/lazaragoev/Desktop/repos/graph-compiler/compilers/typescript/src";
const RESULTS = join(dirname(new URL(import.meta.url).pathname), "..", "test-results", "budgets.json");

interface Row {
  readonly target: string;
  readonly measured: string;
  readonly detail?: unknown;
}

/** Rows measured so far: a partial run (`-g`) updates its rows and keeps the others. */
const rows: Record<string, Row> = existsSync(RESULTS) ? (JSON.parse(readFileSync(RESULTS, "utf8")) as { rows: Record<string, Row> }).rows : {};

function record(name: string, row: Row): void {
  rows[name] = row;
  test.info().annotations.push({ type: name, description: `${row.measured} (target ${row.target})` });
  mkdirSync(dirname(RESULTS), { recursive: true });
  writeFileSync(RESULTS, JSON.stringify({ when: new Date().toISOString(), rows }, null, 2));
}

function quantile(values: readonly number[], q: number): number {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? Number.NaN;
}

function ms(value: number): string {
  return `${Math.round(value)} ms`;
}

function mb(kb: number): string {
  return `${Math.round(kb / 1024)} MB`;
}

async function totalMemoryKb(app: UnfoldApp): Promise<number> {
  return (await app.processMemory()).reduce((sum, process) => sum + process.kb, 0);
}

/** Memory once it stops moving: two samples a second apart within 2%. */
async function settledMemoryKb(app: UnfoldApp): Promise<number> {
  let previous = await totalMemoryKb(app);
  for (let round = 0; round < 30; round++) {
    await new Promise((resolve) => setTimeout(resolve, 1000));
    const now = await totalMemoryKb(app);
    if (Math.abs(now - previous) / previous < 0.02) {
      return now;
    }
    previous = now;
  }
  return previous;
}

/** Opens a welcome window and closes every other one, so the next launch starts on welcome. */
async function leaveOnlyWelcome(app: UnfoldApp, page: Page): Promise<void> {
  const welcome = await app.nextWindow(() => windowCommand(page, "new_window", { folder: "none" }));
  await expect(welcome.locator(".welcome")).toBeVisible();
  await expect
    .poll(
      async () => {
        for (const win of app.windows()) {
          if (win !== welcome) {
            await windowCommand(win, "close_window").catch(() => undefined);
          }
        }
        return app.windows().length;
      },
      { timeout: 30_000 },
    )
    .toBe(1);
}

function copyCorpus(sandbox: Sandbox): string {
  const folder = join(sandbox.root, "typescript-src");
  cpSync(CORPUS, folder, { recursive: true });
  return folder;
}

/** Timed engine round trips from the renderer, in ms. */
async function timedInvokes(page: Page, command: string, argsList: readonly Record<string, unknown>[]): Promise<number[]> {
  return page.evaluate(
    async ([name, list]) => {
      const out: number[] = [];
      for (const args of list) {
        const start = performance.now();
        await window.unfold.invoke(name, args);
        out.push(performance.now() - start);
      }
      return out;
    },
    [command, argsList] as const,
  );
}

test.describe.configure({ mode: "serial" });

test("launch to the welcome window", async ({ sandbox }) => {
  const times: number[] = [];
  for (let run = 0; run < 3; run++) {
    const start = Date.now();
    const app = await UnfoldApp.launch(sandbox);
    const page = await app.electron.firstWindow();
    await expect(page.locator(".welcome-wordmark")).toBeVisible();
    times.push(Date.now() - start);
    await app.quit();
  }
  record("Launch to the welcome window", { target: "< 1.5 s", measured: ms(quantile(times, 0.5)), detail: times });
});

test.describe("on TypeScript's 602-file compiler source", () => {
  test.skip(!existsSync(CORPUS), `the corpus isn't at ${CORPUS} (set UNFOLD_TS_CORPUS)`);

  test("first Compile, opening it compiled, search, cards, cancel, memory, windows, restart", async ({ sandbox, launch }) => {
    const folder = copyCorpus(sandbox);
    let app = await launch();
    let page = await app.firstWindow();
    await openFromWelcome(app, page, folder);

    // First Compile: press Compile, until the graph is current; peak memory of every process.
    let peakKb = 0;
    const sampling = { on: true };
    const sampler = (async () => {
      while (sampling.on) {
        peakKb = Math.max(peakKb, await totalMemoryKb(app).catch(() => 0));
        await new Promise((resolve) => setTimeout(resolve, 100));
      }
    })();
    const compileStart = Date.now();
    await firstCompile(page);
    const compileMs = Date.now() - compileStart;
    sampling.on = false;
    await sampler;
    const status = (await graphStatus(page).textContent()) ?? "";
    record("First Compile, 602-file TypeScript source", { target: "< 5 s, < 1 GB peak", measured: `${ms(compileMs)}, ${mb(peakKb)} peak (all processes)`, detail: status });

    // Search: 40 queries, p95 of the round trip from the window.
    const queries = ["checker", "createNode", "emit", "parse", "Symbol", "getTypeOf", "visit", "scanner", "binder", "resolve", "factory", "Diagnostic", "program", "host", "transform", "isIdentifier", "flags", "node", "sourceFile", "compile"];
    const searchArgs = [...queries, ...queries].map((query) => ({ query, limit: 50 }));
    const searchTimes = await timedInvokes(page, "search", searchArgs);
    record("Search, 95th percentile", { target: "< 100 ms", measured: ms(quantile(searchTimes, 0.95)), detail: { p50: quantile(searchTimes, 0.5), max: Math.max(...searchTimes) } });

    // Open a cached card: the `card` command for 50 definitions, the second time (cached).
    const keys = await page.evaluate(async (list) => {
      const found = new Set<string>();
      for (const query of list) {
        const result = (await window.unfold.invoke("search", { query, kinds: ["definitions"], limit: 10 })) as { definitions: { key: string }[] };
        for (const hit of result.definitions) {
          found.add(hit.key);
        }
      }
      return [...found].slice(0, 50);
    }, queries);
    const cold = await timedInvokes(page, "card", keys.map((key) => ({ key })));
    const warm = await timedInvokes(page, "card", keys.map((key) => ({ key })));
    record("Open a cached card, 95th percentile (engine round trip)", { target: "< 150 ms", measured: ms(quantile(warm, 0.95)), detail: { cards: keys.length, coldP95: quantile(cold, 0.95), warmP50: quantile(warm, 0.5) } });

    // Cancel: start Rebuild from scratch, cancel it, until the engine acknowledges.
    const cancels: number[] = [];
    for (let run = 0; run < 5; run++) {
      cancels.push(
        await page.evaluate(async (after) => {
          const handle = window.unfold.stream("compile", { mode: "from_scratch" }, () => undefined);
          await new Promise((resolve) => setTimeout(resolve, after));
          const start = performance.now();
          handle.cancel();
          await handle.result.catch(() => undefined);
          return performance.now() - start;
        }, 200 + run * 300),
      );
      await expectGraphCurrent(page, 120_000);
    }
    record("Cancel acknowledged", { target: "< 250 ms", measured: `${ms(quantile(cancels, 0.5))} median, ${ms(Math.max(...cancels))} worst`, detail: cancels });

    // Idle memory with one window showing the workbench.
    const oneWindowKb = await settledMemoryKb(app);
    record("Idle memory, one window, 602-file folder open", { target: "< 600 MB (2,000-file repository)", measured: mb(oneWindowKb), detail: await app.processMemory() });

    // Each further window on the same folder.
    const second = await app.nextWindow(() => windowCommand(page, "new_window", { folder: "same" }));
    await expectGraphCurrent(second);
    const twoWindowsKb = await settledMemoryKb(app);
    const third = await app.nextWindow(() => windowCommand(page, "new_window", { folder: "same" }));
    await expectGraphCurrent(third);
    const threeWindowsKb = await settledMemoryKb(app);
    record("Each further window on the same folder", { target: "< 150 MB more", measured: `${mb(twoWindowsKb - oneWindowKb)}, ${mb(threeWindowsKb - twoWindowsKb)}`, detail: { oneWindowKb, twoWindowsKb, threeWindowsKb } });

    // Engine-host restart until every window is usable again (three windows), three times.
    const restarts: number[] = [];
    for (let run = 0; run < 3; run++) {
      await app.killEngineHost();
      const start = Date.now();
      for (const win of app.windows()) {
        await expect.poll(() => win.evaluate(() => window.unfold.invoke("graph_status", {}).then(() => true, () => false)), { timeout: 30_000, intervals: [20] }).toBe(true);
      }
      restarts.push(Date.now() - start);
      for (const win of app.windows()) {
        await expectConnection(win, "connected");
      }
    }
    record("Engine-host restart until every window is usable (3 windows)", { target: "< 2 s", measured: `${ms(quantile(restarts, 0.5))} median, ${ms(Math.max(...restarts))} worst`, detail: restarts });

    // Open the compiled folder: quit, relaunch to welcome, open it from Recent, until the
    // workbench shows and the graph answers a search.
    await app.quit();
    const opens: number[] = [];
    for (let run = 0; run < 2; run++) {
      app = await launch();
      page = await app.firstWindow();
      await expect(page.locator(".workbench, .welcome").first()).toBeVisible();
      if (await page.locator(".workbench").isVisible()) {
        // The folder's windows came back by themselves; close all but welcome next time.
        await leaveOnlyWelcome(app, page);
        await app.quit();
        app = await launch();
        page = await app.firstWindow();
      }
      await expect(page.locator(".welcome-recent-open").first()).toBeVisible();
      await expectConnection(page, "connected");
      const start = Date.now();
      await page.locator(".welcome-recent-open").first().click();
      await expect(page.locator(".workbench")).toBeVisible();
      // Usable: the graph answers (this corpus has no entrances; a definition search reads it).
      await expect
        .poll(
          () =>
            page.evaluate(() =>
              window.unfold.invoke("search", { query: "createSourceFile", kinds: ["definitions"], limit: 5 }).then(
                (result) => (result as { definitions: unknown[] }).definitions.length,
                () => 0,
              ),
            ),
          { timeout: 30_000, intervals: [20] },
        )
        .toBeGreaterThan(0);
      opens.push(Date.now() - start);
      await expectGraphCurrent(page, 120_000);
      await leaveOnlyWelcome(app, page);
      await app.quit();
    }
    record("Open a compiled folder to a usable graph", { target: "< 1 s", measured: opens.map(ms).join(", "), detail: opens });
  });

  test("canvas with 50 cards", async ({ sandbox, launch }) => {
    const folder = copyCorpus(sandbox);
    const app = await launch();
    const page = await app.firstWindow();
    await openAndCompile(app, page, folder);
    const keys = await page.evaluate(async () => {
      const found: string[] = [];
      for (const query of ["create", "get", "is", "visit", "emit", "parse", "check", "resolve"]) {
        const result = (await window.unfold.invoke("search", { query, kinds: ["definitions"], limit: 20 })) as { definitions: { key: string; kind: string }[] };
        for (const hit of result.definitions) {
          if (hit.kind !== "class" && !found.includes(hit.key)) {
            found.push(hit.key);
          }
        }
      }
      return found.slice(0, 50);
    });
    expect(keys.length).toBe(50);
    const board = {
      format: "unfold-board",
      version: 1,
      title: "Fifty cards",
      defaultView: "code",
      cards: keys.map((key, index) => ({ id: `c${index}`, at: [(index % 8) * 620, Math.floor(index / 8) * 520], ref: { block: key }, view: "code" })),
      groups: [],
    };
    writeFile(join(folder, ".unfold/local/drafts/fifty-cards.json"), JSON.stringify(board));
    await showView(page, "Boards");
    await page.locator(".board-row", { hasText: "Fifty cards" }).click();
    await expect(activeBoard(page).locator(".board-tab")).toBeVisible();
    await activeBoard(page).locator(".react-flow__pane").click({ position: { x: 12, y: 12 } });
    await page.keyboard.press("Shift+Digit1");
    // Fit shows the whole board, so every card mounts and loads.
    await expect(cards(page)).toHaveCount(50, { timeout: 60_000 });
    await expect(activeBoard(page).locator(".board-card[data-loading]")).toHaveCount(0, { timeout: 60_000 });
    const pane = activeBoard(page).locator(".react-flow__pane");
    const box = await pane.boundingBox();
    expect(box).not.toBeNull();
    if (!box) {
      return;
    }
    await page.evaluate(() => {
      const store = window as unknown as { __frames?: { on: boolean; deltas: number[] } };
      const frames = { on: false, deltas: [] as number[] };
      store.__frames = frames;
      let last = 0;
      const tick = (now: number) => {
        if (frames.on && last > 0) {
          frames.deltas.push(now - last);
        }
        last = now;
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    });
    const measure = async (action: () => Promise<void>) => {
      await page.evaluate(() => {
        const frames = (window as unknown as { __frames: { on: boolean; deltas: number[] } }).__frames;
        frames.deltas = [];
        frames.on = true;
      });
      await action();
      return page.evaluate(() => {
        const frames = (window as unknown as { __frames: { on: boolean; deltas: number[] } }).__frames;
        frames.on = false;
        const total = frames.deltas.reduce((sum, value) => sum + value, 0);
        return { fps: frames.deltas.length / (total / 1000), worst: Math.max(...frames.deltas), frames: frames.deltas.length };
      });
    };
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    const wheel = async (control: boolean) => {
      if (control) {
        await page.keyboard.down("Control");
      }
      for (let round = 0; round < 12; round++) {
        for (const sign of [1, -1, -1, 1]) {
          for (let step = 0; step < 6; step++) {
            await page.mouse.wheel(control ? 0 : 20 * sign, (control ? 15 : 30) * sign);
          }
        }
      }
      if (control) {
        await page.keyboard.up("Control");
      }
    };
    const mountedAtFit = await cards(page).count();
    const panAll = await measure(() => wheel(false));
    const zoomAll = await measure(() => wheel(true));
    const shown = await cards(page).count();
    record("Canvas, 50 cards (pan and zoom at Fit)", {
      target: "60 fps",
      measured: `${Math.round(Math.min(panAll.fps, zoomAll.fps))} fps (worst frame ${Math.round(Math.max(panAll.worst, zoomAll.worst))} ms)`,
      detail: { panAll, zoomAll, mountedAtFit, mountedAfterZoom: shown },
    });
  });
});

test("save to updated card on a 2,000-file repository", async ({ sandbox, launch }) => {
  test.setTimeout(900_000);
  const count = 2000;
  const folder = sandbox.folder("synthetic-2000");
  writeSyntheticRepo(folder, count, "main");
  const app = await launch();
  const page = await app.firstWindow();
  const compileStart = Date.now();
  await openAndCompile(app, page, folder);
  const compileMs = Date.now() - compileStart;
  await expect(graphStatus(page)).toHaveText(`Graph current · 2,000 files · TypeScript`);
  record("First Compile, 2,000-file repository", { target: "< 5 s", measured: ms(compileMs) });

  await showView(page, "Search");
  await page.locator(".search-view").getByRole("searchbox").fill("f7");
  await page.getByRole("listbox", { name: "Search results" }).getByRole("option", { name: /^f7 / }).first().click();
  const target = card(page, "#function:f7");
  await expect(target.locator(".code-view")).toBeVisible();

  const toCard: number[] = [];
  const toEvent: number[] = [];
  for (let run = 0; run < 12; run++) {
    const mark = await eventMark(page, "graph_updated");
    const marker = `x + ${7 + 1000 * (run + 1)}`;
    const text = syntheticFile(7, count, "main").replace("x + 7", marker);
    const shown = whenTextShows(page, '.editor-board:not([hidden]) .board-card[data-key$="#function:f7"] .code-view', marker);
    const saved = Date.now();
    writeFile(join(folder, syntheticPath(7)), text);
    const update = await waitForEvent(page, "graph_updated", mark);
    toCard.push((await shown) - saved);
    toEvent.push(update.at - saved);
    await expectGraphCurrent(page);
  }
  const idleKb = await settledMemoryKb(app);
  record("Idle memory, one window, 2,000-file repository open", { target: "< 600 MB", measured: mb(idleKb), detail: await app.processMemory() });
  record("Save to updated card, 2,000-file repository (p95)", {
    target: "< 1 s",
    measured: ms(quantile(toCard, 0.95)),
    detail: { cardP50: quantile(toCard, 0.5), graphUpdatedP50: quantile(toEvent, 0.5), graphUpdatedP95: quantile(toEvent, 0.95), runs: toCard },
  });
});
