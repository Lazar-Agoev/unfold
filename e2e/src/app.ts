/**
 * Launching the built, unpackaged app with a sandbox's user data and home, and driving what only
 * the main process can do: stubbing the folder picker, the application menu, finding and killing the
 * engine host, quitting.
 */

import type { ElectronApplication, Page } from "@playwright/test";
import { _electron } from "@playwright/test";
import { desktopDir, electronExecutable } from "./paths";
import type { Sandbox } from "./sandbox";
import { prepareWindow } from "./window";

/** The environment the app runs in: this process's, with a throwaway home. */
function appEnv(sandbox: Sandbox): Record<string, string> {
  const env: Record<string, string> = {};
  for (const [key, value] of Object.entries(process.env)) {
    if (value !== undefined && key !== "ELECTRON_RUN_AS_NODE" && key !== "NODE_OPTIONS") {
      env[key] = value;
    }
  }
  // A throwaway home keeps logs (~/Library/Logs/Unfold) out of the real ones.
  env["HOME"] = sandbox.home;
  return env;
}

/** One running copy of the app. */
export class UnfoldApp {
  readonly electron: ElectronApplication;
  /** The main process's and engine host's output, for failure reports. */
  readonly logs: string[];
  private closed = false;

  private constructor(electron: ElectronApplication, logs: string[]) {
    this.electron = electron;
    this.logs = logs;
  }

  static async launch(sandbox: Sandbox): Promise<UnfoldApp> {
    const electron = await _electron.launch({
      executablePath: electronExecutable(),
      // A window another window covers, or on another Space, stops drawing frames; journeys wait for frames.
      args: ["--disable-renderer-backgrounding", "--disable-backgrounding-occluded-windows", desktopDir, `--user-data-dir=${sandbox.userData}`],
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
    return new UnfoldApp(electron, logs);
  }

  get isClosed(): boolean {
    return this.closed;
  }

  /** The app's window, ready to drive (sized). */
  async firstWindow(): Promise<Page> {
    const page = await this.electron.firstWindow();
    await prepareWindow(this, page);
    return page;
  }

  /** Every open window's page (the app's window, and the render window while it draws a page). */
  windows(): Page[] {
    return this.electron.windows();
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

  /**
   * Clicks an item of the application menu (`["Edit", "Undo"]`) for the window of `page`, as choosing
   * it with the mouse does: Electron passes that window to the item. Key presses never reach the
   * native menu, so the journeys undo and redo through it.
   */
  async clickMenu(page: Page, path: readonly string[]): Promise<void> {
    const handle = await this.electron.browserWindow(page);
    const windowId = await handle.evaluate((window: Electron.BrowserWindow) => window.id);
    await this.electron.evaluate(({ BrowserWindow, Menu }, { labels, id }) => {
      let items = Menu.getApplicationMenu()?.items ?? [];
      let item: Electron.MenuItem | undefined;
      for (const label of labels) {
        item = items.find((candidate) => candidate.label === label);
        if (!item) {
          throw new Error(`no menu item ${label}`);
        }
        items = item.submenu?.items ?? [];
      }
      const window = BrowserWindow.fromId(id);
      if (!item || !window) {
        throw new Error("nothing to click");
      }
      // Typed as a bare `Function`: Electron calls it with the event, the window, and its contents.
      const click = item.click as (event: unknown, window: Electron.BrowserWindow, contents: Electron.WebContents) => void;
      click.call(item, undefined, window, window.webContents);
    }, { labels: [...path], id: windowId });
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

  /** Quits like the person would (Unfold > Quit): the window is restored from this on relaunch. */
  async quit(): Promise<void> {
    if (this.closed) {
      return;
    }
    this.closed = true;
    await this.electron.close();
  }
}
