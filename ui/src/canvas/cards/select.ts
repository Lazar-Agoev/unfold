/** What a click on a card does: select it (or a node inside it), or, in link mode, finish the link (V-58). */

import type { Key } from "../../ipc/generated";
import { requestFocus } from "../../keyboard";
import { addLink, getPageView, select, setLinking, type PageTarget } from "../../state/boards";
import { setSelectedCollapsed } from "../../state/layout";

/** The focus target of the selected link's label field in the sidebar. */
export const LINK_LABEL_FIELD = "selected.linkLabel";

/**
 * Selects a card (`key`: a Flow node or statement in it). While a link is being drawn, a click on
 * another card draws it and selects it, with its label field focused; a click on the first card waits.
 */
export function clickCard(target: PageTarget, cardId: string, key: Key | null): void {
  const linking = getPageView(target).linking;
  if (!linking) {
    select(target, { kind: "card", cardId, key });
    return;
  }
  if (linking.from === cardId) {
    return;
  }
  setLinking(target, null);
  const linkId = addLink(target, linking.from, cardId);
  if (linkId !== undefined) {
    select(target, { kind: "link", linkId });
    setSelectedCollapsed(false);
    requestFocus(LINK_LABEL_FIELD);
  }
}
