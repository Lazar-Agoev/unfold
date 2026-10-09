/**
 * Starts and supervises the engine host (architecture §4.1, U-31). The main process hands every
 * window its own MessagePort to the engine host and then stays out of the data path. If the engine
 * host exits, it is restarted and every window is re-attached with fresh ports.
 *
 * The main process also sends the engine a few requests of its own ({@link EngineHost.request}),
 * which the engine host dispatches with `"trusted": true`: adding a project from a dialog, a drop,
 * or the command line, exporting to a path from a save dialog, and remembering windows.
 */

import type { UtilityProcess } from "electron";
import { MessageChannelMain, utilityProcess } from "electron";
import type { AppFolders, EngineLost, HostToMain, MainToHost } from "../shared/transport";
import { IPC, transportError } from "../shared/transport";
import type { TransportError } from "../shared/unfold-api";
import type { WindowEntry } from "./windows";

/** A failed main-process request, carrying the engine's `{code, message}`. */
export class EngineRequestError extends Error {
  constructor(readonly error: TransportError) {
    super(`${error.code}: ${error.message}`);
  }
}

interface PendingRequest {
  readonly resolve: (value: unknown) => void;
  readonly reject: (error: EngineRequestError) => void;
}

export interface EngineHostOptions {
  /** The built engine-host entry (`out/main/engine-host.js`). */
  readonly modulePath: string;
  readonly addonPath: string;
  readonly folders: AppFolders;
  /** Every open window, for re-attaching after a restart. */
  readonly windows: () => readonly WindowEntry[];
}

/** More exits than this within {@link RESTART_WINDOW_MS} and the main process stops restarting. */
const MAX_RESTARTS = 5;
const RESTART_WINDOW_MS = 60_000;
const SHUTDOWN_TIMEOUT_MS = 5_000;

export class EngineHost {
  private child: UtilityProcess | null = null;
  private readonly exits: number[] = [];
  private gaveUp = false;
  private stopping = false;
  private nextRequestId = 1;
  private readonly requests = new Map<number, PendingRequest>();

  constructor(private readonly options: EngineHostOptions) {}

  start(): void {
    const child = utilityProcess.fork(this.options.modulePath, [], {
      serviceName: "Unfold Engine",
      stdio: "inherit",
    });
    this.child = child;
    this.post({ type: "init", addonPath: this.options.addonPath, folders: this.options.folders });
    child.on("exit", (code) => {
      this.onExit(child, code);
    });
    child.on("message", (message: HostToMain) => {
      this.onHostMessage(message);
    });
    for (const entry of this.options.windows()) {
      this.attach(entry);
    }
  }

  /** Hands a loaded window a fresh port to the engine host, replacing any earlier one. */
  attach(entry: WindowEntry): void {
    if (!entry.loaded || entry.window.isDestroyed()) {
      return;
    }
    if (this.gaveUp) {
      this.notifyLost(entry, { restarting: false });
      return;
    }
    if (!this.child) {
      return; // Restarting: start() attaches every loaded window.
    }
    const { port1, port2 } = new MessageChannelMain();
    this.child.postMessage({ type: "attach", windowId: entry.id } satisfies MainToHost, [port1]);
    entry.window.webContents.postMessage(IPC.enginePort, null, [port2]);
  }

  detach(entry: WindowEntry): void {
    this.post({ type: "detach", windowId: entry.id });
  }

  /**
   * Asks the engine host to detach every window (landing their closing writes), close every live
   * project, and exit. Never rejects.
   */
  async shutdown(): Promise<void> {
    this.stopping = true;
    const child = this.child;
    if (!child) {
      return;
    }
    await new Promise<void>((resolve) => {
      const timer = setTimeout(() => {
        child.kill();
        resolve();
      }, SHUTDOWN_TIMEOUT_MS);
      child.once("exit", () => {
        clearTimeout(timer);
        resolve();
      });
      this.post({ type: "shutdown" });
    });
    this.child = null;
  }

  /**
   * Runs one engine command on behalf of window `windowId` (`0` for none) with `"trusted": true`.
   * Rejects with an {@link EngineRequestError}: the engine's error, `disconnected` if the engine host
   * stops first, or `engine_unavailable` while it is down.
   */
  request(windowId: number, command: string, args: Readonly<Record<string, unknown>>): Promise<unknown> {
    const child = this.child;
    if (!child) {
      const code = this.gaveUp ? "engine_unavailable" : "disconnected";
      return Promise.reject(new EngineRequestError(transportError(code, "The Unfold engine is not running.")));
    }
    const id = this.nextRequestId++;
    return new Promise((resolve, reject) => {
      this.requests.set(id, { resolve, reject });
      child.postMessage({ type: "request", id, windowId, command, args } satisfies MainToHost);
    });
  }

  private post(message: MainToHost): void {
    this.child?.postMessage(message);
  }

  private onHostMessage(message: HostToMain): void {
    const request = this.requests.get(message.id);
    this.requests.delete(message.id);
    if (message.ok) {
      request?.resolve(message.value);
    } else {
      request?.reject(new EngineRequestError(message.error));
    }
  }

  private onExit(child: UtilityProcess, code: number): void {
    if (this.child !== child) {
      return;
    }
    this.child = null;
    for (const request of this.requests.values()) {
      request.reject(new EngineRequestError(transportError("disconnected", "The engine host stopped.")));
    }
    this.requests.clear();
    if (this.stopping) {
      return;
    }
    console.error(`[main] the engine host exited with code ${code}`);
    const now = Date.now();
    this.exits.push(now);
    while (this.exits.length > 0 && (this.exits[0] ?? now) < now - RESTART_WINDOW_MS) {
      this.exits.shift();
    }
    this.gaveUp = this.exits.length > MAX_RESTARTS;
    for (const entry of this.options.windows()) {
      this.notifyLost(entry, { restarting: !this.gaveUp });
    }
    if (!this.gaveUp) {
      const delay = Math.min(250 * 2 ** (this.exits.length - 1), 4_000);
      setTimeout(() => {
        if (!this.stopping) {
          this.start();
        }
      }, delay);
    }
  }

  private notifyLost(entry: WindowEntry, payload: EngineLost): void {
    if (!entry.window.isDestroyed()) {
      entry.window.webContents.send(IPC.engineLost, payload);
    }
  }
}
