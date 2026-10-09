/**
 * The window's open boards (architecture §10.5, §11.2, U-37): each an `unfold-board` v1 document
 * plus view state that never goes into the file. A board is open in one window at a time, so its
 * edits and undo history live here alone.
 *
 * - `openBoard` claims a board for this window; one held elsewhere focuses that window instead.
 * - Every edit is one undo step (bursts with a `coalesce` key merge) and autosaves a moment later
 *   through `save_board`; saves never overlap, and edits made during one are saved right after.
 * - Outside edits arrive as `board_conflict`: a board without unsaved edits reloads, one with
 *   unsaved edits gets a `conflict` for the person to resolve.
 */

import { replaceEqualDeep } from "@tanstack/react-query";
import { produce, type Draft } from "immer";
import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import { showToast } from "../../components/Toast";
import type { BoardDocument, BoardState, CommandResult, View } from "../../ipc/generated";
import { toUnfoldError, windowCommand } from "../../ipc/unfold";
import { call } from "./engine";
import type { BoardSession, BoardViewState, Camera, FocusMove, OpenBoard, PendingGroup, PendingPlacement, TrailItem } from "./types";

/** How long after the last edit a board saves itself. */
export const AUTOSAVE_MS = 600;
/** Edits with the same coalesce key closer together than this are one undo step. */
export const COALESCE_MS = 1200;
/** Undo steps kept per board. */
export const HISTORY_LIMIT = 200;
/** Focus moves kept for Back. */
const TRAIL_LIMIT = 100;

export interface EditOptions {
  /** Merges consecutive edits with this key (a drag, typing a title) into one undo step. */
  readonly coalesce?: string;
}

export interface BoardsState {
  /** Open boards by id. */
  boards: Record<string, OpenBoard>;
  /** Open board ids in the order they opened. */
  order: string[];
  /** The board new cards go to: the board tab that was active last. */
  activeBoardId: string | null;
}

const emptyView = (): BoardViewState => ({
  lens: null,
  camera: null,
  selectedCardId: null,
  selectedKey: null,
  codeFolds: {},
  expanded: {},
  flowChains: {},
  lensExceptions: [],
  trail: [],
  back: [],
  forward: [],
  sizes: {},
  focus: null,
  pendingGroup: null,
  placement: null,
});

function freshBoard(state: BoardState): OpenBoard {
  return {
    id: state.id,
    place: state.place,
    name: state.name,
    document: state.document,
    revision: state.revision,
    health: state.health,
    outline: state.outline,
    label: state.label,
    edits: 0,
    savedEdits: 0,
    saving: false,
    saveError: null,
    conflict: null,
    history: { past: [], future: [], coalesce: null },
    view: emptyView(),
  };
}

export const useBoards = create<BoardsState>()(
  immer(() => ({
    boards: {},
    order: [],
    activeBoardId: null,
  })),
);

function set(recipe: (draft: Draft<BoardsState>) => void): void {
  useBoards.setState(recipe);
}

function get(): BoardsState {
  return useBoards.getState();
}

/** The open boards without `id` (a dynamic `delete` would do the same on the draft). */
function without(boards: Draft<Record<string, OpenBoard>>, id: string): Draft<Record<string, OpenBoard>> {
  return Object.fromEntries(Object.entries(boards).filter(([open]) => open !== id));
}

/** The open board, or undefined. */
export function getBoard(id: string): OpenBoard | undefined {
  return get().boards[id];
}

export function isDirty(board: OpenBoard): boolean {
  return board.edits !== board.savedEdits;
}

// ---------------------------------------------------------------------------------------------
// Opening and closing

type OpenedListener = (id: string) => void;
const openedListeners = new Set<OpenedListener>();

/** Called whenever a board opens in this window (so the editor can show its tab). */
export function subscribeBoardOpened(listener: OpenedListener): () => void {
  openedListeners.add(listener);
  return () => {
    openedListeners.delete(listener);
  };
}

