/**
 * Find code (design §6.4): `search` and `list_entrances`.
 *
 * `search` asks the engine's name search for entrances, functions, types (each a definition search
 * narrowed to their block kinds, so each has its own total), and files, and its streamed text search
 * for text. A text hit names the function or type it's in: the innermost definition of the file's
 * outline at that line (`file_view` of that one line), so it's ready to place. A cursor is an offset
 * into every list at once.
 */

import { z } from "zod";
import type { ToolContext } from "../context";
import type { BlockKind, EntranceRow, ProjectRow, SearchHit, TextMatch } from "../engine-types";
import { clip, count, keyName, place } from "../format";
import { cant, cursorOffset, textResult } from "../results";
import { projectOrView } from "../target";
import { defineTool } from "../tool";

const FUNCTION_KINDS: BlockKind[] = ["function", "method", "constructor", "getter", "setter"];
const TYPE_KINDS: BlockKind[] = ["interface", "type_alias", "enum", "class"];
/** What a text hit can be inside of: a function or a type. */
const DEFINITION_KINDS: ReadonlySet<string> = new Set<string>([...FUNCTION_KINDS, ...TYPE_KINDS]);
/** The engine's most hits per kind, and most entrances per application. */
const MAX_ENGINE_LIMIT = 500;

const KINDS = ["entrances", "functions", "types", "files", "text"] as const;
type Kind = (typeof KINDS)[number];

/** One page of one list: its heading with what is shown of the total, its lines, and whether there's more. */
interface Section {
  readonly lines: readonly string[];
  readonly more: boolean;
}

/** The page of `hits` from `offset` (at most `limit` of them), under `title`; nothing when the list is empty. */
function section<T>(title: string, hits: readonly T[], total: number, offset: number, limit: number, line: (hit: T) => string): Section {
  const shown = hits.slice(offset, offset + limit);
  return pageSection(title, shown.map(line), total, offset, limit);
}

/** A page already made (`shown` lines from `offset`), under `title`; nothing when the list is empty. */
function pageSection(title: string, shown: readonly string[], total: number, offset: number, limit: number): Section {
  if (total === 0) {
    return { lines: [], more: false };
  }
  const range = shown.length === 0 ? `none past ${offset} of ${total}` : shown.length < total ? `${offset + 1}–${offset + shown.length} of ${total}` : `${total}`;
  return { lines: [`${title} (${range}):`, ...shown], more: offset + limit < Math.min(total, MAX_ENGINE_LIMIT) };
}

/** The line that says how to get the next page, when any list has more. */
function moreLine(sections: readonly Section[], tool: string, next: number): string | null {
  return sections.some((item) => item.more) ? `More: ${tool} with cursor "${next}" for the next page.` : null;
}

/** One text match, with the file it's in. */
interface TextHit {
  readonly project: string;
  readonly path: string;
  readonly match: TextMatch;
}

/** The innermost function or type around a text hit's line; `changed` when the file changed since the compile. */
async function ownerAt(context: ToolContext, hit: TextHit): Promise<{ readonly key: string; readonly kind: string } | "changed" | null> {
  const line = hit.match.line;
  const view = await context.engine("file_view", { project: hit.project, path: hit.path, range: { startLine: line, endLine: line } }).catch(() => null);
  if (view?.graph === "not_compiled") {
    return "changed";
  }
  const around = (view?.outline ?? []).filter((item) => DEFINITION_KINDS.has(item.kind) && item.startLine <= line && line <= item.endLine);
  const innermost = around.reduce<(typeof around)[number] | undefined>((best, item) => (best === undefined || item.start >= best.start ? item : best), undefined);
  return innermost ? { key: innermost.key, kind: innermost.kind } : null;
}

/** The text hits shown, each with the function it's in. */
async function textLines(context: ToolContext, hits: readonly TextHit[], tag: (project: string) => string): Promise<string[]> {
  const owners = await Promise.all(hits.map((hit) => ownerAt(context, hit)));
  return hits.map((hit, index) => {
    const owner = owners[index];
    const inside =
      owner === "changed"
        ? " (the file changed since the last compile: compile to name its function)"
        : owner
          ? ` in ${owner.kind} ${keyName(owner.key)} (key ${owner.key})`
          : "";
    return `- ${place(hit.path, hit.match.line)}${tag(hit.project)}${inside}: ${clip(hit.match.text.trim())}`;
  });
}

