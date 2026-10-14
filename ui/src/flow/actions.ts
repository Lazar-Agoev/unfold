/**
 * A card's flow wired to its page: what `FlowView` selects becomes the page's selection, and opening
 * a call in place or a combined step edits the board (one undo step each).
 */

import type { Key } from "../ipc/generated";
import { editCombined, editPage, select, toggleOpen, type PageTarget, type Selection } from "../state/boards";
import type { FlowSelection, FlowViewProps } from "./FlowView";

/** `FlowView`'s callbacks for card `cardId` on a page: selection, opening calls in place, and combined steps' toggles. */
export function flowActions(target: PageTarget, cardId: string): Pick<FlowViewProps, "onSelect" | "onToggleOpen" | "onToggleCombined"> {
  return {
    onSelect: (selection: FlowSelection) => {
      switch (selection.kind) {
        case "block":
          select(target, { kind: "card", cardId, key: selection.key });
          break;
        case "combined":
          select(target, { kind: "combined", combinedId: selection.item });
          break;
        case "proposed":
          select(target, { kind: "proposal", proposalId: selection.item });
          break;
      }
    },
    onToggleOpen: (path: readonly Key[]) => {
      editPage(target, (page) => {
        toggleOpen(page, cardId, path);
      });
    },
    onToggleCombined: (item: string, collapsed: boolean) => {
      editCombined(target, item, { collapsed });
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
