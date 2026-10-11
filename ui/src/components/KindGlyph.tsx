/**
 * What a row on the page is: `ƒ` for a function (set in the code face), and the
 * entrance, group, drawn-link, and frame icons (a frame's own, when it has one). Accent when its row
 * is selected.
 */

import { Icon, type IconName } from "../icons";
import { cx } from "./internal/dom";
import "./KindGlyph.css";

export interface KindGlyphProps {
  readonly kind: "function" | "entrance" | "group" | "link" | "frame";
  /** A frame's own icon (`frame-api`, …), drawn instead of the plain frame's. */
  readonly icon?: IconName;
  readonly selected?: boolean;
}

export function KindGlyph({ kind, icon, selected = false }: KindGlyphProps) {
  return (
    <span className={cx("kind-glyph", selected && "kind-glyph-selected")} aria-hidden="true">
      {kind === "function" ? "ƒ" : <Icon name={icon ?? kind} />}
    </span>
  );
}
