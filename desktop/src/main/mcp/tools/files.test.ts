import { describe, expect, it, vi } from "vitest";
import type { CodeView } from "../engine-types";
import { MAX_RESULT_CHARS } from "../results";
import { readFile } from "./files";
import { harness, PROJECT } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

/**
 * A file of `count` lines, `line N` each (padded to `width`), as `code_view` shows a file: the lines of
 * the window asked for (the first 200 without one), and the definitions that start on them.
 */
function wholeFile(count: number, state: CodeView["state"], width = 0, definitions: CodeView["definitions"] = []) {
  return (args: Readonly<Record<string, unknown>>): CodeView => {
    const [from, to] = (args["window"] as [number, number] | undefined) ?? [1, 200];
    const shown = (number: number) => number >= from && number <= Math.min(to, count);
    const lines = Array.from({ length: count }, (_, index) => ({ number: index + 1, text: `line ${index + 1}`.padEnd(width, "-"), tokens: [], calls: [] })).filter((line) => shown(line.number));
    return { file: "db/schema.prisma", state, ...(count > 0 ? { span: [1, count] } : {}), lines, path: [], definitions: definitions.filter((definition) => shown(definition.line)) };
  };
}

describe("read_file", () => {
  it("lists a compiled file's definitions with keys, and its verified text", async () => {
    const definitions = [
      { line: 11, key: "src/routes/orders.ts#listOrders" },
      { line: 25, key: "src/routes/orders.ts#createOrder" },
    ];
    const app = harness({ engine: { code_view: (args) => ({ ...wholeFile(40, "current", 0, definitions)(args), file: "src/routes/orders.ts" }) } });
    const { text } = await app.call(readFile, { project: "ts-orders", path: "src/routes/orders.ts" });
    expect(app.engineCalls.at(-1)).toEqual(["code_view", { project: PROJECT, target: { kind: "file", path: "src/routes/orders.ts" }, window: [1, 200] }]);
    const lines = text.split("\n");
    expect(lines.slice(0, 5)).toEqual([
      "Unfold · ts-orders · as Claude",
      "src/routes/orders.ts: 40 lines.",
      "Definitions:",
      "- line 11: key src/routes/orders.ts#listOrders",
      "- line 25: key src/routes/orders.ts#createOrder",
    ]);
    expect(lines).toContain("Lines 1–40:");
    expect(lines).toContain(" 1 | line 1");
    expect(lines.at(-1)).toBe("Next: read_code with a key reads a definition's compiled tree.");
    const part = await app.call(readFile, { path: "src/routes/orders.ts", lines: [20, 30] });
    expect(app.engineCalls.at(-1)?.[1]).toMatchObject({ window: [20, 30] });
    expect(part.text).toContain("Definitions in lines 20–30:\n- line 25: key src/routes/orders.ts#createOrder\nLines 20–30:");
  });

  it("asks only for the lines it shows, at most 400, and says where a file ends", async () => {
    const app = harness({ engine: { code_view: wholeFile(1_000_000, "current") } });
    const { text } = await app.call(readFile, { path: "logs/run.log", lines: [500_001, 999_999] });
    expect(app.engineCalls.at(-1)?.[1]).toMatchObject({ window: [500_001, 500_400] });
    expect(text).toContain("logs/run.log: 1000000 lines.");
    expect(text).toContain("Lines 500001–");
    const short = harness({ engine: { code_view: wholeFile(40, "current") } });
    expect((await short.call(readFile, { path: "db/schema.prisma", lines: [90, 99] })).text).toContain(
      "It ends at line 40: read_file with lines [1, 40] reads its last lines.",
    );
  });

  it("ends a line that is cut with …", async () => {
    const cut = (args: Readonly<Record<string, unknown>>): CodeView => {
      const view = wholeFile(2, "current")(args);
      return { ...view, lines: view.lines.map((line) => (line.number === 2 ? { ...line, cut: true } : line)) };
    };
    const app = harness({ engine: { code_view: cut } });
    expect((await app.call(readFile, { path: "data.json" })).text).toContain("1 | line 1\n2 | line 2…");
  });

  it("reads evidence files as plain text, in pages", async () => {
    const app = harness({ engine: { code_view: wholeFile(450, "current") } });
    const first = await app.call(readFile, { path: "db/schema.prisma" });
    expect(first.text).toContain("db/schema.prisma: 450 lines.");
    expect(first.text).toContain("Lines 1–200:\n  1 | line 1");
    expect(first.text).toContain("More: read_file with lines [201, 400].");
    expect(first.text).not.toContain("Definitions");
    const last = await app.call(readFile, { path: "db/schema.prisma", lines: [401, 999] });
    expect(last.text).toContain("Lines 401–450:\n401 | line 401");
    expect(last.text).not.toContain("More:");
  });

  it("ends a page of long lines early, so the size guard never cuts where the next page starts", async () => {
    const app = harness({ engine: { code_view: wholeFile(450, "current", 200) } });
    const { text } = await app.call(readFile, { path: "db/schema.prisma", lines: [1, 400] });
    expect(text.length).toBeLessThan(MAX_RESULT_CHARS);
    expect(text).not.toContain("Cut here");
    const shown = /Lines 1–(\d+):/.exec(text)?.[1];
    expect(Number(shown)).toBeLessThan(400);
    expect(text).toContain(`More: read_file with lines [${Number(shown) + 1}, ${Number(shown) + 200}].`);
  });

  it("shows no text Unfold can't verify, and says what to do", async () => {
    const changed = harness({ engine: { code_view: wholeFile(3, "changed") } });
    expect((await changed.call(readFile, { path: "src/a.ts" })).text).toBe(
      "Can't read the file: src/a.ts changed since the last compile, so its text isn't what Unfold compiled. compile_project ts-orders, then read it again.",
    );
    const outside = harness({ engine: { code_view: wholeFile(3, "notAnalyzed") } });
    expect((await outside.call(readFile, { path: "db/schema.prisma" })).text).toContain("add it with update_project_settings (add_evidence_pattern)");
    const missing = harness({ engine: { code_view: wholeFile(0, "notFound") } });
    expect((await missing.call(readFile, { path: "logo.png" })).text).toBe(
      "Can't read the file: ts-orders has no file logo.png. search finds files by name; paths are relative to the project's folder.",
    );
  });
});
