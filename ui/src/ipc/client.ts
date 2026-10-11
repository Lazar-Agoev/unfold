/**
 * The window's query cache for engine data. One per window (each window is its own renderer), shared
 * by React (through the provider the app mounts) and by store code that reads through the cache, such
 * as opening an entrance (its card model is then already there when the card draws).
 */

import { QueryClient, focusManager } from "@tanstack/react-query";

export const queryClient = new QueryClient({
  defaultOptions: {
    // Data comes from Rust and is invalidated by engine events, not by focus or timers.
    queries: { refetchOnWindowFocus: false, retry: false, staleTime: Infinity },
  },
});

// Several windows share one screen, so "focused" means the window has focus rather than the page
// being visible: the few queries that refetch on focus (`workspace`, worktree metadata) ask again on window focus.
focusManager.setEventListener((setFocused) => {
  if (typeof window === "undefined") {
    return undefined;
  }
  const onFocus = () => {
    setFocused(true);
  };
  const onBlur = () => {
    setFocused(false);
  };
  window.addEventListener("focus", onFocus);
  window.addEventListener("blur", onBlur);
  return () => {
    window.removeEventListener("focus", onFocus);
    window.removeEventListener("blur", onBlur);
  };
});
