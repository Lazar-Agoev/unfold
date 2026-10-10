/**
 * Long lists inside a scrolling area: Home's main area and the sidebar's projects list each
 * scroll as one; a long list inside renders only the rows near the view (`@tanstack/react-virtual`
 * against that scroller). Short lists render in full, as plain flow.
 */

import { useVirtualizer, type VirtualItem } from "@tanstack/react-virtual";
import { createContext, useContext, useLayoutEffect, useState } from "react";

/** The element that scrolls around a windowed list (Home's main area). */
export const ScrollArea = createContext<HTMLElement | null>(null);

export interface WindowedOptions {
  readonly count: number;
  /** Each row's height in px. */
  readonly size: (index: number) => number;
  /** At or below this many rows the list renders in full. */
  readonly threshold: number;
  /** For a list with its own scroller (the sidebar, an expanded application); else the `ScrollArea`. */
  readonly scroller?: HTMLElement | null;
}

export interface Windowed {
  /** Pass as the `ref` of the list's element (whose children are the rows). */
  readonly attach: (element: HTMLElement | null) => void;
  /** Null while the list renders in full; else the rows to render, placed absolutely. */
  readonly rows: readonly VirtualItem[] | null;
  /** The list's full height while windowed. */
  readonly height: number;
  /** The rendered rows' positions, start inclusive, end exclusive. */
  readonly range: readonly [number, number];
  /** Where a windowed row sits inside the list, in px from its top. */
  readonly offsets: readonly number[];
}

export function useWindowed({ count, size, threshold, scroller }: WindowedOptions): Windowed {
  const area = useContext(ScrollArea);
  const scrollElement = scroller === undefined ? area : scroller;
  const [list, setList] = useState<HTMLElement | null>(null);
  const windowed = count > threshold;
  const [margin, setMargin] = useState(0);

  // The list's offset inside the scroller's content, kept current as things above it change size.
  useLayoutEffect(() => {
    if (!windowed || !list || !scrollElement) {
      return undefined;
    }
    const measure = () => {
      setMargin(list.getBoundingClientRect().top - scrollElement.getBoundingClientRect().top + scrollElement.scrollTop);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(scrollElement);
    if (scrollElement.firstElementChild) {
      observer.observe(scrollElement.firstElementChild);
    }
    return () => {
      observer.disconnect();
    };
  }, [windowed, list, scrollElement]);

  // TanStack Virtual hands back fresh functions each render by design; the app doesn't use the React
  // Compiler, so its "can't memoize" notice doesn't apply.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count,
    enabled: windowed,
    getScrollElement: () => scrollElement,
    estimateSize: size,
    scrollMargin: margin,
    overscan: 6,
  });

  if (!windowed) {
    return { attach: setList, rows: null, height: 0, range: [0, count], offsets: [] };
  }
  const rows = virtualizer.getVirtualItems();
  const first = rows[0]?.index ?? 0;
  const last = rows.at(-1)?.index ?? -1;
  return {
    attach: setList,
    rows,
    height: virtualizer.getTotalSize(),
    range: [first, last + 1],
    offsets: rows.map((row) => row.start - margin),
  };
}