/** Adds a board this window now holds (from `open_board`, `new_board`, or a moved session). */
export function adoptBoardState(state: BoardState, options: { replaces?: string; activate?: boolean } = {}): void {
  const replaced = options.replaces !== undefined ? get().boards[options.replaces] : undefined;
  set((draft) => {
    const board = freshBoard(state);
    if (replaced) {
      // Publish and rename change the id; the session (history, view) carries over.
      board.history = replaced.history;
      board.view = replaced.view;
      draft.boards = without(draft.boards, replaced.id);
      draft.order = draft.order.map((id) => (id === replaced.id ? state.id : id));
      if (draft.activeBoardId === replaced.id) {
        draft.activeBoardId = state.id;
      }
    }
    draft.boards[state.id] = board as Draft<OpenBoard>;
    if (!draft.order.includes(state.id)) {
      draft.order.push(state.id);
    }
    if (options.activate ?? true) {
      draft.activeBoardId = state.id;
    }
  });
  for (const listener of openedListeners) {
    listener(state.id);
  }
}

export type OpenOutcome = { readonly status: "opened"; readonly id: string } | { readonly status: "held"; readonly heldBy: number };

/** Claims a board for this window and opens it, or focuses the window that holds it. */
export async function openBoard(id: string, options: { activate?: boolean } = {}): Promise<OpenOutcome> {
  if (get().boards[id]) {
    if (options.activate ?? true) {
      setActiveBoard(id);
    }
    return { status: "opened", id };
  }
  const result = await call("open_board", { id });
  if (result.status === "held") {
    await windowCommand("focus_window", { windowId: result.heldBy });
    return { status: "held", heldBy: result.heldBy };
  }
  adoptBoardState(result.board, options);
  return { status: "opened", id: result.board.id };
}

/** Creates an empty draft, opens it, and makes it the active board. */
export async function newBoard(title?: string): Promise<string> {
  const state = await call("new_board", title !== undefined && title.trim().length > 0 ? { title } : {});
  adoptBoardState(state);
  return state.id;
}

/** Saves what is unsaved, releases the claim, and forgets the board. */
export async function closeBoard(id: string): Promise<void> {
  if (!get().boards[id]) {
    return;
  }
  await flushBoard(id);
  cancelTimer(id);
  try {
    await call("close_board", { id });
  } finally {
    set((draft) => {
      draft.boards = without(draft.boards, id);
      draft.order = draft.order.filter((open) => open !== id);
      if (draft.activeBoardId === id) {
        draft.activeBoardId = draft.order.at(-1) ?? null;
      }
    });
  }
}

/** Forgets a board without asking the engine (it was moved to another window or deleted). */
export function forgetBoard(id: string): void {
  cancelTimer(id);
  set((draft) => {
    draft.boards = without(draft.boards, id);
    draft.order = draft.order.filter((open) => open !== id);
    if (draft.activeBoardId === id) {
      draft.activeBoardId = draft.order.at(-1) ?? null;
    }
  });
}

export function setActiveBoard(id: string | null): void {
  if (id !== null && !get().boards[id]) {
    return;
  }
  if (get().activeBoardId !== id) {
    set((draft) => {
      draft.activeBoardId = id;
    });
  }
}

// ---------------------------------------------------------------------------------------------
// Editing, undo, redo

/**
 * Applies one edit to a board's document as one undo step, and schedules an autosave. Returns
 * whatever the recipe returns (e.g. a new card's id).
 */
export function editBoard<T>(id: string, recipe: (document: Draft<BoardDocument>) => T, options: EditOptions = {}): T | undefined {
  const board = get().boards[id];
  if (!board) {
    return undefined;
  }
  let result: T | undefined;
  const next = produce(board.document, (draft) => {
    result = recipe(draft);
  });
  if (next === board.document) {
    return result;
  }
  const before = board.document;
  const now = Date.now();
  set((draft) => {
    const open = draft.boards[id];
    if (!open) {
      return;
    }
    const last = open.history.coalesce;
    const merge = options.coalesce !== undefined && last?.key === options.coalesce && now - last.at < COALESCE_MS;
    if (!merge) {
      open.history.past.push(before);
      if (open.history.past.length > HISTORY_LIMIT) {
        open.history.past.splice(0, open.history.past.length - HISTORY_LIMIT);
      }
    }
    open.history.future = [];
    open.history.coalesce = options.coalesce !== undefined ? { key: options.coalesce, at: now } : null;
    open.document = next;
    open.edits += 1;
  });
  scheduleSave(id);
  return result;
}

