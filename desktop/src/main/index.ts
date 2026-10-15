/**
 * The main process: the app's lifecycle, its one window, the menu, dialogs, and the single-instance
 * lock. It does no graph work and never waits on the engine (docs/architecture.md#processes).
 */

import { join } from "node:path";
import { app, Menu } from "electron";
import { AgentBridge } from "./agents/bridge";
import { PageRenderer } from "./agents/render";
import { ViewTracker } from "./agents/views";
import { addonPath, appFolders, uiDirectory } from "./app-paths";
import type { UiSource } from "./app-url";
import { chooseFolder, confirmAgentAddProject } from "./dialogs";
import { EngineHost } from "./engine-host";
import { registerWindowCommands } from "./ipc";
import { McpService } from "./mcp/service";
import { buildMenu } from "./menu";
import { registerAppScheme, serveUi } from "./protocol";
import { hardenApp } from "./security";
import { AppWindow } from "./windows";
import { Workspace } from "./workspace";

app.setName("Unfold");
registerAppScheme();

const devServerUrl = app.isPackaged ? undefined : process.env["UNFOLD_UI_DEV_URL"];
/** A `just dev` session: unpackaged, with the UI served by the Vite dev server. */
const isDevSession = devServerUrl !== undefined;
const ui: UiSource = devServerUrl
  ? { kind: "dev", url: devServerUrl }
  : { kind: "app", dir: uiDirectory(app.getAppPath(), app.isPackaged) };

if (isDevSession) {
  // A dev session keeps its own settings, workspace, and single-instance lock (Electron keys the
  // lock on this folder), so it never hands its launch to an installed Unfold.
  app.setPath("userData", join(app.getPath("appData"), "Unfold Dev"));
}

if (!app.requestSingleInstanceLock()) {
  // Another copy is running; it brings its window to the front (`second-instance`).
  if (isDevSession) {
    // Otherwise `just dev` would just stop, with no hint that an earlier session's app is still up.
    process.stderr.write(
      "Unfold is already running (an earlier `just dev` session?). Quit it with ⌘Q or run `just dev-stop`, then try again.\n",
    );
  }
  app.quit();
} else {
  void app.whenReady().then(() => {
    hardenApp(ui);
    serveUi(ui);

    const devTools = !app.isPackaged;
    const engine = new EngineHost({
      modulePath: join(__dirname, "engine-host.js"),
      addonPath: addonPath(app.getAppPath(), process.resourcesPath, app.isPackaged),
      folders: appFolders(app.getPath("userData")),
      windows: () => {
        const entry = appWindow.current();
        return [...(entry ? [entry] : []), ...renderer.entries()];
      },
    });
    const appWindow = new AppWindow(
      ui,
      {
        onLoaded: (entry) => {
          engine.attach(entry);
        },
        onPlacementChanged: () => {
          workspace.placementChanged();
        },
        onClosing: () => {
          workspace.windowClosing();
        },
        onClosed: (entry) => {
          agents.windowClosed(entry.id);
          // Its last save lands first, then the engine releases its board sessions.
          void workspace.windowClosed().finally(() => {
            engine.detach(entry);
          });
        },
      },
      devTools,
    );
    const workspace = new Workspace(appWindow, engine);
    const agents = new AgentBridge({ window: appWindow, openWindow: () => workspace.open(), views: new ViewTracker() });
    const renderer: PageRenderer = new PageRenderer({ ui, bridge: agents, engine, reserveId: () => appWindow.reserveId(), showAllCalls: () => workspace.showAllCalls() });
    const mcp = new McpService({
      services: {
        bridge: agents,
        renderer,
        engine: (command, args) => engine.untrusted(0, command, args),
        engineAs: (windowId, command, args) => engine.untrusted(windowId, command, args),
        confirmAddProject: (agent, path) => confirmAgentAddProject(appWindow.current()?.window ?? null, agent.name, path),
        addProject: (path) => workspace.add(0, path),
      },
      version: app.getVersion(),
      onStatus: (status) => {
        const entry = appWindow.current();
        if (entry) {
          appWindow.send(entry, { type: "agent_server_changed", status });
        }
      },
    });
    engine.start();
    void mcp.start();

    registerWindowCommands({ window: appWindow, workspace, ui, agents, mcp, renderer });
    Menu.setApplicationMenu(
      buildMenu(
        {
          addProject: () => {
            chooseFolder(appWindow.current()?.window ?? null)
              .then((folder) => {
                if (folder !== null) {
                  void workspace.addProject(folder);
                }
              })
              .catch((error: unknown) => {
                console.error("[main] the folder picker failed:", error);
              });
          },
          forward: (command, window) => {
            const entry = appWindow.forWindow(window);
            if (entry) {
              appWindow.send(entry, { type: "menu_command", command });
            }
          },
        },
        { devTools },
      ),
    );

    const show = (): void => {
      workspace.show().catch((error: unknown) => {
        console.error("[main] opening the window failed:", error);
      });
    };
    app.on("second-instance", show);
    app.on("activate", show);

    app.on("before-quit", () => {
      renderer.close();
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
      // Let the engine host detach the window and close every live project before the app exits.
      event.preventDefault();
      void engine.shutdown().finally(() => {
        shutdownDone = true;
        app.quit();
      });
    });

    workspace.restore().catch((error: unknown) => {
      console.error("[main] opening the window failed:", error);
    });
  });

  app.on("window-all-closed", () => {
    // The app keeps running with its window closed, as macOS apps do. A dev session quits, so
    // closing its window ends `just dev` instead of leaving the app running in the background.
    if (isDevSession) {
      app.quit();
    }
  });
}
