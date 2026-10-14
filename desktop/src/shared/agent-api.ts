/**
 * The agent channel: how the main process has the window apply an agent's edits
 * to a board it holds, read a page it holds, load a board's session without showing it, create a
 * board, change a board's pages and title, show a board to the person, apply a health repair, and end
 * a piece of work, and what the window reports about its view. Types only, so the UI imports them as
 * `@unfold/desktop/agent-api`.
 *
 * - main → window: the shell events of {@link AgentShellEvent};
 * - window → main: the window commands of {@link AgentWindowCommands}.
 *
 * Settings reads the state of the MCP server agents connect to ({@link AgentServerStatus}).
 *
 * The main process owns argument parsing, links, defaults, and page revisions; the window owns
 * placement, the document's rules, and the refusal texts. Every edit is one undo step of the board,
 * saved at once, and signed with the agent's identity.
 */

/** A board: its project's id (`ProjectId`) and the board's id (its file's path in the project, under `.unfold/boards/`). */
export interface AgentBoardRef {
  readonly project: string;
  readonly id: string;
}

/** Who an edit is from: `{author: "agent:claude", name: "Claude"}`. An agent never writes as a person. */
export interface AgentIdentity {
  readonly author: `agent:${string}`;
  /** How the app shows it: "Claude", "Codex". */
  readonly name: string;
}

/** How a card shows, as board files save it: `compact` or `expanded` (a page's default view too). */
export type AgentView = "compact" | "expanded";

/**
 * A call or callback opened in place on a card (the board's `open`): 1 to 3 block keys, outermost
 * first, each a call or callback of the definition drawn at that level.
 */
export type AgentCallPath = readonly string[];

/** What a card shows (the board's card `ref`): a module, definition, or type, or an app's start or an entrance. */
export type AgentCardRef = { readonly block: string } | { readonly entrance: string };

/** A page as its board file holds it (the UI's generated `Page`); plain JSON on this side. */
export interface AgentPage {
  readonly id: string;
  readonly title: string;
  readonly [field: string]: unknown;
}

// ---------------------------------------------------------------------------------------------
// Operations: one per write tool, with resolved arguments.

/** One card `add_cards` puts on a page. */
export interface AgentCardSpec {
  /** What it shows. */
  readonly ref: AgentCardRef;
  /** Its own view (types reached by following its fields too); absent: the page's default view. */
  readonly view?: AgentView;
  /** The id of an item on the page to place it beside; absent: beside the cards it relates to. */
  readonly near?: string;
  /** The id of a frame of the page the cards it adds (following included) go into; cards already on the page stay where they are. */
  readonly frame?: string;
  /**
   * Adds its call tree as well, following resolved calls only: what it calls (0–3 levels) and what
   * calls it (0–2 levels); and, for a type or class, the types its fields name, transitively (0–2
   * levels). The result says where following stopped.
   */
  readonly follow?: { readonly callees?: number; readonly callers?: number; readonly types?: number };
}

/** Puts cards on the page, near the cards they relate to; cards already there are kept, not duplicated. */
export interface AddCardsOp {
  readonly op: "add_cards";
  readonly cards: readonly AgentCardSpec[];
}

/** How one item shows: a card compact or expanded, or whether a combined step or a frame is collapsed. */
export interface AgentViewSpec {
  /** A card's, a combined step's, or a frame's id. */
  readonly id: string;
  /** A card: compact or expanded. Type cards keep their fields and refuse it. */
  readonly view?: AgentView;
  /** Whether a combined step or a frame is collapsed. */
  readonly collapsed?: boolean;
}

/** Sets how cards, combined steps, and frames show. */
export interface SetViewOp {
  readonly op: "set_view";
  readonly items: readonly AgentViewSpec[];
}

/**
 * Where a card, resource card, note, or frame goes after Arrange: a canvas position (a frame's: its
 * box's top-left), or just right of or below another item. A frame moves with everything in it; an
 * attached note stays attached, at a new offset.
 */
export type AgentMove = { readonly id: string } & (
  | { readonly at: readonly [number, number] }
  | { readonly rightOf: string }
  | { readonly below: string }
);

/** The way a frame, or the page, lays its items out. */
export type AgentDirection = "right" | "down";

