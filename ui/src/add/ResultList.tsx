/**
 * Add's virtualized result list: section headers (caps labels) and fixed-height rows. The selection is
 * driven from Add's input (focus never leaves it, so the caret stays where the person types), and the list keeps the selected row in view. A listbox the input points into with
 * `aria-activedescendant`.
 */

import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, type MouseEvent, type ReactNode } from "react";

interface ListSection<T> {
  readonly id: string;
  readonly label: string;
  readonly items: readonly T[];
}

interface ResultListProps<T> {
  /** The listbox's DOM id; options are `${id}-<index>`. */
  readonly id: string;
  readonly label: string;
  readonly sections: readonly ListSection<T>[];
  readonly getId: (item: T) => string;
  readonly getHeight: (item: T) => number;
  /** The row's accessible name. */
  readonly getText: (item: T) => string;
  readonly renderItem: (item: T, selected: boolean) => ReactNode;
  readonly selectedId: string | null;
  readonly onActivate: (item: T, event: MouseEvent) => void;
  readonly className?: string;
}

const HEADER_HEIGHT = 24;
/** The list's padding above the first and below the last row. */
const LIST_PADDING = { start: 6, end: 8 } as const;

type Row<T> = { readonly kind: "header"; readonly section: ListSection<T> } | { readonly kind: "item"; readonly item: T; readonly option: number };

/** The rows of the sections: a header, then the items, numbered as options. */
function rowsOf<T>(sections: readonly ListSection<T>[]): Row<T>[] {
  const rows: Row<T>[] = [];
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

export function ResultList<T>({
  id,
  label,
  sections,
  getId,
  getHeight,
  getText,
  renderItem,
  selectedId,
  onActivate,
  className,
}: ResultListProps<T>) {
  const scroller = useRef<HTMLDivElement | null>(null);
  const rows = useMemo(() => rowsOf(sections), [sections]);

  // TanStack Virtual hands back fresh functions each render by design; the app doesn't use the React
  // Compiler, so its "can't memoize" notice doesn't apply.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: (index) => {
      const row = rows[index];
      return row?.kind === "item" ? getHeight(row.item) : HEADER_HEIGHT;
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

  return (
    <div ref={scroller} id={id} role="listbox" aria-label={label} className={className}>
      <div className="add-list-spacer" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((virtual) => {
          const row = rows[virtual.index];
          if (!row) {
            return null;
          }
          const style = { height: virtual.size, transform: `translateY(${virtual.start}px)` };
          if (row.kind === "header") {
            return (
              <div key={virtual.key} role="presentation" className="add-list-header" style={style}>
                {row.section.label}
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
