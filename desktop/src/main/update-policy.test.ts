import { describe, expect, it } from "vitest";
import type { UpdateEnvironment } from "./update-policy";
import { pinsPublisher, RELEASES_URL, releasePageUrl, updateMode, updatesEnabled } from "./update-policy";

const RELEASE: UpdateEnvironment = {
  isPackaged: true,
  platform: "darwin",
  releaseBuild: true,
  appImage: undefined,
  packageType: null,
  publisherPinned: false,
};

describe("update policy", () => {
  it("is off in development and in builds the release workflow didn't make", () => {
    expect(updateMode({ ...RELEASE, isPackaged: false })).toEqual({ kind: "off", reason: "development" });
    expect(updateMode({ ...RELEASE, releaseBuild: false })).toEqual({ kind: "off", reason: "not_a_release_build" });
  });

  it("installs on macOS, and on Windows only when the publisher is pinned", () => {
    expect(updateMode(RELEASE)).toEqual({ kind: "install" });
    expect(updateMode({ ...RELEASE, platform: "win32", publisherPinned: true })).toEqual({ kind: "install" });
    expect(updateMode({ ...RELEASE, platform: "win32" })).toEqual({ kind: "off", reason: "unsigned" });
  });

  it("lets the AppImage update itself and only notifies .deb and .rpm installs", () => {
    const linux = { ...RELEASE, platform: "linux" } as const;
    expect(updateMode({ ...linux, appImage: "/home/a/Unfold.AppImage" })).toEqual({ kind: "install" });
    expect(updateMode({ ...linux, packageType: "deb" })).toEqual({ kind: "notify", packageType: "deb" });
    expect(updateMode({ ...linux, packageType: "rpm" })).toEqual({ kind: "notify", packageType: "rpm" });
    expect(updateMode({ ...linux, packageType: "pacman" })).toEqual({ kind: "off", reason: "unsupported_package" });
    expect(updateMode(linux)).toEqual({ kind: "off", reason: "unsupported_package" });
  });

  it("reads updates.enabled, and nothing else counts", () => {
    expect(updatesEnabled({ updates: { enabled: true } })).toBe(true);
    expect(updatesEnabled({ updates: { enabled: false } })).toBe(false);
    expect(updatesEnabled({ updates: {} })).toBeNull();
    expect(updatesEnabled({ theme: "dark" })).toBeNull();
    expect(updatesEnabled(null)).toBeNull();
  });

  it("links a release page only for a well-formed version", () => {
    expect(releasePageUrl("1.2.3")).toBe(`${RELEASES_URL}/tag/v1.2.3`);
    expect(releasePageUrl("1.2.3-beta.1")).toBe(`${RELEASES_URL}/tag/v1.2.3-beta.1`);
    expect(releasePageUrl("1.2.3/../../evil")).toBe(`${RELEASES_URL}/latest`);
  });

  it("sees whether app-update.yml pins a Windows publisher", () => {
    expect(pinsPublisher("owner: o\npublisherName:\n  - Example Ltd\n")).toBe(true);
    expect(pinsPublisher("publisherName: Example Ltd\n")).toBe(true);
    expect(pinsPublisher("owner: o\nrepo: r\nprovider: github\n")).toBe(false);
  });
});
