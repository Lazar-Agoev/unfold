import { describe, expect, it } from "vitest";
import { problemText, settingsProblem } from "./edits";

describe("settings problems", () => {
  it("splits the position out of Rust's message", () => {
    const problem = settingsProblem("invalid_settings", "unfold.toml:31:11: `src/a.ts#a.b.c` is not a valid locator");
    expect(problem).toEqual({ code: "invalid_settings", message: "`src/a.ts#a.b.c` is not a valid locator", line: 31, column: 11 });
    expect(problemText(problem)).toBe("Line 31, column 11: `src/a.ts#a.b.c` is not a valid locator");
    const prefixed = settingsProblem("invalid_settings", "fix unfold.toml first: unfold.toml:2:1: unknown field `bogus`");
    expect(prefixed.line).toBe(2);
    expect(prefixed.message).toBe("fix unfold.toml first: unknown field `bogus`");
    expect(settingsProblem("settings_conflict", "no `[[entrance]]` has the id `gone`")).toMatchObject({ line: null, column: null });
  });
});
