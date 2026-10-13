/**
 * The project-state glyphs of 07 (verbatim paths): the progress ring, the hollow circle of a project
 * that isn't compiled, the failed triangle, and the resting moon. Each comes in the sidebar row's size
 * and the Home state line's size. The state dots are plain boxes, drawn by `StateIndicator` itself.
 */

import "./icons.css";

type StateGlyphName = "ring" | "not-compiled" | "failed" | "resting";

export interface StateGlyphProps {
  readonly glyph: StateGlyphName;
  /** `row`: the sidebar's 16px state slot; `line`: Home's 12px state-line slot. */
  readonly variant: "row" | "line";
  /** The ring's stroke, when a place draws it thinner or thicker than its variant. */
  readonly strokeWidth?: number;
  readonly className?: string;
}

const RING_TRACK = { cx: 7, cy: 7, r: 5.25 };
const RING_ARC = "M7 1.75A5.25 5.25 0 0 1 12.25 7";
const TRIANGLE = "M6.13 2.0C6.52 1.33 7.48 1.33 7.87 2.0L12.6 10.5C12.98 11.17 12.5 12 11.73 12H2.27C1.5 12 1.02 11.17 1.4 10.5L6.13 2.0Z";
const MOON = "M11.5 8.6A4.75 4.75 0 1 1 5.4 2.5A3.75 3.75 0 0 0 11.5 8.6Z";

export function StateGlyph({ glyph, variant, strokeWidth, className }: StateGlyphProps) {
  const name = className === undefined ? `state-glyph state-glyph-${glyph}` : `state-glyph state-glyph-${glyph} ${className}`;
  const common = { "aria-hidden": true, focusable: false, className: name, "data-variant": variant } as const;
  switch (glyph) {
    case "ring": {
      const size = variant === "row" ? 14 : 12;
      const stroke = strokeWidth ?? (variant === "row" ? 1.75 : 2);
      return (
        <svg {...common} width={size} height={size} viewBox="0 0 14 14" fill="none" strokeWidth={stroke}>
          <circle {...RING_TRACK} className="state-glyph-track" />
          <path d={RING_ARC} strokeLinecap="round" className="state-glyph-arc" />
        </svg>
      );
    }
    case "not-compiled":
      return (
        <svg {...common} width={12} height={12} viewBox="0 0 12 12" fill="none" strokeWidth={1.5}>
          <circle cx={6} cy={6} r={4.25} />
        </svg>
      );
    case "failed": {
      const size = variant === "row" ? 14 : 12;
      return (
        <svg {...common} width={size} height={size} viewBox="0 0 14 14">
          <path d={TRIANGLE} className="state-glyph-fill" />
          <path d="M7 5V7.75" strokeWidth={1.4} strokeLinecap="round" className="state-glyph-mark" />
          <circle cx={7} cy={9.75} r={0.8} className="state-glyph-dot" />
        </svg>
      );
    }
    case "resting": {
      const size = variant === "row" ? 14 : 12;
      return (
        <svg {...common} width={size} height={size} viewBox="0 0 14 14" fill="none" strokeWidth={variant === "row" ? 1.5 : 1.6} strokeLinejoin="round">
          <path d={MOON} />
        </svg>
      );
    }
  }
}