function hitLine(hit: SearchHit, tag: string): string {
  const where = place(hit.path, hit.line);
  if (hit.key.startsWith("entrance:")) {
    return `- ${hit.name} (${hit.kind}${hit.application !== null ? `, ${hit.application}` : ""}) declared in ${where}${tag}: key ${hit.key}`;
  }
  return `- ${hit.qualifiedName || hit.name} (${hit.kind}) at ${where}${tag}: key ${hit.key}`;
}

export const search = defineTool({
  name: "search",
  title: "Search",
  description:
    "Finds code by name or by text, in one project or all of them. kinds (default: all but text): entrances (routes, workers, schedules, commands; by title, trigger, or path), functions (functions, methods, constructors, accessors), types (interfaces, type aliases, enums, classes), files (by path), and text (lines of any file, as written; with regex, caseSensitive, and wholeWord). Names match fuzzily: every word of query must appear in a name, qualified name, or path. Each hit has Unfold's name, file:line, and its key: pass the key to read_code, get_relations, or add_cards. A text hit names the function it's in with that function's key, ready to place, and says when the file changed since the last compile. limit is per kind (default 10, at most 50); pass the next cursor for more. projects narrows the search (names or ids). Searching never wakes a resting project; reading a hit does.",
  input: z.object({
    query: z.string().min(1).describe("Words of a name, or the text to find (a regular expression with regex)"),
    kinds: z.array(z.enum(KINDS)).min(1).optional().describe("What to find (default: entrances, functions, types, files)"),
    projects: z.array(z.string().min(1)).min(1).optional().describe("Projects to look in, by name or id (default: all)"),
    regex: z.boolean().optional().describe("text only: query is a regular expression"),
    caseSensitive: z.boolean().optional().describe("text only: match case exactly"),
    wholeWord: z.boolean().optional().describe("text only: match whole words only"),
    limit: z.number().int().min(1).max(50).default(10).describe("Hits per kind"),
    cursor: z.string().optional().describe("The cursor an earlier page gave"),
  }),
  annotations: { readOnlyHint: true },
  async run({ query, kinds, projects, regex, caseSensitive, wholeWord, limit, cursor }, context) {
    const wanted = new Set<Kind>(kinds ?? ["entrances", "functions", "types", "files"]);
    if ((regex !== undefined || caseSensitive !== undefined || wholeWord !== undefined) && !wanted.has("text")) {
      throw cant("search", "regex, caseSensitive, and wholeWord apply to text only", 'Add "text" to kinds, or leave them out.');
    }
    const offset = cursorOffset(cursor);
    const rows: readonly ProjectRow[] = projects === undefined ? await context.projects() : await Promise.all(projects.map((name) => context.project(name)));
    const ids = rows.map((row) => row.id);
    const engineLimit = Math.min(MAX_ENGINE_LIMIT, offset + limit);
    const names = (definitionKinds: BlockKind[]) => context.engine("search", { query, kinds: ["definitions"], definitionKinds, limit: engineLimit, projects: ids });
    const others = [...(wanted.has("entrances") ? (["entrances"] as const) : []), ...(wanted.has("files") ? (["files"] as const) : [])];
    const texts: TextHit[] = [];
    const [functions, types, rest, text] = await Promise.all([
      wanted.has("functions") ? names(FUNCTION_KINDS) : null,
      wanted.has("types") ? names(TYPE_KINDS) : null,
      others.length > 0 ? context.engine("search", { query, kinds: [...others], limit: engineLimit, projects: ids }) : null,
      wanted.has("text")
        ? context.stream(
            "search_text",
            { query, projects: ids, ...(regex !== undefined ? { regex } : {}), ...(caseSensitive !== undefined ? { caseSensitive } : {}), ...(wholeWord !== undefined ? { wholeWord } : {}) },
            (file) => {
              texts.push(...file.matches.map((match) => ({ project: file.project, path: file.path, match })));
            },
          )
        : null,
    ]);
    const projectNames = new Map(rows.map((row) => [row.id, row.name]));
    const tag = (project: string) => (rows.length > 1 ? ` in ${projectNames.get(project) ?? project}` : "");
    const hits = (title: string, group: { readonly hits: readonly SearchHit[]; readonly total: number } | undefined) =>
      group ? section(title, group.hits, group.total, offset, limit, (hit) => hitLine(hit, tag(hit.project))) : null;
    const textTitle = text ? `Text in ${count(text.files, "file")}${text.truncated ? ", stopped at 2,000 matches (narrow the query)" : ""}` : "";
    const sections = [
      wanted.has("entrances") ? hits("Entrances", rest?.entrances) : null,
      hits("Functions", functions?.definitions),
      hits("Types", types?.definitions),
      wanted.has("files") && rest
        ? section("Files", rest.files.hits, rest.files.total, offset, limit, (hit) => `- ${hit.path}${tag(hit.project)}${hit.definitions === null ? " (not compiled)" : `: ${count(hit.definitions, "definition")}`}`)
        : null,
      text ? pageSection(textTitle, await textLines(context, texts.slice(offset, offset + limit), tag), texts.length, offset, limit) : null,
    ].filter((item): item is Section => item !== null);
    const lines = sections.flatMap((item) => item.lines);
    return textResult(
      context.header(rows.length === 1 && rows[0] ? { project: rows[0].name } : {}),
      lines.length > 0 ? lines.join("\n") : `Nothing matches “${query}”${rows.length > 1 ? " in any project" : ""}. Try fewer or shorter words${wanted.has("text") ? "" : ', or kinds ["text"]'}.`,
      moreLine(sections, "search", offset + limit),
      lines.length > 0 ? "Next: read_code or get_relations with a key; add_cards puts it on a page." : null,
    );
  },
});

