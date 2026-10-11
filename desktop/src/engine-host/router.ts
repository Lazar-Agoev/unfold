/**
 * Routes messages between window ports and the engine binding. This is all the engine host does:
 * it holds no graph logic and knows nothing about projects (architecture §15.3 rule 5).
 *
 * - Each window has one port. `invoke` becomes `dispatch` at once, with a host-wide request id (the
 *   addon's `cancel` takes only a request id, so ids must be unique across windows). Every window
 *   shows the whole workspace, so attaching opens nothing.
 * - Sink messages go to windows: events by their `windows` list, broadcasts (every project event,
 *   and app-wide ones such as `preferences_changed`) to every attached window, and stream messages
 *   by window id and request id. The addon settles a request's promise only after its stream
 *   messages reached the sink, so a window gets them before the result.
 * - Trusted commands: the main process's own requests ({@link EngineRouter.trusted}) are dispatched
 *   with `"trusted": true`. A window can never send one: its commands are rebuilt here from `command`
 *   and `args`, and a window message naming a {@link MAIN_PROCESS_ONLY} command or carrying `trusted`
 *   is refused with `untrusted` before it reaches Rust. The requests the main process makes for
 *   agents ({@link EngineRouter.untrusted}) get exactly a window's treatment.
 * - The main process follows a few events too ({@link MAIN_EVENTS}): broadcasts of those also go to
 *   `toMain`.
 * - Closing writes outlive their window: detaching cancels the window's other requests but never a
 *   {@link CLOSING_WRITES} one. Every request is dispatched the moment it arrives, so by the time
 *   `detachWindow` is called each closing write the host has received (the window's own, and the
 *   main process's trusted ones) has reached Rust, and Rust's detach waits for them (`host.rs`).
 *   Their results have nowhere to go, so failures are logged. Detaches of one window run in order.
 */

import type { HostToRenderer } from "../shared/transport";
import { MAIN_EVENTS, transportError } from "../shared/transport";
import type { TransportError, UnfoldEvent } from "../shared/unfold-api";
import type { EngineBinding } from "./binding";

/** The engine host's end of one window's MessagePort. */
export interface WindowPort {
  post(message: HostToRenderer): void;
  onMessage(handler: (data: unknown) => void): void;
  close(): void;
}

/** Either a loaded binding, or the reason it could not be loaded. */
export type Engine = { readonly binding: EngineBinding } | { readonly unavailable: TransportError };

/** The outcome of a main-process request, as plain data. */
export type MainResult =
  | { readonly ok: true; readonly value: unknown }
  | { readonly ok: false; readonly error: TransportError };

/**
 * Commands Rust accepts only with `"trusted": true`, which only the main process may send: they take
 * absolute paths from dialogs, drops, or the command line, or manage window lifecycle. Mirrors the
 * trusted commands of `crates/unfold-workspace/src/host.rs`.
 */
export const MAIN_PROCESS_ONLY: ReadonlySet<string> = new Set([
  "export_mermaid",
  "add_project",
  "saved_windows",
  "save_window_bounds",
  "forget_window",
]);

/**
 * Commands a closing window must not lose: board and folder writes, and the window's remembered
 * state and bounds (their final saves are sent as the window closes, e.g. when the app quits). Keep
 * in sync with `CLOSING_WRITES` in `crates/unfold-workspace/src/host.rs`, whose `detach_window`
 * waits for exactly these.
 */
export const CLOSING_WRITES: ReadonlySet<string> = new Set([
  "save_board",
  "repair_board",
  "publish_board",
  "discard_changes",
  "unpublish_board",
  "delete_board",
  "duplicate_board",
  "set_board_folder",
  "create_folder",
  "rename_folder",
  "delete_folder",
  "save_board_view",
  "save_window_state",
  "save_window_bounds",
]);

interface Connection {
  readonly windowId: number;
  readonly port: WindowPort;
  /** Renderer request id -> host request id. */
  readonly inFlight: Map<number, number>;
}

