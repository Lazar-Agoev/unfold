/**
 * Tab strips. `EditorTabs` holds boards and source files: closable tabs (× button, middle-click,
 * Delete), a context-menu callback, and overflow handled by horizontal scrolling plus a "Show all
 * tabs" menu. `PanelTabs` is the bottom panel's quieter strip with optional counts.
 *
 * Both are WAI-ARIA tab lists with automatic activation: Tab reaches the active tab, ←/→ (and
 * Home/End) move and select.
 */

import { ChevronDown, X } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { Badge } from "./Chip";
import { cx, isContextMenuKey, pointBelow, type Point } from "./internal/dom";
import { MenuButton } from "./Menu";
import "./Tabs.css";

/** The DOM id of a tab, for `aria-labelledby` on the panel it controls. */
export function tabDomId(listId: string, tabId: string): string {
  return `${listId}-tab-${tabId}`;
}

/** Arrow/Home/End navigation shared by both strips. Returns the id to select, if the key moves. */
function nextTab(key: string, ids: readonly string[], current: string | null): string | null {
  const index = current === null ? -1 : ids.indexOf(current);
  switch (key) {
    case "ArrowRight":
      return ids[(index + 1) % ids.length] ?? null;
    case "ArrowLeft":
      return ids[(index - 1 + ids.length) % ids.length] ?? null;
    case "Home":
      return ids[0] ?? null;
    case "End":
      return ids.at(-1) ?? null;
    default:
      return null;
  }
}

export interface EditorTab {
  readonly id: string;
  readonly label: string;
  /** A small leading icon (board or file type). */
  readonly icon?: ReactNode;
  /** The tooltip, e.g. the file's full path. */
  readonly title?: string;
  /** Trailing content before the close button, e.g. a Draft chip or a change dot. */
  readonly decoration?: ReactNode;
  /** Default true. */
  readonly closable?: boolean;
}

export interface EditorTabsProps {
  /** Used for DOM ids; one per strip. */
  readonly id: string;
  readonly tabs: readonly EditorTab[];
  readonly activeId: string | null;
  readonly onSelect: (id: string) => void;
  readonly onClose: (id: string) => void;
  /** Right-click, Shift+F10, or the Menu key on a tab. Open a ContextMenu at `at`. */
  readonly onContextMenu?: (id: string, at: Point) => void;
  /** The id of the element showing the active tab's content. */
  readonly panelId?: string;
  readonly label?: string;
  readonly className?: string;
}

