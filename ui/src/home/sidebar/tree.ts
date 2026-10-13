/**
 * The Home sidebar's projects list as flat rows: each project row, and under an expanded project its
 * folders (one level deep) and boards, folders first, then boards, each by name.
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
      readonly name: string;
      readonly expanded: boolean;
      /** No board in it (Delete applies). */
      readonly empty: boolean;
    }
  /** A board at the project's top level (depth 1) or in a folder (depth 2). */
  | { readonly kind: "board"; readonly key: string; readonly board: BoardSummary; readonly depth: 1 | 2 }
  /** The tree's 2px top and 4px bottom padding. */
  | { readonly kind: "gap"; readonly key: string; readonly height: number }
  | { readonly kind: "note"; readonly key: string; readonly text: string };

const ROW_HEIGHT = 28;

export function rowHeight(row: SidebarRow): number {
  return row.kind === "gap" ? row.height : ROW_HEIGHT;
}

/**
 * The rows under one expanded project: its folders (one level deep) by name, each followed by its
 * boards when expanded, then the boards at its top level, each by title.
 */
export function treeRows(project: ProjectId, list: BoardList, expandedFolders: ReadonlySet<string>): SidebarRow[] {
  const boardsIn = new Map<string | null, BoardSummary[]>();
  for (const board of list.boards) {
    boardsIn.set(board.folder, [...(boardsIn.get(board.folder) ?? []), board]);
  }
  const boardRows = (folder: string | null, depth: 1 | 2): SidebarRow[] =>
    [...(boardsIn.get(folder) ?? [])]
      .sort((a, b) => byName(boardTitle(a), boardTitle(b)))
      .map((board) => ({ kind: "board", key: `b:${project}\u0000${board.id}`, board, depth }));
  const folders = [...new Set([...list.emptyFolders, ...list.boards.flatMap((board) => (board.folder !== null ? [board.folder] : []))])].sort(byName);
  return [
    ...folders.flatMap((name): SidebarRow[] => {
      const expanded = expandedFolders.has(folderKey(project, name));
      const empty = !boardsIn.has(name);
      return [{ kind: "folder", key: `f:${project}\u0000${name}`, project, name, expanded, empty }, ...(expanded ? boardRows(name, 2) : [])];
    }),
    ...boardRows(null, 1),
  ];
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
