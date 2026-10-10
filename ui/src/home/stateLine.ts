/**
 * Home's state line under the project's name (07 "Home state line", V-25, V-68): the state in words,
 * a muted detail, one action link, and the quiet notes (fallback location, watching by scanning) that
 * the trailing info icon explains. Plain data, so every state's wording is testable.
 */

import type { CompileProgress, FallbackReason, ProjectState, ProjectSummary } from "../ipc";
import { formatCount, languageList, plural, relativeTime } from "../lib/format";

/** A run of the detail: mono for branch names (V-25). */
interface DetailPart {
  readonly text: string;
  readonly mono?: boolean;
}

export type StateAction = "cancel" | "health" | "resume";

export interface StateLineModel {
  readonly headline: string;
  /** Starts with "· "; empty when the state needs no detail. */
  readonly detail: readonly DetailPart[];
  /** The accent link after the detail. `joined` follows the detail with no gap (07 row 7). */
  readonly action: { readonly kind: StateAction; readonly label: string; readonly joined: boolean } | null;
  /** Sentences for the info icon's tooltip; no icon when empty. */
  readonly notes: readonly string[];
}

export interface StateLineInput {
  readonly state: ProjectState;
  /** The selected project's summary, once it has answered. */
  readonly summary: ProjectSummary | undefined;
  readonly progress: CompileProgress | null;
  /** The branch (or detached commit) for the not-compiled line. */
  readonly branch: string | null;
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
  detect: "finding entrances",
  validate: "checking the graph",
};

const FALLBACK_REASONS: Readonly<Record<FallbackReason, string>> = {
  read_only: "The folder is read-only, so the graph and drafts are kept in Unfold's app data.",
  network_volume: "The folder is on a network volume, so the graph and drafts are kept in Unfold's app data.",
  wal_unavailable: "The folder's disk can't hold the graph database safely, so it is kept in Unfold's app data.",
};

/** Why the graph lives in app data, in a sentence. */
export function fallbackReasonText(reason: FallbackReason): string {
  return FALLBACK_REASONS[reason];
}

const SCANNING_NOTE = "File events aren't available for this folder, so Unfold checks it for changes every few seconds.";

/** "412 of 1,058 files · resolving calls" from the state's counts and the latest progress (V-68). */
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

/** "1,284 files · TypeScript + Python · updated 2 minutes ago" (V-25). */
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

function dot(text: string): DetailPart[] {
  return text === "" ? [] : [{ text: `· ${text}` }];
}

function notesOf(summary: ProjectSummary | undefined): string[] {
  const notes: string[] = [];
  if (summary?.status.fallback) {
    notes.push(fallbackReasonText(summary.status.fallback));
  }
  if (summary?.status.watching === "scanning") {
    notes.push(SCANNING_NOTE);
  }
  return notes;
}

export function stateLine({ state, summary, progress, branch, lastGood, nowMs }: StateLineInput): StateLineModel {
  const notes = notesOf(summary);
  const fallback = summary?.status.fallback ? " · using fallback location" : "";
  const line = (headline: string, detail: DetailPart[], action: StateLineModel["action"] = null): StateLineModel => ({
    headline,
    detail,
    action,
    notes,
  });
  switch (state.kind) {
    case "not_compiled":
      return line("Not compiled yet", branch === null ? [] : [{ text: "· branch " }, { text: branch, mono: true }]);
    case "compiling":
      return line("Compiling", dot(compilingDetail(state, progress)), { kind: "cancel", label: "Cancel", joined: false });
    case "updating":
      return line("Updating", dot(state.files === null ? "checking the folder" : `${plural(state.files, "file")} changed`));
    case "current":
      return line("Graph current", dot(graphDetail(summary, nowMs) + fallback));
    case "problems": {
      const health = { kind: "health" as const, label: "See Health", joined: false };
      if (summary?.health.doesntParse === state.count) {
        const one = state.count === 1;
        const headline = one ? "1 file doesn't parse" : `${plural(state.count, "file")} don't parse`;
        const shown = one ? "showing its last version that parsed" : "showing their last versions that parsed";
        return line(headline, dot(lastGood === true ? shown : graphDetail(summary, nowMs) + fallback), health);
      }
      return line(plural(state.count, "problem"), dot(graphDetail(summary, nowMs) + fallback), health);
    }
    case "paused":
      return line("Paused", dot(state.changed > 0 ? `${plural(state.changed, "file")} changed since` : "background updates are off"), {
        kind: "resume",
        label: "Resume",
        joined: false,
      });
    case "failed":
      return line("Compile failed", [{ text: "· the previous graph stays ·" }], { kind: "health", label: "See Health", joined: true });
    case "resting":
      return line("Resting", dot(`last checked ${relativeTime(state.atMs, nowMs)}`));
    case "unavailable":
      return line("Unavailable", dot(state.reason));
  }
}

/** The state a project shows its Home by: a resting project's last state. */
export function homeStateKind(state: ProjectState): ProjectState["kind"] {
  return state.kind === "resting" ? state.last.kind : state.kind;
}
