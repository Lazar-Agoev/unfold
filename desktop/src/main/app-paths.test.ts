import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { addonPath, appFolders, uiDirectory } from "./app-paths";

describe("appFolders", () => {
  it("follows architecture §8.2", () => {
    expect(appFolders({ userData: "/Users/me/Library/Application Support/Unfold", home: "/Users/me" })).toEqual({
      appDataDir: "/Users/me/Library/Application Support/Unfold",
      logsDir: join("/Users/me", "Library", "Logs", "Unfold"),
    });
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
