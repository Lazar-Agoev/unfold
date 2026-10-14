/**
 * A card's context menu: Show callers (a definition's card), Copy link, and Remove from page. Removing
 * is one board edit, so ⌘Z undoes it.
 */

import type { MouseEvent, ReactNode } from "react";
import { copyLink } from "../../agent";
import { useContextMenu, type MenuEntry } from "../../components";
import { removeCard, select, type PageTarget } from "../../state/boards";
import { setSelectedCollapsed } from "../../state/layout";
import { requestFocus } from "../../lib/focus-targets";
import { showCallers } from "../showCallers";

function stop(event: MouseEvent): void {
  event.stopPropagation();
}

/** Expands the sidebar's Selected section on the page's selection and focuses one of its fields. */
export function showInSelected(field: string): void {
  setSelectedCollapsed(false);
  requestFocus(field);
}

interface CardMenu {
  readonly onContextMenu: (event: MouseEvent) => void;
  readonly element: ReactNode;
}

/** The context menu of one card; `callers` offers Show callers (a definition's card). */
export function useCardMenu(target: PageTarget, cardId: string, callers: boolean): CardMenu {
  const menu = useContextMenu("Card actions", 220);
  const entries = (): MenuEntry[] => [
    ...(callers ? [{ id: "callers", label: "Show callers", onSelect: () => void showCallers(target, cardId) }] : []),
    { id: "copy-link", label: "Copy link", onSelect: () => void copyLink(target, cardId) },
    { kind: "separator", id: "s1" },
    {
      id: "remove",
      label: "Remove from page",
      danger: true,
      onSelect: () => {
        removeCard(target, cardId);
      },
    },
  ];
  return {
    onContextMenu: (event) => {
      event.stopPropagation();
      select(target, { kind: "card", cardId, key: null });
      menu.open(event, entries());
    },
    // The menu renders in a portal, but its events still bubble through the card: a choice never
    // also clicks (selects) or double-clicks the card it was opened on.
    element: menu.element ? (
      <span className="card-menu-layer" onClick={stop} onDoubleClick={stop} onContextMenu={stop}>
        {menu.element}
      </span>
    ) : null,
  };
}