/**
 * Changes the document as part of the last undo step instead of a new one (placing the cards an
 * entrance just opened once they are measured), and schedules an autosave.
 */
export function amendBoard(id: string, recipe: (document: Draft<BoardDocument>) => void): void {
  const board = get().boards[id];
  if (!board) {
    return;
  }
  const next = produce(board.document, recipe);
  if (next === board.document) {
    return;
  }
  set((draft) => {
    const open = draft.boards[id];
    if (open) {
      open.document = next;
      open.edits += 1;
    }
  });
  scheduleSave(id);
}

export function canUndo(id: string): boolean {
  return (get().boards[id]?.history.past.length ?? 0) > 0;
}

export function canRedo(id: string): boolean {
  return (get().boards[id]?.history.future.length ?? 0) > 0;
}

function step(id: string, from: "past" | "future"): boolean {
  const board = get().boards[id];
  const target = board?.history[from].at(-1);
  if (!board || !target) {
    return false;
  }
  const current = board.document;
  set((draft) => {
    const open = draft.boards[id];
    if (!open) {
      return;
    }
    open.history[from].pop();
    open.history[from === "past" ? "future" : "past"].push(current);
    open.history.coalesce = null;
    open.document = target;
    open.edits += 1;
  });
  scheduleSave(id);
  return true;
}

export function undo(id: string): boolean {
  return step(id, "past");
}

export function redo(id: string): boolean {
  return step(id, "future");
}

// ---------------------------------------------------------------------------------------------
// View state (never saved)

/** Changes a board's view state. View changes are not undo steps and never save. */
export function updateView(id: string, recipe: (view: Draft<BoardViewState>) => void): void {
  if (!get().boards[id]) {
    return;
  }
  set((draft) => {
    const open = draft.boards[id];
    if (open) {
      recipe(open.view);
    }
  });
}

export function setCamera(id: string, camera: Camera): void {
  updateView(id, (view) => {
    view.camera = camera;
  });
}

export function selectCard(id: string, cardId: string | null): void {
  updateView(id, (view) => {
    if (view.selectedCardId !== cardId) {
      view.selectedCardId = cardId;
      view.selectedKey = null;
    }
  });
}

/** Selects a block inside a card (a statement or Flow node), or the card itself with its own key. */
export function selectBlock(id: string, cardId: string, key: string | null): void {
  updateView(id, (view) => {
    view.selectedCardId = cardId;
    view.selectedKey = key;
  });
}

let focusSeq = 0;

/** Asks the canvas to bring cards into view once they are measured. */
export function requestFocus(id: string, cardIds: readonly string[], mode: "center" | "fit"): void {
  focusSeq += 1;
  const seq = focusSeq;
  updateView(id, (view) => {
    view.focus = { seq, cardIds: [...cardIds], mode };
  });
}

/** Records the current place for Back (a focus move is about to happen); clears Forward. */
export function pushFocusMove(id: string, trail?: readonly TrailItem[]): void {
  updateView(id, (view) => {
    const move: FocusMove = { camera: view.camera, selectedCardId: view.selectedCardId, trail: [...view.trail] };
    view.back.push(move as Draft<FocusMove>);
    if (view.back.length > TRAIL_LIMIT) {
      view.back.splice(0, view.back.length - TRAIL_LIMIT);
    }
    view.forward = [];
    if (trail) {
      view.trail = [...trail];
    }
  });
}

/** Back (`forward: false`) or Forward: restores the selection, trail, and camera of that move. */
export function travel(id: string, direction: "back" | "forward"): FocusMove | null {
  const board = get().boards[id];
  const move = board?.view[direction].at(-1);
  if (!board || !move) {
    return null;
  }
  const here: FocusMove = { camera: board.view.camera, selectedCardId: board.view.selectedCardId, trail: board.view.trail };
  updateView(id, (view) => {
    view[direction].pop();
    view[direction === "back" ? "forward" : "back"].push(here as Draft<FocusMove>);
    view.selectedCardId = move.selectedCardId;
    view.trail = [...move.trail];
    if (move.camera) {
      view.camera = move.camera;
    }
  });
  return move;
}

