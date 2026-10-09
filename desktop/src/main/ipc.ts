/**
 * Window and dialog commands, the only IPC the main process answers (architecture §12). Engine
 * commands never come here: they travel over each window's port to the engine host. A few window
 * commands end in an engine request the main process sends itself, with `"trusted": true`
 * (`add_project`, `export_mermaid`), because they involve a path only the main process may choose.
 */

import { clipboard, ipcMain } from "electron";
import type { ExportedFile, TransportError, WindowCommand, WindowCommands, WindowInfo } from "../shared/unfold-api";
import { IPC, PRELOAD_COMMANDS, transportError, type WindowReply } from "../shared/transport";
import type { UiSource } from "./app-url";
import { chooseExportPath, chooseFolder, confirmRemoveProject } from "./dialogs";
import type { EngineHost } from "./engine-host";
import { EngineRequestError } from "./engine-host";
import { exportFileName } from "./export-name";
import { trustedSender } from "./security";
import type { WindowEntry, WindowRegistry } from "./windows";
import type { Workspace } from "./workspace";

type Handler<C extends WindowCommand> = (
  entry: WindowEntry,
  args: Readonly<Record<string, unknown>>,
) => Promise<WindowCommands[C]["result"]> | WindowCommands[C]["result"];

/** The most text `copy_to_clipboard` accepts (a large board's Mermaid is well under this). */
const MAX_CLIPBOARD_CHARS = 8_000_000;
/** The longest project name `confirm_remove_project` shows (a shared name is at most 64 characters). */
const MAX_PROJECT_NAME = 64;

class CommandError extends Error {
  constructor(readonly error: TransportError) {
    super(error.message);
  }
}

function invalid(message: string): CommandError {
  return new CommandError(transportError("invalid_command", message));
}

export interface WindowCommandServices {
  readonly registry: WindowRegistry;
  readonly workspace: Pick<Workspace, "add">;
  readonly engine: Pick<EngineHost, "request">;
  readonly ui: UiSource;
}

