/**
 * An agent's repair (`repair`): one repair a health item offers, applied the way Review
 * applies one (`repairBoard`, `engineStep`): the board's unsaved edits are saved first (refused while
 * they can't be), the engine repairs and saves, and the result becomes one undo step; edits the person
 * makes meanwhile are kept, and the repair is refused (`page_changed`). The main
 * process chose the repair and checked that the agent may sign off on it; the items it changed
 * are marked as the agent's work.
 */

import type { AgentRepairRequest, AgentRepairResult } from "@unfold/desktop/agent-api";
import type { RepairSpec } from "../../../ipc/generated";
import { toUnfoldError } from "../../../ipc/unfold";
import { findPage } from "../document";
import { repairBoard } from "../files";
import { getOpenBoard } from "../store";
import { boardInfo, failure } from "./context";
import { markWorking } from "./working";

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
  return { ok: true, page, board: boardInfo(board) };
}
