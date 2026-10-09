/** The settings dialogs and actions, mounted once per window by the shell. */

import { useSettingsActions } from "./actions";
import { EntranceDialogs } from "./EntranceEditor";
import { FolderSettingsDialog } from "./FolderSettingsDialog";

export function SettingsDialogs() {
  useSettingsActions();
  return (
    <>
      <EntranceDialogs />
      <FolderSettingsDialog />
    </>
  );
}
