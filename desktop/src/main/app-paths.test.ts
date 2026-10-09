import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { addonPath, appFolders, folderArgument, uiDirectory } from "./app-paths";

describe("appFolders", () => {
  it("follows architecture §8.2 on macOS", () => {
    expect(appFolders("darwin", { userData: "/Users/me/Library/Application Support/Unfold", home: "/Users/me" }, {})).toEqual({
      appDataDir: "/Users/me/Library/Application Support/Unfold",
      logsDir: join("/Users/me", "Library", "Logs", "Unfold"),
      fallbackCacheDir: join("/Users/me", "Library", "Caches", "Unfold", "folders"),
    });
  });

  it("uses XDG_CACHE_HOME on Linux only when it is absolute", () => {
    const base = { userData: "/home/me/.config/Unfold", home: "/home/me" };
    expect(appFolders("linux", base, {})).toEqual({
      appDataDir: "/home/me/.config/Unfold",
      logsDir: join("/home/me/.config/Unfold", "logs"),
      fallbackCacheDir: join("/home/me", ".cache", "Unfold", "folders"),
    });
    expect(appFolders("linux", base, { XDG_CACHE_HOME: "/var/cache/me" }).fallbackCacheDir).toBe(
      join("/var/cache/me", "Unfold", "folders"),
    );
    expect(appFolders("linux", base, { XDG_CACHE_HOME: "relative" }).fallbackCacheDir).toBe(
      join("/home/me", ".cache", "Unfold", "folders"),
    );
  });

  it("puts logs under app data and caches under LOCALAPPDATA on Windows", () => {
    const folders = appFolders("win32", { userData: "/appdata/Unfold", home: "/home" }, { LOCALAPPDATA: "/local" });
    expect(folders.logsDir).toBe(join("/appdata/Unfold", "logs"));
    expect(folders.fallbackCacheDir).toBe(join("/local", "Unfold", "folders"));
  });
});

describe("bundled locations", () => {
  it("finds the UI and the addon beside desktop/ in a checkout", () => {
    expect(uiDirectory("/repo/desktop", false)).toBe(resolve("/repo/ui/dist"));
    expect(addonPath("/repo/desktop", "/ignored", false)).toBe(resolve("/repo/crates/unfold-node"));
  });

  it("finds them inside the packaged app", () => {
    expect(uiDirectory("/App/Resources/app.asar", true)).toBe(join("/App/Resources/app.asar", "ui"));
    expect(addonPath("/App/Resources/app.asar", "/App/Resources", true)).toBe(join("/App/Resources", "unfold-node"));
  });
});

describe("folderArgument", () => {
  const dirs = new Set([resolve("/work/repo"), resolve("/abs/other")]);
  const isDirectory = (path: string) => dirs.has(path);

  it("takes the last directory argument, ignoring flags and the app path", () => {
    expect(folderArgument(["electron", "--inspect=0", "/work/repo", "repo"], true, "/work", isDirectory)).toBe(
      resolve("/work/repo"),
    );
    expect(folderArgument(["Unfold", "/abs/other", "missing"], false, "/work", isDirectory)).toBe(resolve("/abs/other"));
  });

  it("never mistakes the app path for a folder", () => {
    expect(folderArgument(["electron", "--remote-debugging-port=0", "/work/repo"], true, "/", isDirectory)).toBeNull();
  });

  it("returns null when no argument is a directory", () => {
    expect(folderArgument(["Unfold", "--flag", "file.txt"], false, "/work", isDirectory)).toBeNull();
  });
});
