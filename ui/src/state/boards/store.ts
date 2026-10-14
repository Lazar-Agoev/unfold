/**
 * The open boards: each an `unfold-board` document plus view state per page. Every board opened while
 * the app runs stays open, so its edits and undo history live here until the app quits.
 *
 * - Every edit is one undo step (bursts with a `coalesce` key merge); a step remembers the page it
 *   changed, and undo or redo shows that page. One history per board, across its pages.
 * - Edits autosave a moment later through `save_board` as a `BoardEdit` (title and pages only; the
 *   folder is Rust's), written over the board's file; saves never overlap, and edits made during one are
 *   saved right after. A failed save keeps the edits, and the next edit or Retry writes them.
 * - An outside change to the file arrives as `board_reloaded`: the board takes the file's version and
 *   its undo history starts over, except while it has edits not yet saved or a save is in flight (the
 *   person's version is written over the file then). A file deleted outside is written back by the
 *   next save.
 * - View state (each page's camera and selection, and the active page) lasts only while the app runs;
 *   view changes are never undo steps.
 * - An agent's edit is saved at once (`saveStep`), and a repair is written by the engine itself
 *   (`engineStep`); either becomes an undo step only once the engine accepts it.
 */

import { replaceEqualDeep } from "@tanstack/react-query";
import { produce, type Draft } from "immer";
import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import type { BoardDocument, BoardState, Page } from "../../ipc/generated";
import { UnfoldError, toUnfoldError } from "../../ipc/unfold";
import { findPage } from "./document";
import { call } from "./engine";
import { boardKey } from "./keys";
import type {
  BoardRef,
  Camera,
  HistoryEntry,
  OpenBoard,
  PageId,
  PageTarget,
  PageView,
  Selection,
} from "./types";

/** How long after the last edit a board saves itself. */
export const AUTOSAVE_MS = 600;
/** Edits with the same coalesce key closer together than this are one undo step. */
const COALESCE_MS = 1200;
/** Undo steps kept per board. */
export const HISTORY_LIMIT = 200;

export interface EditOptions {
  /** Merges consecutive edits with this key (a drag, typing a title) into one undo step. */
  readonly coalesce?: string;
  /** The page the edit changes, which undo and redo then show (default: the active page). */
  readonly page?: PageId;
}

export interface BoardsState {
  /** Open boards by `boardKey`. */
  readonly boards: Readonly<Record<string, OpenBoard>>;
}

export const useBoards = create<BoardsState>()(immer(() => ({ boards: {} })));

function set(recipe: (draft: Draft<BoardsState>) => void): void {
  useBoards.setState(recipe);
}

/** Runs `recipe` on the open board's draft, if it is open. */
function update(ref: BoardRef, recipe: (board: Draft<OpenBoard>) => void): void {
  set((draft) => {
    const open = draft.boards[boardKey(ref)];
    if (open) {
      recipe(open);
    }
  });
}

/** The open board, or undefined. */
export function getOpenBoard(ref: BoardRef): OpenBoard | undefined {
  return useBoards.getState().boards[boardKey(ref)];
}

/**
 * Part of an open board, kept current (undefined while it isn't open). `select` must return stable
 * values (store fields, not new objects), as with any zustand selector.
 */
export function useOpenBoard<T>(ref: BoardRef, select: (board: OpenBoard) => T): T | undefined {
  const key = boardKey(ref);
  return useBoards((state) => {
    const board = state.boards[key];
    return board ? select(board) : undefined;
  });
}

export function isDirty(board: OpenBoard): boolean {
  return board.edits !== board.savedEdits;
}

/** Every open board. */
export function openBoards(): OpenBoard[] {
  return Object.values(useBoards.getState().boards);
}

// ---------------------------------------------------------------------------------------------
// Page views

function pageView(): PageView {
  return { camera: null, selection: { kind: "none" }, sizes: {}, focus: null };
}

/** What a page that was never shown reads as (frozen: selectors get stable values from it). */
const EMPTY_VIEW: PageView = Object.freeze(pageView());

