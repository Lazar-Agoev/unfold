import { describe, expect, it } from "vitest";
import type { IndexLogEntry } from "../../ipc";
import { duration, factLines, logRows } from "./activity";

const NOW = Date.UTC(2026, 9, 9, 12);

function entry(atMs: number, action: string, text: string, updateMs = 100, commitMs = 40): IndexLogEntry {
  return { atMs, action, files: 214, updateMs, commitMs, filesWritten: 3, filesRemoved: 0, edgesWritten: 10, text };
}

const TEXT = [
  "update · 214 files (typescript 214) · 8 threads · 140ms",
  "files: read 3, from cache 211, extracted 3, removed 0; inputs read 1; discovery changed paths",
  "states: current 213, failed 1",
  "graph unchanged: nothing that shapes it changed",
].join("\n");

describe("the Activity log (11)", () => {
  it("splits the engine's fact lines", () => {
    const { summary, facts } = factLines(TEXT);
    expect(summary).toMatch(/^update/);
    expect(facts[0]).toEqual({ label: "files", value: "read 3, from cache 211, extracted 3, removed 0; inputs read 1; discovery changed paths" });
    expect(facts[2]).toEqual({ label: "graph unchanged", value: "nothing that shapes it changed" });
  });

  it("lists updates newest first with files read / cached / failed", () => {
    const rows = logRows([entry(NOW - 86_400_000, "Compile", "compile\nfiles: read 214, from cache 0\nstates: current 214", 2000, 100), entry(NOW - 120_000, "Background update", TEXT)], NOW);
    expect(rows.map((row) => [row.time, row.what, row.files, row.duration])).toEqual([
      ["2 minutes ago", "Background update", "3 / 211 / 1", "140 ms"],
      ["yesterday", "Compile", "214 / 0 / 0", "2.1 s"],
    ]);
  });

  it("counts files showing their last version that parsed as failed", () => {
    const [row] = logRows([entry(NOW, "Update", "x\nfiles: read 2, from cache 5\nstates: current 5, last_good 1, failed 1")], NOW);
    expect(row?.files).toBe("2 / 5 / 2");
  });

  it("formats durations", () => {
    expect(duration(90)).toBe("90 ms");
    expect(duration(2100)).toBe("2.1 s");
  });
});
