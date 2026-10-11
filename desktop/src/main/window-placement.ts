/**
 * Restoring windows (desktop/README.md#transport-and-windows): reading the saved windows, and where each one goes:
 * back where it was, unless no screen shows it any more (a display was unplugged or rearranged), in
 * which case it moves onto the primary screen. Pure functions, so they are tested without Electron.
 */

export interface Rect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** How a remembered window comes back. */
export interface RestoredWindow {
  readonly key: string;
  readonly bounds: Rect | null;
  readonly maximized: boolean;
}

/** The windows of a `saved_windows` result, skipping malformed entries. */
export function savedWindowList(result: unknown): RestoredWindow[] {
  if (!isRecord(result) || !Array.isArray(result["windows"])) {
    return [];
  }
  return result["windows"].flatMap((window: unknown): RestoredWindow[] => {
    if (!isRecord(window) || typeof window["key"] !== "string" || window["key"].length === 0) {
      return [];
    }
    return [{ key: window["key"], bounds: rectangle(window["bounds"]), maximized: window["maximized"] === true }];
  });
}

/** Saved `{x, y, width, height}` as whole pixels, or null unless it is a nonempty rectangle. */
function rectangle(value: unknown): Rect | null {
  if (!isRecord(value)) {
    return null;
  }
  const { x, y, width, height } = value;
  if (![x, y, width, height].every((n) => typeof n === "number" && Number.isFinite(n))) {
    return null;
  }
  const rect = {
    x: Math.round(x as number),
    y: Math.round(y as number),
    width: Math.round(width as number),
    height: Math.round(height as number),
  };
  return rect.width > 0 && rect.height > 0 ? rect : null;
}

/** A window counts as visible when this much of its title bar is on some screen. */
const TITLE_BAR = { width: 120, height: 32 } as const;

/**
 * The bounds to restore a window at. `workAreas` are the screens' usable areas, the primary screen
 * first. Bounds already reachable stay as they are; otherwise the window keeps its size (shrunk to
 * fit) and is centered on the primary screen.
 */
export function placeOnScreen(bounds: Rect, workAreas: readonly Rect[]): Rect {
  const [primary] = workAreas;
  if (!primary || workAreas.some((area) => titleBarVisible(bounds, area))) {
    return bounds;
  }
  const width = Math.min(bounds.width, primary.width);
  const height = Math.min(bounds.height, primary.height);
  return {
    x: primary.x + Math.round((primary.width - width) / 2),
    y: primary.y + Math.round((primary.height - height) / 2),
    width,
    height,
  };
}

/** A save that waits for things to settle, but can be made to happen now. */
export interface DebouncedSave {
  /** (Re)starts the wait; the save runs once nothing changed for the debounce period. */
  schedule(): void;
  /** Runs a waiting save immediately (a window closing must not lose its last move); no-op if none waits. */
  flush(): void;
  /** Drops a waiting save. */
  cancel(): void;
}

/**
 * Debounces `save` by `ms`. A window's placement is saved this way (a drag fires many `move`s), and
 * flushed as the window closes so the last move or resize before closing or quitting is kept.
 */
export function debouncedSave(save: () => void, ms: number): DebouncedSave {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const cancel = (): void => {
    clearTimeout(timer);
    timer = undefined;
  };
  return {
    schedule: () => {
      cancel();
      timer = setTimeout(() => {
        timer = undefined;
        save();
      }, ms);
    },
    flush: () => {
      if (timer !== undefined) {
        cancel();
        save();
      }
    },
    cancel,
  };
}

function titleBarVisible(bounds: Rect, area: Rect): boolean {
  const left = Math.max(bounds.x, area.x);
  const right = Math.min(bounds.x + bounds.width, area.x + area.width);
  const top = Math.max(bounds.y, area.y);
  const bottom = Math.min(bounds.y + TITLE_BAR.height, area.y + area.height);
  return right - left >= TITLE_BAR.width && bottom - top >= TITLE_BAR.height;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
