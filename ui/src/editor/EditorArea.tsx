/**
 * The editor area (architecture §11.4): tabs for boards and read-only source files. Every tab stays
 * mounted so a board keeps its camera and a source tab its folds and scroll; only the active one
 * shows. With none open, a quiet empty state.
 */

import { FileCode, LayoutGrid } from "lucide-react";
import { BoardTab } from "../canvas/BoardTab";
import { Chip, EditorTabs, EmptyState, Kbd, useContextMenu, type EditorTab } from "../components";
import { useEngineQuery } from "../ipc/queries";
import { ActionIds, useShortcut, usePlatform } from "../keyboard";
import { runMoveToNewWindow } from "../boards/flows";
import { activateTab, closeTab, useTabs, type Tab } from "../state/tabs";
import { revealInFileManager, revealLabel } from "../views/actions";
import "./EditorArea.css";
import { SourceTab } from "./source/SourceTab";
import { useBoardTabs } from "./useBoardTabs";

const STRIP_ID = "editor";

function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/** Board titles and places by ID, from the Boards list. */
function useBoardTitles(): ReadonlyMap<string, { title: string; draft: boolean }> {
  const list = useEngineQuery("list_boards", {});
  const map = new Map<string, { title: string; draft: boolean }>();
  for (const board of [...(list.data?.published ?? []), ...(list.data?.drafts ?? [])]) {
    map.set(board.id, { title: board.title ?? board.name, draft: board.place === "draft" });
  }
  return map;
}

function boardFallbackTitle(boardId: string): string {
  return fileName(boardId).replace(/\.json$/, "");
}

export function EditorArea() {
  useBoardTabs();
  const tabs = useTabs((state) => state.tabs);
  const activeId = useTabs((state) => state.activeId);
  const titles = useBoardTitles();
  const platform = usePlatform();
  const menu = useContextMenu("Tab actions");
  const openShortcut = useShortcut(ActionIds.quickOpenFiles) ?? undefined;

  const stripTabs = tabs.map((tab): EditorTab => {
    if (tab.kind === "source") {
      return { id: tab.id, label: fileName(tab.path), title: tab.path, icon: <FileCode size={13} /> };
    }
    const known = titles.get(tab.boardId);
    return {
      id: tab.id,
      label: known?.title ?? boardFallbackTitle(tab.boardId),
      title: tab.boardId,
      icon: <LayoutGrid size={13} />,
      ...(known?.draft ? { decoration: <Chip tone="neutral">Draft</Chip> } : {}),
    };
  });

  const tabMenu = (tab: Tab) => [
    { id: "close", label: "Close", shortcut: "Mod+W", onSelect: () => void closeTab(tab.id) },
    {
      id: "others",
      label: "Close Others",
      disabled: tabs.length < 2,
      onSelect: () => {
        for (const other of tabs) {
          if (other.id !== tab.id) {
            void closeTab(other.id);
          }
        }
      },
    },
    ...(tab.kind === "source"
      ? [
          { kind: "separator" as const, id: "s1" },
          { id: "reveal", label: revealLabel(platform), onSelect: () => void revealInFileManager(tab.path) },
        ]
      : [
          { kind: "separator" as const, id: "s1" },
          { id: "move", label: "Move to New Window", onSelect: () => void runMoveToNewWindow(tab.boardId) },
        ]),
  ];

  if (tabs.length === 0) {
    return (
      <main className="editor" aria-label="Editor">
        <EmptyState
          title="No tabs open"
          hint={
            <>
              Open an entrance or a definition to start a board, or a file to read it.
              {openShortcut ? (
                <>
                  {" "}
                  Go to a file with <Kbd shortcut={openShortcut} />.
                </>
              ) : null}
            </>
          }
        />
      </main>
    );
  }

  return (
    <main className="editor editor-with-tabs" aria-label="Editor">
      <EditorTabs
        id={STRIP_ID}
        tabs={stripTabs}
        activeId={activeId}
        panelId="editor-panel"
        onSelect={activateTab}
        onClose={(id) => void closeTab(id)}
        onContextMenu={(id, at) => {
          const tab = tabs.find((entry) => entry.id === id);
          if (tab) {
            menu.open(at, tabMenu(tab));
          }
        }}
      />
      <div id="editor-panel" role="tabpanel" className="editor-panel">
        {tabs.map((tab) =>
          tab.kind === "board" ? (
            <div key={tab.id} className="editor-board" hidden={tab.id !== activeId}>
              <BoardTab boardId={tab.boardId} active={tab.id === activeId} />
            </div>
          ) : (
            <SourceTab key={tab.id} tab={tab} active={tab.id === activeId} />
          ),
        )}
      </div>
      {menu.element}
    </main>
  );
}
