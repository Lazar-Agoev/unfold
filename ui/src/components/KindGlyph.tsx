/**
 * What a row on the page is: `ƒ` for a function (set in the code face), and the
 * entrance, group, link, frame, and resource card icons (a frame's or a resource card's own, when it
 * has one). Accent when its row is selected.
 */

import { Icon, type IconName } from "../icons";
import { cx } from "./internal/dom";
import "./KindGlyph.css";

export interface KindGlyphProps {
  readonly kind: "function" | "entrance" | "group" | "link" | "frame" | "resource";
  /** A frame's or a resource card's own icon (`frame-api`, `resource-table`, …), drawn instead of the plain one. */
  readonly icon?: IconName;
  readonly selected?: boolean;
}

export function KindGlyph({ kind, icon, selected = false }: KindGlyphProps) {
  return (
    <span className={cx("kind-glyph", selected && "kind-glyph-selected")} aria-hidden="true">
      {kind === "function" ? "ƒ" : <Icon name={icon ?? (kind === "resource" ? "resource-table" : kind)} />}
    </span>
  );
}
