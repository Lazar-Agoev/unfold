/**
 * Add's streamed text search (R-13): one `search_text` at a time per Add box, across every connected
 * project. A new query (or option) cancels the running search; matches arrive file by file and are
 * shown in batches. The graph changing re-runs it (the engine doesn't invalidate streams).
 */

import { useEffect, useRef, useState } from "react";
import { createLatestStream, onEngineEvent, toUnfoldError, type LatestStream, type TextSearchArgs, type TextSearchFile } from "../ipc";
import type { TextResults } from "./search";

interface TextSearchState extends TextResults {
  readonly running: boolean;
  readonly error: string | null;
}

/** How often streamed files are shown while a search runs. */
const FLUSH_MS = 50;
/** How long after the last `graph_updated` or `files_changed` a search runs again. */
const RERUN_MS = 300;

const IDLE: TextSearchState = { files: [], matches: 0, truncated: false, running: false, error: null };
const RUNNING: TextSearchState = { ...IDLE, running: true };

/** A counter that changes shortly after any project's graph or files change, while `active`. */
function useChangeEpoch(active: boolean): number {
  const [epoch, setEpoch] = useState(0);
  useEffect(() => {
    if (!active) {
      return undefined;
    }
    let timer: number | undefined;
    const bump = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        setEpoch((value) => value + 1);
      }, RERUN_MS);
    };
    const offGraph = onEngineEvent("graph_updated", bump);
    const offFiles = onEngineEvent("files_changed", bump);
    return () => {
      window.clearTimeout(timer);
      offGraph();
      offFiles();
    };
  }, [active]);
  return epoch;
}

/** A search's state, tagged with the request it belongs to. */
interface Tagged extends TextSearchState {
  readonly request: string;
}

/** Runs `args` as a text search (or nothing for null) and returns what has streamed in. */
export function useTextSearch(args: TextSearchArgs | null): TextSearchState {
  const stream = useRef<LatestStream<"search_text"> | null>(null);
  const [state, setState] = useState<Tagged | null>(null);
  const signature = args === null ? null : JSON.stringify(args);
  const epoch = useChangeEpoch(signature !== null);
  const request = signature === null ? null : `${epoch}:${signature}`;

  useEffect(() => {
    stream.current ??= createLatestStream("search_text");
    const latest = stream.current;
    if (request === null || signature === null) {
      latest.cancel();
      return undefined;
    }
    let pending: TextSearchFile[] = [];
    let timer: number | undefined;
    // Each update applies only to this request's state (a newer request starts from nothing).
    const update = (change: (current: TextSearchState) => Partial<TextSearchState>) => {
      setState((current) => {
        const base: TextSearchState = current?.request === request ? current : RUNNING;
        return { ...base, ...change(base), request };
      });
    };
    const flush = () => {
      timer = undefined;
      const files = pending;
      pending = [];
      if (files.length > 0) {
        update((current) => ({
          files: [...current.files, ...files],
          matches: current.matches + files.reduce((sum, file) => sum + file.matches.length, 0),
        }));
      }
    };
    latest
      .run(JSON.parse(signature) as TextSearchArgs, (file) => {
        pending.push(file);
        timer ??= window.setTimeout(flush, FLUSH_MS);
      })
      .then(
        (summary) => {
          window.clearTimeout(timer);
          flush();
          update(() => ({ matches: summary.matches, truncated: summary.truncated, running: false }));
        },
        (cause: unknown) => {
          window.clearTimeout(timer);
          const error = toUnfoldError(cause);
          if (error.code !== "cancelled") {
            update(() => ({ running: false, error: error.message }));
          }
        },
      );
    return () => {
      window.clearTimeout(timer);
    };
  }, [request, signature]);

  useEffect(() => () => stream.current?.cancel(), []);

  if (request === null) {
    return IDLE;
  }
  return state?.request === request ? state : RUNNING;
}
