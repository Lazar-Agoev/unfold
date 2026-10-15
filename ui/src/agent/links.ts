/**
 * Copy link: a board, a page, or an item on it (a card, frame, resource card, note, connection, or
 * proposed step) copies `unfold://<project>/<board id>?page=<page id>&item=<item id>`, which a person
 * pastes into an agent's chat to point at it. `<project>` is the project's folder name, or its id
 * when another project's folder has the same name, so a link always names one project. Only the
 * main process reads links; clicking one opens nothing.
 */

import type { AgentLink } from "@unfold/desktop/agent-api";
import { showToast } from "../components";
import { toUnfoldError, windowCommand, type ProjectId } from "../ipc";
import type { BoardRef, PageTarget } from "../state/boards";
import { getProjectIndex } from "../state/workspace";

/** The link text: each part URI-encoded, the board id segment by segment. */
export function boardLink(link: AgentLink): string {
  const query = [
    ...(link.page !== undefined ? [`page=${encodeURIComponent(link.page)}`] : []),
    ...(link.item !== undefined ? [`item=${encodeURIComponent(link.item)}`] : []),
  ];
  const board = link.board.split("/").map(encodeURIComponent).join("/");
  return `unfold://${encodeURIComponent(link.project)}/${board}${query.length > 0 ? `?${query.join("&")}` : ""}`;
}

/** How a link names project `id`: its folder's name, or its id when another project's folder has that name. */
function linkProject(id: ProjectId): string | undefined {
  const { rows, byId } = getProjectIndex();
  const name = byId.get(id)?.name;
  return name !== undefined && rows.some((row) => row.id !== id && row.name === name) ? id : name;
}

/** Puts a link to `board` (and a page or item on it) on the clipboard. */
async function copyBoardItemLink(board: BoardRef, place: { readonly page?: string; readonly item?: string }): Promise<void> {
  const project = linkProject(board.project);
  try {
    if (project === undefined) {
      throw new Error("The board's project isn't in the workspace.");
    }
    await windowCommand("copy_to_clipboard", { text: boardLink({ project, board: board.id, ...place }) });
    showToast({ title: "Copied the link", body: "Paste it into an agent's chat to point at this." });
  } catch (error) {
    showToast({ title: "Couldn't copy the link", body: toUnfoldError(error).message, tone: "danger" });
  }
}

/** Copies a link to a board (the board menu's Copy link). */
export async function copyBoardLink(board: BoardRef): Promise<void> {
  await copyBoardItemLink(board, {});
}

/** Copies a link to a page, or to an item on it. */
export async function copyLink(target: PageTarget, item?: string): Promise<void> {
  await copyBoardItemLink(target.board, { page: target.page, ...(item !== undefined ? { item } : {}) });
}
