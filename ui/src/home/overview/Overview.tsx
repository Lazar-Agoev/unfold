/** A project's Home overview: Boards, and the Health line. */

import type { ProjectId, ProjectSummary } from "../../ipc";
import { BoardsSection } from "../boards/BoardsSection";
import { HealthLine } from "./HealthLine";
import "./overview.css";

export function Overview({ project, summary }: { readonly project: ProjectId; readonly summary: ProjectSummary | undefined }) {
  return (
    <>
      <BoardsSection project={project} />
      {summary ? <HealthLine project={project} summary={summary} /> : null}
    </>
  );
}