interface Request {
  readonly connection: Connection;
  readonly rendererId: number;
  /** Whether the command must land even if its window closes ({@link CLOSING_WRITES}). */
  readonly closingWrite: boolean;
}

const COMMAND_NAME = /^[a-z][a-z0-9_]*$/;

export class EngineRouter {
  private readonly connections = new Map<number, Connection>();
  private readonly requests = new Map<number, Request>();
  /** The main process's streamed requests in flight: host request id -> where their stream messages go. */
  private readonly mainStreams = new Map<number, { readonly windowId: number; readonly onStream: (message: unknown) => void }>();
  /** Each window's latest detach, until it settles. */
  private readonly detaches = new Map<number, Promise<void>>();
  private nextRequestId = 1;

  constructor(
    private readonly engine: Engine,
    private readonly log: (message: string) => void = (message) => {
      console.error(`[engine-host] ${message}`);
    },
    /** Receives the broadcasts of {@link MAIN_EVENTS}, for the main process. */
    private readonly toMain: (event: UnfoldEvent) => void = () => undefined,
  ) {}

  /**
   * Attaches a window. A re-attach of the same id (a reload) replaces the port: the old page's
   * requests other than closing writes are cancelled, and Rust's state for the window (its board
   * claims and selection) is kept.
   */
  attach(windowId: number, port: WindowPort): void {
    const previous = this.connections.get(windowId);
    if (previous) {
      this.release(previous);
    }
    const connection: Connection = { windowId, port, inFlight: new Map() };
    this.connections.set(windowId, connection);
    port.onMessage((data) => {
      this.onPortMessage(connection, data);
    });
    port.post(
      "binding" in this.engine
        ? { kind: "status", state: "connected" }
        : { kind: "status", state: "unavailable", error: this.engine.unavailable },
    );
  }

  /**
   * Detaches a closed window: cancels its requests except closing writes, closes its port, then
   * has Rust release the window (`detachWindow`, which waits for those writes).
   */
  detach(windowId: number): void {
    const connection = this.connections.get(windowId);
    if (connection) {
      this.release(connection);
    }
    if (!("binding" in this.engine)) {
      return;
    }
    const binding = this.engine.binding;
    const previous = this.detaches.get(windowId) ?? Promise.resolve();
    const detached = previous
      .then(() => binding.detachWindow(windowId))
      .catch((error: unknown) => {
        this.log(`detaching window ${windowId} failed: ${errorFromRejection(error).message}`);
      });
    this.detaches.set(windowId, detached);
    void detached.then(() => {
      if (this.detaches.get(windowId) === detached) {
        this.detaches.delete(windowId);
      }
    });
  }

  /** Detaches every window, waits for every detach, then closes every live project. Never rejects. */
  async shutdown(): Promise<void> {
    for (const windowId of [...this.connections.keys()]) {
      this.detach(windowId);
    }
    await Promise.all(this.detaches.values());
    if ("binding" in this.engine) {
      try {
        await this.engine.binding.shutdown();
      } catch (error) {
        this.log(`shutting the engine down failed: ${errorFromRejection(error).message}`);
      }
    }
  }

  /** Runs a command the main process sent, with `"trusted": true`, on behalf of `windowId`. Never rejects. */
  trusted(windowId: number, command: string, args: Readonly<Record<string, unknown>>): Promise<MainResult> {
    return this.fromMain(windowId, command, args, true);
  }

  /**
   * Runs a command the main process sent for an agent, without `trusted`: a {@link MAIN_PROCESS_ONLY}
   * command, or one carrying `trusted`, is refused with `untrusted`, as a window's would be. A
   * streamed command's messages go to `onStream`, each before the result. Never rejects.
   */
  untrusted(
    windowId: number,
    command: string,
    args: Readonly<Record<string, unknown>>,
    onStream?: (message: unknown) => void,
  ): Promise<MainResult> {
    if (MAIN_PROCESS_ONLY.has(command) || Object.hasOwn(args, "trusted")) {
      return Promise.resolve({ ok: false, error: transportError("untrusted", `${command} is not available to agents.`) });
    }
    return this.fromMain(windowId, command, args, false, onStream);
  }

