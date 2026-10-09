/** A text button, optionally with a leading icon, a spinner while busy, and a trailing shortcut. */

import type { LucideIcon } from "lucide-react";
import type { ButtonHTMLAttributes, Ref } from "react";
import { cx } from "./internal/dom";
import { Kbd } from "./Kbd";
import { Spinner } from "./Progress";
import "./Button.css";

export type ButtonVariant = "primary" | "secondary" | "quiet" | "danger";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  readonly variant?: ButtonVariant;
  readonly size?: "small" | "medium" | "large";
  readonly icon?: LucideIcon;
  /** Shows a spinner in place of the icon and marks the button busy (it stays focusable). */
  readonly busy?: boolean;
  /** A shortcut shown at the end, e.g. `Mod+O`. */
  readonly shortcut?: string;
  readonly ref?: Ref<HTMLButtonElement>;
}

/** The standard button. `type` defaults to "button" so it never submits a form by accident. */
export function Button({
  variant = "secondary",
  size = "medium",
  icon: Icon,
  busy = false,
  shortcut,
  className,
  children,
  type = "button",
  ...rest
}: ButtonProps) {
  const iconSize = size === "large" ? 16 : 14;
  return (
    <button
      {...rest}
      type={type}
      className={cx("button", `button-${variant}`, `button-${size}`, className)}
      aria-busy={busy || undefined}
    >
      {busy ? <Spinner size={iconSize} label={null} /> : Icon ? <Icon size={iconSize} aria-hidden="true" /> : null}
      {children === undefined ? null : <span className="button-label">{children}</span>}
      {shortcut ? <Kbd shortcut={shortcut} tone={variant === "primary" ? "onAccent" : "plain"} /> : null}
    </button>
  );
}
