/**
 * Home's main area (V-47): one of the selected project's overview, Health, or Activity; the Recents or
 * Drafts board grid; first launch for an empty workspace (05); the first-Compile setup for a project
 * that was never compiled (06). It scrolls as one; the sidebar stays put.
 */

import { useState, type JSX, type ReactNode } from "react";
import { EmptyState } from "../../components";
import { useEngineQuery, type ProjectRow } from "../../ipc";
import { useLayout } from "../../state/layout";
import { useSelectedProject } from "../../state/window";
import { useProjectRow, useWorkspace } from "../../state/workspace";
import { DraftsView, RecentsView } from "../boards/CrossProjectViews";
import { FirstLaunch } from "../firstLaunch/FirstLaunch";
import { HealthActivity } from "../health/HealthActivity";
import { NotCompiled } from "../notCompiled/NotCompiled";
import { Overview } from "../overview/Overview";
import { homeStateKind } from "../stateLine";
import { ScrollArea } from "../windowed";
import { ProjectHeader } from "./ProjectHeader";
import "./main.css";

function ProjectHome({ row }: { readonly row: ProjectRow }) {
  const view = useLayout((layout) => layout.home.view);
  const unavailable = row.state.kind === "unavailable" ? row.state.reason : null;
  const summary = useEngineQuery("project_summary", { project: row.id }, { enabled: unavailable === null });
  if (homeStateKind(row.state) === "not_compiled") {
    return <NotCompiled row={row} summary={summary.data} />;
  }
  let body: ReactNode;
  if (unavailable !== null) {
    body = <EmptyState title="This project isn't available" body={unavailable} />;
  } else if (summary.error) {
    body = <EmptyState title="Couldn't read this project" body={summary.error.message} />;
  } else if (view === "health" || view === "activity") {
    body = <HealthActivity project={row.id} view={view} summary={summary.data} />;
  } else {
    body = <Overview project={row.id} summary={summary.data} />;
  }
  return (
    <div className="home-page">
      <ProjectHeader row={row} summary={summary.data} />
      {body}
    </div>
  );
}

function MainView() {
  const view = useLayout((layout) => layout.home.view);
  const workspace = useWorkspace();
  const row = useProjectRow(useSelectedProject());
  if (view === "recents") {
    return <RecentsView />;
  }
  if (view === "drafts") {
    return <DraftsView />;
  }
  if (!workspace) {
    return null;
  }
  if (workspace.projects.length === 0) {
    return <FirstLaunch removed={workspace.removed} />;
  }
  return row ? <ProjectHome key={row.id} row={row} /> : null;
}

export function HomeMain(): JSX.Element {
  const [scroller, setScroller] = useState<HTMLDivElement | null>(null);
  return (
    <ScrollArea.Provider value={scroller}>
      <div ref={setScroller} className="home-main">
        <MainView />
      </div>
    </ScrollArea.Provider>
  );
}
