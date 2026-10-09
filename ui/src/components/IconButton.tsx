/** An icon-only button with an accessible label and a tooltip that shows the label and shortcut. */

import type { LucideIcon } from "lucide-react";
import type { ButtonHTMLAttributes, Ref } from "react";
import { cx } from "./internal/dom";
import type { Side } from "./internal/floating";
import { Tooltip } from "./Tooltip";
import "./IconButton.css";

export interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label" | "children"> {
  readonly icon: LucideIcon;
  /** The accessible name, also shown as the tooltip. */
  readonly label: string;
  /** Shown in the tooltip, e.g. `Mod+B`. */
  readonly shortcut?: string;
  /** For toggles: rendered as `aria-pressed` and highlighted. */
  readonly pressed?: boolean;
  readonly size?: "small" | "medium" | "large";
  readonly tooltipSide?: Side;
  readonly ref?: Ref<HTMLButtonElement>;
}

const ICON_SIZES = { small: 14, medium: 16, large: 20 } as const;

export function IconButton({
  icon: Icon,
  label,
  shortcut,
  pressed,
  size = "medium",
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
        <Icon size={ICON_SIZES[size]} strokeWidth={size === "large" ? 1.75 : 2} aria-hidden="true" />
      </button>
    </Tooltip>
  );
}
