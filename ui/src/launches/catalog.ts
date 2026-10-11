/** Finding one launch in the entrance catalog's overview (every application's launches). */

import type { EntrancesView, LaunchRow } from "../ipc/generated";

export function findLaunch(view: EntrancesView, test: (row: LaunchRow) => boolean): LaunchRow | null {
  if (view.view !== "overview") {
    return null;
  }
  for (const group of view.categories) {
    for (const application of group.applications) {
      const row = application.launches.find(test);
      if (row) {
        return row;
      }
    }
  }
  return null;
}
