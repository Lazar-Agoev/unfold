/**
 * The `window.unfold` surface: the only door from a window's UI to the rest of the app.
 *
 * This file holds types only, so the UI can import it (`@unfold/desktop/unfold-api`) without pulling
 * in Electron or Node. Engine commands are routed to the engine host and answered by Rust; their
 * argument and result types are generated from Rust later (`ui/src/ipc/generated/`), so at this
 * transport level they are `unknown`. Window commands are handled by the main process and typed here.
 */

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
  /** A window sent a command only the main process may send (`export_mermaid`, `folder_windows`). */
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
  /**
   * File > Open Folder… while a window has focus: the window shows the picker itself, so it can
   * write its unsaved board edits before its folder is replaced.
   */
  | "open_folder"
  /**
   * File > Close Folder: the window writes its unsaved board edits, then runs the `close_folder`
   * window command.
   */
  | "close_folder"
  | "compile"
  | "close_tab"
  | "command_palette"
  | "toggle_sidebar"
  | "toggle_inspector"
  | "toggle_panel"
  | "show_files"
  | "show_entrances"
  | "show_search"
  | "show_boards"
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
  /** The window now shows another folder (or welcome). `windowKey` is its stable key from now on. */
  | { readonly type: "folder_changed"; readonly folder: string | null; readonly windowKey: string }
  | { readonly type: "workspace_open_failed"; readonly folder: string; readonly error: TransportError }
  /**
   * Another window of the folder handed this window a board's state (its undo history) with
   * `transfer_board_state`, after moving the board's claim here with `move_board`.
   */
  | { readonly type: "board_state_transferred"; readonly fromWindow: number; readonly payload: unknown };

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
   * The window's stable key for `window_state` / `save_window_state` / `forget_window_state`. A
   * restored window gets its saved key back; the key changes when the window switches folders
   * (see the `folder_changed` event).
   */
  readonly key: string;
  /** The folder this window shows, or null on the welcome screen. */
  readonly folder: string | null;
  readonly platform: "darwin" | "win32" | "linux";
}

/** What `export_mermaid` wrote. */
export interface ExportedFile {
  readonly path: string;
  readonly bytes: number;
}

/** Where a folder opened. */
export interface OpenedIn {
  readonly windowId: number;
  readonly folder: string;
}

/** A dropped item, as the UI receives it in a `drop` event (a DOM `File`). */
export interface DroppedFile {
  readonly name: string;
}

/** Window and dialog commands, handled by the main process. */
export interface WindowCommands {
  /** Open another window. `folder: "same"` (default) shows this window's folder; `"none"` shows welcome. */
  new_window: { args: { folder?: "same" | "none" }; result: { windowId: number } };
  /** Bring a window to the front (e.g. the one `open_board` reports in `heldBy`). */
  focus_window: { args: { windowId: number }; result: null };
  /** Close the calling window. */
  close_window: { args: Record<string, never>; result: null };
  /**
   * Close the calling window's folder: the window detaches from the folder's workspace (the
   * folder's last window closes it) and shows the welcome screen. The window stays open (on every
   * platform), and other windows on the folder are unaffected. Write unsaved board edits first.
   * Resolves with the window's key from now on (null and a no-op when it shows no folder); a
   * `folder_changed` event follows as for any folder change.
   */
  close_folder: { args: Record<string, never>; result: { windowKey: string } | null };
  /**
   * Show the folder picker. The chosen folder opens in this window (default) or a new one, and its
   * saved windows come back. If a window already shows it, that window is focused instead.
   * Resolves with null when the picker is cancelled.
   */
  choose_folder: {
    args: { target?: "this_window" | "new_window" };
    result: OpenedIn | null;
  };
  /**
   * Open a folder from the recent list (the welcome screen) here or in a new window (`target:
   * "new_window"` or `newWindow: true`), like
   * `choose_folder` after the picker. Only a path the engine lists in `recent_folders` that is still a
   * directory is accepted (`not_recent` / `not_a_folder` otherwise): paths outside a folder come only
   * from dialogs, the recent list, or a drop.
   */
  open_folder: {
    args: { path: string; target?: "this_window" | "new_window"; newWindow?: boolean };
    result: OpenedIn;
  };
  /** Show the export save dialog. Resolves with null when cancelled. */
  choose_export_path: { args: { defaultName?: string }; result: { path: string } | null };
  /**
   * Export a board this window holds (`boardId`, its last saved version) or an entrance (`entrance`)
   * as Mermaid: the main process shows the save dialog (default name from `title`, `.mmd`) and has
   * the engine write the file. Resolves with null when the dialog is cancelled; rejects with the
   * engine's error (`board_not_held`, `io`, …).
   */
  export_mermaid: {
    args: { boardId: string; title?: string } | { entrance: string; title?: string };
    result: ExportedFile | null;
  };
  /**
   * Open a new window on this window's folder for a tab. Resolves once the new window is attached
   * to the folder, so `move_board` to its `windowId` can follow at once.
   */
  move_tab_to_new_window: { args: Record<string, never>; result: { windowId: number } };
  /**
   * Hand a board's window-side state (its undo history) to another window of the same folder. The
   * target receives a `board_state_transferred` event. Call after `move_board`.
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
   * Open a folder the person just dropped on this window, from the `drop` event's `File`. Only an
   * item of the latest drop is accepted, and the main process checks it is a directory (else it
   * rejects with `not_a_folder`). Without a handler in the UI, the preload opens a folder dropped
   * anywhere in the window in this window.
   */
  openDroppedFolder(file: DroppedFile, options?: { target?: "this_window" | "new_window" }): Promise<OpenedIn>;
}
