/**
 * The projects list: a project row selects the project and shows its Home overview; its chevron only
 * expands it; under an expanded project, its folders and boards, from `list_boards`. The list scrolls
 * on its own and renders only the rows in view once it is long.
 */

import { useQueries } from "@tanstack/react-query";
import { useMemo, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { Monogram, StateIndicator, showToast, useContextMenu, type MenuEntry } from "../../components";
import { Icon } from "../../icons";
import { engineQueryOptions, toUnfoldError, type BoardList, type ProjectId, type ProjectRow } from "../../ipc";
import { boardMenuEntries, ProjectMenuButton, projectMenuEntries } from "../../menus";
import { showProject } from "../../state/actions";
import { deleteFolder } from "../../state/boards";
import { setFolderExpanded, useLayout } from "../../state/layout";
import { openDialog } from "../../state/overlays";
import { useSelectedProject } from "../../state/window";
import { setProjectExpanded } from "../../state/workspace";
import { newBoardIn, openBoardFromHome } from "../actions";
import { boardTitle } from "../boards/model";
import { useWindowed } from "../windowed";
import { rowHeight, sidebarRows, type ProjectTree, type SidebarRow } from "./tree";

/** Past this many rows the list renders only the rows in view. */
const WINDOW_THRESHOLD = 120;

/** The branch a project row shows: nothing on the usual default branches. */
function branchOf(row: ProjectRow): string | null {
  const git = row.git;
  if (!git) {
    return null;
  }
  if (git.detached !== null) {
    return git.detached;
  }
  return git.branch === null || git.branch === "main" || git.branch === "master" ? null : git.branch;
}

function toggleProject(row: ProjectRow): void {
  setProjectExpanded(row.id, !row.expanded).catch((error: unknown) => {
    showToast({ title: "Couldn't expand the project", body: toUnfoldError(error).message, tone: "danger" });
  });
}

/** ↑/↓ move between rows; → and ← expand and collapse (a tree's keys). */
function moveFocus(event: KeyboardEvent<HTMLElement>, expand?: (open: boolean) => void): void {
  const row = event.currentTarget;
  if (event.target !== row) {
    return;
  }
  const rows = [...(row.closest(".home-projects")?.querySelectorAll<HTMLElement>("[data-tree-row]") ?? [])];
  const index = rows.indexOf(row);
  const step = { ArrowDown: 1, ArrowUp: -1 }[event.key];
  if (step !== undefined) {
    event.preventDefault();
    rows[index + step]?.focus();
  } else if (expand && (event.key === "ArrowRight" || event.key === "ArrowLeft")) {
    event.preventDefault();
    expand(event.key === "ArrowRight");
  }
}

function Disclosure({ expanded, label, onToggle }: { readonly expanded: boolean; readonly label: string; readonly onToggle: () => void }) {
  return (
    <button
      type="button"
      className="home-disclosure"
      tabIndex={-1}
      aria-label={`${expanded ? "Collapse" : "Expand"} ${label}`}
      onClick={(event) => {
        event.stopPropagation();
        onToggle();
      }}
    >
      <Icon name={expanded ? "chevron-down-small" : "chevron-right-small"} size={10} />
    </button>
  );
}

function ProjectRowView({ row, selected, onContextMenu }: { readonly row: ProjectRow; readonly selected: boolean; readonly onContextMenu: (event: MouseEvent) => void }) {
  const branch = branchOf(row);
  return (
    <div
      role="treeitem"
      tabIndex={0}
      aria-level={1}
      aria-expanded={row.expanded}
      aria-selected={selected}
      data-tree-row=""
      data-project-id={row.id}
      className="home-project-row"
      onClick={() => void showProject(row.id)}
      onContextMenu={onContextMenu}
      onKeyDown={(event) => {
        if (event.key === "Enter" && event.target === event.currentTarget) {
          event.preventDefault();
          void showProject(row.id);
        }
        moveFocus(event, (open) => {
          if (open !== row.expanded) {
            toggleProject(row);
          }
        });
      }}
    >
      <Disclosure
        expanded={row.expanded}
        label={row.name}
        onToggle={() => {
          toggleProject(row);
        }}
      />
      <Monogram name={row.name} size={20} />
      <span className="home-project-name">
        <span className="home-project-title">{row.name}</span>
        {branch === null ? null : <span className="home-project-branch">{branch}</span>}
      </span>
      <StateIndicator state={row.state} variant="row" />
      {/* The menu button is its own control: pressing it never selects the row. */}
      <span
        className="home-row-menu"
        onClick={(event) => {
          event.stopPropagation();
        }}
        onKeyDown={(event) => {
          event.stopPropagation();
        }}
      >
        <ProjectMenuButton project={row.id} variant="row" />
      </span>
    </div>
  );
}

function treePadding(depth: number): number {
  return 24 + 18 * (depth - 1);
}

function folderEntries(row: Extract<SidebarRow, { kind: "folder" }>): MenuEntry[] {
  const { project, path } = row;
  return [
    { id: "new-board", label: "New Board Here", icon: "board", onSelect: () => void newBoardIn(project, { folder: path }) },
    { id: "new-folder", label: "New Folder…", icon: "folder", onSelect: () => { openDialog({ kind: "newFolder", project, parent: path }); } },
    { kind: "separator", id: "s1" },
    { id: "rename", label: "Rename…", onSelect: () => { openDialog({ kind: "renameFolder", project, path }); } },
    {
      id: "delete",
      label: "Delete",
      danger: true,
      disabled: !row.empty,
      onSelect: () => {
        deleteFolder(project, path).catch((error: unknown) => {
          showToast({ title: "Couldn't delete the folder", body: toUnfoldError(error).message, tone: "danger" });
        });
      },
    },
  ];
}

function TreeRow({ row, onContextMenu }: { readonly row: SidebarRow; readonly onContextMenu: (event: MouseEvent, entries: MenuEntry[]) => void }): ReactNode {
  switch (row.kind) {
    case "project":
      return null;
    case "gap":
      return <div aria-hidden="true" style={{ height: row.height }} />;
    case "note":
      return <p className="home-tree-note">{row.text}</p>;
    case "folder": {
      const toggle = (open: boolean) => {
        setFolderExpanded(row.project, row.path, open);
      };
      return (
        <div
          role="treeitem"
          tabIndex={0}
          aria-level={row.depth + 1}
          aria-expanded={row.expanded}
          data-tree-row=""
          className="home-tree-row"
          style={{ paddingLeft: treePadding(row.depth) }}
          onClick={() => {
            toggle(!row.expanded);
          }}
          onContextMenu={(event) => {
            onContextMenu(event, folderEntries(row));
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter" && event.target === event.currentTarget) {
              event.preventDefault();
              toggle(!row.expanded);
            }
            moveFocus(event, toggle);
          }}
        >
          <Disclosure
            expanded={row.expanded}
            label={row.name}
            onToggle={() => {
              toggle(!row.expanded);
            }}
          />
          <span className="home-tree-icon">
            <Icon name="folder" />
          </span>
          <span className="home-tree-label">{row.name}</span>
          <span className="home-tree-trailing" />
        </div>
      );
    }
    case "board": {
      const { board } = row;
      const ref = { project: board.project, id: board.id };
      return (
        <div
          role="treeitem"
          tabIndex={0}
          aria-level={row.depth + 1}
          data-tree-row=""
          data-board-key={`${board.project}\u0000${board.id}`}
          className="home-tree-row"
          style={{ paddingLeft: treePadding(row.depth) }}
          onClick={() => void openBoardFromHome(ref)}
          onContextMenu={(event) => {
            onContextMenu(event, boardMenuEntries(ref));
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter" && event.target === event.currentTarget) {
              event.preventDefault();
              void openBoardFromHome(ref);
            }
            moveFocus(event);
          }}
        >
          <span className="home-disclosure" />
          <span className="home-tree-icon">
            <Icon name={board.place === "draft" ? "board-draft" : "board"} />
          </span>
          <span className="home-tree-label">{boardTitle(board)}</span>
          <span className="home-tree-trailing">{board.pages}</span>
        </div>
      );
    }
  }
}

