/**
 * An icon-only button with an accessible label and a tooltip that shows the label
 * and shortcut. Pressed (a toggle that is on, or a menu that is open) shows the pressed fill.
 */

import type { ButtonHTMLAttributes, Ref } from "react";
import { Icon, type IconName } from "../icons";
import { cx } from "./internal/dom";
import type { Side } from "./internal/floating";
import { Tooltip } from "./Tooltip";
import "./IconButton.css";

export interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label" | "children"> {
  readonly icon: IconName;
  /** The accessible name, also shown as the tooltip. */
  readonly label: string;
  /** 28 (default), 24 in section headers, 20 in the breadcrumb; the icon is 16 in each. */
  readonly size?: 28 | 24 | 20;
  /** For toggles: rendered as `aria-pressed` and the pressed fill. */
  readonly pressed?: boolean;
  /** Shown in the tooltip, e.g. `Mod+B`. */
  readonly shortcut?: string;
  readonly tooltipSide?: Side;
  readonly ref?: Ref<HTMLButtonElement>;
}

export function IconButton({
  icon,
  label,
  size = 28,
  pressed,
  shortcut,
  tooltipSide = "bottom",
  className,
  type = "button",
  ...rest
}: IconButtonProps) {
  return (
    <Tooltip content={label} {...(shortcut ? { shortcut } : {})} side={tooltipSide} describe={false}>
      <button
        {...rest}
        type={type}
        className={cx("icon-button", `icon-button-${size}`, className)}
        aria-label={label}
        aria-pressed={pressed}
      >
        <Icon name={icon} />
      </button>
    </Tooltip>
  );
}
