/**
 * Native dialogs. Paths outside a project come only from here (or from a drop or the command line):
 * the main process shows the dialog, and only the path the person chose leaves this process
 * (architecture §13).
 */

import { basename, join } from "node:path";
import type { BrowserWindow, MessageBoxOptions, OpenDialogOptions, SaveDialogOptions } from "electron";
import { app, dialog } from "electron";

/** Shows the folder picker for Add Project. Resolves with the chosen folder, or null if cancelled. */
export async function chooseFolder(parent: BrowserWindow | null): Promise<string | null> {
  const options: OpenDialogOptions = { title: "Add Project", buttonLabel: "Add", properties: ["openDirectory"] };
  const result = parent ? await dialog.showOpenDialog(parent, options) : await dialog.showOpenDialog(options);
  return result.canceled ? null : (result.filePaths[0] ?? null);
}

/** Shows the export save dialog. Resolves with the chosen path, or null if cancelled. */
export async function chooseExportPath(parent: BrowserWindow, defaultName: string): Promise<string | null> {
  const options: SaveDialogOptions = {
    title: "Export",
    // Only a file name is accepted from the window; the folder is the person's Documents.
    defaultPath: join(app.getPath("documents"), basename(defaultName) || "board.mmd"),
    filters: [{ name: "Mermaid", extensions: ["mmd"] }],
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
