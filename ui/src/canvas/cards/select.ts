/**
 * What a click on a card, a frame, a resource card, or a note does: select it (or a node inside a
 * card), or, in link mode, finish the link there (a note can't end one).
 */

import type { Key } from "../../ipc/generated";
import { requestFocus } from "../../keyboard";
import { addLink, getPageView, select, setLinking, type PageTarget, type Selection } from "../../state/boards";
import { setSelectedCollapsed } from "../../state/layout";

/** The focus target of the selected link's label field in the sidebar. */
export const LINK_LABEL_FIELD = "selected.linkLabel";

/**
 * Selects `selection` (of item `id`). While a link is being drawn, a click on another card or frame
 * draws it and selects it, with its label field focused; a click on where it starts waits.
 */
function clickItem(target: PageTarget, id: string, selection: Selection): void {
  const linking = getPageView(target).linking;
  if (!linking) {
    select(target, selection);
    return;
  }
  if (linking.from === id) {
    return;
  }
  setLinking(target, null);
  const linkId = addLink(target, linking.from, id);
  if (linkId !== undefined) {
    select(target, { kind: "link", linkId });
    setSelectedCollapsed(false);
    requestFocus(LINK_LABEL_FIELD);
  }
}

/** Selects a card (`key`: a Flow node or statement in it), or finishes the link being drawn there. */
export function clickCard(target: PageTarget, cardId: string, key: Key | null): void {
  clickItem(target, cardId, { kind: "card", cardId, key });
}

/** Selects a frame, or finishes the link being drawn there. */
export function clickFrame(target: PageTarget, frameId: string): void {
  clickItem(target, frameId, { kind: "frame", frameId });
}

/** Selects a resource card, or finishes the link being drawn there. */
export function clickResource(target: PageTarget, resourceId: string): void {
  clickItem(target, resourceId, { kind: "resource", resourceId });
}

/** Selects a note (a link being drawn waits: a note can't end one). */
export function clickNote(target: PageTarget, noteId: string): void {
  if (!getPageView(target).linking) {
    select(target, { kind: "note", noteId });
  }
}