/**
 * Arranges the page from the inside out (each frame, deepest first, by call depth and connections in
 * its direction; then the page, each frame one node), or only the frame `frame`, whose box keeps its
 * top-left; then applies the moves in order. `direction` lays out the page's top level for this call,
 * or, with `frame`, becomes that frame's direction.
 */
export interface ArrangeOp {
  readonly op: "arrange";
  readonly frame?: string;
  readonly direction?: AgentDirection;
  readonly moves?: readonly AgentMove[];
}

/**
 * Takes items off the page: cards (with the connections that end at them), combined steps,
 * connections, frames, resource cards (with the connections that end at them), notes, and proposed
 * steps (never code). A removed frame's members stay, in the frame that held it, unless `keepMembers` is false,
 * which removes them too. What would dangle goes as well: a frame left with no members, and the links
 * to it; a note attached to a removed item stays where it was, free. Refused when that removes text a
 * person wrote, unless `replacePeopleText`.
 */
export interface RemoveItemsOp {
  readonly op: "remove_items";
  readonly ids: readonly string[];
  readonly keepMembers?: boolean;
  readonly replacePeopleText?: boolean;
}

/** How a frame is drawn: an outlined box with a title bar, or a tinted zone with a large title. */
export type AgentFrameLook = "box" | "zone";

/** What a frame is, as its icon. */
export type AgentFrameIcon = "api" | "worker" | "schedule" | "cli" | "web" | "library" | "system";

/** One frame `frame_cards` creates or updates. */
export interface AgentFrameSpec {
  /** An existing frame's id updates it; a new id names the new frame; absent: one from the title. */
  readonly id?: string;
  readonly title: string;
  readonly description?: string;
  /**
   * The ids of the cards, resource cards, notes, and frames it holds, in order (at least one). An
   * item can be in one frame: members may come from no frame, from this frame, or from `parent`. On
   * an update, the items it held that aren't listed move to the frame that holds it (or out of every
   * frame).
   */
  readonly members: readonly string[];
  /** Absent: `box` for a new frame, unchanged for an existing one. */
  readonly look?: AgentFrameLook;
  readonly icon?: AgentFrameIcon;
  /** The frame to nest it in; null takes it out of the frame that holds it; absent: unchanged (the page for a new frame). */
  readonly parent?: string | null;
  readonly direction?: AgentDirection;
  readonly collapsed?: boolean;
}

/** Gathers items of a page into frames, or updates frames by id. Frames nest at most three deep. */
export interface FrameCardsOp {
  readonly op: "frame_cards";
  readonly frames: readonly AgentFrameSpec[];
  readonly replacePeopleText?: boolean;
}

/** Writes descriptions of what isn't a card: combined steps, frames, resource cards, and connections (a card's is `shape_card`'s). */
export interface DescribeOp {
  readonly op: "describe";
  readonly items: readonly { readonly id: string; readonly text: string }[];
  readonly replacePeopleText?: boolean;
}

/** What a resource card stands for: a table, a queue, a cache, storage, a service outside the project, or who starts things. */
export type AgentResourceKind = "table" | "queue" | "cache" | "storage" | "external" | "actor";

/**
 * What an authored claim cites: a block's or an entrance's key, or lines of an evidence
 * file (relative to the project's folder; 1-based and inclusive).
 */
export type AgentAnchor = { readonly key: string } | { readonly file: string; readonly lines: readonly [number, number] };

/**
 * A column of a table card: its name, its type as written, and its marks (primary key, foreign key
 * with what it `references`, unique; optional). A table backed by a type holds marks only (`name`,
 * `key`, `references`), naming fields of the type.
 */
export interface AgentColumn {
  readonly name: string;
  readonly type?: string;
  readonly key?: "primary" | "foreign" | "unique";
  readonly references?: string;
  readonly optional?: boolean;
}

