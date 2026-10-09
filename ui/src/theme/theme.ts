/**
 * Theme selection. The tokens follow the OS (`prefers-color-scheme`) unless an override is set on
 * <html>. Preferences are stored by Rust later; this module only applies them.
 */

import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "./tokens.css";
import "./base.css";

export type ThemeOverride = "light" | "dark" | null;

/** Forces a theme, or `null` to follow the OS again. */
export function applyThemeOverride(theme: ThemeOverride, root: HTMLElement = document.documentElement): void {
  if (theme === null) {
    delete root.dataset["theme"];
  } else {
    root.dataset["theme"] = theme;
  }
}
