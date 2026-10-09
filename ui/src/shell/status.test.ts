import { describe, expect, it } from "vitest";
import type { GraphState, GraphStatus } from "../ipc/generated";
import { progressText, statusLine } from "./status";

function status(state: GraphState, extra: Partial<GraphStatus> = {}): GraphStatus {
  return { state, fallback: null, watching: "events", backgroundUpdates: true, complete: true, ...extra };
}

const text = (state: GraphState) => {
  const line = statusLine(status(state));
  return `${line.glyph} ${line.text}`;
};

describe("statusLine", () => {
  it("says what architecture §7.9 says for every state", () => {
    expect(text({ kind: "current", files: 84, languages: ["typescript", "python"] })).toBe("● Graph current · 84 files · TypeScript + Python");
    expect(text({ kind: "compiling", done: 12, total: 84 })).toBe("◌ Compiling · 12 of 84 files");
    expect(text({ kind: "compiling", done: 0, total: 0 })).toBe("◌ Compiling…");
    expect(text({ kind: "updating", files: 2 })).toBe("◌ Updating · 2 files changed");
    expect(text({ kind: "updating", files: null })).toBe("◌ Updating · checking the folder");
    expect(text({ kind: "doesnt_parse", files: 1, total: 84, languages: ["typescript"] })).toBe(
      "● 1 file doesn't parse · showing its last version that parsed",
    );
    expect(text({ kind: "doesnt_parse", files: 3, total: 84, languages: ["typescript"] })).toBe(
      "● 3 files don't parse · showing their last versions that parsed",
    );
    expect(text({ kind: "paused", changed: 3 })).toBe("● Background updates paused · 3 files changed since the last compile");
    expect(text({ kind: "failed", message: "disk full" })).toBe("▲ Compile failed · see Problems");
    expect(text({ kind: "not_compiled" })).toBe("○ Not compiled yet");
  });

  it("lays live progress over a compile, in files while reading them and in words after", () => {
    const compiling = status({ kind: "compiling", done: 3, total: 84 });
    expect(statusLine(compiling, { stage: "extract", done: 40, total: 84 }).text).toBe("Compiling · 40 of 84 files");
    expect(statusLine(compiling, { stage: "resolve", done: 1, total: 4 }).text).toBe("Compiling · resolving calls");
    expect(progressText(null)).toBeNull();
  });

  it("adds the fallback-location and scanning notes", () => {
    const line = statusLine(status({ kind: "current", files: 1, languages: [] }, { fallback: "network_volume", watching: "scanning" }));
    expect(line.text).toBe("Graph current · 1 file");
    expect(line.notes.map((note) => note.text)).toEqual(["using fallback location", "watching by scanning"]);
    expect(line.notes[0]?.title).toContain("network volume");
  });
});
