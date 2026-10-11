/**
 * Add's search while typing: kind tabs with counts, result groups in the order Entrances, Functions,
 * Files, Text, and on the All tab the first four of each kind with a "N more" row that switches to that
 * kind's tab. Names come from one `search` fan-out; text matches stream from `search_text`.
 */

import { declarationWord } from "../code/typeKinds";
import type { FileHit, ProjectId, SearchArgs, SearchHit, SearchKind, SearchResults, TextMatch, TextSearchFile } from "../ipc/generated";
import type { AddKind } from "../state/overlays";

export type AddTab = "all" | AddKind;

/** The kinds in tab and group order. */
export const KINDS: readonly AddKind[] = ["entrances", "functions", "files", "text"];

export const KIND_LABELS: Readonly<Record<AddKind, string>> = {
  entrances: "Entrances",
  functions: "Functions",
  files: "Files",
  text: "Text",
};

/** What "N more …" calls a kind's hidden results. */
const MORE_WORDS: Readonly<Record<AddKind, readonly [string, string]>> = {
  entrances: ["entrance", "entrances"],
  functions: ["function", "functions"],
  files: ["file", "files"],
  text: ["match", "matches"],
};

/** Rows per kind on the All tab. */
const ALL_ROWS = 4;
/** Name results asked for on the All tab, and on one kind's tab. */
const ALL_LIMIT = 50;
const KIND_LIMIT = 500;
/** Text search starts at this many characters. */
export const TEXT_MIN = 2;

const SEARCH_KIND: Readonly<Record<Exclude<AddKind, "text">, SearchKind>> = {
  entrances: "entrances",
  functions: "definitions",
  files: "files",
};

/** The `search` arguments the All tab and the counts use. */
export function allSearchArgs(query: string): SearchArgs {
  return { query, limit: ALL_LIMIT };
}

/** The `search` arguments for one name kind's own tab (null on All and Text, which need none). */
export function kindSearchArgs(query: string, tab: AddTab): SearchArgs | null {
  return tab === "all" || tab === "text" ? null : { query, kinds: [SEARCH_KIND[tab]], limit: KIND_LIMIT };
}

/** One text match with the file it is in (its index among the file's matches, for the peek sheet). */
interface TextHit {
  readonly file: TextSearchFile;
  readonly match: TextMatch;
  readonly index: number;
}

export type ResultItem =
  | { readonly type: "entrance"; readonly id: string; readonly hit: SearchHit }
  | { readonly type: "function"; readonly id: string; readonly hit: SearchHit }
  | { readonly type: "file"; readonly id: string; readonly hit: FileHit }
  | { readonly type: "text"; readonly id: string; readonly hit: TextHit }
  | { readonly type: "more"; readonly id: string; readonly kind: AddKind; readonly count: number; readonly label: string };

interface ResultGroup {
  readonly id: AddKind;
  readonly label: string;
  readonly items: readonly ResultItem[];
}

/** The streamed text search so far. */
export interface TextResults {
  readonly files: readonly TextSearchFile[];
  /** Matches streamed so far, or the summary's count once it finished. */
  readonly matches: number;
  readonly truncated: boolean;
}

function hitItems(kind: "entrance" | "function", hits: readonly SearchHit[]): ResultItem[] {
  return hits.map((hit) => ({ type: kind, id: `${kind}:${hit.project}\u0000${hit.key}`, hit }));
}

function fileItems(hits: readonly FileHit[]): ResultItem[] {
  return hits.map((hit) => ({ type: "file", id: `file:${hit.project}\u0000${hit.path}`, hit }));
}

/** Every streamed match, one row each, up to `limit`. */
function textItems(files: readonly TextSearchFile[], limit: number): ResultItem[] {
  const items: ResultItem[] = [];
  for (const file of files) {
    for (const [index, match] of file.matches.entries()) {
      if (items.length >= limit) {
        return items;
      }
      items.push({ type: "text", id: `text:${file.project}\u0000${file.path}:${match.line}:${index}`, hit: { file, match, index } });
    }
  }
  return items;
}

/** "5 more functions", "25 more matches". */
export function moreLabel(kind: AddKind, count: number, truncated = false): string {
  const [one, many] = MORE_WORDS[kind];
  return `${count.toLocaleString("en-US")}${truncated ? "+" : ""} more ${count === 1 && !truncated ? one : many}`;
}

/** The results each kind has in all: name totals from `search`, text from the stream. */
export function kindCounts(names: SearchResults | undefined, text: TextResults | null): Readonly<Record<AddKind, number | null>> {
  return {
    entrances: names ? names.entrances.total : null,
    functions: names ? names.definitions.total : null,
    files: names ? names.files.total : null,
    text: text ? text.matches : null,
  };
}

