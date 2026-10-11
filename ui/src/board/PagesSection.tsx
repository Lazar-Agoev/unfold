/**
 * Pages: the board's pages in order, each with a review mark when something on it needs review; + adds
 * a page. A row's menu (or right-click) renames it in place (also on double-click), duplicates it,
 * copies a link to it for an agent's chat, moves it up or down, or deletes it after asking (not the
 * only page).
 */

import { useMemo, useState, type MouseEvent } from "react";
import { copyLink } from "../agent";
import { ConfirmDialog, IconButton, ListRow, MenuButton, SectionLabel, useContextMenu, type MenuEntry } from "../components";
import { Icon } from "../icons";
import type { Page } from "../ipc/generated";
import { ActionIds, useShortcut } from "../keyboard";
import { addPage, deletePage, duplicatePage, movePage, renamePage, setActivePage, useOpenBoard, type BoardRef, type PageTarget } from "../state/boards";
import { InlineRename } from "./InlineRename";

/** The page menu's width (as the board menu). */
const PAGE_MENU_WIDTH = 220;

/** The ids of the pages with something that needs review (a card from a project that isn't connected doesn't). */
function usePagesWithItems(board: BoardRef): ReadonlySet<string> {
  const health = useOpenBoard(board, (open) => open.health);
  return useMemo(() => new Set((health?.items ?? []).filter((item) => item.kind !== "notConnected").map((item) => item.page)), [health]);
}

export function PagesSection({ board }: { readonly board: BoardRef }) {
  const pages = useOpenBoard(board, (open) => open.document.pages);
  const active = useOpenBoard(board, (open) => open.activePage);
  const marked = usePagesWithItems(board);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<Page | null>(null);
  const contextMenu = useContextMenu("Page actions", PAGE_MENU_WIDTH);
  const newPageKey = useShortcut(ActionIds.newPage);
  const count = pages?.length ?? 0;

  const entries = (page: Page, index: number): MenuEntry[] => {
    const target: PageTarget = { board, page: page.id };
    return [
      { id: "rename", label: "Rename", icon: "edit", onSelect: () => { setRenaming(page.id); } },
      { id: "duplicate", label: "Duplicate page", icon: "copy", onSelect: () => { duplicatePage(target); } },
      { id: "copy-link", label: "Copy link", onSelect: () => void copyLink(target) },
      { kind: "separator", id: "s1" },
      { id: "up", label: "Move up", disabled: index === 0, onSelect: () => { movePage(target, -1); } },
      { id: "down", label: "Move down", disabled: index === count - 1, onSelect: () => { movePage(target, 1); } },
      { kind: "separator", id: "s2" },
      { id: "delete", label: "Delete page…", danger: true, disabled: count < 2, onSelect: () => { setDeleting(page); } },
    ];
  };

  return (
    <section className="board-section" aria-label="Pages">
      <SectionLabel
        trailing={
          <IconButton
            icon="plus"
            label="New page"
            size={24}
            {...(newPageKey !== null ? { shortcut: newPageKey } : {})}
            onClick={() => {
              addPage(board);
            }}
          />
        }
      >
        Pages
      </SectionLabel>
      <div role="listbox" aria-label="Pages of the board">
        {(pages ?? []).map((page, index) => (
          <ListRow
            key={page.id}
            className="page-row"
            role="option"
            aria-selected={page.id === active}
            tabIndex={0}
            selected={page.id === active}
            leading={<Icon name="page" />}
            label={
              <InlineRename
                value={page.title}
                editing={renaming === page.id}
                label="Page title"
                className="board-page-title"
                onDone={(title) => {
                  setRenaming(null);
                  if (title !== null) {
                    renamePage({ board, page: page.id }, title);
                  }
                }}
              >
                {page.title}
              </InlineRename>
            }
            trailing={marked.has(page.id) ? <span className="board-review-dot" aria-label="Needs review" /> : undefined}
            menu={<MenuButton variant="icon" icon="dots" label={`${page.title} actions`} size={20} entries={entries(page, index)} width={PAGE_MENU_WIDTH} align="end" />}
            onClick={() => {
              setActivePage(board, page.id);
            }}
            onDoubleClick={() => {
              setRenaming(page.id);
            }}
            onContextMenu={(event: MouseEvent) => {
              contextMenu.open(event, entries(page, index));
            }}
          />
        ))}
      </div>
      {contextMenu.element}
      {deleting ? (
        <ConfirmDialog
          open
          danger
          placement="canvas"
          title={`Delete “${deleting.title}”?`}
          message="Its cards, groups, labels, and links go with it. ⌘Z brings it back."
          confirmLabel="Delete page"
          onCancel={() => {
            setDeleting(null);
          }}
          onConfirm={() => {
            deletePage({ board, page: deleting.id });
            setDeleting(null);
          }}
        />
      ) : null}
    </section>
  );
}
