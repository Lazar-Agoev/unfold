/**
 * The packaged-app smoke test (docs/architecture.md#security and §16 "Packaging", migration Step 11). Run it after
 * `just bundle-dir` (`just smoke`, or `UNFOLD_APP=<app or executable> node scripts/smoke.mts`).
 *
 * 1. Reads the Electron fuses back from the packaged binary.
 * 2. Launches the packaged app with an empty PATH (no Node, no Python), a throwaway home and
 *    user-data folder, `ELECTRON_RUN_AS_NODE`, `NODE_OPTIONS` and `--inspect`, which the fuses must
 *    all ignore, on a copy of the TypeScript playground in a path with spaces and non-ASCII letters.
 *    Playwright attaches over the Chrome DevTools protocol (the only door the fuses leave), finds the
 *    folder added as a project, compiles it through `window.unfold`, waits for it to be current,
 *    and checks the window's sandbox from the inside: no Node globals, only `window.unfold`, inline scripts and the network
 *    blocked.
 * 3. Checks that nothing in the folder changed outside `.unfold/`, and that no execution sentinel
 *    planted in it ran.
 * 4. Reads every window's `webPreferences` from the main process. Playwright's Electron driver needs
 *    the Node inspector, which the shipped fuses forbid, so this step runs on a copy of the app
 *    whose only change is that one fuse turned back on.
 */

import type { ChildProcess } from "node:child_process";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { flipFuses, FuseState, FuseV1Options, FuseVersion, getCurrentFuseWire } from "@electron/fuses";
import type { Browser, Page } from "playwright-core";
import { _electron, chromium } from "playwright-core";

const desktopDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(desktopDir, "..");
const playground = join(repoRoot, "testdata", "playgrounds", "ts-orders");

/** The shipped fuse settings (desktop/electron-builder.yml `electronFuses`). */
const EXPECTED_FUSES: readonly (readonly [FuseV1Options, boolean])[] = [
  [FuseV1Options.RunAsNode, false],
  [FuseV1Options.EnableCookieEncryption, true],
  [FuseV1Options.EnableNodeOptionsEnvironmentVariable, false],
  [FuseV1Options.EnableNodeCliInspectArguments, false],
  [FuseV1Options.EnableEmbeddedAsarIntegrityValidation, true],
  [FuseV1Options.OnlyLoadAppFromAsar, true],
  [FuseV1Options.LoadBrowserProcessSpecificV8Snapshot, false],
  [FuseV1Options.GrantFileProtocolExtraPrivileges, false],
];

/**
 * Every window's hardening (desktop/src/main/windows.ts), as Electron's `getLastWebPreferences`
 * reports it. It doesn't report `spellcheck`, `devTools`, or `navigateOnDragDrop`; DevTools are
 * checked by trying to open them.
 */
const EXPECTED_PREFERENCES: Readonly<Record<string, boolean>> = {
  contextIsolation: true,
  sandbox: true,
  nodeIntegration: false,
  nodeIntegrationInWorker: false,
  nodeIntegrationInSubFrames: false,
  webviewTag: false,
  webSecurity: true,
  allowRunningInsecureContent: false,
  safeDialogs: true,
};

/** What the preload exposes, and nothing more (docs/architecture.md#security). */
const UNFOLD_KEYS = ["addDroppedFolder", "invoke", "on", "onConnection", "stream", "window"];

// --- Reporting ---------------------------------------------------------------------------------

const results: { readonly name: string; readonly ok: boolean; readonly detail: string }[] = [];

function log(line: string): void {
  process.stdout.write(`${line}\n`);
}

function check(name: string, ok: boolean, detail = ""): void {
  results.push({ name, ok, detail });
  log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
}

// --- The packaged app --------------------------------------------------------------------------

interface PackagedApp {
  /** What `@electron/fuses` reads: the `.app` bundle on macOS, the executable elsewhere. */
  readonly fuseTarget: string;
  readonly executable: string;
  /** The folder to copy for the inspectable twin. */
  readonly root: string;
}

