/** Asks before removing a configured entrance (a launch or a handler) from a project's `unfold.toml`. */

import { useQueryClient } from "@tanstack/react-query";
import { ConfirmDialog, showToast } from "../components";
import { engineKey, type ProjectConfig } from "../ipc";
import { applySettingsEdits, problemText } from "../settings/edits";
import type { DialogRequest } from "../state/overlays";

type RemoveRequest = Extract<DialogRequest, { readonly kind: "removeEntrance" }>;

export function RemoveEntranceDialog({ request, onClose }: { readonly request: RemoveRequest; readonly onClose: () => void }) {
  const client = useQueryClient();
  const { project, id, name } = request;
  return (
    <ConfirmDialog
      open
      danger
      title="Remove from unfold.toml"
      message={`Remove ${name} from unfold.toml? Everyone who shares the repository loses it too.`}
      confirmLabel="Remove"
      placement="window"
      onCancel={onClose}
      onConfirm={() => {
        onClose();
        void applySettingsEdits(project, [{ op: "remove_entrance", id }]).then((outcome) => {
          if (!outcome.ok) {
            showToast({ title: `Couldn't remove ${name}`, body: problemText(outcome.problem), tone: "danger" });
            return;
          }
          client.setQueryData<ProjectConfig>(engineKey("project_config", { project }), outcome.config);
          showToast({ title: `Removed ${name}`, body: "The graph updates in the background." });
        });
      }}
    />
  );
}
