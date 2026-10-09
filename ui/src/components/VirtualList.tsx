/**
 * A virtualized, grouped list (Add's results, the Health list): each group has a caps header that
 * sticks to the top while its items scroll. A listbox: focus stays on the list, ↑/↓ PageUp/PageDown Home/End move the
 * active item (`aria-activedescendant`), Enter or a click activates it, Shift+F10 and the Menu key
 * open its context menu.
 */

import { defaultRangeExtractor, useVirtualizer, type Range } from "@tanstack/react-virtual";
import { useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type ReactNode, type Ref } from "react";
import { assignRef, cx, isContextMenuKey, pointBelow, type Point } from "./internal/dom";
import "./VirtualList.css";

export interface ListGroup<T> {
  readonly id: string;
  /** Header text, e.g. "Functions". */
  readonly label: ReactNode;
  /** Shown after the label, e.g. `2` or "12 matches in 5 files". */
  readonly count?: ReactNode;
  readonly items: readonly T[];
}

export interface ListItemState {
  readonly active: boolean;
  readonly selected: boolean;
}

export interface VirtualListProps<T> {
  /** The list's accessible name. */
  readonly label: string;
  readonly groups: readonly ListGroup<T>[];
  readonly getId: (item: T) => string;
  readonly renderItem: (item: T, state: ListItemState) => ReactNode;
  /** Plain text of an item, its accessible name when `renderItem` isn't plain text. */
  readonly getText?: (item: T) => string;
  /** Every item's height in px (rows aren't measured). Default 28; Add's results are 32. */
  readonly rowHeight?: number;
  /** For lists whose items differ in height; overrides `rowHeight`. */
  readonly getItemHeight?: (item: T) => number;
  /** Default 24. */
  readonly headerHeight?: number;
  readonly selectedId?: string | null;
  /** Click or Enter. */
  readonly onActivate?: (item: T) => void;
  /** The keyboard-active item changed (for previews). */
  readonly onActiveChange?: (item: T) => void;
  readonly onContextMenu?: (item: T, at: Point) => void;
  /** Shown when there are no items at all. */
  readonly empty?: ReactNode;
  readonly className?: string;
  /** The focusable list element (e.g. so a search field's ↓ can move focus into the results). */
  readonly ref?: Ref<HTMLDivElement>;
}

type Row<T> =
  | { readonly kind: "header"; readonly group: ListGroup<T> }
  | { readonly kind: "item"; readonly item: T; readonly id: string };

