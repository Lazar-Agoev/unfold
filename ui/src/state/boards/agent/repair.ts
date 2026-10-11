/**
 * An agent's repair (design §6.8, `repair`): one repair a health item offers, applied the way Review
 * applies one (`repairBoard`): the board's unsaved edits are saved first, the engine repairs and saves,
 * and the result becomes one undo step. The main process chose the repair and checked that the agent
 * may sign off on it (D6); the items it changed are marked as the agent's work.
 */

import type { AgentFailure, AgentRepairRequest, AgentRepairResult } from "@unfold/desktop/agent-api";
import type { RepairSpec } from "../../../ipc/generated";
import { toUnfoldError } from "../../../ipc/unfold";
import { findPage } from "../document";
import { repairBoard } from "../files";
import { getOpenBoard } from "../store";
import { markWorking } from "./working";

function failure(code: string, message: string): AgentFailure {
  return { ok: false, code, message };
}

/** Applies a repair to a board this window holds, and answers with the page as saved. */
export async function repairForAgent(request: AgentRepairRequest): Promise<AgentRepairResult> {
  const ref = { project: request.board.project, id: request.board.id };
  if (!getOpenBoard(ref)) {
    return failure("board_not_open", "This window doesn't hold the board.");
  }
  try {
    await repairBoard(ref, request.repair as unknown as RepairSpec);
  } catch (error) {
    const { code, message } = toUnfoldError(error);
    return failure(code, message);
  }
  const board = getOpenBoard(ref);
  const page = board ? findPage(board.document, request.page) : undefined;
  if (!board || !page) {
    return failure("page_not_found", `The board has no page with id “${request.page}” any more.`);
  }
  if (request.items.length > 0) {
    markWorking({ board: ref, page: request.page }, request.agent, request.items);
  }
  return { ok: true, page, board: { title: board.document.title, place: board.place, changes: board.published?.changes ?? 0, revision: board.revision } };
}
