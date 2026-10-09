/** A shortcut as plain text in the platform's style (`⇧⌘E`, `Ctrl+Shift+E`), spelled out for screen readers. */

import { shortcutAriaLabel, shortcutLabel, usePlatform } from "../../keyboard";

export function ShortcutText({ shortcut, className }: { readonly shortcut: string; readonly className: string }) {
  const platform = usePlatform();
  return (
    <kbd className={className} aria-label={shortcutAriaLabel(platform, shortcut)}>
      {shortcutLabel(platform, shortcut)}
    </kbd>
  );
}
