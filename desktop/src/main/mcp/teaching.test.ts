import type { McpServer } from "@modelcontextprotocol/server";
import { describe, expect, it } from "vitest";
import { z } from "zod";
import { GUIDE_TOPICS, guideText, type GuideTopic } from "./guide";
import { INSTRUCTIONS } from "./instructions";
import { MAX_RESULT_CHARS } from "./results";
import { TOOLS } from "./tools";

/** Claude Code cuts the instructions and each tool's description here. */
const CLAUDE_CODE_LIMIT = 2_048;
/** Codex wants the instructions' first characters, up to here, to stand on their own. */
const CODEX_LEAD = 512;
/** Room a `get_guide` result keeps for its header, so a topic is never cut. */
const HEADER_ROOM = 200;

/** Words the guide uses that look like a tool's name but name something else. */
const OTHER_WORDS: ReadonlySet<string> = new Set(["node_modules"]);

/**
 * A word written like a tool's name: lowercase, with underscores, and not part of a key
 * (`…/call:save_order`, `shop/api.py#create_order`).
 */
const SNAKE = /(?<![\w:#/])[a-z][a-z0-9]*(?:_[a-z0-9]+)+(?![\w[:/])/g;
/** A call written in text: `add_cards(`, `get_guide({topic: …})`. */
const CALL = /\b([a-z][a-z0-9]*(?:_[a-z0-9]+)+)\(/g;
/** A guide topic named in text: `get_guide({topic: "diagrams"})`, `get_guide("diagrams")`, `(diagrams topic)`. */
const TOPIC_NAMED = /get_guide\(\{\s*topic:\s*"([^"]+)"\s*\}\)|get_guide\("([^"]+)"\)|\(([a-z-]+) topic\)/g;

/** Each tool's input schema, as the tools register them with the SDK, from a stand-in for its server. */
function registered(): ReadonlyMap<string, z.ZodType> {
  const inputs = new Map<string, z.ZodType>();
  const server = {
    registerTool(name: string, config: { readonly inputSchema: z.ZodType }) {
      inputs.set(name, config.inputSchema);
    },
  } as unknown as McpServer;
  for (const tool of TOOLS) {
    tool.register(server, () => Promise.resolve({ content: [] }));
  }
  return inputs;
}

/** Every string a JSON Schema takes as a value (`enum`, `const`), at any depth. */
function schemaValues(schema: unknown, into: Set<string>): Set<string> {
  if (Array.isArray(schema)) {
    for (const item of schema) {
      schemaValues(item, into);
    }
  } else if (typeof schema === "object" && schema !== null) {
    for (const [key, value] of Object.entries(schema)) {
      if (key === "const" && typeof value === "string") {
        into.add(value);
      } else if (key === "enum" && Array.isArray(value)) {
        value.filter((item): item is string => typeof item === "string").forEach((item) => into.add(item));
      } else {
        schemaValues(value, into);
      }
    }
  }
  return into;
}

const TOOL_NAMES: ReadonlySet<string> = new Set(TOOLS.map((tool) => tool.name));
/** A tool's call written with an object argument: `add_cards({board, page, cards: …})`, `check({board})`. */
const CALL_WITH_ARGUMENTS = new RegExp(`\\b(${[...TOOL_NAMES].join("|")})\\(\\{`, "g");
/** Each tool's top-level arguments, by tool name. */
const ARGUMENTS: ReadonlyMap<string, ReadonlySet<string>> = new Map(
  [...registered()].map(([name, input]) => [name, new Set(Object.keys((z.toJSONSchema(input) as { properties?: object }).properties ?? {}))]),
);
/** The tools' names and every value their arguments take (op names, repair names, kinds). */
const VOCABULARY: ReadonlySet<string> = new Set([...TOOL_NAMES, ...[...registered().values()].flatMap((input) => [...schemaValues(z.toJSONSchema(input), new Set())])]);

/** The words in `text` written like tool names that aren't a tool's or one of its arguments' values. */
function unknownWords(text: string, known: ReadonlySet<string>): string[] {
  return [...new Set(text.match(SNAKE) ?? [])].filter((word) => !known.has(word));
}

/** The calls in `text` that aren't tools. */
function unknownCalls(text: string): string[] {
  return [...text.matchAll(CALL)].map((match) => match[1] ?? "").filter((name) => !TOOL_NAMES.has(name));
}

/**
 * The names written at the top level of the object that opens at `text[open]`: `key: value` and
 * shorthand `key`. Quoted text and `<placeholders>` are skipped, and nested objects and lists aren't
 * read.
 */
function topLevelNames(text: string, open: number): string[] {
  const names: string[] = [];
  let depth = 0;
  let expectName = false;
  for (let index = open; index < text.length; index += 1) {
    const char = text.charAt(index);
    if (char === '"') {
      index = text.indexOf('"', index + 1);
      if (index < 0) {
        break;
      }
    } else if (char === "<") {
      index = text.indexOf(">", index + 1);
      if (index < 0) {
        break;
      }
    } else if ("{[(".includes(char)) {
      depth += 1;
      expectName = depth === 1;
    } else if ("}])".includes(char)) {
      depth -= 1;
      if (depth === 0) {
        break;
      }
    } else if (char === "," && depth === 1) {
      expectName = true;
    } else if (expectName && /[A-Za-z_]/.test(char)) {
      const name = /^[A-Za-z_]\w*/.exec(text.slice(index))?.[0] ?? "";
      names.push(name);
      index += name.length - 1;
      expectName = false;
    } else if (!/\s/.test(char)) {
      expectName = false;
    }
  }
  return names;
}

/** The arguments written in `text`'s calls (`tool({name: …})`) that their tool doesn't take, as `tool.name`. */
function unknownArguments(text: string): string[] {
  return [...text.matchAll(CALL_WITH_ARGUMENTS)].flatMap((match) => {
    const tool = match[1] ?? "";
    const taken = ARGUMENTS.get(tool) ?? new Set<string>();
    return topLevelNames(text, match.index + match[0].length - 1)
      .filter((name) => !taken.has(name))
      .map((name) => `${tool}.${name}`);
  });
}

/** The guide topics `text` names that don't exist. */
function unknownTopics(text: string): string[] {
  return [...text.matchAll(TOPIC_NAMED)].map((match) => match[1] ?? match[2] ?? match[3] ?? "").filter((topic) => !(GUIDE_TOPICS as readonly string[]).includes(topic));
}

describe("the instructions", () => {
  it("fit Claude Code's limit, and their first paragraph stands alone within Codex's", () => {
    expect(INSTRUCTIONS.length).toBeLessThanOrEqual(CLAUDE_CODE_LIMIT);
    const [first = ""] = INSTRUCTIONS.split("\n\n");
    expect(first.length).toBeLessThanOrEqual(CODEX_LEAD);
    expect(first.startsWith("Unfold compiles")).toBe(true);
    expect(first.endsWith('Call get_guide({topic: "start"}) before any other Unfold tool.')).toBe(true);
  });

  it("name only tools, arguments, and topics that exist", () => {
    expect(unknownWords(INSTRUCTIONS, VOCABULARY)).toEqual([]);
    expect(unknownArguments(INSTRUCTIONS)).toEqual([]);
    expect(unknownTopics(INSTRUCTIONS)).toEqual([]);
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

  it("name only tools, arguments' values, and guide topics that exist", () => {
    for (const tool of TOOLS) {
      expect(unknownWords(tool.description, VOCABULARY), tool.name).toEqual([]);
      expect(unknownTopics(tool.description), tool.name).toEqual([]);
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
});

describe("the guide", () => {
  const topics = GUIDE_TOPICS.map((topic): [GuideTopic, string] => [topic, guideText(topic)]);

  it("has every topic written, as a Markdown document that fits one result", () => {
    expect(guideText("start").startsWith("# Working in Unfold\n\n## What Unfold is")).toBe(true);
    expect(guideText("start")).toContain("## Review checkpoints (mandatory)");
    for (const [topic, text] of topics) {
      expect(text, topic).toMatch(/^# \S.*\n\n\S/);
      expect(text.length, topic).toBeLessThanOrEqual(MAX_RESULT_CHARS - HEADER_ROOM);
    }
  });

  it("calls only tools with arguments they take, names only tools and their arguments' values, and points only at topics that exist", () => {
    for (const [topic, text] of topics) {
      expect(unknownCalls(text), topic).toEqual([]);
      expect(unknownArguments(text), topic).toEqual([]);
      expect(unknownWords(text, new Set([...VOCABULARY, ...OTHER_WORDS])), topic).toEqual([]);
      expect(unknownTopics(text), topic).toEqual([]);
    }
  });

  it("teaches every tool somewhere, and the start topic lists every other topic", () => {
    const words = new Set(topics.flatMap(([, text]) => text.match(/\b[a-z_]+\b/g) ?? []));
    expect(TOOLS.map((tool) => tool.name).filter((name) => !words.has(name))).toEqual([]);
    for (const topic of GUIDE_TOPICS.filter((name) => name !== "start")) {
      expect(guideText("start"), topic).toContain(`- ${topic}: `);
    }
  });
});
