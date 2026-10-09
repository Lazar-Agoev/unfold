#!/usr/bin/env node
/**
 * Launches the built, unpackaged Unfold app on a throwaway app-data folder and saves a screenshot of its
 * first window. For checking a UI package against the Paper artboards (and for the orchestrator).
 *
 * Usage (from anywhere; paths are relative to the current directory):
 *
 *   node docs/redesign/build/screenshot.mjs --out docs/redesign/build/screens/u1c-home-light.png
 *   node docs/redesign/build/screenshot.mjs --theme dark --project testdata/playgrounds/ts-orders \
 *     --actions '[{"compile":true},{"newBoard":true},{"wait":500}]' --out /tmp/board-dark.png
 *
 * Options:
 *   --out <file.png>       Where to write the PNG (required). Its folder is created.
 *   --theme light|dark     The theme the window renders in (default light): Electron's
 *                          `nativeTheme.themeSource` and the page's `prefers-color-scheme` (Playwright pins
 *                          that to light unless told), so the UI's default "system" preference follows it.
 *   --project <folder>     A folder to add as a project (passed on the command line, as the OS would).
 *                          It is copied into the throwaway folder first (without `.unfold/local/`), so
 *                          the original is never written to; `--in-place` uses it as is.
 *   --actions <json|@file> Steps run in order before the screenshot (a JSON array, or `@path` to a JSON
 *                          file). Each step is an object with one of:
 *                            {"compile": true}           Compile every project and wait for it to finish.
 *                            {"newBoard": true}          New Board (⌘N's action) in the selected project.
 *                            {"run": "<action id>"}      Presses the action's default shortcut (ids listed in
 *                                                        SHORTCUTS below, e.g. "view.toggleSidebar").
 *                            {"press": "Meta+B"}         A Playwright key press.
 *                            {"click": "<selector>"}     Clicks a Playwright selector (`role=tab[name="Home"]`).
 *                            {"rightClick": "<selector>"} Opens a context menu.
 *                            {"hover": "<selector>"}     Hovers a selector.
 *                            {"waitFor": "<selector>"}   Waits until a selector is visible.
 *                            {"wait": <ms>}              Waits a fixed time (animations, debounces).
 *                            {"invoke": "<command>", "args": {...}}  Runs an engine command (`window.unfold.invoke`).
 *                            {"eval": "<js>"}            Evaluates JavaScript in the window.
 *                            {"log": "<js>"}             Evaluates JavaScript and prints the result as JSON
 *                                                        (measure geometry: getBoundingClientRect()).
 *   --size <W>x<H>         The window's content size (default 1440x900, the artboards' size).
 *   --scale css|device     Screenshot pixels: CSS pixels (default, matches the artboards) or the
 *                          display's (2x on Retina).
 *   --no-build             Skip `pnpm --filter @unfold/ui build` and `pnpm --filter @unfold/desktop build`.
 *                          By default the UI and the desktop shell are rebuilt first, so the shot shows the
 *                          current source. The engine addon must already be built (`just addon`).
 *   --keep                 Keep the throwaway folder (its path is printed) instead of deleting it.
 *
 * What it does: builds (unless --no-build); creates a temporary folder with `user-data/` and `home/`;
 * launches `electron desktop --user-data-dir=<tmp>/user-data [<project>]` with HOME=<tmp>/home through
 * Playwright's `_electron` (as e2e/ does); sets the theme and window size; waits for the tab strip, the
 * project row (with --project), and the fonts; runs the actions; screenshots the page; quits; cleans up.
 * Logs from the main process and page errors are printed when something fails.
 *
 * The macOS traffic lights are drawn by the OS outside the web contents, so they are not in the PNG; the
 * tab strip's 84px left inset is.
 */

import { execFileSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const desktopDir = join(repoRoot, "desktop");

function usage(message) {
  console.error(`screenshot.mjs: ${message}\nSee the comment at the top of docs/redesign/build/screenshot.mjs for the options.`);
  process.exit(2);
}

function parseArgs(argv) {
  const options = { theme: "light", size: { width: 1440, height: 900 }, scale: "css", build: true, keep: false, inPlace: false, actions: [] };
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    const value = () => {
      const next = argv[index + 1];
      if (next === undefined) {
        usage(`${flag} needs a value`);
      }
      index += 1;
      return next;
    };
    switch (flag) {
      case "--out":
        options.out = resolve(value());
        break;
      case "--theme":
        options.theme = value();
        if (options.theme !== "light" && options.theme !== "dark") {
          usage("--theme is light or dark");
        }
        break;
      case "--project":
        options.project = resolve(value());
        break;
      case "--in-place":
        options.inPlace = true;
        break;
      case "--actions": {
        const text = value();
        const json = text.startsWith("@") ? readFileSync(resolve(text.slice(1)), "utf8") : text;
        options.actions = JSON.parse(json);
        if (!Array.isArray(options.actions)) {
          usage("--actions is a JSON array of steps");
        }
        break;
      }
      case "--size": {
        const match = /^(\d+)x(\d+)$/.exec(value());
        if (!match) {
          usage("--size is <width>x<height>");
        }
        options.size = { width: Number(match[1]), height: Number(match[2]) };
        break;
      }
      case "--scale":
        options.scale = value();
        if (options.scale !== "css" && options.scale !== "device") {
          usage("--scale is css or device");
        }
        break;
      case "--no-build":
        options.build = false;
        break;
      case "--keep":
        options.keep = true;
        break;
      default:
        usage(`unknown option ${flag}`);
    }
  }
  if (!options.out) {
    usage("--out is required");
  }
  return options;
}

function build() {
  for (const pkg of ["@unfold/ui", "@unfold/desktop"]) {
    console.log(`Building ${pkg}…`);
    execFileSync("pnpm", ["--filter", pkg, "build"], { cwd: repoRoot, stdio: ["ignore", "ignore", "inherit"] });
  }
}

/** Runs one action step in the app. */
async function runStep(page, step) {
  if (step.compile) {
    await page.evaluate(async () => {
      const workspace = await window.unfold.invoke("workspace", {});
      for (const row of workspace.projects) {
        await window.unfold.stream("compile", { project: row.id, mode: "check" }, () => undefined).result;
      }
    });
  } else if (step.newBoard) {
    await page.evaluate(async () => {
      const workspace = await window.unfold.invoke("workspace", {});
      if (workspace.projects.length === 0) {
        throw new Error("newBoard needs a project (--project)");
      }
    });
    await pressAction(page, "board.new");
    await page.locator('[role="tab"][aria-selected="true"]').filter({ hasNotText: "Home" }).waitFor();
  } else if (step.run !== undefined) {
    await pressAction(page, step.run);
  } else if (step.press !== undefined) {
    await page.keyboard.press(step.press);
  } else if (step.click !== undefined) {
    await page.locator(step.click).first().click();
  } else if (step.rightClick !== undefined) {
    await page.locator(step.rightClick).first().click({ button: "right" });
  } else if (step.hover !== undefined) {
    await page.locator(step.hover).first().hover();
  } else if (step.waitFor !== undefined) {
    await page.locator(step.waitFor).first().waitFor({ state: "visible" });
  } else if (step.wait !== undefined) {
    await page.waitForTimeout(step.wait);
  } else if (step.invoke !== undefined) {
    await page.evaluate(({ command, args }) => window.unfold.invoke(command, args), { command: step.invoke, args: step.args ?? {} });
  } else if (step.eval !== undefined) {
    await page.evaluate(step.eval);
  } else if (step.log !== undefined) {
    console.log(JSON.stringify(await page.evaluate(step.log)));
  } else {
    throw new Error(`unknown action step ${JSON.stringify(step)}`);
  }
}

