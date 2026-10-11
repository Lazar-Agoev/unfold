/**
 * The agent channel (design §5.5, §5.6): how the main process has a window apply an agent's edits to
 * a board it holds, read a page it holds, open a board in a background tab, create a board, change a
 * board's pages and title, show a board to the person, and end a piece of work, and what each window
 * reports about its view. Types only, so the UI imports them as
 * `@unfold/desktop/agent-api`.
 *
 * - main → window: the shell events of {@link AgentShellEvent};
 * - window → main: the window commands of {@link AgentWindowCommands}.
 *
 * Preferences › Agents reads the state of the MCP server agents connect to ({@link AgentServerStatus}).
 *
 * The main process owns argument parsing, links, defaults, and page revisions; the window owns
 * placement, the document's rules, and the refusal texts. Every edit is one undo step of the board,
 * saved at once, and signed with the agent's identity.
 */

/** A board: its project's id (`ProjectId`) and the board's id (its public file's folder-relative path). */
export interface AgentBoardRef {
  readonly project: string;
  readonly id: string;
}

/** Who an edit is from (D5): `{author: "agent:claude", name: "Claude"}`. An agent never writes as a person. */
export interface AgentIdentity {
  readonly author: `agent:${string}`;
  /** How the app shows it: "Claude", "Codex". */
  readonly name: string;
}

/** How a card shows: a diagram, its Flow, or its Code. */
export type AgentView = "diagram" | "flow" | "code";

/** A page as its board file holds it (the UI's generated `Page`); plain JSON on this side. */
export interface AgentPage {
  readonly id: string;
  readonly title: string;
  readonly [field: string]: unknown;
}

// ---------------------------------------------------------------------------------------------
// Operations: one per write tool, with resolved arguments. Later tools add their own (resources,
// connections, notes).

/** One card `add_cards` puts on a page. */
export interface AgentCardSpec {
  /** A block's key (`src/routes/orders.ts#function:createOrder`) or an entrance's (`entrance:[…]`). */
  readonly key: string;
  /** The project it is from, by id; absent: the board's own project. */
  readonly project?: string;
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

/** How one item shows: a card's view and fold, or whether a step group or a frame is collapsed. */
export interface AgentViewSpec {
  /** A card's, a step group's, or a frame's id. */
  readonly id: string;
  /** A card's view. */
  readonly view?: AgentView;
  /** Whether a card is folded to its title. */
  readonly folded?: boolean;
  /** Whether a step group or a frame is collapsed. */
  readonly collapsed?: boolean;
}

/** Sets how cards, step groups, and frames show, and the page's default view. */
export interface SetViewOp {
  readonly op: "set_view";
  readonly items: readonly AgentViewSpec[];
  readonly defaultView?: AgentView;
}

/**
 * Where a card or a frame goes after Arrange: a canvas position (a frame's: its box's top-left), or
 * just right of or below another card or frame. A frame moves with everything in it.
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
 * Takes items off the page: cards (with their step groups and the links that end at them), step
 * groups, links, and frames. A removed frame's members stay, in the frame that held it, unless
 * `keepMembers` is false, which removes them too. What would dangle goes as well: a frame left with
 * no members, and the links to it. Refused when that removes text a person wrote, unless
 * `replacePeopleText`.
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

/** One step group: consecutive statements of one branch of a card, by line range or by step keys. */
export interface AgentStepGroupSpec {
  /** An existing group's id updates it; a new id names the new group; absent: one from the title. */
  readonly id?: string;
  readonly title: string;
  readonly description?: string;
  /** Lines `[from, to]`, 1-based and inclusive; the group takes the whole statements they cover. */
  readonly lines?: readonly [number, number];
  /** The statements' keys (from read_code), consecutive in one branch. */
  readonly steps?: readonly string[];
  readonly collapsed?: boolean;
}

/** Gathers consecutive steps of one card into step groups, or updates them. */
export interface GroupStepsOp {
  readonly op: "group_steps";
  /** The card's id. */
  readonly card: string;
  readonly groups: readonly AgentStepGroupSpec[];
  readonly replacePeopleText?: boolean;
}

/** Writes descriptions of cards, step groups, and frames, and labels of links. */
export interface DescribeOp {
  readonly op: "describe";
  readonly items: readonly { readonly id: string; readonly text: string }[];
  readonly replacePeopleText?: boolean;
}

/** Writes Flow labels for a card's steps. */
export interface LabelStepsOp {
  readonly op: "label_steps";
  /** The card's id. */
  readonly card: string;
  /** Each step's key (a Flow step of the card) and its label. */
  readonly labels: readonly { readonly step: string; readonly text: string }[];
  readonly replacePeopleText?: boolean;
}

/** An agent's operation on a page, tagged by `op`. */
export type AgentOp = AddCardsOp | SetViewOp | ArrangeOp | RemoveItemsOp | GroupStepsOp | DescribeOp | LabelStepsOp | FrameCardsOp;

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
  /** Its project's id. */
  readonly project: string;
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
}

