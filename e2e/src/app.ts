/**
 * Launching the built, unpackaged app with a sandbox's user data and home, and driving what only
 * the main process can do: stubbing dialogs, a second launch, the clipboard, finding and killing the
 * engine host, memory, quitting.
 */

import { spawn } from "node:child_process";
import type { ElectronApplication, Page } from "@playwright/test";
import { _electron } from "@playwright/test";
import { desktopDir, electronExecutable } from "./paths";
import type { Sandbox } from "./sandbox";
import { prepareWindow } from "./window";

export interface LaunchOptions {
  /** Extra command-line arguments, such as a folder to add as a project. */
  readonly args?: readonly string[];
}

/** The environment the app runs in: this process's, with a throwaway home. */
function appEnv(sandbox: Sandbox): Record<string, string> {
  const env: Record<string, string> = {};
  for (const [key, value] of Object.entries(process.env)) {
    if (value !== undefined && key !== "ELECTRON_RUN_AS_NODE" && key !== "NODE_OPTIONS") {
      env[key] = value;
    }
  }
  // A throwaway home keeps logs (~/Library/Logs/Unfold) and fallback caches out of the real ones.
  env["HOME"] = sandbox.home;
  return env;
}

/** One running copy of the app. */
export class UnfoldApp {
  readonly electron: ElectronApplication;
  /** The main process's and engine host's output, for failure reports. */
  readonly logs: string[];
  private readonly sandbox: Sandbox;
  private closed = false;

  private constructor(electron: ElectronApplication, logs: string[], sandbox: Sandbox) {
    this.electron = electron;
    this.logs = logs;
    this.sandbox = sandbox;
  }

  static async launch(sandbox: Sandbox, options: LaunchOptions = {}): Promise<UnfoldApp> {
    const electron = await _electron.launch({
      executablePath: electronExecutable(),
      args: [desktopDir, `--user-data-dir=${sandbox.userData}`, ...(options.args ?? [])],
      env: appEnv(sandbox),
    });
    const logs: string[] = [];
    const proc = electron.process();
    proc.stdout?.on("data", (chunk: Buffer) => logs.push(chunk.toString()));
    proc.stderr?.on("data", (chunk: Buffer) => logs.push(chunk.toString()));
    electron.on("window", (page) => {
      page.on("pageerror", (error) => logs.push(`[pageerror] ${error.message}\n`));
      page.on("console", (message) => {
        if (message.type() === "error") {
          logs.push(`[console.error] ${message.text()}\n`);
        }
      });
    });
    return new UnfoldApp(electron, logs, sandbox);
  }

  get isClosed(): boolean {
    return this.closed;
  }

  /** The first window, ready to drive (sized, with the event recorder installed). */
  async firstWindow(): Promise<Page> {
    const page = await this.electron.firstWindow();
    await prepareWindow(this, page);
    return page;
  }

  /** Waits for the next window to open and prepares it. Call before the action that opens it. */
  async nextWindow(action: () => Promise<unknown>): Promise<Page> {
    const [page] = await Promise.all([this.electron.waitForEvent("window"), action()]);
    await prepareWindow(this, page);
    return page;
  }

  /** Every open window's page. */
  windows(): Page[] {
    return this.electron.windows();
  }

  /** The Electron window id of a page. */
  async windowId(page: Page): Promise<number> {
    const handle = await this.electron.browserWindow(page);
    return handle.evaluate((window: Electron.BrowserWindow) => window.id);
  }

  async resize(page: Page, width: number, height: number): Promise<void> {
    const handle = await this.electron.browserWindow(page);
    await handle.evaluate((window: Electron.BrowserWindow, size: { width: number; height: number }) => {
      window.setSize(size.width, size.height);
    }, { width, height });
  }

  /** The folder picker answers `folder` from now on. */
  async stubFolderDialog(folder: string): Promise<void> {
    await this.electron.evaluate(({ dialog }, chosen) => {
      dialog.showOpenDialog = () => Promise.resolve({ canceled: false, filePaths: [chosen] });
    }, folder);
  }

  /** The save dialog answers `file` from now on. */
  async stubSaveDialog(file: string): Promise<void> {
    await this.electron.evaluate(({ dialog }, chosen) => {
      dialog.showSaveDialog = () => Promise.resolve({ canceled: false, filePath: chosen });
    }, file);
  }

  /**
   * Message boxes (Remove from workspace's confirmation) answer with button `response` from now on,
   * and record the messages they showed (`messageBoxes`).
   */
  async stubMessageBox(response: number): Promise<void> {
    await this.electron.evaluate(({ dialog }, answer) => {
      const store = globalThis as unknown as { __messageBoxes?: string[] };
      const shown: string[] = [];
      store.__messageBoxes = shown;
      const stub = (...args: unknown[]): Promise<Electron.MessageBoxReturnValue> => {
        const options = args.at(-1) as Electron.MessageBoxOptions;
        shown.push(options.message);
        return Promise.resolve({ response: answer, checkboxChecked: false });
      };
      dialog.showMessageBox = stub;
    }, response);
  }

  /** The messages the stubbed message boxes showed, oldest first. */
  async messageBoxes(): Promise<string[]> {
    return this.electron.evaluate(() => (globalThis as unknown as { __messageBoxes?: string[] }).__messageBoxes ?? []);
  }

  /** What the clipboard holds (the main process writes it for the windows). */
  async clipboardText(): Promise<string> {
    return this.electron.evaluate(({ clipboard }) => clipboard.readText());
  }

  /**
   * Launches the app a second time with `args`, as opening a folder from the Finder or a terminal
   * does: the second copy hands its arguments to this one and exits.
   */
  async secondLaunch(args: readonly string[]): Promise<void> {
    const child = spawn(electronExecutable(), [desktopDir, `--user-data-dir=${this.sandbox.userData}`, ...args], {
      env: appEnv(this.sandbox),
      stdio: "ignore",
    });
    await new Promise<void>((resolve, reject) => {
      child.once("error", reject);
      child.once("exit", () => {
        resolve();
      });
    });
  }

  /** The engine host's process id (the utility process named "Unfold Engine"), or null. */
  async engineHostPid(): Promise<number | null> {
    return this.electron.evaluate(({ app }) => app.getAppMetrics().find((metric) => metric.type === "Utility" && metric.name === "Unfold Engine")?.pid ?? null);
  }

  /** Kills the engine host the hard way and returns its old pid. */
  async killEngineHost(): Promise<number> {
    const pid = await this.engineHostPid();
    if (pid === null) {
      throw new Error("the engine host isn't running");
    }
    process.kill(pid, "SIGKILL");
    return pid;
  }

  /** Every Unfold process with its working set in KB (main, renderers, GPU, utilities). */
  async processMemory(): Promise<{ readonly type: string; readonly name: string; readonly pid: number; readonly kb: number }[]> {
    return this.electron.evaluate(({ app }) =>
      app.getAppMetrics().map((metric) => ({ type: metric.type, name: metric.name ?? metric.serviceName ?? "", pid: metric.pid, kb: metric.memory.workingSetSize })),
    );
  }

  /** Quits like the person would (Unfold > Quit): windows are restored from this on relaunch. */
  async quit(): Promise<void> {
    if (this.closed) {
      return;
    }
    this.closed = true;
    await this.electron.close();
  }
}
