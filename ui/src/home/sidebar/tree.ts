/**
 * The Home sidebar's projects list as flat rows: each project row, and under an expanded project its
 * folders and boards, folders first, then boards, each by name.
 */

import type { BoardList, BoardSummary, ProjectId, ProjectRow } from "../../ipc";
import { folderKey } from "../../state/layout";
import { boardTitle, byName } from "../boards/model";

export type SidebarRow =
  | { readonly kind: "project"; readonly key: string; readonly row: ProjectRow }
  | {
      readonly kind: "folder";
      readonly key: string;
      readonly project: ProjectId;
      readonly path: string;
      readonly name: string;
      readonly depth: number;
      readonly expanded: boolean;
      /** No board at or under it (Delete applies). */
      readonly empty: boolean;
    }
  | { readonly kind: "board"; readonly key: string; readonly board: BoardSummary; readonly depth: number }
  /** The tree's 2px top and 4px bottom padding. */
  | { readonly kind: "gap"; readonly key: string; readonly height: number }
  | { readonly kind: "note"; readonly key: string; readonly text: string };

const ROW_HEIGHT = 28;

export function rowHeight(row: SidebarRow): number {
  return row.kind === "gap" ? row.height : ROW_HEIGHT;
}

function parentOf(path: string): string | null {
  const slash = path.lastIndexOf("/");
  return slash < 0 ? null : path.slice(0, slash);
}

/** Every folder of the list, with its ancestors. */
function folderPaths(list: BoardList): Set<string> {
  const paths = new Set<string>();
  const add = (path: string | null) => {
    for (let current = path; current !== null && current !== "" && !paths.has(current); current = parentOf(current)) {
      paths.add(current);
    }
  };
  list.emptyFolders.forEach(add);
  list.boards.forEach((board) => {
    add(board.folder);
  });
  return paths;
}

/** The rows under one expanded project, from depth 1. */
export function treeRows(project: ProjectId, list: BoardList, expandedFolders: ReadonlySet<string>): SidebarRow[] {
  const folders = folderPaths(list);
  const childFolders = new Map<string | null, string[]>();
  for (const path of folders) {
    const parent = parentOf(path);
    childFolders.set(parent, [...(childFolders.get(parent) ?? []), path]);
  }
  const boardsIn = new Map<string | null, BoardSummary[]>();
  for (const board of list.boards) {
    boardsIn.set(board.folder, [...(boardsIn.get(board.folder) ?? []), board]);
  }
  const hasBoards = (path: string): boolean => (boardsIn.get(path)?.length ?? 0) > 0 || (childFolders.get(path) ?? []).some(hasBoards);
  const nameOf = (path: string) => path.slice(path.lastIndexOf("/") + 1);

  const rows: SidebarRow[] = [];
  const walk = (parent: string | null, depth: number) => {
    for (const path of [...(childFolders.get(parent) ?? [])].sort((a, b) => byName(nameOf(a), nameOf(b)))) {
      const expanded = expandedFolders.has(folderKey(project, path));
      rows.push({ kind: "folder", key: `f:${project}\u0000${path}`, project, path, name: nameOf(path), depth, expanded, empty: !hasBoards(path) });
      if (expanded) {
        walk(path, depth + 1);
      }
    }
    for (const board of [...(boardsIn.get(parent) ?? [])].sort((a, b) => byName(boardTitle(a), boardTitle(b)))) {
      rows.push({ kind: "board", key: `b:${project}\u0000${board.id}`, board, depth });
    }
  };
  walk(null, 1);
  return rows;
}

export interface ProjectTree {
  readonly list: BoardList | undefined;
  readonly error: string | null;
}

/** Every row of the projects list: each project, then its tree when it is expanded. */
export function sidebarRows(
  projects: readonly ProjectRow[],
  trees: ReadonlyMap<ProjectId, ProjectTree>,
  expandedFolders: ReadonlySet<string>,
): SidebarRow[] {
  const rows: SidebarRow[] = [];
  for (const row of projects) {
    rows.push({ kind: "project", key: `p:${row.id}`, row });
    if (!row.expanded) {
      continue;
    }
    const tree = trees.get(row.id);
    let children: SidebarRow[];
    if (tree?.error) {
      children = [{ kind: "note", key: `n:${row.id}`, text: "Couldn't list its boards" }];
    } else if (!tree?.list) {
      children = [];
    } else {
      children = treeRows(row.id, tree.list, expandedFolders);
      if (children.length === 0) {
        children = [{ kind: "note", key: `n:${row.id}`, text: "No boards yet" }];
      }
    }
    if (children.length > 0) {
      rows.push({ kind: "gap", key: `t:${row.id}`, height: 2 }, ...children, { kind: "gap", key: `e:${row.id}`, height: 4 });
    }
  }
  return rows;
}
