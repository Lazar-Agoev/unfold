/**
 * A card's flow wired to its page: what `FlowView` selects becomes the page's selection (or ends a link
 * being drawn at the card), opening a call in place or a combined step edits the board (one undo step
 * each), and a block's or a thin row's `</>` or a gap tick opens its code in the code panel.
 */

import { viewCode, viewLines, viewRow } from "../code/open";
import type { CardView, Key, RepoPath, ThinRow } from "../ipc/generated";
import { clickItem, editCombined, editPage, toggleOpen, type PageTarget, type Selection } from "../state/boards";
import type { FlowSelection, FlowViewProps } from "./FlowView";

/** The page's selection for what `FlowView` selects in card `cardId`. */
function pageSelection(cardId: string, selection: FlowSelection): Selection {
  switch (selection.kind) {
    case "block":
      return { kind: "card", cardId, key: selection.key };
    case "combined":
      return { kind: "combined", combinedId: selection.item };
    case "proposed":
      return { kind: "proposal", proposalId: selection.item };
  }
}

/**
 * `FlowView`'s callbacks for card `cardId` (drawn as `view`) on a page: selection, opening calls in
 * place, combined steps' toggles, `</>` on a block or a thin row, and gap ticks.
 */
export function flowActions(
  target: PageTarget,
  cardId: string,
  view: CardView,
): Required<Pick<FlowViewProps, "onSelect" | "onToggleOpen" | "onToggleCombined" | "onViewCode" | "onViewRow" | "onOpenLines">> {
  return {
    onSelect: (selection: FlowSelection) => {
      clickItem(target, cardId, pageSelection(cardId, selection));
    },
    onToggleOpen: (path: readonly Key[]) => {
      editPage(target, (page) => {
        toggleOpen(page, cardId, path);
      });
    },
    onToggleCombined: (item: string, collapsed: boolean) => {
      editCombined(target, item, { collapsed });
    },
    onViewCode: (selection: FlowSelection) => {
      viewCode(target, pageSelection(cardId, selection), new Map([[cardId, view]]));
    },
    onViewRow: (path: RepoPath, row: ThinRow) => {
      viewRow(target, cardId, path, row);
    },
    onOpenLines: (path: RepoPath, lines: readonly [number, number]) => {
      viewLines(target, path, lines);
    },
  };
}

/** What a page's selection shows selected in card `cardId`'s flow. */
export function flowSelection(selection: Selection, cardId: string): FlowSelection | null {
  switch (selection.kind) {
    case "card":
      return selection.cardId === cardId && selection.key !== null ? { kind: "block", key: selection.key } : null;
    case "combined":
      return { kind: "combined", item: selection.combinedId };
    case "proposal":
      return { kind: "proposed", item: selection.proposalId };
    default:
      return null;
  }
}