export function registerWindowCommands({ registry, workspace, engine, ui }: WindowCommandServices): void {
  const handlers: { [C in WindowCommand]: Handler<C> } = {
    new_window: () => ({ windowId: registry.create().id }),
    focus_window: (_entry, args) => {
      const { windowId, board } = args;
      const target = typeof windowId === "number" ? registry.get(windowId) : undefined;
      if (!target) {
        throw invalid("focus_window: no such window.");
      }
      const focusBoard = board === undefined ? null : boardRef(board);
      if (focusBoard === undefined) {
        throw invalid("focus_window: board must be {project, board} (strings).");
      }
      registry.focus(target);
      if (focusBoard) {
        registry.deliver(target, { type: "focus_board", ...focusBoard });
      }
      return null;
    },
    close_window: (entry) => {
      entry.window.close();
      return null;
    },
    add_project: async (entry) => {
      const path = await chooseFolder(entry.window);
      return path === null ? null : await addProject(entry, path);
    },
    confirm_remove_project: async (entry, args) => {
      const { name } = args;
      if (typeof name !== "string" || name.length === 0 || name.length > MAX_PROJECT_NAME) {
        throw invalid("confirm_remove_project: name must be a project name.");
      }
      return { confirmed: await confirmRemoveProject(entry.window, name) };
    },
    export_mermaid: async (entry, args) => {
      const source = exportSource(args);
      if (!source) {
        throw invalid(
          "export_mermaid: pass {project, board, page} or {project, entrance} (strings), and an optional title.",
        );
      }
      const path = await chooseExportPath(entry.window, exportFileName(source.title));
      if (path === null) {
        return null;
      }
      return (await engineRequest(entry.id, "export_mermaid", { ...source.args, path })) as ExportedFile;
    },
    move_tab_to_new_window: () => ({ windowId: registry.create().id }),
    transfer_board_state: (entry, args) => {
      const toWindow = args["toWindow"];
      const target = typeof toWindow === "number" ? registry.get(toWindow) : undefined;
      if (!target || target === entry) {
        throw invalid("transfer_board_state: no such other window.");
      }
      registry.deliver(target, { type: "board_state_transferred", fromWindow: entry.id, payload: args["payload"] });
      return null;
    },
    copy_to_clipboard: async (_entry, args) => {
      const text = args["text"];
      if (typeof text !== "string" || text.length > MAX_CLIPBOARD_CHARS) {
        throw invalid("copy_to_clipboard: text must be a string of at most 8,000,000 characters.");
      }
      await clipboard.writeText(text);
      return null;
    },
    window_info: (entry): WindowInfo => ({
      windowId: entry.id,
      key: entry.key,
      platform: process.platform === "darwin" || process.platform === "win32" ? process.platform : "linux",
    }),
  };

  /** Commands the preload sends on the person's behalf, outside `window.unfold.window`. */
  const preloadHandlers: Readonly<Record<string, Handler<WindowCommand>>> = {
    [PRELOAD_COMMANDS.addDroppedFolder]: async (entry, args) => {
      const { path } = args;
      if (typeof path !== "string" || path.length === 0) {
        throw invalid("add_dropped_folder: path must be a string.");
      }
      return addProject(entry, path);
    },
  };

  async function addProject(entry: WindowEntry, path: string) {
    try {
      return await workspace.add(entry.id, path);
    } catch (error) {
      throw error instanceof EngineRequestError ? new CommandError(error.error) : error;
    }
  }

  async function engineRequest(windowId: number, command: string, args: Readonly<Record<string, unknown>>) {
    try {
      return await engine.request(windowId, command, args);
    } catch (error) {
      throw error instanceof EngineRequestError ? new CommandError(error.error) : error;
    }
  }

  ipcMain.handle(IPC.windowCommand, async (event, command: unknown, args: unknown): Promise<WindowReply> => {
    const sender = trustedSender(ui, event);
    const entry = sender ? registry.forWebContents(sender) : undefined;
    if (!entry) {
      return { ok: false, error: transportError("invalid_command", "Window commands are accepted only from Unfold windows.") };
    }
    // Own properties only: a command named after an Object.prototype member must not reach it.
    const handler =
      typeof command !== "string"
        ? undefined
        : Object.hasOwn(handlers, command)
          ? (handlers[command as WindowCommand] as Handler<WindowCommand>)
          : Object.hasOwn(preloadHandlers, command)
            ? preloadHandlers[command]
            : undefined;
    if (!handler) {
      return { ok: false, error: transportError("invalid_command", `Unknown window command: ${String(command)}.`) };
    }
    if (args !== undefined && (typeof args !== "object" || args === null || Array.isArray(args))) {
      return { ok: false, error: transportError("invalid_command", `${String(command)}: args must be an object.`) };
    }
    try {
      const value = await handler(entry, (args ?? {}) as Readonly<Record<string, unknown>>);
      return { ok: true, value };
    } catch (error) {
      if (error instanceof CommandError) {
        return { ok: false, error: error.error };
      }
      console.error(`[main] window command ${String(command)} failed:`, error);
      return { ok: false, error: transportError("internal", `${String(command)} failed.`) };
    }
  });
}

/** `focus_window`'s `board`: `{project, board}`, or undefined when malformed. */
function boardRef(value: unknown): { project: string; board: string } | undefined {
  if (!isRecord(value)) {
    return undefined;
  }
  const { project, board } = value;
  return typeof project === "string" && typeof board === "string" ? { project, board } : undefined;
}

/**
 * `export_mermaid`'s source as the engine takes it (`{project, board: {id, page}}` or `{project,
 * entrance}`), and the title the default file name comes from; null when malformed.
 */
function exportSource(
  args: Readonly<Record<string, unknown>>,
): { args: Readonly<Record<string, unknown>>; title: string } | null {
  const { project, board, page, entrance, title } = args;
  if (typeof project !== "string" || (title !== undefined && typeof title !== "string")) {
    return null;
  }
  if (typeof board === "string" && typeof page === "string" && entrance === undefined) {
    return { args: { project, board: { id: board, page } }, title: title ?? board };
  }
  if (typeof entrance === "string" && board === undefined && page === undefined) {
    return { args: { project, entrance }, title: title ?? entrance };
  }
  return null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
