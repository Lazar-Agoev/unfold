/**
 * Board folder prompts, 400 wide over the content area: a folder's Rename…, and the board menu's Move
 * to folder › New folder…. Folders are one level deep, so each names one folder. Failures are toasts;
 * the sidebar follows by `boards_changed`.
 */

import { PromptDialog, showToast } from "../components";
import { toUnfoldError, type ProjectId } from "../ipc";
import { renameFolder, setBoardFolder } from "../state/boards";
import { folderProblem } from "./folders";
import type { MoveToFolderRequest } from "./moveToFolder";

function fail(title: string, error: unknown): void {
  const failure = toUnfoldError(error);
  showToast({ title, body: failure.code === "folder_exists" ? "A folder with that name is already there." : failure.message, tone: "danger" });
}

export function RenameFolderDialog({ project, folder, onClose }: { readonly project: ProjectId; readonly folder: string; readonly onClose: () => void }) {
  return (
    <PromptDialog
      open
      title="Rename folder"
      description="Its boards move with it."
      label="Name"
      defaultValue={folder}
      confirmLabel="Rename"
      placement="canvas"
      validate={folderProblem}
      onCancel={onClose}
      onSubmit={(value) => {
        onClose();
        if (value === folder) {
          return;
        }
        renameFolder(project, folder, value).catch((error: unknown) => {
          fail("Couldn't rename the folder", error);
        });
      }}
    />
  );
}

export function MoveToFolderDialog({ request, onClose }: { readonly request: MoveToFolderRequest; readonly onClose: () => void }) {
  return (
    <PromptDialog
      open
      title="New folder"
      description="Moves the board into a new folder."
      label="Name"
      confirmLabel="Move"
      placement="canvas"
      validate={folderProblem}
      onCancel={onClose}
      onSubmit={(value) => {
        onClose();
        if (value !== request.folder) {
          setBoardFolder(request.board, value).catch((error: unknown) => {
            fail("Couldn't move the board", error);
          });
        }
      }}
    />
  );
}