export function setPendingGroup(id: string, pending: PendingGroup | null): void {
  updateView(id, (view) => {
    view.pendingGroup = pending as Draft<PendingGroup> | null;
  });
}

export function setPendingPlacement(id: string, placement: PendingPlacement | null): void {
  updateView(id, (view) => {
    view.placement = placement as Draft<PendingPlacement> | null;
  });
}

// ---------------------------------------------------------------------------------------------
// Autosave

const timers = new Map<string, ReturnType<typeof setTimeout>>();
const inflight = new Map<string, Promise<void>>();

function cancelTimer(id: string): void {
  const timer = timers.get(id);
  if (timer !== undefined) {
    clearTimeout(timer);
    timers.delete(id);
  }
}

/** Saves a moment after the last change; repeated changes push the save back. */
export function scheduleSave(id: string): void {
  cancelTimer(id);
  timers.set(
    id,
    setTimeout(() => {
      timers.delete(id);
      void saveBoard(id);
    }, AUTOSAVE_MS),
  );
}

/**
 * `next`, reusing every part of `previous` it equals: a save or health refresh that changed nothing
 * (or one signature) keeps the identity of everything else, so cards that didn't change don't
 * re-render.
 */
function shared<T>(previous: T, next: T): T {
  return replaceEqualDeep(previous, next);
}

function applySaved(id: string, saved: BoardState, coveredEdits: number): void {
  const board = get().boards[id];
  if (!board) {
    return;
  }
  // The saved document carries the signatures; adopt it only if nothing changed meanwhile.
  const document = board.edits === coveredEdits ? shared(board.document, saved.document) : board.document;
  const health = shared(board.health, saved.health);
  const outline = shared(board.outline, saved.outline);
  set((draft) => {
    const open = draft.boards[id];
    if (!open) {
      return;
    }
    open.revision = saved.revision;
    open.health = health;
    open.outline = outline;
    open.label = saved.label;
    open.name = saved.name;
    open.place = saved.place;
    open.savedEdits = Math.max(open.savedEdits, coveredEdits);
    open.saveError = null;
    if (open.edits === coveredEdits) {
      open.document = document;
    }
  });
}

/**
 * Adopts a state the engine wrote itself from this window's saved version (a repair): the new
 * baseline, with the session (view) kept. The change is one undo step, so ⌘Z takes back exactly the
 * repair (and saves that). Call after a flush, so nothing is unsaved.
 */
export function applyServerState(id: string, state: BoardState): void {
  const board = get().boards[id];
  if (!board) {
    return;
  }
  const before = board.document;
  applySaved(id, state, board.edits);
  if (get().boards[id]?.document !== before) {
    set((draft) => {
      const open = draft.boards[id];
      if (open) {
        open.history.past.push(before);
        if (open.history.past.length > HISTORY_LIMIT) {
          open.history.past.splice(0, open.history.past.length - HISTORY_LIMIT);
        }
        open.history.future = [];
        open.history.coalesce = null;
      }
    });
  }
}

/**
 * Writes the board's unsaved edits now (`force` keeps this window's version over an outside
 * edit). Saves of one board never overlap: a save requested during another waits for it and then
 * writes whatever is still unsaved.
 */
