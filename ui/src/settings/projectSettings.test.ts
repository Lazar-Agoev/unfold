import { describe, expect, it } from "vitest";
import type { ProjectConfig } from "../ipc";
import { draftOf, draftProblems, editsBetween, evidenceProblem, newPythonProject, relativeToRoot, type SettingsDraft } from "./projectSettings";

function config(overrides: Partial<NonNullable<ProjectConfig["settings"]>> = {}, name = { value: "orders", isDefault: true }): ProjectConfig {
  return {
    exists: true,
    text: "",
    error: null,
    name,
    settings: { languages: null, include: [], exclude: [], disabledDetectors: [], pythonProjects: [], applications: [], entrances: [], overrides: [], evidence: [], ...overrides },
  };
}

describe("project settings drafts", () => {
  it("has no edits when nothing changed", () => {
    const saved = draftOf(config({ exclude: ["scripts"], disabledDetectors: ["celery.task"] }));
    expect(editsBetween(saved, saved)).toEqual([]);
  });

  it("turns scope changes into removals, then excludes and includes of folder paths", () => {
    const saved = draftOf(config({ exclude: ["scripts"], include: ["src", "lib"] }));
    const draft: SettingsDraft = { ...saved, scope: { include: ["src", "tests"], exclude: ["src/\\[id\\]"] } };
    expect(editsBetween(saved, draft)).toEqual([
      { op: "remove_scope_pattern", list: "include", pattern: "lib" },
      { op: "remove_scope_pattern", list: "exclude", pattern: "scripts" },
      { op: "exclude_from_graph", path: "src/[id]" },
      { op: "include_in_graph", path: "tests" },
    ]);
  });

  it("switches detectors and edits Python projects", () => {
    const saved = draftOf(config({ pythonProjects: [{ root: "py", sourceRoots: ["py/src"], version: "3.12" }] }));
    expect(saved.python[0]?.sourceRoots).toBe("src");
    const draft: SettingsDraft = {
      ...saved,
      disabled: new Set(["fastapi.route"]),
      python: [{ ...newPythonProject(), root: "./tools/", version: "3.13" }],
    };
    expect(editsBetween(saved, draft)).toEqual([
      { op: "set_detector", detector: "fastapi.route", enabled: false },
      { op: "remove_python_project", root: "py" },
      { op: "set_python_project", project: { root: "tools", sourceRoots: null, version: "3.13" } },
    ]);
  });

  it("sets the shared name, and an emptied field returns to the default", () => {
    const fromDefault = draftOf(config());
    expect(editsBetween(fromDefault, { ...fromDefault, name: "orders-api" })).toEqual([{ op: "set_project_name", name: "orders-api" }]);
    expect(editsBetween(fromDefault, { ...fromDefault, name: " " })).toEqual([]);
    const named = draftOf(config({}, { value: "orders-api", isDefault: false }));
    expect(editsBetween(named, { ...named, name: "" })).toEqual([{ op: "set_project_name", name: null }]);
  });

  it("removes and adds evidence patterns", () => {
    const saved = draftOf(config({ evidence: ["prisma/schema.prisma", "db"] }));
    expect(saved.evidence).toEqual(["prisma/schema.prisma", "db"]);
    expect(editsBetween(saved, { ...saved, evidence: ["db", "db/migrations/*.sql"] })).toEqual([
      { op: "remove_evidence_pattern", pattern: "prisma/schema.prisma" },
      { op: "add_evidence_pattern", pattern: "db/migrations/*.sql" },
    ]);
  });

  it("checks an evidence pattern before it is added", () => {
    expect(evidenceProblem("db/migrations/*.sql", ["db"])).toBeNull();
    expect(evidenceProblem(" ", [])).not.toBeNull();
    expect(evidenceProblem("/etc/hosts", [])).toContain("inside this folder");
    expect(evidenceProblem("../secrets.sql", [])).toContain("inside this folder");
    expect(evidenceProblem("db", ["db"])).toContain("listed already");
  });

  it("flags a bad name and Python rows that can't be saved", () => {
    const saved = draftOf(config());
    const problems = draftProblems({ ...saved, name: "-bad name", python: [newPythonProject(), { ...newPythonProject(), root: "../x" }] });
    expect(problems.name).not.toBeNull();
    expect(problems.python.every((problem) => problem !== null)).toBe(true);
    expect(draftProblems(saved)).toEqual({ name: null, python: [] });
  });

  it("writes source roots relative to their project", () => {
    expect(relativeToRoot("py", "py/src")).toBe("src");
    expect(relativeToRoot(".", "src")).toBe("src");
    expect(relativeToRoot("py", "py")).toBe(".");
  });
});
