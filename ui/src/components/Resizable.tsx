/**
 * `ResizeHandle`: a WAI-ARIA window splitter on one edge of a pane (the sidebar's, 220–360px). Drag
 * it, use the arrow keys (Shift for bigger steps), Home/End for the minimum/maximum, and double-click
 * or Enter to reset.
 *
 * Sizes are owned by the caller (the layout store), so they can be persisted.
 */

import { useRef, type KeyboardEvent, type PointerEvent } from "react";
import { cx } from "./internal/dom";
import "./Resizable.css";

/** The pane edge the handle sits on. Dragging away from the pane grows it. */
type PaneEdge = "left" | "right" | "top" | "bottom";

interface SizeBounds {
  readonly min: number;
  readonly max: number;
  /** Double-click and Enter return here. */
  readonly initial: number;
}

function clampSize(size: number, bounds: Pick<SizeBounds, "min" | "max">): number {
  return Math.round(Math.max(bounds.min, Math.min(bounds.max, size)));
}

export interface ResizeHandleProps {
  readonly edge: PaneEdge;
  readonly size: number;
  readonly bounds: SizeBounds;
  readonly onResize: (size: number) => void;
  /** Called once when a drag ends (e.g. to persist). */
  readonly onResizeEnd?: (size: number) => void;
  /** Names the pane, e.g. "Resize sidebar". */
  readonly label: string;
  /** The id of the pane element it sizes. */
  readonly controls?: string;
  readonly className?: string;
}

const KEY_STEP = 10;
const KEY_STEP_LARGE = 50;

export function ResizeHandle({ edge, size, bounds, onResize, onResizeEnd, label, controls, className }: ResizeHandleProps) {
  const drag = useRef<{ pointer: number; start: number; size: number } | null>(null);
  const vertical = edge === "left" || edge === "right";
  // Moving the pointer right (or down) grows a pane whose handle is on its right (or bottom) edge.
  const sign = edge === "right" || edge === "bottom" ? 1 : -1;

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { pointer: event.pointerId, start: vertical ? event.clientX : event.clientY, size };
    document.body.dataset["resizing"] = vertical ? "column" : "row";
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    if (current?.pointer !== event.pointerId) {
      return;
    }
    const delta = (vertical ? event.clientX : event.clientY) - current.start;
    onResize(clampSize(current.size + sign * delta, bounds));
  };

  const endDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (drag.current?.pointer !== event.pointerId) {
      return;
    }
    drag.current = null;
    delete document.body.dataset["resizing"];
    onResizeEnd?.(size);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? KEY_STEP_LARGE : KEY_STEP;
    // Arrow keys move the splitter in their direction; what that does to the size depends on the edge.
    const moves: Readonly<Record<string, number>> = vertical
      ? { ArrowRight: step, ArrowLeft: -step }
      : { ArrowDown: step, ArrowUp: -step };
    let next: number | null = null;
    const move = moves[event.key];
    if (move !== undefined) {
      next = size + sign * move;
    } else if (event.key === "Home") {
      next = bounds.min;
    } else if (event.key === "End") {
      next = bounds.max;
    } else if (event.key === "Enter") {
      next = bounds.initial;
    }
    if (next === null) {
      return;
    }
    event.preventDefault();
    const clamped = clampSize(next, bounds);
    onResize(clamped);
    onResizeEnd?.(clamped);
  };

  return (
    <div
      role="separator"
      aria-orientation={vertical ? "vertical" : "horizontal"}
      aria-label={label}
      aria-controls={controls}
      aria-valuenow={size}
      aria-valuemin={bounds.min}
      aria-valuemax={bounds.max}
      tabIndex={0}
      className={cx("resize-handle", `resize-handle-${edge}`, className)}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onLostPointerCapture={endDrag}
      onDoubleClick={() => {
        onResize(bounds.initial);
        onResizeEnd?.(bounds.initial);
      }}
      onKeyDown={onKeyDown}
    />
  );
}
