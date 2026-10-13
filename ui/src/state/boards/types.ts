/**
 * Types of the open boards: the document as its file holds it, plus view state per page that never
 * goes into the file and lasts only while the app runs.
 */

import type { BoardDocument, Health, Key, ProjectId, View } from "../../ipc/generated";

/** A board's id: the folder-relative path of its file. */
export type BoardId = string;
/** A page's id, unique in its board (`page-1`). */
export type PageId = string;

/** A board: its project and its id (the folder-relative path of its file). */
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
  /**
   * A card, or a Flow node or statement inside it (`key`). `owner` is the block whose Flow holds the
   * step when it is shown in a call expanded in place (absent: the card's own block); `last` ends a
   * Shift-selected run of steps that starts at `key`.
   */
  | { readonly kind: "card"; readonly cardId: string; readonly key: Key | null; readonly owner?: Key; readonly last?: Key }
  | { readonly kind: "group"; readonly groupId: string }
  /** A drawn link or a connection. */
  | { readonly kind: "link"; readonly linkId: string }
  | { readonly kind: "frame"; readonly frameId: string }
  | { readonly kind: "resource"; readonly resourceId: string }
  | { readonly kind: "note"; readonly noteId: string }
  /** A proposed step. */
  | { readonly kind: "proposal"; readonly proposalId: string }
  /** A call's arrow between cards (Selected shows the call's contract). */
  | ({ readonly kind: "line" } & SelectedLine)
  /** An arrow from a Diagram node that stands for several calls: Selected lists them. */
  | ({ readonly kind: "calls" } & SelectedCalls);

/**
 * A selected line: its arrow, the graph edge it draws (`source` occurrence, `seq`), and its two cards.
 * A call chosen from an arrow that stands for several keeps that arrow.
 */
export interface SelectedLine {
  readonly arrowId: string;
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
interface SelectedCall {
  readonly source: Key;
  readonly seq: number;
  readonly line: number | null;
}

/** A selected arrow from a Diagram node to a card it calls more than once: the calls it stands for, in source order. */
export interface SelectedCalls {
  readonly arrowId: string;
  /** The calling card. */
  readonly from: string;
  /** The called card. */
  readonly to: string;
  readonly calls: readonly SelectedCall[];
}

/** A request for the canvas to bring cards into view once they are measured. */
export interface FocusRequest {
  readonly seq: number;
  readonly cardIds: readonly string[];
  /** `center` puts one card in view at a readable zoom; `fit` shows all of `cardIds` (all cards when empty). */
  readonly mode: "center" | "fit";
}

/** View state of one page (created when the page is first shown). Survives unmounting. */
export interface PageView {
  /**
   * The page's lens: null until the person uses the top Diagram | Code switch on this page (`pageLens`
   * reads it; a Flow lens shows Diagram).
   */
  lens: View | null;
  camera: Camera | null;
  selection: Selection;
  /** Folded outline keys per card, once the person changed them; until then the model's defaults apply. */
  codeFolds: Record<string, string[]>;
  /** Callee occurrences expanded inline in Code (⌥-click) per card; Diagram's are saved on the card (`Card.expanded`). */
  expanded: Record<string, string[]>;
  /** "▸ N checks" chains opened per card, by the chain head's key. */
  flowChains: Record<string, string[]>;
  /** Measured card sizes, in canvas units. */
  sizes: Record<string, CardSize>;
  focus: FocusRequest | null;
  /** Link drawing mode: the next card clicked becomes the link's other end. */
  linking: { readonly from: string } | null;
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

export interface OpenBoard {
  readonly ref: BoardRef;
  name: string;
  /** The board as this window edits it. Its `folder` is Rust's (never edited or sent by the UI). */
  document: BoardDocument;
  /** The revision of the file last loaded or saved. */
  revision: string;
  health: Health;
  label: string | null;
  /** Edits made in this window; `savedEdits` is the count the last finished save covered. */
  edits: number;
  savedEdits: number;
  saving: boolean;
  /** Why the last save failed, until one succeeds; while set, outside changes don't reload the board. */
  saveError: string | null;
  history: BoardHistory;
  activePage: PageId;
  pages: Record<PageId, PageView>;
  /** Shown read-only (the render window's page, which the engine doesn't hold open): nothing of it is ever saved. */
  readonly readOnly?: true;
}
