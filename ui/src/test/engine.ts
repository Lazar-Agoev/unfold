/**
 * A fake `window.unfold` for component tests: engine commands answer from handlers (usually the
 * golden responses in `testdata/workspace/commands/`), events can be emitted, and every call is
 * recorded.
 */

import type { ConnectionState, StreamHandle, UnfoldApi, UnfoldEvent, WindowCommand } from "@unfold/desktop/unfold-api";
import type { CommandArgs, CommandName, CommandResult, WorkspaceEvent } from "../ipc/generated";

const GOLDENS = import.meta.glob<unknown>("../../../testdata/workspace/commands/*.json", { eager: true, import: "default" });

/** One golden response of `command` (`testdata/workspace/commands/<name>.json`), as a fresh copy. */
export function fixture<C extends CommandName>(_command: C, name: string): CommandResult<C> {
  const value = GOLDENS[`../../../testdata/workspace/commands/${name}.json`];
  if (value === undefined) {
    throw new Error(`No golden response named ${name}`);
  }
  return structuredClone(value) as CommandResult<C>;
}

type Handler<C extends CommandName> = (args: CommandArgs<C>) => CommandResult<C> | Promise<CommandResult<C>>;

export type EngineHandlers = { [C in CommandName]?: Handler<C> };

/** A streamed command's handler: send messages, then resolve (or never, until cancelled). */
export type StreamHandler = (
  args: Readonly<Record<string, unknown>>,
  send: (message: unknown) => void,
  signal: AbortSignal,
) => Promise<unknown>;

export interface FakeEngine {
  /** Every engine command, in order. */
  readonly calls: { command: string; args: Readonly<Record<string, unknown>> }[];
  /** Every window command, in order. */
  readonly windowCalls: { command: string; args: unknown }[];
  /** Delivers an engine event to the UI. */
  emit: (event: WorkspaceEvent | UnfoldEvent) => void;
  setConnection: (state: ConnectionState) => void;
  callsOf: (command: string) => Readonly<Record<string, unknown>>[];
  uninstall: () => void;
}

export interface FakeEngineOptions {
  readonly handlers?: EngineHandlers;
  readonly streams?: Readonly<Record<string, StreamHandler>>;
  readonly windowHandlers?: Partial<Record<WindowCommand, (args: unknown) => unknown>>;
}

/** Installs the fake bridge as `window.unfold`. */
export function installFakeEngine({ handlers = {}, streams = {}, windowHandlers = {} }: FakeEngineOptions = {}): FakeEngine {
  const calls: FakeEngine["calls"] = [];
  const windowCalls: FakeEngine["windowCalls"] = [];
  const listeners = new Map<string, Set<(event: UnfoldEvent) => void>>();
  const connectionListeners = new Set<(state: ConnectionState) => void>();
  let connection: ConnectionState = "connected";

  const api: UnfoldApi = {
    invoke: async (command, args = {}) => {
      calls.push({ command, args });
      const handler = (handlers as Record<string, ((args: unknown) => unknown) | undefined>)[command];
      if (!handler) {
        // Rejections are plain {code, message} data, as from the real preload.
        // eslint-disable-next-line @typescript-eslint/only-throw-error
        throw { code: "not_found", message: `No fake for ${command}` };
      }
      return await handler(args);
    },
    stream: (command, args = {}, onMessage): StreamHandle => {
      calls.push({ command, args });
      const controller = new AbortController();
      const handler = streams[command];
      const result = handler
        ? handler(args, onMessage, controller.signal)
        : Promise.reject(new Error(`No fake stream for ${command}`));
      return {
        result,
        cancel: () => {
          controller.abort();
        },
      };
    },
    on: (eventType, handler) => {
      const set = listeners.get(eventType) ?? new Set();
      set.add(handler);
      listeners.set(eventType, set);
      return () => {
        set.delete(handler);
      };
    },
    onConnection: (handler) => {
      connectionListeners.add(handler);
      handler(connection);
      return () => {
        connectionListeners.delete(handler);
      };
    },
    window: (async (command: WindowCommand, args?: unknown) => {
      windowCalls.push({ command, args });
      const handler = windowHandlers[command];
      return handler ? await handler(args) : null;
    }) as UnfoldApi["window"],
    // Rejections are plain {code, message} data, as from the real preload.
    // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
    openDroppedFolder: () => Promise.reject({ code: "invalid_command", message: "No drops in tests." }),
  };

  Object.defineProperty(window, "unfold", { configurable: true, value: api });

  return {
    calls,
    windowCalls,
    emit: (event) => {
      for (const handler of [...(listeners.get(event.type) ?? [])]) {
        handler(event);
      }
    },
    setConnection: (state) => {
      connection = state;
      for (const handler of [...connectionListeners]) {
        handler(state);
      }
    },
    callsOf: (command) => calls.filter((entry) => entry.command === command).map((entry) => entry.args),
    uninstall: () => {
      Reflect.deleteProperty(window, "unfold");
    },
  };
}
