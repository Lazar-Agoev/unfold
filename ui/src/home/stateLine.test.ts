import { describe, expect, it } from "vitest";
import type { ProjectState, ProjectSummary } from "../ipc";
import { compilingDetail, homeStateKind, stateLine, type StateLineModel } from "./stateLine";

const NOW = Date.UTC(2026, 9, 9, 12, 0, 0);

function summary(overrides: Partial<ProjectSummary> = {}, health: Partial<ProjectSummary["health"]> = {}): ProjectSummary {
  return {
    project: "p-1",
    name: "storefront-api",
    path: "/Users/x/code/storefront-api",
    state: { kind: "current" },
    status: { state: { kind: "current", files: 1284, languages: ["typescript", "python"] }, fallback: null, watching: "events", backgroundUpdates: true, complete: true },
    updatedAtMs: NOW - 2 * 60_000,
    files: 1284,
    languages: ["typescript", "python"],
    applications: 3,
    entrances: 19,
    health: {
      problems: 0,
      doesntParse: 0,
      settingsError: null,
      unreachedEntrances: 0,
      unindexedLaunches: 0,
      boardsNeedingReview: 0,
      boards: [],
      ...health,
    },
    git: null,
    ...overrides,
  };
}

function text(line: StateLineModel): string {
  const detail = line.detail.map((part) => part.text).join("");
  return [line.headline, detail, line.action ? `[${line.action.label}]` : ""].filter((part) => part !== "").join(" ");
}

function lineFor(state: ProjectState, extra: { summary?: ProjectSummary; branch?: string | null; lastGood?: boolean } = {}): StateLineModel {
  return stateLine({
    state,
    summary: extra.summary ?? summary(),
    progress: null,
    branch: extra.branch === undefined ? "main" : extra.branch,
    ...(extra.lastGood === undefined ? {} : { lastGood: extra.lastGood }),
    nowMs: NOW,
  });
}

describe("Home's state line (07, V-25)", () => {
  it("words every state", () => {
    expect(text(lineFor({ kind: "not_compiled" }))).toBe("Not compiled yet · branch main");
    expect(text(lineFor({ kind: "compiling", done: 412, total: 1058 }))).toBe("Compiling · 412 of 1,058 files [Cancel]");
    expect(text(lineFor({ kind: "updating", files: 3 }))).toBe("Updating · 3 files changed");
    expect(text(lineFor({ kind: "updating", files: null }))).toBe("Updating · checking the folder");
    expect(text(lineFor({ kind: "current" }))).toBe("Graph current · 1,284 files · TypeScript + Python · updated 2 minutes ago");
    expect(text(lineFor({ kind: "paused", changed: 12 }))).toBe("Paused · 12 files changed since [Resume]");
    expect(text(lineFor({ kind: "failed", message: "disk full" }))).toBe("Compile failed · the previous graph stays · [See Health]");
    expect(text(lineFor({ kind: "resting", last: { kind: "current" }, atMs: NOW - 2 * 3_600_000 }))).toBe("Resting · last checked 2 hours ago");
    expect(text(lineFor({ kind: "unavailable", reason: "The folder is gone." }))).toBe("Unavailable · The folder is gone.");
  });

  it("names the problem from the actual counts", () => {
    const parse = summary({}, { problems: 1, doesntParse: 1 });
    expect(text(lineFor({ kind: "problems", count: 1 }, { summary: parse, lastGood: true }))).toBe("1 file doesn't parse · showing its last version that parsed [See Health]");
    const two = summary({}, { problems: 2, doesntParse: 2 });
    expect(text(lineFor({ kind: "problems", count: 2 }, { summary: two, lastGood: true }))).toBe("2 files don't parse · showing their last versions that parsed [See Health]");
    // A file that never parsed isn't on the graph: no "last version" to show.
    expect(text(lineFor({ kind: "problems", count: 1 }, { summary: parse, lastGood: false }))).toBe(
      "1 file doesn't parse · 1,284 files · TypeScript + Python · updated 2 minutes ago [See Health]",
    );
    const mixed = summary({ files: 214, languages: ["python"], updatedAtMs: NOW - 5 * 60_000 }, { problems: 2, doesntParse: 1 });
    expect(text(lineFor({ kind: "problems", count: 2 }, { summary: mixed }))).toBe("2 problems · 214 files · Python · updated 5 minutes ago [See Health]");
  });

  it("puts branch names in mono and joins the failed state's link to its detail", () => {
    expect(lineFor({ kind: "not_compiled" }, { branch: "feat/cart-v2" }).detail).toEqual([{ text: "· branch " }, { text: "feat/cart-v2", mono: true }]);
    expect(lineFor({ kind: "not_compiled" }, { branch: null }).detail).toEqual([]);
    expect(lineFor({ kind: "failed", message: "x" }).action?.joined).toBe(true);
    expect(lineFor({ kind: "compiling", done: 0, total: 0 }).action?.joined).toBe(false);
  });

  it("notes the fallback location and scanning in the tooltip", () => {
    const fallback = summary({ status: { ...summary().status, fallback: "read_only", watching: "scanning" } });
    const line = lineFor({ kind: "current" }, { summary: fallback });
    expect(text(line)).toContain("using fallback location");
    expect(line.notes).toHaveLength(2);
    expect(line.notes[0]).toContain("read-only");
    expect(lineFor({ kind: "current" }).notes).toEqual([]);
  });

  it("shows only the headline before the summary answers", () => {
    expect(stateLine({ state: { kind: "current" }, summary: undefined, progress: null, branch: null }).detail).toEqual([]);
  });
});

describe("compile progress (V-68)", () => {
  it("counts files while reading, then names the stage", () => {
    expect(compilingDetail({ done: 0, total: 0 }, { stage: "extract", done: 412, total: 1058 })).toBe("412 of 1,058 files · reading files");
    expect(compilingDetail({ done: 1058, total: 1058 }, { stage: "resolve", done: 3, total: 9 })).toBe("1,058 of 1,058 files · resolving calls");
    expect(compilingDetail({ done: 0, total: 0 }, null)).toBe("");
  });
});

describe("homeStateKind", () => {
  it("reads a resting project by its last state", () => {
    expect(homeStateKind({ kind: "resting", last: { kind: "not_compiled" }, atMs: 0 })).toBe("not_compiled");
    expect(homeStateKind({ kind: "paused", changed: 0 })).toBe("paused");
  });
});