/** One resource card `add_resources` creates or updates. */
export interface AgentResourceSpec {
  /**
   * An existing resource card's id updates it; a new id names the new card; absent: the card of the
   * same kind and name already on the page, else a new one with an id from the name.
   */
  readonly id?: string;
  readonly kind: AgentResourceKind;
  /** The technical name (`orders`); it must appear in what the card cites. */
  readonly name: string;
  /** A table's type: a type declaration's or class's key, whose fields are its columns. */
  readonly type?: string;
  readonly description?: string;
  /** A table's columns: its own (without `type`), or marks on the type's fields. On an update, `type` and `columns` are replaced together when either is given. */
  readonly columns?: readonly AgentColumn[];
  /** What it cites; on an update, replaced when given. */
  readonly anchors?: readonly AgentAnchor[];
  /** The id of a frame of the page to put it in. */
  readonly frame?: string;
  /** The id of an item of the page to place it beside; absent: right of what its frame holds, else right of everything (Arrange then places it by data flow). */
  readonly near?: string;
}

/** Adds resource cards to a page, or updates them by id. */
export interface AddResourcesOp {
  readonly op: "add_resources";
  readonly resources: readonly AgentResourceSpec[];
  readonly replacePeopleText?: boolean;
}

/** What a connection says; `other` needs a label. */
export type AgentConnectionKind = "http" | "queue" | "reads" | "writes" | "calls" | "relates" | "other";

/** How many rows of one table a row of the other relates to (`relates` only). */
export type AgentCardinality = "one-to-one" | "one-to-many" | "many-to-one" | "many-to-many";

/** One connection `connect` draws or updates. */
export interface AgentConnectionSpec {
  /**
   * An existing link's id updates it; a new id names the new connection; absent: the connection of
   * the same kind, ends, step, and columns already on the page, else a new one.
   */
  readonly id?: string;
  readonly kind: AgentConnectionKind;
  /** The id of the side that acts: a card, a resource card, or a frame of the page. */
  readonly from: string;
  /** The id of the other side: a card, a resource card, or a frame of the page. */
  readonly to: string;
  /** The block key of the call it leaves from, inside `from`'s block. */
  readonly step?: string;
  /** `relates` only: the columns it joins. */
  readonly fromColumn?: string;
  readonly toColumn?: string;
  readonly cardinality?: AgentCardinality;
  /** What travels (a route, a queue, an event); filled in by the save when `to` is an entrance. */
  readonly channel?: string;
  /** The key of the type it carries. */
  readonly carries?: string;
  /** A short label for what travels; on an update, replaced when given. */
  readonly label?: string;
  /** The evidence in words; on an update, replaced when given. */
  readonly description?: string;
  /** What it cites on the receiving side. */
  readonly anchors?: readonly AgentAnchor[];
}

/**
 * Draws connections, or updates them by id: everything a connection says (its kind, ends, step,
 * columns, cardinality, channel, carried type, and anchors) is replaced as a whole.
 */
export interface ConnectOp {
  readonly op: "connect";
  readonly connections: readonly AgentConnectionSpec[];
  readonly replacePeopleText?: boolean;
}

/** One note `add_notes` adds or updates. */
export interface AgentNoteSpec {
  /** An existing note's id updates it; a new id names the new note; absent: the same text on the same item is kept, else a new note. */
  readonly id?: string;
  /** Plain text, 1 to 400 characters. */
  readonly text: string;
  /** The id of a card, frame, resource card, or link of the page it moves with; absent: free (on an update: where it is). */
  readonly attach?: string;
  /** The id of an item of the page to place a free note beside; absent: beside what it's attached to, else right of everything. */
  readonly near?: string;
}

/** Adds notes to a page, attached or free, or updates them by id. */
export interface AddNotesOp {
  readonly op: "add_notes";
  readonly notes: readonly AgentNoteSpec[];
  readonly replacePeopleText?: boolean;
}

/**
 * Where a proposed step goes (the board's proposal `after | start`): after a block (`after`, its key),
 * or at the start of the body of a definition or a part (`start`, its key).
 */
export type AgentProposalSpot = { readonly after: string } | { readonly start: string };

/**
 * One proposed step `propose` creates or updates: an intended addition, never code. A new one needs a
 * spot and `title`; an update names its `id`, and a new spot moves it (the explicit placement repair,
 * also for an anchor gone missing). It applies page-wide, wherever its block is drawn.
 */
export type AgentProposalSpec = {
  /** An existing proposal's id updates it; a new id names the new one; absent: the page's proposal with the same spot and title, else a new one. */
  readonly id?: string;
  /** What it would do, in words: one line, at most 120 characters. */
  readonly title?: string;
  /** Why, in words (at most 400 characters); null clears it. */
  readonly reason?: string | null;
} & (AgentProposalSpot | { readonly after?: never; readonly start?: never });

