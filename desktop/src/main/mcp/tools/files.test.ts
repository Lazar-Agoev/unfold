import { describe, expect, it, vi } from "vitest";
import fileViewPage from "../../../../../testdata/workspace/commands/file_view_orders_page.json";
import type { FileView } from "../engine-types";
import { MAX_RESULT_CHARS } from "../results";
import { readFile } from "./files";
import { harness, PROJECT, recorded } from "./testing";

vi.mock("electron", async () => (await import("../../fake-electron")).electronModule);

/** A whole file of `count` lines, `line N` each (padded to `width`), as `file_view` returns it. */
function wholeFile(count: number, graph: FileView["graph"], width = 0): FileView {
  const texts = Array.from({ length: count }, (_, index) => `line ${index + 1}`.padEnd(width, "-"));
  let at = 0;
  const lines = texts.map((text, index) => {
    const line = { number: index + 1, start: at, end: at + text.length, indent: 0 };
    at += text.length + 1;
    return line;
  });
  return { ...recorded<FileView>(fileViewPage), path: "db/schema.prisma", language: null, graph, lineCount: count, startLine: 1, endLine: count, textStart: 0, text: texts.join("\n"), lines, outline: [], links: [], spans: [] };
}

describe("read_file", () => {
  it("lists a compiled file's definitions with keys, and its verified text", async () => {
    const page = recorded<FileView>(fileViewPage);
    const app = harness({ engine: { file_view: () => ({ ...page, lineCount: 12 }) } });
    const { text } = await app.call(readFile, { project: "ts-orders", path: "src/routes/orders.ts" });
    expect(app.engineCalls.at(-1)).toEqual(["file_view", { project: PROJECT, path: "src/routes/orders.ts" }]);
    const lines = text.split("\n");
    expect(lines.slice(0, 5)).toEqual([
      "Unfold · ts-orders · as Claude",
      "src/routes/orders.ts: 12 lines, TypeScript.",
      "Outline:",
      "- 9 variable ordersRouter: key src/routes/orders.ts#variable:ordersRouter",
      "- 11–13 function listOrders: key src/routes/orders.ts#function:listOrders",
    ]);
    expect(lines).toContain("Lines 1–12:");
    expect(lines).toContain(' 1 | import express, { type Request, type Response, type Router } from "express";');
    expect(lines.at(-1)).toBe("Next: read_code with a key reads a definition's steps and calls.");
  });

  it("reads evidence files as plain text, in pages", async () => {
    const app = harness({ engine: { file_view: () => wholeFile(450, "evidence") } });
    const first = await app.call(readFile, { path: "db/schema.prisma" });
    expect(first.text).toContain("db/schema.prisma: 450 lines, an evidence file (plain text, verified against the bytes Unfold recorded; cite lines as {file, lines}).");
    expect(first.text).toContain("Lines 1–200:\n  1 | line 1");
    expect(first.text).toContain("More: read_file with lines [201, 400].");
    expect(first.text).not.toContain("Outline");
    const last = await app.call(readFile, { path: "db/schema.prisma", lines: [401, 999] });
    expect(last.text).toContain("Lines 401–450:\n401 | line 401");
    expect(last.text).not.toContain("More:");
  });

  it("ends a page of long lines early, so the size guard never cuts where the next page starts", async () => {
    const app = harness({ engine: { file_view: () => wholeFile(450, "evidence", 200) } });
    const { text } = await app.call(readFile, { path: "db/schema.prisma", lines: [1, 400] });
    expect(text.length).toBeLessThan(MAX_RESULT_CHARS);
    expect(text).not.toContain("Cut here");
    const shown = /Lines 1–(\d+):/.exec(text)?.[1];
    expect(Number(shown)).toBeLessThan(400);
    expect(text).toContain(`More: read_file with lines [${Number(shown) + 1}, ${Number(shown) + 200}].`);
  });

  it("shows no text Unfold can't verify, and says what to do", async () => {
    const changed = harness({ engine: { file_view: () => wholeFile(3, "not_compiled") } });
    expect((await changed.call(readFile, { path: "src/a.ts" })).text).toBe(
      "Can't read the file: src/a.ts changed since the last compile, so its text isn't what Unfold compiled. compile_project ts-orders, then read it again.",
    );
    const outside = harness({ engine: { file_view: () => wholeFile(3, "not_indexed") } });
    expect((await outside.call(readFile, { path: "db/schema.prisma" })).text).toContain("add it with update_project_settings (add_evidence_pattern)");
    const binary = harness({ engine: { file_view: () => ({ ...wholeFile(0, "not_indexed"), unshowable: "binary" }) } });
    expect((await binary.call(readFile, { path: "logo.png" })).text).toBe("Can't read the file: logo.png isn't text. Read a smaller file, or search its text.");
  });
});
