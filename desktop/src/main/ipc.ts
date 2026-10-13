/**
 * Window and dialog commands, the only IPC the main process answers (docs/ipc.md#how-a-command-travels). Engine
 * commands never come here: they travel over the window's port to the engine host. `add_project`
 * ends in an engine request the main process sends itself, with `"trusted": true`, because it takes a
 * path only the main process may choose.
 * `window_info` and `report_shown_board` hand the window's shown board to and from the workspace,
 * which saves it with the window's bounds. The agent channel's commands (`report_view` and the
 * answers to agent requests) go to the agent bridge; `agent_server_status` reads the MCP server's
 * state for Settings. The render window may send only `render_ready`, its answers; `export_png` has
 * it draw a page for the person.
 */

import { writeFile } from "node:fs/promises";
import { clipboard, ipcMain } from "electron";
import type { TransportError, WindowCommand, WindowCommands, WindowInfo } from "../shared/unfold-api";
import { IPC, transportError, type WindowReply } from "../shared/transport";
import type { AgentBridge } from "./agents/bridge";
import type { PageRenderer } from "./agents/render";
import { parseViewReport } from "./agents/views";
import type { UiSource } from "./app-url";
import { chooseExportPath, chooseFolder, confirmRemoveProject } from "./dialogs";
import { EngineRequestError } from "./engine-host";
import { exportFileName } from "./export-name";
import { trustedSender } from "./security";
import { shownBoard } from "./window-placement";
import type { AppWindow, WindowEntry } from "./windows";
import type { McpService } from "./mcp/service";
import type { Workspace } from "./workspace";

type Handler<C extends WindowCommand> = (
  entry: WindowEntry,
  args: Readonly<Record<string, unknown>>,
) => Promise<WindowCommands[C]["result"]> | WindowCommands[C]["result"];

/** The most text `copy_to_clipboard` accepts (a link or a setup command is far under this). */
const MAX_CLIPBOARD_CHARS = 8_000_000;
/** The longest project name `confirm_remove_project` shows (a project is named by its folder, and a macOS file name is at most 255 characters). */
const MAX_PROJECT_NAME = 255;

class CommandError extends Error {
  constructor(readonly error: TransportError) {
    super(error.message);
  }
}

function invalid(message: string): CommandError {
  return new CommandError(transportError("invalid_command", message));
}

export interface WindowCommandServices {
  readonly window: Pick<AppWindow, "forWebContents">;
  readonly workspace: Pick<Workspace, "add" | "shownBoard" | "showBoard">;
  readonly ui: UiSource;
  readonly agents: Pick<AgentBridge, "views" | "settle">;
  readonly mcp: Pick<McpService, "status">;
  readonly renderer: Pick<PageRenderer, "owns" | "settle" | "screenshot">;
}

export function registerWindowCommands({ window, workspace, ui, agents, mcp, renderer }: WindowCommandServices): void {
  const handlers: { [C in WindowCommand]: Handler<C> } = {
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
    export_png: async (entry, args) => {
      const { project, board, page, title } = args;
      if (typeof project !== "string" || typeof board !== "string" || typeof page !== "string" || (title !== undefined && typeof title !== "string")) {
        throw invalid("export_png: pass {project, board, page} (strings), and an optional title.");
      }
      const path = await chooseExportPath(entry.window, exportFileName(title ?? board));
      if (path === null) {
        return null;
      }
      // Twice the size, so it stays sharp on a high-density screen or in a document.
      const image = await renderer.screenshot({ board: { project, id: board }, page, scale: 2 });
      if (!image.ok) {
        throw new CommandError({ code: image.code, message: image.message });
      }
      await writeFile(path, image.png);
      return { path, bytes: image.png.length };
    },
    copy_to_clipboard: async (_entry, args) => {
      const text = args["text"];
      if (typeof text !== "string" || text.length > MAX_CLIPBOARD_CHARS) {
        throw invalid("copy_to_clipboard: text must be a string of at most 8,000,000 characters.");
      }
      await clipboard.writeText(text);
      return null;
    },
    window_info: (entry): WindowInfo => ({ windowId: entry.id, board: workspace.shownBoard() }),
    report_shown_board: (_entry, args) => {
      const { board } = args;
      const shown = shownBoard(board);
      if (board !== null && shown === null) {
        throw invalid("report_shown_board: board must be {project, board} (strings), or null for Home.");
      }
      workspace.showBoard(shown);
      return null;
    },
    report_view: (entry, args) => {
      const report = parseViewReport(args);
      if (!report) {
        throw invalid("report_view: pass {board?, page?, selection, lens?, visible, held}.");
      }
      agents.views.report(entry.id, report);
      return null;
    },
    agent_edit_result: (entry, args) => {
      agents.settle(entry.id, "edit", args["requestId"], args["result"]);
      return null;
    },
    agent_read_result: (entry, args) => {
      agents.settle(entry.id, "read", args["requestId"], args["result"]);
      return null;
    },
    agent_open_board_result: (entry, args) => {
      agents.settle(entry.id, "open", args["requestId"], args["result"]);
      return null;
    },
    agent_create_board_result: (entry, args) => {
      agents.settle(entry.id, "create", args["requestId"], args["result"]);
      return null;
    },
    agent_edit_board_result: (entry, args) => {
      agents.settle(entry.id, "board", args["requestId"], args["result"]);
      return null;
    },
    agent_show_result: (entry, args) => {
      agents.settle(entry.id, "show", args["requestId"], args["result"]);
      return null;
    },
    agent_repair_result: (entry, args) => {
      agents.settle(entry.id, "repair", args["requestId"], args["result"]);
      return null;
    },
    agent_server_status: () => mcp.status(),
    render_ready: () => {
      throw invalid("render_ready: only the render window answers render requests.");
    },
  };

  async function addProject(entry: WindowEntry, path: string) {
    try {
      return await workspace.add(entry.id, path);
    } catch (error) {
      throw error instanceof EngineRequestError ? new CommandError(error.error) : error;
    }
  }

  ipcMain.handle(IPC.windowCommand, async (event, command: unknown, args: unknown): Promise<WindowReply> => {
    const sender = trustedSender(ui, event);
    if (sender && renderer.owns(sender)) {
      if (command !== "render_ready" || !isRecord(args)) {
        return { ok: false, error: transportError("invalid_command", "The render window may only answer render requests.") };
      }
      renderer.settle(args["requestId"], args["result"]);
      return { ok: true, value: null };
    }
    const entry = sender ? window.forWebContents(sender) : undefined;
    if (!entry) {
      return { ok: false, error: transportError("invalid_command", "Window commands are accepted only from Unfold's window.") };
    }
    // Own properties only: a command named after an Object.prototype member must not reach it.
    const handler =
      typeof command === "string" && Object.hasOwn(handlers, command)
        ? (handlers[command as WindowCommand] as Handler<WindowCommand>)
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
