/** What an image captures: the whole page as drawn, or one item of it. */

import type { RenderAnswer } from "@unfold/desktop/render-api";
import { boundsOf, frameBox, frameTree, type PageGeometry } from "../board/checks";
import type { Page } from "../ipc/generated";

/**
 * The bounds of the page's drawn items, or of `item` (a card, frame, resource card, or note); a refusal
 * when the page is empty, the item isn't one of the page's, or a collapsed frame hides it.
 */
export function imageBounds(page: Page, geometry: PageGeometry, item: string | undefined): RenderAnswer {
  if (item === undefined) {
    const bounds = boundsOf([...geometry.boxes.values()]);
    return bounds ? { ok: true, bounds } : { ok: false, code: "empty_page", message: "The page has nothing on it yet." };
  }
  const tree = frameTree(page);
  const boxes = [page.cards, page.frames ?? [], page.resources ?? [], page.notes ?? []];
  if (!boxes.some((list) => list.some((entry) => entry.id === item))) {
    return { ok: false, code: "item_not_found", message: `The page has no card, frame, resource card, or note \`${item}\`.` };
  }
  const hiddenBy = tree.hiddenBy(item);
  if (hiddenBy !== undefined) {
    return {
      ok: false,
      code: "item_hidden",
      message: `\`${item}\` is inside the collapsed frame \`${hiddenBy}\`. Take the frame's image, or expand the frame first.`,
    };
  }
  const bounds = tree.frames.has(item) ? frameBox(tree, geometry.boxes, item) : geometry.boxes.get(item);
  return bounds ? { ok: true, bounds } : { ok: false, code: "item_not_found", message: `\`${item}\` isn't drawn on the page.` };
}
