import { describe, expect, it } from "vitest";
import { isRecentFolder, openFolderTarget } from "./recent";

const recent = {
  folders: [
    { path: "/Users/someone/code/orders", name: "orders", openedAtMs: 1 },
    { path: "/private/var/repos/shop", name: "shop", openedAtMs: 2 },
  ],
  welcomeBounds: null,
};
// A stand-in for realpath: `/var` is a symlink to `/private/var`, as on macOS.
const canonical = (path: string) => path.replace(/^\/var\//, "/private/var/").replace(/\/+$/, "");

describe("isRecentFolder", () => {
  it("accepts listed folders, comparing canonical paths", () => {
    expect(isRecentFolder(recent, "/Users/someone/code/orders", canonical)).toBe(true);
    expect(isRecentFolder(recent, "/Users/someone/code/orders/", canonical)).toBe(true);
    expect(isRecentFolder(recent, "/var/repos/shop", canonical)).toBe(true);
  });

  it("refuses anything else", () => {
    expect(isRecentFolder(recent, "/Users/someone/code", canonical)).toBe(false);
    expect(isRecentFolder(recent, "/etc", canonical)).toBe(false);
    expect(isRecentFolder({ folders: "nope" }, "/etc", canonical)).toBe(false);
    expect(isRecentFolder(null, "/etc", canonical)).toBe(false);
    expect(isRecentFolder({ folders: [{ path: 3 }, null] }, "/etc", canonical)).toBe(false);
  });
});

describe("openFolderTarget", () => {
  it("reads target or newWindow", () => {
    expect(openFolderTarget({})).toBe("this_window");
    expect(openFolderTarget({ target: "new_window" })).toBe("new_window");
    expect(openFolderTarget({ newWindow: true })).toBe("new_window");
    expect(openFolderTarget({ newWindow: false })).toBe("this_window");
    expect(openFolderTarget({ target: "new_window", newWindow: true })).toBe("new_window");
  });

  it("refuses malformed or contradictory targets", () => {
    expect(openFolderTarget({ target: "elsewhere" })).toBeNull();
    expect(openFolderTarget({ newWindow: "yes" })).toBeNull();
    expect(openFolderTarget({ target: "this_window", newWindow: true })).toBeNull();
  });
});
