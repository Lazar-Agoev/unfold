/** Placeholder Home (U1c): the projects, and the selected project's boards, until U3 builds Home. */

import type { JSX } from "react";
import { Button, EmptyState, IconButton, ListRow, Monogram, SectionLabel, StateIndicator } from "../components";
import { Icon } from "../icons";
import { useEngineQuery } from "../ipc";
import { ProjectMenuButton } from "../menus";
import { displayPath } from "../lib/format";
import { addProject, compileHere, newBoardHere, openPreferences, showProject } from "../state/actions";
import { openBoard } from "../state/boards";
import { useSelectedProject } from "../state/window";
import { useProjectRow, useWorkspace } from "../state/workspace";
import "./home.css";

export function HomeSidebar(): JSX.Element {
  const workspace = useWorkspace();
  const selected = useSelectedProject();
  return (
    <div className="home-stub-sidebar">
      <SectionLabel>Projects</SectionLabel>
      <div className="home-stub-rows">
        {(workspace?.projects ?? []).map((row) => (
          <ListRow
            key={row.id}
            role="button"
            tabIndex={0}
            data-project-id={row.id}
            leading={<Monogram name={row.name} size={14} />}
            label={row.name}
            trailing={<StateIndicator state={row.state} variant="row" />}
            selected={row.id === selected}
            menu={<ProjectMenuButton project={row.id} variant="row" />}
            onClick={() => void showProject(row.id)}
          />
        ))}
      </div>
      <div className="home-stub-footer">
        <ListRow role="button" tabIndex={0} leading={<Icon name="plus" />} label="Add project" onClick={() => void addProject()} />
        <IconButton icon="gear" label="Preferences" onClick={openPreferences} />
      </div>
    </div>
  );
}

export function HomeMain(): JSX.Element {
  const project = useSelectedProject();
  const row = useProjectRow(project);
  const boards = useEngineQuery("list_boards", { project: project ?? "" }, { enabled: project !== null });
  if (!row) {
    return (
      <div className="home-stub">
        <EmptyState
          title="Add a project"
          body="Drop a folder on the window, or choose one."
          action={
            <Button variant="primary" onClick={() => void addProject()}>
              Add project
            </Button>
          }
        />
      </div>
    );
  }
  return (
    <div className="home-stub">
      <header className="home-stub-header">
        <h1 className="home-stub-title">{row.name}</h1>
        <p className="home-stub-path">{displayPath(row.path)}</p>
        <div className="home-stub-actions">
          <Button variant="secondary" onClick={() => void compileHere()}>
            Compile
          </Button>
          <Button variant="primary" icon="plus" onClick={() => void newBoardHere()}>
            New board
          </Button>
        </div>
      </header>
      <SectionLabel>Boards</SectionLabel>
      <div className="home-stub-rows">
        {(boards.data?.boards ?? []).map((board) => (
          <ListRow
            key={board.id}
            role="button"
            tabIndex={0}
            data-board-key={`${board.project}\u0000${board.id}`}
            leading={<Icon name={board.place === "draft" ? "board-draft" : "board"} />}
            label={board.title ?? board.name}
            onClick={() => void openBoard({ project: board.project, id: board.id })}
          />
        ))}
      </div>
    </div>
  );
}
