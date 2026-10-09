/**
 * What a row on the page is (components §4.5): `ƒ` for a function (set in the code face), and the
 * entrance, group, and drawn-link icons. Accent when its row is selected.
 */

import { Icon } from "../icons";
import { cx } from "./internal/dom";
import "./KindGlyph.css";

export interface KindGlyphProps {
  readonly kind: "function" | "entrance" | "group" | "link";
  readonly selected?: boolean;
}

export function KindGlyph({ kind, selected = false }: KindGlyphProps) {
  return (
    <span className={cx("kind-glyph", selected && "kind-glyph-selected")} aria-hidden="true">
      {kind === "function" ? "ƒ" : <Icon name={kind} />}
    </span>
  );
}
