/**
 * The main process: the app's lifecycle, windows, menus, dialogs, and the single-instance lock. It
 * does no graph work and never waits on the engine (architecture §4.1).
 */

import { statSync } from "node:fs";
import { join } from "node:path";
import { app, Menu } from "electron";
import { addonPath, appFolders, folderArgument, uiDirectory } from "./app-paths";
import type { UiSource } from "./app-url";
import { chooseFolder } from "./dialogs";
import { EngineHost } from "./engine-host";
import { Folders } from "./folders";
import { registerWindowCommands } from "./ipc";
import { buildMenu } from "./menu";
import { registerAppScheme, serveUi } from "./protocol";
import { hardenApp } from "./security";
import { updateMode, updatesEnabled } from "./update-policy";
import { currentUpdateEnvironment, startUpdater } from "./updater";
import { WindowRegistry } from "./windows";

app.setName("Unfold");
registerAppScheme();

const isMac = process.platform === "darwin";
const devServerUrl = app.isPackaged ? undefined : process.env["UNFOLD_UI_DEV_URL"];
const ui: UiSource = devServerUrl
  ? { kind: "dev", url: devServerUrl }
  : { kind: "app", dir: uiDirectory(app.getAppPath(), app.isPackaged) };

function isDirectory(path: string): boolean {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

/** When run unpackaged (`electron <app>`), argv carries the app path before any folder. */
const hasAppPathArgument = process.defaultApp;

if (!app.requestSingleInstanceLock()) {
  // Another copy is running; it receives our argv through `second-instance`.
  app.quit();
} else {
  // Folders the OS asks us to open before the app is ready (macOS `open-file`, e.g. a dock drop).
  const pendingFolders: string[] = [];
  let openFolderRequest = (folder: string): void => {
    pendingFolders.push(folder);
  };

  app.on("open-file", (event, path) => {
    event.preventDefault();
    if (isDirectory(path)) {
      openFolderRequest(path);
    }
  });

  void app.whenReady().then(() => {
    hardenApp(ui);
    serveUi(ui);

    const devTools = !app.isPackaged;
    const engine = new EngineHost({
      modulePath: join(__dirname, "engine-host.js"),
      addonPath: addonPath(app.getAppPath(), process.resourcesPath, app.isPackaged),
      folders: appFolders(
        isMac ? "darwin" : process.platform === "win32" ? "win32" : "linux",
        { userData: app.getPath("userData"), home: app.getPath("home") },
        process.env,
      ),
      windows: () => registry.all(),
      onAttached: (windowId, folder, error) => {
        const entry = registry.get(windowId);
        if (entry && error === null && entry.folder === folder) {
          // Remember every window as soon as it shows its folder, before it is ever moved.
          folders.savePlacement(entry);
        }
      },
    });
    const registry = new WindowRegistry(
      ui,
      {
        onLoaded: (entry) => {
          engine.attach(entry);
        },
        onFolderChanged: (entry) => {
          engine.attach(entry);
        },
        onClosed: (entry) => {
          // Forget the window first (it must still be attached), then detach it.
          void folders.windowClosed(entry).finally(() => {
            engine.detach(entry);
          });
        },
        onPlacementChanged: (entry) => {
          folders.savePlacement(entry);
        },
      },
      devTools,
    );
    const folders = new Folders(registry, engine);
    engine.start();
    startUpdater(updateMode(currentUpdateEnvironment()), {
      enabled: () => engine.request(0, "get_preferences", {}).then(updatesEnabled, () => null),
      parentWindow: () => registry.focused()?.window ?? null,
    });

    const logOpenFailure = (folder: string) => (error: unknown) => {
      console.error(`[main] opening ${folder} failed:`, error);
    };

    /** Focuses a window already showing `folder`, or brings back its windows. */
    openFolderRequest = (folder: string) => {
      folders.open(folder).catch(logOpenFailure(folder));
    };

    const openFolderFromMenu = async (inNewWindow: boolean): Promise<void> => {
      const focused = registry.focused();
      const folder = await chooseFolder(focused?.window ?? null);
      if (folder !== null) {
        await folders.open(folder, inNewWindow ? undefined : focused);
      }
    };
    const openWelcome = (): void => {
      folders.openWelcome().catch((error: unknown) => {
        console.error("[main] opening a window failed:", error);
      });
    };

    registerWindowCommands({ registry, folders, engine, ui });
    Menu.setApplicationMenu(
      buildMenu(
        {
          openFolder: (window) => {
            // The window opens the picker itself, so it can save its boards before its folder is
            // replaced; with no window there is nothing to save.
            const target = (window ? registry.forWindow(window) : undefined) ?? registry.focused();
            if (target?.loaded) {
              registry.send(target, { type: "menu_command", command: "open_folder" });
            } else {
              void openFolderFromMenu(false).catch(logOpenFailure("a folder"));
            }
          },
          openFolderInNewWindow: () => void openFolderFromMenu(true).catch(logOpenFailure("a folder")),
          newWindow: () => registry.create(registry.focused()?.folder ?? null),
          forward: (command, window) => {
            const target = (window ? registry.forWindow(window) : undefined) ?? registry.focused();
            if (target) {
              registry.send(target, { type: "menu_command", command });
            }
          },
        },
        { isMac, devTools },
      ),
    );

    app.on("second-instance", (_event, argv, workingDirectory) => {
      const folder = folderArgument(argv, hasAppPathArgument, workingDirectory, isDirectory);
      if (folder) {
        openFolderRequest(folder);
        return;
      }
      const [first] = registry.all();
      if (first) {
        registry.focus(first);
      } else {
        openWelcome();
      }
    });

    app.on("activate", () => {
      if (registry.all().length === 0) {
        openWelcome();
      }
    });

    app.on("before-quit", () => {
      folders.setQuitting();
    });

    let shutdownDone = false;
    app.on("will-quit", (event) => {
      if (shutdownDone) {
        return;
      }
      // Let the engine host close every workspace (saving drafts) before the app exits.
      event.preventDefault();
      void engine.shutdown().finally(() => {
        shutdownDone = true;
        app.quit();
      });
    });

    const initial = [
      ...pendingFolders.splice(0),
      ...[folderArgument(process.argv, hasAppPathArgument, process.cwd(), isDirectory)].filter((f) => f !== null),
    ];
    if (initial.length === 0) {
      openWelcome();
    }
    for (const folder of initial) {
      openFolderRequest(folder);
    }
  });

  app.on("window-all-closed", () => {
    // macOS apps keep running with no windows; Windows and Linux quit with the last one.
    if (!isMac) {
      app.quit();
    }
  });
}
