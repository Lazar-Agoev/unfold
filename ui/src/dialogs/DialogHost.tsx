/**
 * The one host for every dialog: the window's `DialogRequest` (one at a time) and this
 * module's Move to folder prompt.
 */

import type { JSX } from "react";
import { GroupFromLinesDialog } from "../board";
import { PreferencesDialog } from "../preferences";
import { EntranceEditorDialog, ProjectSettingsDialog, RemoveEntranceDialog } from "../settings";
import { closeDialog, useDialog, type DialogRequest } from "../state/overlays";
import { DeleteBoardDialog, DiscardDialog } from "./BoardDialogs";
import { MoveToFolderDialog, NewFolderDialog, RenameFolderDialog } from "./FolderDialogs";
import { closeMoveToFolder, useMoveToFolder } from "./moveToFolder";
import { PublishDialog } from "./PublishDialog";
import "./dialogs.css";

function DialogFor({ request }: { readonly request: DialogRequest }): JSX.Element | null {
  switch (request.kind) {
    case "preferences":
      return <PreferencesDialog onClose={closeDialog} />;
    case "projectSettings":
      return <ProjectSettingsDialog project={request.project} section={request.section} onClose={closeDialog} />;
    case "entranceEditor":
      return <EntranceEditorDialog request={request} onClose={closeDialog} />;
    case "removeEntrance":
      return <RemoveEntranceDialog request={request} onClose={closeDialog} />;
    case "publish":
      return <PublishDialog board={request.board} onClose={closeDialog} />;
    case "discard":
      return <DiscardDialog board={request.board} onClose={closeDialog} />;
    case "deleteBoard":
      return <DeleteBoardDialog board={request.board} onClose={closeDialog} />;
    case "newFolder":
      return <NewFolderDialog project={request.project} parent={request.parent} onClose={closeDialog} />;
    case "renameFolder":
      return <RenameFolderDialog project={request.project} path={request.path} onClose={closeDialog} />;
    case "groupFromLines":
      return <GroupFromLinesDialog request={request} onClose={closeDialog} />;
  }
}

export function DialogHost(): JSX.Element | null {
  const request = useDialog();
  const move = useMoveToFolder((state) => state.request);
  // A fresh dialog for each request, so a form never keeps another request's draft.
  return (
    <>
      {request ? <DialogFor key={JSON.stringify(request)} request={request} /> : null}
      {move ? <MoveToFolderDialog key={JSON.stringify(move)} request={move} onClose={closeMoveToFolder} /> : null}
    </>
  );
}
