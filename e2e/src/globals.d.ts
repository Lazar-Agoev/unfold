/** What the journeys see in a window: the preload's `window.unfold`, and the event recorder. */

import type { UnfoldApi, UnfoldEvent } from "@unfold/desktop/unfold-api";

declare global {
  interface Window {
    readonly unfold: UnfoldApi;
    __e2e?: { readonly events: { readonly at: number; readonly event: UnfoldEvent }[] };
  }
}

export {};
