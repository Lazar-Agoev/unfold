/**
 * Where things live: the app folders of architecture §8.2, the built UI, and the engine addon.
 * Pure functions of their inputs, so they are tested without Electron.
 */

import { isAbsolute, join, resolve } from "node:path";
import type { AppFolders } from "../shared/transport";

export type Platform = "darwin" | "win32" | "linux";

/** Electron's own paths that the app folders derive from. */
export interface BasePaths {
  /** `app.getPath("userData")`: `~/Library/Application Support/Unfold`, `%APPDATA%\Unfold`, `~/.config/Unfold`. */
  readonly userData: string;
  readonly home: string;
}

/**
 * The app folders Rust receives in `addon.init` (architecture §8.2):
 * - app data (preferences, the workspace, remembered windows) is Electron's user-data folder;
 * - logs are `~/Library/Logs/Unfold` on macOS and `<user data>/logs` elsewhere;
 * - fallback caches for folders Unfold can't write to sit under the platform's cache folder.
 */
export function appFolders(
  platform: Platform,
  base: BasePaths,
  env: Readonly<Record<string, string | undefined>>,
): AppFolders {
  switch (platform) {
    case "darwin":
      return {
        appDataDir: base.userData,
        logsDir: join(base.home, "Library", "Logs", "Unfold"),
        fallbackCacheDir: join(base.home, "Library", "Caches", "Unfold", "folders"),
      };
    case "win32":
      return {
        appDataDir: base.userData,
        logsDir: join(base.userData, "logs"),
        fallbackCacheDir: join(absoluteOr(env["LOCALAPPDATA"], join(base.home, "AppData", "Local")), "Unfold", "folders"),
      };
    case "linux":
      return {
        appDataDir: base.userData,
        logsDir: join(base.userData, "logs"),
        fallbackCacheDir: join(absoluteOr(env["XDG_CACHE_HOME"], join(base.home, ".cache")), "Unfold", "folders"),
      };
  }
}

/** Environment paths must be absolute to count (the XDG spec says to ignore relative ones). */
function absoluteOr(value: string | undefined, fallback: string): string {
  return value && isAbsolute(value) ? value : fallback;
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

/**
 * The folder named on a command line, if any: the last non-flag argument that is a directory.
 * `argv` is a process's full argv. When Electron runs an unpackaged app (`electron <app> ...`), the
 * first non-flag argument after the executable is the app path, not a folder to add.
 */
export function folderArgument(
  argv: readonly string[],
  hasAppPathArgument: boolean,
  cwd: string,
  isDirectory: (path: string) => boolean,
): string | null {
  const positional = argv.slice(1).filter((arg) => arg.length > 0 && !arg.startsWith("-"));
  const candidates = hasAppPathArgument ? positional.slice(1) : positional;
  for (const arg of candidates.reverse()) {
    const path = resolve(cwd, arg);
    if (isDirectory(path)) {
      return path;
    }
  }
  return null;
}
