/**
 * A non-modal floating panel anchored to an element (the folder switcher's recent folders, a filter
 * menu with checkboxes). It takes focus when it opens, closes on Escape or a press outside, and returns
 * focus to where it was.
 */

import { useEffect, useRef, type ReactNode } from "react";
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
  readonly className?: string;
  readonly children: ReactNode;
}

export function Popover({ open, onClose, anchor, label, side = "bottom", align = "start", className, children }: PopoverProps) {
  const layer = useFloatingPosition<HTMLDivElement>(anchor, { side, align }, open, 4);
  const anchorRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    anchorRef.current = anchor;
  }, [anchor]);
  useDismiss(layer, open, onClose, anchorRef);
  useRestoreFocus(open);
  useEffect(() => {
    const element = layer.current;
    if (open && element) {
      (tabbableElements(element)[0] ?? element).focus({ preventScroll: true });
    }
  }, [open, layer]);

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
