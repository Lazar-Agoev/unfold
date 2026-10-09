/**
 * Routes messages between window ports and the engine binding. This is all the engine host does:
 * it holds no graph logic (architecture §15.3 rule 5).
 *
 * - Each window has one port. `invoke` becomes `dispatch` with a host-wide request id (the addon's
 *   `cancel` takes only a request id, so ids must be unique across windows).
 * - Sink messages go to windows: events by their `windows` list, broadcasts (app-wide events such
 *   as `preferences_changed`) to every attached window, including windows that show no folder (Rust
 *   cannot list those: they never open a workspace), and stream messages by window id and request id.
 * - Attaching a window with a folder calls `openWorkspace`; detaching calls `closeWorkspace`.
 *   Lifecycle calls for one window run in order, and its commands wait until its workspace is open.
 * - Trusted commands: the main process's own requests ({@link EngineRouter.trusted}) are dispatched
 *   with `"trusted": true`. A window can never send one: its commands are rebuilt here from `command`
 *   and `args`, and a window message naming a main-process-only command or carrying `trusted` is
 *   refused with `untrusted` before it reaches Rust.
 * - Closing writes outlive their window: detaching never cancels a {@link CLOSING_WRITES} request
 *   (the window's own, or the main process's trusted bounds save), a queued one is still dispatched
 *   once the workspace is open, and `closeWorkspace` runs only after
 *   every such request has reached `dispatch`, so Rust's close waits for it (`host.rs`). Their
 *   results have nowhere to go, so failures are logged.
 */

import type { HostToRenderer } from "../shared/transport";
import { transportError } from "../shared/transport";
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

/** The outcome of a trusted request, as plain data. */
export type TrustedResult =
  | { readonly ok: true; readonly value: unknown }
  | { readonly ok: false; readonly error: TransportError };

/** Commands Rust accepts only with `"trusted": true`, which only the main process may send. */
export const MAIN_PROCESS_ONLY: ReadonlySet<string> = new Set(["export_mermaid", "folder_windows"]);

/**
 * Commands a closing window must not lose: board writes and the window's remembered state and
 * bounds (their final saves are sent as the window closes, e.g. when the app quits). Keep in sync with
 * `CLOSING_WRITES` in `crates/unfold-workspace/src/host.rs`, whose `close_workspace` waits for
 * exactly these.
 */
export const CLOSING_WRITES: ReadonlySet<string> = new Set([
  "save_board",
  "repair_board",
  "publish_board",
  "unpublish_board",
  "rename_board",
  "delete_board",
  "save_window_state",
  "save_window_bounds",
]);

/** What the router tells the main process about. */
export interface RouterEvents {
  /** A window's attach settled: its workspace opened (`error` null), failed, or it shows no folder. */
  attached?(windowId: number, folder: string | null, error: TransportError | null): void;
}

interface Connection {
  readonly windowId: number;
  readonly folder: string | null;
  readonly port: WindowPort;
  /** Settles (never rejects) once this window's workspace is open, or failed to open. */
  readonly workspaceReady: Promise<void>;
  /** Settles like `workspaceReady`, with why the workspace failed to open (null when it opened). */
  readonly opened: Promise<TransportError | null>;
  /** Renderer request id -> host request id. */
  readonly inFlight: Map<number, number>;
  /** The `sent` of each main-process (trusted) closing write for this window not yet dispatched. */
  readonly trustedWrites: Set<Promise<void>>;
}

interface Request {
  readonly connection: Connection;
  readonly rendererId: number;
  /** Whether the command must land even if its window closes ({@link CLOSING_WRITES}). */
  readonly closingWrite: boolean;
  /** Settles once the request reached `binding.dispatch`, or was dropped before it could. */
  readonly sent: Promise<void>;
  dispatched: boolean;
  cancelRequested: boolean;
}

const COMMAND_NAME = /^[a-z][a-z0-9_]*$/;

export class EngineRouter {
  private readonly connections = new Map<number, Connection>();
  private readonly requests = new Map<number, Request>();
  private readonly lifecycle = new Map<number, Promise<void>>();
  private nextRequestId = 1;

  constructor(
    private readonly engine: Engine,
    private readonly log: (message: string) => void = (message) => {
      console.error(`[engine-host] ${message}`);
    },
    private readonly events: RouterEvents = {},
  ) {}

  /** Attaches a window, replacing any earlier connection for the same window id. */
  attach(windowId: number, folder: string | null, port: WindowPort): void {
    const previous = this.connections.get(windowId);
    // A reload re-attaches with the same folder: keep the workspace and swap the port.
    const reusedWorkspace = previous?.folder === folder ? previous.opened : null;
    if (previous) {
      this.release(previous, { closeWorkspace: reusedWorkspace === null });
    }

    let opened: Promise<TransportError | null>;
    if (reusedWorkspace) {
      opened = reusedWorkspace;
    } else if ("binding" in this.engine && folder !== null) {
      const binding = this.engine.binding;
      opened = this.serialize(windowId, () => binding.openWorkspace(windowId, folder)).then(
        () => null,
        (error: unknown) => {
          const reason = errorFromRejection(error);
          this.postIfCurrent(windowId, port, {
            kind: "event",
            event: { type: "workspace_open_failed", folder, error: reason },
          });
          return reason;
        },
      );
    } else {
      opened = Promise.resolve("binding" in this.engine ? null : this.engine.unavailable);
    }

    const workspaceReady = opened.then(() => undefined);
    const connection: Connection = {
      windowId,
      folder,
      port,
      workspaceReady,
      opened,
      inFlight: new Map(),
      trustedWrites: new Set(),
    };
    this.connections.set(windowId, connection);
    void opened.then((error) => {
      if (this.connections.get(windowId) === connection) {
        this.events.attached?.(windowId, folder, error);
      }
    });
    port.onMessage((data) => {
      this.onPortMessage(connection, data);
    });
    port.post(
      "binding" in this.engine
        ? { kind: "status", state: "connected" }
        : { kind: "status", state: "unavailable", error: this.engine.unavailable },
    );
  }

