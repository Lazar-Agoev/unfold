import { describe, expect, it } from "vitest";
import type { EntranceRow } from "../../ipc";
import { fixture } from "../../test/engine";
import { applicationRows, launchCommand, overflowWords, widestApplication } from "./entrances";

function route(key: string): EntranceRow {
  return {
    key,
    title: key,
    kind: "http",
    method: "GET",
    trigger: key,
    triggerStatus: "exact",
    label: null,
    detector: "express.route",
    path: "src/app.ts",
    line: 1,
    language: "typescript",
    group: null,
    reaches: 3,
  };
}

describe("How it starts", () => {
  it("names the launch as it is typed", () => {
    const map = fixture("entrances", "entrances");
    const [cli] = map.applications;
    expect(cli?.launches.map(launchCommand)).toEqual(["orders", "npm run cli"]);
    expect(launchCommand({ detector: "pyproject.script", trigger: null, startup: "jobs.main:run", title: "process jobs" })).toBe("jobs.main:run");
  });

  it("says what is left after the first rows", () => {
    expect(overflowWords({ total: 14, entrances: [route("/a"), route("/b")] })).toBe("12 more routes");
    expect(overflowWords({ total: 3, entrances: [route("/a"), { ...route("q"), kind: "worker" }] })).toBe("1 more entrance");
  });

  it("widens the application with the most entrances, when there are several", () => {
    expect(widestApplication([{ startup: "a", total: 3 }])).toBeNull();
    expect(
      widestApplication([
        { startup: "a", total: 3 },
        { startup: "b", total: 14 },
        { startup: "c", total: 14 },
      ]),
    ).toBe("b");
  });

  it("lays applications out in even rows of at most three", () => {
    const sizes = (count: number) => applicationRows(Array.from({ length: count }, (_, index) => index)).map((row) => row.length);
    expect(sizes(1)).toEqual([1]);
    expect(sizes(3)).toEqual([3]);
    expect(sizes(4)).toEqual([2, 2]);
    expect(sizes(5)).toEqual([3, 2]);
    expect(sizes(7)).toEqual([3, 2, 2]);
    expect(sizes(0)).toEqual([]);
  });
});
