/**
 * The window's query cache for engine data, shared by React (through the provider the app mounts) and
 * by store code that reads through the cache, such as opening an entrance (its card model is then
 * already there when the card draws).
 */

import { QueryClient } from "@tanstack/react-query";

export const queryClient = new QueryClient({
  defaultOptions: {
    // Data comes from Rust and is invalidated by engine events, not by focus or timers.
    queries: { refetchOnWindowFocus: false, retry: false, staleTime: Infinity },
  },
});
