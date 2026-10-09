/**
 * Dropping folders anywhere on the window adds them to the workspace as projects (UI plan §7.1); the
 * last one added shows on Home. Only items of this drop are accepted (the preload checks), and the
 * engine checks each is a folder.
 */

import type { DragEvent } from "react";
import { showToast } from "../components";
import { addDroppedFolder, toUnfoldError, type DroppedFile, type ProjectId } from "../ipc";
import { showProject } from "../state/actions";

function carriesFiles(event: DragEvent): boolean {
  return event.dataTransfer.types.includes("Files");
}

/** Lets files be dropped (a drop target has to accept the drag first). */
export function onWindowDragOver(event: DragEvent): void {
  if (!event.defaultPrevented && carriesFiles(event)) {
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
  }
}

/** Adds every dropped folder, in order. A drop a view handled itself (`defaultPrevented`) is left alone. */
export function onWindowDrop(event: DragEvent): void {
  if (event.defaultPrevented || !carriesFiles(event)) {
    return;
  }
  // Handled here, so the preload doesn't add the folder a second time.
  event.preventDefault();
  void addDroppedFolders(Array.from(event.dataTransfer.files));
}

/** Adds the folders one by one, says why any can't be added, then shows the last one added. */
export async function addDroppedFolders(files: readonly DroppedFile[]): Promise<void> {
  let added: ProjectId | null = null;
  for (const file of files) {
    try {
      added = (await addDroppedFolder(file)).project;
    } catch (error) {
      const failure = toUnfoldError(error);
      showToast(
        failure.code === "not_a_folder"
          ? { title: `${file.name} isn't a folder`, body: "Drop a project's folder to add it.", tone: "danger" }
          : { title: `Couldn't add ${file.name}`, body: failure.message, tone: "danger" },
      );
    }
  }
  if (added !== null) {
    await showProject(added);
  }
}
