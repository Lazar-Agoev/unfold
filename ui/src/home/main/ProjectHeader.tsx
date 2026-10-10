/**
 * The project header on Home (01 "Project header", 06, 11): the path, the 32px name, the project menu,
 * Settings and New board, then the branch and worktree switcher and the state line. A project that
 * isn't compiled yet shows only its menu and the state line (06).
 */

import { Button } from "../../components";
import type { ProjectRow, ProjectSummary } from "../../ipc";
import { displayPath } from "../../lib/format";
import { ProjectMenuButton, WorktreeButton } from "../../menus";
import { openDialog } from "../../state/overlays";
import { newBoardIn } from "../actions";
import { StateLine } from "./StateLine";

export interface ProjectHeaderProps {
  readonly row: ProjectRow;
  readonly summary: ProjectSummary | undefined;
  readonly compact?: boolean;
}

/** The branch the not-compiled state line names, or a detached commit. */
function branchOf(row: ProjectRow): string | null {
  return row.git ? (row.git.branch ?? row.git.detached) : null;
}

export function ProjectHeader({ row, summary, compact = false }: ProjectHeaderProps) {
  const state = <StateLine project={row.id} state={row.state} summary={summary} branch={branchOf(row)} />;
  return (
    <header className={compact ? "home-header home-header-compact" : "home-header"}>
      <p className="home-path">{displayPath(row.path)}</p>
      <div className="home-title-row">
        <h1 className="home-title">{row.name}</h1>
        <div className="home-header-actions">
          <ProjectMenuButton project={row.id} variant="header" />
          {compact ? null : (
            <>
              <Button
                variant="secondary"
                onClick={() => {
                  openDialog({ kind: "projectSettings", project: row.id });
                }}
              >
                Settings
              </Button>
              <Button
                variant="primary"
                icon="plus"
                onClick={() => void newBoardIn(row.id)}
              >
                New board
              </Button>
            </>
          )}
        </div>
      </div>
      {compact ? (
        state
      ) : (
        <div className="home-meta">
          {row.git ? <WorktreeButton project={row.id} /> : null}
          {state}
        </div>
      )}
    </header>
  );
}