/** Creates or updates proposed steps on a page. Changing a person's words is refused unless `replacePeopleText`. */
export interface ProposeOp {
  readonly op: "propose";
  readonly proposals: readonly AgentProposalSpec[];
  readonly replacePeopleText?: boolean;
}

/** A name `shape_card` sets on a block (any block, by its full key); null takes the page's name for it off. */
export interface AgentNameSpec {
  readonly key: string;
  readonly text: string | null;
}

/**
 * A combined step `shape_card` makes, or updates when the page has one with exactly these blocks:
 * consecutive blocks of one body, or one whole control block, by their full keys, in written order.
 */
export interface AgentCombineSpec {
  readonly members: readonly string[];
  readonly title: string;
  /** Drawn as one step (absent: collapsed for a new one, unchanged for an existing one). */
  readonly collapsed?: boolean;
}

/**
 * Shapes one card of the page, as one undo step: its title (the page's name for its definition), its
 * description, names on blocks, combined steps, and the calls it opens in place. An app's or
 * entrance's card, or a type card, draws no flow: it takes only a description. Names and combined
 * steps apply page-wide, by key. The save checks every key and the overlay rules. Changing or removing
 * a person's words is refused unless `replacePeopleText`.
 */
export interface ShapeCardOp {
  readonly op: "shape_card";
  /** The card's id. */
  readonly card: string;
  readonly title?: string;
  readonly description?: string;
  readonly names?: readonly AgentNameSpec[];
  readonly combine?: readonly AgentCombineSpec[];
  /** The calls and callbacks it opens in place, replacing the ones it opens now (empty: none). */
  readonly open?: readonly AgentCallPath[];
  readonly replacePeopleText?: boolean;
}

/** An agent's operation on a page, tagged by `op`. */
export type AgentOp =
  | AddCardsOp
  | SetViewOp
  | ArrangeOp
  | RemoveItemsOp
  | DescribeOp
  | FrameCardsOp
  | AddResourcesOp
  | ConnectOp
  | AddNotesOp
  | ProposeOp
  | ShapeCardOp;

// ---------------------------------------------------------------------------------------------
// Results

/** What happened to an item: added, changed, already as asked, or taken off the page. */
export type AgentItemStatus = "created" | "updated" | "kept" | "removed";

/** Where following calls or a type's fields stopped, at one card. */
export interface AgentFollowStop {
  /**
   * `external` and `unresolved`: a call or a field's type name following can't go through; `builtin`: a
   * field's type is the language's own (`Date`, `str`); `depth`: resolved calls or declared types past
   * the depth asked for; `limit`: callers past the most one step lists, or cards past the most one call adds.
   */
  readonly reason: "external" | "unresolved" | "builtin" | "depth" | "limit";
  /** The call as written (`ordersQueue.add`), for an external or unresolved call. */
  readonly call?: string;
  /** The calling line. */
  readonly line?: number;
  /** A field's type name as written (`Money`, `Decimal`), for a type following can't go through. */
  readonly type?: string;
  /** The field that writes it (the first, when several do). */
  readonly field?: string;
  /** An external call's or type's library, or why it is unresolved. */
  readonly note?: string;
  /** How many calls, callers, or types were not followed (`depth`, `limit`). */
  readonly count?: number;
}

/** A card an operation added, changed, kept, or removed. */
export interface AgentCardResult {
  readonly kind: "card";
  readonly id: string;
  readonly status: AgentItemStatus;
  readonly key: string;
  /** Unfold's name for it: `OrderService.place`, `POST /orders`. */
  readonly name: string;
  /** Its canvas position, when an operation placed or moved it. */
  readonly at?: readonly [number, number];
  /** How following reached it: from which card, which way (calls, callers, or a type's fields), and how many levels deep. */
  readonly via?: { readonly from: string; readonly direction: "callees" | "callers" | "types"; readonly depth: number };
  /** Where following stopped at this card. */
  readonly stops?: readonly AgentFollowStop[];
  /** The frame an operation put it in (`add_cards` with `frame`). */
  readonly frame?: string;
  /** How it shows (`set_view`; absent for a type card, which keeps its fields). */
  readonly view?: AgentView;
  /** The calls and callbacks it opens in place, as saved (`set_view`, `shape_card`). */
  readonly open?: readonly AgentCallPath[];
}

