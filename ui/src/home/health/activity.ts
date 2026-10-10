/**
 * The Activity log: each committed update to the graph, newest first, as a table row of
 * when, what, files read / cached / failed, and how long it took. The counts come from the engine's
 * fact lines ("files: read 3, from cache 211, …", "states: current 213, failed 1").
 */

import type { IndexLogEntry } from "../../ipc";
import { formatCount, relativeTime } from "../../lib/format";

/** The engine's fact lines as label/value pairs; the first line is the entry's summary. */
export function factLines(text: string): { summary: string | null; facts: { label: string; value: string }[] } {
  const lines = text.split("\n").filter((line) => line.trim() !== "");
  const [summary, ...rest] = lines;
  return {
    summary: summary ?? null,
    facts: rest.map((line) => {
      const colon = line.indexOf(": ");
      return colon < 0 ? { label: "", value: line } : { label: line.slice(0, colon), value: line.slice(colon + 2) };
    }),
  };
}

/** The number after `name` in a fact ("read 3, from cache 211" → read = 3), or 0. */
function countIn(value: string | undefined, name: string): number {
  const match = value === undefined ? null : new RegExp(`(?:^|[,;] )${name} (\\d+)`).exec(value);
  return match ? Number(match[1]) : 0;
}

export interface LogRow {
  readonly id: string;
  readonly time: string;
  /** The short word ("Update"); `action` is the engine's full name, for the cell's tooltip. */
  readonly what: string;
  readonly action: string;
  readonly files: string;
  readonly duration: string;
}

/** The engine's update kinds as words short enough for the 120px What column. */
const ACTION_WORDS: Readonly<Record<string, string>> = {
  Compile: "Compile",
  "Background update": "Update",
  "Rebuild from scratch": "Rebuild",
};

/** "140 ms", "2.1 s". */
export function duration(ms: number): string {
  return ms < 1000 ? `${formatCount(Math.round(ms))} ms` : `${(ms / 1000).toFixed(1)} s`;
}

function logRow(entry: IndexLogEntry, nowMs?: number): LogRow {
  const facts = new Map(factLines(entry.text).facts.map((fact) => [fact.label, fact.value]));
  const read = countIn(facts.get("files"), "read");
  const cached = countIn(facts.get("files"), "from cache");
  // A file that doesn't parse shows its last version that parsed, or nothing: both failed this update.
  const failed = countIn(facts.get("states"), "failed") + countIn(facts.get("states"), "last_good");
  return {
    id: `${entry.atMs}:${entry.action}`,
    time: relativeTime(entry.atMs, nowMs),
    what: ACTION_WORDS[entry.action] ?? entry.action,
    action: entry.action,
    files: `${formatCount(read)} / ${formatCount(cached)} / ${formatCount(failed)}`,
    duration: duration(entry.updateMs + entry.commitMs),
  };
}

/** The log's rows, newest first (the engine lists them oldest first). */
export function logRows(entries: readonly IndexLogEntry[], nowMs?: number): LogRow[] {
  return [...entries].reverse().map((entry) => logRow(entry, nowMs));
}
