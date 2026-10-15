/**
 * Board folder prompts, 400 wide over the content area: a folder's Rename…, and the board menu's Move
 * to folder › New folder…. Folders are one level deep, so each names one folder. The engine's folder
 * commands are the only check of a name: what they refuse shows on the field, and the dialog stays
 * open. The sidebar follows by `boards_changed`.
 */

import { PromptDialog } from "../components";
import { toUnfoldError, type ProjectId } from "../ipc";
import { renameFolder, setBoardFolder } from "../state/boards";
import type { MoveToFolderRequest } from "./moveToFolder";

/** Closes the dialog once `work` is done; why the engine refused it otherwise. */
async function refusal(work: Promise<unknown>, onClose: () => void): Promise<string | null> {
  try {
    await work;
  } catch (error) {
    return toUnfoldError(error).message;
  }
  onClose();
  return null;
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
      onCancel={onClose}
      onSubmit={(value) => {
        if (value === folder) {
          onClose();
          return null;
        }
        return refusal(renameFolder(project, folder, value), onClose);
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
      onCancel={onClose}
      onSubmit={(value) => {
        if (value === request.folder) {
          onClose();
          return null;
        }
        return refusal(setBoardFolder(request.board, value), onClose);
      }}
    />
  );
}
