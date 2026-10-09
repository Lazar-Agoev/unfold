/** What the status bar says about the graph (architecture §7.9), as plain data for the view and tests. */

import type { CompileProgress, FallbackReason, GraphStatus, Language } from "../ipc/generated";

export type StatusTone = "current" | "busy" | "warn" | "failed" | "idle";

export interface StatusLine {
  /** `●` steady, `◌` working, `▲` failed, `○` nothing yet. */
  readonly glyph: "●" | "◌" | "▲" | "○";
  readonly tone: StatusTone;
  readonly text: string;
  /** Quiet notes after the state ("using fallback location", "watching by scanning"). */
  readonly notes: readonly { readonly text: string; readonly title: string }[];
}

const LANGUAGE_NAMES: Readonly<Record<Language, string>> = { typescript: "TypeScript", python: "Python" };

export function languageName(language: Language): string {
  return LANGUAGE_NAMES[language];
}

/** `TypeScript + Python`. */
export function languageList(languages: readonly Language[]): string {
  return languages.map(languageName).join(" + ");
}

export function plural(count: number, one: string, many = `${one}s`): string {
  return `${count.toLocaleString("en-US")} ${count === 1 ? one : many}`;
}

const STAGE_WORDS: Readonly<Record<string, string>> = {
  discover: "finding files",
  link: "linking",
  resolve: "resolving calls",
  detect: "finding entrances",
  validate: "checking the graph",
};

/** "12 of 84 files" while reading files; the stage in words for the stages after. */
export function progressText(progress: CompileProgress | null): string | null {
  if (!progress) {
    return null;
  }
  if ((progress.stage === "snapshot" || progress.stage === "extract") && progress.total > 0) {
    return `${progress.done.toLocaleString("en-US")} of ${plural(progress.total, "file")}`;
  }
  return STAGE_WORDS[progress.stage] ?? null;
}

const FALLBACK_REASONS: Readonly<Record<FallbackReason, string>> = {
  read_only: "The folder is read-only, so the graph and drafts are kept in Unfold's app data.",
  network_volume: "The folder is on a network volume, so the graph and drafts are kept in Unfold's app data.",
  wal_unavailable: "The folder's disk can't hold the graph database safely, so it is kept in Unfold's app data.",
};

export function fallbackReasonText(reason: FallbackReason): string {
  return FALLBACK_REASONS[reason];
}

/** The status line for a graph status, with live progress laid over a compile. */
export function statusLine(status: GraphStatus, progress: CompileProgress | null = null): StatusLine {
  const notes: { text: string; title: string }[] = [];
  if (status.fallback) {
    notes.push({ text: "using fallback location", title: fallbackReasonText(status.fallback) });
  }
  if (status.watching === "scanning") {
    notes.push({
      text: "watching by scanning",
      title: "File events aren't available for this folder, so Unfold checks it for changes every few seconds.",
    });
  }
  const { state } = status;
  switch (state.kind) {
    case "not_compiled":
      return { glyph: "○", tone: "idle", text: "Not compiled yet", notes };
    case "current": {
      const parts = ["Graph current", plural(state.files, "file")];
      if (state.languages.length > 0) {
        parts.push(languageList(state.languages));
      }
      return { glyph: "●", tone: "current", text: parts.join(" · "), notes };
    }
    case "compiling": {
      const live = progressText(progress) ?? (state.total > 0 ? `${state.done.toLocaleString("en-US")} of ${plural(state.total, "file")}` : null);
      return { glyph: "◌", tone: "busy", text: live ? `Compiling · ${live}` : "Compiling…", notes };
    }
    case "updating":
      return {
        glyph: "◌",
        tone: "busy",
        text: state.files === null ? "Updating · checking the folder" : `Updating · ${plural(state.files, "file")} changed`,
        notes,
      };
    case "doesnt_parse":
      return {
        glyph: "●",
        tone: "warn",
        text:
          state.files === 1
            ? "1 file doesn't parse · showing its last version that parsed"
            : `${plural(state.files, "file")} don't parse · showing their last versions that parsed`,
        notes,
      };
    case "paused":
      return {
        glyph: "●",
        tone: "warn",
        text:
          state.changed > 0
            ? `Background updates paused · ${plural(state.changed, "file")} changed since the last compile`
            : "Background updates paused",
        notes,
      };
    case "failed":
      return { glyph: "▲", tone: "failed", text: "Compile failed · see Problems", notes };
  }
}
