/**
 * The window's open boards (UI plan §3.4–§3.6): each an `unfold-board` document plus view state per
 * page. A board is open in one window at a time, so its edits and undo history live here alone.
 *
 * - Every edit is one undo step (bursts with a `coalesce` key merge); a step remembers the page it
 *   changed, and undo or redo shows that page (V-54). One history per board, across its pages.
 * - Edits autosave a moment later through `save_board` as a `BoardEdit` (title and pages only, D-12);
 *   saves never overlap, and edits made during one are saved right after.
 * - Outside edits arrive as `board_conflict`: a board without unsaved edits reloads, one with unsaved
 *   edits gets a `conflict` for the person to resolve.
 * - Each page's camera and lens, and the active page, are saved with `save_board_view` (debounced);
 *   view changes are never undo steps.
 */

import { replaceEqualDeep } from "@tanstack/react-query";
import { produce, type Draft } from "immer";
import { create } from "zustand";
import { immer } from "zustand/middleware/immer";
import { showToast } from "../../components/Toast";
import type { BoardDocument, BoardState, BoardView, Page, PageOutline, View } from "../../ipc/generated";
import { toUnfoldError } from "../../ipc/unfold";
import { findPage } from "./document";
import { call } from "./engine";
import { boardKey } from "./keys";
import type {
  BoardRef,
  BoardSession,
  Camera,
  FocusMove,
  HistoryEntry,
  OpenBoard,
  PageId,
  PageTarget,
  PageView,
  PendingGroup,
  PendingPlacement,
  Selection,
  TrailItem,
} from "./types";

/** How long after the last edit a board saves itself. */
export const AUTOSAVE_MS = 600;
/** Edits with the same coalesce key closer together than this are one undo step. */
const COALESCE_MS = 1200;
/** Undo steps kept per board. */
export const HISTORY_LIMIT = 200;
/** How long after the last camera, lens, or page change a board's view is saved. */
export const VIEW_SAVE_MS = 1000;
/** Focus moves kept for Back, per page. */
const TRAIL_LIMIT = 100;
/** Pages whose view `state.json` keeps per board. */
const VIEW_PAGES_LIMIT = 200;
/** The camera saved for a page that has a lens but was never moved (the canvas's starting camera). */
const START_CAMERA: Camera = { x: 40, y: 40, zoom: 1 };

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

function pageView(camera: Camera | null = null, lens: View | null = null): PageView {
  return {
    lens,
    camera,
    selection: { kind: "none" },
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
    linking: null,
  };
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

/**
 * Changes a page's view state. Never an undo step; a changed camera or lens is saved with the
 * board's view (debounced).
 */
export function updatePageView(target: PageTarget, recipe: (view: Draft<PageView>) => void): void {
  const changed = { view: false };
  update(target.board, (board) => {
    const view = viewOf(board, target.page);
    const { camera, lens } = view;
    recipe(view);
    if (view.camera !== camera || view.lens !== lens) {
      board.viewDirty = true;
      changed.view = true;
    }
  });
  if (changed.view) {
    scheduleViewSave(target.board);
  }
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
    open.viewDirty = true;
  });
  scheduleViewSave(ref);
}

export function setCamera(target: PageTarget, camera: Camera): void {
  updatePageView(target, (view) => {
    view.camera = { x: camera.x, y: camera.y, zoom: camera.zoom };
  });
}

/** Sets the page's lens (⌘1, ⌘2): every card shows that view until its own switch is used. */
export function setPageLens(target: PageTarget, lens: View | null): void {
  updatePageView(target, (view) => {
    view.lens = lens;
    view.lensExceptions = [];
  });
}

/** The view a card shows: the page's lens (unless the card is an exception), its own switch, or the page default. */
export function effectiveView(lens: View | null, cardView: View | undefined, defaultView: View, exception: boolean): View {
  if (lens !== null && !exception) {
    return lens;
  }
  return cardView ?? defaultView;
}

