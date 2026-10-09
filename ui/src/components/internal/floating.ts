/**
 * Positioning for floating layers (tooltips, hover cards, popovers, menus): place beside an anchor,
 * flip to the other side when there is no room, and keep inside the window.
 */

import { useLayoutEffect, useRef, type RefObject } from "react";
import type { Point } from "./dom";

export type Side = "top" | "bottom" | "left" | "right";
export type Align = "start" | "center" | "end";

export interface Placement {
  readonly side: Side;
  readonly align?: Align;
}

interface Rect {
  readonly left: number;
  readonly top: number;
  readonly width: number;
  readonly height: number;
}

const MARGIN = 6;

const OPPOSITE: Readonly<Record<Side, Side>> = { top: "bottom", bottom: "top", left: "right", right: "left" };

function along(start: number, length: number, size: number, align: Align): number {
  if (align === "start") {
    return start;
  }
  return align === "end" ? start + length - size : start + (length - size) / 2;
}

function place(anchor: Rect, size: { width: number; height: number }, side: Side, align: Align, gap: number) {
  switch (side) {
    case "top":
      return { left: along(anchor.left, anchor.width, size.width, align), top: anchor.top - size.height - gap };
    case "bottom":
      return { left: along(anchor.left, anchor.width, size.width, align), top: anchor.top + anchor.height + gap };
    case "left":
      return { left: anchor.left - size.width - gap, top: along(anchor.top, anchor.height, size.height, align) };
    case "right":
      return { left: anchor.left + anchor.width + gap, top: along(anchor.top, anchor.height, size.height, align) };
  }
}

function fits(position: { left: number; top: number }, size: { width: number; height: number }, viewport: Rect): boolean {
  return (
    position.left >= MARGIN &&
    position.top >= MARGIN &&
    position.left + size.width <= viewport.width - MARGIN &&
    position.top + size.height <= viewport.height - MARGIN
  );
}

/** Computes where a floating element of `size` goes next to `anchor` within `viewport`. */
export function computePosition(
  anchor: Rect,
  size: { width: number; height: number },
  placement: Placement,
  viewport: Rect,
  gap = 6,
): { left: number; top: number; side: Side } {
  const align = placement.align ?? "center";
  let side = placement.side;
  let position = place(anchor, size, side, align, gap);
  if (!fits(position, size, viewport)) {
    const flipped = place(anchor, size, OPPOSITE[side], align, gap);
    if (fits(flipped, size, viewport)) {
      side = OPPOSITE[side];
      position = flipped;
    }
  }
  const clamp = (value: number, max: number) => Math.max(MARGIN, Math.min(value, max - MARGIN));
  return {
    left: clamp(position.left, viewport.width - size.width),
    top: clamp(position.top, viewport.height - size.height),
    side,
  };
}

/** What a floating layer is anchored to: an element, or a point (context menus). */
export type Anchor = HTMLElement | Point | null;

function anchorRect(anchor: HTMLElement | Point): Rect {
  if (anchor instanceof HTMLElement) {
    return anchor.getBoundingClientRect();
  }
  return { left: anchor.x, top: anchor.y, width: 0, height: 0 };
}

/**
 * Keeps a floating element positioned next to `anchor` while `open`. Attach the returned ref to the
 * floating element; the hook sets its `left`/`top` (CSSOM, which the content security policy allows)
 * and `data-side`.
 */
export function useFloatingPosition<E extends HTMLElement>(
  anchor: Anchor,
  placement: Placement,
  open: boolean,
  gap?: number,
): RefObject<E | null> {
  const floating = useRef<E>(null);
  const { side, align } = placement;
  useLayoutEffect(() => {
    const element = floating.current;
    if (!open || !element || !anchor) {
      return undefined;
    }
    const update = () => {
      const position = computePosition(
        anchorRect(anchor),
        { width: element.offsetWidth, height: element.offsetHeight },
        { side, ...(align ? { align } : {}) },
        { left: 0, top: 0, width: window.innerWidth, height: window.innerHeight },
        gap,
      );
      element.style.left = `${Math.round(position.left)}px`;
      element.style.top = `${Math.round(position.top)}px`;
      element.dataset["side"] = position.side;
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, { capture: true, passive: true });
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, { capture: true });
    };
  }, [anchor, side, align, open, gap]);
  return floating;
}