// ---------------------------------------------------------------------------------------------
// list_entrances

function entranceLine(entrance: EntranceRow): string {
  const declared = place(entrance.path, entrance.line);
  const reaches = entrance.reaches === null ? "" : `, reaches ${count(entrance.reaches, "definition")}`;
  return `- ${entrance.label ?? entrance.title} (${entrance.kind}, ${entrance.detector}) declared in ${declared}${reaches}: key ${entrance.key}`;
}

export const listEntrances = defineTool({
  name: "list_entrances",
  title: "List entrances",
  description:
    "How a project starts: its applications (named after their startup, such as server or worker), the launches that start each (package scripts and bins, console scripts), and each application's entrances: HTTP routes, queue workers, schedules, commands, and events, each with its key and where it's declared. Then the entrances no launch reaches, and launches into code Unfold doesn't index. application narrows it to one (its name or startup key, as get_project lists them); filter keeps entrances whose title, trigger, label, or path contains it, ignoring case. limit is per application (default 25, at most 100); pass the next cursor for more. Pass an entrance's key to read_code (its evidence and handlers), add_cards, or create_board's trace. The project must be compiled.",
  input: z.object({
    project: z.string().min(1).optional().describe("The project's name or id (default: the project of the board in view)"),
    application: z.string().min(1).optional().describe("An application's name or startup key"),
    filter: z.string().min(1).optional().describe("Text an entrance's title, trigger, label, or path contains"),
    limit: z.number().int().min(1).max(100).default(25).describe("Entrances per application"),
    cursor: z.string().optional().describe("The cursor an earlier page gave"),
  }),
  annotations: { readOnlyHint: true },
  async run({ project, application, filter, limit, cursor }, context) {
    const row = await projectOrView(context, project);
    const offset = cursorOffset(cursor);
    const map = await context.engine("entrances", { project: row.id, perApplication: Math.min(MAX_ENGINE_LIMIT, offset + limit), ...(filter !== undefined ? { filter } : {}) });
    const applications = application === undefined ? map.applications : map.applications.filter((item) => item.name === application || item.startup === application);
    if (application !== undefined && applications.length === 0) {
      throw cant("list the entrances", `${row.name} has no application “${application}”`, `Its applications: ${map.applications.map((item) => item.name).join(", ") || "none"}.`);
    }
    const matching = filter === undefined ? "" : ` matching “${filter}”`;
    const sections = [
      ...applications.map((item) => {
        const launches = item.launches.map((launch) => `${launch.title} (${launch.path})`).join(", ");
        return section(`${item.name} (startup ${item.path}${launches.length > 0 ? `; started by ${launches}` : ""}), entrances${matching}`, item.entrances, item.total, offset, limit, entranceLine);
      }),
      ...(application === undefined ? [section(`Not reached from a launch${matching}`, map.unreached.entrances, map.unreached.total, offset, limit, entranceLine)] : []),
    ];
    const unindexed = application === undefined && map.unindexedLaunches.length > 0 ? `Launches into code Unfold doesn't index: ${map.unindexedLaunches.map((launch) => `${launch.title} (${launch.path})`).join(", ")}.` : null;
    const lines = sections.flatMap((item) => item.lines);
    return textResult(
      context.header({ project: row.name }),
      lines.length > 0 ? lines.join("\n") : `${row.name} has no entrances${matching}.`,
      unindexed,
      moreLine(sections, "list_entrances", offset + limit),
      lines.length > 0 ? "Next: read_code with an entrance's key shows its handlers; create_board with trace makes a board from it." : null,
    );
  },
});
