/**
 * A project's monogram (components §3, V-24): a neutral rounded square with the name's initial, no
 * hue per project. The workspace's mark is the one dark monogram.
 */

import { cx } from "./internal/dom";
import "./Monogram.css";

export interface MonogramProps {
  readonly name: string;
  /** 14 in tabs and On this page rows, 20 in project rows, card headers, toasts, and Add. */
  readonly size: 14 | 20;
  /** `selected` on a selected Add result row; `workspace` for the workspace row. */
  readonly tone?: "default" | "selected" | "workspace";
  readonly className?: string;
}

/** The name's first character, uppercased. */
function initial(name: string): string {
  return (Array.from(name.trim())[0] ?? "").toUpperCase();
}

export function Monogram({ name, size, tone = "default", className }: MonogramProps) {
  return (
    <span className={cx("monogram", `monogram-${size}`, `monogram-${tone}`, className)} aria-hidden="true">
      {initial(name)}
    </span>
  );
}
