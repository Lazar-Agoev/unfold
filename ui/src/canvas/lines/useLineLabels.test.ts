import { describe, expect, it } from "vitest";
import type { Arrow } from "../arrows";
import { LABELS_PER_CALL, labelRequests } from "./useLineLabels";

function arrow(id: string, project: string, source: string, seq = 0, kind: Arrow["kind"] = "calls"): Arrow {
  return { id, source: "a", sourceHandle: source, target: "b", project, seq, kind, dashed: kind === "references", order: 0, line: 1 };
}

describe("labelRequests", () => {
  it("asks once per project for its distinct lines in order, and never for an entrance's handler", () => {
    const requests = labelRequests([
      arrow("1", "q", "src/b.ts#f/call[0]"),
      arrow("2", "p", "src/a.ts#f/call[1]", 1, "references"),
      arrow("3", "p", "src/a.ts#f/call[0]"),
      arrow("4", "p", "src/a.ts#f/call[1]"),
      // The same line drawn to a second card is asked for once.
      { ...arrow("5", "p", "src/a.ts#f/call[0]"), target: "c" },
      arrow("6", "p", "target:0", 0, "handler"),
    ]);
    expect(requests).toEqual([
      {
        project: "p",
        lines: [
          { source: "src/a.ts#f/call[0]", seq: 0 },
          { source: "src/a.ts#f/call[1]", seq: 0 },
          { source: "src/a.ts#f/call[1]", seq: 1 },
        ],
      },
      { project: "q", lines: [{ source: "src/b.ts#f/call[0]", seq: 0 }] },
    ]);
  });

  it("splits a page with more lines than one call takes", () => {
    const many = Array.from({ length: LABELS_PER_CALL + 3 }, (_, index) => arrow(String(index), "p", `src/a.ts#f/call[${String(index).padStart(4, "0")}]`));
    const requests = labelRequests(many);
    expect(requests.map((request) => request.lines.length)).toEqual([LABELS_PER_CALL, 3]);
  });
});
