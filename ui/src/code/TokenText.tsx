/**
 * One line of source in the syntax palette: its text cut into runs at its token spans (`CodeLine`'s
 * `tokens`, UTF-16 offsets into the line, sorted and never overlapping), each run colored by its token
 * class. Text between tokens is plain. The runs are cut again where the line's calls' names and the
 * marked expression start and end: a run inside a call's name carries the call (`data-call`), and a run
 * inside the mark is filled.
 */

import { memo } from "react";
import type { CallName, Key, Token, TokenClass } from "../ipc/generated";
import "./tokens.css";

/** A run of a line's text with one token class (`null`: plain). */
export interface TokenRun {
  readonly start: number;
  readonly end: number;
  readonly token: TokenClass | null;
}

/** A run of a line's text: its token class, the call whose name it is in, and whether it is marked. */
interface LineRun extends TokenRun {
  readonly call: Key | null;
  readonly marked: boolean;
}

/** A range of a line, in UTF-16 units. */
interface Span {
  readonly start: number;
  readonly end: number;
}

/** The runs of a line `length` units long: its tokens, clamped to the line, and the plain text between them. */
export function tokenRuns(length: number, tokens: readonly Token[]): TokenRun[] {
  const runs: TokenRun[] = [];
  let at = 0;
  for (const [tokenStart, tokenEnd, token] of tokens) {
    const start = Math.max(at, tokenStart);
    const end = Math.min(length, tokenEnd);
    if (end <= start) {
      continue;
    }
    if (start > at) {
      runs.push({ start: at, end: start, token: null });
    }
    runs.push({ start, end, token });
    at = end;
  }
  if (at < length) {
    runs.push({ start: at, end: length, token: null });
  }
  return runs;
}

function covering<T extends Span>(spans: readonly T[], at: number): T | undefined {
  return spans.find((span) => span.start <= at && at < span.end);
}

/** The token runs of a line, cut where its calls' names and its marks start and end. */
export function lineRuns(length: number, tokens: readonly Token[], calls: readonly CallName[], marks: readonly Span[]): LineRun[] {
  const cuts = [...calls, ...marks].flatMap((span) => [span.start, span.end]);
  return tokenRuns(length, tokens).flatMap((run) => {
    const edges = [...new Set([run.start, ...cuts.filter((cut) => cut > run.start && cut < run.end), run.end])].sort((a, b) => a - b);
    return edges.slice(0, -1).map((start, index) => ({
      start,
      end: edges[index + 1] ?? run.end,
      token: run.token,
      call: covering(calls, start)?.key ?? null,
      marked: covering(marks, start) !== undefined,
    }));
  });
}

function runClass(run: LineRun): string | undefined {
  const names = [run.token === null ? null : `tok-${run.token}`, run.marked ? "code-mark" : null].filter((name) => name !== null);
  return names.length > 0 ? names.join(" ") : undefined;
}

export const TokenText = memo(function TokenText({
  text,
  tokens,
  calls = [],
  marks = [],
}: {
  readonly text: string;
  readonly tokens: readonly Token[];
  readonly calls?: readonly CallName[];
  readonly marks?: readonly Span[];
}) {
  return (
    <>
      {lineRuns(text.length, tokens, calls, marks).map((run) => (
        <span key={run.start} className={runClass(run)} data-call={run.call ?? undefined}>
          {text.slice(run.start, run.end)}
        </span>
      ))}
    </>
  );
});
