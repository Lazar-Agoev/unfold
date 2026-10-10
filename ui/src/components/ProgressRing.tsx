/**
 * The progress ring: a quarter arc over a hairline track that turns while work runs.
 * Decorative: whatever shows it also says what is running in words.
 */

import { StateGlyph } from "../icons";
import "./ProgressRing.css";

export interface ProgressRingProps {
  /** 14 in rows and toasts, 12 on Home's state line and Add's map. */
  readonly size: 14 | 12;
  /** Defaults to the size's stroke as drawn: 1.75 at 14, 2 at 12. */
  readonly strokeWidth?: number;
}

export function ProgressRing({ size, strokeWidth }: ProgressRingProps) {
  return (
    <StateGlyph
      glyph="ring"
      variant={size === 14 ? "row" : "line"}
      className="progress-ring"
      {...(strokeWidth === undefined ? {} : { strokeWidth })}
    />
  );
}
