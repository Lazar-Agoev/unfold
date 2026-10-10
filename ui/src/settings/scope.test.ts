import { describe, expect, it } from "vitest";
import type { ScopeFolder } from "../ipc";
import { checkOf, covers, escapeGlob, folderScope, literalOf, ruleRows, toggleFolder, toggleRule, treeOrder, type ScopeLists } from "./scope";

const folder = (path: string, skipped: ScopeFolder["skipped"] = null): ScopeFolder => ({
  name: path.split("/").at(-1) ?? path,
  path,
  skipped,
  sources: 3,
});

const NONE: ScopeLists = { include: [], exclude: [] };

describe("scope patterns", () => {
  it("escapes and reads folder paths as Rust writes them", () => {
    expect(escapeGlob("src/[id]")).toBe("src/\\[id\\]");
    expect(literalOf("src/\\[id\\]")).toBe("src/[id]");
    expect(literalOf("src/legacy/")).toBe("src/legacy");
    expect(literalOf("src/*.ts")).toBeNull();
  });

  it("covers a path, its folders' contents, and simple globs; other globs are unknown", () => {
    expect(covers("src", "src/api")).toBe(true);
    expect(covers("src", "srcx")).toBe(false);
    expect(covers("**/legacy", "src/legacy")).toBe(true);
    expect(covers("src/*", "src/api/v1")).toBe(true);
    expect(covers("src/?pi", "src/api")).toBe(true);
    expect(covers("lib/*", "src/api")).toBe(false);
    expect(covers("src/[ab]", "src/a")).toBeNull();
  });
});

describe("folderScope", () => {
  it("disables folders discovery skips, with their reason", () => {
    expect(folderScope(folder("node_modules", "default"), NONE, NONE)).toEqual({ kind: "fixed", reason: "skipped by default" });
    expect(folderScope(folder(".venv", "ignored"), NONE, NONE)).toEqual({ kind: "fixed", reason: "ignored by .gitignore" });
  });

  it("is mixed when an exclude pattern lies under a folder", () => {
    const scope = folderScope(folder("src"), NONE, { include: [], exclude: ["src/legacy"] });
    expect(checkOf(scope)).toBe("mixed");
  });

  it("follows a saved exclude until it is staged for removal", () => {
    const saved = { include: [], exclude: ["scripts"] };
    expect(folderScope(folder("scripts", "excluded"), saved, saved)).toEqual({ kind: "excluded", locked: null });
    expect(folderScope(folder("scripts", "excluded"), saved, NONE)).toEqual({ kind: "included", mixed: false });
  });

  it("locks a folder excluded from above", () => {
    const saved = { include: [], exclude: ["src"] };
    expect(folderScope(folder("src/api", "excluded"), saved, saved)).toEqual({ kind: "excluded", locked: "src" });
    expect(folderScope(folder("src/api", "excluded"), { include: [], exclude: ["src/[ab]"] }, { include: [], exclude: ["src/[ab]"] })).toEqual({
      kind: "excluded",
      locked: "src/[ab]",
    });
  });

  it("trusts Rust when no saved pattern explains an exclusion", () => {
    expect(folderScope(folder("gen", "excluded"), NONE, NONE)).toEqual({ kind: "excluded", locked: "a pattern in the file" });
  });

  it("leaves folders outside an include list unchecked, and mixed above an included one", () => {
    const lists = { include: ["src/api"], exclude: [] };
    expect(folderScope(folder("tests", "not_included"), lists, lists)).toEqual({ kind: "not_included", mixed: false });
    expect(folderScope(folder("src", "not_included"), lists, lists)).toEqual({ kind: "not_included", mixed: true });
    expect(folderScope(folder("src/api"), lists, lists)).toEqual({ kind: "included", mixed: false });
  });
});

describe("toggleFolder", () => {
  it("excludes a graphed folder and checks it again by dropping that pattern", () => {
    const unchecked = toggleFolder(folder("src/legacy"), NONE, NONE);
    expect(unchecked).toEqual({ include: [], exclude: ["src/legacy"] });
    expect(toggleFolder(folder("src/legacy"), NONE, unchecked)).toEqual(NONE);
  });

  it("drops the folder's own include pattern while others remain, as Rust does", () => {
    const lists = { include: ["src", "lib"], exclude: [] };
    expect(toggleFolder(folder("src"), lists, lists)).toEqual({ include: ["lib"], exclude: [] });
    const only = { include: ["src"], exclude: [] };
    expect(toggleFolder(folder("src"), only, only)).toEqual({ include: ["src"], exclude: ["src"] });
  });

  it("includes a folder outside the include list", () => {
    const lists = { include: ["src"], exclude: [] };
    expect(toggleFolder(folder("tests", "not_included"), lists, lists)).toEqual({ include: ["src", "tests"], exclude: [] });
  });

  it("changes nothing for a locked or skipped folder", () => {
    const saved = { include: [], exclude: ["src"] };
    expect(toggleFolder(folder("src/api", "excluded"), saved, saved)).toBe(saved);
    expect(toggleFolder(folder("dist", "default"), NONE, NONE)).toBe(NONE);
  });
});

describe("rules", () => {
  it("lists saved patterns, struck when staged for removal, then the staged ones", () => {
    const saved = { include: [], exclude: ["src/legacy", "scripts"] };
    const draft = { include: [], exclude: ["src/legacy", "tests"] };
    const rows = ruleRows(saved, draft);
    expect(rows).toEqual([
      { list: "exclude", pattern: "src/legacy", status: "kept" },
      { list: "exclude", pattern: "scripts", status: "removed" },
      { list: "exclude", pattern: "tests", status: "added" },
    ]);
    const [, removed, added] = rows;
    if (!removed || !added) {
      throw new Error("missing rows");
    }
    expect(toggleRule(draft, removed).exclude).toContain("scripts");
    expect(toggleRule(draft, added).exclude).toEqual(["src/legacy"]);
  });

  it("orders the tree with skipped folders last", () => {
    expect(treeOrder([folder("dist", "default"), folder("src"), folder(".venv", "ignored"), folder("tests")]).map((item) => item.path)).toEqual([
      "src",
      "tests",
      "dist",
      ".venv",
    ]);
  });
});
