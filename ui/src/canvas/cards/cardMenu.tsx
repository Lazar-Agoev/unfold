/**
 * Context menus on the canvas: a card's (Describe, Fold, Add as Entrance for a definition, Remove from
 * Page) and a group rail's (Collapse, Describe, Ungroup). Each page choice is one board edit, so ⌘Z
 * undoes it.
 */

import type { MouseEvent, ReactNode } from "react";
import { useContextMenu, type MenuEntry } from "../../components";
import type { ProjectId } from "../../ipc/generated";
import { requestFocus as focusTarget } from "../../keyboard";
import { canBeHandler } from "../../settings/locators";
import { editGroup, editPage, findPage, getOpenBoard, removeCard, select, setCardsFolded, ungroup, type PageTarget } from "../../state/boards";
import { setSelectedCollapsed } from "../../state/layout";
import { openDialog, type DefinitionRef } from "../../state/overlays";

/** Expands the sidebar's Selected section on the page's selection and focuses one of its fields. */
function showInSelected(field: string): void {
  setSelectedCollapsed(false);
  focusTarget(field);
}

export interface CardMenu {
  readonly onContextMenu: (event: MouseEvent) => void;
  readonly openGroupMenu: (groupId: string, at: MouseEvent | Element) => void;
  readonly element: ReactNode;
}

/** The context menus of one card. `definition` (the card's header, once loaded) offers Add as Entrance. */
export function useCardMenu(
  target: PageTarget,
  project: ProjectId,
  cardId: string,
  folded: boolean,
  definition?: DefinitionRef | null,
): CardMenu {
  const menu = useContextMenu("Card actions");
  const cardEntries = (): MenuEntry[] => [
    {
      id: "describe",
      label: "Describe…",
      onSelect: () => {
        select(target, { kind: "card", cardId, key: null });
        showInSelected("selected.description");
      },
    },
    {
      id: "fold",
      label: folded ? "Unfold Card" : "Fold Card",
      onSelect: () => {
        editPage(target, (page) => {
          setCardsFolded(page, [cardId], !folded);
        });
      },
    },
    ...(definition && canBeHandler(definition.kind)
      ? [
          {
            id: "add-entrance",
            label: "Add as Entrance…",
            onSelect: () => {
              openDialog({ kind: "entranceEditor", project, mode: "add", handler: definition });
            },
          },
        ]
      : []),
    { kind: "separator", id: "s1" },
    {
      id: "remove",
      label: "Remove from Page",
      danger: true,
      onSelect: () => {
        removeCard(target, cardId);
      },
    },
  ];
  const groupEntries = (groupId: string): MenuEntry[] => {
    const board = getOpenBoard(target.board);
    const group = board ? findPage(board.document, target.page)?.groups.find((item) => item.id === groupId) : undefined;
    if (!group) {
      return [];
    }
    return [
      {
        id: "collapse",
        label: group.collapsed ? "Expand Group" : "Collapse Group",
        onSelect: () => {
          editGroup(target, groupId, { collapsed: !group.collapsed });
        },
      },
      {
        id: "describe",
        label: "Describe Group…",
        onSelect: () => {
          select(target, { kind: "group", groupId });
          showInSelected(`selected.group:${groupId}`);
        },
      },
      { kind: "separator", id: "s1" },
      {
        id: "ungroup",
        label: "Ungroup",
        danger: true,
        onSelect: () => {
          ungroup(target, groupId);
        },
      },
    ];
  };
  return {
    onContextMenu: (event) => {
      event.stopPropagation();
      select(target, { kind: "card", cardId, key: null });
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