/** A combined step an operation changed, kept, described, or removed. */
export interface AgentCombinedResult {
  readonly kind: "combined";
  readonly id: string;
  readonly status: AgentItemStatus;
  /** Its title. */
  readonly name: string;
  /** The block keys it combines, in order. */
  readonly members: readonly string[];
  /** Whether it is drawn as one step. */
  readonly collapsed: boolean;
}

/** A connection an operation drew, described, or removed. */
export interface AgentLinkResult {
  readonly kind: "link";
  readonly id: string;
  readonly status: AgentItemStatus;
  readonly from: string;
  readonly to: string;
  /** Its label, if it has one. */
  readonly name?: string;
  readonly connection: AgentConnectionKind;
  /** The block key of the call it leaves from. */
  readonly step?: string;
  /** What travels, as saved: the save fills it in when `to` is an entrance. */
  readonly channel?: string;
  /** The key of the type it carries. */
  readonly carries?: string;
}

/** A resource card an operation added, changed, kept, described, or removed. */
export interface AgentResourceResult {
  readonly kind: "resource";
  readonly id: string;
  readonly status: AgentItemStatus;
  readonly resource: AgentResourceKind;
  /** Its name (`orders`). */
  readonly name: string;
  /** A table's type key. */
  readonly type?: string;
  /** A table's columns as its card shows them: its type's fields with the marks laid on, or its own. */
  readonly columns?: readonly AgentColumn[];
  /** Its canvas position, when an operation placed it. */
  readonly at?: readonly [number, number];
  /** The frame an operation put it in. */
  readonly frame?: string;
}

/** A note an operation added, changed, kept, or removed. */
export interface AgentNoteResult {
  readonly kind: "note";
  readonly id: string;
  readonly status: AgentItemStatus;
  /** Its first 40 characters. */
  readonly name: string;
  /** The item it moves with. */
  readonly attach?: string;
  /** Its canvas position (an offset from what it's attached to), when an operation placed it. */
  readonly at?: readonly [number, number];
}

/** A proposed step an operation created, changed, kept, or removed: an intention, never code. */
export type AgentProposalResult = {
  readonly kind: "proposal";
  readonly id: string;
  readonly status: AgentItemStatus;
  readonly title: string;
  readonly reason?: string;
} & AgentProposalSpot;

/** An item a frame holds: its id, what it is, and Unfold's name for it (a frame's title). */
export interface AgentFrameMember {
  readonly id: string;
  readonly kind: "card" | "frame" | "resource" | "note";
  readonly name: string;
}

/** A frame an operation created, changed, kept, or removed. */
export interface AgentFrameResult {
  readonly kind: "frame";
  readonly id: string;
  readonly status: AgentItemStatus;
  /** Its title. */
  readonly name: string;
  readonly look: AgentFrameLook;
  readonly icon?: AgentFrameIcon;
  readonly direction?: AgentDirection;
  readonly collapsed?: boolean;
  /** The frame it is nested in. */
  readonly parent?: string;
  /** What it holds, in order. */
  readonly members: readonly AgentFrameMember[];
}

/** One item in an edit's result, tagged by `kind`. */
export type AgentItemResult =
  | AgentCardResult
  | AgentCombinedResult
  | AgentLinkResult
  | AgentFrameResult
  | AgentResourceResult
  | AgentNoteResult
  | AgentProposalResult;

/** The board a page is on, as the window holds it. */
export interface AgentBoardInfo {
  readonly title: string;
  /** The board file's revision (the engine's), as last loaded or saved. */
  readonly revision: string;
}

/**
 * A refusal: what failed and what to do instead. The window's codes are
 * `board_not_open`, `page_not_found`, `invalid_op`, `people_text`, `code_unavailable`,
 * `unsaved_edits`, and `page_changed`, besides the engine's own (`board_problem`, …); the main
 * process adds `revision_mismatch`, `window_closed`, `timeout` (a read), `unconfirmed` (a change that
 * may still land), and `internal` (a malformed answer).
 */
export interface AgentFailure {
  readonly ok: false;
  readonly code: string;
  readonly message: string;
}

