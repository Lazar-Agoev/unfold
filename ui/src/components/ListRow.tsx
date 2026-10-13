/**
 * A 28px row: a 16px leading slot, a label, a trailing slot that stays 16px wide unless it holds
 * more, and an optional menu button that shows on hover, focus, and selection. Hover is one step
 * lighter than selection.
 *
 * The row is a plain element: give it the role its list needs (`option`, `treeitem`, `menuitem`) and
 * a `tabIndex` when it takes focus; Enter on a focused row calls `onClick`.
 */

import type { HTMLAttributes, KeyboardEvent, MouseEvent, ReactNode } from "react";
import { cx } from "./internal/dom";
import "./ListRow.css";

export interface ListRowProps extends Omit<HTMLAttributes<HTMLDivElement>, "onClick" | "onDoubleClick" | "onContextMenu"> {
  readonly leading?: ReactNode;
  readonly label: ReactNode;
  readonly trailing?: ReactNode;
  readonly selected?: boolean;
  readonly onClick?: (event: MouseEvent<HTMLDivElement> | KeyboardEvent<HTMLDivElement>) => void;
  readonly onDoubleClick?: (event: MouseEvent<HTMLDivElement>) => void;
  readonly onContextMenu?: (event: MouseEvent<HTMLDivElement>) => void;
  /** A menu button (`⋯`), shown after the trailing slot on hover, focus, and selection. */
  readonly menu?: ReactNode;
}

export function ListRow({
  leading,
  label,
  trailing,
  selected = false,
  onClick,
  onDoubleClick,
  onContextMenu,
  menu,
  className,
  onKeyDown,
  ...rest
}: ListRowProps) {
  return (
    <div
      {...rest}
      className={cx("list-row", selected && "list-row-selected", className)}
      data-selected={selected || undefined}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      onContextMenu={onContextMenu}
      onKeyDown={(event) => {
        onKeyDown?.(event);
        if (!event.defaultPrevented && event.key === "Enter" && event.target === event.currentTarget && onClick) {
          event.preventDefault();
          onClick(event);
        }
      }}
    >
      <span className="list-row-leading">{leading}</span>
      <span className="list-row-label">{label}</span>
      <span className={cx("list-row-trailing", trailing !== undefined && "list-row-trailing-filled")}>{trailing}</span>
      {menu === undefined ? null : <span className="list-row-menu">{menu}</span>}
    </div>
  );
}
