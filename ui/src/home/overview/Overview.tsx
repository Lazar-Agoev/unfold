/** A project's Home overview: How it starts, Boards, and the Health line. */

import { useState } from "react";
import type { ProjectId, ProjectSummary } from "../../ipc";
import { BoardsSection } from "../boards/BoardsSection";
import { HowItStarts } from "../starts/HowItStarts";
import { HealthLine } from "./HealthLine";
import "./overview.css";

export function Overview({ project, summary }: { readonly project: ProjectId; readonly summary: ProjectSummary | undefined }) {
  // Each click of the Health line's "code isn't found" shows How it starts' problems again.
  const [highlight, setHighlight] = useState(0);
  return (
    <>
      <HowItStarts project={project} highlight={highlight} />
      <BoardsSection project={project} />
      {summary ? (
        <HealthLine
          project={project}
          summary={summary}
          onShowEntrances={() => {
            setHighlight((count) => count + 1);
          }}
        />
      ) : null}
    </>
  );
}
