/**
 * The board menu: the `⋯` in the board header, and the right-click menu of a board on Home and in the
 * sidebar. Rename…, Move to folder ›, Export PNG…, Copy link, Delete…. Export PNG needs a page, so it
 * is disabled for a board that isn't open.
 */

import type { JSX } from "react";
import { copyBoardLink } from "../agent";
import { MenuButton, type MenuEntry } from "../components";
import { engineKey, queryClient, useEngineQuery, type BoardList, type BoardSummary } from "../ipc";
import { openMoveToFolder } from "../dialogs";
import { getOpenBoard, useOpenBoard, type BoardRef, type PageId } from "../state/boards";
import { openDialog } from "../state/overlays";
import { exportPng, moveToFolder, renameBoardInPlace } from "./actions";

/** What the menu needs to know about a board. */
interface BoardFacts {
  /** From `list_boards`, when loaded. */
  readonly summary: BoardSummary | undefined;
  /** Every board folder of the project, sorted. */
  readonly folders: readonly string[];
  /** The page the board's open session in this window shows; null while it isn't open. */
  readonly page: PageId | null;
}

const byName = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** Every folder of a project: its boards' folders and its empty folders, sorted. */
export function projectFolders(list: BoardList | undefined): string[] {
  const folders = new Set<string>(list?.emptyFolders);
  for (const board of list?.boards ?? []) {
    if (board.folder !== null) {
      folders.add(board.folder);
    }
  }
  return [...folders].sort(byName.compare);
}

function factsOf(ref: BoardRef, list: BoardList | undefined): BoardFacts {
  return {
    summary: list?.boards.find((item) => item.id === ref.id),
    folders: projectFolders(list),
    page: getOpenBoard(ref)?.activePage ?? null,
  };
}

function folderEntries(ref: BoardRef, current: string | null, folders: readonly string[]): MenuEntry[] {
  const move = (folder: string | null) => () => {
    if (folder !== current) {
      void moveToFolder(ref, folder);
    }
  };
  return [
    { id: "top-level", label: "Top level", checked: current === null, onSelect: move(null) },
    ...(folders.length > 0 ? [{ kind: "separator" as const, id: "folders" }] : []),
    ...folders.map((folder): MenuEntry => ({ id: `folder:${folder}`, label: folder, checked: folder === current, onSelect: move(folder) })),
    { kind: "separator", id: "new" },
    {
      id: "new-folder",
      label: "New folder…",
      onSelect: () => {
        openMoveToFolder({ board: ref, folder: current });
      },
    },
  ];
}

/** The menu for a board, from what is known about it. */
export function boardMenuFor(ref: BoardRef, { summary, folders, page }: BoardFacts): MenuEntry[] {
  return [
    { id: "rename", label: "Rename…", onSelect: () => void renameBoardInPlace(ref) },
    { id: "move", label: "Move to folder", submenu: folderEntries(ref, summary?.folder ?? null, folders) },
    { kind: "separator", id: "s1" },
    {
      id: "export-png",
      label: "Export PNG…",
      disabled: page === null,
      onSelect: () => {
        if (page !== null) {
          void exportPng({ board: ref, page });
        }
      },
    },
    { id: "copy-link", label: "Copy link", onSelect: () => void copyBoardLink(ref) },
    { kind: "separator", id: "s2" },
    {
      id: "delete",
      label: "Delete…",
      danger: true,
      onSelect: () => {
        openDialog({ kind: "deleteBoard", board: ref });
      },
    },
  ];
}

/** The board menu for a board (for context menus on Home and in the sidebar), from the cached `list_boards`. */
export function boardMenuEntries(ref: BoardRef): MenuEntry[] {
  return boardMenuFor(ref, factsOf(ref, queryClient.getQueryData<BoardList>(engineKey("list_boards", { project: ref.project }))));
}

/** The board header's `⋯` (12 §2.1), right-aligned under it, 220 wide. */
export function BoardMenuButton({ board }: { readonly board: BoardRef }): JSX.Element {
  const list = useEngineQuery("list_boards", { project: board.project });
  // Re-render when the page the session shows changes (Export PNG exports it).
  useOpenBoard(board, (open) => open.activePage);
  return <MenuButton variant="icon" icon="dots" label="Board menu" entries={boardMenuFor(board, factsOf(board, list.data))} align="end" width={220} />;
}
