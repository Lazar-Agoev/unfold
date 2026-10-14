/**
 * Home's state line under the project's name: the state in words, a muted detail, one action link, and the
 * quiet note (watching by scanning) that the trailing info icon explains. Plain data, so every state's
 * wording is testable.
 */

import type { CompileProgress, ProjectState, ProjectSummary } from "../ipc";
import { formatCount, languageList, plural, relativeTime } from "../lib/format";

export type StateAction = "cancel" | "health";

export interface StateLineModel {
  readonly headline: string;
  /** Starts with "· "; empty when the state needs no detail. */
  readonly detail: string;
  /** The accent link after the detail. `joined` follows the detail with no gap. */
  readonly action: { readonly kind: StateAction; readonly label: string; readonly joined: boolean } | null;
  /** Sentences for the info icon's tooltip; no icon when empty. */
  readonly notes: readonly string[];
}

export interface StateLineInput {
  readonly state: ProjectState;
  /** The selected project's summary, once it has answered. */
  readonly summary: ProjectSummary | undefined;
  readonly progress: CompileProgress | null;
  /** Whether every file that doesn't parse shows its last version that parsed (from `problems`); unknown until read. */
  readonly lastGood?: boolean;
  readonly nowMs?: number;
}

const STAGE_WORDS: Readonly<Record<string, string>> = {
  snapshot: "reading files",
  extract: "reading files",
  discover: "finding files",
  link: "linking",
  resolve: "resolving calls",
  validate: "checking the graph",
};

const SCANNING_NOTE = "File events aren't available for this folder, so Unfold checks it for changes every few seconds.";

/** "412 of 1,058 files · resolving calls" from the state's counts and the latest progress. */
export function compilingDetail(state: { readonly done: number; readonly total: number }, progress: CompileProgress | null): string {
  const reading = progress !== null && (progress.stage === "snapshot" || progress.stage === "extract") && progress.total > 0;
  const done = reading ? progress.done : state.done;
  const total = reading ? progress.total : state.total;
  const parts: string[] = [];
  if (total > 0) {
    parts.push(`${formatCount(done)} of ${plural(total, "file")}`);
  }
  const stage = progress ? STAGE_WORDS[progress.stage] : undefined;
  if (stage !== undefined) {
    parts.push(stage);
  }
  return parts.join(" · ");
}

/** "1,284 files · TypeScript + Python · updated 2 minutes ago". */
function graphDetail(summary: ProjectSummary | undefined, nowMs: number | undefined): string {
  if (!summary) {
    return "";
  }
  const parts = [plural(summary.files, "file")];
  if (summary.languages.length > 0) {
    parts.push(languageList(summary.languages));
  }
  if (summary.updatedAtMs !== null) {
    parts.push(`updated ${relativeTime(summary.updatedAtMs, nowMs)}`);
  }
  return parts.join(" · ");
}

function dot(text: string): string {
  return text === "" ? "" : `· ${text}`;
}

function notesOf(summary: ProjectSummary | undefined): string[] {
  return summary?.status.watching === "scanning" ? [SCANNING_NOTE] : [];
}

export function stateLine({ state, summary, progress, lastGood, nowMs }: StateLineInput): StateLineModel {
  const notes = notesOf(summary);
  const line = (headline: string, detail: string, action: StateLineModel["action"] = null): StateLineModel => ({ headline, detail, action, notes });
  switch (state.kind) {
    case "not_compiled":
      return line("Not compiled yet", "");
    case "compiling":
      return line("Compiling", dot(compilingDetail(state, progress)), { kind: "cancel", label: "Cancel", joined: false });
    case "updating":
      return line("Updating", dot(state.files === null ? "checking the folder" : `${plural(state.files, "file")} changed`));
    case "current":
      return line("Graph current", dot(graphDetail(summary, nowMs)));
    case "problems": {
      const health = { kind: "health" as const, label: "See Health", joined: false };
      if (summary?.health.doesntParse === state.count) {
        const one = state.count === 1;
        const headline = one ? "1 file doesn't parse" : `${plural(state.count, "file")} don't parse`;
        const shown = one ? "showing its last version that parsed" : "showing their last versions that parsed";
        return line(headline, dot(lastGood === true ? shown : graphDetail(summary, nowMs)), health);
      }
      return line(plural(state.count, "problem"), dot(graphDetail(summary, nowMs)), health);
    }
    case "failed":
      return line("Compile failed", "· the previous graph stays ·", { kind: "health", label: "See Health", joined: true });
    case "resting":
      return line("Resting", dot(`last checked ${relativeTime(state.atMs, nowMs)}`));
    case "unavailable":
      return line("Unavailable", dot(state.reason));
  }
}
