/** String keys for boards, pages, and project-qualified cards (for maps and on-page marks). */

import type { Key, ProjectId } from "../../ipc/generated";
import type { BoardRef, PageTarget } from "./types";

/** A board's key: its project and id, which together are unique in the workspace. */
export function boardKey(ref: BoardRef): string {
  return `${ref.project}\u0000${ref.id}`;
}

/** Whether two refs name the same board (false when either is missing). */
export function sameBoard(a: BoardRef | null | undefined, b: BoardRef | null | undefined): boolean {
  return a !== null && a !== undefined && a.project === b?.project && a.id === b.id;
}

/** A page's key. */
export function pageKey(target: PageTarget): string {
  return `${boardKey(target.board)}\u0000${target.page}`;
}

/** A definition or entrance in a project: what the on-page marks compare (`${project}\0${key}`). */
export function cardRefKey(project: ProjectId, key: Key): string {
  return `${project}\u0000${key}`;
}
