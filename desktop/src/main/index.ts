/**
 * The main process: the app's lifecycle, windows, menus, dialogs, and the single-instance lock. It
 * does no graph work and never waits on the engine (architecture §4.1).
 */

import { statSync } from "node:fs";
import { join } from "node:path";
import { app, Menu } from "electron";
import { AgentBridge } from "./agents/bridge";
import { ViewTracker } from "./agents/views";
import { addonPath, appFolders, folderArgument, uiDirectory } from "./app-paths";
import type { UiSource } from "./app-url";
import { chooseFolder, confirmAgentAddProject } from "./dialogs";
import { EngineHost } from "./engine-host";
import { registerWindowCommands } from "./ipc";
import { agentSettings, McpService } from "./mcp/service";
import { buildMenu } from "./menu";
import { registerAppScheme, serveUi } from "./protocol";
import { hardenApp } from "./security";
import { updateMode, updatesEnabled } from "./update-policy";
import { currentUpdateEnvironment, startUpdater } from "./updater";
import { WindowRegistry } from "./windows";
import { Workspace } from "./workspace";

app.setName("Unfold");
registerAppScheme();

const isMac = process.platform === "darwin";
const devServerUrl = app.isPackaged ? undefined : process.env["UNFOLD_UI_DEV_URL"];
/** A `just dev` session: unpackaged, with the UI served by the Vite dev server. */
const isDevSession = devServerUrl !== undefined;
const ui: UiSource = devServerUrl
  ? { kind: "dev", url: devServerUrl }
  : { kind: "app", dir: uiDirectory(app.getAppPath(), app.isPackaged) };

if (isDevSession) {
  // A dev session keeps its own preferences, workspace, and single-instance lock (Electron
  // keys the lock on this folder), so it never hands its launch to an installed Unfold.
  app.setPath("userData", join(app.getPath("appData"), "Unfold Dev"));
}

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
  if (isDevSession) {
    // Otherwise `just dev` would just stop, with no hint that an earlier session's app is still up.
    process.stderr.write(
      "Unfold is already running (an earlier `just dev` session?). Quit it with ⌘Q or run `just dev-stop`, then try again.\n",
    );
  }
  app.quit();
} else {
  // Folders the OS asks us to add before the app is ready (macOS `open-file`, e.g. a dock drop).
  const pendingFolders: string[] = [];
  let addProjectRequest = (folder: string): void => {
    pendingFolders.push(folder);
  };

  app.on("open-file", (event, path) => {
    event.preventDefault();
    if (isDirectory(path)) {
      addProjectRequest(path);
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
    });
    const registry = new WindowRegistry(
      ui,
      {
        onLoaded: (entry) => {
          engine.attach(entry);
        },
        onClosed: (entry) => {
          agents.windowClosed(entry.id);
          // Forget the window first (its bounds save lands before), then detach it.
          void workspace.windowClosed(entry).finally(() => {
            engine.detach(entry);
          });
        },
        onPlacementChanged: (entry) => {
          workspace.savePlacement(entry);
        },
        onFocused: (entry) => {
          agents.views.focused(entry.id);
        },
      },
      devTools,
    );
    const workspace = new Workspace(registry, engine);
    const agents = new AgentBridge({ windows: registry, views: new ViewTracker() });
    const mcp = new McpService({
      services: {
        bridge: agents,
        engine: (command, args) => engine.untrusted(0, command, args),
        confirmAddProject: (agent, path) => {
          const last = agents.views.lastUsed();
          return confirmAgentAddProject((last === undefined ? undefined : registry.get(last))?.window ?? null, agent.name, path);
        },
        addProject: (path) => workspace.add(0, path),
      },
      version: app.getVersion(),
      onStatus: (status) => {
        for (const entry of registry.all()) {
          registry.send(entry, { type: "agent_server_changed", status });
        }
      },
    });
    engine.start();
    startUpdater(updateMode(currentUpdateEnvironment()), {
      enabled: () => engine.request(0, "get_preferences", {}).then(updatesEnabled, () => null),
      parentWindow: () => registry.focused()?.window ?? null,
    });
    // Preferences › Agents: as saved at launch, then as the person changes it.
    let preferencesChanged = false;
    engine.onEvent((event) => {
      if (event.type === "preferences_changed") {
        preferencesChanged = true;
        void mcp.configure(agentSettings(event["preferences"]));
      }
    });
    engine.request(0, "get_preferences", {}).then(
      (preferences) => {
        if (!preferencesChanged) {
          void mcp.configure(agentSettings(preferences));
        }
      },
      (error: unknown) => {
        console.error("[main] reading the preferences failed; agents can't connect:", error);
      },
    );

    addProjectRequest = (folder: string) => {
      void workspace.addProject(folder);
    };
    const restore = (): void => {
      workspace.restore().catch((error: unknown) => {
        console.error("[main] opening windows failed:", error);
      });
    };

    registerWindowCommands({ registry, workspace, engine, ui, agents, mcp });
    Menu.setApplicationMenu(
      buildMenu(
        {
          addProject: () => {
            chooseFolder(registry.focused()?.window ?? null)
              .then((folder) => {
                if (folder !== null) {
                  addProjectRequest(folder);
                }
              })
              .catch((error: unknown) => {
                console.error("[main] the folder picker failed:", error);
              });
          },
          newWindow: () => registry.create(),
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
        addProjectRequest(folder);
        return;
      }
      const [first] = registry.all();
      if (first) {
        registry.focus(first);
      } else {
        restore();
      }
    });

    app.on("activate", () => {
      if (registry.all().length === 0) {
        restore();
      }
    });

    app.on("before-quit", () => {
      workspace.setQuitting();
      void mcp.close();
    });

    // Electron's main process ignores these by default; quitting gracefully lets the engine host land
    // board writes first (Ctrl+C in a `just dev` terminal, `kill`, or a system shutdown).
    for (const signal of ["SIGINT", "SIGTERM"] as const) {
      process.on(signal, () => {
        app.quit();
      });
    }

    let shutdownDone = false;
    app.on("will-quit", (event) => {
      if (shutdownDone) {
        return;
      }
      // Let the engine host detach every window and close every live project before the app exits.
      event.preventDefault();
      void engine.shutdown().finally(() => {
        shutdownDone = true;
        app.quit();
      });
    });

    restore();
    const argvFolder = folderArgument(process.argv, hasAppPathArgument, process.cwd(), isDirectory);
    for (const folder of [...pendingFolders.splice(0), ...(argvFolder ? [argvFolder] : [])]) {
      addProjectRequest(folder);
    }
  });

  app.on("window-all-closed", () => {
    // macOS apps keep running with no windows; Windows and Linux quit with the last one. A dev
    // session always quits, so closing its last window ends `just dev` instead of leaving the app
    // running in the background.
    if (!isMac || isDevSession) {
      app.quit();
    }
  });
}
