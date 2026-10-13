/**
 * Home's main area: the selected project's overview or Health; or, for an empty workspace, Add
 * project. It scrolls as one; the sidebar stays put.
 */

import { useState, type JSX, type ReactNode } from "react";
import { EmptyState } from "../../components";
import { useEngineQuery, type ProjectRow } from "../../ipc";
import { useLayout } from "../../state/layout";
import { useSelectedProject } from "../../state/window";
import { useProjectRow, useWorkspace } from "../../state/workspace";
import { EmptyWorkspace } from "../emptyWorkspace/EmptyWorkspace";
import { HealthView } from "../health/HealthView";
import { Overview } from "../overview/Overview";
import { ScrollArea } from "../windowed";
import { ProjectHeader } from "./ProjectHeader";
import "./main.css";

function ProjectHome({ row }: { readonly row: ProjectRow }) {
  const view = useLayout((layout) => layout.home.view);
  const unavailable = row.state.kind === "unavailable" ? row.state.reason : null;
  const summary = useEngineQuery("project_summary", { project: row.id }, { enabled: unavailable === null });
  let body: ReactNode;
  if (unavailable !== null) {
    body = <EmptyState title="This project isn't available" body={unavailable} />;
  } else if (summary.error) {
    body = <EmptyState title="Couldn't read this project" body={summary.error.message} />;
  } else if (view === "health") {
    body = <HealthView project={row.id} summary={summary.data} />;
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
  const workspace = useWorkspace();
  const row = useProjectRow(useSelectedProject());
  if (!workspace) {
    return null;
  }
  if (workspace.projects.length === 0) {
    return <EmptyWorkspace />;
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
