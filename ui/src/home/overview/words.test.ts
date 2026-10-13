import { describe, expect, it } from "vitest";
import { moreWords, summaryLine } from "./words";

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

describe("moreWords", () => {
  it("says routes when the rows past the launches are routes", () => {
    expect(moreWords(2, [{ kind: "process" }, { kind: "http" }])).toBe("2 more routes");
    expect(moreWords(1, [{ kind: "http" }])).toBe("1 more route");
    expect(moreWords(3, [{ kind: "http" }, { kind: "command" }])).toBe("3 more entrances");
  });
});
