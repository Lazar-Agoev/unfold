/**
 * Card models from Rust (`card`), cached with TanStack Query under the engine key
 * (`["engine", "card", {key}]`), so `graph_updated` and `files_changed` refresh exactly the cards
 * of the files that changed (ipc/queries). The model is the same on every board: a board's Flow
 * labels are applied by the UI from its own document (flow/layout), never baked in by Rust, so a
 * label the person just changed or cleared shows at once.
 *
 * When a file's source changes, the last verified Code render stays in memory to be shown dimmed
 * (architecture §10.6): it is never re-sliced and never saved.
 */

import type { CardView, CodeView } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import { UnfoldError } from "../ipc/unfold";
import { call } from "../state/boards/engine";

const lastVerified = new Map<string, CodeView>();

function remember(key: string, view: CardView): CardView {
  if (view.type === "block" && view.code?.source.state === "current") {
    lastVerified.set(key, view.code);
  }
  return view;
}

/** The query for one card's models (Code and Flow both, so switching views is instant). */
export function cardQuery(key: string) {
  return {
    queryKey: engineKey("card", { key }),
    queryFn: async () => remember(key, await call("card", { key })),
    // A missing key is an answer, not a hiccup; engine events say when to ask again.
    retry: false,
    staleTime: Infinity,
  } as const;
}

/** The last verified Code render of a block whose source has since changed, if this window saw one. */
export function lastVerifiedCode(key: string): CodeView | undefined {
  return lastVerified.get(key);
}

/** Whether a failed card query means the key is gone from the graph (the card is a placeholder). */
export function isMissing(error: unknown): boolean {
  return error instanceof UnfoldError ? error.code === "not_found" : false;
}

/** Forgets the remembered renders: the window now shows another folder. */
export function forgetVerifiedCode(): void {
  lastVerified.clear();
}
