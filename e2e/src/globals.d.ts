/** What the journeys see in a window: the preload's `window.unfold`. */

import type { UnfoldApi } from "@unfold/desktop/unfold-api";

declare global {
  interface Window {
    readonly unfold: UnfoldApi;
  }
}

export {};
