/**
 * Opening a project's file in the person's editor and showing it in the file manager. Failures
 * become toasts; neither ever throws.
 */

import { showToast } from "../components";
import { call } from "../ipc/commands";
import { toUnfoldError } from "../ipc/unfold";

function fail(what: string, error: unknown): void {
  showToast({ title: what, body: toUnfoldError(error).message, tone: "danger" });
}

/** Opens `path` (relative to the project's folder) in the editor, at `line` when given. */
export async function openInEditor(project: string, path: string, line?: number | null): Promise<void> {
  const args = line ? { project, path, line } : { project, path };
  try {
    await call("open_in_editor", args);
  } catch (error) {
    fail("Couldn't open the file", error);
  }
}

/** Shows `path` (relative to the project's folder; `""` is the folder itself) in Finder. */
export async function revealInFinder(project: string, path: string): Promise<void> {
  const args = { project, path };
  try {
    await call("reveal_in_finder", args);
  } catch (error) {
    fail("Couldn't show the file", error);
  }
}