/** The page view, creating it the first time the page is touched. */
function viewOf(board: Draft<OpenBoard>, page: PageId): Draft<PageView> {
  board.pages[page] ??= pageView() as Draft<PageView>;
  return board.pages[page];
}

/** A page of the board's document: `wanted` when it exists, else the first. */
function existingPage(document: BoardDocument, wanted: PageId | null | undefined): PageId {
  return (wanted !== null && wanted !== undefined && findPage(document, wanted) ? wanted : document.pages[0]?.id) ?? "page-1";
}

/** Part of a page's view state, kept current (a never-shown page reads as empty). */
export function usePageView<T>(target: PageTarget, select: (view: PageView) => T): T {
  const key = boardKey(target.board);
  return useBoards((state) => select(state.boards[key]?.pages[target.page] ?? EMPTY_VIEW));
}

/** A page's view state outside React (empty when never shown). */
export function getPageView(target: PageTarget): PageView {
  return getOpenBoard(target.board)?.pages[target.page] ?? EMPTY_VIEW;
}

/** Changes a page's view state. Never an undo step, and never saved. */
export function updatePageView(target: PageTarget, recipe: (view: Draft<PageView>) => void): void {
  update(target.board, (board) => {
    recipe(viewOf(board, target.page));
  });
}

/** Shows another page of the board. */
export function setActivePage(ref: BoardRef, page: PageId): void {
  const board = getOpenBoard(ref);
  if (!board || board.activePage === page || !findPage(board.document, page)) {
    return;
  }
  update(ref, (open) => {
    open.activePage = page;
    viewOf(open, page);
  });
}

export function setCamera(target: PageTarget, camera: Camera): void {
  updatePageView(target, (view) => {
    view.camera = { x: camera.x, y: camera.y, zoom: camera.zoom };
  });
}

export function select(target: PageTarget, selection: Selection): void {
  updatePageView(target, (view) => {
    view.selection = selection as Draft<Selection>;
  });
}

let focusSeq = 0;

/** Asks the canvas to bring cards into view once they are measured. */
export function requestCardFocus(target: PageTarget, cardIds: readonly string[], mode: "center" | "fit"): void {
  focusSeq += 1;
  const seq = focusSeq;
  updatePageView(target, (view) => {
    view.focus = { seq, cardIds: [...cardIds], mode };
  });
}

// ---------------------------------------------------------------------------------------------
// Opening and forgetting

function freshBoard(state: BoardState, page: PageId | null, readOnly: boolean): OpenBoard {
  const activePage = existingPage(state.document, page);
  return {
    ref: { project: state.project, id: state.id },
    name: state.name,
    document: state.document,
    revision: state.revision,
    health: state.health,
    label: state.label,
    edits: 0,
    savedEdits: 0,
    saving: false,
    saveError: null,
    history: { past: [], future: [], coalesce: null },
    activePage,
    pages: { [activePage]: pageView() },
    ...(readOnly ? { readOnly } : {}),
  };
}

/**
 * Adds a board just opened (from `open_board` or `new_board`), showing `page` (else the first). A
 * `readOnly` board (the render window's, which the engine doesn't hold open) is shown, never saved.
 */
export function adoptBoardState(state: BoardState, options: { readonly page?: PageId | null; readonly readOnly?: boolean } = {}): BoardRef {
  const board = freshBoard(state, options.page ?? null, options.readOnly === true);
  set((draft) => {
    draft.boards[boardKey(board.ref)] = board as Draft<OpenBoard>;
  });
  return board.ref;
}

/** The open boards without `key` (a dynamic `delete` would do the same on the draft). */
function without(boards: Draft<Record<string, OpenBoard>>, key: string): Draft<Record<string, OpenBoard>> {
  return Object.fromEntries(Object.entries(boards).filter(([open]) => open !== key));
}

/** Forgets a board without asking the engine (closed or deleted). */
export function forgetBoard(ref: BoardRef): void {
  cancelSaveTimer(ref);
  set((draft) => {
    draft.boards = without(draft.boards, boardKey(ref));
  });
}

