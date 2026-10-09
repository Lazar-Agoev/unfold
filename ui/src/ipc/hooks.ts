/** React hooks over the transport client. */

import { useSyncExternalStore } from "react";
import type { ConnectionState } from "./unfold";
import { onConnection } from "./unfold";

let connectionState: ConnectionState = "connecting";
const connectionListeners = new Set<() => void>();
let unsubscribeConnection: (() => void) | null = null;

function subscribeConnection(listener: () => void): () => void {
  connectionListeners.add(listener);
  unsubscribeConnection ??= onConnection((state) => {
    connectionState = state;
    for (const notify of connectionListeners) {
      notify();
    }
  });
  return () => {
    connectionListeners.delete(listener);
  };
}

/** The window's engine connection state, kept current. */
export function useConnectionState(): ConnectionState {
  return useSyncExternalStore(subscribeConnection, () => connectionState);
}

