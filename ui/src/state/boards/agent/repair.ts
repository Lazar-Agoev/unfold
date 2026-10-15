/**
 * An agent's repair (`repair`): one repair a health item offers, applied the way Review
 * applies one (`repairBoard`, `engineStep`): the board's unsaved edits are saved first (refused while
 * they can't be), the engine repairs and saves, and the result becomes one undo step; edits the person
 * makes meanwhile are kept, and the repair is refused (`page_changed`). The main
 * process chose the repair and checked that the agent may sign off on it. A removal takes a person's
 * words off the page only when the person asked (`replacePeopleText`), as `remove_items` does: what
 * goes with the item counts too. The items it changed are marked as the agent's work.
 */

import type { AgentRepairRequest, AgentRepairResult } from "@unfold/desktop/agent-api";
import type { Description, Page, RepairSpec, Target } from "../../../ipc/generated";
import { toUnfoldError } from "../../../ipc/unfold";
import { cardKey, findCard, findCombined, findPage, removalCascade } from "../document";
import { repairBoard } from "../files";
import { findFrame } from "../frames";
import { findResource } from "../resources";
import { getOpenBoard } from "../store";
import { AgentRefusal, boardInfo, failure, guardPeopleText, itemWords, lineWords, resourceName, type Words } from "./context";
import { markWorking } from "./working";

/** What a description repair is about, in words, and its description; a name and a proposed step have none. */
function described(page: Page, target: Target): { readonly what: string; readonly description: Description | undefined } | undefined {
  if ("card" in target) {
    const card = findCard(page, target.card);
    return card && { what: cardKey(card), description: card.description };
  }
  if ("combined" in target) {
    const combined = findCombined(page, target.combined);
    return combined && { what: `the combined step “${combined.title}”`, description: combined.description };
  }
  if ("connection" in target) {
    const link = page.links.find((item) => item.id === target.connection);
    return link && { what: lineWords(link)[0], description: link.description };
  }
  if ("resource" in target) {
    const resource = findResource(page, target.resource);
    return resource && { what: resourceName(resource), description: resource.description };
  }
  if ("frame" in target) {
    const frame = findFrame(page, target.frame);
    return frame && { what: `the frame “${frame.title}”`, description: frame.description };
  }
  return undefined;
}

/**
 * The words a repair takes off the page, each with who wrote it: what removing an item takes with it
 * (as `remove_items` finds it), a name, or a description. The other repairs keep every text.
 */
function removedWords(page: Page, repair: RepairSpec): Words[] {
  switch (repair.kind) {
    case "remove": {
      const { target } = repair;
      if ("name" in target) {
        return page.names.filter((name) => name.key === target.name).map((name): Words => [`the name “${name.text}”`, name.author]);
      }
      // Every other target names one item of the page by its id.
      const [id = ""] = Object.values(target);
      return [...removalCascade(page, [id])].flatMap((item) => itemWords(page, item, cardKey));
    }
    case "remove_description": {
      const item = described(page, repair.target);
      return item?.description ? [[`the description of ${item.what}`, item.description.author]] : [];
    }
    default:
      return [];
  }
}

/** Applies a repair to a board this window holds, and answers with the page as saved. */
export async function repairForAgent(request: AgentRepairRequest): Promise<AgentRepairResult> {
  const ref = { project: request.board.project, id: request.board.id };
  const held = getOpenBoard(ref);
  if (!held) {
    return failure("board_not_open", "This window doesn't hold the board.");
  }
  const repair = request.repair as unknown as RepairSpec;
  const before = findPage(held.document, request.page);
  try {
    for (const [what, author] of before ? removedWords(before, repair) : []) {
      guardPeopleText(what, author, request.replacePeopleText);
    }
    await repairBoard(ref, repair);
  } catch (error) {
    const { code, message } = error instanceof AgentRefusal ? error : toUnfoldError(error);
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
