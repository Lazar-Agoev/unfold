/**
 * The window's tab strip (components §2, V-32, V-33, V-44), which is also the title bar and the window's
 * drag region: Home pinned first, then a tab per open board with its project's monogram. A board tab
 * shows `×` in a fixed slot while active or hovered, closes on a middle click, and has a menu (Close,
 * Close other tabs, Open in new window). Tabs shrink to 96px; past that the strip scrolls, keeping the
 * active tab in view.
 */

import { useEffect, useRef, type KeyboardEvent, type MouseEvent, type WheelEvent } from "react";
import { Monogram, useContextMenu, type MenuEntry } from "../components";
import { Icon } from "../icons";
import { boardKey, moveBoardToNewWindow, useOpenBoard, type BoardRef } from "../state/boards";
import { activateBoard, activateHome, closeOtherTabs, closeTab, useTabs } from "../state/tabs";
import { useWindowStore } from "../state/window";
import { useProjectRow } from "../state/workspace";
import "./TabStrip.css";

function tabEntries(board: BoardRef): MenuEntry[] {
  const key = boardKey(board);
  return [
    { id: "close", label: "Close", shortcut: "Mod+W", onSelect: () => void closeTab(key) },
    { id: "close-others", label: "Close other tabs", onSelect: () => void closeOtherTabs(key) },
    { kind: "separator", id: "s1" },
    { id: "new-window", label: "Open in new window", onSelect: () => void moveBoardToNewWindow(board) },
  ];
}

/** Moves focus between tabs with the arrow keys, Home, and End (a tab activates on Enter or Space). */
function onTablistKeyDown(event: KeyboardEvent<HTMLDivElement>): void {
  const tabs = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('[role="tab"]'));
  const at = tabs.findIndex((tab) => tab === document.activeElement);
  if (at < 0) {
    return;
  }
  const next = { ArrowLeft: at - 1, ArrowRight: at + 1, Home: 0, End: tabs.length - 1 }[event.key];
  if (next === undefined) {
    return;
  }
  event.preventDefault();
  tabs[(next + tabs.length) % tabs.length]?.focus();
}

/** A vertical wheel scrolls the strip sideways when its tabs overflow. */
function onTablistWheel(event: WheelEvent<HTMLDivElement>): void {
  if (event.deltaX === 0 && event.deltaY !== 0) {
    event.currentTarget.scrollLeft += event.deltaY;
  }
}

function HomeTab({ active }: { readonly active: boolean }) {
  return (
    <div className="tab tab-home" role="presentation" data-active={active || undefined} data-tab-key="home">
      <button type="button" role="tab" className="tab-main" aria-selected={active} tabIndex={active ? 0 : -1} onClick={activateHome}>
        <Icon name="home" size={14} className="tab-icon" />
        <span className="tab-label">Home</span>
      </button>
    </div>
  );
}

interface BoardTabProps {
  readonly board: BoardRef;
  readonly active: boolean;
  readonly onMenu: (event: MouseEvent, board: BoardRef) => void;
}

function BoardTab({ board, active, onMenu }: BoardTabProps) {
  const key = boardKey(board);
  const title = useOpenBoard(board, (open) => open.document.title) ?? "";
  const project = useProjectRow(board.project)?.name ?? "";
  return (
    <div
      className="tab tab-board"
      role="presentation"
      data-active={active || undefined}
      data-tab-key={key}
      title={project === "" ? title : `${title} · ${project}`}
      onMouseDown={(event) => {
        // A middle press would start autoscroll; the click that follows closes the tab.
        if (event.button === 1) {
          event.preventDefault();
        }
      }}
      onAuxClick={(event) => {
        if (event.button === 1) {
          void closeTab(key);
        }
      }}
      onContextMenu={(event) => {
        onMenu(event, board);
      }}
    >
      <button
        type="button"
        role="tab"
        className="tab-main"
        aria-selected={active}
        tabIndex={active ? 0 : -1}
        onClick={() => {
          activateBoard(board);
        }}
      >
        <Monogram name={project} size={14} />
        <span className="tab-label">{title}</span>
      </button>
      <button
        type="button"
        className="tab-close"
        aria-label={`Close ${title}`}
        tabIndex={-1}
        onClick={() => void closeTab(key)}
      >
        <Icon name="close" size={12} />
      </button>
    </div>
  );
}

export function TabStrip() {
  const boards = useTabs((state) => state.boards);
  const active = useTabs((state) => state.active);
  const list = useRef<HTMLDivElement>(null);
  const menu = useContextMenu("Tab");
  // macOS draws its traffic lights over the strip's left end (`titleBarStyle: hiddenInset`).
  const trafficLights = useWindowStore((state) => state.info?.platform === "darwin");

  // Keep the active tab in view when it changes or the strip overflows.
  useEffect(() => {
    const tabs = Array.from(list.current?.children ?? []) as HTMLElement[];
    tabs.find((tab) => tab.dataset["tabKey"] === active)?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [active, boards.length]);

  return (
    <header className="tab-strip" data-traffic-lights={trafficLights || undefined}>
      <div ref={list} role="tablist" aria-label="Window tabs" className="tab-strip-tabs" onKeyDown={onTablistKeyDown} onWheel={onTablistWheel}>
        <HomeTab active={active === "home"} />
        {boards.map((board) => {
          const key = boardKey(board);
          return (
            <BoardTab
              key={key}
              board={board}
              active={active === key}
              onMenu={(event, target) => {
                menu.open(event, tabEntries(target));
              }}
            />
          );
        })}
      </div>
      {menu.element}
    </header>
  );
}
