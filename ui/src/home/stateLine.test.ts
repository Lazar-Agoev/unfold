import { describe, expect, it } from "vitest";
import type { ProjectState, ProjectSummary } from "../ipc";
import { compilingDetail, stateLine, type StateLineModel } from "./stateLine";

const NOW = Date.UTC(2026, 9, 9, 12, 0, 0);

function summary(overrides: Partial<ProjectSummary> = {}, health: Partial<ProjectSummary["health"]> = {}): ProjectSummary {
  return {
    project: "p-1",
    name: "shop",
    path: "/Users/x/code/shop",
    state: { kind: "current" },
    status: { state: { kind: "current", files: 214, languages: ["python"] }, watching: "events", complete: true },
    updatedAtMs: NOW - 2_000,
    files: 214,
    languages: ["python"],
    health: { doesntParse: 0, boardsNeedingReview: 0, boards: [], ...health },
    ...overrides,
  };
}

function text(line: StateLineModel): string {
  return [line.headline, line.detail, line.action ? `[${line.action.label}]` : ""].filter((part) => part !== "").join(" ");
}

function lineFor(state: ProjectState, extra: { summary?: ProjectSummary; lastGood?: boolean } = {}): StateLineModel {
  return stateLine({
    state,
    summary: extra.summary ?? summary(),
    progress: null,
    ...(extra.lastGood === undefined ? {} : { lastGood: extra.lastGood }),
    nowMs: NOW,
  });
}

describe("Home's state line", () => {
  it("reads like the frame for a current graph", () => {
    expect(text(lineFor({ kind: "current" }))).toBe("Graph current · 214 files · Python · updated just now");
    const mixed = summary({ files: 1284, languages: ["typescript", "python"], updatedAtMs: NOW - 2 * 60_000 });
    expect(text(lineFor({ kind: "current" }, { summary: mixed }))).toBe("Graph current · 1,284 files · TypeScript + Python · updated 2 minutes ago");
  });

  it("words every other state", () => {
    expect(text(lineFor({ kind: "compiling", done: 412, total: 1058 }))).toBe("Compiling · 412 of 1,058 files [Cancel]");
    expect(text(lineFor({ kind: "updating", files: 3 }))).toBe("Updating · 3 files changed");
    expect(text(lineFor({ kind: "updating", files: null }))).toBe("Updating · checking the folder");
    expect(text(lineFor({ kind: "not_compiled" }))).toBe("Not compiled yet");
    expect(text(lineFor({ kind: "failed", message: "disk full" }))).toBe("Compile failed · the previous graph stays · [See Health]");
    expect(text(lineFor({ kind: "resting", last: { kind: "current" }, atMs: NOW - 2 * 3_600_000 }))).toBe("Resting · last checked 2 hours ago");
    expect(text(lineFor({ kind: "unavailable", reason: "The folder is gone." }))).toBe("Unavailable · The folder is gone.");
  });

  it("names the problem from the actual counts", () => {
    const parse = summary({}, { doesntParse: 1 });
    expect(text(lineFor({ kind: "problems", count: 1 }, { summary: parse, lastGood: true }))).toBe("1 file doesn't parse · showing its last version that parsed [See Health]");
    const two = summary({}, { doesntParse: 2 });
    expect(text(lineFor({ kind: "problems", count: 2 }, { summary: two, lastGood: true }))).toBe("2 files don't parse · showing their last versions that parsed [See Health]");
    // A file that never parsed isn't on the graph: no "last version" to show.
    expect(text(lineFor({ kind: "problems", count: 1 }, { summary: parse, lastGood: false }))).toBe("1 file doesn't parse · 214 files · Python · updated just now [See Health]");
    const mixed = summary({}, { doesntParse: 1 });
    expect(text(lineFor({ kind: "problems", count: 2 }, { summary: mixed }))).toBe("2 problems · 214 files · Python · updated just now [See Health]");
  });

  it("joins the failed state's link to its detail", () => {
    expect(lineFor({ kind: "failed", message: "x" }).action?.joined).toBe(true);
    expect(lineFor({ kind: "compiling", done: 0, total: 0 }).action?.joined).toBe(false);
  });

  it("notes scanning in the tooltip", () => {
    const scanning = summary({ status: { ...summary().status, watching: "scanning" } });
    expect(lineFor({ kind: "current" }, { summary: scanning }).notes).toEqual([
      "File events aren't available for this folder, so Unfold checks it for changes every few seconds.",
    ]);
    expect(lineFor({ kind: "current" }).notes).toEqual([]);
  });

  it("shows only the headline before the summary answers", () => {
    expect(stateLine({ state: { kind: "current" }, summary: undefined, progress: null }).detail).toBe("");
  });
});

describe("compile progress", () => {
  it("counts files while reading, then names the stage", () => {
    expect(compilingDetail({ done: 0, total: 0 }, { stage: "extract", done: 412, total: 1058 })).toBe("412 of 1,058 files · reading files");
    expect(compilingDetail({ done: 1058, total: 1058 }, { stage: "resolve", done: 3, total: 9 })).toBe("1,058 of 1,058 files · resolving calls");
    expect(compilingDetail({ done: 0, total: 0 }, null)).toBe("");
  });
});