export function VirtualList<T>({
  label,
  groups,
  getId,
  renderItem,
  getText,
  rowHeight = 28,
  getItemHeight,
  headerHeight = 24,
  selectedId = null,
  onActivate,
  onActiveChange,
  onContextMenu,
  empty,
  className,
  ref,
}: VirtualListProps<T>) {
  const domId = useId();
  const scroller = useRef<HTMLDivElement | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);

  const rows = useMemo(() => {
    const flat: Row<T>[] = [];
    for (const group of groups) {
      if (group.items.length === 0) {
        continue;
      }
      flat.push({ kind: "header", group });
      for (const item of group.items) {
        flat.push({ kind: "item", item, id: getId(item) });
      }
    }
    return flat;
  }, [groups, getId]);
  const headerIndexes = useMemo(() => rows.flatMap((row, index) => (row.kind === "header" ? [index] : [])), [rows]);
  const itemIndexes = useMemo(() => rows.flatMap((row, index) => (row.kind === "item" ? [index] : [])), [rows]);
  const indexById = useMemo(
    () => new Map(rows.flatMap((row, index) => (row.kind === "item" ? [[row.id, index] as const] : []))),
    [rows],
  );

  const stickyIndexFor = useCallback(
    (start: number) => headerIndexes.findLast((index) => index <= start) ?? -1,
    [headerIndexes],
  );

  // TanStack Virtual hands back fresh functions each render by design; the app doesn't use the React
  // Compiler, so its "can't memoize" notice doesn't apply.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: (index) => {
      const row = rows[index];
      return row?.kind === "item" ? (getItemHeight?.(row.item) ?? rowHeight) : headerHeight;
    },
    getItemKey: (index) => {
      const row = rows[index];
      return row?.kind === "item" ? `i:${row.id}` : `h:${row?.group.id ?? index}`;
    },
    // The current group's header always renders, so it can stick.
    rangeExtractor: useCallback(
      (range: Range) => {
        const sticky = stickyIndexFor(range.startIndex);
        const indexes = defaultRangeExtractor(range);
        return sticky >= 0 && !indexes.includes(sticky) ? [sticky, ...indexes] : indexes;
      },
      [stickyIndexFor],
    ),
    overscan: 10,
  });

  // Row sizes depend on the rows; re-measure when they change.
  useEffect(() => {
    virtualizer.measure();
  }, [rows, virtualizer]);

  const active = activeId !== null && indexById.has(activeId) ? activeId : null;
  const activeIndex = active === null ? -1 : (indexById.get(active) ?? -1);
  const stickyIndex = stickyIndexFor(virtualizer.range?.startIndex ?? 0);

  const moveTo = (index: number) => {
    const row = rows[index];
    if (row?.kind !== "item") {
      return;
    }
    setActiveId(row.id);
    onActiveChange?.(row.item);
    // Keep the active row clear of the sticky header above it.
    virtualizer.scrollToIndex(index > 0 && rows[index - 1]?.kind === "header" ? index - 1 : index, { align: "auto" });
  };

  const moveBy = (delta: number) => {
    const position = itemIndexes.indexOf(activeIndex);
    const from = position < 0 ? (delta > 0 ? -1 : itemIndexes.length) : position;
    const next = itemIndexes[Math.max(0, Math.min(itemIndexes.length - 1, from + delta))];
    if (next !== undefined) {
      moveTo(next);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.metaKey || event.ctrlKey || event.altKey) {
      return;
    }
    const row = rows[activeIndex];
    if (isContextMenuKey(event)) {
      if (row?.kind === "item" && onContextMenu) {
        event.preventDefault();
        const element = document.getElementById(`${domId}-${activeIndex}`);
        onContextMenu(row.item, element ? pointBelow(element) : { x: 0, y: 0 });
      }
      return;
    }
    const page = Math.max(1, Math.floor((scroller.current?.clientHeight ?? rowHeight * 10) / rowHeight) - 1);
    switch (event.key) {
      case "ArrowDown":
        moveBy(1);
        break;
      case "ArrowUp":
        moveBy(-1);
        break;
      case "PageDown":
        moveBy(page);
        break;
      case "PageUp":
        moveBy(-page);
        break;
      case "Home":
        moveBy(-itemIndexes.length);
        break;
      case "End":
        moveBy(itemIndexes.length);
        break;
      case "Enter":
        if (row?.kind === "item") {
          onActivate?.(row.item);
        }
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  if (rows.length === 0) {
    return <div className={cx("vlist", "vlist-empty", className)}>{empty}</div>;
  }

  return (
    <div
      ref={(element) => {
        scroller.current = element;
        assignRef(ref, element);
      }}
      role="listbox"
      aria-label={label}
      aria-activedescendant={activeIndex >= 0 ? `${domId}-${activeIndex}` : undefined}
      tabIndex={0}
      className={cx("vlist", className)}
      onKeyDown={onKeyDown}
      onFocus={(event) => {
        // Arriving by keyboard with nothing active starts at the selection, or the first item.
        if (event.target === event.currentTarget && active === null) {
          const start = selectedId === null ? undefined : indexById.get(selectedId);
          const first = start ?? itemIndexes[0];
          if (first !== undefined) {
            moveTo(first);
          }
        }
      }}
    >
      <div className="vlist-spacer" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((virtual) => {
          const row = rows[virtual.index];
          if (!row) {
            return null;
          }
          if (row.kind === "header") {
            const sticky = virtual.index === stickyIndex;
            return (
              <div
                key={virtual.key}
                role="presentation"
                className={cx("vlist-header", sticky && "vlist-header-sticky")}
                style={sticky ? { height: virtual.size } : { height: virtual.size, transform: `translateY(${virtual.start}px)` }}
              >
                <span className="vlist-header-label">{row.group.label}</span>
                {row.group.count === undefined ? null : <span className="vlist-header-count">{row.group.count}</span>}
              </div>
            );
          }
          const selected = row.id === selectedId;
          const isActive = virtual.index === activeIndex;
          return (
            <div
              key={virtual.key}
              id={`${domId}-${virtual.index}`}
              role="option"
              aria-selected={selected}
              aria-label={getText?.(row.item)}
              data-active={isActive || undefined}
              className={cx("vlist-item", selected && "vlist-item-selected")}
              style={{ height: virtual.size, transform: `translateY(${virtual.start}px)` }}
              onClick={() => {
                setActiveId(row.id);
                onActivate?.(row.item);
              }}
              onContextMenu={(event) => {
                if (onContextMenu) {
                  event.preventDefault();
                  setActiveId(row.id);
                  onContextMenu(row.item, { x: event.clientX, y: event.clientY });
                }
              }}
            >
              {renderItem(row.item, { active: isActive, selected })}
            </div>
          );
        })}
      </div>
    </div>
  );
}
