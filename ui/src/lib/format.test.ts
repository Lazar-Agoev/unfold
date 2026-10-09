import { describe, expect, it } from "vitest";
import { displayPath, formatCount, languageList, plural, relativeTime } from "./format";

const NOW = new Date(2026, 9, 9, 15, 0).getTime();
const HOUR = 3_600_000;

describe("formatCount and plural", () => {
  it("groups thousands and caps at a limit", () => {
    expect(formatCount(1284)).toBe("1,284");
    expect(formatCount(499, 500)).toBe("499");
    expect(formatCount(500, 500)).toBe("500+");
  });

  it("picks the singular only for one", () => {
    expect(plural(1, "file")).toBe("1 file");
    expect(plural(0, "file")).toBe("0 files");
    expect(plural(1058, "file")).toBe("1,058 files");
    expect(plural(2, "entrance")).toBe("2 entrances");
    expect(plural(3, "match", "matches")).toBe("3 matches");
  });
});

describe("languageList", () => {
  it("names the languages in order", () => {
    expect(languageList(["typescript", "python"])).toBe("TypeScript + Python");
    expect(languageList(["python"])).toBe("Python");
    expect(languageList([])).toBe("");
  });
});

describe("displayPath", () => {
  it("shows the home folder as ~ on every platform", () => {
    expect(displayPath("/Users/lazar/code/storefront-api")).toBe("~/code/storefront-api");
    expect(displayPath("/home/lazar/work/billing-worker")).toBe("~/work/billing-worker");
    expect(displayPath("C:\\Users\\lazar\\code\\api")).toBe("~\\code\\api");
    expect(displayPath("/Users/lazar")).toBe("~");
  });

  it("leaves other paths alone", () => {
    expect(displayPath("/srv/repos/api")).toBe("/srv/repos/api");
    expect(displayPath("/Users")).toBe("/Users");
  });
});

describe("relativeTime", () => {
  it("reads like a person would say it", () => {
    expect(relativeTime(NOW - 10_000, NOW)).toBe("just now");
    expect(relativeTime(NOW - 60_000, NOW)).toBe("a minute ago");
    expect(relativeTime(NOW - 5 * 60_000, NOW)).toBe("5 minutes ago");
    expect(relativeTime(NOW - 2 * HOUR, NOW)).toBe("2 hours ago");
    expect(relativeTime(NOW - 20 * HOUR, NOW)).toBe("yesterday");
    expect(relativeTime(NOW - 3 * 24 * HOUR, NOW)).toBe("3 days ago");
    expect(relativeTime(new Date(2026, 2, 4).getTime(), NOW)).toBe("Mar 4");
    expect(relativeTime(new Date(2024, 2, 4).getTime(), NOW)).toBe("Mar 4, 2024");
  });
});
