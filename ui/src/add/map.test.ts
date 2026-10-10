import { describe, expect, it } from "vitest";
import type { EntranceMap, EntranceRow, LaunchRow } from "../ipc/generated";
import { fixture } from "../test/engine";
import { flatChips, laneChips, lanesOf, launchCommand, mapTotal, PER_LANE, type ProjectMap } from "./map";
import { projectRow } from "../test/workspace";

function map(): EntranceMap {
  return { ...fixture("entrances", "entrances"), project: "p-orders" };
}

function row(index: number): EntranceRow {
  return {
    key: `entrance:${index}`,
    title: `GET /r${index}`,
    kind: "http",
    method: "GET",
    trigger: `/r${index}`,
    triggerStatus: "exact",
    label: null,
    detector: "express.route",
    path: "src/app.ts",
    line: index,
    language: "typescript",
    group: "r",
    reaches: 1,
  };
}

describe("lanes", () => {
  it("makes a lane per application with entrances, named and started as its launch says", () => {
    const lanes = lanesOf(map());
    expect(lanes.map((lane) => [lane.name, lane.command, lane.total])).toEqual([
      ["cli", "orders", 4],
      ["scheduler", "npm run scheduler", 2],
      ["server", "npm run start", 9],
      ["worker", "npm run worker", 3],
    ]);
    expect(mapTotal(map())).toBe(18);
  });

  it("adds a lane for the entrances no launch reaches", () => {
    const unreached = { ...map(), unreached: { total: 1, entrances: [row(1)] } };
    expect(lanesOf(unreached).at(-1)).toMatchObject({ name: "Not reached from a launch", startup: null, command: null });
  });

  it("reads a launch's command from its detector", () => {
    const launch: LaunchRow = { key: "k", title: "process start", trigger: "start", detector: "package.script", path: "package.json", startup: "dist/server.js", note: null };
    expect(launchCommand(launch)).toBe("npm run start");
    expect(launchCommand({ ...launch, detector: "pyproject.script", trigger: "jobs" })).toBe("jobs");
    expect(launchCommand({ ...launch, detector: "manual", trigger: null })).toBe("dist/server.js");
  });
});

describe("chips", () => {
  const lane = { id: "p\u001fs", project: "p", startup: "s", name: "server", command: null, total: 30, entrances: Array.from({ length: 12 }, (_, index) => row(index)) };

  it("shows twelve entrances, then +N more", () => {
    const chips = laneChips(lane, undefined);
    expect(chips).toHaveLength(PER_LANE + 1);
    expect(chips.at(-1)).toMatchObject({ type: "more", count: 18 });
  });

  it("shows every entrance of an expanded lane, with no more chip", () => {
    const all = Array.from({ length: 30 }, (_, index) => row(index));
    expect(laneChips(lane, all).map((chip) => chip.type)).toEqual(Array.from({ length: 30 }, () => "entrance"));
  });

  it("walks the chips of the ready projects in reading order", () => {
    const projects: ProjectMap[] = [
      { status: "ready", row: projectRow("p", "orders"), lanes: [{ lane, chips: laneChips(lane, undefined) }] },
      { status: "loading", row: projectRow("q", "payments") },
    ];
    expect(flatChips(projects).map((chip) => chip.id).slice(0, 2)).toEqual(["p\u001fs\u001fentrance:0", "p\u001fs\u001fentrance:1"]);
  });
});
