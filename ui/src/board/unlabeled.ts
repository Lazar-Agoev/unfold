/**
 * "N Flow steps unlabeled · Label them": the Flow nodes of a page that still show their code names,
 * and the stepper that brings each into view in turn, selected, with its label field open in Selected.
 * The page's outline lists them as of the last save; labels written since count as written at once.
 */

import { useMemo } from "react";
import type { Unlabeled } from "../ipc/generated";
import { requestFocus } from "../lib/focus-targets";
import { findPage, focusCard, labelTexts, pageKey, pushFocusMove, select, useOpenBoard, type PageTarget } from "../state/boards";
import { setSelectedCollapsed } from "../state/layout";

/** The focus target of the selected Flow node's label field. */
export const NODE_LABEL_FIELD = "selected.label";

const NO_UNLABELED: readonly Unlabeled[] = [];

/** The page's Flow nodes still showing code names, in outline order. */
export function useUnlabeled(target: PageTarget): readonly Unlabeled[] {
  const outline = useOpenBoard(target.board, (board) => board.outlines[target.page]);
  const page = useOpenBoard(target.board, (board) => findPage(board.document, target.page));
  return useMemo(() => {
    if (!outline || !page) {
      return NO_UNLABELED;
    }
    const cards = new Map(page.cards.map((card) => [card.id, card]));
    return outline.unlabeled.filter((item) => {
      const card = cards.get(item.card);
      return card !== undefined && labelTexts(page.labels)[item.key] === undefined;
    });
  }, [outline, page]);
}

/** Where each page's stepper is, so "Label them" moves on from the last node it showed. */
const steps = new Map<string, number>();

/** Shows the next unlabeled Flow node: its card, the node selected, and its label field focused. */
export function stepUnlabeled(target: PageTarget, unlabeled: readonly Unlabeled[]): void {
  if (unlabeled.length === 0) {
    return;
  }
  const key = pageKey(target);
  const index = (steps.get(key) ?? 0) % unlabeled.length;
  steps.set(key, index + 1);
  const item = unlabeled[index];
  if (!item) {
    return;
  }
  pushFocusMove(target);
  focusCard(target, item.card);
  select(target, { kind: "card", cardId: item.card, key: item.key });
  setSelectedCollapsed(false);
  requestFocus(NODE_LABEL_FIELD);
}