function packagedApp(location: string): PackagedApp {
  if (process.platform === "darwin") {
    return { fuseTarget: location, executable: join(location, "Contents", "MacOS", "Unfold"), root: location };
  }
  return { fuseTarget: location, executable: location, root: dirname(location) };
}

function findPackagedApp(): PackagedApp {
  const override = process.env["UNFOLD_APP"];
  if (override) {
    return packagedApp(resolve(override));
  }
  const dist = join(desktopDir, "dist");
  const candidates =
    process.platform === "darwin"
      ? ["mac-arm64", "mac", "mac-universal"].map((dir) => join(dist, dir, "Unfold.app"))
      : process.platform === "win32"
        ? [join(dist, "win-unpacked", "Unfold.exe")]
        : [join(dist, "linux-unpacked", "unfold")];
  const found = candidates.find((path) => existsSync(path));
  if (!found) {
    throw new Error(`no packaged app under ${dist}: run \`just bundle-dir\` first (or set UNFOLD_APP)`);
  }
  return packagedApp(found);
}

// --- Helpers -----------------------------------------------------------------------------------

function sleep(ms: number): Promise<void> {
  return new Promise((done) => setTimeout(done, ms));
}

async function poll<T>(what: string, timeoutMs: number, probe: () => Promise<T | null>): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = await probe().catch(() => null);
    if (value !== null) {
      return value;
    }
    if (Date.now() > deadline) {
      throw new Error(`timed out waiting for ${what}`);
    }
    await sleep(250);
  }
}

function freePort(): Promise<number> {
  return new Promise((done, fail) => {
    const server = createServer();
    server.once("error", fail);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => {
        if (typeof address === "object" && address !== null) {
          done(address.port);
        } else {
          fail(new Error("no port"));
        }
      });
    });
  });
}

/** A minimal environment: a throwaway home, an empty PATH, and only what the display needs. */
function isolatedEnv(work: string): Record<string, string> {
  const home = join(work, "home");
  const emptyBin = join(work, "empty-bin");
  const temp = join(work, "tmp");
  for (const dir of [home, emptyBin, temp]) {
    mkdirSync(dir, { recursive: true });
  }
  const env: Record<string, string> = { HOME: home, PATH: emptyBin, TMPDIR: temp, LANG: "en_US.UTF-8" };
  const passThrough =
    process.platform === "win32"
      ? ["SystemRoot", "SystemDrive", "WINDIR"]
      : process.platform === "linux"
        ? ["DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "XDG_RUNTIME_DIR"]
        : [];
  for (const name of passThrough) {
    const value = process.env[name];
    if (value) {
      env[name] = value;
    }
  }
  if (process.platform === "win32") {
    Object.assign(env, {
      USERPROFILE: home,
      APPDATA: join(home, "AppData", "Roaming"),
      LOCALAPPDATA: join(home, "AppData", "Local"),
      TEMP: temp,
      TMP: temp,
    });
  }
  return env;
}

/** Every file under `root` but `.unfold/`, by relative path, with its SHA-256. */
function snapshot(root: string): Map<string, string> {
  const files = new Map<string, string>();
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      const rel = relative(root, path).split("\\").join("/");
      if (rel === ".unfold") {
        continue;
      }
      if (entry.isDirectory()) {
        files.set(`${rel}/`, "dir");
        walk(path);
      } else {
        files.set(rel, createHash("sha256").update(readFileSync(path)).digest("hex"));
      }
    }
  };
  walk(root);
  return files;
}

function diff(before: Map<string, string>, after: Map<string, string>): string[] {
  const changes: string[] = [];
  for (const [path, hash] of after) {
    const old = before.get(path);
    if (old === undefined) {
      changes.push(`added ${path}`);
    } else if (old !== hash) {
      changes.push(`changed ${path}`);
    }
  }
  for (const path of before.keys()) {
    if (!after.has(path)) {
      changes.push(`removed ${path}`);
    }
  }
  return changes.sort();
}

interface Launched {
  readonly child: ChildProcess;
  readonly output: () => string;
  readonly exited: Promise<number | null>;
}

