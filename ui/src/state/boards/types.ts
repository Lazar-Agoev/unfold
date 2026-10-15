/**
 * Types of the open boards: the document as its file holds it, plus view state per page that never
 * goes into the file and lasts only while the app runs.
 */

import type { BoardDocument, Health, Key, ProjectId } from "../../ipc/generated";

/** A board's id: the folder-relative path of its file. */
type BoardId = string;
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
  /** A card, or a block drawn in it (`key`), in its own flow or in a call it opens in place. */
  | { readonly kind: "card"; readonly cardId: string; readonly key: Key | null }
  /** A field of a type card: the type that writes it (the card's, or a base it inherits from) and its index among that type's written members. */
  | { readonly kind: "field"; readonly cardId: string; readonly key: Key; readonly index: number }
  | { readonly kind: "combined"; readonly combinedId: string }
  /** A connection. */
  | { readonly kind: "link"; readonly linkId: string }
  | { readonly kind: "frame"; readonly frameId: string }
  | { readonly kind: "resource"; readonly resourceId: string }
  | { readonly kind: "note"; readonly noteId: string }
  /** A proposed step. */
  | { readonly kind: "proposal"; readonly proposalId: string }
  /** A call's arrow between cards (Selected shows the call's contract). */
  | ({ readonly kind: "line" } & SelectedLine)
  /** An arrow that stands for several calls to one card: Selected lists them. */
  | ({ readonly kind: "calls" } & SelectedCalls);

/** A selected arrow of one call: the call's block key and the two cards it joins. */
export interface SelectedLine {
  readonly arrowId: string;
  readonly call: Key;
  /** The calling card. */
  readonly from: string;
  /** The called card. */
  readonly to: string;
}

/** A selected arrow that stands for several calls to one card: their block keys, in the order they run. */
export interface SelectedCalls {
  readonly arrowId: string;
  /** The calling card. */
  readonly from: string;
  /** The called card. */
  readonly to: string;
  readonly calls: readonly Key[];
}

/** A request for the canvas to bring cards into view once they are measured. */
interface FocusRequest {
  readonly seq: number;
  readonly cardIds: readonly string[];
  /** `center` puts one card in view at a readable zoom; `fit` shows all of `cardIds` (all cards when empty). */
  readonly mode: "center" | "fit";
}

/** View state of one page (created when the page is first shown). Survives unmounting. */
export interface PageView {
  camera: Camera | null;
  selection: Selection;
  /** Measured card sizes, in canvas units. */
  sizes: Record<string, CardSize>;
  /** Measured ports (and side and column handles): each item's, by handle id, below its top, in canvas units. */
  ports: Record<string, Record<string, number>>;
  focus: FocusRequest | null;
  /** A link being drawn: the item it starts at, and (once clicked) the item it ends at, waiting for its label. */
  linking: Linking | null;
}

/** A link being drawn by hand (`startLink`). */
interface Linking {
  readonly from: string;
  /** The call it leaves from, when it starts at a call of the from card (it becomes the link's step). */
  readonly step?: Key;
  readonly to: string | null;
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

/**
 * Why a board stopped saving: its file on disk isn't a valid board (`invalid`, with why), or it changed
 * while the board had edits of its own, a save on its way, or a failing save (`changed`).
 */
export type Pause = { readonly kind: "invalid"; readonly message: string } | { readonly kind: "changed" };

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
  /** Why the last save failed, until one succeeds; while set, an outside change asks the person rather than reloading. */
  saveError: string | null;
  /** Why saving stopped until the person chooses which version to keep, while it has. */
  paused: Pause | null;
  history: BoardHistory;
  activePage: PageId;
  pages: Record<PageId, PageView>;
  /** Shown read-only (the render window's page, which the engine doesn't hold open): nothing of it is ever saved. */
  readonly readOnly?: true;
}
