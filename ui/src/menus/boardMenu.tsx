/**
 * The board menu (artboard 12 §2, V-43, V-7): the `⋯` in the board header, and the right-click menu of
 * a board on Home and in the sidebar. Rename…, Move to folder ›, Duplicate, Open in new window, Share ›,
 * Fold all cards, Unfold all cards, Discard changes… (only with unpublished changes), Move back to
 * drafts (only published), Delete…. Share, Fold, and Unfold need a page, so they are disabled for a
 * board this window doesn't hold.
 */

import type { JSX } from "react";
import { MenuButton, type MenuEntry } from "../components";
import { engineKey, queryClient, useEngineQuery, type BoardList, type BoardSummary } from "../ipc";
import { openMoveToFolder } from "../dialogs";
import { foldAll, getOpenBoard, useOpenBoard, type BoardRef, type PageId, type PageTarget } from "../state/boards";
import { openDialog } from "../state/overlays";
import { copyMermaid, duplicate, exportMermaid, moveBackToDrafts, moveToFolder, openInNewWindow, renameBoardInPlace } from "./actions";

/** What the menu needs to know about a board. */
export interface BoardFacts {
  /** From `list_boards`, when loaded. */
  readonly summary: BoardSummary | undefined;
  /** Every board folder of the project, sorted (with the folders above each). */
  readonly folders: readonly string[];
  /** The board's open session in this window. */
  readonly open: { readonly title: string; readonly place: "published" | "draft"; readonly changes: number; readonly page: PageId } | null;
}

const byName = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** Every folder of a project's boards and its empty folders, with each folder's ancestors, sorted. */
export function projectFolders(list: BoardList | undefined): string[] {
  const folders = new Set<string>();
  const add = (path: string) => {
    const parts = path.split("/");
    for (let depth = 1; depth <= parts.length; depth += 1) {
      folders.add(parts.slice(0, depth).join("/"));
    }
  };
  for (const board of list?.boards ?? []) {
    if (board.folder !== null) {
      add(board.folder);
    }
  }
  list?.emptyFolders.forEach(add);
  return [...folders].sort(byName.compare);
}

function factsOf(ref: BoardRef, list: BoardList | undefined): BoardFacts {
  const board = getOpenBoard(ref);
  return {
    summary: list?.boards.find((item) => item.id === ref.id),
    folders: projectFolders(list),
    open: board
      ? { title: board.document.title, place: board.place, changes: board.published?.changes ?? 0, page: board.activePage }
      : null,
  };
}

export function shareMenuEntries(target: PageTarget): MenuEntry[] {
  return [
    { id: "export-mermaid", label: "Export Mermaid…", onSelect: () => void exportMermaid(target) },
    { id: "copy-mermaid", label: "Copy Mermaid", onSelect: () => void copyMermaid(target) },
  ];
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
        openMoveToFolder({ board: ref, folder: current, mode: "new" });
      },
    },
  ];
}

/** A board's title, place, unpublished changes (`list_boards` follows every save), and folder. */
export function describeBoard({ summary, open }: BoardFacts): {
  readonly title: string;
  readonly place: "published" | "draft";
  readonly changes: number;
  readonly folder: string | null;
} {
  return {
    title: open?.title ?? summary?.title ?? summary?.name ?? "the board",
    place: summary?.place ?? open?.place ?? "draft",
    changes: summary?.changes ?? open?.changes ?? 0,
    folder: summary?.folder ?? null,
  };
}

/** The menu for a board, from what is known about it. */
export function boardMenuFor(ref: BoardRef, facts: BoardFacts): MenuEntry[] {
  const { summary, open } = facts;
  const { title, place, changes, folder } = describeBoard(facts);
  const target: PageTarget | null = open ? { board: ref, page: open.page } : null;
  const fold = (folded: boolean) => () => {
    if (target) {
      foldAll(target, folded);
    }
  };
  const fileEntries: MenuEntry[] = [
    ...(changes > 0
      ? [
          {
            id: "discard",
            label: "Discard changes…",
            onSelect: () => {
              openDialog({ kind: "discard", board: ref });
            },
          },
        ]
      : []),
    ...(place === "published" ? [{ id: "drafts", label: "Move back to drafts", onSelect: () => void moveBackToDrafts(ref, title) }] : []),
  ];
  return [
    { id: "rename", label: "Rename…", onSelect: () => void renameBoardInPlace(ref) },
    { id: "move", label: "Move to folder", submenu: folderEntries(ref, folder, facts.folders) },
    { id: "duplicate", label: "Duplicate", onSelect: () => void duplicate(ref) },
    { id: "new-window", label: "Open in new window", disabled: !open && (summary?.heldBy ?? null) !== null, onSelect: () => void openInNewWindow(ref) },
    { kind: "separator", id: "s1" },
    target
      ? { id: "share", label: "Share", submenu: shareMenuEntries(target) }
      : { id: "share", label: "Share", disabled: true, onSelect: () => undefined },
    { id: "fold", label: "Fold all cards", disabled: !target, onSelect: fold(true) },
    { id: "unfold", label: "Unfold all cards", disabled: !target, onSelect: fold(false) },
    ...(fileEntries.length > 0 ? [{ kind: "separator" as const, id: "s2" }, ...fileEntries] : []),
    { kind: "separator", id: "s3" },
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

/** What is known about a board now: the cached `list_boards` and this window's session. */
export function cachedBoardFacts(ref: BoardRef): BoardFacts {
  return factsOf(ref, queryClient.getQueryData<BoardList>(engineKey("list_boards", { project: ref.project })));
}

/** The board menu for a board (for context menus on Home and in the sidebar). */
export function boardMenuEntries(ref: BoardRef): MenuEntry[] {
  return boardMenuFor(ref, cachedBoardFacts(ref));
}

/** The board header's `⋯` (12 §2.1), right-aligned under it, 220 wide. */
export function BoardMenuButton({ board }: { readonly board: BoardRef }): JSX.Element {
  const list = useEngineQuery("list_boards", { project: board.project });
  // Re-render when the session's title, place, or page changes.
  useOpenBoard(board, (open) => `${open.document.title}\u0000${open.place}\u0000${open.activePage}\u0000${open.published?.changes ?? 0}`);
  return (
    <MenuButton
      variant="icon"
      icon="dots"
      label="Board menu"
      entries={boardMenuFor(board, factsOf(board, list.data))}
      align="end"
      width={220}
    />
  );
}
