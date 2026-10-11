/**
 * Context menus on the canvas: a card's (Open source, Show callers, Draw link…, Add as entrance…, Copy
 * link, Fold or Unfold card, Remove from page) and a group rail's (Collapse, Describe, Copy link,
 * Ungroup). Each page choice is one board edit, so ⌘Z undoes it.
 */

import type { MouseEvent, ReactNode } from "react";
import { copyLink } from "../../agent";
import { useContextMenu, type MenuEntry } from "../../components";
import type { ProjectId } from "../../ipc/generated";
import { requestFocus } from "../../keyboard";
import { canBeHandler } from "../../settings/locators";
import { editGroup, editPage, findPage, getOpenBoard, removeCard, select, setCardsFolded, setLinking, ungroup, type PageTarget } from "../../state/boards";
import { setSelectedCollapsed } from "../../state/layout";
import { openDialog, openPeek, type DefinitionRef } from "../../state/overlays";
import { showCallers } from "../showCallers";

/** Expands the sidebar's Selected section on the page's selection and focuses one of its fields. */
function showInSelected(field: string): void {
  setSelectedCollapsed(false);
  requestFocus(field);
}

/** Where a card's source is: a block's definition, or an entrance's declaration. */
interface CardSource {
  readonly path: string;
  readonly line: number | null;
  /** A block's definition, which may become an entrance's handler. */
  readonly definition?: DefinitionRef;
}

interface CardMenu {
  readonly onContextMenu: (event: MouseEvent) => void;
  readonly openGroupMenu: (groupId: string, at: MouseEvent | Element) => void;
  readonly element: ReactNode;
}

/**
 * The context menus of one card. `project` is null for a card of a project that isn't connected;
 * `source` (once the card's model loaded) offers Open source and Add as entrance; `block` offers Show callers.
 */
export function useCardMenu(
  target: PageTarget,
  project: ProjectId | null,
  cardId: string,
  folded: boolean,
  block: boolean,
  source: CardSource | null,
): CardMenu {
  const menu = useContextMenu("Card actions", 220);
  const cardEntries = (): MenuEntry[] => [
    {
      id: "source",
      label: "Open source",
      icon: "external-link",
      disabled: source === null || project === null,
      onSelect: () => {
        if (source && project !== null) {
          openPeek({ project, path: source.path, ...(source.line !== null ? { line: source.line } : {}) });
        }
      },
    },
    ...(block
      ? [
          {
            id: "callers",
            label: "Show callers",
            onSelect: () => void showCallers(target, cardId),
          },
        ]
      : []),
    {
      id: "link",
      label: "Draw link…",
      icon: "link",
      onSelect: () => {
        setLinking(target, cardId);
      },
    },
    ...(project !== null && source?.definition && canBeHandler(source.definition.kind)
      ? [
          {
            id: "add-entrance",
            label: "Add as entrance…",
            icon: "entrance",
            onSelect: () => {
              if (source.definition) {
                openDialog({ kind: "entranceEditor", project, mode: "add", handler: source.definition });
              }
            },
          } satisfies MenuEntry,
        ]
      : []),
    { id: "copy-link", label: "Copy link", onSelect: () => void copyLink(target, cardId) },
    { kind: "separator", id: "s1" },
    {
      id: "fold",
      label: folded ? "Unfold card" : "Fold card",
      onSelect: () => {
        editPage(target, (page) => {
          setCardsFolded(page, [cardId], !folded);
        });
      },
    },
    { kind: "separator", id: "s2" },
    {
      id: "remove",
      label: "Remove from page",
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
        label: group.collapsed ? "Expand group" : "Collapse group",
        onSelect: () => {
          editGroup(target, groupId, { collapsed: !group.collapsed });
        },
      },
      {
        id: "describe",
        label: "Describe group…",
        onSelect: () => {
          select(target, { kind: "group", groupId });
          showInSelected(`selected.group:${groupId}`);
        },
      },
      { id: "copy-link", label: "Copy link", onSelect: () => void copyLink(target, groupId) },
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