  private async fromMain(
    windowId: number,
    command: string,
    args: Readonly<Record<string, unknown>>,
    trusted: boolean,
    onStream?: (message: unknown) => void,
  ): Promise<MainResult> {
    if (!("binding" in this.engine)) {
      return { ok: false, error: this.engine.unavailable };
    }
    if (!COMMAND_NAME.test(command)) {
      return { ok: false, error: transportError("invalid_message", `Malformed command name: ${command}.`) };
    }
    const requestId = this.nextRequestId++;
    if (onStream) {
      this.mainStreams.set(requestId, { windowId, onStream });
    }
    let json: string;
    try {
      json = await this.engine.binding.dispatch(
        windowId,
        requestId,
        JSON.stringify(trusted ? { command, args, trusted } : { command, args }),
      );
    } catch (error) {
      return { ok: false, error: errorFromRejection(error) };
    } finally {
      this.mainStreams.delete(requestId);
    }
    try {
      return { ok: true, value: JSON.parse(json) as unknown };
    } catch {
      return { ok: false, error: transportError("internal", "The engine returned a result that is not JSON.") };
    }
  }

  /** Delivers one message from the addon's sink. */
  handleSinkMessage(raw: string): void {
    let message: unknown;
    try {
      message = JSON.parse(raw);
    } catch {
      this.log("dropped a sink message that is not JSON");
      return;
    }
    if (!isRecord(message)) {
      this.log("dropped a sink message that is not an object");
      return;
    }
    if (message["kind"] === "event") {
      const windows = message["windows"];
      const event = message["event"];
      if (!Array.isArray(windows) || !isEvent(event)) {
        this.log("dropped a malformed event message");
        return;
      }
      for (const windowId of windows) {
        if (typeof windowId === "number") {
          this.connections.get(windowId)?.port.post({ kind: "event", event });
        }
      }
      return;
    }
    if (message["kind"] === "broadcast") {
      const event = message["event"];
      if (!isEvent(event)) {
        this.log("dropped a malformed broadcast message");
        return;
      }
      for (const connection of this.connections.values()) {
        connection.port.post({ kind: "event", event });
      }
      if (MAIN_EVENTS.has(event.type)) {
        this.toMain(event);
      }
      return;
    }
    if (message["kind"] === "stream") {
      const requestId = message["requestId"];
      const fromMain = typeof requestId === "number" ? this.mainStreams.get(requestId) : undefined;
      if (fromMain && fromMain.windowId === message["windowId"]) {
        fromMain.onStream(message["message"]);
        return;
      }
      const request = typeof requestId === "number" ? this.requests.get(requestId) : undefined;
      if (!request || request.connection.windowId !== message["windowId"]) {
        return; // The request finished or its window went away; late stream messages are expected.
      }
      if (this.connections.get(request.connection.windowId) === request.connection) {
        request.connection.port.post({ kind: "stream", id: request.rendererId, message: message["message"] });
      }
      return;
    }
    this.log("dropped a sink message of unknown kind");
  }

  private onPortMessage(connection: Connection, data: unknown): void {
    if (this.connections.get(connection.windowId) !== connection) {
      return;
    }
    if (!isRecord(data) || !isRequestId(data["id"])) {
      this.log(`window ${connection.windowId} sent a message without a valid id`);
      return;
    }
    const id = data["id"];
    if (data["kind"] === "cancel") {
      this.cancel(connection, id);
      return;
    }
    const command = data["command"];
    const args = data["args"] ?? {};
    if (data["kind"] !== "invoke" || typeof command !== "string" || !COMMAND_NAME.test(command) || !isRecord(args)) {
      connection.port.post(failure(id, transportError("invalid_message", "Malformed command message.")));
      return;
    }
    if (MAIN_PROCESS_ONLY.has(command) || Object.hasOwn(data, "trusted") || Object.hasOwn(args, "trusted")) {
      connection.port.post(
        failure(id, transportError("untrusted", `${command} is accepted only from the main process.`)),
      );
      return;
    }
    if (connection.inFlight.has(id)) {
      connection.port.post(failure(id, transportError("invalid_message", `Request id ${id} is already in use.`)));
      return;
    }
    if (!("binding" in this.engine)) {
      connection.port.post(failure(id, this.engine.unavailable));
      return;
    }
    this.dispatch(this.engine.binding, connection, id, command, JSON.stringify({ command, args }));
  }

