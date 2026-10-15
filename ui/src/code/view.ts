/**
 * The code panel's model for what it shows (`code_view`), and the line its selection marks. The first
 * answer holds a page of the view around what the selection marks; the rest of a long view is asked
 * for a page at a time, as its pages come into view.
 */

import { keepPreviousData, skipToken, useQueries, useQuery, type UseQueryResult } from "@tanstack/react-query";
import { useMemo } from "react";
import { call, engineKey, engineQueryOptions } from "../ipc";
import type { CodeLine, CodeView, CodeViewArgs, DefinitionLine, LineRange } from "../ipc/generated";
import type { UnfoldError } from "../ipc/unfold";
import type { CodeOpen } from "../state/codePanel";
import { pageLines, type LineSpan } from "./paging";

/** The `code_view` arguments for what the panel shows; with `window`, for those of its lines. */
export function codeViewArgs(open: CodeOpen, window?: LineSpan): CodeViewArgs {
  return { project: open.project, target: open.target, ...(open.show ? { show: open.show } : {}), ...(window ? { window: [window[0], window[1]] } : {}) };
}

/** The view of what the panel shows, kept current (and kept while it is asked for again). */
export function useCodeView(open: CodeOpen | null): UseQueryResult<CodeView, UnfoldError> {
  const args = open ? codeViewArgs(open) : null;
  return useQuery<CodeView, UnfoldError>({
    queryKey: args ? engineKey("code_view", args) : ["engine", "code_view", null],
    queryFn: args ? () => call("code_view", args) : skipToken,
    placeholderData: keepPreviousData,
  });
}

/** The first line the selection marks (only when the lines can be trusted). */
export function markedLine(view: CodeView): number | undefined {
  return view.state === "current" ? (view.mark?.lines?.[0] ?? view.mark?.expression[0]?.line) : undefined;
}

/** One loaded line of the view, with what is found on it. */
interface LoadedLine {
  readonly line: CodeLine;
  /** The marked expression's ranges on it. */
  readonly marks: readonly LineRange[];
}

/** The lines of a view loaded so far. */
export interface LoadedLines {
  /** The view's first and last line; `[1, 0]` when it shows none. */
  readonly span: LineSpan;
  /** Each loaded line, by number. */
  readonly lines: ReadonlyMap<number, LoadedLine>;
  /** The definitions that start on the loaded lines, in written order. */
  readonly definitions: readonly DefinitionLine[];
}

/** No lines. */
const NO_SPAN: LineSpan = [1, 0];

/** Nothing loaded yet. */
const NOTHING: LoadedLines = { span: NO_SPAN, lines: new Map(), definitions: [] };

/** The answers of the page queries. */
function answersOf(results: readonly { readonly data?: CodeView | undefined }[]): (CodeView | undefined)[] {
  return results.map((result) => result.data);
}

/**
 * `view`'s lines merged with those of `answers`, each line from the first answer that holds it. An
 * answer for another state or span of the file (it changed between them) is left out until all of them
 * are asked for again.
 */
export function loadedLines(view: CodeView, answers: readonly (CodeView | undefined)[]): LoadedLines {
  const span: LineSpan = view.span ?? NO_SPAN;
  const lines = new Map<number, LoadedLine & { readonly definitions: readonly DefinitionLine[] }>();
  for (const answer of [view, ...answers]) {
    if (answer?.state !== view.state || answer.span?.[0] !== span[0] || answer.span[1] !== span[1]) {
      continue;
    }
    for (const line of answer.lines) {
      if (!lines.has(line.number)) {
        lines.set(line.number, {
          line,
          marks: (answer.mark?.expression ?? []).filter((range) => range.line === line.number),
          definitions: answer.definitions.filter((definition) => definition.line === line.number),
        });
      }
    }
  }
  const definitions = [...lines.keys()].sort((a, b) => a - b).flatMap((number) => lines.get(number)?.definitions ?? []);
  return { span, lines, definitions };
}

/**
 * The lines of `view` (the panel's view of `open`, once it has one) on `pages`, with what is found on
 * them: the pages the first answer doesn't hold are asked for (`window`) as they come into view. While
 * `view` is the previous selection's, kept as the next one loads, only its own lines are used.
 */
export function useCodeLines(open: CodeOpen, view: CodeView | undefined, pages: readonly number[], stale: boolean): LoadedLines {
  const span: LineSpan = view?.span ?? NO_SPAN;
  const held: LineSpan = [view?.lines[0]?.number ?? 1, view?.lines.at(-1)?.number ?? 0];
  const windows = stale
    ? []
    : pages.map((page) => pageLines(page, span)).filter(([start, end]) => start <= span[1] && (start < held[0] || end > held[1]));
  const answers = useQueries({
    queries: windows.map((window) => engineQueryOptions("code_view", codeViewArgs(open, window))),
    combine: answersOf,
  });
  return useMemo(() => (view ? loadedLines(view, answers) : NOTHING), [view, answers]);
}
