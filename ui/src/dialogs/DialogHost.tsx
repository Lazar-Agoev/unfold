/** Placeholder dialog host (U1c): the dialogs that exist already, and a stand-in for the rest until U5. */

import type { JSX } from "react";
import { Dialog } from "../components";
import { PreferencesDialog } from "../preferences";
import { EntranceEditorDialog, RemoveEntranceDialog } from "../settings";
import { closeDialog, useDialog, type DialogRequest } from "../state/overlays";

function DialogFor({ request }: { readonly request: DialogRequest }): JSX.Element {
  switch (request.kind) {
    case "preferences":
      return <PreferencesDialog onClose={closeDialog} />;
    case "entranceEditor":
      return <EntranceEditorDialog request={request} onClose={closeDialog} />;
    case "removeEntrance":
      return <RemoveEntranceDialog request={request} onClose={closeDialog} />;
    default:
      return (
        <Dialog open title={request.kind} width={400} placement="window" onClose={closeDialog}>
          <p>This dialog isn't built yet.</p>
        </Dialog>
      );
  }
}

export function DialogHost(): JSX.Element | null {
  const request = useDialog();
  // A fresh dialog for each request, so a form never keeps another request's draft.
  return request ? <DialogFor key={JSON.stringify(request)} request={request} /> : null;
}