/** An applied and saved edit: the page as saved, its board, what each operation did, and warnings. */
export interface AgentEditSuccess {
  readonly ok: true;
  readonly page: AgentPage;
  readonly board: AgentBoardInfo;
  readonly items: readonly AgentItemResult[];
  readonly warnings: readonly string[];
}

export type AgentEditResult = AgentEditSuccess | AgentFailure;

/** A held page, unsaved edits included. */
export interface AgentReadSuccess {
  readonly ok: true;
  readonly page: AgentPage;
  /** The board, and whether the window has edits not saved yet. */
  readonly board: AgentBoardInfo & { readonly unsaved: boolean };
  /** The sizes the canvas measured for the page's items, by id, in canvas units (none before the page is drawn). */
  readonly sizes?: Readonly<Record<string, { readonly width: number; readonly height: number }>>;
}

export type AgentReadResult = AgentReadSuccess | AgentFailure;

/** Whether the window opened (or already held) a board. */
export type AgentOpenResult = { readonly ok: true } | AgentFailure;

// ---------------------------------------------------------------------------------------------
// Boards as a whole: creating one, its pages and title, and showing it to the person

/** A page of a board: its id and title. */
export interface AgentPageSummary {
  readonly id: string;
  readonly title: string;
}

/**
 * One change to a board's pages or title. A page added goes after `after` (a page id; absent: last);
 * `move_page` puts a page at `to` (0 is first). The only page of a board is never deleted.
 */
export type AgentBoardOp =
  | { readonly op: "add_page"; readonly title: string; readonly after?: string }
  | { readonly op: "rename_page"; readonly page: string; readonly title: string }
  | { readonly op: "move_page"; readonly page: string; readonly to: number }
  | { readonly op: "delete_page"; readonly page: string }
  | { readonly op: "set_title"; readonly title: string };

/** What one {@link AgentBoardOp} did: the page it made or changed (`page` absent for the title), and its title. */
export interface AgentBoardOpResult {
  readonly op: AgentBoardOp["op"];
  readonly status: AgentItemStatus;
  readonly page?: string;
  readonly title: string;
}

/** An applied and saved board edit: the board, its pages in order, and what each operation did. */
export interface AgentBoardEditSuccess {
  readonly ok: true;
  readonly board: AgentBoardInfo;
  readonly pages: readonly AgentPageSummary[];
  readonly results: readonly AgentBoardOpResult[];
}

export type AgentBoardEditResult = AgentBoardEditSuccess | AgentFailure;

/** A board the window created and holds without showing it: its id, folder, and pages. */
export interface AgentCreateSuccess {
  readonly ok: true;
  readonly board: AgentBoardRef;
  readonly info: AgentBoardInfo;
  /** Its folder, or null at the project's top level. */
  readonly folder: string | null;
  readonly pages: readonly AgentPageSummary[];
}

export type AgentCreateResult = AgentCreateSuccess | AgentFailure;

/** The window shows a board: the page it switched to. */
export type AgentShowResult = { readonly ok: true; readonly page: string } | AgentFailure;

/** A repair the window applied and saved: the page and its board as saved. */
export type AgentRepairResult = { readonly ok: true; readonly page: AgentPage; readonly board: AgentBoardInfo } | AgentFailure;

// ---------------------------------------------------------------------------------------------
// Requests (shell events) and replies (window commands)

/** Applies `ops` to a page of a board the window holds, as one undo step named "<agent> · <title>". */
export interface AgentEditRequest {
  readonly requestId: number;
  readonly board: AgentBoardRef;
  readonly page: string;
  readonly agent: AgentIdentity;
  /** The tool's title, which names the undo step ("Add cards"). */
  readonly title: string;
  readonly ops: readonly AgentOp[];
}

/** Reads a page of a board the window holds. */
export interface AgentReadRequest {
  readonly requestId: number;
  readonly board: AgentBoardRef;
  readonly page: string;
}

/** Loads a board's session in the window without changing what the person sees. */
export interface AgentOpenRequest {
  readonly requestId: number;
  readonly board: AgentBoardRef;
  /** The page it shows when the person switches to it. */
  readonly page?: string;
}

/**
 * Creates a board file in `project`'s `.unfold/boards/` and loads its session in the window without
 * changing what the person sees: empty, with one page per title in `pages` ("Page 1" when absent).
 */
