/**
 * Buttons: `Button` in four variants, with an optional leading icon, a
 * trailing icon (Share's chevron), a shortcut hint, and a busy ring; and `TextButton`, a link-like
 * button inside text ("Label them", "+ Open", "Review").
 */

import type { ButtonHTMLAttributes, ReactNode, Ref } from "react";
import { Icon, type IconName } from "../icons";
import { cx } from "./internal/dom";
import { ShortcutText } from "./internal/shortcut";
import { ProgressRing } from "./ProgressRing";
import "./Button.css";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  readonly variant: "primary" | "secondary" | "ghost" | "danger";
  /** `md` is 30px tall, `sm` 28px (list-row actions). Ghost buttons are always 28px. */
  readonly size?: "md" | "sm";
  readonly icon?: IconName;
  readonly trailingIcon?: IconName;
  /** A shortcut shown after the label in the platform's style, e.g. `Shift+Mod+B`. */
  readonly shortcut?: string;
  /** Shows the progress ring in place of the icon and marks the button busy (it stays focusable). */
  readonly busy?: boolean;
  readonly ref?: Ref<HTMLButtonElement>;
}

/** `type` defaults to "button" so it never submits a form by accident. */
export function Button({
  variant,
  size = "md",
  icon,
  trailingIcon,
  shortcut,
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
      className={cx(
        "button",
        `button-${variant}`,
        `button-${size}`,
        leading && "button-with-icon",
        trailingIcon !== undefined && "button-with-trailing",
        shortcut !== undefined && "button-with-shortcut",
        className,
      )}
      aria-busy={busy || undefined}
    >
      {busy ? <ProgressRing size={14} /> : icon === undefined ? null : <Icon name={icon} size={14} />}
      {children === undefined ? null : <span className="button-label">{children}</span>}
      {shortcut === undefined ? null : <ShortcutText shortcut={shortcut} className="button-shortcut" />}
      {trailingIcon === undefined ? null : <Icon name={trailingIcon} size={14} className="button-trailing" />}
    </button>
  );
}

export interface TextButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  readonly children: ReactNode;
  /** 500 for actions ("Review", "+ Open"), 400 for quiet links ("Copy diagnostics"). Default 500. */
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
