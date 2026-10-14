/**
 * Find code: `search` and `list_entrances`.
 *
 * `search` asks the engine's name search for entrances, functions, types (each a definition search
 * narrowed to their block kinds, so each has its own total), and files. A cursor is an offset into
 * every list at once.
 */

import { z } from "zod";
import type { AppView, EntranceCode, EntranceView, NodeKind, ProjectRow, SearchHit } from "../engine-types";
import { count, place } from "../format";
import { cursorOffset, MAX_RESULT_CHARS, RESULT_ROOM, textResult } from "../results";
import { projectOrView } from "../target";
import { defineTool } from "../tool";

const FUNCTION_KINDS: NodeKind[] = ["function"];
const TYPE_KINDS: NodeKind[] = ["type", "class"];
/** The engine's most hits per kind. */
const MAX_ENGINE_LIMIT = 500;

const KINDS = ["entrances", "functions", "types", "files"] as const;
type Kind = (typeof KINDS)[number];

/** One page of one list: its heading with what is shown of the total, its lines, and whether there's more. */
interface Section {
  readonly lines: readonly string[];
  readonly more: boolean;
}

/** The page of `hits` from `offset` (at most `limit` of them), under `title`; nothing when the list is empty. */
function section<T>(title: string, hits: readonly T[], total: number, offset: number, limit: number, line: (hit: T) => string): Section {
  if (total === 0) {
    return { lines: [], more: false };
  }
  const shown = hits.slice(offset, offset + limit);
  const range = shown.length === 0 ? `none past ${offset} of ${total}` : shown.length < total ? `${offset + 1}–${offset + shown.length} of ${total}` : `${total}`;
  return { lines: [`${title} (${range}):`, ...shown.map(line)], more: offset + limit < Math.min(total, MAX_ENGINE_LIMIT) };
}

/** The line that says how to get the next page, when any list has more. */
function moreLine(sections: readonly Section[], tool: string, next: number): string | null {
  return sections.some((item) => item.more) ? `More: ${tool} with cursor "${next}" for the next page.` : null;
}

/**
 * The sections of the largest page, at most `limit` per list (at least 1), whose lines fit in a
 * result beside `room` characters of other lines, so the size guard never cuts the line that says
 * where the next page starts. One cursor pages every list, so every list shows the same number.
 */
function fitting(limit: number, room: number, build: (limit: number) => readonly Section[]): { readonly sections: readonly Section[]; readonly limit: number } {
  const budget = MAX_RESULT_CHARS - RESULT_ROOM - room;
  for (let size = limit; size > 1; size -= 1) {
    const sections = build(size);
    if (sections.reduce((chars, item) => item.lines.reduce((sum, line) => sum + line.length + 1, chars), 0) <= budget) {
      return { sections, limit: size };
    }
  }
  return { sections: build(1), limit: 1 };
}

function hitLine(hit: SearchHit, tag: string): string {
  return `- ${hit.qualifiedName || hit.name} (${hit.kind}) at ${place(hit.path, hit.line)}${tag}: key ${hit.key}`;
}

function entranceHitLine(hit: SearchHit, tag: string): string {
  return `- ${hit.name} (${hit.kind}${hit.app !== null ? ` of ${hit.app}` : ""})${tag}: key ${hit.key}`;
}

