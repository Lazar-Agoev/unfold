/** The project header on Home: the path, the 32px name, the project menu and New board, then the state line. */

import { Button } from "../../components";
import type { ProjectRow, ProjectSummary } from "../../ipc";
import { displayPath } from "../../lib/format";
import { ProjectMenuButton } from "../../menus";
import { newBoardIn } from "../actions";
import { StateLine } from "./StateLine";

export interface ProjectHeaderProps {
  readonly row: ProjectRow;
  readonly summary: ProjectSummary | undefined;
}

export function ProjectHeader({ row, summary }: ProjectHeaderProps) {
  return (
    <header className="home-header">
      <p className="home-path">{displayPath(row.path)}</p>
      <div className="home-title-row">
        <h1 className="home-title">{row.name}</h1>
        <div className="home-header-actions">
          <ProjectMenuButton project={row.id} variant="header" />
          <Button variant="primary" icon="plus" onClick={() => void newBoardIn(row.id)}>
            New board
          </Button>
        </div>
      </div>
      <StateLine project={row.id} state={row.state} summary={summary} />
    </header>
  );
}
