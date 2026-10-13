/** Opening a project's file in the person's editor. A failure becomes a toast; it never throws. */

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