export const search = defineTool({
  name: "search",
  title: "Search",
  description:
    "Finds code by name, in one project or all of them. kinds (default: all): entrances (the apps and entrances unfold.toml declares, by name), functions (functions, methods, constructors, accessors), types (interfaces, type aliases, enums, classes), and files (by path). Names match fuzzily: every word of query must appear in a name, qualified name, or path. Each hit has Unfold's name, file:line, and its key: pass the key to read_code, get_relations, or add_cards. limit is per kind (default 10, at most 50); pass the next cursor for more (a page of long lines shows fewer). projects narrows the search (names or ids). Searching never wakes a resting project; reading a hit does.",
  input: z.object({
    query: z.string().min(1).describe("Words of a name"),
    kinds: z.array(z.enum(KINDS)).min(1).optional().describe("What to find (default: entrances, functions, types, files)"),
    projects: z.array(z.string().min(1)).min(1).optional().describe("Projects to look in, by name or id (default: all)"),
    limit: z.number().int().min(1).max(50).default(10).describe("Hits per kind"),
    cursor: z.string().optional().describe("The cursor an earlier page gave"),
  }),
  annotations: { readOnlyHint: true },
  async run({ query, kinds, projects, limit, cursor }, context) {
    const wanted = new Set<Kind>(kinds ?? KINDS);
    const offset = cursorOffset(cursor);
    const rows: readonly ProjectRow[] = projects === undefined ? await context.projects() : await Promise.all(projects.map((name) => context.project(name)));
    const ids = rows.map((row) => row.id);
    const engineLimit = Math.min(MAX_ENGINE_LIMIT, offset + limit);
    const names = (definitionKinds: NodeKind[]) => context.engine("search", { query, kinds: ["definitions"], definitionKinds, limit: engineLimit, projects: ids });
    const others = [...(wanted.has("entrances") ? (["entrances"] as const) : []), ...(wanted.has("files") ? (["files"] as const) : [])];
    const [functions, types, rest] = await Promise.all([
      wanted.has("functions") ? names(FUNCTION_KINDS) : null,
      wanted.has("types") ? names(TYPE_KINDS) : null,
      others.length > 0 ? context.engine("search", { query, kinds: [...others], limit: engineLimit, projects: ids }) : null,
    ]);
    const namesById = new Map(rows.map((row) => [row.id, row.name]));
    const tag = (project: string) => (rows.length > 1 ? ` in ${namesById.get(project) ?? project}` : "");
    const hits = (title: string, group: { readonly hits: readonly SearchHit[]; readonly total: number } | undefined, size: number, line = hitLine) =>
      group ? section(title, group.hits, group.total, offset, size, (hit) => line(hit, tag(hit.project))) : null;
    const page = fitting(limit, 0, (size) =>
      [
        wanted.has("entrances") ? hits("Entrances", rest?.entrances, size, entranceHitLine) : null,
        hits("Functions", functions?.definitions, size),
        hits("Types", types?.definitions, size),
        wanted.has("files") && rest
          ? section("Files", rest.files.hits, rest.files.total, offset, size, (hit) => `- ${hit.path}${tag(hit.project)}${hit.definitions === null ? " (not compiled)" : `: ${count(hit.definitions, "definition")}`}`)
          : null,
      ].filter((item): item is Section => item !== null),
    );
    const lines = page.sections.flatMap((item) => item.lines);
    return textResult(
      context.header(rows.length === 1 && rows[0] ? { project: rows[0].name } : {}),
      lines.length > 0 ? lines.join("\n") : `Nothing matches “${query}”${rows.length > 1 ? " in any project" : ""}. Try fewer or shorter words.`,
      moreLine(page.sections, "search", offset + page.limit),
      lines.length > 0 ? "Next: read_code or get_relations with a key; add_cards puts it on a page." : null,
    );
  },
});

// ---------------------------------------------------------------------------------------------
// list_entrances

/** A `code` key with its state, and its fix when one exists. */
function codeText(code: EntranceCode): string {
  const state = code.state === undefined ? "" : ` (${code.state.replaceAll("_", " ")}${code.fix !== undefined ? `; fix: ${code.fix.kind.replaceAll("_", " ")} to ${code.fix.code}` : ""})`;
  return `${code.key}${state}`;
}

/** An entry's problem in `unfold.toml`, which keeps it out of the graph. */
function problemText(problem: AppView["problem"]): string | null {
  return problem === undefined ? null : `problem: ${problem.message}${problem.line !== undefined ? ` (unfold.toml line ${problem.line})` : ""}`;
}