// ---------------------------------------------------------------------------------------------
// Editing, undo, redo

function pushPast(board: Draft<OpenBoard>, entry: HistoryEntry): void {
  board.history.past.push(entry);
  if (board.history.past.length > HISTORY_LIMIT) {
    board.history.past.splice(0, board.history.past.length - HISTORY_LIMIT);
  }
}

/**
 * Applies one edit to a board's document as one undo step, and schedules an autosave. Returns whatever
 * the recipe returns (e.g. a new card's id).
 */
export function editBoard<T>(ref: BoardRef, recipe: (document: Draft<BoardDocument>) => T, options: EditOptions = {}): T | undefined {
  const board = getOpenBoard(ref);
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
  const entry: HistoryEntry = { document: board.document, page: options.page ?? board.activePage };
  const now = Date.now();
  update(ref, (open) => {
    const last = open.history.coalesce;
    const merge = options.coalesce !== undefined && last?.key === options.coalesce && now - last.at < COALESCE_MS;
    if (!merge) {
      pushPast(open, entry);
    }
    open.history.future = [];
    open.history.coalesce = options.coalesce !== undefined ? { key: options.coalesce, at: now } : null;
    open.document = next;
    open.edits += 1;
  });
  scheduleSave(ref);
  return result;
}

/** `editBoard` on one page: the recipe gets the page's draft (nothing happens when the page is gone). */
export function editPage<T>(
  target: PageTarget,
  recipe: (page: Draft<Page>, document: Draft<BoardDocument>) => T,
  options: Omit<EditOptions, "page"> = {},
): T | undefined {
  return editBoard(
    target.board,
    (document) => {
      const page = document.pages.find((item) => item.id === target.page);
      return page ? recipe(page, document) : undefined;
    },
    { ...options, page: target.page },
  );
}

export function canUndo(ref: BoardRef): boolean {
  return (getOpenBoard(ref)?.history.past.length ?? 0) > 0;
}

function step(ref: BoardRef, from: "past" | "future"): boolean {
  const board = getOpenBoard(ref);
  const entry = board?.history[from].at(-1);
  if (!board || !entry) {
    return false;
  }
  const back: HistoryEntry = { document: board.document, page: entry.page, ...(entry.name !== undefined ? { name: entry.name } : {}) };
  const page = existingPage(entry.document, entry.page);
  update(ref, (open) => {
    open.history[from].pop();
    open.history[from === "past" ? "future" : "past"].push(back);
    open.history.coalesce = null;
    open.document = entry.document;
    open.edits += 1;
    open.activePage = page;
    viewOf(open, page);
  });
  scheduleSave(ref);
  return true;
}

/** Undoes the board's last step and shows the page it changed. */
export function undo(ref: BoardRef): boolean {
  return step(ref, "past");
}

/** Redoes the board's next step and shows the page it changed. */
export function redo(ref: BoardRef): boolean {
  return step(ref, "future");
}

// ---------------------------------------------------------------------------------------------
// Autosave

const saveTimers = new Map<string, ReturnType<typeof setTimeout>>();
const inflight = new Map<string, Promise<void>>();

function cancelSaveTimer(ref: BoardRef): void {
  const key = boardKey(ref);
  const timer = saveTimers.get(key);
  if (timer !== undefined) {
    clearTimeout(timer);
    saveTimers.delete(key);
  }
}

/** Saves a moment after the last change; repeated changes push the save back. A read-only board never saves. */
function scheduleSave(ref: BoardRef): void {
  cancelSaveTimer(ref);
  if (getOpenBoard(ref)?.readOnly) {
    return;
  }
  saveTimers.set(
    boardKey(ref),
    setTimeout(() => {
      saveTimers.delete(boardKey(ref));
      void saveBoard(ref);
    }, AUTOSAVE_MS),
  );
}

/**
 * `next`, reusing every part of `previous` it equals: a save or health refresh that changed nothing
 * (or one signature) keeps the identity of everything else, so cards that didn't change don't redraw.
 */
