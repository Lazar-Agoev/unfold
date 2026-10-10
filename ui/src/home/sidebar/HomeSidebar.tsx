/**
 * The Home-mode sidebar (home-sidebar.md): the workspace row with search, Recents and Drafts (V-65),
 * the projects list with each project's state (07) and its folder and board tree (V-51, V-52), and the
 * footer with Add project and Preferences.
 */

import type { JSX } from "react";
import { IconButton, Monogram } from "../../components";
import { Icon, type IconName } from "../../icons";
import { useEngineQuery } from "../../ipc";
import { ActionIds, useShortcut } from "../../keyboard";
import { formatCount } from "../../lib/format";
import { addProject, openPreferences } from "../../state/actions";
import { useAuthor, authorName } from "../../state/author";
import { setHomeView, useLayout, type HomeView } from "../../state/layout";
import { openAdd } from "../../state/overlays";
import { activateHome } from "../../state/tabs";
import { useWorkspace } from "../../state/workspace";
import { ProjectsList } from "./ProjectsList";
import "./sidebar.css";

/** Drafts lists at most this many (contract §4.2); the count reads "500+" there (V-65). */
const DRAFTS_CAP = 500;

function workspaceName(author: string | null): string {
  if (author === null) {
    return "Workspace";
  }
  const name = authorName(author);
  return `${name.charAt(0).toUpperCase()}${name.slice(1)}'s workspace`;
}

function NavRow({ icon, label, view, count }: { readonly icon: IconName; readonly label: string; readonly view: HomeView; readonly count?: string }) {
  const selected = useLayout((layout) => layout.home.view === view);
  return (
    <button
      type="button"
      className="home-nav-row"
      aria-current={selected ? "page" : undefined}
      onClick={() => {
        setHomeView(view);
        activateHome();
      }}
    >
      <span className="home-nav-icon">
        <Icon name={icon} />
      </span>
      <span className="home-nav-label">{label}</span>
      {count === undefined ? null : <span className="home-nav-count">{count}</span>}
    </button>
  );
}

export function HomeSidebar(): JSX.Element {
  const workspace = useWorkspace();
  const name = workspaceName(useAuthor());
  const projects = workspace?.projects ?? [];
  const drafts = useEngineQuery("drafts", {}, { enabled: projects.length > 0 });
  // No count without drafts (05 draws none).
  const draftCount = drafts.data && drafts.data.drafts.length > 0 ? formatCount(drafts.data.drafts.length, DRAFTS_CAP) : undefined;
  const addShortcut = useShortcut(ActionIds.add) ?? undefined;
  const projectShortcut = useShortcut(ActionIds.addProject) ?? undefined;
  const preferencesShortcut = useShortcut(ActionIds.preferences) ?? undefined;

  return (
    <div className="home-sidebar">
      <div className="home-workspace-row">
        <Monogram name={name} size={20} tone="workspace" />
        <span className="home-workspace-name">{name}</span>
        <IconButton
          icon="search"
          label="Add…"
          {...(addShortcut === undefined ? {} : { shortcut: addShortcut })}
          onClick={() => {
            openAdd();
          }}
        />
      </div>
      <nav className="home-nav" aria-label="Boards across projects">
        <NavRow icon="clock" label="Recents" view="recents" />
        <NavRow icon="drafts" label="Drafts" view="drafts" {...(draftCount === undefined ? {} : { count: draftCount })} />
      </nav>
      <div className="home-projects-header">
        <h2 className="home-projects-label">Projects</h2>
        <IconButton
          icon="plus"
          label="Add Project…"
          size={20}
          {...(projectShortcut === undefined ? {} : { shortcut: projectShortcut })}
          onClick={() => void addProject()}
        />
      </div>
      {workspace && projects.length === 0 ? <p className="home-projects-empty">No projects yet</p> : <ProjectsList projects={projects} />}
      <div className="home-sidebar-footer">
        <button type="button" className="home-footer-add" onClick={() => void addProject()}>
          <span className="home-nav-icon">
            <Icon name="plus" />
          </span>
          Add project
        </button>
        <IconButton
          icon="gear"
          label="Preferences…"
          tooltipSide="top"
          {...(preferencesShortcut === undefined ? {} : { shortcut: preferencesShortcut })}
          onClick={openPreferences}
        />
      </div>
    </div>
  );
}
