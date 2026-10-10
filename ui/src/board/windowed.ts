/**
 * On this page past 150 rows: the rows near the view, against the sidebar's scrolling region
 * (`@tanstack/react-virtual`). Shorter lists render in full, as plain flow.
 */

import { useVirtualizer, type VirtualItem } from "@tanstack/react-virtual";
import { useLayoutEffect, useRef, useState, type RefObject } from "react";

/** At or below this many rows a list renders in full. */
const WINDOW_ABOVE = 150;

interface Windowed {
  /** Attach to the list's element. */
  readonly listRef: RefObject<HTMLDivElement | null>;
  /** Null while the list renders in full; else the rows to render, placed absolutely. */
  readonly rows: readonly VirtualItem[] | null;
  /** The list's full height while windowed. */
  readonly height: number;
  /** Where a windowed row sits inside the list, in px from its top. */
  readonly offsetOf: (row: VirtualItem) => number;
}

/** Windows `count` rows of `rowHeight` px inside `scroller` once there are more than `WINDOW_ABOVE`. */
export function useWindowed(count: number, rowHeight: number, scroller: RefObject<HTMLElement | null>): Windowed {
  const listRef = useRef<HTMLDivElement | null>(null);
  const windowed = count > WINDOW_ABOVE;
  const [margin, setMargin] = useState(0);

  // The list's offset inside the scroller's content, kept current as the sections above it change size.
  useLayoutEffect(() => {
    const list = listRef.current;
    const element = scroller.current;
    if (!windowed || !list || !element) {
      return undefined;
    }
    const measure = () => {
      setMargin(list.getBoundingClientRect().top - element.getBoundingClientRect().top + element.scrollTop);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    if (element.firstElementChild) {
      observer.observe(element.firstElementChild);
    }
    return () => {
      observer.disconnect();
    };
  }, [windowed, scroller]);

  // TanStack Virtual hands back fresh functions each render by design; the app doesn't use the React
  // Compiler, so its "can't memoize" notice doesn't apply.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count,
    enabled: windowed,
    getScrollElement: () => scroller.current,
    estimateSize: () => rowHeight,
    scrollMargin: margin,
    overscan: 8,
  });

  if (!windowed) {
    return { listRef, rows: null, height: 0, offsetOf: (row) => row.start };
  }
  return { listRef, rows: virtualizer.getVirtualItems(), height: virtualizer.getTotalSize(), offsetOf: (row) => row.start - margin };
}
