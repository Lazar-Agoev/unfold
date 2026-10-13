/**
 * The `window.unfold` surface: the only door from a window's UI to the rest of the app.
 *
 * This file holds types only, so the UI can import it (`@unfold/desktop/unfold-api`) without pulling
 * in Electron or Node. Engine commands are routed to the engine host and answered by Rust; their
 * argument and result types are generated from Rust later (`ui/src/ipc/generated/`), so at this
 * transport level they are `unknown`. Window commands are handled by the main process and typed here.
 */

import type { AgentShellEvent, AgentWindowCommands } from "./agent-api";
import type { RenderShellEvent, RenderWindowCommands } from "./render-api";

/** An error a command was rejected with. Plain data, so it survives the context bridge. */
export interface TransportError {
  /** A stable snake_case code, such as `engine_unavailable`, `disconnected`, or one chosen by Rust. */
  readonly code: string;
  readonly message: string;
}

/** Error codes produced by the transport itself rather than by Rust. */
export type TransportErrorCode =
  /** The engine host is running but the Rust addon could not be loaded. */
  | "engine_unavailable"
  /** The connection to the engine host was lost while the request was in flight. */
  | "disconnected"
  /** The request was cancelled before it reached the engine. */
  | "cancelled"
  /** A message did not have the shape the protocol requires. */
  | "invalid_message"
  /** A window command the main process does not know, or one it refused. */
  | "invalid_command"
  /** A window sent a command only the main process may send (`add_project`, `export_mermaid`, …). */
  | "untrusted"
  /** Rust rejected without a well-formed `{code, message}` payload. */
  | "internal";

/**
 * The window's connection to the engine host.
 * - `connecting`: no engine port yet (window just opened).
 * - `connected`: the engine host is attached and the addon is loaded.
 * - `reconnecting`: the engine host stopped; the main process is restarting it.
 * - `unavailable`: the addon could not be loaded, or the engine host keeps failing.
 */
export type ConnectionState = "connecting" | "connected" | "reconnecting" | "unavailable";

/** An event delivered to the window. Engine events carry Rust's payload; `type` selects handlers. */
export interface UnfoldEvent {
  readonly type: string;
  readonly [field: string]: unknown;
}

/**
 * Edit > Undo / Redo, which the application menu forwards to the window's UI. Sent only when focus is
 * outside a text field: inside one, the preload runs the field's native undo instead, so these reach
 * the board.
 */
export type MenuCommand = "undo" | "redo";

/** A board the window shows: its project's id (`ProjectId`) and the board's id. */
export interface ShownBoard {
  readonly project: string;
  readonly board: string;
}

/**
 * Events that originate in the desktop shell rather than in Rust. They arrive through `on` like
 * engine events.
 */
export type ShellEvent =
  | { readonly type: "menu_command"; readonly command: MenuCommand }
  /** File > Add Project… added a project: select it. */
  | { readonly type: "project_added"; readonly project: string }
  /** File > Add Project… couldn't add the folder (the workspace is full, …): say why. */
  | { readonly type: "project_add_failed"; readonly path: string; readonly message: string }
  /** An agent's edit, read, background open, or finish (`agent-api.ts`). */
  | AgentShellEvent
  /** A page for the render window to draw, or the camera to capture it with (`render-api.ts`). */
  | RenderShellEvent;

/** A running streamed command. */
export interface StreamHandle {
  /** Resolves with the command's final result, or rejects with a {@link TransportError}. */
  readonly result: Promise<unknown>;
  /** Asks the engine to stop. The result then settles with whatever Rust reports. */
  cancel(): void;
}

/** What `window_info` reports about the window. */
export interface WindowInfo {
  readonly windowId: number;
  /**
   * The board to show: the one it last reported with `report_shown_board`, else the last board shown
   * when the app last quit. Null for Home.
   */
  readonly board: ShownBoard | null;
}

/** What `export_mermaid` or `export_png` wrote. */
export interface ExportedFile {
  readonly path: string;
  readonly bytes: number;
}

