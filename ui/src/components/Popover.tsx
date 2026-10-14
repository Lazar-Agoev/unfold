/**
 * A non-modal floating panel anchored to an element (where a type is used, a repair list). It takes
 * focus when it opens, closes on Escape or a press outside, and returns focus to where it was.
 */

import { useEffect, useRef, type ReactNode, type RefObject } from "react";
import { cx, tabbableElements } from "./internal/dom";
import { useFloatingPosition, type Align, type Side } from "./internal/floating";
import { Portal, useDismiss, useRestoreFocus } from "./internal/layers";
import "./Popover.css";

export interface PopoverProps {
  readonly open: boolean;
  readonly onClose: () => void;
  /** Usually the button that opened it (also ignored by the outside-press check). */
  readonly anchor: HTMLElement | null;
  /** The accessible name of the panel. */
  readonly label: string;
  readonly side?: Side;
  readonly align?: Align;
  /** The gap to the anchor in px. Default 4. */
  readonly offset?: number;
  /** A fixed width in px; otherwise it fits its content. */
  readonly width?: number;
  /**
   * `anchor` (default) keeps it beside its anchor while the page scrolls; `fixed` leaves it where it
   * opened.
   */
  readonly placement?: "anchor" | "fixed";
  /** What takes focus when it opens; defaults to the first focusable element. */
  readonly initialFocus?: RefObject<HTMLElement | null>;
  readonly className?: string;
  readonly children: ReactNode;
}

export function Popover({
  open,
  onClose,
  anchor,
  label,
  side = "bottom",
  align = "start",
  offset = 4,
  width,
  placement = "anchor",
  initialFocus,
  className,
  children,
}: PopoverProps) {
  const layer = useFloatingPosition<HTMLDivElement>(anchor, { side, align }, open, offset, placement === "anchor");
  const anchorRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    anchorRef.current = anchor;
  }, [anchor]);
  useDismiss(layer, open, onClose, anchorRef);
  useRestoreFocus(open);
  useEffect(() => {
    const element = layer.current;
    if (open && element) {
      (initialFocus?.current ?? tabbableElements(element)[0] ?? element).focus({ preventScroll: true });
    }
  }, [open, layer, initialFocus]);

  if (!open) {
    return null;
  }
  return (
    <Portal>
      <div
        ref={layer}
        role="dialog"
        aria-label={label}
        tabIndex={-1}
        className={cx("popover", className)}
        style={width === undefined ? undefined : { width }}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            onClose();
          }
        }}
      >
        {children}
      </div>
    </Portal>
  );
}
