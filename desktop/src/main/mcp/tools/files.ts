/**
 * `read_file`: a file's verified text and its definitions, from `code_view`'s file
 * target, asking only for the lines it shows. Text is shown only when it is what Unfold compiled or
 * recorded (`current`): what boards may cite. Evidence files read as plain text.
 */

import { z } from "zod";
import type { CodeView } from "../engine-types";
import { count } from "../format";
import { cant, fittingLines, MAX_RESULT_CHARS, RESULT_ROOM, textResult } from "../results";
import { projectOrView } from "../target";
import { defineTool } from "../tool";

/** The lines one result shows by default, and at most. */
const DEFAULT_LINES = 200;
const MAX_LINES = 400;
/** The most definitions one result lists. */
const MAX_OUTLINE = 150;

/** The view's lines, numbered; a line too long to show whole ends in `…`. */
function textLines(view: CodeView): string[] {
  const width = String(view.lines.at(-1)?.number ?? 0).length;
  return view.lines.map((line) => `${String(line.number).padStart(width)} | ${line.text}${line.cut ? "…" : ""}`);
}

export const readFile = defineTool({
  name: "read_file",
  title: "Read file",
  description:
    "A file's verified text with line numbers, and the definitions it holds (each with its first line and key). path is relative to the project's folder. lines [from, to] picks the range (default the first 200 lines, at most 400 at a time, fewer when lines are long), and the definitions listed are those that start in it; pass the next range for more. A line longer than 1,000 bytes ends in … where it is cut. An evidence file (a migration, schema, or config listed in the project's evidence files) reads as plain text with line numbers, so resource cards and connections can cite it as {file, lines}. Unfold shows text only when it is the file it compiled or recorded: a file that changed needs compile_project first, and a file outside the graph needs adding as an evidence file (update_project_settings, add_evidence_pattern) before boards can cite it. Reading changes nothing.",
  input: z.strictObject({
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
    const from = lines?.[0] ?? 1;
    const asked: [number, number] = [from, Math.min(lines?.[1] ?? from + DEFAULT_LINES - 1, from + MAX_LINES - 1)];
    const view = await context.engine("code_view", { project: row.id, target: { kind: "file", path }, window: asked });
    switch (view.state) {
      case "notFound":
        throw cant("read the file", `${row.name} has no file ${path}`, "search finds files by name; paths are relative to the project's folder.");
      case "changed":
        throw cant("read the file", `${path} changed since the last compile, so its text isn't what Unfold compiled`, `compile_project ${row.name}, then read it again.`);
      case "notAnalyzed":
        throw cant(
          "read the file",
          `Unfold doesn't compile or record ${path}, so it can't verify its text`,
          "If boards should cite it (a migration, a schema, a config), ask the person, add it with update_project_settings (add_evidence_pattern), compile, then read it again.",
        );
      case "current":
      case "doesNotParse":
        break;
    }
    const last = view.span?.[1] ?? 0;
    const shown: [number, number] = [from, Math.min(last, asked[1])];
    const definitions = view.definitions.map((definition) => `- line ${definition.line}: key ${definition.key}`);
    const shownDefinitions = definitions.slice(0, MAX_OUTLINE);
    const outlineText =
      definitions.length > 0
        ? `Definitions${shown[0] === 1 && shown[1] === last ? "" : ` in lines ${shown[0]}–${shown[1]}`}${shownDefinitions.length < definitions.length ? ` (the first ${MAX_OUTLINE} of ${definitions.length}; pass lines for a part)` : ""}:\n${shownDefinitions.join("\n")}`
        : null;
    const text = fittingLines(textLines(view), MAX_RESULT_CHARS - RESULT_ROOM - (outlineText?.length ?? 0));
    const to = Math.max(from, from + text.length - 1);
    const shownText =
      text.length > 0 ? `Lines ${from}–${to}:\n${text.join("\n")}` : `It ends at line ${last}: read_file with lines [${Math.max(1, last - DEFAULT_LINES + 1)}, ${last}] reads its last lines.`;
    return textResult(
      context.header({ project: row.name }),
      `${path}: ${count(last, "line")}${view.state === "doesNotParse" ? "; it doesn't parse now, so its definitions are the last ones that did" : ""}.`,
      outlineText,
      last === 0 ? "The file is empty." : shownText,
      text.length > 0 && to < last ? `More: read_file with lines [${to + 1}, ${Math.min(last, to + DEFAULT_LINES)}].` : null,
      definitions.length > 0 ? "Next: read_code with a key reads a definition's compiled tree." : null,
    );
  },
});
