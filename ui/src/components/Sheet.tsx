/**
 * A sheet that slides in from the right over the canvas area. Unpinned it floats over the
 * content; pinned it takes its place in the row, so the content beside it narrows. It is not modal:
 * the rest of the window stays usable, and Escape inside it closes it.
 *
 * Mount it as the last child of `#content-area`, which is a positioned flex row.
 */

import type { HTMLAttributes, ReactNode } from "react";
import { cx } from "./internal/dom";
import "./Sheet.css";

export interface SheetProps extends Omit<HTMLAttributes<HTMLElement>, "children"> {
  readonly open: boolean;
  /** The sheet's accessible name, e.g. the file it shows. */
  readonly label: string;
  /** Width in px. Default 640. */
  readonly width?: number;
  readonly pinned?: boolean;
  readonly onClose: () => void;
  readonly children: ReactNode;
}

export function Sheet({ open, label, width, pinned = false, onClose, className, children, onKeyDown, ...rest }: SheetProps) {
  if (!open) {
    return null;
  }
  return (
    <aside
      {...rest}
      aria-label={label}
      className={cx("sheet", pinned && "sheet-pinned", className)}
      style={width === undefined ? undefined : { width }}
      onKeyDown={(event) => {
        onKeyDown?.(event);
        if (!event.defaultPrevented && event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          onClose();
        }
      }}
    >
      {children}
    </aside>
  );
}
