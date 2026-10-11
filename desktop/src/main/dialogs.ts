/**
 * Native dialogs. Paths outside a project come only from here (or from a drop or the command line):
 * the main process shows the dialog, and only the path the person chose, or an agent's path the
 * person agreed to, leaves this process (architecture §13).
 */

import { homedir } from "node:os";
import { basename, join, sep } from "node:path";
import type { BrowserWindow, MessageBoxOptions, OpenDialogOptions, SaveDialogOptions } from "electron";
import { app, dialog } from "electron";

/** Shows the folder picker for Add Project. Resolves with the chosen folder, or null if cancelled. */
export async function chooseFolder(parent: BrowserWindow | null): Promise<string | null> {
  const options: OpenDialogOptions = { title: "Add Project", buttonLabel: "Add", properties: ["openDirectory"] };
  const result = parent ? await dialog.showOpenDialog(parent, options) : await dialog.showOpenDialog(options);
  return result.canceled ? null : (result.filePaths[0] ?? null);
}

/** What an export writes: its file type's name and extension. */
const EXPORT_FORMATS = {
  mermaid: { name: "Mermaid", extension: "mmd" },
  png: { name: "PNG image", extension: "png" },
} as const;

/** Shows the export save dialog for a Mermaid or PNG file. Resolves with the chosen path, or null if cancelled. */
export async function chooseExportPath(parent: BrowserWindow, defaultName: string, format: keyof typeof EXPORT_FORMATS = "mermaid"): Promise<string | null> {
  const { name, extension } = EXPORT_FORMATS[format];
  const options: SaveDialogOptions = {
    title: "Export",
    // Only a file name is accepted from the window; the folder is the person's Documents.
    defaultPath: join(app.getPath("documents"), basename(defaultName) || `board.${extension}`),
    filters: [{ name, extensions: [extension] }],
    properties: ["createDirectory", "showOverwriteConfirmation"],
  };
  const result = await dialog.showSaveDialog(parent, options);
  return result.canceled || !result.filePath ? null : result.filePath;
}

/** Asks to confirm removing a project from the workspace. Resolves with whether the person did. */
export async function confirmRemoveProject(parent: BrowserWindow, name: string): Promise<boolean> {
  const options: MessageBoxOptions = {
    type: "question",
    message: `Remove ${name} from the workspace?`,
    detail: "Its .unfold folder stays.",
    buttons: ["Remove", "Cancel"],
    defaultId: 0,
    cancelId: 1,
  };
  const result = await dialog.showMessageBox(parent, options);
  return result.response === 0;
}

/** Brings Unfold forward, and `parent` with it, even from another app: the person must see a dialog that waits for them. */
function bringForward(parent: BrowserWindow | null): void {
  app.focus({ steal: true });
  if (parent) {
    if (parent.isMinimized()) {
      parent.restore();
    }
    parent.show();
    parent.focus();
  }
}

/**
 * Asks whether an agent may add a folder to the workspace (design §5.7): "Claude wants to add
 * `~/repos/payments` to your workspace." Unfold comes forward first, since the agent's call waits for
 * the answer. Cancel is the default, so a stray Return doesn't agree. Resolves with whether the person
 * chose Add.
 */
export async function confirmAgentAddProject(parent: BrowserWindow | null, agent: string, path: string): Promise<boolean> {
  const home = homedir();
  const shown = path === home || path.startsWith(`${home}${sep}`) ? `~${path.slice(home.length)}` : path;
  const options: MessageBoxOptions = {
    type: "question",
    message: `${agent} wants to add \`${shown}\` to your workspace.`,
    detail: "Unfold reads the folder and writes only in its .unfold folder.",
    buttons: ["Add", "Cancel"],
    defaultId: 1,
    cancelId: 1,
  };
  bringForward(parent);
  const result = parent ? await dialog.showMessageBox(parent, options) : await dialog.showMessageBox(options);
  return result.response === 0;
}
