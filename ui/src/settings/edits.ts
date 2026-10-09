/**
 * The one door every settings change goes through: `update_project_config`, whose refusals carry the
 * position in `.unfold/config.toml` that Rust found wrong. Rust keeps the file's comments and formatting.
 */

import { showToast } from "../components";
import { call, toUnfoldError, type ProjectConfig, type ProjectId, type SettingsEdit } from "../ipc";
import type { ManualRow } from "../state/overlays";

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

export type EditOutcome = { readonly ok: true; readonly config: ProjectConfig } | { readonly ok: false; readonly problem: SettingsProblemText };

/** Applies settings edits to a project, all or nothing. */
export async function applySettingsEdits(project: ProjectId, edits: readonly SettingsEdit[]): Promise<EditOutcome> {
  try {
    const config = await call("update_project_config", { project, edits: [...edits] });
    return { ok: true, config };
  } catch (error) {
    const failure = toUnfoldError(error);
    return { ok: false, problem: settingsProblem(failure.code, failure.message) };
  }
}

/** "Line 4, column 15: …" for a problem with a position. */
export function problemText(problem: SettingsProblemText): string {
  return problem.line === null ? problem.message : `Line ${problem.line}, column ${problem.column ?? 1}: ${problem.message}`;
}

/** Removes a manual entrance by finding its table again, then applying the edit. */
export async function removeManualEntrance(
  project: ProjectId,
  row: ManualRow,
  findIndex: (config: ProjectConfig) => number | null,
): Promise<boolean> {
  try {
    const config = await call("project_config", { project });
    const index = findIndex(config);
    if (index === null) {
      showToast({ title: "That entrance isn't in .unfold/config.toml any more.", tone: "danger" });
      return false;
    }
    const outcome = await applySettingsEdits(project, [{ op: "remove_entrance", index }]);
    if (!outcome.ok) {
      showToast({ title: "Couldn't remove the entrance", body: problemText(outcome.problem), tone: "danger" });
      return false;
    }
    showToast({ title: `Removed ${row.title}`, body: "The graph updates in the background." });
    return true;
  } catch (error) {
    showToast({ title: "Couldn't remove the entrance", body: toUnfoldError(error).message, tone: "danger" });
    return false;
  }
}
