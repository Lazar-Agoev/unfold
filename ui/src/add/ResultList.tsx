/**
 * Add's virtualized result list of fixed-height rows. The selection is driven from Add's input (focus
 * never leaves it, so the caret stays where the person types), and the list keeps the selected row in
 * view. A listbox the input points into with `aria-activedescendant`.
 */

import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useRef, type ReactNode } from "react";

interface ResultListProps<T> {
  /** The listbox's DOM id; options are `${id}-<index>`. */
  readonly id: string;
  readonly label: string;
  readonly items: readonly T[];
  readonly getId: (item: T) => string;
  /** The row's accessible name. */
  readonly getText: (item: T) => string;
  readonly renderItem: (item: T) => ReactNode;
  readonly selectedId: string | null;
  readonly onActivate: (item: T) => void;
  readonly className?: string;
}

const ROW_HEIGHT = 32;
/** The list's padding above the first and below the last row. */
const LIST_PADDING = 8;

/** The DOM id of the option at `index` of the list `id` (for `aria-activedescendant`). */
export function optionId(id: string, index: number): string {
  return `${id}-${index}`;
}

export function ResultList<T>({ id, label, items, getId, getText, renderItem, selectedId, onActivate, className }: ResultListProps<T>) {
  const scroller = useRef<HTMLDivElement | null>(null);

  // TanStack Virtual hands back fresh functions each render by design; the app doesn't use the React
  // Compiler, so its "can't memoize" notice doesn't apply.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    getItemKey: (index) => {
      const item = items[index];
      return item === undefined ? index : getId(item);
    },
    paddingStart: LIST_PADDING,
    paddingEnd: LIST_PADDING,
    overscan: 8,
  });

  const selectedIndex = selectedId === null ? -1 : items.findIndex((item) => getId(item) === selectedId);
  useEffect(() => {
    if (selectedIndex >= 0) {
      virtualizer.scrollToIndex(selectedIndex, { align: "auto" });
    }
    // Only a new selection scrolls, not new rows under the same one.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedIndex]);

  return (
    <div ref={scroller} id={id} role="listbox" aria-label={label} className={className}>
      <div className="add-list-spacer" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((virtual) => {
          const item = items[virtual.index];
          if (item === undefined) {
            return null;
          }
          const selected = virtual.index === selectedIndex;
          return (
            <div
              key={virtual.key}
              id={optionId(id, virtual.index)}
              role="option"
              aria-selected={selected}
              aria-label={getText(item)}
              className={selected ? "add-list-row selected" : "add-list-row"}
              style={{ height: virtual.size, transform: `translateY(${virtual.start}px)` }}
              onMouseDown={(event) => {
                // Keep the caret in Add's input.
                event.preventDefault();
              }}
              onClick={() => {
                onActivate(item);
              }}
            >
              {renderItem(item)}
            </div>
          );
        })}
      </div>
    </div>
  );
}
