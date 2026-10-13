/**
 * Builders for workspace and board data in tests: project rows, a workspace, pages, documents, and
 * board states, with everything a test doesn't name filled in.
 */

import type { BoardDocument, BoardState, Card, Health, Page, ProjectId, ProjectRow, WorkspaceView } from "../ipc/generated";

/** A connected, current project row. */
export function projectRow(id: ProjectId, name: string, overrides: Partial<ProjectRow> = {}): ProjectRow {
  return { id, name, path: `/work/${name}`, expanded: false, live: true, state: { kind: "current" }, ...overrides };
}

/** A workspace of these rows, in this order, with the first selected. */
export function workspaceView(rows: readonly ProjectRow[], overrides: Partial<WorkspaceView> = {}): WorkspaceView {
  return { projects: [...rows], selected: rows[0]?.id ?? null, ...overrides };
}

/** A page with these cards (and whatever else is given). */
export function page(id: string, cards: readonly Card[] = [], overrides: Partial<Page> = {}): Page {
  return { id, title: `Page ${id}`, defaultView: "code", cards: [...cards], groups: [], labels: [], guards: [], links: [], ...overrides };
}

/** A board document with these pages (one empty `page-1` when none are given). */
export function boardDocument(pages: readonly Page[] = [page("page-1")], overrides: Partial<BoardDocument> = {}): BoardDocument {
  return { format: "unfold-board", version: 1, title: "Order placement", projects: [], pages: [...pages], ...overrides };
}

export const HEALTHY: Health = { items: [], missing: 0, outdated: 0, brokenGroups: 0, review: 0, notConnected: 0, brokenLinks: 0 };

/** A draft's state as `open_board` and `save_board` return it. */
export function boardState(project: ProjectId, id: string, document: BoardDocument = boardDocument(), overrides: Partial<BoardState> = {}): BoardState {
  return {
    project,
    id,
    place: "draft",
    name: id.replace(/^.*\//, "").replace(/\.json$/, ""),
    document,
    revision: "r1",
    published: null,
    health: HEALTHY,
    outlines: document.pages.map((item) => ({ page: item.id, rows: [], unlabeled: [] })),
    label: null,
    ...overrides,
  };
}
