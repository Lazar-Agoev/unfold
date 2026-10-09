/** A keyboard shortcut, shown in the platform's style (`⇧⌘P` on macOS, `Ctrl+Shift+P` elsewhere). */

import { shortcutAriaLabel, shortcutLabel, usePlatform } from "../keyboard";
import { cx } from "./internal/dom";
import "./Kbd.css";

export interface KbdProps {
  /** Written `Mod+Shift+P` style; see `keyboard/shortcut.ts`. */
  readonly shortcut: string;
  /** `onAccent` for use on an accent-filled button. */
  readonly tone?: "default" | "onAccent" | "plain";
  readonly className?: string;
}

/** Renders a shortcut with a spelled-out label for screen readers. */
export function Kbd({ shortcut, tone = "default", className }: KbdProps) {
  const platform = usePlatform();
  return (
    <kbd className={cx("kbd", `kbd-${tone}`, className)} aria-label={shortcutAriaLabel(platform, shortcut)}>
      {shortcutLabel(platform, shortcut)}
    </kbd>
  );
}