export function select(target: PageTarget, selection: Selection): void {
  updatePageView(target, (view) => {
    view.selection = selection;
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

/** Records the current place for Back (a focus move is about to happen); clears Forward. */
export function pushFocusMove(target: PageTarget, trail?: readonly TrailItem[]): void {
  updatePageView(target, (view) => {
    const move: FocusMove = { camera: view.camera, selection: view.selection, trail: [...view.trail] };
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

/** Back or Forward: restores the selection, trail, and camera of that move, and returns it. */
export function travel(target: PageTarget, direction: "back" | "forward"): FocusMove | null {
  const view = getPageView(target);
  const move = view[direction].at(-1);
  if (!move) {
    return null;
  }
  const here: FocusMove = { camera: view.camera, selection: view.selection, trail: view.trail };
  updatePageView(target, (draft) => {
    draft[direction].pop();
    draft[direction === "back" ? "forward" : "back"].push(here as Draft<FocusMove>);
    draft.selection = move.selection;
    draft.trail = [...move.trail];
    if (move.camera) {
      draft.camera = move.camera;
    }
  });
  return move;
}

export function setPendingGroup(target: PageTarget, pending: PendingGroup | null): void {
  updatePageView(target, (view) => {
    view.pendingGroup = pending as Draft<PendingGroup> | null;
  });
}

export function setPendingPlacement(target: PageTarget, placement: PendingPlacement | null): void {
  updatePageView(target, (view) => {
    view.placement = placement as Draft<PendingPlacement> | null;
  });
}

/** Starts (from a card) or ends (null) link drawing mode on a page (V-58). */
export function setLinking(target: PageTarget, from: string | null): void {
  updatePageView(target, (view) => {
    view.linking = from === null ? null : { from };
  });
}

// ---------------------------------------------------------------------------------------------
// Opening and forgetting

function outlinesOf(outlines: readonly PageOutline[]): Record<PageId, PageOutline> {
  return Object.fromEntries(outlines.map((outline) => [outline.page, outline]));
}

function freshBoard(state: BoardState, page: PageId | null): OpenBoard {
  const saved = state.view?.pages ?? {};
  const pages: Record<PageId, PageView> = {};
  for (const { id } of state.document.pages) {
    const view = saved[id];
    if (view) {
      pages[id] = pageView({ x: view.x, y: view.y, zoom: view.zoom }, view.lens);
    }
  }
  const activePage = existingPage(state.document, page ?? state.view?.page);
  pages[activePage] ??= pageView();
  return {
    ref: { project: state.project, id: state.id },
    place: state.place,
    name: state.name,
    document: state.document,
    savedDocument: state.document,
    revision: state.revision,
    published: state.published,
    health: state.health,
    outlines: outlinesOf(state.outlines),
    label: state.label,
    edits: 0,
    savedEdits: 0,
    saving: false,
    saveError: null,
    conflict: null,
    history: { past: [], future: [], coalesce: null },
    activePage,
    pages,
    viewDirty: false,
  };
}

/**
 * Adds a board this window now holds (from `open_board`, `new_board`, or a moved session), showing
 * `page` (else the page its saved view names, else the first). With `replaces` (Publish and Move back
 * to drafts change the id), the replaced session's history and view carry over.
 */
export function adoptBoardState(state: BoardState, options: { readonly replaces?: BoardRef; readonly page?: PageId | null } = {}): BoardRef {
  const replaced = options.replaces ? getOpenBoard(options.replaces) : undefined;
  const board = freshBoard(state, options.page ?? null);
  if (replaced) {
    board.history = replaced.history;
    board.pages = replaced.pages;
    board.activePage = existingPage(state.document, replaced.activePage);
    cancelTimers(replaced.ref);
  }
  set((draft) => {
    if (replaced) {
      draft.boards = without(draft.boards, boardKey(replaced.ref));
    }
    draft.boards[boardKey(board.ref)] = board as Draft<OpenBoard>;
  });
  return board.ref;
}

/** The open boards without `key` (a dynamic `delete` would do the same on the draft). */
function without(boards: Draft<Record<string, OpenBoard>>, key: string): Draft<Record<string, OpenBoard>> {
  return Object.fromEntries(Object.entries(boards).filter(([open]) => open !== key));
}

/** Forgets a board without asking the engine (closed, moved to another window, or deleted). */
export function forgetBoard(ref: BoardRef): void {
  cancelTimers(ref);
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

/**
 * Changes a page as part of the last undo step instead of a new one (placing the cards an entrance
 * just opened once they are measured), and schedules an autosave.
 */
export function amendPage(target: PageTarget, recipe: (page: Draft<Page>) => void): void {
  const board = getOpenBoard(target.board);
  if (!board) {
    return;
  }
  const next = produce(board.document, (draft) => {
    const page = draft.pages.find((item) => item.id === target.page);
    if (page) {
      recipe(page);
    }
  });
  if (next === board.document) {
    return;
  }
  update(target.board, (open) => {
    open.document = next;
    open.edits += 1;
  });
  scheduleSave(target.board);
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
  const back: HistoryEntry = { document: board.document, page: entry.page };
  const page = existingPage(entry.document, entry.page);
  update(ref, (open) => {
    open.history[from].pop();
    open.history[from === "past" ? "future" : "past"].push(back);
    open.history.coalesce = null;
    open.document = entry.document;
    open.edits += 1;
    if (open.activePage !== page) {
      open.activePage = page;
      open.viewDirty = true;
    }
    viewOf(open, page);
  });
  scheduleSave(ref);
  if (board.activePage !== page) {
    scheduleViewSave(ref);
  }
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
const viewTimers = new Map<string, ReturnType<typeof setTimeout>>();
const inflight = new Map<string, Promise<void>>();

function cancelSaveTimer(ref: BoardRef): void {
  const key = boardKey(ref);
  const timer = saveTimers.get(key);
  if (timer !== undefined) {
    clearTimeout(timer);
    saveTimers.delete(key);
  }
}

function cancelViewTimer(ref: BoardRef): void {
  const key = boardKey(ref);
  const timer = viewTimers.get(key);
  if (timer !== undefined) {
    clearTimeout(timer);
    viewTimers.delete(key);
  }
}

function cancelTimers(ref: BoardRef): void {
  cancelSaveTimer(ref);
  cancelViewTimer(ref);
}

/** Saves a moment after the last change; repeated changes push the save back. */
function scheduleSave(ref: BoardRef): void {
  cancelSaveTimer(ref);
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

/** What `save_board` takes: the title and pages, never `folder` or `projects` (Rust's, D-12). */
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
  const outlines = shared(board.outlines, outlinesOf(saved.outlines));
  update(ref, (open) => {
    open.revision = saved.revision;
    open.health = health;
    open.outlines = outlines;
    open.label = saved.label;
    open.name = saved.name;
    open.place = saved.place;
    open.published = saved.published;
    open.savedEdits = Math.max(open.savedEdits, coveredEdits);
    open.saveError = null;
    open.savedDocument = (open.edits === coveredEdits ? document : saved.document);
    if (open.edits === coveredEdits) {
      open.document = document;
    }
  });
}

/**
 * Adopts a state the engine wrote itself from this window's saved version (a repair): the new
 * baseline, with the session kept. The change is one undo step, so ⌘Z takes back exactly the repair
 * (and saves that). Call after a flush, so nothing is unsaved.
 */
export function applyServerState(ref: BoardRef, state: BoardState, page?: PageId): void {
  const board = getOpenBoard(ref);
  if (!board) {
    return;
  }
  const before: HistoryEntry = { document: board.document, page: page ?? board.activePage };
  applySaved(ref, state, board.edits);
  if (getOpenBoard(ref)?.document !== before.document) {
    update(ref, (open) => {
      pushPast(open, before);
      open.history.future = [];
      open.history.coalesce = null;
      open.activePage = existingPage(open.document, open.activePage);
    });
  }
}

/**
 * Writes the board's unsaved edits now (`force` keeps this window's version over an outside edit).
 * Saves of one board never overlap: a save requested during another waits for it and then writes
 * whatever is still unsaved.
 */
export async function saveBoard(ref: BoardRef, options: { readonly force?: boolean } = {}): Promise<void> {
  const key = boardKey(ref);
  // Several callers may be waiting on the same save; the first to resume starts the next one and the
  // others wait for that too, so two saves of one board never run at once.
  for (let running = inflight.get(key); running; running = inflight.get(key)) {
    await running;
  }
  const board = getOpenBoard(ref);
  const force = options.force === true;
  if (!board || (!isDirty(board) && !force)) {
    return;
  }
  // A conflict waits for the person; only an explicit choice (`force`) writes over it.
  if (board.conflict && !force) {
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
      const saved = await call("save_board", { project: ref.project, id: ref.id, board: edit, ...(force ? { force: true } : {}) });
      applySaved(ref, saved, covered);
      if (force) {
        update(ref, (open) => {
          open.conflict = null;
        });
      }
    } catch (cause) {
      const error = toUnfoldError(cause);
      // A disconnected save stays unsaved and is written once the engine is back.
      const offline = error.code === "disconnected" || error.code === "engine_unavailable";
      update(ref, (open) => {
        if (error.code === "board_conflict") {
          open.conflict ??= { diskRevision: null };
        } else if (!offline) {
          open.saveError = error.message;
        }
      });
      if (error.code !== "board_conflict" && !offline) {
        showToast({ title: `Couldn't save “${board.document.title}”`, body: error.message, tone: "danger" });
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

/** Saves now whatever is waiting for the autosave (before closing, moving, or publishing a board). */
export async function flushBoard(ref: BoardRef): Promise<void> {
  cancelSaveTimer(ref);
  await saveBoard(ref);
}

/** What `save_board_view` takes: the active page, and each page's camera and lens that has either. */
function boardView(board: OpenBoard): BoardView {
  const pages: BoardView["pages"] = {};
  let count = 0;
  for (const { id } of board.document.pages) {
    const view = board.pages[id];
    if (!view || (view.camera === null && view.lens === null) || count >= VIEW_PAGES_LIMIT) {
      continue;
    }
    const camera = view.camera ?? START_CAMERA;
    pages[id] = { x: camera.x, y: camera.y, zoom: camera.zoom, lens: view.lens };
    count += 1;
  }
  return { page: board.activePage, pages };
}

function scheduleViewSave(ref: BoardRef): void {
  cancelViewTimer(ref);
  viewTimers.set(
    boardKey(ref),
    setTimeout(() => {
      viewTimers.delete(boardKey(ref));
      void flushBoardView(ref);
    }, VIEW_SAVE_MS),
  );
}

/** Saves the board's view (active page, cameras, lenses) now if it changed. */
export async function flushBoardView(ref: BoardRef): Promise<void> {
  cancelViewTimer(ref);
  const board = getOpenBoard(ref);
  if (!board?.viewDirty) {
    return;
  }
  update(ref, (open) => {
    open.viewDirty = false;
  });
  try {
    await call("save_board_view", { project: ref.project, id: ref.id, view: boardView(board) });
  } catch (error) {
    // Kept for the next flush (the view is a convenience: nothing else waits on it).
    update(ref, (open) => {
      open.viewDirty = true;
    });
    console.warn("Saving the board's view failed:", error);
  }
}

/** Saves every open board's unsaved edits and view (before the window restores or reconnects). */
export async function flushAllBoards(): Promise<void> {
  await Promise.all(openBoards().map(async (board) => Promise.all([flushBoard(board.ref), flushBoardView(board.ref)])));
}

/**
 * The page is going away: sends every unsaved board and view now. Nothing runs after the page hides,
 * so this can't wait for a save in flight; the newer document simply follows it.
 */
export function saveAllNow(): void {
  for (const board of openBoards()) {
    cancelTimers(board.ref);
    const { project, id } = board.ref;
    if (isDirty(board) && !board.conflict) {
      call("save_board", { project, id, board: boardEdit(board.document) }).catch(() => undefined);
    }
    if (board.viewDirty) {
      call("save_board_view", { project, id, view: boardView(board) }).catch(() => undefined);
    }
  }
}

// ---------------------------------------------------------------------------------------------
// Outside edits and health

/** Replaces the session's document with a state read from disk, as one undo step (so it can be taken back). */
function adoptReloaded(ref: BoardRef, state: BoardState): void {
  cancelSaveTimer(ref);
  update(ref, (open) => {
    pushPast(open, { document: open.document, page: open.activePage });
    open.history.future = [];
    open.history.coalesce = null;
    open.document = state.document;
    open.savedDocument = state.document;
    open.revision = state.revision;
    open.published = state.published;
    open.health = state.health;
    open.outlines = outlinesOf(state.outlines);
    open.label = state.label;
    open.place = state.place;
    open.name = state.name;
    open.edits += 1;
    open.savedEdits = open.edits;
    open.conflict = null;
    open.saveError = null;
    open.activePage = existingPage(state.document, open.activePage);
    viewOf(open, open.activePage);
  });
}

/** Reads the board from disk again, replacing this window's document (Take theirs). */
async function reloadBoard(ref: BoardRef): Promise<void> {
  const running = inflight.get(boardKey(ref));
  if (running) {
    await running;
  }
  adoptReloaded(ref, await call("reload_board", { project: ref.project, id: ref.id }));
}

/** Drops a published board's unpublished changes (the session returns to the published file). */
export async function discardChanges(ref: BoardRef): Promise<void> {
  cancelSaveTimer(ref);
  const running = inflight.get(boardKey(ref));
  if (running) {
    await running;
  }
  adoptReloaded(ref, await call("discard_changes", { project: ref.project, id: ref.id }));
}

/** The person's answer to a conflict: take the file on disk, or write this window's version over it. */
export async function resolveConflict(ref: BoardRef, keep: "mine" | "theirs"): Promise<void> {
  if (keep === "theirs") {
    await reloadBoard(ref);
  } else {
    await saveBoard(ref, { force: true });
  }
}

/** Handles `board_conflict`: reloads a clean board, or asks about one with unsaved edits. */
export async function handleOutsideChange(ref: BoardRef, diskRevision: string | null): Promise<void> {
  const board = getOpenBoard(ref);
  if (!board || board.revision === diskRevision) {
    return;
  }
  const running = inflight.get(boardKey(ref));
  if (running) {
    await running;
  }
  const current = getOpenBoard(ref);
  if (!current || current.revision === diskRevision) {
    return;
  }
  if (!isDirty(current) && diskRevision !== null) {
    try {
      await reloadBoard(ref);
      return;
    } catch {
      // Fall through: the person decides.
    }
  }
  update(ref, (open) => {
    open.conflict = { diskRevision };
  });
}

/** Refreshes an open board's health, outlines, and label (after a graph update). */
export async function refreshHealth(ref: BoardRef): Promise<void> {
  try {
    const view = await call("board_health", { project: ref.project, id: ref.id });
    const board = getOpenBoard(ref);
    if (!board) {
      return;
    }
    const health = shared(board.health, view.health);
    const outlines = shared(board.outlines, outlinesOf(view.outlines));
    if (health === board.health && outlines === board.outlines && view.label === board.label) {
      return;
    }
    update(ref, (open) => {
      open.health = health;
      open.outlines = outlines;
      open.label = view.label;
    });
  } catch {
    // Health is advisory; the next save or update refreshes it.
  }
}

// ---------------------------------------------------------------------------------------------
// Sessions (Move to New Window)

/** The board with its unsaved document, undo history, and each page's camera and lens, as plain JSON. */
export function exportSession(ref: BoardRef): BoardSession | null {
  const board = getOpenBoard(ref);
  if (!board) {
    return null;
  }
  return {
    state: {
      project: board.ref.project,
      id: board.ref.id,
      place: board.place,
      name: board.name,
      document: board.savedDocument,
      revision: board.revision,
      published: board.published,
      health: board.health,
      outlines: Object.values(board.outlines),
      label: board.label,
      view: null,
    },
    document: board.document,
    history: { past: [...board.history.past], future: [...board.history.future] },
    activePage: board.activePage,
    pages: Object.fromEntries(Object.entries(board.pages).map(([id, view]) => [id, { camera: view.camera, lens: view.lens }])),
  };
}

/** Opens a moved board with its history and views; unsaved edits from the other window save here. */
export function adoptSession(session: BoardSession): BoardRef {
  const ref = adoptBoardState(session.state, { page: session.activePage });
  // Structured cloning keeps no identity across windows, so unsaved edits show as a different document.
  const dirty = JSON.stringify(session.document) !== JSON.stringify(session.state.document);
  update(ref, (open) => {
    open.document = session.document;
    open.history.past = [...session.history.past] as Draft<HistoryEntry>[];
    open.history.future = [...session.history.future] as Draft<HistoryEntry>[];
    for (const [id, view] of Object.entries(session.pages)) {
      open.pages[id] = pageView(view.camera, view.lens) as Draft<PageView>;
    }
    open.activePage = existingPage(session.document, session.activePage);
    viewOf(open, open.activePage);
    if (dirty) {
      open.edits += 1;
    }
  });
  if (dirty) {
    scheduleSave(ref);
  }
  return ref;
}

/** Forgets every board and timer (tests). */
export function resetBoards(): void {
  for (const timer of [...saveTimers.values(), ...viewTimers.values()]) {
    clearTimeout(timer);
  }
  saveTimers.clear();
  viewTimers.clear();
  inflight.clear();
  useBoards.setState({ boards: {} });
}