function launch(executable: string, args: readonly string[], env: Record<string, string>): Launched {
  const child = spawn(executable, args, { env, stdio: ["ignore", "pipe", "pipe"] });
  let output = "";
  child.stdout.on("data", (chunk: Buffer) => (output += chunk.toString()));
  child.stderr.on("data", (chunk: Buffer) => (output += chunk.toString()));
  const exited = new Promise<number | null>((done) => {
    child.once("exit", (code) => {
      done(code);
    });
  });
  return { child, output: () => output, exited };
}

async function stop(app: Launched, timeoutMs: number): Promise<boolean> {
  if (app.child.exitCode !== null) {
    return true;
  }
  app.child.kill("SIGTERM");
  const graceful = await Promise.race([app.exited.then(() => true), sleep(timeoutMs).then(() => false)]);
  if (!graceful) {
    app.child.kill("SIGKILL");
    await app.exited;
  }
  return graceful;
}

/** Plants files that would leave a marker if anything ran them (nothing may). */
function plantSentinels(folder: string): string[] {
  const markers = ["EXECUTED-BY-NODE", "EXECUTED-BY-PYTHON", "EXECUTED-BY-NODE-OPTIONS"].map((name) =>
    join(folder, name),
  );
  writeFileSync(
    join(folder, "src", "unfold-smoke-sentinel.ts"),
    `import { writeFileSync } from "node:fs";\nwriteFileSync(${JSON.stringify(markers[0])}, "ran");\n`,
  );
  writeFileSync(
    join(folder, "unfold_smoke_sentinel.py"),
    `open(${JSON.stringify(markers[1])}, "w").write("ran")\n`,
  );
  return markers;
}

// --- Steps -------------------------------------------------------------------------------------

async function checkFuses(app: PackagedApp): Promise<void> {
  const wire = await getCurrentFuseWire(app.fuseTarget);
  const readback: string[] = [];
  for (const [option, expected] of EXPECTED_FUSES) {
    const state = wire[option];
    const name = FuseV1Options[option];
    readback.push(`${name}=${state === FuseState.ENABLE ? "on" : state === FuseState.DISABLE ? "off" : String(state)}`);
    check(`fuse ${name} is ${expected ? "on" : "off"}`, state === (expected ? FuseState.ENABLE : FuseState.DISABLE));
  }
  log(`      fuse readback: ${readback.join(", ")}`);
}

/** With RunAsNode off, ELECTRON_RUN_AS_NODE starts the app instead of a Node REPL. */
async function checkRunAsNodeIgnored(app: PackagedApp, work: string): Promise<void> {
  const env = { ...isolatedEnv(join(work, "run-as-node")), ELECTRON_RUN_AS_NODE: "1" };
  const marker = "UNFOLD_SMOKE_RAN_AS_NODE";
  const launched = launch(app.executable, ["-p", `"${marker}"`, `--user-data-dir=${join(work, "run-as-node", "ud")}`], env);
  await Promise.race([launched.exited, sleep(5_000)]);
  await stop(launched, 10_000);
  check("ELECTRON_RUN_AS_NODE is ignored", !launched.output().includes(marker));
}

interface WorkspaceLike {
  readonly projects: readonly { readonly id: string }[];
}

interface ProjectSummaryLike {
  readonly state: { readonly kind: string };
  readonly files: number;
  readonly languages: readonly string[];
}

interface SmokeWindow {
  readonly unfold: {
    invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
    stream(command: string, args: Record<string, unknown>, onMessage: (message: unknown) => void): { result: Promise<unknown> };
    window(command: string, args?: Record<string, unknown>): Promise<unknown>;
  };
}

async function unfoldPage(browser: Browser): Promise<Page> {
  return poll("the Unfold window", 30_000, () => {
    const page = browser.contexts().flatMap((context) => context.pages()).find((p) => p.url().startsWith("app://unfold/"));
    return Promise.resolve(page ?? null);
  });
}