export async function saveBoard(id: string, options: { force?: boolean } = {}): Promise<void> {
  // Several callers may be waiting on the same save; the first to resume starts the next one and
  // the others wait for that too, so two saves of one board never run at once.
  for (let running = inflight.get(id); running; running = inflight.get(id)) {
    await running;
  }
  const board = get().boards[id];
  if (!board || (!isDirty(board) && options.force !== true)) {
    return;
  }
  // A conflict waits for the person; only an explicit choice (`force`) writes over it.
  if (board.conflict && options.force !== true) {
    return;
  }
  cancelTimer(id);
  const covered = board.edits;
  const document = board.document;
  set((draft) => {
    const open = draft.boards[id];
    if (open) {
      open.saving = true;
    }
  });
  const work = (async () => {
    try {
      const saved = await call("save_board", { id, board: document, ...(options.force === true ? { force: true } : {}) });
      applySaved(id, saved, covered);
      if (options.force === true) {
        set((draft) => {
          const open = draft.boards[id];
          if (open) {
            open.conflict = null;
          }
        });
      }
    } catch (cause) {
      const error = toUnfoldError(cause);
      set((draft) => {
        const open = draft.boards[id];
        if (!open) {
          return;
        }
        if (error.code === "board_conflict") {
          open.conflict ??= { diskRevision: null };
        } else if (error.code !== "disconnected" && error.code !== "engine_unavailable") {
          // A disconnected save stays unsaved and is written once the engine is back.
          open.saveError = error.message;
        }
      });
      if (error.code !== "board_conflict" && error.code !== "disconnected" && error.code !== "engine_unavailable") {
        showToast({ message: `This board couldn't be saved: ${error.message}`, tone: "danger" });
      }
    } finally {
      set((draft) => {
        const open = draft.boards[id];
        if (open) {
          open.saving = false;
        }
      });
    }
  })();
  inflight.set(id, work);
  try {
    await work;
  } finally {
    inflight.delete(id);
  }
}

/** Saves now whatever is waiting for the autosave (before closing or moving a board). */
export async function flushBoard(id: string): Promise<void> {
  cancelTimer(id);
  await saveBoard(id);
}

/** Saves every open board's unsaved edits (before the window switches folders). */
export async function flushAllBoards(): Promise<void> {
  await Promise.all(get().order.map((id) => flushBoard(id)));
}

/**
 * The page is going away: sends every unsaved board now. Nothing runs after the page hides, so this
 * can't wait for a save in flight the way {@link flushAllBoards} does; the newer document simply
 * follows it.
 */
export function saveAllNow(): void {
  for (const board of Object.values(get().boards)) {
    if (isDirty(board) && !board.conflict) {
      cancelTimer(board.id);
      call("save_board", { id: board.id, board: board.document }).catch(() => undefined);
    }
  }
}

/** Forgets every open board without asking the engine (the window now shows another folder). */
export function forgetAllBoards(): void {
  for (const id of [...timers.keys()]) {
    cancelTimer(id);
  }
  inflight.clear();
  useBoards.setState({ boards: {}, order: [], activeBoardId: null });
}

/**
 * After the engine host restarted. The board claims went with it, so each open board is claimed
 * again (`open_board`) before anything is saved: a board the file of which changed meanwhile
 * reloads, or asks about the unsaved edits; then the edits the engine couldn't take are written.
 */
export async function reclaimBoards(): Promise<void> {
  await Promise.all(
    get().order.map(async (id) => {
      let result: CommandResult<"open_board">;
      try {
        result = await call("open_board", { id });
      } catch (cause) {
        const error = toUnfoldError(cause);
        if (error.code === "disconnected" || error.code === "engine_unavailable") {
          return; // The next reconnect tries again.
        }
        set((draft) => {
          const open = draft.boards[id];
          if (open) {
            open.saveError = error.message;
          }
        });
        return;
      }
      if (result.status === "held") {
        set((draft) => {
          const open = draft.boards[id];
          if (open) {
            open.saveError = "Another window opened this board while the engine restarted.";
          }
        });
        return;
      }
      const board = get().boards[id];
      if (!board) {
        return;
      }
      if (board.revision !== result.board.revision) {
        await handleOutsideChange(id, result.board.revision);
      } else if (isDirty(board) && !board.conflict) {
        await saveBoard(id);
      }
    }),
  );
}

// ---------------------------------------------------------------------------------------------
// Outside edits

