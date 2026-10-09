/**
 * Window and dialog commands, the only IPC the main process answers (architecture §12). Engine
 * commands never come here: they travel over each window's port to the engine host. A few window
 * commands end in an engine request the main process sends itself, with `"trusted": true`
 * (`export_mermaid`), because they involve a path only the main process may choose.
 */

import { clipboard, ipcMain } from "electron";
import type {
  ExportedFile,
  OpenedIn,
  TransportError,
  WindowCommand,
  WindowCommands,
  WindowInfo,
} from "../shared/unfold-api";
import { IPC, PRELOAD_COMMANDS, transportError, type WindowReply } from "../shared/transport";
import type { UiSource } from "./app-url";
import { chooseExportPath, chooseFolder } from "./dialogs";
import type { EngineHost } from "./engine-host";
import { EngineRequestError } from "./engine-host";
import { exportFileName } from "./export-name";
import { canonicalFolder, isDirectoryPath, type Folders } from "./folders";
import { isRecentFolder, openFolderTarget } from "./recent";
import { trustedSender } from "./security";
import type { WindowEntry, WindowRegistry } from "./windows";

type Handler<C extends WindowCommand> = (
  entry: WindowEntry,
  args: Readonly<Record<string, unknown>>,
) => Promise<WindowCommands[C]["result"]> | WindowCommands[C]["result"];

/** How long `move_tab_to_new_window` waits for the new window to attach to the folder. */
const ATTACH_TIMEOUT_MS = 20_000;
/** The most text `copy_to_clipboard` accepts (a large board's Mermaid is well under this). */
const MAX_CLIPBOARD_CHARS = 8_000_000;

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
  readonly folders: Folders;
  readonly engine: Pick<EngineHost, "request" | "whenAttached">;
  readonly ui: UiSource;
}

export function registerWindowCommands({ registry, folders, engine, ui }: WindowCommandServices): void {
  const opened = (entry: WindowEntry): OpenedIn => ({ windowId: entry.id, folder: entry.folder ?? "" });

  const handlers: { [C in WindowCommand]: Handler<C> } = {
    new_window: (entry, args) => {
      const folder = args["folder"] ?? "same";
      if (folder !== "same" && folder !== "none") {
        throw invalid('new_window: folder must be "same" or "none".');
      }
      return { windowId: registry.create(folder === "same" ? entry.folder : null).id };
    },
    focus_window: (_entry, args) => {
      const windowId = args["windowId"];
      const target = typeof windowId === "number" ? registry.get(windowId) : undefined;
      if (!target) {
        throw invalid("focus_window: no such window.");
      }
      registry.focus(target);
      return null;
    },
    close_window: (entry) => {
      entry.window.close();
      return null;
    },
    close_folder: async (entry) => {
      if (entry.folder === null) {
        return null;
      }
      await folders.close(entry);
      return { windowKey: entry.key };
    },
    choose_folder: async (entry, args) => {
      const target = args["target"] ?? "this_window";
      if (target !== "this_window" && target !== "new_window") {
        throw invalid('choose_folder: target must be "this_window" or "new_window".');
      }
      const folder = await chooseFolder(entry.window);
      if (folder === null) {
        return null;
      }
      return opened(await folders.open(folder, target === "this_window" ? entry : undefined));
    },
    open_folder: async (entry, args) => {
      const { path } = args;
      const target = openFolderTarget(args);
      if (typeof path !== "string" || target === null) {
        throw invalid('open_folder: path must be a string, target "this_window" or "new_window", newWindow a boolean.');
      }
      const recent = await engineRequest(0, "recent_folders", {});
      if (!isRecentFolder(recent, path, canonicalFolder)) {
        throw new CommandError({ code: "not_recent", message: "Only a folder from the recent list can be opened by path." });
      }
      const folder = canonicalFolder(path);
      if (!isDirectoryPath(folder)) {
        throw new CommandError({ code: "not_a_folder", message: `${path} is no longer a folder.` });
      }
      return opened(await folders.open(folder, target === "this_window" ? entry : undefined));
    },
    choose_export_path: async (entry, args) => {
      const defaultName = args["defaultName"] ?? "board.mmd";
      if (typeof defaultName !== "string") {
        throw invalid("choose_export_path: defaultName must be a string.");
      }
      const path = await chooseExportPath(entry.window, defaultName);
      return path === null ? null : { path };
    },
    export_mermaid: async (entry, args) => {
      const { boardId, entrance, title } = args;
      const source =
        typeof boardId === "string" && entrance === undefined
          ? { board: boardId }
          : typeof entrance === "string" && boardId === undefined
            ? { entrance }
            : null;
      if (!source || (title !== undefined && typeof title !== "string")) {
        throw invalid("export_mermaid: pass exactly one of boardId and entrance (strings), and an optional title.");
      }
      const path = await chooseExportPath(entry.window, exportFileName(title ?? source.board ?? source.entrance));
      if (path === null) {
        return null;
      }
      return (await engineRequest(entry.id, "export_mermaid", { ...source, path })) as ExportedFile;
    },
    move_tab_to_new_window: async (entry) => {
      const created = registry.create(entry.folder);
      if (entry.folder !== null) {
        const error = await engine.whenAttached(created.id, entry.folder, ATTACH_TIMEOUT_MS);
        if (error) {
          throw new CommandError(error);
        }
      }
      return { windowId: created.id };
    },
    transfer_board_state: (entry, args) => {
      const toWindow = args["toWindow"];
      const target = typeof toWindow === "number" ? registry.get(toWindow) : undefined;
      if (!target || target === entry) {
        throw invalid("transfer_board_state: no such other window.");
      }
      if (entry.folder === null || target.folder !== entry.folder) {
        throw invalid("transfer_board_state: the target window shows another folder.");
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
      folder: entry.folder,
      platform: process.platform === "darwin" || process.platform === "win32" ? process.platform : "linux",
    }),
  };

  /** Commands the preload sends on the person's behalf, outside `window.unfold.window`. */
  const preloadHandlers: Readonly<Record<string, Handler<WindowCommand>>> = {
    [PRELOAD_COMMANDS.openDroppedFolder]: async (entry, args) => {
      const { path, target = "this_window" } = args;
      if (typeof path !== "string" || (target !== "this_window" && target !== "new_window")) {
        throw invalid("open_dropped_folder: path must be a string and target this_window or new_window.");
      }
      if (!isDirectoryPath(path)) {
        throw new CommandError({ code: "not_a_folder", message: "Only a folder can be opened." });
      }
      return opened(await folders.open(path, target === "this_window" ? entry : undefined));
    },
  };

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
