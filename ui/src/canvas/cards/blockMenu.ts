/**
 * A block's menu in an expanded card: View code, Open in place, then
 * Name…, Describe…, Combine…, and Add proposed step after… (or at the start of a body). What each item
 * offers comes from the block's overlay actions in the drawn flow, never from a rule of our own: Open
 * in place when it opens, Combine… with the steps of the body it can be combined in (or by itself), a
 * proposed step after it or at the start of its body where it allows one. Any block can be named. Name…
 * and Describe… go to their fields in Selected; a name applies page-wide, and Describe… describes the
 * card. A combined step's menu opens or collapses it, names and describes it, and uncombines it.
 */

import type { MenuEntry } from "../../components";
import type { Card, DrawnFlow, Key } from "../../ipc/generated";
import { editCombined, findCombined, findPage, getOpenBoard, isOpen, select, uncombine, type PageTarget, type ProposalPlace } from "../../state/boards";
import { combinableSteps, findBlock, openPathOf } from "../../state/cards";
import type { FlowSelection } from "../../flow/FlowView";
import { showInSelected } from "./cardMenu";

/** The focus target of the selected block's name field in Selected. */
export const BLOCK_NAME_FIELD = "selected.name";
/** The focus target of the selected card's description field in Selected. */
export const CARD_DESCRIPTION_FIELD = "selected.description";

/** The focus target of a combined step's title field in Selected. */
export function combinedTitleField(combinedId: string): string {
  return `selected.combinedTitle:${combinedId}`;
}

/** The focus target of a combined step's description field in Selected. */
export function combinedDescriptionField(combinedId: string): string {
  return `selected.combined:${combinedId}`;
}

/** Combine…'s request: the block it was chosen on, and the steps it can be combined with (null: only by itself). */
export interface CombineRequest {
  readonly key: Key;
  readonly steps: readonly Key[] | null;
}

/** Add proposed step…'s request: where it goes, and the block's words for it. */
export interface ProposeRequest {
  readonly place: ProposalPlace;
  readonly label: string;
}

/** What a block's menu does besides editing the page: the code panel, opening in place, and the two popovers. */
export interface BlockMenuActions {
  readonly onViewCode: (selection: FlowSelection) => void;
  readonly onToggleOpen: (path: readonly Key[]) => void;
  readonly onCombine: (request: CombineRequest) => void;
  readonly onPropose: (request: ProposeRequest) => void;
}

/** The entries of a block's menu in card `card`, drawn as `flow`. */
function blockEntries(target: PageTarget, card: Card, flow: DrawnFlow, key: Key, actions: BlockMenuActions): MenuEntry[] {
  const block = findBlock(flow, key);
  const allows = block?.actions;
  const path = allows?.open === true ? openPathOf(flow, key) : undefined;
  const label = block ? (block.name ?? block.label) : key;
  let combine: CombineRequest | undefined;
  if (allows?.combine?.with === "alone") {
    combine = { key, steps: null };
  } else if (allows?.combine?.with === "steps") {
    const steps = combinableSteps(flow, allows.combine.body);
    combine = steps?.includes(key) === true ? { key, steps } : undefined;
  }
  return [
    { id: "code", label: "View code", onSelect: () => { actions.onViewCode({ kind: "block", key }); } },
    ...(path
      ? [{ id: "open", label: isOpen(card, path) ? "Close in place" : "Open in place", onSelect: () => { actions.onToggleOpen(path); } }]
      : []),
    { kind: "separator", id: "s1" },
    {
      id: "name",
      label: "Name…",
      onSelect: () => {
        select(target, { kind: "card", cardId: card.id, key });
        showInSelected(BLOCK_NAME_FIELD);
      },
    },
    {
      id: "describe",
      label: "Describe…",
      onSelect: () => {
        select(target, { kind: "card", cardId: card.id, key: null });
        showInSelected(CARD_DESCRIPTION_FIELD);
      },
    },
    ...(combine ? [{ id: "combine", label: "Combine…", onSelect: () => { actions.onCombine(combine); } }] : []),
    ...(allows?.after === true ? [{ id: "propose-after", label: "Add proposed step after…", onSelect: () => { actions.onPropose({ place: { after: key }, label }); } }] : []),
    ...(allows?.start === true ? [{ id: "propose-start", label: "Add proposed step at the start…", onSelect: () => { actions.onPropose({ place: { start: key }, label }); } }] : []),
  ];
}

/** The entries of a combined step's menu. */
function combinedEntries(target: PageTarget, combinedId: string, actions: BlockMenuActions): MenuEntry[] {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const combined = page ? findCombined(page, combinedId) : undefined;
  if (!combined) {
    return [];
  }
  const show = (field: string) => {
    select(target, { kind: "combined", combinedId });
    showInSelected(field);
  };
  return [
    { id: "code", label: "View code", onSelect: () => { actions.onViewCode({ kind: "combined", item: combinedId }); } },
    {
      id: "toggle",
      label: combined.collapsed ? "Open" : "Collapse",
      onSelect: () => {
        editCombined(target, combinedId, { collapsed: !combined.collapsed });
      },
    },
    { kind: "separator", id: "s1" },
    { id: "name", label: "Name…", onSelect: () => { show(combinedTitleField(combinedId)); } },
    { id: "describe", label: "Describe…", onSelect: () => { show(combinedDescriptionField(combinedId)); } },
    { id: "uncombine", label: "Uncombine", onSelect: () => { uncombine(target, combinedId); } },
  ];
}

/** The menu of what `selection` picks in card `card` (drawn as `flow`): a block's, or a combined step's. */
export function blockMenuEntries(target: PageTarget, card: Card, flow: DrawnFlow, selection: FlowSelection, actions: BlockMenuActions): MenuEntry[] {
  switch (selection.kind) {
    case "block":
      return blockEntries(target, card, flow, selection.key, actions);
    case "combined":
      return combinedEntries(target, selection.item, actions);
    case "proposed":
      return [];
  }
}
