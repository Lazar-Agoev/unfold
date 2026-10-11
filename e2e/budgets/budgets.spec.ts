/**
 * Architecture §17's budgets, measured on the built app on this machine (run with the release
 * addon: `just e2e-budgets`), including idle memory with three projects connected, all live and
 * with resting applied (project liveness). Each test measures rows and records them in
 * `test-results/budgets.json`; docs/testing.md keeps the table.
 *
 * The 602-file corpus is TypeScript 6.0.3's `src` (testdata/corpora.lock.toml), read from
 * `UNFOLD_TS_CORPUS` or the proof of concept's vendored copy, and copied into the sandbox: the
 * source is never written. Tests that need it are skipped when it isn't there.
 */

import { cpSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import type { Page } from "@playwright/test";
import { addInput, openAdd, result, search, selectResult } from "../src/add";
import { UnfoldApp } from "../src/app";
import { boardView, card, cards } from "../src/board";
import { expect, test } from "../src/fixtures";
import {
  addProjectFromSidebar,
  expectGraphCurrent,
  firstCompile,
  homeTitle,
  projectId,
  projectRow,
  selectProject,
  sidebarBoard,
  stateLine,
  workspace,
} from "../src/home";
import type { Sandbox } from "../src/sandbox";
import { writeFile } from "../src/sandbox";
import { syntheticFile, syntheticPath, writeSyntheticRepo } from "../src/synthetic";
import { eventMark, expectConnection, waitForEvent, whenTextShows, windowCommand } from "../src/window";

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

interface Memory {
  /** Every Unfold process. */
  readonly totalKb: number;
  /** The main process, the engine host, and the renderers (the processes the three-project budget names). */
  readonly appKb: number;
  readonly processes: Awaited<ReturnType<UnfoldApp["processMemory"]>>;
}

async function memory(app: UnfoldApp): Promise<Memory> {
  const processes = await app.processMemory();
  const sum = (list: typeof processes) => list.reduce((total, process) => total + process.kb, 0);
  const app_ = processes.filter((process) => process.type === "Browser" || process.type === "Tab" || (process.type === "Utility" && process.name === "Unfold Engine"));
  return { totalKb: sum(processes), appKb: sum(app_), processes };
}

/** Memory once it stops moving: two samples a second apart within 2%. */
async function settledMemory(app: UnfoldApp): Promise<Memory> {
  let previous = await memory(app);
  for (let round = 0; round < 30; round++) {
    await new Promise((resolve) => setTimeout(resolve, 1000));
    const now = await memory(app);
    if (Math.abs(now.totalKb - previous.totalKb) / previous.totalKb < 0.02) {
      return now;
    }
    previous = now;
  }
  return previous;
}

function memoryText(sample: Memory): string {
  return `${mb(sample.totalKb)} all processes; ${mb(sample.appKb)} main + engine host + renderers`;
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

/** Definition keys of a project from a few searches. */
async function definitionKeys(page: Page, project: string, queries: readonly string[], count: number): Promise<string[]> {
  return page.evaluate(
    async ([id, list, wanted]) => {
      const found: string[] = [];
      for (const query of list) {
        const answer = (await window.unfold.invoke("search", { query, kinds: ["definitions"], limit: 20, projects: [id] })) as {
          definitions: { hits: { key: string; kind: string }[] };
        };
        for (const hit of answer.definitions.hits) {
          if (hit.kind !== "class" && !found.includes(hit.key)) {
            found.push(hit.key);
          }
        }
      }
      return found.slice(0, wanted);
    },
    [project, queries, count] as const,
  );
}

/** Whether every window answers `project_summary` for `project` (the engine is back for it). */
async function usable(page: Page, project: string): Promise<boolean> {
  return page.evaluate((id) => window.unfold.invoke("project_summary", { project: id }).then(() => true, () => false), project);
}

test.describe.configure({ mode: "serial" });

test("launch to the first window", async ({ sandbox }) => {
  const times: number[] = [];
  for (let run = 0; run < 3; run++) {
    const start = Date.now();
    const app = await UnfoldApp.launch(sandbox);
    const page = await app.electron.firstWindow();
    await expect(page.getByRole("heading", { name: "Read a codebase by how it starts." })).toBeVisible();
    times.push(Date.now() - start);
    await app.quit();
  }
  record("Launch to the first window", { target: "< 1.5 s", measured: ms(quantile(times, 0.5)), detail: times });
});

test.describe("on TypeScript's 602-file compiler source", () => {
  test.skip(!existsSync(CORPUS), `the corpus isn't at ${CORPUS} (set UNFOLD_TS_CORPUS)`);

  test("first Compile, search, cards, cancel, memory, windows, restart, opening it compiled", async ({ sandbox, launch }) => {
    const folder = copyCorpus(sandbox);
    const small = sandbox.playground("ts-orders");
    let app = await launch({ args: [folder] });
    let page = await app.firstWindow();
    await expect(homeTitle(page)).toHaveText("typescript-src");
    await expectConnection(page, "connected");
    const project = await projectId(page, "typescript-src");

    // First Compile: press Compile, until the graph is current; peak memory of every process.
    let peakKb = 0;
    const sampling = { on: true };
    const sampler = (async () => {
      while (sampling.on) {
        peakKb = Math.max(peakKb, await memory(app).then((sample) => sample.totalKb, () => 0));
        await new Promise((resolve) => setTimeout(resolve, 100));
      }
    })();
    const compileStart = Date.now();
    await firstCompile(page);
    const compileMs = Date.now() - compileStart;
    sampling.on = false;
    await sampler;
    record("First Compile, 602-file TypeScript source", {
      target: "< 5 s, < 1 GB peak",
      measured: `${ms(compileMs)}, ${mb(peakKb)} peak (all processes)`,
      detail: (await stateLine(page).textContent()) ?? "",
    });

    // Search: 40 queries across the workspace, p95 of the round trip from the window.
    const queries = ["checker", "createNode", "emit", "parse", "Symbol", "getTypeOf", "visit", "scanner", "binder", "resolve", "factory", "Diagnostic", "program", "host", "transform", "isIdentifier", "flags", "node", "sourceFile", "compile"];
    const searchTimes = await timedInvokes(page, "search", [...queries, ...queries].map((query) => ({ query, limit: 50 })));
    record("Search, 95th percentile", { target: "< 100 ms", measured: ms(quantile(searchTimes, 0.95)), detail: { p50: quantile(searchTimes, 0.5), max: Math.max(...searchTimes) } });

    // Open a cached card: the `card` command for 50 definitions, the second time (cached).
    const keys = await definitionKeys(page, project, queries, 50);
    const cold = await timedInvokes(page, "card", keys.map((key) => ({ project, key })));
    const warm = await timedInvokes(page, "card", keys.map((key) => ({ project, key })));
    record("Open a cached card, 95th percentile (engine round trip)", {
      target: "< 150 ms",
      measured: ms(quantile(warm, 0.95)),
      detail: { cards: keys.length, coldP95: quantile(cold, 0.95), warmP50: quantile(warm, 0.5) },
    });

    // Cancel: start Rebuild from scratch, cancel it, until the engine acknowledges.
    const cancels: number[] = [];
    for (let run = 0; run < 5; run++) {
      cancels.push(
        await page.evaluate(
          async ([id, after]) => {
            const handle = window.unfold.stream("compile", { project: id, mode: "from_scratch" }, () => undefined);
            await new Promise((resolve) => setTimeout(resolve, after));
            const start = performance.now();
            handle.cancel();
            await handle.result.catch(() => undefined);
            return performance.now() - start;
          },
          [project, 200 + run * 300] as const,
        ),
      );
      await expectGraphCurrent(page, 120_000);
    }
    record("Cancel acknowledged", { target: "< 250 ms", measured: `${ms(quantile(cancels, 0.5))} median, ${ms(Math.max(...cancels))} worst`, detail: cancels });

    // Idle memory with one window showing the project's Home.
    const one = await settledMemory(app);
    record("Idle memory, one window, 602-file project open", { target: "(comparison; the budget is the 2,000-file row)", measured: memoryText(one), detail: one.processes });

    // Each further window on the workspace (it shows the selected project's Home).
    const second = await app.nextWindow(() => windowCommand(page, "new_window"));
    await expectGraphCurrent(second);
    const two = await settledMemory(app);
    const third = await app.nextWindow(() => windowCommand(page, "new_window"));
    await expectGraphCurrent(third);
    const three = await settledMemory(app);
    record("Each further window on the workspace", {
      target: "< 150 MB more",
      measured: `${mb(two.totalKb - one.totalKb)}, ${mb(three.totalKb - two.totalKb)}`,
      detail: { one: one.totalKb, two: two.totalKb, three: three.totalKb },
    });

    // Engine-host restart until every window is usable again (three windows), three times.
    const restarts: number[] = [];
    for (let run = 0; run < 3; run++) {
      await app.killEngineHost();
      const start = Date.now();
      for (const win of app.windows()) {
        await expect.poll(() => usable(win, project), { timeout: 30_000, intervals: [20] }).toBe(true);
      }
      restarts.push(Date.now() - start);
      for (const win of app.windows()) {
        await expectConnection(win, "connected");
      }
    }
    record("Engine-host restart until every window is usable (3 windows)", {
      target: "< 2 s",
      measured: `${ms(quantile(restarts, 0.5))} median, ${ms(Math.max(...restarts))} worst`,
      detail: restarts,
    });

    // Open the compiled project: a second project is shown when the app quits, so the corpus rests
    // on relaunch; then its row is clicked, until its Home shows the graph current and a card answers.
    await windowCommand(second, "close_window");
    await windowCommand(third, "close_window");
    await addProjectFromSidebar(app, page, small);
    await expect(homeTitle(page)).toHaveText("ts-orders");
    await firstCompile(page);
    const [key] = keys;
    if (key === undefined) {
      throw new Error("no definition to open");
    }
    await app.quit();
    const opens: number[] = [];
    const toCurrent: number[] = [];
    for (let run = 0; run < 2; run++) {
      app = await launch();
      page = await app.firstWindow();
      await expect(homeTitle(page)).toHaveText("ts-orders");
      await expectConnection(page, "connected");
      await expect.poll(async () => (await workspace(page)).projects.find((row) => row.id === project)?.live, { timeout: 30_000 }).toBe(false);
      const start = Date.now();
      await projectRow(page, "typescript-src").locator(".home-project-title").click();
      // Usable: its Home shows and the graph answers (it has no entrances; a card reads it).
      await expect(homeTitle(page)).toHaveText("typescript-src");
      await expect.poll(() => page.evaluate(([id, k]) => window.unfold.invoke("card", { project: id, key: k }).then(() => true, () => false), [project, key] as const), { intervals: [20] }).toBe(true);
      opens.push(Date.now() - start);
      // The reopen check (files compared by size and time) then reports the graph current.
      await expectGraphCurrent(page, 30_000);
      toCurrent.push(Date.now() - start);
      await selectProject(page, "ts-orders");
      await app.quit();
    }
    record("Open a compiled project to a usable graph (602 files, resting)", {
      target: "< 1 s",
      measured: `${opens.map(ms).join(", ")} (Graph current after ${toCurrent.map(ms).join(", ")})`,
      detail: { opens, toCurrent },
    });
  });

  test("canvas with 50 cards", async ({ sandbox, launch }) => {
    const folder = copyCorpus(sandbox);
    const app = await launch({ args: [folder] });
    const page = await app.firstWindow();
    await expect(homeTitle(page)).toHaveText("typescript-src");
    await firstCompile(page);
    const project = await projectId(page, "typescript-src");
    const keys = await definitionKeys(page, project, ["create", "get", "is", "visit", "emit", "parse", "check", "resolve"], 50);
    expect(keys.length).toBe(50);
    const board = {
      format: "unfold-board",
      version: 1,
      title: "Fifty cards",
      projects: [],
      pages: [
        {
          id: "page-1",
          title: "Page 1",
          defaultView: "code",
          cards: keys.map((key, index) => ({ id: `c${index}`, at: [(index % 8) * 620, Math.floor(index / 8) * 520], ref: { block: key }, view: "code" })),
          groups: [],
          labels: [],
          links: [],
        },
      ],
    };
    writeFile(join(folder, ".unfold/local/drafts/fifty-cards.json"), JSON.stringify(board));
    await expect(sidebarBoard(page, "Fifty cards")).toBeVisible();
    await sidebarBoard(page, "Fifty cards").click();
    await expect(boardView(page)).toBeVisible();
    await boardView(page).locator(".react-flow__pane").click({ position: { x: 12, y: 12 } });
    await page.keyboard.press("Shift+Digit1");
    // Fit shows the whole page, so every card mounts and loads.
    await expect(cards(page)).toHaveCount(50, { timeout: 60_000 });
    await expect(boardView(page).locator(".board-card[data-loading]")).toHaveCount(0, { timeout: 60_000 });
    const box = await boardView(page).locator(".react-flow__pane").boundingBox();
    if (!box) {
      throw new Error("no canvas");
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
    const pan = await measure(() => wheel(false));
    const zoom = await measure(() => wheel(true));
    record("Canvas, 50 cards (pan and zoom at Fit)", {
      target: "60 fps",
      measured: `${Math.round(Math.min(pan.fps, zoom.fps))} fps (worst frame ${Math.round(Math.max(pan.worst, zoom.worst))} ms)`,
      detail: { pan, zoom, mountedAtFit, mountedAfterZoom: await cards(page).count() },
    });
  });
});

test("a 2,000-file repository: first Compile, save to updated card, idle memory", async ({ sandbox, launch }) => {
  const count = 2000;
  const folder = sandbox.folder("synthetic-2000");
  writeSyntheticRepo(folder, count, "main");
  const app = await launch({ args: [folder] });
  const page = await app.firstWindow();
  await expect(homeTitle(page)).toHaveText("synthetic-2000");
  await expectConnection(page, "connected");
  const compileStart = Date.now();
  await firstCompile(page);
  const compileMs = Date.now() - compileStart;
  await expect(stateLine(page)).toContainText(/Graph current\s*· 2,000 files · TypeScript/);
  record("First Compile, 2,000-file repository", { target: "< 5 s", measured: ms(compileMs) });

  // From Home, Enter on a function makes a new draft with its card.
  await openAdd(page);
  await search(page, "f7");
  const f7 = result(page, /^Function f7,/);
  await selectResult(page, f7);
  await addInput(page).press("Enter");
  const target = card(page, "#function:f7");
  await expect(target.locator(".code-view")).toBeVisible();

  const toCard: number[] = [];
  const toEvent: number[] = [];
  for (let run = 0; run < 12; run++) {
    const mark = await eventMark(page, "graph_updated");
    const marker = `x + ${7 + 1000 * (run + 1)}`;
    const text = syntheticFile(7, count, "main").replace("x + 7", marker);
    const shown = whenTextShows(page, '.board-view .board-card[data-key$="#function:f7"] .code-view', marker);
    const saved = Date.now();
    writeFile(join(folder, syntheticPath(7)), text);
    const update = await waitForEvent(page, "graph_updated", mark);
    toCard.push((await shown) - saved);
    toEvent.push(update.at - saved);
  }
  record("Save to updated card, 2,000-file repository (p95, a body-only edit)", {
    target: "< 1 s (V1); < 300 ms (V2)",
    measured: ms(quantile(toCard, 0.95)),
    detail: { cardP50: quantile(toCard, 0.5), graphUpdatedP50: quantile(toEvent, 0.5), graphUpdatedP95: quantile(toEvent, 0.95), runs: toCard },
  });
  const idle = await settledMemory(app);
  record("Idle memory, one window, 2,000-file repository open", { target: "< 600 MB", measured: memoryText(idle), detail: idle.processes });
});

test("idle memory with three projects connected: all live, then resting", async ({ sandbox, launch }) => {
  test.setTimeout(900_000);
  const synthetic = sandbox.folder("synthetic-2000");
  writeSyntheticRepo(synthetic, 2000, "main");
  const orders = sandbox.playground("ts-orders");
  const python = sandbox.playground("py-orders");
  const app = await launch({ args: [synthetic] });
  const page = await app.firstWindow();
  await expect(homeTitle(page)).toHaveText("synthetic-2000");
  await firstCompile(page);
  for (const [folder, name] of [[orders, "ts-orders"], [python, "py-orders"]] as const) {
    await addProjectFromSidebar(app, page, folder);
    await expect(homeTitle(page)).toHaveText(name);
    await firstCompile(page);
  }
  // Home shows the largest project; each was just used, so all three hold a lease and are live.
  await selectProject(page, "synthetic-2000");
  await expectGraphCurrent(page);
  await expect.poll(async () => (await workspace(page)).projects.map((row) => row.live)).toEqual([true, true, true]);
  const live = await settledMemory(app);
  record("Idle memory, three projects connected, all live (2,000 files + ts-orders + py-orders)", {
    target: "< 600 MB",
    measured: memoryText(live),
    detail: live.processes,
  });

  // A window always shows one project on Home, which stays live (project liveness); once their five-minute leases
  // run out, the other two rest.
  await expect
    .poll(async () => (await workspace(page)).projects.map((row) => row.live), { timeout: 480_000, intervals: [5_000] })
    .toEqual([true, false, false]);
  const resting = await settledMemory(app);
  record("Idle memory, three projects connected, resting applied (2,000 files shown and live; two resting)", {
    target: "< 600 MB",
    measured: memoryText(resting),
    detail: resting.processes,
  });
});