export interface AgentCreateRequest {
  readonly requestId: number;
  readonly project: string;
  readonly title?: string;
  /** Its folder (absent: the project's top level). */
  readonly folder?: string;
  readonly pages?: readonly string[];
}

/**
 * Changes a held board's pages or title as one undo step named "<agent> · <title>", saved at once.
 * Deleting a page that holds text a person wrote is refused unless `replacePeopleText`.
 */
export interface AgentBoardEditRequest {
  readonly requestId: number;
  readonly board: AgentBoardRef;
  readonly agent: AgentIdentity;
  /** The tool's title, which names the undo step ("Edit pages"). */
  readonly title: string;
  readonly ops: readonly AgentBoardOp[];
  readonly replacePeopleText?: boolean;
}

/** Shows a held board to the person: the board, a page, and an item selected and centered. */
export interface AgentShowRequest {
  readonly requestId: number;
  readonly board: AgentBoardRef;
  /** The page (absent: the page the board shows). */
  readonly page?: string;
  /** An item of the page to select and center. */
  readonly item?: string;
}

/**
 * Applies one repair a health item offers, on a page of a held board, the way Review applies one: the
 * board's unsaved edits are saved first, the engine repairs and saves (`repair_board`), and the result
 * is one undo step. `items` are marked as the agent's work.
 */
export interface AgentRepairRequest {
  readonly requestId: number;
  readonly board: AgentBoardRef;
  readonly page: string;
  readonly agent: AgentIdentity;
  /** `repair_board`'s repair (the UI's generated `RepairSpec`, on `page`); plain JSON on this side. */
  readonly repair: { readonly page: string; readonly kind: string; readonly [field: string]: unknown };
  /** The ids of the page's items the repair changed, to outline. */
  readonly items: readonly string[];
}

/** Ends an agent's piece of work on a board: its working marks clear and its summary shows as a toast. */
export interface AgentFinish {
  readonly board: AgentBoardRef;
  readonly agent: AgentIdentity;
  /** One line ("Framed the order flow in three services"). */
  readonly summary: string;
}

/** A request as a shell event: its `type` and its fields (flattened, so it reads as any event does). */
type ShellEventOf<T extends string, P> = { readonly type: T } & { readonly [K in keyof P]: P[K] };

/** Shell events the main process sends a window for an agent, and the agents' server's state. */
export type AgentShellEvent =
  | ShellEventOf<"agent_edit", AgentEditRequest>
  | ShellEventOf<"agent_read", AgentReadRequest>
  | ShellEventOf<"agent_open_board", AgentOpenRequest>
  | ShellEventOf<"agent_finish", AgentFinish>
  | ShellEventOf<"agent_create_board", AgentCreateRequest>
  | ShellEventOf<"agent_edit_board", AgentBoardEditRequest>
  | ShellEventOf<"agent_show", AgentShowRequest>
  | ShellEventOf<"agent_repair", AgentRepairRequest>
  /** The MCP server started listening or failed to, or an agent connected or acted. */
  | { readonly type: "agent_server_changed"; readonly status: AgentServerStatus };

/** What is selected in a window's view. */
export type ViewSelection =
  | { readonly kind: "none" }
  | { readonly kind: "card"; readonly id: string }
  /**
   * A block drawn on a card, by its key; `owner` is the definition whose drawn flow holds it when it
   * shows in a call opened in place (absent: the card's own block).
   */
  | { readonly kind: "step"; readonly card: string; readonly key: string; readonly owner?: string }
  /** A proposed step. */
  | { readonly kind: "proposal"; readonly id: string }
  | { readonly kind: "combined"; readonly id: string }
  | { readonly kind: "frame"; readonly id: string }
  | { readonly kind: "resource"; readonly id: string }
  | { readonly kind: "note"; readonly id: string }
  /** A connection. */
  | { readonly kind: "link"; readonly id: string }
  /**
   * An arrow between two cards: the call blocks it draws, in source order (one from an expanded card;
   * every call to `to` from a compact card's edge).
   */
  | { readonly kind: "arrow"; readonly from: string; readonly to: string; readonly calls: readonly string[] };

