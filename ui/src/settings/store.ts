/**
 * Which settings dialog is open (the manual entrance editor, its remove confirmation, the folder
 * settings), and the one door every settings change goes through: `update_folder_config`, whose
 * refusals carry the position in `.unfold/config.toml` that Rust found wrong.
 */

import { create } from "zustand";
import { showToast } from "../components";
import { call } from "../ipc/commands";
import type { EntranceRow, FolderConfig, SettingsEdit } from "../ipc/generated";
import { toUnfoldError } from "../ipc/unfold";

/** An Entrances-view row that names a manual entrance. */
export type ManualRow = Pick<EntranceRow, "key" | "detector" | "title">;

export type EditorTarget =
  /** A new entrance, optionally with its first handler filled in ("Add as Entrance…"). */
  | { readonly mode: "add"; readonly handler?: string }
  | { readonly mode: "edit"; readonly row: ManualRow };

interface SettingsDialogsState {
  readonly editor: EditorTarget | null;
  readonly removing: ManualRow | null;
  readonly folderSettings: boolean;
}

export const useSettingsDialogs = create<SettingsDialogsState>()(() => ({ editor: null, removing: null, folderSettings: false }));

export function openEntranceEditor(target: EditorTarget = { mode: "add" }): void {
  useSettingsDialogs.setState({ editor: target });
}

export function closeEntranceEditor(): void {
  useSettingsDialogs.setState({ editor: null });
}

/** Asks before removing a manual entrance. */
export function confirmRemoveEntrance(row: ManualRow): void {
  useSettingsDialogs.setState({ removing: row });
}

export function closeRemoveEntrance(): void {
  useSettingsDialogs.setState({ removing: null });
}

export function openFolderSettings(): void {
  useSettingsDialogs.setState({ folderSettings: true });
}

export function closeFolderSettings(): void {
  useSettingsDialogs.setState({ folderSettings: false });
}

/** Why Rust refused a settings change, with its position when it named one. */
export interface SettingsProblemText {
  readonly code: string;
  readonly message: string;
  readonly line: number | null;
  readonly column: number | null;
}

const POSITION = /^(.*?)\.unfold\/config\.toml:(\d+):(\d+): ([\s\S]*)$/;

/** Splits Rust's `.unfold/config.toml:<line>:<column>: <message>` (with any prefix) apart. */
export function settingsProblem(code: string, text: string): SettingsProblemText {
  const match = POSITION.exec(text);
  if (match?.[2] === undefined || match[3] === undefined || match[4] === undefined) {
    return { code, message: text, line: null, column: null };
  }
  // A prefix such as "fix .unfold/config.toml first:" stays in front of the message.
  const prefix = (match[1] ?? "").trim();
  const message = prefix === "" ? match[4] : `${prefix} ${match[4]}`;
  return { code, message, line: Number(match[2]), column: Number(match[3]) };
}

export type EditOutcome = { readonly ok: true; readonly config: FolderConfig } | { readonly ok: false; readonly problem: SettingsProblemText };

/** Applies settings edits, all or nothing. Rust keeps the file's comments and formatting. */
export async function applySettingsEdits(edits: readonly SettingsEdit[]): Promise<EditOutcome> {
  try {
    const config = await call("update_folder_config", { edits: [...edits] });
    return { ok: true, config };
  } catch (error) {
    const failure = toUnfoldError(error);
    return { ok: false, problem: settingsProblem(failure.code, failure.message) };
  }
}

/** "line 4, column 15: …" for a problem with a position. */
export function problemText(problem: SettingsProblemText): string {
  return problem.line === null ? problem.message : `Line ${problem.line}, column ${problem.column ?? 1}: ${problem.message}`;
}

/** Removes a manual entrance by finding its table again, then applying the edit. */
export async function removeManualEntrance(row: ManualRow, findIndex: (config: FolderConfig) => number | null): Promise<boolean> {
  try {
    const config = await call("get_folder_config", {});
    const index = findIndex(config);
    if (index === null) {
      showToast({ message: "That entrance isn't in .unfold/config.toml any more.", tone: "danger" });
      return false;
    }
    const outcome = await applySettingsEdits([{ op: "remove_entrance", index }]);
    if (!outcome.ok) {
      showToast({ message: `Couldn't remove the entrance: ${problemText(outcome.problem)}`, tone: "danger" });
      return false;
    }
    showToast({ message: `Removed ${row.title}. The graph updates in the background.`, tone: "success" });
    return true;
  } catch (error) {
    showToast({ message: `Couldn't remove the entrance: ${toUnfoldError(error).message}`, tone: "danger" });
    return false;
  }
}
