/** Placeholder Add box (U1c): opens and closes like the real one, until U4 builds it. */

import type { JSX } from "react";
import { Dialog } from "../components";
import { ActionIds, useRegisterActions } from "../keyboard";
import { closeAdd, openAdd, useAddRequest } from "../state/overlays";

export function AddBox(): JSX.Element | null {
  useRegisterActions([
    { id: ActionIds.add, title: "Add…", category: "Go", icon: "plus", run: () => { openAdd(); } },
    { id: ActionIds.addCommands, title: "Commands…", category: "Go", run: () => { openAdd({ mode: "commands" }); } },
    { id: ActionIds.addFiles, title: "Go to File…", category: "Go", run: () => { openAdd({ mode: "find", kind: "files" }); } },
    { id: ActionIds.addFunctions, title: "Go to Function…", category: "Go", run: () => { openAdd({ mode: "find", kind: "functions" }); } },
  ]);
  const request = useAddRequest();
  return request ? (
    <Dialog open title="Add" width={520} placement="window" onClose={closeAdd}>
      <p>Search arrives with the Add box.</p>
    </Dialog>
  ) : null;
}