/** A statement a step group holds. */
export interface AgentStatement {
  readonly key: string;
  /** Its block kind (`if`, `variable`, `expression_statement`). */
  readonly kind: string;
  readonly lines: readonly [number, number];
}

/** A step group an operation added, changed, kept, or removed. */
export interface AgentGroupResult {
  readonly kind: "group";
  readonly id: string;
  readonly status: AgentItemStatus;
  /** The card it is in. */
  readonly card: string;
  /** Its title. */
  readonly name: string;
  /** The lines its statements span. */
  readonly lines?: readonly [number, number];
  /** The statements it holds, in source order. */
  readonly statements?: readonly AgentStatement[];
}

/** A drawn link an operation labeled or removed. */
export interface AgentLinkResult {
  readonly kind: "link";
  readonly id: string;
  readonly status: AgentItemStatus;
  readonly from: string;
  readonly to: string;
  /** Its label, if it has one. */
  readonly name?: string;
}

/** A Flow label an operation wrote or kept. */
export interface AgentLabelResult {
  readonly kind: "label";
  readonly status: AgentItemStatus;
  /** The card whose step it labels. */
  readonly card: string;
  readonly step: string;
  /** The label's text. */
  readonly name: string;
}

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
export type AgentItemResult = AgentCardResult | AgentGroupResult | AgentLinkResult | AgentLabelResult | AgentFrameResult;

/** The board a page is on, as the window holds it. */
export interface AgentBoardInfo {
  readonly title: string;
  readonly place: "draft" | "published";
  /** A published board's unpublished changes (0 for a draft). */
  readonly changes: number;
  /** The board file's revision (the engine's), as last loaded or saved. */
  readonly revision: string;
}

/**
 * A refusal: what failed and what to do instead (design §7.8). The window's codes are
 * `board_not_open`, `page_not_found`, `invalid_op`, `people_text`, `code_unavailable`,
 * `board_conflict`, `unsaved_edits`, `page_changed`, and, opening a board, `held` and `too_many_tabs`,
 * besides the engine's own (`board_problem`, …); the main process adds `revision_mismatch`,
 * `board_held_elsewhere`, `window_closed`, `timeout`, and `internal` (a malformed answer).
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
}

export type AgentReadResult = AgentReadSuccess | AgentFailure;

/** Whether a window opened (or already held) a board; `heldBy` names the window that holds it instead. */
export type AgentOpenResult = { readonly ok: true } | (AgentFailure & { readonly heldBy?: number });

// ---------------------------------------------------------------------------------------------
// Boards as a whole: creating one, its pages and title, and showing it to the person

/** A page of a board: its id and title. */
export interface AgentPageSummary {
  readonly id: string;
  readonly title: string;
}

/**
 * One change to a board's pages or title. A page added or duplicated goes after `after` (a page id;
 * absent: right after the page it copies, or last); `move_page` puts a page at `to` (0 is first).
 * The only page of a board is never deleted.
 */
export type AgentBoardOp =
  | { readonly op: "add_page"; readonly title: string; readonly after?: string }
  | { readonly op: "rename_page"; readonly page: string; readonly title: string }
  | { readonly op: "move_page"; readonly page: string; readonly to: number }
  | { readonly op: "duplicate_page"; readonly page: string; readonly title?: string }
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

/** A draft a window created and holds in a background tab: its id, folder, and pages. */
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

