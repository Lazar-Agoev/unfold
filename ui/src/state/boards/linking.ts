/**
 * Drawing a link by hand (docs/ui-shell.md#explaining-by-hand): Draw link… on a card, a
 * resource card, or a frame starts it, and on a call in a card it starts at that call, which becomes
 * the link's step; the next card, resource card, or frame clicked ends it, and the page asks for the
 * label an `other` connection says what it is with. Escape, or a click on the canvas, lets go. While a
 * link is being drawn, a click selects nothing. View state only: nothing is saved until the label is.
 */

import type { Key } from "../../ipc/generated";
import { getPageView, select, updatePageView } from "./store";
import type { PageTarget, Selection } from "./types";

/** Starts drawing a link from item `from`, leaving from call `step` of it when given. */
export function startLink(target: PageTarget, from: string, step?: Key): void {
  updatePageView(target, (view) => {
    view.linking = { from, ...(step !== undefined ? { step } : {}), to: null };
  });
}

/** Stops drawing a link, saved or not. */
export function cancelLink(target: PageTarget): void {
  updatePageView(target, (view) => {
    view.linking = null;
  });
}

/**
 * A click on item `id` (a card, resource card, or frame): it ends a link being drawn there (a click
 * where the link starts waits), else selects `selection`.
 */
export function clickItem(target: PageTarget, id: string, selection: Selection): void {
  const linking = getPageView(target).linking;
  if (!linking) {
    select(target, selection);
    return;
  }
  if (linking.from !== id && linking.to === null) {
    updatePageView(target, (view) => {
      view.linking = { ...linking, to: id };
    });
  }
}
