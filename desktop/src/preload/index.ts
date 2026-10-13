/**
 * The preload script: exposes `window.unfold` and nothing else (docs/architecture.md#security).
 *
 * Engine commands travel over a MessagePort to the engine host, which the main process hands over
 * (again after every engine-host restart or reload). Invokes made before a port
 * arrives are queued; requests in flight when a connection drops are rejected with `disconnected`.
 * Window commands go to the main process over IPC.
 *
 * Edit > Undo / Redo arrive as `menu_command`s. They are handled here because they need the DOM:
 * with focus in a text field, the field's native undo runs instead, so the board's undo never
 * swallows typing undo.
 */

import { contextBridge, ipcRenderer } from "electron";
import type { EngineLost, HostToRenderer, RendererToHost, WindowReply } from "../shared/transport";
import { IPC, transportError } from "../shared/transport";
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
} from "../shared/unfold-api";

interface Pending {
  readonly resolve: (value: unknown) => void;
  readonly reject: (error: TransportError) => void;
  readonly onMessage?: (message: unknown) => void;
  /** The message, kept until it has been posted on a port. */
  unsent: RendererToHost | null;
}

let port: MessagePort | null = null;
let connection: ConnectionState = "connecting";
let nextId = 1;
const pending = new Map<number, Pending>();
const eventHandlers = new Map<string, Set<(event: UnfoldEvent) => void>>();
const connectionHandlers = new Set<(state: ConnectionState) => void>();

function setConnection(state: ConnectionState): void {
  if (state === connection) {
    return;
  }
  connection = state;
  for (const handler of [...connectionHandlers]) {
    handler(state);
  }
}

function emit(event: UnfoldEvent): void {
  for (const handler of [...(eventHandlers.get(event.type) ?? [])]) {
    handler(event);
  }
}

/** Rejects requests already sent on the current port, and forgets the port. */
function dropPort(reason: string): void {
  if (!port) {
    return;
  }
  port.onmessage = null;
  port.close();
  port = null;
  for (const [id, request] of pending) {
    if (request.unsent === null) {
      pending.delete(id);
      request.reject(transportError("disconnected", reason));
    }
  }
}

function adoptPort(next: MessagePort): void {
  dropPort("The engine connection was replaced.");
  port = next;
  next.onmessage = (event: MessageEvent<HostToRenderer>) => {
    if (port === next) {
      onHostMessage(event.data);
    }
  };
  for (const request of pending.values()) {
    if (request.unsent) {
      next.postMessage(request.unsent);
      request.unsent = null;
    }
  }
}

function onHostMessage(message: HostToRenderer): void {
  switch (message.kind) {
    case "status":
      setConnection(message.state);
      return;
    case "result": {
      const request = pending.get(message.id);
      if (request) {
        pending.delete(message.id);
        if (message.ok) {
          request.resolve(message.value);
        } else {
          request.reject(message.error);
        }
      }
      return;
    }
    case "stream":
      pending.get(message.id)?.onMessage?.(message.message);
      return;
    case "event":
      emit(message.event);
      return;
  }
}

ipcRenderer.on(IPC.enginePort, (event) => {
  const [next] = event.ports;
  if (next) {
    adoptPort(next);
  }
});

ipcRenderer.on(IPC.engineLost, (_event, lost: EngineLost) => {
  dropPort("The engine stopped while this request was running.");
  setConnection(lost.restarting ? "reconnecting" : "unavailable");
  if (!lost.restarting) {
    for (const [id, request] of pending) {
      pending.delete(id);
      request.reject(transportError("engine_unavailable", "The Unfold engine is not running."));
    }
  }
});

/** Whether keyboard focus is in a field that has its own native undo. */
function textFieldFocused(): boolean {
  const element = document.activeElement;
  if (element instanceof HTMLTextAreaElement) {
    return !element.readOnly && !element.disabled;
  }
  if (element instanceof HTMLInputElement) {
    return !element.readOnly && !element.disabled && TEXT_INPUT_TYPES.has(element.type);
  }
  return element instanceof HTMLElement && element.isContentEditable;
}

const TEXT_INPUT_TYPES = new Set(["text", "search", "url", "tel", "email", "password", "number"]);

ipcRenderer.on(IPC.shellEvent, (_event, shellEvent: ShellEvent) => {
  if (shellEvent.type === "menu_command" && textFieldFocused()) {
    // The field's own undo stack. execCommand is deprecated but remains the only way to drive it.
    // eslint-disable-next-line @typescript-eslint/no-deprecated
    document.execCommand(shellEvent.command);
    return;
  }
  emit(shellEvent);
});

/** Starts one request. Its message is posted now if a port is present, or queued until one is. */
function request(
  command: string,
  args: Readonly<Record<string, unknown>> | undefined,
  onMessage?: (message: unknown) => void,
): { id: number; result: Promise<unknown> } {
  const id = nextId++;
  const message: RendererToHost = { kind: "invoke", id, command, args: args ?? {} };
  const result = new Promise<unknown>((resolve, reject) => {
    if (connection === "unavailable" && !port) {
      // Rejections carry plain {code, message} data: Error objects lose their fields crossing the bridge.
      // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
      reject(transportError("engine_unavailable", "The Unfold engine is not running."));
      return;
    }
    // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
    const entry: Pending = { resolve, reject: (error) => { reject(error); }, unsent: message, ...(onMessage ? { onMessage } : {}) };
    pending.set(id, entry);
    if (port) {
      port.postMessage(message);
      entry.unsent = null;
    }
  });
  return { id, result };
}

function cancel(id: number): void {
  const entry = pending.get(id);
  if (!entry) {
    return;
  }
  if (entry.unsent) {
    pending.delete(id);
    entry.reject(transportError("cancelled", "Cancelled before it reached the engine."));
  } else {
    port?.postMessage({ kind: "cancel", id } satisfies RendererToHost);
  }
}

async function windowCommand<C extends WindowCommand>(
  command: C,
  ...args: WindowCommandArgs<C>
): Promise<WindowCommands[C]["result"]> {
  const reply = (await ipcRenderer.invoke(IPC.windowCommand, command, args[0] ?? {})) as WindowReply;
  if (!reply.ok) {
    // eslint-disable-next-line @typescript-eslint/only-throw-error -- plain data survives the bridge
    throw reply.error;
  }
  return reply.value as WindowCommands[C]["result"];
}

const api: UnfoldApi = {
  invoke: (command, args) => request(command, args).result,
  stream: (command, args, onMessage): StreamHandle => {
    const { id, result } = request(command, args, onMessage);
    return {
      result,
      cancel: () => {
        cancel(id);
      },
    };
  },
  on: (eventType, handler) => {
    let handlers = eventHandlers.get(eventType);
    if (!handlers) {
      handlers = new Set();
      eventHandlers.set(eventType, handlers);
    }
    handlers.add(handler);
    return () => {
      handlers.delete(handler);
    };
  },
  onConnection: (handler) => {
    connectionHandlers.add(handler);
    handler(connection);
    return () => {
      connectionHandlers.delete(handler);
    };
  },
  window: windowCommand,
};

contextBridge.exposeInMainWorld("unfold", api);
