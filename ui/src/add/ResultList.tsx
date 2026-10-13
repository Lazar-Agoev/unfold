/**
 * Add's virtualized result list (search results, the entrance browser's list, and `>` commands): section
 * headers (caps labels, or the caller's) and fixed-height rows. The selection is driven from Add's input (focus never leaves it, so the caret stays where the
 * person types), and the list keeps the selected row in view. A listbox the input points into with
 * `aria-activedescendant`.
 */

import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, type MouseEvent, type ReactNode } from "react";

interface ListSection<T> {
  readonly id: string;
  readonly label: string;
  readonly items: readonly T[];
}

interface ResultListProps<T, S extends ListSection<T>> {
  /** The listbox's DOM id; options are `${id}-<index>`. */
  readonly id: string;
  readonly label: string;
  readonly sections: readonly S[];
  readonly getId: (item: T) => string;
  readonly getHeight: (item: T) => number;
  /** The row's accessible name. */
  readonly getText: (item: T) => string;
  readonly renderItem: (item: T, selected: boolean) => ReactNode;
  /** A section's header and its height (its caps label, 24 tall, when absent). */
  readonly renderHeader?: (section: S) => ReactNode;
  readonly getHeaderHeight?: (section: S) => number;
  /** Called when the last row comes into view (to load more). */
  readonly onEnd?: () => void;
  readonly selectedId: string | null;
  readonly onActivate: (item: T, event: MouseEvent) => void;
  readonly className?: string;
  /** A fixed height in px; otherwise the list fills its container. */
  readonly height?: number;
}

const HEADER_HEIGHT = 24;
/** The list's padding above the first and below the last row. */
const LIST_PADDING = { start: 6, end: 8 } as const;

type Row<T, S> = { readonly kind: "header"; readonly section: S } | { readonly kind: "item"; readonly item: T; readonly option: number };

/** The rows of the sections: a header, then the items, numbered as options. */
function rowsOf<T, S extends ListSection<T>>(sections: readonly S[]): Row<T, S>[] {
  const rows: Row<T, S>[] = [];
  let option = 0;
  for (const section of sections) {
    if (section.items.length === 0) {
      continue;
    }
    rows.push({ kind: "header", section });
    for (const item of section.items) {
      rows.push({ kind: "item", item, option });
      option += 1;
    }
  }
  return rows;
}

/** The DOM id of the option at `index` of the list `id` (for `aria-activedescendant`). */
export function optionId(id: string, index: number): string {
  return `${id}-${index}`;
}

export function ResultList<T, S extends ListSection<T> = ListSection<T>>({
  id,
  label,
  sections,
  getId,
  getHeight,
  getText,
  renderItem,
  renderHeader,
  getHeaderHeight,
  onEnd,
  selectedId,
  onActivate,
  className,
  height,
}: ResultListProps<T, S>) {
  const scroller = useRef<HTMLDivElement | null>(null);
  const rows = useMemo(() => rowsOf<T, S>(sections), [sections]);

  // TanStack Virtual hands back fresh functions each render by design; the app doesn't use the React
  // Compiler, so its "can't memoize" notice doesn't apply.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: (index) => {
      const row = rows[index];
      if (row?.kind === "item") {
        return getHeight(row.item);
      }
      return row && getHeaderHeight ? getHeaderHeight(row.section) : HEADER_HEIGHT;
    },
    getItemKey: (index) => {
      const row = rows[index];
      return row?.kind === "item" ? `i:${getId(row.item)}` : `h:${row?.section.id ?? index}`;
    },
    paddingStart: LIST_PADDING.start,
    paddingEnd: LIST_PADDING.end,
    overscan: 8,
  });

  useEffect(() => {
    virtualizer.measure();
  }, [rows, virtualizer]);

  const selectedIndex = selectedId === null ? -1 : rows.findIndex((row) => row.kind === "item" && getId(row.item) === selectedId);
  useEffect(() => {
    if (selectedIndex < 0) {
      return;
    }
    // A section's first row brings its header along.
    const header = rows[selectedIndex - 1]?.kind === "header";
    virtualizer.scrollToIndex(header ? selectedIndex - 1 : selectedIndex, { align: "auto" });
    // Only a new selection scrolls, not new rows under the same one.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedIndex]);

  const virtualItems = virtualizer.getVirtualItems();
  const atEnd = rows.length > 0 && virtualItems.at(-1)?.index === rows.length - 1;
  useEffect(() => {
    if (atEnd) {
      onEnd?.();
    }
    // Once each time the end comes into view, or new rows end the list.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [atEnd, rows.length]);

  return (
    <div ref={scroller} id={id} role="listbox" aria-label={label} className={className} style={height === undefined ? undefined : { height }}>
      <div className="add-list-spacer" style={{ height: virtualizer.getTotalSize() }}>
        {virtualItems.map((virtual) => {
          const row = rows[virtual.index];
          if (!row) {
            return null;
          }
          const style = { height: virtual.size, transform: `translateY(${virtual.start}px)` };
          if (row.kind === "header") {
            return (
              <div key={virtual.key} role="presentation" className="add-list-header" style={style}>
                {renderHeader ? renderHeader(row.section) : row.section.label}
              </div>
            );
          }
          const selected = virtual.index === selectedIndex;
          return (
            <div
              key={virtual.key}
              id={optionId(id, row.option)}
              role="option"
              aria-selected={selected}
              aria-label={getText(row.item)}
              className={selected ? "add-list-row selected" : "add-list-row"}
              style={style}
              onMouseDown={(event) => {
                // Keep the caret in Add's input.
                event.preventDefault();
              }}
              onClick={(event) => {
                onActivate(row.item, event);
              }}
            >
              {renderItem(row.item, selected)}
            </div>
          );
        })}
      </div>
    </div>
  );
}
