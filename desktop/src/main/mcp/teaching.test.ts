import { describe, expect, it } from "vitest";
import { GUIDE_TOPICS, guideText } from "./guide";
import { INSTRUCTIONS } from "./instructions";
import { PROMPTS, TOOLS } from "./tools";

/** Claude Code cuts the instructions and each tool's description here (design §5.3). */
const CLAUDE_CODE_LIMIT = 2_048;
/** Codex wants the instructions' first characters, up to here, to stand on their own. */
const CODEX_LEAD = 512;

describe("the instructions (design §7.2)", () => {
  it("fit Claude Code's limit, and their first paragraph stands alone within Codex's", () => {
    expect(INSTRUCTIONS.length).toBeLessThanOrEqual(CLAUDE_CODE_LIMIT);
    const [first = ""] = INSTRUCTIONS.split("\n\n");
    expect(first.length).toBeLessThanOrEqual(CODEX_LEAD);
    expect(first.startsWith("Unfold compiles")).toBe(true);
    expect(first.endsWith('Call get_guide({topic: "start"}) before any other Unfold tool.')).toBe(true);
  });
});

describe("the tools' definitions", () => {
  it("have unique snake_case names, titles, and descriptions within Claude Code's limit", () => {
    expect(new Set(TOOLS.map((tool) => tool.name)).size).toBe(TOOLS.length);
    for (const tool of TOOLS) {
      expect(tool.name).toMatch(/^[a-z]+(_[a-z]+)*$/);
      expect(tool.title.length).toBeGreaterThan(0);
      expect(tool.description.length, tool.name).toBeLessThanOrEqual(CLAUDE_CODE_LIMIT);
      expect(tool.description.length, tool.name).toBeGreaterThan(80);
    }
  });

  it("load get_guide and get_current_view with the session, and mark reads read-only", () => {
    expect(TOOLS.filter((tool) => tool.alwaysLoad).map((tool) => tool.name)).toEqual(["get_guide", "get_current_view"]);
    for (const name of ["get_guide", "get_current_view", "list_projects", "get_project", "search", "list_entrances", "read_code", "read_call", "read_file", "get_relations"]) {
      expect(TOOLS.find((tool) => tool.name === name)?.annotations.readOnlyHint, name).toBe(true);
    }
    for (const name of ["add_project", "compile_project", "update_project_settings", "create_board", "organize_boards", "edit_pages", "show"]) {
      expect(TOOLS.find((tool) => tool.name === name)?.annotations, name).toMatchObject({ readOnlyHint: false, destructiveHint: expect.any(Boolean) as boolean });
    }
  });

  it("list prompts by unique names", () => {
    expect(new Set(PROMPTS.map((prompt) => prompt.name)).size).toBe(PROMPTS.length);
  });
});

describe("the guide (design §7.3)", () => {
  it("has the start topic written, and every written topic is a Markdown document", () => {
    expect(guideText("start")?.startsWith("# Working in Unfold\n\n## What Unfold is")).toBe(true);
    expect(guideText("start")).toContain("## Review checkpoints (mandatory)");
    for (const topic of GUIDE_TOPICS) {
      const text = guideText(topic);
      expect(text === null || /^# \S/.test(text), topic).toBe(true);
    }
  });
});
