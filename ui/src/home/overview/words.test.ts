import { describe, expect, it } from "vitest";
import { DEPLOY, SUGGESTIONS } from "../../test/entrances";
import { deployBreakdown, listWords, moreWords, setupSentence, summaryLine } from "./words";

describe("listWords", () => {
  it("joins one, two, or more words", () => {
    expect(listWords(["shared"])).toBe("shared");
    expect(listWords(["shared", "api"])).toBe("shared and api");
    expect(listWords(["shared", "api", "utils"])).toBe("shared, api, and utils");
  });
});

describe("summaryLine", () => {
  it("counts applications, launches, and routes once there are launches", () => {
    expect(summaryLine({ applications: 15, launches: 16, routes: 31, entrances: 60 }, false)).toBe(
      "15 applications · 16 launches · 31 routes. Hover a row to trace it on a new board.",
    );
    expect(summaryLine({ applications: 1, launches: 1, routes: 1, entrances: 2 }, false)).toBe("1 application · 1 launch · 1 route. Hover a row to trace it on a new board.");
  });

  it("says why routes without launches belong to no service", () => {
    expect(summaryLine({ applications: 0, launches: 0, routes: 31, entrances: 31 }, false)).toBe(
      "31 routes, but no launches yet, so Unfold can't tell which service serves them.",
    );
  });

  it("waits for the compile, or says there is nothing yet", () => {
    const none = { applications: 0, launches: 0, routes: 0, entrances: 0 };
    expect(summaryLine(none, true)).toBe("Entrances show up here as the compile finds them.");
    expect(summaryLine(none, false)).toBe("No launches or routes yet.");
  });
});

describe("setupSentence", () => {
  it("names the routes, the local-looking imports, and how many imports resolve", () => {
    expect(setupSentence(SUGGESTIONS)).toBe(
      "There are no package scripts or pyproject entry points here, so routes belong to no application, and imports like shared, api, and utils read as packages: 249 of 7,239 imports resolve.",
    );
  });

  it("leaves out what doesn't apply", () => {
    // A TypeScript project: no Python imports to count.
    expect(setupSentence({ routes: 8, imports: { resolved: 0, total: 0 }, localLooking: [] })).toBe(
      "There are no package scripts or pyproject entry points here, so routes belong to no application.",
    );
    expect(setupSentence({ routes: 0, imports: { resolved: 40, total: 90 }, localLooking: [] })).toBe(
      "There are no package scripts or pyproject entry points here, so only 40 of 90 imports resolve.",
    );
    expect(setupSentence({ routes: 0, imports: { resolved: 90, total: 90 }, localLooking: ["api"] })).toBe(
      "There are no package scripts or pyproject entry points here, so imports like api read as packages.",
    );
    expect(setupSentence(undefined)).toBe("There are no package scripts or pyproject entry points here.");
  });
});

describe("deployBreakdown", () => {
  it("counts each kind of service and names the files", () => {
    expect(deployBreakdown(DEPLOY)).toBe("2 web services, 1 worker, and 3 cron jobs in render.yaml and data-pipelines/render.yaml");
    expect(deployBreakdown({ files: ["render.yaml"], launches: DEPLOY.launches.slice(0, 1) })).toBe("1 web service in render.yaml");
  });
});

describe("moreWords", () => {
  it("says routes when the rows past the launches are routes", () => {
    expect(moreWords(2, [{ kind: "process" }, { kind: "http" }])).toBe("2 more routes");
    expect(moreWords(1, [{ kind: "http" }])).toBe("1 more route");
    expect(moreWords(3, [{ kind: "http" }, { kind: "command" }])).toBe("3 more entrances");
  });
});