/** Reads the board from disk again, replacing this window's document (Load theirs). */
export async function reloadBoard(id: string): Promise<void> {
  const state = await call("reload_board", { id });
  cancelTimer(id);
  const before = get().boards[id]?.document;
  set((draft) => {
    const open = draft.boards[id];
    if (!open) {
      return;
    }
    if (before) {
      // Reloading is undoable like an edit, so the person can get their version back.
      open.history.past.push(before);
      open.history.future = [];
      open.history.coalesce = null;
    }
    open.document = state.document;
    open.revision = state.revision;
    open.health = state.health;
    open.outline = state.outline;
    open.label = state.label;
    open.edits += 1;
    open.savedEdits = open.edits;
    open.conflict = null;
    open.saveError = null;
  });
}

/** The person's answer to a conflict: take the file on disk, or write this window's version over it. */
export async function resolveConflict(id: string, keep: "theirs" | "mine"): Promise<void> {
  if (keep === "theirs") {
    await reloadBoard(id);
  } else {
    await saveBoard(id, { force: true });
  }
}

/** Handles `board_conflict`: reload a clean board, or ask about one with unsaved edits. */
export async function handleOutsideChange(id: string, diskRevision: string | null): Promise<void> {
  const board = get().boards[id];
  if (!board || board.revision === diskRevision) {
    return;
  }
  const running = inflight.get(id);
  if (running) {
    await running;
  }
  const current = get().boards[id];
  if (!current || current.revision === diskRevision) {
    return;
  }
  if (!isDirty(current) && diskRevision !== null) {
    try {
      await reloadBoard(id);
      return;
    } catch {
      // Fall through: the person decides.
    }
  }
  set((draft) => {
    const open = draft.boards[id];
    if (open) {
      open.conflict = { diskRevision };
    }
  });
}

/** Refreshes open boards' health and outline after a graph update. */
export async function refreshHealth(): Promise<void> {
  await Promise.all(
    get().order.map(async (id) => {
      try {
        const view = await call("board_health", { id });
        const board = get().boards[id];
        if (!board) {
          return;
        }
        const health = shared(board.health, view.health);
        const outline = shared(board.outline, view.outline);
        if (health === board.health && outline === board.outline && view.label === board.label) {
          return;
        }
        set((draft) => {
          const open = draft.boards[id];
          if (open) {
            open.health = health;
            open.outline = outline;
            open.label = view.label;
          }
        });
      } catch {
        // Health is advisory; the next save or update refreshes it.
      }
    }),
  );
}

// ---------------------------------------------------------------------------------------------
// Sessions (Move to New Window)

/** The board with its unsaved document and undo history, as plain JSON. */
export function exportSession(id: string): BoardSession | null {
  const board = get().boards[id];
  if (!board) {
    return null;
  }
  return {
    state: {
      id: board.id,
      place: board.place,
      name: board.name,
      document: board.document,
      revision: board.revision,
      health: board.health,
      outline: board.outline,
      label: board.label,
    },
    document: board.document,
    history: { past: [...board.history.past], future: [...board.history.future] },
  };
}

/** Opens a moved board with its history; unsaved edits from the other window save here. */
export function adoptSession(session: BoardSession): void {
  adoptBoardState(session.state);
  const dirty = session.document !== session.state.document;
  set((draft) => {
    const open = draft.boards[session.state.id];
    if (!open) {
      return;
    }
    open.document = session.document;
    open.history.past = [...session.history.past] as Draft<BoardDocument>[];
    open.history.future = [...session.history.future] as Draft<BoardDocument>[];
    if (dirty) {
      open.edits += 1;
    }
  });
  if (dirty) {
    scheduleSave(session.state.id);
  }
}

/**
 * Applies a change of the window's top Code/Flow switch to every open board: each card shows that
 * view until its own switch is used. Nothing is written to the boards.
 */
export function applyLensToBoards(lens: View): void {
  set((draft) => {
    for (const board of Object.values(draft.boards)) {
      board.view.lens = lens;
      board.view.lensExceptions = [];
    }
  });
}

/** The effective view of a card: the board's lens (unless the card is an exception), its own switch, or the board default. */
export function effectiveView(
  lens: View | null,
  cardView: View | undefined,
  defaultView: View,
  exception: boolean,
): View {
  if (lens !== null && !exception) {
    return lens;
  }
  return cardView ?? defaultView;
}

/** Resets the store (tests). */
export function resetBoards(): void {
  forgetAllBoards();
  openedListeners.clear();
}
