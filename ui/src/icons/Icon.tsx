/** `<Icon name="folder" />`: one glyph of the set, in `currentColor`, hidden from screen readers. */

import { GLYPHS, type IconName, type Shape } from "./glyphs";
import "./icons.css";

export interface IconProps {
  readonly name: IconName;
  /** Pixel size of the square; the drawing scales from its own viewBox. Default 16. */
  readonly size?: 16 | 14 | 12 | 10;
  readonly className?: string;
}

function ShapeElement({ shape }: { readonly shape: Shape }) {
  if ("d" in shape) {
    return <path d={shape.d} strokeDasharray={shape.dash} />;
  }
  if ("circle" in shape) {
    const [cx, cy, r] = shape.circle;
    return shape.filled ? <circle cx={cx} cy={cy} r={r} fill="currentColor" stroke="none" /> : <circle cx={cx} cy={cy} r={r} />;
  }
  const [x, y, width, height, rx] = shape.rect;
  return <rect x={x} y={y} width={width} height={height} rx={rx} strokeDasharray={shape.dash} />;
}

export function Icon({ name, size = 16, className }: IconProps) {
  const glyph = GLYPHS[name];
  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${glyph.box} ${glyph.box}`}
      fill="none"
      stroke="currentColor"
      strokeWidth={glyph.stroke ?? 1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      className={className === undefined ? "icon" : `icon ${className}`}
      data-icon={name}
    >
      {glyph.shapes.map((shape, index) => (
        <ShapeElement key={index} shape={shape} />
      ))}
    </svg>
  );
}
