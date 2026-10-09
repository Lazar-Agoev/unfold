/** The Compile action: available once a folder is open and the engine is connected. */

import { useEffect } from "react";
import type { CompileMode, GraphStatus } from "../ipc/generated";
import { useConnectionState } from "../ipc/hooks";
import { useEngineQuery } from "../ipc/queries";
import { cancelCompile, clearCompileProgress, followCompileProgress, startCompile, useCompileState } from "../state/compile";

export interface CompileAction {
  readonly canCompile: boolean;
  /** This window's Compile, or the graph compiling or updating for any reason. */
  readonly isCompiling: boolean;
  /** The graph's status, once known. */
  readonly status: GraphStatus | null;
  readonly compile: (mode?: CompileMode) => Promise<void>;
  readonly cancel: () => Promise<void>;
}

export function useCompile(folder: string | null): CompileAction {
  const connection = useConnectionState();
  const running = useCompileState((state) => state.running);
  const status = useEngineQuery("graph_status", {}, { enabled: folder !== null && connection === "connected" });
  const kind = status.data?.state.kind;
  const busy = kind === "compiling" || kind === "updating";

  useEffect(() => followCompileProgress(), []);
  useEffect(() => {
    if (!busy) {
      clearCompileProgress();
    }
  }, [busy]);

  const canCompile = folder !== null && connection === "connected" && running === null;
  return {
    canCompile,
    isCompiling: running !== null || busy,
    status: folder === null ? null : (status.data ?? null),
    compile: async (mode = "check") => {
      if (canCompile) {
        await startCompile(mode);
      }
    },
    cancel: cancelCompile,
  };
}