/** Whether the code panel's lines can be trusted: `current`, or why not. */
export type CodePanelState = "current" | "changed" | "doesNotParse" | "notAnalyzed" | "notFound";

/** What the code panel shows, so "this code" means the same to the agent and the person. */
export interface CodePanelReport {
  /** The block it opened on: a block's key, a type's key (for one of its fields), or an app's or entrance's key; absent when it opened on lines of a file or a whole file. */
  readonly key?: string;
  /** The lines it opened on, when it opened on lines of a file (a combined step, a thin row, a citation). */
  readonly span?: readonly [number, number];
  /** The file it shows. */
  readonly file?: string;
  /** The first and last line it shows; absent until its lines load, or when it shows none. */
  readonly lines?: readonly [number, number];
  /** Whether the person widened it past the selection's own lines, to an enclosing block or the file. */
  readonly widened: boolean;
  /** Whether its lines can be trusted; absent until they load. */
  readonly state?: CodePanelState;
}

/** What the window shows: its board and page (none on Home), selection, the items in view, and the code panel; and the boards it holds. */
export interface ViewReport {
  readonly board?: AgentBoardRef;
  readonly page?: string;
  readonly selection: ViewSelection;
  /** What the code panel shows, while it is open. */
  readonly code?: CodePanelReport;
  /** The ids of the page's cards, combined steps, connections, frames, resource cards, and notes in view. */
  readonly visible: readonly string[];
  /** Every board whose session the window holds (each board opened while the app runs), the one shown included. */
  readonly held: readonly AgentBoardRef[];
}

/** Window commands for the agent channel (part of `WindowCommands`). */
export interface AgentWindowCommands {
  /** The window's view changed or it gained focus (debounced ~150 ms). */
  report_view: { args: ViewReport; result: null };
  /** The answer to an `agent_edit`. */
  agent_edit_result: { args: { requestId: number; result: AgentEditResult }; result: null };
  /** The answer to an `agent_read`. */
  agent_read_result: { args: { requestId: number; result: AgentReadResult }; result: null };
  /** The answer to an `agent_open_board`. */
  agent_open_board_result: { args: { requestId: number; result: AgentOpenResult }; result: null };
  /** The answer to an `agent_create_board`. */
  agent_create_board_result: { args: { requestId: number; result: AgentCreateResult }; result: null };
  /** The answer to an `agent_edit_board`. */
  agent_edit_board_result: { args: { requestId: number; result: AgentBoardEditResult }; result: null };
  /** The answer to an `agent_show`. */
  agent_show_result: { args: { requestId: number; result: AgentShowResult }; result: null };
  /** The answer to an `agent_repair`. */
  agent_repair_result: { args: { requestId: number; result: AgentRepairResult }; result: null };
  /** The MCP server's state, for Settings (`agent_server_changed` follows it). */
  agent_server_status: { args: Record<string, never>; result: AgentServerStatus };
}

// ---------------------------------------------------------------------------------------------
// The MCP server (Settings)

/** An agent that has connected since the server started, by the identity its edits are signed with. */
export interface ConnectedAgent {
  readonly author: `agent:${string}`;
  readonly name: string;
  /** When it last sent the server anything (ms since the epoch). */
  readonly lastSeenMs: number;
  /** When it last called a tool; null when it has only connected. */
  readonly lastActedMs: number | null;
}

/**
 * What the MCP server is doing: `starting`, `listening` at `address`, `port_taken` (another program
 * listens on the port), or `failed` (it couldn't listen, for `message`).
 */
export type AgentServerState = "starting" | "listening" | "port_taken" | "failed";

/** The MCP server's state, the address agents connect to, and the agents that connected. */
export interface AgentServerStatus {
  readonly state: AgentServerState;
  /** `http://127.0.0.1:29180/mcp`. */
  readonly address: string;
  /** Why it failed (`failed` only). */
  readonly message?: string;
  /** The agents seen since it started listening, the one seen last first. */
  readonly agents: readonly ConnectedAgent[];
}

/**
 * A link to a board, a page, or an item: `unfold://<project's folder name>/<board id>?page=<page
 * id>&item=<item id>`, each part URI-encoded. Only the main process parses links.
 */
export interface AgentLink {
  /** The name of the project's folder. */
  readonly project: string;
  readonly board: string;
  readonly page?: string;
  readonly item?: string;
}
