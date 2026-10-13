/**
 * A thin typed client over `window.unfold`, the UI's only door to the rest of the app.
 *
 * This is the transport level: engine commands take and return `unknown` here. Command, event, and
 * payload types are generated from Rust into `src/ipc/generated/` (`cargo xtask gen-ipc`) and layered
 * on top of these functions (`./commands`). Window commands and shell events are typed by the desktop
 * shell (`@unfold/desktop/unfold-api`).
 */

import type {
  ConnectionState,
  ShellEvent,
  StreamHandle,
  TransportError,
  UnfoldApi,
  UnfoldEvent,
  WindowCommand,
  WindowCommandArgs,
  WindowCommands,
} from "@unfold/desktop/unfold-api";

export type {
  ConnectionState,
  ShellEvent,
  UnfoldEvent,
  WindowInfo,
} from "@unfold/desktop/unfold-api";

declare global {
  interface Window {
    /** Present when the UI runs inside the Unfold app (exposed by the preload script). */
    readonly unfold?: UnfoldApi;
  }
}

/** A rejected command, as a real Error with the transport's code. */
export class UnfoldError extends Error {
  readonly code: string;

  constructor(error: TransportError) {
    super(error.message);
    this.name = "UnfoldError";
    this.code = error.code;
  }
}

const NO_BRIDGE: TransportError = {
  code: "engine_unavailable",
  message: "The UI is not running inside the Unfold app.",
};

/** The bridge, or null when the UI runs outside the app (e.g. a plain browser). */
function bridge(): UnfoldApi | null {
  return window.unfold ?? null;
}

/** Converts whatever a bridge call rejected with into an {@link UnfoldError}. */
export function toUnfoldError(error: unknown): UnfoldError {
  if (error instanceof UnfoldError) {
    return error;
  }
  if (typeof error === "object" && error !== null) {
    const { code, message } = error as Partial<Record<"code" | "message", unknown>>;
    if (typeof code === "string" && typeof message === "string") {
      return new UnfoldError({ code, message });
    }
  }
  return new UnfoldError({ code: "internal", message: error instanceof Error ? error.message : String(error) });
}

/** Runs one engine command. */
export async function invoke(command: string, args?: Readonly<Record<string, unknown>>): Promise<unknown> {
  const api = bridge();
  if (!api) {
    throw new UnfoldError(NO_BRIDGE);
  }
  try {
    return await api.invoke(command, args);
  } catch (error) {
    throw toUnfoldError(error);
  }
}

/** Runs a streamed engine command; `onMessage` receives each progress message. */
export function stream(
  command: string,
  args: Readonly<Record<string, unknown>> | undefined,
  onMessage: (message: unknown) => void,
): StreamHandle {
  const api = bridge();
  if (!api) {
    return { result: Promise.reject(new UnfoldError(NO_BRIDGE)), cancel: () => undefined };
  }
  const handle = api.stream(command, args, onMessage);
  return {
    result: handle.result.catch((error: unknown) => {
      throw toUnfoldError(error);
    }),
    cancel: () => {
      handle.cancel();
    },
  };
}

/** Runs a window or dialog command in the main process. */
export async function windowCommand<C extends WindowCommand>(
  command: C,
  ...args: WindowCommandArgs<C>
): Promise<WindowCommands[C]["result"]> {
  const api = bridge();
  if (!api) {
    throw new UnfoldError(NO_BRIDGE);
  }
  try {
    return await api.window(command, ...args);
  } catch (error) {
    throw toUnfoldError(error);
  }
}

/** Subscribes to events of one type. Returns an unsubscribe function (a no-op outside the app). */
export function onEvent(eventType: string, handler: (event: UnfoldEvent) => void): () => void {
  return bridge()?.on(eventType, handler) ?? (() => undefined);
}

/** Subscribes to one shell event type (events the desktop shell sends, not Rust). */
export function onShellEvent<T extends ShellEvent["type"]>(
  type: T,
  handler: (event: Extract<ShellEvent, { readonly type: T }>) => void,
): () => void {
  return onEvent(type, (event) => {
    handler(event as unknown as Extract<ShellEvent, { readonly type: T }>);
  });
}

/** Observes the engine connection. Outside the app the state is always `unavailable`. */
export function onConnection(handler: (state: ConnectionState) => void): () => void {
  const api = bridge();
  if (!api) {
    handler("unavailable");
    return () => undefined;
  }
  return api.onConnection(handler);
}
