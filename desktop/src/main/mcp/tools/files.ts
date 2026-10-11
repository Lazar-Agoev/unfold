/**
 * `read_file` (design §6.4): a file's outline and a range of its text, from `file_view`. Text is shown
 * only when it is what Unfold compiled (`current`) or recorded (`evidence`): what boards may cite.
 */

import { z } from "zod";
import type { FileView, OutlineItem } from "../engine-types";
import { count, keyName } from "../format";
import { cant, fittingLines, MAX_RESULT_CHARS, RESULT_ROOM, textResult } from "../results";
import { projectOrView } from "../target";
import { defineTool } from "../tool";

/** The lines one result shows by default, and at most. */
const DEFAULT_LINES = 200;
const MAX_LINES = 400;
/** The most outline entries one result lists. */
const MAX_OUTLINE = 150;
const DEFINITION_KINDS: ReadonlySet<string> = new Set(["function", "method", "constructor", "getter", "setter", "class", "interface", "type_alias", "enum"]);

/** The file's definitions and types, and its top-level variables, indented by nesting, overlapping `[from, to]`. */
function outlineLines(outline: readonly OutlineItem[], [from, to]: readonly [number, number]): string[] {
  const listed = (item: OutlineItem) => DEFINITION_KINDS.has(item.kind) || (item.kind === "variable" && item.parent === undefined);
  const depth = (item: OutlineItem): number => {
    const parent = item.parent === undefined ? undefined : outline[item.parent];
    return parent === undefined ? 0 : depth(parent) + (listed(parent) ? 1 : 0);
  };
  return outline
    .filter((item) => listed(item) && item.startLine <= to && item.endLine >= from)
    .map((item) => {
      const lines = item.startLine === item.endLine ? `${item.startLine}` : `${item.startLine}–${item.endLine}`;
      return `${"  ".repeat(depth(item))}- ${lines} ${item.kind} ${keyName(item.key)}: key ${item.key}`;
    });
}

function textLines(view: FileView, [from, to]: readonly [number, number]): string[] {
  const width = String(to).length;
  return view.lines
    .filter((line) => line.number >= from && line.number <= to)
    .map((line) => `${String(line.number).padStart(width)} | ${view.text.slice(line.start - view.textStart, line.end - view.textStart)}`);
}

export const readFile = defineTool({
  name: "read_file",
  title: "Read file",
  description:
    "A file's outline (its functions, classes, methods, types, and top-level variables, with keys and lines) and a range of its verified text with line numbers. path is relative to the project's folder. lines [from, to] picks the range (default the first 200 lines, at most 400 at a time, fewer when lines are long) and narrows the outline to it; pass the next range for more. An evidence file (a migration, schema, or config listed in the project's evidence files) reads as plain text with line numbers, so resource cards and connections can cite it as {file, lines}. Unfold shows text only when it is the file it compiled or recorded: a file that changed needs compile_project first, and a file outside the graph needs adding as an evidence file (update_project_settings, add_evidence_pattern) before boards can cite it. Reading changes nothing.",
  input: z.object({
    project: z.string().min(1).optional().describe("The project's name or id (default: the project of the board in view)"),
    path: z.string().min(1).describe("The file, relative to the project's folder"),
    lines: z
      .tuple([z.number().int().min(1), z.number().int().min(1)])
      .refine(([from, to]) => from <= to, "lines is [from, to] with from ≤ to")
      .optional()
      .describe("The lines to show, 1-based and inclusive"),
  }),
  annotations: { readOnlyHint: true },
  async run({ project, path, lines }, context) {
    const row = await projectOrView(context, project);
    const view = await context.engine("file_view", { project: row.id, path });
    if (view.unshowable !== null) {
      throw cant("read the file", view.unshowable === "binary" ? `${path} isn't text` : `${path} is larger than 8 MiB`, "Read a smaller file, or search its text.");
    }
    if (view.graph === "not_compiled") {
      throw cant("read the file", `${path} changed since the last compile, so its text isn't what Unfold compiled`, `compile_project ${row.name}, then read it again.`);
    }
    if (view.graph === "not_indexed") {
      throw cant(
        "read the file",
        `Unfold doesn't compile or record ${path}, so it can't verify its text`,
        "If boards should cite it (a migration, a schema, a config), ask the person, add it with update_project_settings (add_evidence_pattern), compile, then read it again.",
      );
    }
    const last = view.lineCount;
    const from = Math.min(lines?.[0] ?? 1, Math.max(last, 1));
    const asked: [number, number] = [from, Math.min(last, lines?.[1] ?? from + DEFAULT_LINES - 1, from + MAX_LINES - 1)];
    const outline = view.graph === "current" ? outlineLines(view.outline, lines === undefined ? [1, last] : asked) : [];
    const shownOutline = outline.slice(0, MAX_OUTLINE);
    const outlineText =
      view.graph === "current"
        ? outline.length > 0
          ? `Outline${lines === undefined ? "" : ` of lines ${asked[0]}–${asked[1]}`}${shownOutline.length < outline.length ? ` (the first ${MAX_OUTLINE} of ${outline.length}; pass lines for a part)` : ""}:\n${shownOutline.join("\n")}`
          : "No definitions."
        : null;
    const text = fittingLines(textLines(view, asked), MAX_RESULT_CHARS - RESULT_ROOM - (outlineText?.length ?? 0));
    const to = Math.max(asked[0], asked[0] + text.length - 1);
    return textResult(
      context.header({ project: row.name }),
      `${path}: ${count(last, "line")}${view.graph === "evidence" ? ", an evidence file (plain text, verified against the bytes Unfold recorded; cite lines as {file, lines})" : view.language ? `, ${view.language === "typescript" ? "TypeScript" : "Python"}` : ""}.`,
      outlineText,
      last === 0 ? "The file is empty." : `Lines ${asked[0]}–${to}:\n${text.join("\n")}`,
      to < last ? `More: read_file with lines [${to + 1}, ${Math.min(last, to + DEFAULT_LINES)}].` : null,
      view.graph === "current" ? "Next: read_code with a key reads a definition's steps and calls." : null,
    );
  },
});
