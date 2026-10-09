/** The platform the window runs on, which decides ⌘ versus Ctrl (architecture §11.7). */

import type { WindowInfo } from "../ipc/unfold";
import { useWindowStore } from "../state/window";

export type Platform = WindowInfo["platform"];

/** Outside the app (a plain browser during development) there is no `window_info`; ask the browser. */
function browserPlatform(): Platform {
  const name = typeof navigator === "undefined" ? "" : navigator.platform;
  if (name.startsWith("Mac")) {
    return "darwin";
  }
  return name.startsWith("Win") ? "win32" : "linux";
}

/** The platform now, for code outside React (the shortcut dispatcher). */
export function currentPlatform(): Platform {
  return useWindowStore.getState().info?.platform ?? browserPlatform();
}

/** The platform, kept current. */
export function usePlatform(): Platform {
  return useWindowStore((state) => state.info?.platform ?? null) ?? browserPlatform();
}
