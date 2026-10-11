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

/** Commands the application menu forwards to the focused window's UI. */
export type MenuCommand =
  /** File > Remove Project: the window asks to confirm (`confirm_remove_project`), then removes it. */
  | "remove_project"
  | "new_board"
  | "new_page"
  | "compile"
  | "publish"
  | "close_tab"
  /** View > Home. */
  | "show_home"
  | "toggle_sidebar"
  /** View > Diagram / Flow / Code: the board's lens. */
  | "lens_diagram"
  | "lens_flow"
  | "lens_code"
  /** Go > Add, Go to File, Go to Function, Commands: Add opened as is, on Files, on Functions, or with `>`. */
  | "add"
  | "add_files"
  | "add_functions"
  | "commands"
  /**
   * Edit > Undo / Redo. Sent only when focus is outside a text field: inside one, the preload runs
   * the field's native undo instead, so these reach the focused board.
   */
  | "undo"
  | "redo"
  /** Help > Copy Diagnostics: the window builds the redacted summary and copies it. */
  | "copy_diagnostics";

/**
 * Events that originate in the desktop shell rather than in Rust. They arrive through `on` like
 * engine events.
 */
export type ShellEvent =
  | { readonly type: "menu_command"; readonly command: MenuCommand }
  /**
   * Another window handed this window a board's state (its undo history) with
   * `transfer_board_state`, after moving the board's claim here with `move_board`.
   */
  | { readonly type: "board_state_transferred"; readonly fromWindow: number; readonly payload: unknown }
  /** `focus_window` named a board this window holds: show its tab. */
  | { readonly type: "focus_board"; readonly project: string; readonly board: string }
  /**
   * A project was added from the menu, the command line, the OS (`open-file`), a second launch, or a
   * drop the UI didn't handle: select it. Sent to the focused window.
   */
  | { readonly type: "project_added"; readonly project: string }
  /**
   * Adding a folder from the menu, the command line, the OS, a second launch, or a drop the UI didn't
   * handle failed (it isn't a folder, the workspace is full, …): say why. Sent to the focused window.
   */
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

/** What `window_info` reports about the calling window. */
export interface WindowInfo {
  readonly windowId: number;
  /**
   * The window's stable key for `window_state` / `save_window_state`. A restored window gets its
   * saved key back.
   */
  readonly key: string;
  readonly platform: "darwin" | "win32" | "linux";
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

/** A dropped item, as the UI receives it in a `drop` event (a DOM `File`). */
export interface DroppedFile {
  readonly name: string;
}

/** Window and dialog commands, handled by the main process (the agent channel's and the render window's too). */
export interface WindowCommands extends AgentWindowCommands, RenderWindowCommands {
  /** Open another window on the workspace (it shows the workspace's selected project). */
  new_window: { args: Record<string, never>; result: { windowId: number } };
  /**
   * Bring a window to the front (e.g. the one `open_board` reports in `heldBy`). With `board`, that
   * window also receives a `focus_board` event, so it shows the board's tab.
   */
  focus_window: {
    args: { windowId: number; board?: { project: string; board: string } };
    result: null;
  };
  /** Close the calling window. */
  close_window: { args: Record<string, never>; result: null };
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
   * Export a page of a board this window holds (its last saved version) or an entrance as Mermaid:
   * the main process shows the save dialog (default name from `title`, `.mmd`) and has the engine
   * write the file. Resolves with null when the dialog is cancelled; rejects with the engine's error
   * (`board_not_held`, `io`, …).
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
   * Open a new window for a tab. Resolves with its id at once: the UI then calls `move_board` to it
   * and `transfer_board_state`.
   */
  move_tab_to_new_window: { args: Record<string, never>; result: { windowId: number } };
  /**
   * Hand a board's window-side state (its undo history) to another window. The target receives a
   * `board_state_transferred` event (held until its UI has loaded). Call after `move_board`.
   */
  transfer_board_state: { args: { toWindow: number; payload: unknown }; result: null };
  /**
   * Put text on the clipboard (Copy Mermaid, copy a path). The main process writes it: windows have
   * no clipboard permission (`navigator.clipboard.writeText` is denied, like every permission).
   */
  copy_to_clipboard: { args: { text: string }; result: null };
  /** Describe the calling window. */
  window_info: { args: Record<string, never>; result: WindowInfo };
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
  /**
   * Add a folder the person just dropped on this window as a project, from the `drop` event's
   * `File`. Only an item of the latest drop is accepted, and the engine checks it is a folder (else
   * it rejects with `not_a_folder`). Without a handler in the UI, the preload adds a single folder
   * dropped anywhere in the window by itself and emits `project_added` (or `project_add_failed`).
   */
  addDroppedFolder(file: DroppedFile): Promise<AddedProject>;
}
