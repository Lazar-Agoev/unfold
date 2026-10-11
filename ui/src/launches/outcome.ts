/**
 * What a save says once `unfold.toml` is written: **Waiting for compile** when the graph won't pick
 * the change up by itself (background updates off, or never compiled), else **Saved**; and for a
 * launch, **Compiled with diagnostics** when the update that follows finds it can't be read.
 */

import { showToast } from "../components";
import { call, onEngineEvent, type ProjectId, type ProjectSummary } from "../ipc";
import { findLaunch } from "./catalog";

/** How long a saved launch is watched for the update that compiles it. */
const WATCH_MS = 5 * 60_000;

/** Whether the graph waits for a Compile before it shows the project's saved settings. */
export function waitsForCompile(summary: ProjectSummary | undefined): boolean {
  return summary !== undefined && (!summary.status.backgroundUpdates || summary.state.kind === "not_compiled" || summary.state.kind === "paused");
}

/** After the next graph update, tells whether the launch `id` (shown as `name`) compiled with diagnostics. */
function watchLaunch(project: ProjectId, id: string, name: string): void {
  const finish = () => {
    stop();
    clearTimeout(timer);
  };
  const stop = onEngineEvent("graph_updated", (event) => {
    if (event.project !== project) {
      return;
    }
    finish();
    call("entrances", { project })
      .then((view) => {
        const row = findLaunch(view, (launch) => launch.id === id);
        const problem = row && row.status !== "ok" ? (row.problem ?? row.note) : null;
        if (problem) {
          showToast({ title: "Compiled with diagnostics", body: `${name}: ${problem}` });
        }
      })
      .catch(() => {
        // Home and Add show the launch's state once it loads; the toast is only a shortcut.
      });
  });
  const timer = setTimeout(finish, WATCH_MS);
}

/** Says how a save went; `launch` is the saved launch's ID and name, to watch it compile. */
export function reportSaved(project: ProjectId, summary: ProjectSummary | undefined, saved: string, launch: { readonly id: string; readonly name: string } | null): void {
  if (waitsForCompile(summary)) {
    showToast({ title: "Waiting for compile", body: `${saved} Compile to see it in the graph.` });
    return;
  }
  showToast({ title: "Saved", body: `${saved} The graph updates in the background.` });
  if (launch) {
    watchLaunch(project, launch.id, launch.name);
  }
}