/** A project added to the workspace (`exists`: the folder, or its repository, already was one). */
export interface AddedProject {
  readonly status: "added" | "exists";
  /** The project's `ProjectId`. */
  readonly project: string;
}

/** Window and dialog commands, handled by the main process (the agent channel's and the render window's too). */
export interface WindowCommands extends AgentWindowCommands, RenderWindowCommands {
  /**
   * Show the folder picker and add the chosen folder to the workspace as a project. Resolves with
   * null when the picker is cancelled; rejects with the engine's error (`not_a_folder`,
   * `too_many_projects`).
   */
  add_project: { args: Record<string, never>; result: AddedProject | null };
  /**
   * Ask the person to confirm removing a project ("Remove <name> from the workspace? Its .unfold
   * folder stays."). The UI then calls the engine's `remove_project`.
   */
  confirm_remove_project: { args: { name: string }; result: { confirmed: boolean } };
  /**
   * Export a page of an open board (its last saved version) or an entrance as Mermaid:
   * the main process shows the save dialog (default name from `title`, `.mmd`) and has the engine
   * write the file. Resolves with null when the dialog is cancelled; rejects with the engine's error
   * (`board_not_open`, `io`, …).
   */
  export_mermaid: {
    args:
      | { project: string; board: string; page: string; title?: string }
      | { project: string; entrance: string; title?: string };
    result: ExportedFile | null;
  };
  /**
   * Export a page of a board this window holds as a PNG image: the main process shows the save dialog
   * (default name from `title`, `.png`), draws the page in the render window (unsaved edits included)
   * at twice the size, and writes the file. Resolves with null when the dialog is cancelled; rejects
   * with the reason the page couldn't be drawn (`render_timeout`, …).
   */
  export_png: { args: { project: string; board: string; page: string; title?: string }; result: ExportedFile | null };
  /**
   * Put text on the clipboard (Copy Mermaid, copy a path). The main process writes it: windows have
   * no clipboard permission (`navigator.clipboard.writeText` is denied, like every permission).
   */
  copy_to_clipboard: { args: { text: string }; result: null };
  /** Describe the window, and the board it shows at launch. */
  window_info: { args: Record<string, never>; result: WindowInfo };
  /**
   * The board the window now shows, or null for Home. The main process saves it with the window's
   * bounds, so the window comes back to it.
   */
  report_shown_board: { args: { board: ShownBoard | null }; result: null };
}

export type WindowCommand = keyof WindowCommands;

/** The argument list for a window command: optional when every field of its args is optional. */
export type WindowCommandArgs<C extends WindowCommand> =
  // eslint-disable-next-line @typescript-eslint/no-generated-empty-object-type -- "does {} satisfy the args?"
  Record<never, never> extends WindowCommands[C]["args"]
    ? [args?: WindowCommands[C]["args"]]
    : [args: WindowCommands[C]["args"]];

/** The object the preload script exposes as `window.unfold`. */
export interface UnfoldApi {
  /** Run one engine command. Rejects with a {@link TransportError}. */
  invoke(command: string, args?: Readonly<Record<string, unknown>>): Promise<unknown>;
  /** Run a streamed engine command; `onMessage` receives each progress message. */
  stream(
    command: string,
    args: Readonly<Record<string, unknown>> | undefined,
    onMessage: (message: unknown) => void,
  ): StreamHandle;
  /** Subscribe to events of one type. Returns an unsubscribe function. */
  on(eventType: string, handler: (event: UnfoldEvent) => void): () => void;
  /** Observe the engine connection. The handler is called at once with the current state. */
  onConnection(handler: (state: ConnectionState) => void): () => void;
  /** Run a window or dialog command in the main process. Rejects with a {@link TransportError}. */
  window<C extends WindowCommand>(
    command: C,
    ...args: WindowCommandArgs<C>
  ): Promise<WindowCommands[C]["result"]>;
}
