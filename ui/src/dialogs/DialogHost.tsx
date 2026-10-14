/**
 * The one host for every dialog: the window's `DialogRequest` (one at a time) and this
 * module's Move to folder prompt.
 */

import type { JSX } from "react";
import { SettingsDialog } from "../settings";
import { closeDialog, useDialog, type DialogRequest } from "../state/overlays";
import { DeleteBoardDialog } from "./BoardDialogs";
import { MoveToFolderDialog, RenameFolderDialog } from "./FolderDialogs";
import { closeMoveToFolder, useMoveToFolder } from "./moveToFolder";

function DialogFor({ request }: { readonly request: DialogRequest }): JSX.Element | null {
  switch (request.kind) {
    case "settings":
      return <SettingsDialog onClose={closeDialog} />;
    case "deleteBoard":
      return <DeleteBoardDialog board={request.board} onClose={closeDialog} />;
    case "renameFolder":
      return <RenameFolderDialog project={request.project} folder={request.folder} onClose={closeDialog} />;
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
