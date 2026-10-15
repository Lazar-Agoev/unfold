import { describe, expect, it, vi } from "vitest";
import entrances from "../../../../../testdata/workspace/commands/entrances.json";
import type { BoardList, EntrancesView, ProjectConfig, ProjectSummary } from "../engine-types";
import { getProject } from "./projects";
import { harness, PROJECT, recorded } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

const SUMMARY: ProjectSummary = {
  project: PROJECT,
  name: "ts-orders",
  path: "/code/ts-orders",
  state: { kind: "current" },
  status: { state: { kind: "current", files: 12, languages: ["typescript"] }, watching: "events", complete: true },
  updatedAtMs: 0,
  files: 12,
  languages: ["typescript"],
  health: { doesntParse: 0, boardsNeedingReview: 0, boards: [], entrancesToCheck: 0 },
};
const CONFIG: ProjectConfig = {
  exists: true,
  text: "",
  settings: { languages: null, include: [], exclude: [], pythonProjects: [], evidence: [], apps: [] },
  error: null,
};
const BOARDS: BoardList = { project: PROJECT, emptyFolders: [], boards: [] };

/** A project whose unfold.toml declares the recorded apps, or none. */
function project(declared: boolean) {
  const view = recorded<EntrancesView>(entrances);
  return harness({
    engine: {
      project_summary: () => SUMMARY,
      project_config: () => CONFIG,
      list_boards: () => BOARDS,
      entrances: () => (declared ? view : { ...view, apps: [] }),
    },
  });
}

describe("get_project", () => {
  it("points at list_entrances when the project declares apps", async () => {
    const { text } = await project(true).call(getProject, { project: "ts-orders" });
    expect(text).toContain("7 apps, 18 entrances in all:");
    expect(text.split("\n").at(-1)).toBe("Next: list_entrances to see how it starts, search to find code, or read_page to read a board.");
  });

  it("tells the agent to propose apps and entrances when the project declares none", async () => {
    const { text } = await project(false).call(getProject, { project: "ts-orders" });
    expect(text).toContain("0 apps, 0 entrances in all:");
    expect(text.split("\n").at(-1)).toBe(
      'Next: propose its apps and entrances to the person (get_guide("setup") says how); with their yes, set_app and set_entrance, then trace them: Unfold draws the arrows from each entrance.',
    );
  });
});
