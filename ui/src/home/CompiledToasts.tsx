/**
 * "<project> compiled": when a Compile this window started ends well, a toast with
 * the project's monogram, how many entrances it found, and Open Home, unless this window's Home is
 * already showing that project. Mounted once per window, so it speaks from a board tab too.
 */

import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useEffect, useRef, type JSX } from "react";
import { Monogram, showToast } from "../components";
import { engineQueryOptions, type ProjectId } from "../ipc";
import { plural } from "../lib/format";
import { showProject } from "../state/actions";
import { useLayout } from "../state/layout";
import { useTabs } from "../state/tabs";
import { useWindowStore } from "../state/window";
import { getProjectIndex, useWorkspace } from "../state/workspace";
import { useCompileStartedHere } from "../state/compile";

/** Whether this window's Home shows the project now (its overview, Health, or Activity). */
function homeShows(project: ProjectId): boolean {
  const view = useLayout.getState().home.view;
  return (
    useTabs.getState().active === "home" &&
    useWindowStore.getState().selectedProject === project &&
    (view === "overview" || view === "health" || view === "activity")
  );
}

async function announceCompiled(client: QueryClient, project: ProjectId): Promise<void> {
  if (homeShows(project)) {
    return;
  }
  const summary = await client.query({ ...engineQueryOptions("project_summary", { project }), staleTime: 0 });
  // A cancelled first Compile leaves the project not compiled; a failed one says so on its own.
  if (summary.state.kind === "failed" || summary.state.kind === "not_compiled" || homeShows(project)) {
    return;
  }
  const name = getProjectIndex().byId.get(project)?.name ?? summary.name;
  showToast({
    id: `compiled:${project}`,
    title: `${name} compiled`,
    body: `${plural(summary.entrances, "entrance")} found`,
    leading: <Monogram name={name} size={20} />,
    action: { label: "Open Home", run: () => void showProject(project) },
  });
}

/** Watches one project's Compile from this window and announces its end. */
function CompileWatch({ project }: { readonly project: ProjectId }): null {
  const client = useQueryClient();
  const running = useCompileStartedHere(project);
  const was = useRef(running);
  useEffect(() => {
    if (was.current && !running) {
      announceCompiled(client, project).catch((error: unknown) => {
        console.warn("Couldn't read the compiled project's summary:", error);
      });
    }
    was.current = running;
  }, [client, project, running]);
  return null;
}

export function CompiledToasts(): JSX.Element {
  const projects = useWorkspace()?.projects ?? [];
  return (
    <>
      {projects.map((row) => (
        <CompileWatch key={row.id} project={row.id} />
      ))}
    </>
  );
}
