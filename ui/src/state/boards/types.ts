/** Types of the window's open boards: the document as its file holds it, plus view state that never goes into the file. */

import type { BoardDocument, BoardOutline, BoardState, Health, Place, View } from "../../ipc/generated";

/** The canvas camera (React Flow's viewport). */
export interface Camera {
  readonly x: number;
  readonly y: number;
  readonly zoom: number;
}

export interface CardSize {
  readonly width: number;
  readonly height: number;
}

/** One step of the breadcrumb trail ("POST /orders › createOrder"). */
export interface TrailItem {
  readonly cardId: string;
  readonly label: string;
  /** The calling line in this card that led to the next step. */
  readonly line?: number;
}

/** Where the person was before a focus move, for Back and Forward. */
export interface FocusMove {
  readonly camera: Camera | null;
  readonly selectedCardId: string | null;
  readonly trail: readonly TrailItem[];
}

/** A request for the canvas to bring cards into view once they are measured. */
export interface FocusRequest {
  readonly seq: number;
  readonly cardIds: readonly string[];
  /** `center` puts one card in view at a readable zoom; `fit` shows all of `cardIds` (all cards when empty). */
  readonly mode: "center" | "fit";
}

/** A gutter selection waiting for its group title (the magenta rail). */
export interface PendingGroup {
  readonly cardId: string;
  readonly members: readonly string[];
  readonly error?: string;
}

/**
 * Handler cards an entrance just opened, waiting for the entrance card to be measured so each
 * can be placed level with its handler row (architecture §10.2).
 */
export interface PendingPlacement {
  readonly entranceId: string;
  /** The handler cards, in registration order, with the keys their rows resolve to. */
  readonly handlers: readonly { readonly cardId: string; readonly key: string }[];
}

/** View state: the lens exceptions, folds, peeks, camera, and trail. Never saved into the board file. */
export interface BoardViewState {
  /**
   * The window lens as it applies to this board: null until the person uses the top Code/Flow
   * switch, then that view for every card (a lens, never an edit of the board).
   */
  lens: View | null;
  camera: Camera | null;
  selectedCardId: string | null;
  /** The block selected inside the selected card (a statement or Flow node), if any. */
  selectedKey: string | null;
  /** Folded outline keys per card, once the person changed them; until then the model's defaults apply. */
  codeFolds: Record<string, string[]>;
  /** Callee occurrences expanded inline (⌥-click) per card. */
  expanded: Record<string, string[]>;
  /** "▸ N checks" chains opened per card, by the chain head's key. */
  flowChains: Record<string, string[]>;
  /** Cards whose own switch was used while the window lens was on: they show their own view. */
  lensExceptions: string[];
  trail: TrailItem[];
  back: FocusMove[];
  forward: FocusMove[];
  /** Measured card sizes, in canvas units. */
  sizes: Record<string, CardSize>;
  focus: FocusRequest | null;
  pendingGroup: PendingGroup | null;
  placement: PendingPlacement | null;
}

/** An outside change to a board with unsaved edits, waiting for the person to choose. */
export interface BoardConflict {
  /** The revision on disk, or null when the file was deleted. */
  readonly diskRevision: string | null;
}

/** Undo and redo for one board: whole documents, so the history is plain JSON (Move to New Window). */
export interface BoardHistory {
  past: BoardDocument[];
  future: BoardDocument[];
  /** The last coalescing edit (a drag, typing), so a burst becomes one step. */
  coalesce: { readonly key: string; readonly at: number } | null;
}

export interface OpenBoard {
  readonly id: string;
  place: Place;
  name: string;
  document: BoardDocument;
  /** The file revision this window last loaded or saved. */
  revision: string;
  health: Health;
  outline: BoardOutline;
  label: string | null;
  /** Edits made in this window; `savedEdits` is the count the last finished save covered. */
  edits: number;
  savedEdits: number;
  saving: boolean;
  saveError: string | null;
  conflict: BoardConflict | null;
  history: BoardHistory;
  view: BoardViewState;
}

/** What Move to New Window carries: the board as saved, its unsaved document, and its undo history. */
export interface BoardSession {
  readonly state: BoardState;
  readonly document: BoardDocument;
  readonly history: { readonly past: readonly BoardDocument[]; readonly future: readonly BoardDocument[] };
}
