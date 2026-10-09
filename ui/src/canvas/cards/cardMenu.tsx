/**
 * Context menus on the canvas: a card's (Show in Inspector, Fold, Add as Entrance for a definition,
 * Remove from Board) and a group rail's (Collapse, Describe, Ungroup). Each board choice is one
 * board edit, so ⌘Z undoes it.
 */

import type { MouseEvent, ReactNode } from "react";
import { useContextMenu, type MenuEntry } from "../../components";
import { addAsEntranceItem, type DefinitionRef } from "../../settings";
import { editGroup, removeCardFromBoard, ungroup } from "../../state/boards/api";
import { setCardsFolded } from "../../state/boards/document";
import { editBoard, getBoard, selectBlock, selectCard } from "../../state/boards/store";
import { requestFocus as focusTarget } from "../../keyboard";
import { useLayout } from "../../state/layout";

/** Opens the inspector (if closed) on the board's selection, and focuses one of its fields. */
export function showInInspector(field = "inspector"): void {
  if (!useLayout.getState().inspectorOpen) {
    useLayout.getState().toggleInspector();
  }
  focusTarget(field);
}

export interface CardMenu {
  readonly onContextMenu: (event: MouseEvent) => void;
  readonly openGroupMenu: (groupId: string, at: MouseEvent | Element) => void;
  readonly element: ReactNode;
}

/**
 * The context menus of one card. `definition` (the card's header, once loaded) offers Add as
 * Entrance.
 */
export function useCardMenu(boardId: string, cardId: string, cardKey: string, folded: boolean, definition?: DefinitionRef | null): CardMenu {
  const menu = useContextMenu("Card actions");
  const cardEntries = (): MenuEntry[] => [
    {
      id: "inspect",
      label: "Show in Inspector",
      onSelect: () => {
        selectCard(boardId, cardId);
        showInInspector("inspector.description");
      },
    },
    {
      id: "fold",
      label: folded ? "Unfold Card" : "Fold Card",
      onSelect: () => {
        editBoard(boardId, (document) => {
          setCardsFolded(document, [cardId], !folded);
        });
      },
    },
    ...(definition ? addAsEntranceItem(definition) : []),
    { kind: "separator", id: "s1" },
    {
      id: "remove",
      label: "Remove from Board",
      danger: true,
      onSelect: () => {
        removeCardFromBoard(boardId, cardId);
      },
    },
  ];
  const groupEntries = (groupId: string): MenuEntry[] => {
    const group = getBoard(boardId)?.document.groups.find((item) => item.id === groupId);
    if (!group) {
      return [];
    }
    return [
      {
        id: "collapse",
        label: group.collapsed ? "Expand Group" : "Collapse Group",
        onSelect: () => {
          editGroup(boardId, groupId, { collapsed: !group.collapsed });
        },
      },
      {
        id: "describe",
        label: "Describe Group…",
        onSelect: () => {
          selectBlock(boardId, cardId, cardKey);
          showInInspector(`inspector.group:${groupId}`);
        },
      },
      { kind: "separator", id: "s1" },
      {
        id: "ungroup",
        label: "Ungroup",
        danger: true,
        onSelect: () => {
          ungroup(boardId, groupId);
        },
      },
    ];
  };
  return {
    onContextMenu: (event) => {
      event.stopPropagation();
      selectCard(boardId, cardId);
      menu.open(event, cardEntries());
    },
    openGroupMenu: (groupId, at) => {
      const entries = groupEntries(groupId);
      if (entries.length > 0) {
        menu.open(at, entries);
      }
    },
    element: menu.element,
  };
}