async function journey(app: PackagedApp, work: string, folder: string): Promise<void> {
  const env = isolatedEnv(join(work, "journey"));
  const nodeOptionsMarker = join(folder, "EXECUTED-BY-NODE-OPTIONS");
  const preload = join(work, "node-options-sentinel.cjs");
  writeFileSync(preload, `require("fs").writeFileSync(${JSON.stringify(nodeOptionsMarker)}, "ran");\n`);
  env["NODE_OPTIONS"] = `--require "${preload}"`;
  log(`      PATH for the app: ${env["PATH"] ?? ""} (empty)`);

  const port = await freePort();
  const launched = launch(
    app.executable,
    ["--inspect=0", `--remote-debugging-port=${port}`, `--user-data-dir=${join(work, "journey", "user-data")}`, folder],
    env,
  );
  let browser: Browser | null = null;
  try {
    browser = await poll("the DevTools endpoint", 30_000, async () => chromium.connectOverCDP(`http://127.0.0.1:${port}`));
    const page = await unfoldPage(browser);
    await page.waitForFunction(() => typeof (window as unknown as Partial<SmokeWindow>).unfold === "object");

    const project = await poll("the folder added as a project", 30_000, async () => {
      const value = (await page.evaluate(() => (window as unknown as SmokeWindow).unfold.invoke("workspace", {}))) as WorkspaceLike;
      return value.projects[0]?.id ?? null;
    });

    const started = Date.now();
    const compile = await page.evaluate(async (id) => {
      const unfold = (window as unknown as SmokeWindow).unfold;
      let progress = 0;
      const outcome = await unfold.stream("compile", { project: id }, () => {
        progress += 1;
      }).result;
      return { outcome, progress };
    }, project);
    check("compile finishes", compile.outcome !== null, `${compile.progress} progress messages, ${Date.now() - started} ms`);

    const summary = await poll("the project current", 60_000, async () => {
      const value = (await page.evaluate(
        (id) => (window as unknown as SmokeWindow).unfold.invoke("project_summary", { project: id }),
        project,
      )) as ProjectSummaryLike;
      return value.state.kind === "current" ? value : null;
    });
    check(
      "the project is current",
      summary.state.kind === "current" && summary.files > 0 && summary.languages.includes("typescript"),
      `${summary.files} files, ${summary.languages.join(", ")}`,
    );

    const inside = await page.evaluate(async () => {
      const globals = window as unknown as Record<string, unknown>;
      const inline = document.createElement("script");
      inline.textContent = "window.__unfoldSmokeInline = true;";
      document.head.append(inline);
      let networkBlocked = false;
      try {
        await fetch("https://example.com/");
      } catch {
        networkBlocked = true;
      }
      return {
        nodeGlobals: ["require", "process", "module", "Buffer", "global"].filter((name) => name in globals),
        unfoldKeys: Object.keys(globals["unfold"] as object).sort(),
        inlineScriptRan: globals["__unfoldSmokeInline"] === true,
        networkBlocked,
        windowOpenBlocked: window.open("https://example.com/") === null,
        origin: location.origin,
      };
    });
    check("the window has no Node globals", inside.nodeGlobals.length === 0, inside.nodeGlobals.join(", "));
    check("window.unfold is the only bridge", JSON.stringify(inside.unfoldKeys) === JSON.stringify(UNFOLD_KEYS), inside.unfoldKeys.join(", "));
    check("the UI is served from app://unfold", inside.origin === "app://unfold");
    check("inline scripts are blocked (CSP)", !inside.inlineScriptRan);
    check("network requests are blocked", inside.networkBlocked);
    check("window.open is denied", inside.windowOpenBlocked);
  } finally {
    await browser?.close().catch(() => undefined);
    const graceful = await stop(launched, 15_000);
    check("the app quits on SIGTERM", graceful);
  }
  const output = launched.output();
  check("--inspect is ignored", !output.includes("Debugger listening"));
  check("NODE_OPTIONS is ignored", !existsSync(nodeOptionsMarker));
  check("the engine addon loaded from <resources>/unfold-node", /loaded the engine addon from .*unfold-node/.test(output));
}

