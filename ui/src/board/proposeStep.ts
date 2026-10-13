/**
 * Add proposed step after…: where a person can propose a
 * step from a Flow step's menu, and the edit that does it. Only a real step of a function card's own
 * Flow offers it (not one shown in a call expanded in place, not a run, not inside a collapsed combined
 * block), by the same placement rule the agent's `propose` follows (`placesAfter`): none after a step
 * nothing runs after, and a step with several paths offers each of its actual paths. The proposal is
 * created in place with a placeholder title, as one undo step, selected, and its title focused in
 * Selected.
 */

import { isWithin } from "../code/outline";
import type { Key, Page, ProposalPath } from "../ipc/generated";
import { addProposal, cardKey, editPage, findCard, findPage, getOpenBoard, getPageView, placesAfter, select, type PageTarget, type Selection } from "../state/boards";
import { loadedContext, type SourceContext } from "./viewCode";

/** The title a new proposed step starts with, for the person to replace. */
const PROPOSAL_PLACEHOLDER = "Proposed step";

/** The focus target of the selected proposed step's title field in Selected. */
export const PROPOSAL_TITLE_FIELD = "selected.proposalTitle";

/** How a path of a decision reads in the menu ("on yes"). */
export const PATH_WORDS: Readonly<Record<ProposalPath, string>> = { yes: "on yes", no: "on no", each: "each", done: "done" };

/** Where a proposed step can go after the selected step: its card, the step, and the step's paths (empty: one way on). */
export interface ProposalChoice {
  readonly cardId: string;
  readonly after: Key;
  readonly paths: readonly ProposalPath[];
}

/** Where a proposed step can go after `selection` on `page`, from the card models loaded now; null when nowhere. */
export function proposalChoice(page: Page, selection: Selection, context: SourceContext): ProposalChoice | null {
  if (selection.kind !== "card" || selection.key === null || selection.owner !== undefined || selection.last !== undefined) {
    return null;
  }
  const card = findCard(page, selection.cardId);
  if (!card || !("block" in card.ref) || selection.key === cardKey(card)) {
    return null;
  }
  const project = context.project(card.ref.project);
  const model = project !== null ? context.model(project, cardKey(card)) : undefined;
  const flow = model?.type === "block" && model.flow?.source === "current" ? model.flow : null;
  const key = selection.key;
  const hidden = page.groups.some((group) => group.card === card.id && group.collapsed && group.members.some((member) => isWithin(key, member)));
  if (!flow || hidden) {
    return null;
  }
  const places = placesAfter(flow, key);
  return "problem" in places ? null : { cardId: card.id, after: key, paths: places.paths };
}

/** Where a proposed step can go after `selection` (default: the page's), from what is loaded now. */
export function proposalChoiceOf(target: PageTarget, selection: Selection = getPageView(target).selection): ProposalChoice | null {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  return page ? proposalChoice(page, selection, loadedContext(target.board.project)) : null;
}

/** Proposes a step after the chosen step (on `path` of a decision) as `author`, one undo step, and selects it. Returns its id. */
export function proposeStep(target: PageTarget, choice: ProposalChoice, path: ProposalPath | undefined, author: string): string | undefined {
  const id = editPage(target, (page) =>
    addProposal(page, { card: choice.cardId, after: choice.after, ...(path !== undefined ? { path } : {}), title: PROPOSAL_PLACEHOLDER, author }),
  );
  if (id !== undefined) {
    select(target, { kind: "proposal", proposalId: id });
  }
  return id;
}
