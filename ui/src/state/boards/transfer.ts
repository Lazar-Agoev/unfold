/**
 * Move to New Window: a board, with its unsaved edits, undo history, and page views,
 * moves into a window of its own.
 *
 * The source window opens the new window (`move_tab_to_new_window`), hands it the board's claim
 * (`move_board`), sends it the session (`transfer_board_state`), and forgets the board. The target
 * receives `board_state_transferred` and adopts the session once its own window state is restored
 * (restoring replaces the tabs, so a session that arrives earlier waits).
 */

import { onShellEvent } from "../../ipc/unfold";
import { openBoardTab, removeBoardTab } from "../tabStore";
import { call, windowCommand } from "./engine";
import { adoptSession, exportSession, flushBoard, flushBoardView, forgetBoard } from "./store";
import type { BoardRef, BoardSession, WindowId } from "./types";

/** Moves an open board into a new window. Resolves with the new window's id. */
export async function moveBoardToNewWindow(ref: BoardRef): Promise<WindowId> {
  // Nothing may still be on its way to the engine once the claim moves; a failed save stays unsaved
  // in the session and is written by the new window.
  await Promise.all([flushBoard(ref), flushBoardView(ref)]);
  const session = exportSession(ref);
  if (!session) {
    throw new Error("That board isn't open in this window.");
  }
  const { windowId } = await windowCommand("move_tab_to_new_window");
  await call("move_board", { project: ref.project, id: ref.id, toWindow: windowId });
  await windowCommand("transfer_board_state", { toWindow: windowId, payload: session });
  forgetBoard(ref);
  removeBoardTab(ref);
  return windowId;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isDocument(value: unknown): boolean {
  return isRecord(value) && value["format"] === "unfold-board" && Array.isArray(value["pages"]) && value["pages"].length > 0;
}

function isEntry(value: unknown): boolean {
  return isRecord(value) && typeof value["page"] === "string" && isDocument(value["document"]);
}

/** The session another window sent, or null if the payload isn't one. */
export function parseSession(payload: unknown): BoardSession | null {
  if (!isRecord(payload) || !isRecord(payload["state"]) || !isRecord(payload["history"]) || !isRecord(payload["pages"])) {
    return null;
  }
  const { state, document, history, activePage } = payload;
  const { past, future } = history;
  if (typeof state["project"] !== "string" || typeof state["id"] !== "string" || !isDocument(state["document"]) || !isDocument(document)) {
    return null;
  }
  if (typeof activePage !== "string" || !Array.isArray(past) || !Array.isArray(future) || !past.every(isEntry) || !future.every(isEntry)) {
    return null;
  }
  return payload as unknown as BoardSession;
}

const waiting: BoardSession[] = [];
let ready = false;
let unsubscribe: (() => void) | null = null;

function receive(session: BoardSession): void {
  openBoardTab(adoptSession(session));
}

/**
 * Listens for boards other windows move here (idempotent). Install it before the UI renders, so a
 * session sent while the window was still loading isn't missed.
 */
export function installTransferReceiver(): void {
  if (unsubscribe) {
    return;
  }
  unsubscribe = onShellEvent("board_state_transferred", (event) => {
    const session = parseSession(event.payload);
    if (!session) {
      return;
    }
    if (ready) {
      receive(session);
    } else {
      waiting.push(session);
    }
  });
}

/** The window's own state is restored: adopt the boards that arrived meanwhile, and later ones at once. */
export function setTransfersReady(value: boolean): void {
  ready = value;
  if (!value) {
    return;
  }
  for (const session of waiting.splice(0)) {
    receive(session);
  }
}

/** Resets the receiver (tests). */
export function resetTransfers(): void {
  unsubscribe?.();
  unsubscribe = null;
  waiting.length = 0;
  ready = false;
}
