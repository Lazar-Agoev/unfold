/** A compiled project's Home overview (01): How it starts, Boards, and the Health line. */

import type { ProjectId, ProjectSummary } from "../../ipc";
import { BoardsSection } from "../boards/BoardsSection";
import { HealthStrip } from "./HealthStrip";
import { HowItStarts } from "./HowItStarts";
import "./overview.css";

export function Overview({ project, summary }: { readonly project: ProjectId; readonly summary: ProjectSummary | undefined }) {
  return (
    <>
      <HowItStarts project={project} summary={summary} />
      <BoardsSection project={project} />
      {summary ? <HealthStrip project={project} summary={summary} /> : null}
    </>
  );
}
