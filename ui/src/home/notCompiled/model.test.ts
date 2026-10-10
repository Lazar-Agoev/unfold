import { describe, expect, it } from "vitest";
import type { ScopeFolder, SkipKind, SkippedFolder } from "../../ipc";
import { fixture } from "../../test/engine";
import { compileCaption, effectiveChoice, languageSentence, savedChoice, scopeCounts, scopeEdits, scopeRows, skippedRows } from "./model";

function folder(path: string, skipped: ScopeFolder["skipped"], sources: number): ScopeFolder {
  return { name: path, path, skipped, sources };
}

const WEB = folder("apps/web", null, 398);
const LEGACY = folder("tools/legacy", "ignored", 0);
const FOLDERS = [folder("apps/api", null, 512), WEB, folder("scripts", "excluded", 61), LEGACY, folder("node_modules", "default", 0)];

describe("what the first Compile reads (06)", () => {
  it("says what it reads, by language", () => {
    expect(languageSentence(fixture("compile_preview", "compile_preview"))).toBe("Unfold will read 34 TypeScript files.");
    expect(
      languageSentence({
        files: 1058,
        languages: [
          { language: "typescript", files: 846 },
          { language: "python", files: 212 },
        ],
      }),
    ).toBe("Unfold will read 846 TypeScript and 212 Python files.");
    expect(languageSentence({ files: 0, languages: [] })).toBe("There are no TypeScript or Python files to read.");
  });

  it("groups skipped folders by reason, two examples each, in 06's words", () => {
    const skipped = (folderName: string, reason: SkipKind, files: number, wholeFolder = true): SkippedFolder => ({
      folder: folderName,
      reason,
      reasonText: "the engine's phrase",
      wholeFolder,
      files,
    });
    expect(
      skippedRows([
        skipped("node_modules", "default", 14000),
        skipped("dist", "default", 206),
        skipped("build", "default", 0),
        skipped(".venv", "ignored", 2931),
        skipped(".", "javascript", 1, false),
        skipped("vendor", "excluded", 0),
      ]),
    ).toEqual([
      { reason: "Skipped by default", examples: "node_modules/ · dist/ · 1 more", count: "14,206 files" },
      { reason: "Ignored by .gitignore", examples: ".venv/", count: "2,931 files" },
      { reason: "JavaScript isn't graphed", examples: "top level", count: "1 file" },
      { reason: "Excluded in settings", examples: "vendor/", count: "" },
    ]);
  });

  it("captions the Compile by the project's size", () => {
    expect(compileCaption(34)).toMatch(/^A few seconds/);
    expect(compileCaption(1058)).toMatch(/^About a minute/);
    expect(compileCaption(40_000)).toMatch(/^A few minutes/);
  });
});

describe("the first Compile's scope (V-69)", () => {
  it("reads the saved choice from scope_dir", () => {
    expect(FOLDERS.map(savedChoice)).toEqual(["checked", "checked", "unchecked", "disabled", "disabled"]);
    expect(scopeRows(FOLDERS).map((row) => row.path)).toEqual(["apps/api", "apps/web", "scripts", "tools/legacy"]);
  });

  it("stages changes and writes only those", () => {
    const staged = { "apps/web": false, scripts: true, "tools/legacy": true };
    expect(effectiveChoice(WEB, staged)).toBe("unchecked");
    expect(effectiveChoice(LEGACY, staged)).toBe("disabled");
    expect(scopeEdits(FOLDERS, staged)).toEqual([
      { op: "exclude_from_graph", path: "apps/web" },
      { op: "include_in_graph", path: "scripts" },
    ]);
    expect(scopeEdits(FOLDERS, {})).toEqual([]);
  });

  it("counts the files the Compile will read of those it could", () => {
    // The saved scope reads 910 files (plus top-level ones: 920); scripts' 61 are left out.
    expect(scopeCounts(920, FOLDERS, {})).toEqual({ chosen: 920, total: 981 });
    expect(scopeCounts(920, FOLDERS, { scripts: true })).toEqual({ chosen: 981, total: 981 });
    expect(scopeCounts(920, FOLDERS, { "apps/api": false })).toEqual({ chosen: 408, total: 981 });
  });
});