function treesOf(results: readonly { readonly data?: BoardList | undefined; readonly error: Error | null }[]): ProjectTree[] {
  return results.map((result) => ({ list: result.data, error: result.error ? result.error.message : null }));
}

export function ProjectsList({ projects }: { readonly projects: readonly ProjectRow[] }) {
  const selected = useSelectedProject();
  const view = useLayout((layout) => layout.home.view);
  const folders = useLayout((layout) => layout.folders);
  const expandedFolders = useMemo(() => new Set(folders), [folders]);
  const menu = useContextMenu("Sidebar menu");
  const projectMenu = useContextMenu("Project menu", 264);

  const expanded = useMemo(() => projects.filter((row) => row.expanded).map((row) => row.id), [projects]);
  const trees = useQueries({ queries: expanded.map((project) => engineQueryOptions("list_boards", { project })), combine: treesOf });
  const treeMap = useMemo(() => new Map<ProjectId, ProjectTree>(expanded.map((project, index) => [project, trees[index] ?? { list: undefined, error: null }])), [expanded, trees]);
  const rows = useMemo(() => sidebarRows(projects, treeMap, expandedFolders), [projects, treeMap, expandedFolders]);

  const [scroller, setScroller] = useState<HTMLDivElement | null>(null);
  const { attach, rows: windowRows, height: windowHeight, offsets } = useWindowed({ count: rows.length, size: (index) => rowHeight(rows[index] ?? { kind: "gap", key: "", height: 28 }), threshold: WINDOW_THRESHOLD, scroller });

  // Home's project views select the project row; Recents and Drafts select their own row instead.
  const projectSelected = view === "overview" || view === "health" || view === "activity";
  const render = (row: SidebarRow) =>
    row.kind === "project" ? (
      <ProjectRowView
        key={row.key}
        row={row.row}
        selected={projectSelected && row.row.id === selected}
        onContextMenu={(event) => {
          projectMenu.open(event, projectMenuEntries(row.row));
        }}
      />
    ) : (
      <TreeRow
        key={row.key}
        row={row}
        onContextMenu={(event, entries) => {
          menu.open(event, entries);
        }}
      />
    );

  return (
    <>
      <div ref={setScroller} className="home-projects">
        <div ref={attach} role="tree" aria-label="Projects" className="home-projects-tree" style={windowRows ? { height: windowHeight } : undefined}>
          {windowRows
            ? windowRows.map((item, slot) => {
                const row = rows[item.index];
                return row ? (
                  <div key={row.key} className="home-projects-slot" style={{ transform: `translateY(${offsets[slot] ?? 0}px)` }}>
                    {render(row)}
                  </div>
                ) : null;
              })
            : rows.map(render)}
        </div>
      </div>
      {menu.element}
      {projectMenu.element}
    </>
  );
}
