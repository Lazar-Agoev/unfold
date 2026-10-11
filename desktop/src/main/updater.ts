/**
 * Updates (docs/architecture.md#security): electron-updater against GitHub Releases, the app's only network
 * use. Every download must match the SHA-512 hash in the release's manifest (electron-updater checks
 * it); on macOS an update installs only if it carries the running app's code signature (Squirrel.Mac
 * checks the designated requirement), and on Windows only if it is Authenticode-signed by a
 * publisher named in `app-update.yml`. The AppImage replaces itself; `.deb` and `.rpm` installs only
 * learn that a version exists and show a notice linking to it. The person can turn the check off
 * (`updates.enabled` in the preferences, read through the engine before every check).
 *
 * Updates are off in development and in every build not made by the release workflow, which signs
 * on macOS and Windows and marks the build with `unfoldReleaseBuild` in its `package.json`.
 */

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { BrowserWindow, MessageBoxOptions } from "electron";
import { app, dialog, shell } from "electron";
import type { UpdateEnvironment, UpdateMode } from "./update-policy";
import { pinsPublisher, releasePageUrl } from "./update-policy";

const FIRST_CHECK_DELAY_MS = 30_000;
const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

function readText(path: string): string | null {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return null;
  }
}

/** Reads this app's {@link UpdateEnvironment}. */
export function currentUpdateEnvironment(): UpdateEnvironment {
  const manifest = readText(join(app.getAppPath(), "package.json"));
  let releaseBuild = false;
  if (manifest !== null) {
    try {
      // The release workflow sets it on electron-builder's command line, which may make it a string.
      const flag = (JSON.parse(manifest) as Record<string, unknown>)["unfoldReleaseBuild"];
      releaseBuild = flag === true || flag === "true";
    } catch {
      releaseBuild = false;
    }
  }
  const packageTypePath = join(process.resourcesPath, "package-type");
  return {
    isPackaged: app.isPackaged,
    platform: process.platform,
    releaseBuild,
    appImage: process.env["APPIMAGE"],
    packageType: existsSync(packageTypePath) ? (readText(packageTypePath)?.trim() ?? null) : null,
    publisherPinned: pinsPublisher(readText(join(process.resourcesPath, "app-update.yml")) ?? ""),
  };
}

export interface UpdaterOptions {
  /** The `updates.enabled` preference, or null when it can't be read (then no check runs). */
  readonly enabled: () => Promise<boolean | null>;
  /** The window a prompt belongs to, if any. */
  readonly parentWindow: () => BrowserWindow | null;
}

async function showMessage(parent: BrowserWindow | null, options: MessageBoxOptions): Promise<number> {
  const result = parent ? await dialog.showMessageBox(parent, options) : await dialog.showMessageBox(options);
  return result.response;
}

/**
 * Starts checking for updates in `mode`: half a minute after launch, then every six hours, each
 * time only if the preference allows it. Failures (offline, no release yet) are logged, never shown.
 */
export function startUpdater(mode: UpdateMode, options: UpdaterOptions): void {
  if (mode.kind === "off") {
    return;
  }
  // Loaded only here, so development and unreleased builds never construct an updater.
  void import("electron-updater").then(({ autoUpdater }) => {
    autoUpdater.logger = {
      info: () => undefined,
      debug: () => undefined,
      warn: (message: unknown) => {
        console.warn("[updater]", message);
      },
      // Failures also arrive as the "error" event, logged below.
      error: () => undefined,
    };
    autoUpdater.autoDownload = mode.kind === "install";
    autoUpdater.autoInstallOnAppQuit = mode.kind === "install";
    autoUpdater.allowPrerelease = false;
    autoUpdater.allowDowngrade = false;

    const prompted = new Set<string>();
    autoUpdater.on("error", (error: Error) => {
      // Only the first line: HTTP failures append the response headers.
      console.error("[updater] the update check failed:", error.message.split("\n")[0]);
    });
    autoUpdater.on("update-downloaded", (info: { version: string }) => {
      if (prompted.has(info.version)) {
        return;
      }
      prompted.add(info.version);
      void showMessage(options.parentWindow(), {
        type: "info",
        buttons: ["Restart Now", "Later"],
        defaultId: 0,
        cancelId: 1,
        message: `Unfold ${info.version} is ready to install`,
        detail: "It installs when you quit Unfold. Restart now to use it straight away.",
      }).then((response) => {
        if (response === 0) {
          autoUpdater.quitAndInstall();
        }
      });
    });
    if (mode.kind === "notify") {
      autoUpdater.on("update-available", (info: { version: string }) => {
        if (prompted.has(info.version)) {
          return;
        }
        prompted.add(info.version);
        void showMessage(options.parentWindow(), {
          type: "info",
          buttons: ["Open Download Page", "Later"],
          defaultId: 0,
          cancelId: 1,
          message: `Unfold ${info.version} is available`,
          detail: `Download the new .${mode.packageType} package and install it with your package manager.`,
        }).then((response) => {
          if (response === 0) {
            void shell.openExternal(releasePageUrl(info.version));
          }
        });
      });
    }

    const check = async (): Promise<void> => {
      if ((await options.enabled()) !== true) {
        return;
      }
      // A failed check (offline, no release yet) rejects here and is logged by the "error" event.
      await autoUpdater.checkForUpdates().catch(() => undefined);
    };
    setTimeout(() => {
      void check();
      setInterval(() => void check(), CHECK_INTERVAL_MS);
    }, FIRST_CHECK_DELAY_MS);
  });
}