export function EditorTabs({
  id,
  tabs,
  activeId,
  onSelect,
  onClose,
  onContextMenu,
  panelId,
  label = "Open tabs",
  className,
}: EditorTabsProps) {
  const strip = useRef<HTMLDivElement>(null);
  const [overflowing, setOverflowing] = useState(false);

  // Track whether the tabs overflow the strip, to offer the "Show all tabs" menu.
  useEffect(() => {
    const element = strip.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return undefined;
    }
    const measure = () => {
      setOverflowing(element.scrollWidth > element.clientWidth + 1);
    };
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    return () => {
      observer.disconnect();
    };
  }, [tabs.length]);

  // Keep the active tab in view.
  useEffect(() => {
    if (activeId === null) {
      return;
    }
    document.getElementById(tabDomId(id, activeId))?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [id, activeId]);

  const focusTab = (tabId: string) => {
    document.getElementById(tabDomId(id, tabId))?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>, tab: EditorTab) => {
    if (isContextMenuKey(event) && onContextMenu) {
      event.preventDefault();
      onContextMenu(tab.id, pointBelow(event.currentTarget));
      return;
    }
    if ((event.key === "Delete" || event.key === "Backspace") && tab.closable !== false) {
      event.preventDefault();
      // Focus moves to the neighbour that will become active.
      const index = tabs.findIndex((entry) => entry.id === tab.id);
      const neighbour = tabs[index + 1] ?? tabs[index - 1];
      onClose(tab.id);
      if (neighbour) {
        requestAnimationFrame(() => {
          focusTab(neighbour.id);
        });
      }
      return;
    }
    const next = nextTab(
      event.key,
      tabs.map((entry) => entry.id),
      tab.id,
    );
    if (next !== null) {
      event.preventDefault();
      onSelect(next);
      focusTab(next);
    }
  };

  return (
    <div className={cx("editor-tabs", className)}>
      <div
        ref={strip}
        role="tablist"
        aria-label={label}
        className="editor-tabs-strip"
        onWheel={(event) => {
          // A vertical wheel scrolls the strip sideways.
          if (Math.abs(event.deltaY) > Math.abs(event.deltaX)) {
            event.currentTarget.scrollLeft += event.deltaY;
          }
        }}
      >
        {tabs.map((tab) => {
          const active = tab.id === activeId;
          const closable = tab.closable !== false;
          return (
            <div
              key={tab.id}
              id={tabDomId(id, tab.id)}
              role="tab"
              aria-selected={active}
              aria-controls={panelId}
              tabIndex={active || (activeId === null && tab === tabs[0]) ? 0 : -1}
              title={tab.title}
              className={cx("editor-tab", active && "editor-tab-active")}
              onClick={() => {
                onSelect(tab.id);
              }}
              onMouseDown={(event) => {
                // A middle press would start autoscroll; the click that follows closes the tab.
                if (event.button === 1) {
                  event.preventDefault();
                }
              }}
              onAuxClick={(event) => {
                if (event.button === 1 && closable) {
                  event.preventDefault();
                  onClose(tab.id);
                }
              }}
              onContextMenu={(event) => {
                if (onContextMenu) {
                  event.preventDefault();
                  onSelect(tab.id);
                  onContextMenu(tab.id, { x: event.clientX, y: event.clientY });
                }
              }}
              onKeyDown={(event) => {
                onKeyDown(event, tab);
              }}
            >
              {tab.icon ? (
                <span className="editor-tab-icon" aria-hidden="true">
                  {tab.icon}
                </span>
              ) : null}
              <span className="editor-tab-label">{tab.label}</span>
              {tab.decoration}
              {closable ? (
                <button
                  type="button"
                  className="editor-tab-close"
                  // The tab itself is the keyboard stop; Delete closes it.
                  tabIndex={-1}
                  aria-label={`Close ${tab.label}`}
                  onClick={(event) => {
                    event.stopPropagation();
                    onClose(tab.id);
                  }}
                >
                  <X size={13} aria-hidden="true" />
                </button>
              ) : null}
            </div>
          );
        })}
      </div>
      {overflowing ? (
        <MenuButton
          variant="icon"
          icon={ChevronDown}
          label="Show all tabs"
          size="small"
          className="editor-tabs-more"
          align="end"
          entries={tabs.map((tab) => ({
            id: tab.id,
            label: tab.label,
            checked: tab.id === activeId,
            onSelect: () => {
              onSelect(tab.id);
            },
          }))}
        />
      ) : null}
    </div>
  );
}

export interface PanelTab {
  readonly id: string;
  readonly label: string;
  /** A count shown after the label (hidden when 0 or absent). */
  readonly count?: number;
  /** Spoken form of the count, e.g. "2 problems". */
  readonly countLabel?: string;
}

export interface PanelTabsProps {
  readonly id: string;
  readonly tabs: readonly PanelTab[];
  readonly activeId: string;
  readonly onSelect: (id: string) => void;
  readonly panelId?: string;
  readonly label: string;
  readonly className?: string;
}

export function PanelTabs({ id, tabs, activeId, onSelect, panelId, label, className }: PanelTabsProps) {
  return (
    <div role="tablist" aria-label={label} className={cx("panel-tabs-list", className)}>
      {tabs.map((tab) => (
        <button
          key={tab.id}
          id={tabDomId(id, tab.id)}
          type="button"
          role="tab"
          aria-selected={tab.id === activeId}
          aria-controls={panelId}
          tabIndex={tab.id === activeId ? 0 : -1}
          className="panel-tabs-tab"
          onClick={() => {
            onSelect(tab.id);
          }}
          onKeyDown={(event) => {
            const next = nextTab(
              event.key,
              tabs.map((entry) => entry.id),
              tab.id,
            );
            if (next !== null) {
              event.preventDefault();
              onSelect(next);
              document.getElementById(tabDomId(id, next))?.focus();
            }
          }}
        >
          {tab.label}
          {tab.count ? <Badge count={tab.count} {...(tab.countLabel ? { label: tab.countLabel } : {})} /> : null}
        </button>
      ))}
    </div>
  );
}
