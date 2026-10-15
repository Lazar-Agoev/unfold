/**
 * Where things live: the app folders of docs/architecture.md#storage, the built UI, and the engine addon.
 * Pure functions of their inputs, so they are tested without Electron.
 */

import { join, resolve } from "node:path";
import type { AppFolders } from "../shared/transport";

/**
 * The app folder Rust receives in `addon.init` (docs/architecture.md#storage): app data (settings, the
 * workspace, the window's state) is Electron's user-data folder, `app.getPath("userData")`
 * (`~/Library/Application Support/Unfold`).
 */
export function appFolders(userData: string): AppFolders {
  return { appDataDir: userData };
}

/** The built UI: inside the app archive when packaged, `ui/dist` beside `desktop/` otherwise. */
export function uiDirectory(appPath: string, isPackaged: boolean): string {
  return isPackaged ? join(appPath, "ui") : resolve(appPath, "..", "ui", "dist");
}

/**
 * The engine addon module: unpacked beside the app archive when packaged (native modules can't load
 * from inside it), the napi-rs crate directory in a development checkout.
 */
export function addonPath(appPath: string, resourcesPath: string, isPackaged: boolean): string {
  return isPackaged ? join(resourcesPath, "unfold-node") : resolve(appPath, "..", "crates", "unfold-node");
}