function shared<T>(previous: T, next: T): T {
  return replaceEqualDeep(previous, next);
}

/** What `save_board` takes: the title and pages, never `folder` (Rust's). */
function boardEdit(document: BoardDocument) {
  return { title: document.title, pages: document.pages };
}

function applySaved(ref: BoardRef, saved: BoardState, coveredEdits: number): void {
  const board = getOpenBoard(ref);
  if (!board) {
    return;
  }
  // The saved document carries the signatures; adopt it only if nothing changed meanwhile.
  const document = board.edits === coveredEdits ? shared(board.document, saved.document) : board.document;
  const health = shared(board.health, saved.health);
  update(ref, (open) => {
    open.revision = saved.revision;
    open.health = health;
    open.label = saved.label;
    open.name = saved.name;
    open.savedEdits = Math.max(open.savedEdits, coveredEdits);
    open.saveError = null;
    if (open.edits === coveredEdits) {
      open.document = document;
    }
  });
}

/**
 * Writes the board's unsaved edits over its file now. Saves of one board never overlap: a save
 * requested during another waits for it and then writes whatever is still unsaved. A failed save keeps
 * the edits and records why (`saveError`) until a save succeeds.
 */
export async function saveBoard(ref: BoardRef): Promise<void> {
  const key = boardKey(ref);
  // Several callers may be waiting on the same save; the first to resume starts the next one and the
  // others wait for that too, so two saves of one board never run at once.
  for (let running = inflight.get(key); running; running = inflight.get(key)) {
    await running;
  }
  const board = getOpenBoard(ref);
  if (!board || !isDirty(board)) {
    return;
  }
  cancelSaveTimer(ref);
  const covered = board.edits;
  const edit = boardEdit(board.document);
  update(ref, (open) => {
    open.saving = true;
  });
  const work = (async () => {
    try {
      const saved = await call("save_board", { project: ref.project, id: ref.id, board: edit });
      applySaved(ref, saved, covered);
    } catch (cause) {
      const error = toUnfoldError(cause);
      // A disconnected save stays unsaved and is written once the engine is back.
      if (error.code !== "disconnected" && error.code !== "engine_unavailable") {
        update(ref, (open) => {
          open.saveError = error.message;
        });
      }
    } finally {
      update(ref, (open) => {
        open.saving = false;
      });
    }
  })();
  inflight.set(key, work);
  try {
    await work;
  } finally {
    inflight.delete(key);
  }
}

/** What a change `saveStep` makes: the next document, and what the change reports. */
interface StepChange<T> {
  readonly document: BoardDocument;
  readonly value: T;
}

/** Why a step wasn't made. */
interface NotSaved {
  readonly ok: false;
  readonly error: UnfoldError;
}

/** How a step ended: the board as saved and what the change reported, or why it wasn't saved. */
type StepOutcome<T> = { readonly ok: true; readonly state: BoardState; readonly value: T } | NotSaved;

function notSaved(code: string, message: string): NotSaved {
  return { ok: false, error: new UnfoldError({ code, message }) };
}

/** Why a board's edits are still unsaved after a save: the save's failure, or edits made meanwhile. */
function unsavedEdits(board: OpenBoard): UnfoldError {
  return new UnfoldError({ code: "unsaved_edits", message: board.saveError ?? "The board has edits Unfold couldn't save yet. Try again in a moment." });
}

/**
 * Runs `step` once the board's unsaved edits are saved, holding the board's save slot meanwhile (no
 * other save of it starts), so a step's refusal is about the step alone. Refuses while the board's
 * edits can't be saved.
 */
async function withSaveSlot<T>(ref: BoardRef, step: (board: OpenBoard) => Promise<StepOutcome<T>>): Promise<StepOutcome<T>> {
  const key = boardKey(ref);
  // Another step may take the save slot while this one waits; then wait for that one too.
  do {
    await saveBoard(ref);
  } while (inflight.has(key));
  const board = getOpenBoard(ref);
  if (!board) {
    return notSaved("board_not_open", "This window no longer holds the board.");
  }
  if (isDirty(board)) {
    return { ok: false, error: unsavedEdits(board) };
  }
  let release: () => void = () => undefined;
  inflight.set(
    key,
    new Promise<void>((resolve) => {
      release = resolve;
    }),
  );
  try {
    return await step(board);
  } finally {
    inflight.delete(key);
    release();
  }
}

