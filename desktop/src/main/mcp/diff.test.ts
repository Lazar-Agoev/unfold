import { describe, expect, it } from "vitest";
import { lineDiff } from "./diff";

describe("lineDiff", () => {
  it("shows added and removed lines with a line of context, and … between hunks", () => {
    const before = "[scope]\nexclude = [\"dist/**\"]\n\n[python]\nversion = \"3.11\"\n\n[project]\nname = \"shop\"\n";
    const after = "[scope]\nexclude = [\"dist/**\", \"tmp\"]\n\n[python]\nversion = \"3.11\"\n\n[project]\nname = \"store\"\n";
    expect(lineDiff(before, after)).toEqual([
      " [scope]",
      '-exclude = ["dist/**"]',
      '+exclude = ["dist/**", "tmp"]',
      " ",
      "…",
      " [project]",
      '-name = "shop"',
      '+name = "store"',
    ]);
  });

  it("diffs a file that didn't exist, and nothing for the same text", () => {
    expect(lineDiff("", "[detectors]\ndisabled = [\"node-cron.schedule\"]\n")).toEqual(["+[detectors]", '+disabled = ["node-cron.schedule"]']);
    expect(lineDiff("a\nb\n", "a\nb\n")).toEqual([]);
    expect(lineDiff("a\r\nb", "a\nb\n")).toEqual([]);
  });
});
