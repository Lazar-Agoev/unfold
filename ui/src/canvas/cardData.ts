/**
 * Card models from Rust (`card {project, key}`), cached with TanStack Query under the engine key, so
 * `graph_updated` and `files_changed` refresh exactly the cards of the files that changed in that
 * project (ipc/queries). The model is the same on every page and board: a page's Flow labels are
 * applied by the UI from its own document (flow/layout), never baked in by Rust, so a label the person
 * just changed or cleared shows at once.
 *
 * When a file's source changes, the last verified Code render stays in memory to be shown dimmed: it
 * is never re-sliced and never saved. It is kept per project and key, and forgotten with its project.
 */

import type { CardView, CodeView, Key, ProjectId } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import { UnfoldError } from "../ipc/unfold";
import { call } from "../state/boards/engine";
import { cardRefKey } from "../state/boards/keys";

const lastVerified = new Map<string, CodeView>();

function remember(project: ProjectId, key: Key, view: CardView): CardView {
  if (view.type === "block" && view.code?.source.state === "current") {
    lastVerified.set(cardRefKey(project, key), view.code);
  }
  return view;
}

/** The query for one card's models (Code and Flow both, so switching views is instant). */
export function cardQuery(project: ProjectId, key: Key) {
  return {
    queryKey: engineKey("card", { project, key }),
    queryFn: async () => remember(project, key, await call("card", { project, key })),
    // A missing key is an answer, not a hiccup; engine events say when to ask again.
    retry: false,
    staleTime: Infinity,
  } as const;
}

/** The last verified Code render of a block whose source has since changed, if this window saw one. */
export function lastVerifiedCode(project: ProjectId, key: Key): CodeView | undefined {
  return lastVerified.get(cardRefKey(project, key));
}

/** Whether a failed card query means the key is gone from the graph (the card is a placeholder). */
export function isMissing(error: unknown): boolean {
  return error instanceof UnfoldError ? error.code === "not_found" : false;
}

/** Forgets a project's remembered renders: its folder changed or it left the workspace (`project_reset`). */
export function forgetVerifiedCode(project: ProjectId): void {
  const prefix = cardRefKey(project, "");
  for (const key of [...lastVerified.keys()]) {
    if (key.startsWith(prefix)) {
      lastVerified.delete(key);
    }
  }
}