function entranceLine(entrance: EntranceView): string {
  const facts = [
    entrance.key !== undefined ? `key ${entrance.key}` : null,
    entrance.on !== undefined ? `on ${entrance.on}` : null,
    entrance.code.length > 0 ? `code ${entrance.code.map(codeText).join(", ")}` : null,
    entrance.about ?? null,
    problemText(entrance.problem),
  ].filter((fact) => fact !== null);
  return `- ${entrance.name ?? "(no name)"}: ${facts.join("; ")}`;
}

/** An app's heading: its name, key, run command, code, and problem. */
function appHeading(app: AppView): string {
  const facts = [
    app.key !== undefined ? `key ${app.key}` : null,
    app.run !== undefined ? `run ${app.run}` : null,
    app.code !== undefined ? `code ${codeText(app.code)}` : null,
    problemText(app.problem),
  ].filter((fact) => fact !== null);
  return `${app.name ?? "(no name)"}${facts.length > 0 ? ` (${facts.join("; ")})` : ""}`;
}

/** Whether an entrance's name, `on`, code, or about contains `filter`, ignoring case. */
function matches(entrance: EntranceView, filter: string | undefined): boolean {
  if (filter === undefined) {
    return true;
  }
  const text = [entrance.name, entrance.on, entrance.about, ...entrance.code.map((code) => code.key)].join("\n").toLowerCase();
  return text.includes(filter.toLowerCase());
}

export const listEntrances = defineTool({
  name: "list_entrances",
  title: "List entrances",
  description:
    "How a project starts, as its unfold.toml declares it: its apps in file order, each with its key, run command, and code, and each app's entrances with their key, what starts them (on), and their code, each code key with its state (found, missing, not analyzed, not a start, stale) and its fix when one exists. An entry with a problem in unfold.toml says so and stays out of the graph. app narrows it to one (its name or key); filter keeps entrances whose name, on, code, or about contains it, ignoring case. limit is per app (default 25, at most 100); pass the next cursor for more. Pass a key to read_code, add_cards, or create_board's trace.",
  input: z.object({
    project: z.string().min(1).optional().describe("The project's name or id (default: the project of the board in view)"),
    app: z.string().min(1).optional().describe("An app's name or key"),
    filter: z.string().min(1).optional().describe("Text an entrance's name, on, code, or about contains"),
    limit: z.number().int().min(1).max(100).default(25).describe("Entrances per app"),
    cursor: z.string().optional().describe("The cursor an earlier page gave"),
  }),
  annotations: { readOnlyHint: true },
  async run({ project, app, filter, limit, cursor }, context) {
    const row = await projectOrView(context, project);
    const offset = cursorOffset(cursor);
    // The engine finds the app by its name or key, as edits do, and refuses one the file doesn't have.
    const { resolved, apps } = await context.engine("entrances", app === undefined ? { project: row.id } : { project: row.id, app });
    const matching = filter === undefined ? "" : ` matching “${filter}”`;
    const page = fitting(limit, 0, (size) =>
      apps.map((item) => {
        const entrances = item.entrances.filter((entrance) => matches(entrance, filter));
        const shown = section(`${appHeading(item)}, entrances${matching}`, entrances, entrances.length, offset, size, entranceLine);
        return shown.lines.length > 0 ? shown : { lines: [`${appHeading(item)}: no entrances${matching}.`], more: false };
      }),
    );
    const lines = page.sections.flatMap((item) => item.lines);
    return textResult(
      context.header({ project: row.name }),
      resolved ? null : "The code keys aren't looked up yet: compile_project, then list again.",
      lines.length > 0 ? lines.join("\n") : `${row.name} declares no apps in unfold.toml.`,
      moreLine(page.sections, "list_entrances", offset + page.limit),
      lines.length > 0 ? "Next: read_code with an app's or entrance's key shows its code; create_board with trace makes a board from it." : null,
    );
  },
});