/** Opens a board in a background tab of the window (it doesn't switch to it). */
export interface AgentOpenRequest {
  readonly requestId: number;
  readonly board: AgentBoardRef;
  /** The page its tab shows. */
  readonly page?: string;
}

/**
 * Creates a draft in `project` and opens it in a background tab (the window claims it): empty, with
 * one page per title in `pages` ("Page 1" when absent), or, with `copyOf`, a copy of a board of the
 * project the window holds ("<title> copy", in the same folder).
 */
export interface AgentCreateRequest {
  readonly requestId: number;
  readonly project: string;
  readonly title?: string;
  /** Its folder (absent: the project's top level). */
  readonly folder?: string;
  readonly pages?: readonly string[];
  /** The id of the board to copy. */
  readonly copyOf?: string;
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

/** Shows a held board to the person: its tab, a page, and an item selected and centered. */
export interface AgentShowRequest {
  readonly requestId: number;
  readonly board: AgentBoardRef;
  /** The page (absent: the page the board shows). */
  readonly page?: string;
  /** An item of the page to select and center. */
  readonly item?: string;
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
  /** The MCP server started, stopped, or failed, or an agent connected or acted (sent to every window). */
  | { readonly type: "agent_server_changed"; readonly status: AgentServerStatus };

/** What is selected in a window's view. */
export type ViewSelection =
  | { readonly kind: "none" }
  | { readonly kind: "card"; readonly id: string }
  /** A Flow step or a statement of a card. */
  | { readonly kind: "step"; readonly card: string; readonly key: string }
  | { readonly kind: "group"; readonly id: string }
  | { readonly kind: "frame"; readonly id: string }
  | { readonly kind: "link"; readonly id: string }
  /** A line between two cards: a call (`seq` 0) or a passed reference, by its occurrence. */
  | {
      readonly kind: "line";
      readonly from: string;
      readonly to: string;
      /** The project both cards resolve to. */
      readonly project: string;
      readonly source: string;
      readonly seq: number;
      readonly line: number | null;
    }
  /** An arrow from a Diagram node that stands for several calls to one card: the calls it draws, in source order. */
  | {
      readonly kind: "calls";
      readonly from: string;
      readonly to: string;
      /** The project both cards resolve to. */
      readonly project: string;
      readonly calls: readonly { readonly source: string; readonly seq: number; readonly line: number | null }[];
    };

/** What a window shows: its active board and page (none on Home), selection, lens, the items in view, and its board tabs. */
export interface ViewReport {
  readonly board?: AgentBoardRef;
  readonly page?: string;
  readonly selection: ViewSelection;
  /** The page's lens, or its default view. */
  readonly lens?: AgentView;
  /** The ids of the page's cards, step groups, links, and frames in view. */
  readonly visible: readonly string[];
  /** Every board tab of the window, in order: the boards it holds. */
  readonly tabs: readonly AgentBoardRef[];
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
  /** The MCP server's state, for Preferences › Agents (`agent_server_changed` follows it). */
  agent_server_status: { args: Record<string, never>; result: AgentServerStatus };
}

// ---------------------------------------------------------------------------------------------
// The MCP server (Preferences › Agents)

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
 * What the MCP server is doing: `off` (Preferences › Agents has it off), `starting`, `listening` at
 * `address`, `port_taken` (another program listens on the port), or `failed` (it couldn't listen,
 * for `message`).
 */
export type AgentServerState = "off" | "starting" | "listening" | "port_taken" | "failed";

/** The MCP server's state, the address agents connect to, and the agents that connected. */
export interface AgentServerStatus {
  readonly state: AgentServerState;
  /** The port it listens on, or would. */
  readonly port: number;
  /** `http://127.0.0.1:<port>/mcp`. */
  readonly address: string;
  /** Why it failed (`failed` only). */
  readonly message?: string;
  /** The agents seen since it started listening, the one seen last first. */
  readonly agents: readonly ConnectedAgent[];
}

/**
 * A link to a board, a page, or an item (D9): `unfold://<project's shared name>/<board id>?page=<page
 * id>&item=<item id>`, each part URI-encoded. Only the main process parses links.
 */
export interface AgentLink {
  /** The project's shared name. */
  readonly project: string;
  readonly board: string;
  readonly page?: string;
  readonly item?: string;
}