/** Has the engine write the board (a save, a repair), showing the board as saving meanwhile; a refusal is returned. */
async function engineWrite(ref: BoardRef, write: () => Promise<BoardState>): Promise<BoardState | NotSaved> {
  update(ref, (open) => {
    open.saving = true;
  });
  try {
    return await write();
  } catch (cause) {
    return { ok: false, error: toUnfoldError(cause) };
  } finally {
    update(ref, (open) => {
      open.saving = false;
    });
  }
}

/**
 * Makes one change and saves it at once rather than after the autosave delay (an agent's edit). The
 * board's unsaved edits are saved first, so a refusal is about this change alone; `change` then makes
 * the next document from the current one, which is saved, and only once the engine accepts it does it
 * become one undo step of `page`, named `name`. A refused change leaves the board as it was. When the
 * person edits while it saves, `change` runs again over their edits and that is saved, until a save
 * covers every edit, so what the change reports and the board as saved agree. Saves never overlap.
 * `change` may throw (nothing is applied then).
 */
export async function saveStep<T>(ref: BoardRef, page: PageId, name: string, change: (document: BoardDocument) => StepChange<T>): Promise<StepOutcome<T>> {
  return withSaveSlot(ref, async (board) => {
    let base = board.document;
    let next = change(base);
    if (next.document === base) {
      return { ok: true, state: savedState(board), value: next.value };
    }
    for (;;) {
      const edit = boardEdit(next.document);
      const saved = await engineWrite(ref, () => call("save_board", { project: ref.project, id: ref.id, board: edit }));
      if ("error" in saved) {
        return saved;
      }
      const now = getOpenBoard(ref);
      if (!now || now.document === base) {
        commitStep(ref, { page, name, base, document: next.document, saved });
        return { ok: true, state: saved, value: next.value };
      }
      // The person edited while it saved: the change applies again over their edits, and that is saved.
      base = now.document;
      try {
        next = change(base);
      } catch {
        return notSaved("page_changed", "The page changed while the edit was saving, and the edit no longer applies. Read the page again.");
      }
    }
  });
}

/**
 * Makes a change the engine saved the board's document: `document`, made from `base` (the board's
 * document still), as one undo step when it changed anything.
 */
function commitStep(
  ref: BoardRef,
  step: { readonly page: PageId; readonly name: string; readonly base: BoardDocument; readonly document: BoardDocument; readonly saved: BoardState },
): void {
  const { page, name, base, document, saved } = step;
  const board = getOpenBoard(ref);
  if (!board) {
    return;
  }
  const next = shared(document, saved.document);
  update(ref, (open) => {
    open.revision = saved.revision;
    open.health = shared(board.health, saved.health);
    open.label = saved.label;
    open.name = saved.name;
    open.saveError = null;
    if (document !== base) {
      pushPast(open, { document: base, page, name });
      open.history.future = [];
      open.history.coalesce = null;
    }
    if (next !== base) {
      open.document = next;
      open.edits += 1;
    }
    open.savedEdits = open.edits;
  });
}

/**
 * Has the engine change the board itself from this window's saved version (a repair), as `saveStep`
 * saves a change: the board's unsaved edits are saved first (refused as there), nothing else saves
 * while `write` runs, and the state it writes becomes one undo step of `page`, so ⌘Z takes back exactly
 * the repair (and saves that). Edits made meanwhile are kept, and saved next over the engine's change,
 * which then isn't kept (`page_changed`).
 */
