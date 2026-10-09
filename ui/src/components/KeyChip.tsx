/**
 * Key chips (components §7.6, 03–04 §0): `filled` on an accent button (Add's "A"), `outlined` in hint
 * lines (the peek footer's "esc"), and `keycap` in Add's footer and command rows, one cap per key.
 */

import { shortcutAriaLabel, shortcutLabel, usePlatform, type Platform } from "../keyboard";
import { cx } from "./internal/dom";
import "./KeyChip.css";

export interface KeyChipProps {
  /** A shortcut in `Mod+Shift+E` form, shown in the platform's style. */
  readonly shortcut?: string;
  /** A literal key glyph instead (`esc`, `↵`, `>`). */
  readonly glyph?: string;
  readonly variant: "filled" | "outlined" | "keycap";
  /** Keycaps on a selected row take the selection's line color. */
  readonly selected?: boolean;
  readonly className?: string;
}

const MAC_MODIFIERS = new Set(["⌃", "⌥", "⇧", "⌘"]);

/** The keys of a shortcut label, one per cap: `⇧⌘E` → ⇧ ⌘ E; `Ctrl+Shift+E` → Ctrl Shift E. */
function keysOf(platform: Platform, label: string): string[] {
  if (platform === "darwin") {
    // macOS labels are the modifier symbols followed by the key, with no separator.
    const characters = Array.from(label);
    let modifiers = 0;
    while (modifiers < characters.length - 1 && MAC_MODIFIERS.has(characters[modifiers] ?? "")) {
      modifiers += 1;
    }
    return [...characters.slice(0, modifiers), characters.slice(modifiers).join("")];
  }
  return label.endsWith("++") ? [...label.slice(0, -2).split("+"), "+"] : label.split("+");
}

export function KeyChip({ shortcut, glyph, variant, selected = false, className }: KeyChipProps) {
  const platform = usePlatform();
  const label = shortcut === undefined ? (glyph ?? "") : shortcutLabel(platform, shortcut);
  const ariaLabel = shortcut === undefined ? undefined : shortcutAriaLabel(platform, shortcut);
  const chipClass = cx("key-chip", `key-chip-${variant}`, selected && "key-chip-selected");
  if (variant === "keycap" && shortcut !== undefined) {
    return (
      <kbd className={cx("key-caps", className)} aria-label={ariaLabel}>
        {keysOf(platform, label).map((key, index) => (
          <span key={index} className={chipClass} aria-hidden="true">
            {key}
          </span>
        ))}
      </kbd>
    );
  }
  return (
    <kbd className={cx(chipClass, glyph === ">" && "key-chip-code", className)} aria-label={ariaLabel}>
      {label}
    </kbd>
  );
}
