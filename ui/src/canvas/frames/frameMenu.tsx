/**
 * A frame's context menu: Rename…, Describe…, Collapse or Expand, Draw
 * link… (`startLink`), Copy link, and Remove frame (keep cards). Agents create frames; people adjust them. Each choice that
 * changes the page is one board edit, so ⌘Z undoes it.
 */

import type { MouseEvent, ReactNode } from "react";
import { copyLink } from "../../agent";
import { useContextMenu, type MenuEntry } from "../../components";
import { editFrame, findFrame, findPage, getOpenBoard, removeFrame, select, startLink, type PageTarget } from "../../state/boards";
import { showInSelected } from "../cards/cardMenu";

/** The focus target of a frame's description field in Selected. */
export function frameDescriptionField(frameId: string): string {
  return `selected.frame:${frameId}`;
}

interface FrameMenu {
  readonly open: (at: MouseEvent | Element) => void;
  readonly element: ReactNode;
}

/** The context menu of one frame; `onRename` starts renaming its title in place. */
export function useFrameMenu(target: PageTarget, frameId: string, onRename: () => void): FrameMenu {
  const menu = useContextMenu("Frame actions", 220);
  const entries = (): MenuEntry[] => {
    const board = getOpenBoard(target.board);
    const page = board ? findPage(board.document, target.page) : undefined;
    const frame = page ? findFrame(page, frameId) : undefined;
    if (!frame) {
      return [];
    }
    const collapsed = frame.collapsed === true;
    return [
      { id: "rename", label: "Rename…", onSelect: onRename },
      {
        id: "describe",
        label: "Describe…",
        onSelect: () => {
          select(target, { kind: "frame", frameId });
          showInSelected(frameDescriptionField(frameId));
        },
      },
      {
        id: "collapse",
        label: collapsed ? "Expand" : "Collapse",
        onSelect: () => {
          editFrame(target, frameId, { collapsed: !collapsed });
        },
      },
      { id: "draw-link", label: "Draw link…", onSelect: () => { startLink(target, frameId); } },
      { id: "copy-link", label: "Copy link", onSelect: () => void copyLink(target, frameId) },
      { kind: "separator", id: "s1" },
      {
        id: "remove",
        label: "Remove frame (keep cards)",
        onSelect: () => {
          removeFrame(target, frameId);
        },
      },
    ];
  };
  return {
    open: (at) => {
      const list = entries();
      if (list.length > 0) {
        select(target, { kind: "frame", frameId });
        menu.open(at, list);
      }
    },
    element: menu.element,
  };
}