/** Presses an action's default shortcut (the menu would run the same action). */
const SHORTCUTS = {
  "board.new": "Meta+N",
  "view.toggleSidebar": "Meta+B",
  "tabs.home": "Meta+0",
  "tabs.close": "Meta+W",
  "add.open": "Meta+K",
  "app.preferences": "Meta+Comma",
  "project.compile": "Meta+Shift+B",
};

async function pressAction(page, id) {
  const shortcut = SHORTCUTS[id];
  if (!shortcut) {
    throw new Error(`no known shortcut for ${id}; use {"press": "..."} instead`);
  }
  const key = process.platform === "darwin" ? shortcut : shortcut.replace("Meta", "Control");
  // Focus the page body so the window's dispatcher receives the key.
  await page.locator("body").focus();
  await page.keyboard.press(key);
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.build) {
    build();
  }

  const requireFromE2e = createRequire(join(repoRoot, "e2e", "package.json"));
  const { _electron } = requireFromE2e("@playwright/test");
  const electronPath = createRequire(join(desktopDir, "package.json"))("electron");

  const root = realpathSync(mkdtempSync(join(tmpdir(), "unfold-shot-")));
  const userData = join(root, "user-data");
  const home = join(root, "home");
  mkdirSync(userData, { recursive: true });
  mkdirSync(home, { recursive: true });

  let project;
  if (options.project) {
    project = options.inPlace ? options.project : join(root, basename(options.project));
    if (!options.inPlace) {
      cpSync(options.project, project, { recursive: true });
      rmSync(join(project, ".unfold", "local"), { recursive: true, force: true });
    }
  }

  const env = {};
  for (const [key, value] of Object.entries(process.env)) {
    if (value !== undefined && key !== "ELECTRON_RUN_AS_NODE" && key !== "NODE_OPTIONS") {
      env[key] = value;
    }
  }
  env.HOME = home;

  const logs = [];
  const electronApp = await _electron.launch({
    executablePath: electronPath,
    args: [desktopDir, `--user-data-dir=${userData}`, ...(project ? [project] : [])],
    env,
  });
  const proc = electronApp.process();
  proc.stdout?.on("data", (chunk) => logs.push(chunk.toString()));
  proc.stderr?.on("data", (chunk) => logs.push(chunk.toString()));

  try {
    await electronApp.evaluate(({ nativeTheme }, theme) => {
      nativeTheme.themeSource = theme;
    }, options.theme);
    const page = await electronApp.firstWindow();
    page.on("pageerror", (error) => logs.push(`[pageerror] ${error.message}\n`));
    page.on("console", (message) => {
      if (message.type() === "error" || message.type() === "warning") {
        logs.push(`[console.${message.type()}] ${message.text()}\n`);
      }
    });
    await page.waitForLoadState("domcontentloaded");
    await page.emulateMedia({ colorScheme: options.theme });
    const window = await electronApp.browserWindow(page);
    await window.evaluate((browserWindow, size) => {
      browserWindow.setContentSize(size.width, size.height);
    }, options.size);

    await page.locator('[role="tablist"][aria-label="Window tabs"]').waitFor({ timeout: 30_000 });
    if (project) {
      await page.locator("[data-project-id]").first().waitFor({ timeout: 30_000 });
    }
    for (const step of options.actions) {
      await runStep(page, step);
    }
    // Let fonts load and the last layout settle.
    await page.evaluate(() => document.fonts.ready.then(() => undefined));
    await page.waitForTimeout(300);

    mkdirSync(dirname(options.out), { recursive: true });
    writeFileSync(options.out, await page.screenshot({ scale: options.scale, animations: "disabled" }));
    console.log(`Saved ${options.out}`);
  } catch (error) {
    console.error(logs.join(""));
    throw error;
  } finally {
    await electronApp.close().catch(() => undefined);
    if (options.keep) {
      console.log(`Kept ${root}`);
    } else {
      rmSync(root, { recursive: true, force: true });
    }
  }
}

await main();
