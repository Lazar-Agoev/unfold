import { describe, expect, it } from "vitest";
import type { ProblemItem } from "../../ipc";
import { boardHealthRows, healthRows, problemLocation, reviewWords } from "./problems";

function problem(category: ProblemItem["category"], path: string, message: string, overrides: Partial<ProblemItem> = {}): ProblemItem {
  return { category, path, line: null, column: null, message, fileState: null, ...overrides };
}

const PROBLEMS: ProblemItem[] = [
  problem("doesnt_parse", "billing/tasks.py", "expected ':'", { line: 88, column: 4, fileState: "last_good" }),
  problem("settings", "pyproject.toml", '[tool.unfold] python = "3.11" isn\'t supported'),
  problem("board", ".unfold/boards/monthly.json", "Monthly statements: needs review: Its code changed since it was described."),
  problem("compile_failed", "", "disk full"),
];

describe("Health rows", () => {
  it("reads each problem as a title, an explanation, a location, and its action", () => {
    expect(healthRows(PROBLEMS)).toEqual([
      {
        id: "0",
        title: "Doesn't parse — line 88: expected ':'",
        explanation: "Showing its last version that parsed.",
        location: "billing/tasks.py:88",
        action: { path: "billing/tasks.py", line: 88 },
      },
      {
        id: "1",
        title: '[tool.unfold] python = "3.11" isn\'t supported',
        explanation: "Unfold uses the defaults until the settings file is fixed.",
        location: "pyproject.toml",
        action: { path: "pyproject.toml", line: null },
      },
      {
        id: "3",
        title: "Compile failed — disk full",
        explanation: "The previous graph stays. Compile again once it's fixed.",
        location: "",
        action: null,
      },
    ]);
  });

  it("opens the settings file in the editor, and unfold.toml for a folder that can't be read", () => {
    const [settings, discovery] = healthRows([problem("settings", "", "expected a table at line 3"), problem("discovery", "vendor", "permission denied")]);
    expect(settings?.action).toEqual({ path: "unfold.toml", line: null });
    expect(discovery?.action).toEqual({ path: "unfold.toml", line: null });
  });

  it("explains each board that needs review by its first item", () => {
    expect(boardHealthRows([{ id: ".unfold/boards/monthly.json", title: "Monthly statements", items: 1 }], PROBLEMS)).toEqual([
      { id: ".unfold/boards/monthly.json", board: "Monthly statements", title: "1 item needs review", explanation: "Its code changed since it was described." },
    ]);
    expect(reviewWords(3)).toBe("3 items need review");
  });

  it("locates a problem by path and line", () => {
    expect(problemLocation({ path: "", line: 3 })).toBe("");
    expect(problemLocation({ path: "a.ts", line: null })).toBe("a.ts");
    expect(problemLocation({ path: "a.ts", line: 3 })).toBe("a.ts:3");
  });
});
