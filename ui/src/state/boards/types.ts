/**
 * Types of the window's open boards: the document as its file holds it, plus view state
 * per page that never goes into the file (only each page's camera and lens are remembered, through
 * `save_board_view`).
 */

import type { BoardDocument, BoardState, Health, Key, PageOutline, Place, ProjectId, PublishedState, View } from "../../ipc/generated";

/** A board's id: the folder-relative path of its public file. */
export type BoardId = string;
/** A page's id, unique in its board (`page-1`). */
export type PageId = string;
/** A window's id, as the main process numbers them. */
export type WindowId = number;

/** A board: its project and its id (the folder-relative path of its public file). */
export interface BoardRef {
  readonly project: ProjectId;
  readonly id: BoardId;
}

/** One page of a board. */
export interface PageTarget {
  readonly board: BoardRef;
  readonly page: PageId;
}

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

/** What is selected on a page. */
export type Selection =
  | { readonly kind: "none" }
  /** A card, or a Flow node or statement inside it (`key`). */
  | { readonly kind: "card"; readonly cardId: string; readonly key: Key | null }
  | { readonly kind: "group"; readonly groupId: string }
  | { readonly kind: "link"; readonly linkId: string }
  | { readonly kind: "frame"; readonly frameId: string }
  /** A line between cards (its label opened into the call's contract). */
  | ({ readonly kind: "line" } & SelectedLine)
  /** An arrow from a Diagram node that stands for several calls: Selected lists them. */
  | ({ readonly kind: "calls" } & SelectedCalls);

/**
 * A selected line: its arrow, the graph edge it draws (`source` occurrence, `seq`), and its two cards.
 * A call of an arrow that stands for several opens its contract on that arrow.
 */
export interface SelectedLine {
  readonly arrowId: string;
  readonly project: ProjectId;
  readonly source: Key;
  readonly seq: number;
  /** The calling card. */
  readonly from: string;
  /** The called card. */
  readonly to: string;
  /** The calling line, when the occurrence has one. */
  readonly line: number | null;
}

/** One call of a selected arrow that stands for several: its occurrence, its edge, and its calling line. */
export interface SelectedCall {
  readonly source: Key;
  readonly seq: number;
  readonly line: number | null;
}

/** A selected arrow from a Diagram node to a card it calls more than once: the calls it stands for, in source order. */
export interface SelectedCalls {
  readonly arrowId: string;
  /** The project both cards resolve to. */
  readonly project: ProjectId;
  /** The calling card. */
  readonly from: string;
  /** The called card. */
  readonly to: string;
  readonly calls: readonly SelectedCall[];
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
  readonly selection: Selection;
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

/** View state of one page (created when the page is first shown). Survives unmounting. */
export interface PageView {
  /** The page's lens: null until the person uses the top Diagram | Flow | Code switch on this page. */
  lens: View | null;
  camera: Camera | null;
  selection: Selection;
  /** Folded outline keys per card, once the person changed them; until then the model's defaults apply. */
  codeFolds: Record<string, string[]>;
  /** Callee occurrences expanded inline (⌥-click) per card. */
  expanded: Record<string, string[]>;
  /** "▸ N checks" chains opened per card, by the chain head's key. */
  flowChains: Record<string, string[]>;
  /** Cards whose own switch was used while the lens was on: they show their own view. */
  lensExceptions: string[];
  trail: TrailItem[];
  back: FocusMove[];
  forward: FocusMove[];
  /** Measured card sizes, in canvas units. */
  sizes: Record<string, CardSize>;
  focus: FocusRequest | null;
  pendingGroup: PendingGroup | null;
  /** Link drawing mode: the next card clicked becomes the link's other end. */
  linking: { readonly from: string } | null;
  /**
   * The written types opened in the selected line's contract: each row (`send:0`, `returns`,
   * `send:0/2` for a member of the type opened under `send:0`) → the index of its opened span.
   * Cleared when the selection moves off the line.
   */
  openTypes: Record<string, number>;
}

/** One undo step: the whole document before it, and the page it changed (undo shows that page). */
export interface HistoryEntry {
  readonly document: BoardDocument;
  readonly page: PageId;
  /** The step's name, for an agent's edit ("Claude · Add cards"). */
  readonly name?: string;
}

/** Undo and redo for one board across its pages: whole documents, so the history is plain JSON. */
interface BoardHistory {
  past: HistoryEntry[];
  future: HistoryEntry[];
  /** The last coalescing edit (a drag, typing), so a burst becomes one step. */
  coalesce: { readonly key: string; readonly at: number } | null;
}

/** An outside change to a board with unsaved edits, waiting for the person to choose. */
interface BoardConflict {
  /** The revision on disk, or null when the file was deleted. */
  readonly diskRevision: string | null;
}

export interface OpenBoard {
  readonly ref: BoardRef;
  place: Place;
  name: string;
  /** The working version. Its `folder` and `projects` are Rust's (never edited or sent by the UI). */
  document: BoardDocument;
  /** The version this window last loaded or saved (what Move to New Window reports as saved). */
  savedDocument: BoardDocument;
  /** The revision of the file this window last loaded or saved. */
  revision: string;
  /** The publish state at the last open, save, or publish (the header reads `list_boards`). */
  published: PublishedState | null;
  health: Health;
  outlines: Record<PageId, PageOutline>;
  label: string | null;
  /** Edits made in this window; `savedEdits` is the count the last finished save covered. */
  edits: number;
  savedEdits: number;
  saving: boolean;
  saveError: string | null;
  conflict: BoardConflict | null;
  history: BoardHistory;
  activePage: PageId;
  pages: Record<PageId, PageView>;
  /** The page, a camera, or a lens changed since the last `save_board_view`. */
  viewDirty: boolean;
  /** Shown read-only (the render window's page, which holds no claim): nothing of it is ever saved. */
  readonly readOnly?: true;
}

/** What Move to New Window carries (plain JSON): the board as saved, its unsaved document, its history and view. */
export interface BoardSession {
  readonly state: BoardState;
  readonly document: BoardDocument;
  readonly history: { readonly past: readonly HistoryEntry[]; readonly future: readonly HistoryEntry[] };
  readonly activePage: PageId;
  readonly pages: Readonly<Record<PageId, { readonly camera: Camera | null; readonly lens: View | null }>>;
}
