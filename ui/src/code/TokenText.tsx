/**
 * One line of source in the syntax palette: its text cut into runs at its token spans (`CodeLine`'s
 * `tokens`, UTF-16 offsets into the line, sorted and never overlapping), each run colored by its token
 * class. Text between tokens is plain.
 */

import { memo } from "react";
import type { Token, TokenClass } from "../ipc/generated";
import "./tokens.css";

/** A run of a line's text with one token class (`null`: plain). */
export interface TokenRun {
  readonly start: number;
  readonly end: number;
  readonly token: TokenClass | null;
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

export const TokenText = memo(function TokenText({ text, tokens }: { readonly text: string; readonly tokens: readonly Token[] }) {
  return (
    <>
      {tokenRuns(text.length, tokens).map((run) => (
        <span key={run.start} className={run.token === null ? undefined : `tok-${run.token}`}>
          {text.slice(run.start, run.end)}
        </span>
      ))}
    </>
  );
});
