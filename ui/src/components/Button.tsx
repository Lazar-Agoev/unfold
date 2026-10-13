/**
 * Buttons: `Button` in four variants, with an optional leading icon and a busy ring; and `TextButton`,
 * a link-like button inside text ("Label them", "+ Open", "Review").
 */

import type { ButtonHTMLAttributes, ReactNode, Ref } from "react";
import { Icon, type IconName } from "../icons";
import { cx } from "./internal/dom";
import { ProgressRing } from "./ProgressRing";
import "./Button.css";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  readonly variant: "primary" | "secondary" | "ghost" | "danger";
  /** `md` is 30px tall, `sm` 28px (list-row actions). Ghost buttons are always 28px. */
  readonly size?: "md" | "sm";
  readonly icon?: IconName;
  /** Shows the progress ring in place of the icon and marks the button busy (it stays focusable). */
  readonly busy?: boolean;
  readonly ref?: Ref<HTMLButtonElement>;
}

/** `type` defaults to "button" so it never submits a form by accident. */
export function Button({
  variant,
  size = "md",
  icon,
  busy = false,
  className,
  children,
  type = "button",
  ...rest
}: ButtonProps) {
  const leading = busy || icon !== undefined;
  return (
    <button
      {...rest}
      type={type}
      className={cx("button", `button-${variant}`, `button-${size}`, leading && "button-with-icon", className)}
      aria-busy={busy || undefined}
    >
      {busy ? <ProgressRing size={14} /> : icon === undefined ? null : <Icon name={icon} size={14} />}
      {children === undefined ? null : <span className="button-label">{children}</span>}
    </button>
  );
}

export interface TextButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  readonly children: ReactNode;
  /** 500 for actions ("Review", "+ Open"), 400 for quiet links. Default 500. */
  readonly weight?: 400 | 500;
  readonly tone?: "accent" | "muted";
  readonly ref?: Ref<HTMLButtonElement>;
}

/** A button that reads as a link in running text; it takes the surrounding font size. */
export function TextButton({ children, weight = 500, tone = "accent", className, type = "button", ...rest }: TextButtonProps) {
  return (
    <button {...rest} type={type} className={cx("text-button", `text-button-${tone}`, weight === 400 && "text-button-regular", className)}>
      {children}
    </button>
  );
}
