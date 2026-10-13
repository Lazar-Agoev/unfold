/**
 * Restoring the window (desktop/README.md#transport-and-windows): reading its saved state, and where
 * it goes: back where it was, unless no screen shows it any more (a display was unplugged or
 * rearranged), in which case it moves onto the primary screen. Pure functions, so they are tested
 * without Electron.
 */

import type { ShownBoard } from "../shared/unfold-api";

export interface Rect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** Where the window is: its normal (unmaximized) bounds, and whether it is maximized. */
export interface WindowPlacement {
  readonly bounds: Rect;
  readonly maximized: boolean;
}

/** The window's state as `window_state` returns it and `save_window_state` takes it. */
export interface WindowState extends WindowPlacement {
  /** The board the window showed; null for Home. */
  readonly board: ShownBoard | null;
}

/** A `window_state` result, or null when there is none or it is malformed. */
export function savedWindowState(result: unknown): WindowState | null {
  if (!isRecord(result)) {
    return null;
  }
  const bounds = rectangle(result["bounds"]);
  if (bounds === null) {
    return null;
  }
  return { bounds, maximized: result["maximized"] === true, board: shownBoard(result["board"]) };
}

/** A saved board as `{project, board}`, or null unless both are strings. */
export function shownBoard(value: unknown): ShownBoard | null {
  if (!isRecord(value)) {
    return null;
  }
  const { project, board } = value;
  return typeof project === "string" && project.length > 0 && typeof board === "string" && board.length > 0 ? { project, board } : null;
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
  /** Runs a waiting save immediately (the window closing must not lose its last move); no-op if none waits. */
  flush(): void;
  /** Drops a waiting save. */
  cancel(): void;
}

/**
 * Debounces `save` by `ms`. The window's state is saved this way (a drag fires many `move`s), and
 * flushed as the window closes so the last move, resize, or board shown before closing or quitting
 * is kept.
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