export async function engineStep(ref: BoardRef, page: PageId, write: () => Promise<BoardState>): Promise<StepOutcome<null>> {
  return withSaveSlot(ref, async (board) => {
    const state = await engineWrite(ref, write);
    if ("error" in state) {
      return state;
    }
    // The new baseline; adopted as the document only when nothing was edited meanwhile.
    applySaved(ref, state, board.edits);
    const now = getOpenBoard(ref);
    if (now && now.edits !== board.edits) {
      return notSaved("page_changed", "The board was edited while the repair ran: the edits are kept, and the repair isn't. Read the page again, and repair it if it still needs it.");
    }
    if (now && now.document !== board.document) {
      update(ref, (open) => {
        pushPast(open, { document: board.document, page });
        open.history.future = [];
        open.history.coalesce = null;
        open.activePage = existingPage(open.document, open.activePage);
      });
    }
    return { ok: true, state, value: null };
  });
}

/**
 * Saves now whatever is waiting for the autosave (before closing, renaming, exporting, or deleting a
 * board). Throws `unsaved_edits` when the board still has edits that aren't saved.
 */
export async function flushBoard(ref: BoardRef): Promise<void> {
  cancelSaveTimer(ref);
  await saveBoard(ref);
  const board = getOpenBoard(ref);
  if (board && isDirty(board)) {
    throw unsavedEdits(board);
  }
}

/** Saves every open board's unsaved edits (after the engine reconnects); a failed save shows on its board. */
export async function flushAllBoards(): Promise<void> {
  await Promise.all(openBoards().map(async (board) => saveBoard(board.ref)));
}

/**
 * The page is going away: sends every unsaved board now. Nothing runs after the page hides, so this
 * can't wait for a save in flight; the newer document simply follows it.
 */
export function saveAllNow(): void {
  for (const board of openBoards()) {
    cancelSaveTimer(board.ref);
    const { project, id } = board.ref;
    if (isDirty(board)) {
      call("save_board", { project, id, board: boardEdit(board.document) }).catch(() => undefined);
    }
  }
}

// ---------------------------------------------------------------------------------------------
// Outside edits and health

/**
 * Handles `board_reloaded` with the board as its file now holds it: the session takes that version,
 * and its undo history starts over. Ignored while the board has edits not yet saved (waiting for the
 * autosave, or failing to save): the pending save, or Retry, writes the person's version over the file.
 * Ignored too while a save is in flight: the engine sends the event while holding the board's session,
 * so that save is written after the outside change, and the file holds the person's version.
 */
export function adoptOutsideChange(state: BoardState): void {
  const ref = { project: state.project, id: state.id };
  const board = getOpenBoard(ref);
  if (!board || board.readOnly || inflight.has(boardKey(ref)) || isDirty(board)) {
    return;
  }
  cancelSaveTimer(ref);
  update(ref, (open) => {
    open.history = { past: [], future: [], coalesce: null };
    open.document = state.document;
    open.revision = state.revision;
    open.health = state.health;
    open.label = state.label;
    open.name = state.name;
    open.edits += 1;
    open.savedEdits = open.edits;
    open.activePage = existingPage(state.document, open.activePage);
    viewOf(open, open.activePage);
  });
}

/** Refreshes an open board's health and label (after a graph update). */
export async function refreshHealth(ref: BoardRef): Promise<void> {
  try {
    const view = await call("board_health", { project: ref.project, id: ref.id });
    const board = getOpenBoard(ref);
    if (!board) {
      return;
    }
    const health = shared(board.health, view.health);
    if (health === board.health && view.label === board.label) {
      return;
    }
    update(ref, (open) => {
      open.health = health;
      open.label = view.label;
    });
  } catch {
    // Health is advisory; the next save or update refreshes it.
  }
}

/** The board as it was last loaded or saved (`withSaveSlot` has saved its edits). */
function savedState(board: OpenBoard): BoardState {
  return {
    project: board.ref.project,
    id: board.ref.id,
    name: board.name,
    document: board.document,
    revision: board.revision,
    health: board.health,
    label: board.label,
  };
}

/** Forgets every board and timer (tests). */
export function resetBoards(): void {
  for (const timer of saveTimers.values()) {
    clearTimeout(timer);
  }
  saveTimers.clear();
  inflight.clear();
  useBoards.setState({ boards: {} });
}