  /** Detaches a closed window: cancels its requests (except closing writes) and closes its workspace. */
  detach(windowId: number): void {
    const connection = this.connections.get(windowId);
    if (connection) {
      this.release(connection, { closeWorkspace: true });
    }
  }

  /** Detaches every window and resolves once every workspace has closed. */
  async shutdown(): Promise<void> {
    for (const windowId of [...this.connections.keys()]) {
      this.detach(windowId);
    }
    await Promise.allSettled([...this.lifecycle.values()]);
  }

  /**
   * Runs a command the main process sent, with `"trusted": true`, on behalf of `windowId` (after
   * that window's workspace is open, if it is attached). Never rejects.
   */
  async trusted(windowId: number, command: string, args: Readonly<Record<string, unknown>>): Promise<TrustedResult> {
    if (!("binding" in this.engine)) {
      return { ok: false, error: this.engine.unavailable };
    }
    if (!COMMAND_NAME.test(command)) {
      return { ok: false, error: transportError("invalid_message", `Malformed command name: ${command}.`) };
    }
    const binding = this.engine.binding;
    const connection = this.connections.get(windowId);
    let markSent = (): void => undefined;
    if (connection && CLOSING_WRITES.has(command)) {
      // Like a window's own closing write: the window's detach waits until this reaches Rust.
      const sent = new Promise<void>((resolve) => {
        markSent = resolve;
      });
      connection.trustedWrites.add(sent);
      void sent.then(() => connection.trustedWrites.delete(sent));
    }
    await connection?.workspaceReady;
    let json: string;
    try {
      const result = binding.dispatch(windowId, this.nextRequestId++, JSON.stringify({ command, args, trusted: true }));
      markSent();
      json = await result;
    } catch (error) {
      markSent();
      return { ok: false, error: errorFromRejection(error) };
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
      return;
    }
    if (message["kind"] === "stream") {
      const requestId = message["requestId"];
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
    let markSent!: () => void;
    const sent = new Promise<void>((resolve) => {
      markSent = resolve;
    });
    const closingWrite = CLOSING_WRITES.has(name);
    const request: Request = { connection, rendererId, closingWrite, sent, dispatched: false, cancelRequested: false };
    this.requests.set(requestId, request);
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

    void connection.workspaceReady
      .then(() => {
        if (!this.requests.has(requestId)) {
          markSent();
          return undefined; // Released while waiting for the workspace.
        }
        const result = binding.dispatch(connection.windowId, requestId, command);
        request.dispatched = true;
        markSent();
        if (request.cancelRequested) {
          binding.cancel(requestId);
        }
        return result;
      })
      .then(
        (json) => {
          if (json !== undefined) {
            finish(parseResult(rendererId, json));
          }
        },
        (error: unknown) => {
          finish(failure(rendererId, errorFromRejection(error)));
        },
      );
  }

  private cancel(connection: Connection, rendererId: number): void {
    const requestId = connection.inFlight.get(rendererId);
    const request = requestId === undefined ? undefined : this.requests.get(requestId);
    if (requestId === undefined || !request || !("binding" in this.engine)) {
      return;
    }
    if (request.dispatched) {
      this.engine.binding.cancel(requestId);
    } else {
      request.cancelRequested = true;
    }
  }

  private release(connection: Connection, options: { closeWorkspace: boolean }): void {
    this.connections.delete(connection.windowId);
    const closingWrites: Promise<void>[] = [];
    for (const requestId of connection.inFlight.values()) {
      const request = this.requests.get(requestId);
      if (request?.closingWrite) {
        closingWrites.push(request.sent); // Kept: it still runs, and the close below waits for it.
        continue;
      }
      if (request?.dispatched && "binding" in this.engine) {
        this.engine.binding.cancel(requestId);
      }
      this.requests.delete(requestId);
    }
    connection.inFlight.clear();
    closingWrites.push(...connection.trustedWrites);
    connection.port.close();

    if (options.closeWorkspace && connection.folder !== null && "binding" in this.engine) {
      const binding = this.engine.binding;
      const windowId = connection.windowId;
      const close = async (): Promise<void> => {
        await Promise.all(closingWrites);
        await binding.closeWorkspace(windowId);
      };
      void this.serialize(windowId, close).catch((error: unknown) => {
        this.log(`closing the workspace of window ${windowId} failed: ${errorFromRejection(error).message}`);
      });
    }
  }

  /** Runs one lifecycle step for a window after the previous one settles. */
  private serialize<T>(windowId: number, step: () => Promise<T>): Promise<T> {
    const previous = this.lifecycle.get(windowId) ?? Promise.resolve();
    const next = previous.then(step, step);
    const settled = next.then(
      () => undefined,
      () => undefined,
    );
    this.lifecycle.set(windowId, settled);
    void settled.then(() => {
      if (this.lifecycle.get(windowId) === settled) {
        this.lifecycle.delete(windowId);
      }
    });
    return next;
  }

  private postIfCurrent(windowId: number, port: WindowPort, message: HostToRenderer): void {
    if (this.connections.get(windowId)?.port === port) {
      port.post(message);
    }
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
