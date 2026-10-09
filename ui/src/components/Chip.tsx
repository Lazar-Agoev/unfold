/** Small labels: a Chip for states (Draft, Outdated) and a Badge for counts. */

import type { ReactNode } from "react";
import { cx } from "./internal/dom";
import "./Chip.css";

export type ChipTone = "neutral" | "accent" | "authored" | "warn" | "danger";

export interface ChipProps {
  readonly children: ReactNode;
  readonly tone?: ChipTone;
  readonly title?: string;
  readonly className?: string;
}

/** A pill-shaped state label, e.g. the board title's Draft chip. */
export function Chip({ children, tone = "neutral", title, className }: ChipProps) {
  return (
    <span className={cx("chip", `chip-${tone}`, className)} title={title}>
      {children}
    </span>
  );
}

export interface BadgeProps {
  readonly count: number;
  /** Counts above this show as `max+`. */
  readonly max?: number;
  readonly tone?: ChipTone;
  /** Spoken form, e.g. "2 problems"; defaults to the number. */
  readonly label?: string;
  readonly className?: string;
}

/** A count, e.g. the Problems tab's `2`. */
export function Badge({ count, max = 999, tone = "neutral", label, className }: BadgeProps) {
  const text = count > max ? `${max}+` : String(count);
  return (
    <span className={cx("badge", `chip-${tone}`, className)} aria-label={label}>
      {text}
    </span>
  );
}
