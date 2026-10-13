/**
 * The engine host: an Electron utility process that loads the Rust addon and routes each window's
 * port to it (docs/architecture.md#processes, process hardening). The main process sends lifecycle messages, plus its own
 * trusted requests (adding a project, export to a dialog path, the window's state) and the MCP
 * server's untrusted ones, answered over the parent port.
 */

import type { MessagePortMain } from "electron";
import type { AppFolders, HostToMain, HostToRenderer, MainToHost } from "../shared/transport";
import { transportError } from "../shared/transport";
import { loadEngineBinding } from "./binding";
import type { Engine, WindowPort } from "./router";
import { EngineRouter } from "./router";

let router: EngineRouter | null = null;

// Ctrl+C in a terminal signals the whole process group. The main process answers it with a graceful
// quit that shuts this host down (landing board writes); dying here first would only make it restart us.
process.on("SIGINT", () => undefined);

function log(message: string): void {
  console.error(`[engine-host] ${message}`);
}

function postToMain(message: HostToMain): void {
  process.parentPort.postMessage(message);
}

function startEngine(addonPath: string, folders: AppFolders): EngineRouter {
  let engine: Engine;
  let created: EngineRouter | null = null;
  try {
    const binding = loadEngineBinding(addonPath);
    binding.init(JSON.stringify(folders));
    binding.subscribe((message) => {
      created?.handleSinkMessage(message);
    });
    engine = { binding };
    log(`loaded the engine addon from ${addonPath}`);
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    log(`the engine addon could not be loaded from ${addonPath}: ${reason}`);
    engine = {
      unavailable: transportError("engine_unavailable", `The Unfold engine could not be loaded: ${reason}`),
    };
  }
  created = new EngineRouter(engine, log);
  return created;
}

function windowPort(port: MessagePortMain): WindowPort {
  return {
    post: (message: HostToRenderer) => {
      port.postMessage(message);
    },
    onMessage: (handler) => {
      port.on("message", (event) => {
        handler(event.data);
      });
      port.start();
    },
    close: () => {
      port.close();
    },
  };
}

function handleLifecycle(message: MainToHost, ports: MessagePortMain[]): void {
  switch (message.type) {
    case "init":
      router ??= startEngine(message.addonPath, message.folders);
      return;
    case "attach": {
      const port = ports[0];
      if (!port) {
        log(`attach for window ${message.windowId} arrived without a port`);
        return;
      }
      if (!router) {
        log(`attach for window ${message.windowId} arrived before init`);
        port.close();
        return;
      }
      router.attach(message.windowId, windowPort(port));
      return;
    }
    case "detach":
      router?.detach(message.windowId);
      return;
    case "request": {
      const { id } = message;
      if (!router) {
        postToMain({ type: "result", id, ok: false, error: transportError("disconnected", "The engine host has not started.") });
        return;
      }
      const { windowId, command, args } = message;
      const onStream =
        message.stream === true
          ? (streamed: unknown) => {
              postToMain({ type: "stream", id, message: streamed });
            }
          : undefined;
      void (message.trusted ? router.trusted(windowId, command, args) : router.untrusted(windowId, command, args, onStream)).then(
        (result) => {
          postToMain({ type: "result", id, ...result });
        },
      );
      return;
    }
    case "shutdown":
      void (router?.shutdown() ?? Promise.resolve()).finally(() => {
        process.exit(0);
      });
      return;
  }
}

process.parentPort.on("message", (event) => {
  handleLifecycle(event.data as MainToHost, event.ports);
});
