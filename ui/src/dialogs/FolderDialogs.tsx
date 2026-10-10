/**
 * Board folder prompts (V-51), 400 wide over the content area: New folder…, Rename…, and Move to
 * folder (the board menu's "New folder…", Add's command). Failures are toasts; the sidebar follows by
 * `boards_changed`.
 */

import { PromptDialog, showToast } from "../components";
import { toUnfoldError, type ProjectId } from "../ipc";
import { createFolder, renameFolder, setBoardFolder, type BoardRef } from "../state/boards";
import { setFolderExpanded } from "../state/layout";
import { useProjectRow } from "../state/workspace";
import { childPath, folderProblem, splitPath } from "./folders";
import type { MoveToFolderRequest } from "./moveToFolder";

function fail(title: string, error: unknown): void {
  const failure = toUnfoldError(error);
  showToast({ title, body: failure.code === "folder_exists" ? "A folder with that name is already there." : failure.message, tone: "danger" });
}

/** A folder's name, one segment. */
function nameProblem(value: string): string | null {
  return value.includes("/") ? "A folder's name can't contain “/”." : folderProblem(value);
}

export function NewFolderDialog({ project, parent, onClose }: { readonly project: ProjectId; readonly parent: string | null; readonly onClose: () => void }) {
  const name = useProjectRow(project)?.name ?? "the project";
  return (
    <PromptDialog
      open
      title="New folder"
      description={parent === null ? `At the top level of ${name}'s boards.` : `Inside ${parent}.`}
      label="Name"
      confirmLabel="Create"
      placement="canvas"
      validate={nameProblem}
      onCancel={onClose}
      onSubmit={(value) => {
        onClose();
        const path = childPath(parent, value);
        createFolder(project, path).then(
          () => {
            if (parent !== null) {
              setFolderExpanded(project, parent, true);
            }
          },
          (error: unknown) => {
            fail("Couldn't create the folder", error);
          },
        );
      }}
    />
  );
}

export function RenameFolderDialog({ project, path, onClose }: { readonly project: ProjectId; readonly path: string; readonly onClose: () => void }) {
  const { parent, name } = splitPath(path);
  return (
    <PromptDialog
      open
      title="Rename folder"
      description="Its boards move with it. Published boards get the move as an unpublished change."
      label="Name"
      defaultValue={name}
      confirmLabel="Rename"
      placement="canvas"
      validate={nameProblem}
      onCancel={onClose}
      onSubmit={(value) => {
        onClose();
        if (value === name) {
          return;
        }
        renameFolder(project, path, childPath(parent, value)).catch((error: unknown) => {
          fail("Couldn't rename the folder", error);
        });
      }}
    />
  );
}

/** Moves a board into a folder (an empty path is the top level). */
async function moveBoard(board: BoardRef, folder: string | null): Promise<void> {
  try {
    await setBoardFolder(board, folder);
  } catch (error) {
    fail("Couldn't move the board", error);
  }
}

export function MoveToFolderDialog({ request, onClose }: { readonly request: MoveToFolderRequest; readonly onClose: () => void }) {
  const fresh = request.mode === "new";
  return (
    <PromptDialog
      open
      title={fresh ? "New folder" : "Move to folder"}
      description={fresh ? "Moves the board into a new folder. Use “/” for a folder inside another." : "Type the folder's path. Leave it empty for the top level."}
      label={fresh ? "Name" : "Folder"}
      defaultValue={fresh ? "" : (request.folder ?? "")}
      confirmLabel="Move"
      placement="canvas"
      validate={(value) => (value.trim() === "" && !fresh ? null : folderProblem(value))}
      onCancel={onClose}
      onSubmit={(value) => {
        onClose();
        const folder = value.trim() === "" ? null : value;
        if (folder !== request.folder) {
          void moveBoard(request.board, folder);
        }
      }}
    />
  );
}
