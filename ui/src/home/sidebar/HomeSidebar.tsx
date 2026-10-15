/**
 * The Home-mode sidebar: the workspace row, the projects list with each project's state
 * and its folder and board tree, and the footer with Add project and Settings.
 */

import type { JSX } from "react";
import { IconButton, Monogram } from "../../components";
import { Icon } from "../../icons";
import { addProject, openSettings } from "../../state/actions";
import { authorName } from "../../code/text";
import { useAuthor } from "../../state/author";
import { useWorkspace } from "../../state/workspace";
import { ProjectsList } from "./ProjectsList";
import "./sidebar.css";

function workspaceName(author: string | null): string {
  return author === null ? "Workspace" : `${authorName(author)}'s workspace`;
}

export function HomeSidebar(): JSX.Element {
  const workspace = useWorkspace();
  const name = workspaceName(useAuthor());
  const projects = workspace?.projects ?? [];

  return (
    <div className="home-sidebar">
      <div className="home-workspace-row">
        <Monogram name={name} size={20} tone="workspace" />
        <span className="home-workspace-name">{name}</span>
      </div>
      <div className="home-projects-header">
        <h2 className="home-projects-label">Projects</h2>
        <IconButton
          icon="plus"
          label="Add Project…"
          size={20}
          onClick={() => void addProject()}
        />
      </div>
      {workspace && projects.length === 0 ? <p className="home-projects-empty">No projects yet</p> : <ProjectsList projects={projects} />}
      <div className="home-sidebar-footer">
        <button type="button" className="home-footer-add" onClick={() => void addProject()}>
          <span className="home-footer-icon">
            <Icon name="plus" />
          </span>
          Add project
        </button>
        <IconButton
          icon="gear"
          label="Settings"
          tooltipSide="top"
          onClick={openSettings}
        />
      </div>
    </div>
  );
}
