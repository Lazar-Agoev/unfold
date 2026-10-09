/**
 * The streamed text search behind the Search view's Text results (U-30). One search runs at a time:
 * starting another cancels the one before, and messages from a cancelled search are dropped.
 */

import { create } from "zustand";
import { callStream, type TypedStream } from "../../ipc/commands";
import type { TextSearchArgs, TextSearchFile, TextSearchSummary } from "../../ipc/generated";
import { toUnfoldError } from "../../ipc/unfold";

export interface TextSearchState {
  /** The running or finished search's arguments, as JSON; null when there is none. */
  readonly signature: string | null;
  /** Files with matches, in path order, as they stream in. */
  readonly files: readonly TextSearchFile[];
  readonly summary: TextSearchSummary | null;
  readonly running: boolean;
  readonly error: string | null;
}

const IDLE: TextSearchState = { signature: null, files: [], summary: null, running: false, error: null };

export const useTextSearch = create<TextSearchState>()(() => IDLE);

let current: TypedStream<"search_text"> | null = null;

/** Runs a text search (or clears it with null). Asking again for the running search does nothing. */
export function runTextSearch(args: TextSearchArgs | null): void {
  const signature = args === null ? null : JSON.stringify(args);
  if (signature === useTextSearch.getState().signature) {
    return;
  }
  current?.cancel();
  current = null;
  if (args === null || signature === null) {
    useTextSearch.setState(IDLE);
    return;
  }
  useTextSearch.setState({ ...IDLE, signature, running: true });
  const isCurrent = () => useTextSearch.getState().signature === signature;
  const handle = callStream("search_text", args, (file) => {
    if (isCurrent()) {
      useTextSearch.setState((state) => ({ files: [...state.files, file] }));
    }
  });
  current = handle;
  handle.result.then(
    (summary) => {
      if (isCurrent()) {
        useTextSearch.setState({ summary, running: false });
      }
    },
    (error: unknown) => {
      const failure = toUnfoldError(error);
      if (isCurrent()) {
        useTextSearch.setState({ running: false, error: failure.code === "cancelled" ? null : failure.message });
      }
    },
  );
}