async function checkWindowPreferences(app: PackagedApp, work: string, folder: string): Promise<void> {
  const twinDir = join(work, "inspectable");
  mkdirSync(twinDir, { recursive: true });
  const twinRoot = join(twinDir, basename(app.root));
  cpSync(app.root, twinRoot, { recursive: true, verbatimSymlinks: true });
  const twin = packagedApp(join(twinRoot, relative(app.root, app.fuseTarget)));
  await flipFuses(twin.fuseTarget, {
    version: FuseVersion.V1,
    resetAdHocDarwinSignature: process.platform === "darwin",
    [FuseV1Options.EnableNodeCliInspectArguments]: true,
  });

  const env = isolatedEnv(join(work, "preferences"));
  const electronApp = await _electron.launch({
    executablePath: twin.executable,
    args: [`--user-data-dir=${join(work, "preferences", "user-data")}`, folder],
    env,
    timeout: 60_000,
  });
  try {
    const page = await electronApp.firstWindow();
    await page.waitForFunction(() => typeof (window as unknown as Partial<SmokeWindow>).unfold === "object");
    await page.evaluate(() => (window as unknown as SmokeWindow).unfold.window("new_window"));
    await poll("a second window", 30_000, () => Promise.resolve(electronApp.windows().length >= 2 ? true : null));

    const report = await electronApp.evaluate(({ app: electronAppModule, BrowserWindow }) => ({
      isPackaged: electronAppModule.isPackaged,
      windows: BrowserWindow.getAllWindows().map((window) => {
        // Present at runtime but missing from Electron's type definitions.
        const contents = window.webContents as typeof window.webContents & {
          getLastWebPreferences(): Record<string, unknown> | null;
        };
        contents.openDevTools({ mode: "detach" });
        return {
          url: contents.getURL(),
          preferences: contents.getLastWebPreferences(),
          devToolsOpened: contents.isDevToolsOpened(),
        };
      }),
    }));
    check("the app runs packaged", report.isPackaged);
    check("two windows are open", report.windows.length >= 2, `${report.windows.length} windows`);
    for (const [index, window] of report.windows.entries()) {
      const wrong = Object.entries(EXPECTED_PREFERENCES)
        .filter(([key, expected]) => window.preferences?.[key] !== expected)
        .map(([key, expected]) => `${key}=${String(window.preferences?.[key])} (want ${String(expected)})`);
      check(`window ${index + 1} webPreferences are hardened`, wrong.length === 0, wrong.join(", ") || window.url);
      check(`window ${index + 1} can't open DevTools`, !window.devToolsOpened);
    }
  } finally {
    await electronApp.close().catch(() => undefined);
  }
}

// --- Main --------------------------------------------------------------------------------------

async function main(): Promise<void> {
  const app = findPackagedApp();
  log(`Packaged app: ${app.fuseTarget}`);
  const work = mkdtempSync(join(tmpdir(), "unfold smoke "));
  const folder = join(work, "playground ünïcode", "ts orders");
  cpSync(playground, folder, { recursive: true });
  const markers = plantSentinels(folder);
  const before = snapshot(folder);

  try {
    await checkFuses(app);
    await checkRunAsNodeIgnored(app, work);
    await journey(app, work, folder);

    const changes = diff(before, snapshot(folder));
    check("nothing outside .unfold/ changed in the folder", changes.length === 0, changes.join("; "));
    check("no execution sentinel ran", markers.every((marker) => !existsSync(marker)));
    check(".unfold/local/ holds the graph", existsSync(join(folder, ".unfold", "local")));

    await checkWindowPreferences(app, work, folder);
  } finally {
    if (process.env["UNFOLD_SMOKE_KEEP"]) {
      log(`Kept ${work}`);
    } else {
      try {
        rmSync(work, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
      } catch (error) {
        // A helper process that outlives the app (Windows) can hold a file open; that is no failure.
        log(`Could not remove ${work}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
  }

  const failed = results.filter((result) => !result.ok);
  log(`\n${results.length - failed.length} passed, ${failed.length} failed`);
  process.exitCode = failed.length === 0 ? 0 : 1;
}

await main();