/**
 * The result groups for a tab. `names` is the All search; `kindNames` the tab's own (500 per kind),
 * falling back to `names` while it loads.
 */
export function resultGroups(
  tab: AddTab,
  names: SearchResults | undefined,
  kindNames: SearchResults | undefined,
  text: TextResults | null,
): ResultGroup[] {
  const all = tab === "all";
  const source = all ? names : (kindNames ?? names);
  const groups: ResultGroup[] = [];
  const push = (kind: AddKind, items: ResultItem[], total: number, truncated: boolean) => {
    if (items.length === 0) {
      return;
    }
    if (!all) {
      groups.push({ id: kind, label: KIND_LABELS[kind], items });
      return;
    }
    const shown = items.slice(0, ALL_ROWS);
    const hidden = Math.max(0, total - shown.length);
    const more: ResultItem[] =
      hidden > 0 || (truncated && items.length > ALL_ROWS)
        ? [{ type: "more", id: `more:${kind}`, kind, count: hidden, label: moreLabel(kind, hidden, truncated) }]
        : [];
    groups.push({ id: kind, label: KIND_LABELS[kind], items: [...shown, ...more] });
  };
  if (source && (all || tab === "entrances")) {
    push("entrances", hitItems("entrance", source.entrances.hits), source.entrances.total, false);
  }
  if (source && (all || tab === "functions")) {
    push("functions", hitItems("function", source.definitions.hits), source.definitions.total, false);
  }
  if (source && (all || tab === "files")) {
    push("files", fileItems(source.files.hits), source.files.total, false);
  }
  if (text && (all || tab === "text")) {
    push("text", textItems(text.files, all ? ALL_ROWS : Number.POSITIVE_INFINITY), text.matches, text.truncated);
  }
  return groups;
}

/** The selectable rows of the groups, in order. */
export function flatItems(groups: readonly ResultGroup[]): ResultItem[] {
  return groups.flatMap((group) => group.items);
}

/** The projects that have any result, for "3 projects · 43 results". */
export function projectsWithResults(names: SearchResults | undefined, text: TextResults | null): number {
  const projects = new Set<ProjectId>();
  for (const row of names?.projects ?? []) {
    if (row.definitions + row.entrances + row.files > 0) {
      projects.add(row.project);
    }
  }
  for (const file of text?.files ?? []) {
    projects.add(file.project);
  }
  return projects.size;
}

const HTTP_METHODS = new Set(["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "ALL"]);

/** An entrance hit's kind column: its HTTP method (the first word of its qualified name), else its kind. */
export function entranceColumn(hit: Pick<SearchHit, "kind" | "qualifiedName">): string {
  const first = hit.qualifiedName.split(" ")[0] ?? "";
  return hit.kind === "http" && HTTP_METHODS.has(first) ? first : hit.kind.toUpperCase();
}

/** A definition hit's kind column: `FN` for functions, `TYPE` for type declarations, else the block kind (`METHOD`, `CLASS`). */
export function definitionColumn(kind: string): string {
  if (kind === "function") {
    return "FN";
  }
  return declarationWord(kind) === undefined ? kind.toUpperCase() : "TYPE";
}

/** A definition hit's accessible name: `Function createOrder, src/orders/create.ts`, `Interface Cart, src/cart.ts`. */
export function definitionText(hit: Pick<SearchHit, "kind" | "qualifiedName" | "path">): string {
  const word = declarationWord(hit.kind) ?? "function";
  return `${word.charAt(0).toUpperCase()}${word.slice(1)} ${hit.qualifiedName}, ${hit.path}`;
}

/** How far into a snippet its first match may start before the text before it is cut (the row shows ~50 characters). */
const SNIPPET_LEAD = 32;
/** How much of the text before the match a cut snippet keeps. */
const SNIPPET_CONTEXT = 16;

/**
 * A text match's line as its row shows it: without its indentation, and cut to start a little before
 * the first match when that lies past what the row shows (an ellipsis marks a cut, or a window of a
 * very long line). The ranges move with the text.
 */
export function trimmedMatch(match: TextMatch): { readonly text: string; readonly ranges: readonly (readonly [number, number])[] } {
  const indent = match.column > 0 ? 0 : (/^\s*/.exec(match.text)?.[0].length ?? 0);
  const first = Math.min(...match.ranges.map(([start]) => start));
  const start = Number.isFinite(first) && first - indent > SNIPPET_LEAD ? first - SNIPPET_CONTEXT : indent;
  const ellipsis = match.column > 0 || start > indent ? "…" : "";
  const shift = ellipsis.length - start;
  return {
    text: ellipsis + match.text.slice(start),
    ranges: match.ranges.map(([from, to]) => [from + shift, to + shift] as const),
  };
}

/** The file name of a path. */
export function baseName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}
