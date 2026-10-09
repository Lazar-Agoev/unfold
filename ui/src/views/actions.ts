/**
 * Actions the sidebar views and panels share: show callers, open in the editor, reveal in the
 * file manager, and settings edits from the tree. Failures become toasts.
 */

import { showToast } from "../components";
import { call } from "../ipc/commands";
import type { SettingsEdit } from "../ipc/generated";
import { toUnfoldError } from "../ipc/unfold";
import type { Platform } from "../keyboard";
import { useLayout } from "../state/layout";
import { select } from "../state/selection";

function fail(what: string, error: unknown): void {
  showToast({ message: `${what}: ${toUnfoldError(error).message}`, tone: "danger" });
}

/** Selects a definition and shows its callers in the References panel. */
export function showCallers(key: string): void {
  select({ key });
  useLayout.getState().showPanelTab("references");
}

/** Opens a file in the person's editor (or the system's default app). */
export async function openInEditor(path: string, line?: number | null): Promise<void> {
  try {
    await call("open_in_editor", line ? { path, line } : { path });
  } catch (error) {
    fail("Couldn't open the file", error);
  }
}

/** Shows a file or folder in the system's file manager. */
export async function revealInFileManager(path: string): Promise<void> {
  try {
    await call("reveal_in_finder", { path });
  } catch (error) {
    fail("Couldn't show the file", error);
  }
}

/** "Reveal in Finder", "Show in Explorer", or "Show in file manager". */
export function revealLabel(platform: Platform): string {
  switch (platform) {
    case "darwin":
      return "Reveal in Finder";
    case "win32":
      return "Show in Explorer";
    default:
      return "Show in file manager";
  }
}

const EDIT_DONE: Readonly<Record<"exclude_from_graph" | "include_in_graph" | "graph_only", (path: string) => string>> = {
  exclude_from_graph: (path) => `Excluded ${path} from the graph.`,
  include_in_graph: (path) => `Included ${path} in the graph.`,
  graph_only: (path) => `The graph now covers only ${path}.`,
};

/** Applies one scope edit to `.unfold/config.toml`; the next update applies it to the graph. */
export async function editScope(op: keyof typeof EDIT_DONE, path: string): Promise<boolean> {
  const edit: SettingsEdit = { op, path };
  try {
    await call("update_folder_config", { edits: [edit] });
    showToast({ message: `${EDIT_DONE[op](path === "" ? "the whole folder" : path)} The next update applies it.`, tone: "success" });
    return true;
  } catch (error) {
    fail("Couldn't change the settings", error);
    return false;
  }
}
