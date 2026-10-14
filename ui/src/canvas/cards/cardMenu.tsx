/**
 * A card's context menu: Show callers and Add as entrance… (a module's or definition's card), Draw
 * link… (`startLink`), Copy link, and Remove from page. Removing is one board edit, so ⌘Z undoes it. Add as entrance… opens its
 * popover beside the card, with the card's key as the entrance's `code`.
 */

import { useState, type MouseEvent, type ReactNode } from "react";
import { copyLink } from "../../agent";
import { showToast, useContextMenu, type MenuEntry } from "../../components";
import { AddEntrance } from "../../entrances";
import type { Card } from "../../ipc/generated";
import { cardKey, removeCard, select, startLink, type PageTarget } from "../../state/boards";
import { setSelectedCollapsed } from "../../state/layout";
import { requestFocus } from "../../lib/focus-targets";
import { showCallers } from "../showCallers";

function stop(event: { stopPropagation: () => void }): void {
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

/** The context menu of one card; `definition` (a module's or definition's card) offers Show callers and Add as entrance…. */
export function useCardMenu(target: PageTarget, card: Card, definition: boolean): CardMenu {
  const menu = useContextMenu("Card actions", 220);
  // The card the Add as entrance popover sits beside, while it is open.
  const [adding, setAdding] = useState<HTMLElement | null>(null);
  const entries = (element: HTMLElement): MenuEntry[] => [
    ...(definition
      ? [
          { id: "callers", label: "Show callers", onSelect: () => void showCallers(target, card.id) },
          {
            id: "entrance",
            label: "Add as entrance…",
            onSelect: () => {
              setAdding(element);
            },
          },
        ]
      : []),
    { id: "draw-link", label: "Draw link…", onSelect: () => { startLink(target, card.id); } },
    { id: "copy-link", label: "Copy link", onSelect: () => void copyLink(target, card.id) },
    { kind: "separator", id: "s1" },
    {
      id: "remove",
      label: "Remove from page",
      danger: true,
      onSelect: () => {
        removeCard(target, card.id);
      },
    },
  ];
  const layers = (
    <>
      {menu.element}
      {adding ? (
        <AddEntrance
          project={target.board.project}
          anchor={adding}
          code={cardKey(card)}
          side="right"
          onClose={() => {
            setAdding(null);
          }}
          onAdded={(app, name) => {
            showToast({ title: `Added ${name} to ${app}` });
          }}
        />
      ) : null}
    </>
  );
  return {
    onContextMenu: (event) => {
      event.stopPropagation();
      select(target, { kind: "card", cardId: card.id, key: null });
      menu.open(event, entries(event.currentTarget as HTMLElement));
    },
    // The menu and the popover render in portals, but their events still bubble through the card: a
    // choice never also clicks (selects) or double-clicks the card it was opened on, and typing never
    // reaches the canvas.
    element:
      menu.element || adding ? (
        <span className="card-menu-layer" onClick={stop} onDoubleClick={stop} onContextMenu={stop} onKeyDown={stop}>
          {layers}
        </span>
      ) : null,
  };
}
