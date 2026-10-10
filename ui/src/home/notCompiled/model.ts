/**
 * What the not-compiled Home says (06): what will be read, what is skipped and why, and the scope tree
 * whose check boxes are staged in memory until Compile writes them (V-69).
 */

import type { CompilePreview, Language, ScopeFolder, SettingsEdit, SkipKind, SkippedFolder } from "../../ipc";
import { formatCount, plural } from "../../lib/format";

const LANGUAGE_NAMES: Readonly<Record<Language, string>> = { typescript: "TypeScript", python: "Python" };

/** "Unfold will read 846 TypeScript and 212 Python files." */
export function languageSentence(preview: Pick<CompilePreview, "languages" | "files">): string {
  const counts = preview.languages.filter((entry) => entry.files > 0);
  if (counts.length === 0) {
    return "There are no TypeScript or Python files to read.";
  }
  const parts = counts.map((entry) => `${formatCount(entry.files)} ${LANGUAGE_NAMES[entry.language]}`);
  const last = parts.pop() ?? "";
  const list = parts.length > 0 ? `${parts.join(", ")} and ${last}` : last;
  return `Unfold will read ${list} ${preview.files === 1 ? "file" : "files"}.`;
}

export interface SkippedRow {
  readonly reason: string;
  /** Folders, as "node_modules/ · dist/", with "· N more" past the first two. */
  readonly examples: string;
  readonly count: string;
}

const EXAMPLES = 2;

function folderName(entry: SkippedFolder): string {
  if (entry.folder === "" || entry.folder === ".") {
    return "top level";
  }
  return entry.wholeFolder ? `${entry.folder}/` : entry.folder;
}

/** The Skipped table: one row per reason, in the engine's order. */
export function skippedRows(skipped: readonly SkippedFolder[]): SkippedRow[] {
  const byReason = new Map<string, SkippedFolder[]>();
  for (const entry of skipped) {
    byReason.set(entry.reasonText, [...(byReason.get(entry.reasonText) ?? []), entry]);
  }
  return [...byReason].map(([reason, entries]) => {
    const names = entries.slice(0, EXAMPLES).map(folderName);
    const rest = entries.length - names.length;
    const files = entries.reduce((sum, entry) => sum + entry.files, 0);
    return {
      reason,
      examples: rest > 0 ? `${names.join(" · ")} · ${formatCount(rest)} more` : names.join(" · "),
      count: plural(files, "file"),
    };
  });
}

/** A scope row's box: graphed, left out by `[scope]`, or skipped by discovery (can't be chosen here). */
export type ScopeChoice = "checked" | "unchecked" | "disabled";

export function savedChoice(folder: Pick<ScopeFolder, "skipped">): ScopeChoice {
  if (folder.skipped === null) {
    return "checked";
  }
  return folder.skipped === "excluded" || folder.skipped === "not_included" ? "unchecked" : "disabled";
}

/** Folders the tree shows: discovery's default skips are already listed under Skipped. */
export function scopeRows(folders: readonly ScopeFolder[]): ScopeFolder[] {
  return folders.filter((folder) => folder.skipped !== "default");
}

const SKIP_WORDS: Readonly<Record<SkipKind, string>> = {
  default: "skipped by default",
  ignored: "ignored by .gitignore",
  excluded: "excluded",
  not_included: "not included",
  language_off: "language turned off",
  javascript: "JavaScript isn't graphed",
  symlink: "a symbolic link",
  invalid_path: "path can't be read",
};

/** Why a disabled row can't be chosen, in the right column. */
export function skipWords(kind: SkipKind): string {
  return SKIP_WORDS[kind];
}

/** Staged check box changes: folder path → checked. Only paths that differ from the saved scope. */
export type StagedScope = Readonly<Record<string, boolean>>;

export function effectiveChoice(folder: ScopeFolder, staged: StagedScope): ScopeChoice {
  const saved = savedChoice(folder);
  if (saved === "disabled") {
    return saved;
  }
  const change = staged[folder.path];
  return change === undefined ? saved : change ? "checked" : "unchecked";
}

/** The `update_project_config` edits for the staged changes, in folder order. */
export function scopeEdits(folders: readonly ScopeFolder[], staged: StagedScope): SettingsEdit[] {
  return folders.flatMap((folder): SettingsEdit[] => {
    const change = staged[folder.path];
    if (change === undefined || savedChoice(folder) === "disabled") {
      return [];
    }
    return [change ? { op: "include_in_graph", path: folder.path } : { op: "exclude_from_graph", path: folder.path }];
  });
}

/**
 * "1,058 of 1,119 files": the files the Compile will read with the staged changes, of the files it
 * could read with every folder checked. `previewFiles` is what the saved scope reads.
 */
export function scopeCounts(previewFiles: number, folders: readonly ScopeFolder[], staged: StagedScope): { chosen: number; total: number } {
  let chosen = previewFiles;
  let total = previewFiles;
  for (const folder of folders) {
    const saved = savedChoice(folder);
    if (saved === "unchecked") {
      total += folder.sources;
    }
    const now = effectiveChoice(folder, staged);
    if (saved === "checked" && now === "unchecked") {
      chosen -= folder.sources;
    } else if (saved === "unchecked" && now === "checked") {
      chosen += folder.sources;
    }
  }
  return { chosen: Math.max(0, chosen), total };
}

/** The Compile row's caption, from the project's size. */
export function compileCaption(files: number): string {
  if (files < 500) {
    return "A few seconds for a project this size. You can keep working.";
  }
  if (files < 5000) {
    return "About a minute for a project this size. You can keep working.";
  }
  return "A few minutes for a project this size. You can keep working.";
}
