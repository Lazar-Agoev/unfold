/**
 * A project's monogram: a neutral rounded square with the name's initial, no
 * hue per project. The workspace's mark is the one dark monogram.
 */

import { cx } from "./internal/dom";
import "./Monogram.css";

export interface MonogramProps {
  readonly name: string;
  /** 14 in the board's header and Selected's rows, 20 in project rows and the peek sheet's header. */
  readonly size: 14 | 20;
  /** `workspace` for the workspace row. */
  readonly tone?: "default" | "workspace";
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