  private dispatch(
    binding: EngineBinding,
    connection: Connection,
    rendererId: number,
    name: string,
    command: string,
  ): void {
    const requestId = this.nextRequestId++;
    const closingWrite = CLOSING_WRITES.has(name);
    this.requests.set(requestId, { connection, rendererId, closingWrite });
    connection.inFlight.set(rendererId, requestId);

    const finish = (message: HostToRenderer): void => {
      this.requests.delete(requestId);
      if (connection.inFlight.get(rendererId) === requestId) {
        connection.inFlight.delete(rendererId);
      }
      if (this.connections.get(connection.windowId) === connection) {
        connection.port.post(message);
      } else if (closingWrite && message.kind === "result" && !message.ok) {
        this.log(`${name} for closed window ${connection.windowId} failed: ${message.error.message}`);
      }
    };

    // The addon settles a request only after its stream messages reached the sink, so they are
    // posted before the result.
    dispatchNow(() => binding.dispatch(connection.windowId, requestId, command)).then(
      (json) => {
        finish(parseResult(rendererId, json));
      },
      (error: unknown) => {
        finish(failure(rendererId, errorFromRejection(error)));
      },
    );
  }

  private cancel(connection: Connection, rendererId: number): void {
    const requestId = connection.inFlight.get(rendererId);
    if (requestId !== undefined && this.requests.has(requestId) && "binding" in this.engine) {
      this.engine.binding.cancel(requestId);
    }
  }

  /** Forgets a window's port: cancels its requests except closing writes, which still land. */
  private release(connection: Connection): void {
    this.connections.delete(connection.windowId);
    for (const requestId of connection.inFlight.values()) {
      if (this.requests.get(requestId)?.closingWrite) {
        continue;
      }
      this.requests.delete(requestId);
      if ("binding" in this.engine) {
        this.engine.binding.cancel(requestId);
      }
    }
    connection.inFlight.clear();
    connection.port.close();
  }
}

/** Calls `binding.dispatch` now, turning a synchronous throw into a rejection. */
function dispatchNow(dispatch: () => Promise<string>): Promise<string> {
  try {
    return dispatch();
  } catch (error) {
    return Promise.reject(error instanceof Error ? error : new Error(String(error)));
  }
}

function failure(id: number, error: TransportError): HostToRenderer {
  return { kind: "result", id, ok: false, error };
}

function parseResult(id: number, json: string): HostToRenderer {
  try {
    return { kind: "result", id, ok: true, value: JSON.parse(json) as unknown };
  } catch {
    return failure(id, transportError("internal", "The engine returned a result that is not JSON."));
  }
}

/** Turns an addon rejection (an Error whose message is JSON `{code, message}`) into a TransportError. */
export function errorFromRejection(error: unknown): TransportError {
  const text = error instanceof Error ? error.message : String(error);
  try {
    const parsed: unknown = JSON.parse(text);
    if (isRecord(parsed) && typeof parsed["code"] === "string" && typeof parsed["message"] === "string") {
      return { code: parsed["code"], message: parsed["message"] };
    }
  } catch {
    // Not JSON: fall through to an internal error that keeps the original text.
  }
  return transportError("internal", text);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isEvent(value: unknown): value is UnfoldEvent {
  return isRecord(value) && typeof value["type"] === "string";
}

function isRequestId(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}
