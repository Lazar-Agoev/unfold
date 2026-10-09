/** Busy indicators: a spinner for short waits and a progress bar for work with a known size. */

import { LoaderCircle } from "lucide-react";
import { cx } from "./internal/dom";
import "./Progress.css";

export interface SpinnerProps {
  readonly size?: number;
  /** Announced to screen readers; null when the surrounding control already says it is busy. */
  readonly label?: string | null;
  readonly className?: string;
}

/** A spinning ring. It stops spinning (but stays visible) under reduced motion. */
export function Spinner({ size = 14, label = "Loading", className }: SpinnerProps) {
  const icon = <LoaderCircle size={size} className={cx("spinner", className)} aria-hidden="true" />;
  if (label === null) {
    return icon;
  }
  return (
    <span role="status" className="spinner-wrap">
      {icon}
      <span className="visually-hidden">{label}</span>
    </span>
  );
}

export interface ProgressBarProps {
  /** 0..1, or null for an unknown amount (indeterminate). */
  readonly value: number | null;
  readonly label: string;
  readonly className?: string;
}

/** A thin bar. Indeterminate bars slide; under reduced motion they show a static partial fill. */
export function ProgressBar({ value, label, className }: ProgressBarProps) {
  const percent = value === null ? null : Math.round(Math.min(1, Math.max(0, value)) * 100);
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent ?? undefined}
      className={cx("progress", percent === null && "progress-indeterminate", className)}
    >
      <div className="progress-fill" style={percent === null ? undefined : { width: `${percent}%` }} />
    </div>
  );
}
