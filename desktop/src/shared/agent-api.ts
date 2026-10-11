/**
 * The agent channel (design §5.5, §5.6): how the main process has a window apply an agent's edits to
 * a board it holds, read a page it holds, open a board in a background tab, and end a piece of work,
 * and what each window reports about its view. Types only, so the UI imports them as
 * `@unfold/desktop/agent-api`.
 *
 * - main → window: the shell events of {@link AgentShellEvent};
 * - window → main: the window commands of {@link AgentWindowCommands}.
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
// Operations: one per write tool, with resolved arguments. Later tools add their own (frames,
// resources, connections, notes).

/** One card `add_cards` puts on a page. */
export interface AgentCardSpec {
  /** A block's key (`src/routes/orders.ts#function:createOrder`) or an entrance's (`entrance:[…]`). */
  readonly key: string;
  /** The project it is from, by id; absent: the board's own project. */
  readonly project?: string;
  /** Its own view; absent: the page's default view. */
  readonly view?: AgentView;
  /** The id of an item on the page to place it beside; absent: beside the cards it relates to. */
  readonly near?: string;
  /**
   * Adds its call tree as well, following resolved calls only: what it calls (0–3 levels) and what
   * calls it (0–2 levels). The result says where following stopped.
   */
  readonly follow?: { readonly callees?: number; readonly callers?: number };
}

/** Puts cards on the page, near the cards they relate to; cards already there are kept, not duplicated. */
export interface AddCardsOp {
  readonly op: "add_cards";
  readonly cards: readonly AgentCardSpec[];
}

/** How one item shows: a card's view and fold, or whether a step group is collapsed. */
export interface AgentViewSpec {
  /** A card's or a step group's id. */
  readonly id: string;
  /** A card's view. */
  readonly view?: AgentView;
  /** Whether a card is folded to its title. */
  readonly folded?: boolean;
  /** Whether a step group is collapsed. */
  readonly collapsed?: boolean;
}

/** Sets how cards and step groups show, and the page's default view. */
export interface SetViewOp {
  readonly op: "set_view";
  readonly items: readonly AgentViewSpec[];
  readonly defaultView?: AgentView;
}

/** Where a card goes after Arrange: a canvas position, or just right of or below another card. */
export type AgentMove = { readonly id: string } & (
  | { readonly at: readonly [number, number] }
  | { readonly rightOf: string }
  | { readonly below: string }
);

/** Arranges the page by call depth (today's Arrange), then applies the moves in order. */
export interface ArrangeOp {
  readonly op: "arrange";
  readonly moves?: readonly AgentMove[];
}

/**
 * Takes items off the page: cards (with their step groups and the links that end at them), step
 * groups, and links. Refused when that removes text a person wrote, unless `replacePeopleText`.
 */
export interface RemoveItemsOp {
  readonly op: "remove_items";
  readonly ids: readonly string[];
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

/** Writes descriptions of cards and step groups, and labels of links. */
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
export type AgentOp = AddCardsOp | SetViewOp | ArrangeOp | RemoveItemsOp | GroupStepsOp | DescribeOp | LabelStepsOp;

// ---------------------------------------------------------------------------------------------
// Results

/** What happened to an item: added, changed, already as asked, or taken off the page. */
export type AgentItemStatus = "created" | "updated" | "kept" | "removed";

/** Where following calls stopped, at one card. */
export interface AgentFollowStop {
  /**
   * `external` and `unresolved`: a call following can't go through; `depth`: resolved calls past the
   * depth asked for; `limit`: callers past the most one step lists, or cards past the most one call adds.
   */
  readonly reason: "external" | "unresolved" | "depth" | "limit";
  /** The call as written (`ordersQueue.add`), for an external or unresolved call. */
  readonly call?: string;
  /** The calling line. */
  readonly line?: number;
  /** An external call's library, or why a call is unresolved. */
  readonly note?: string;
  /** How many calls or callers were not followed (`depth`, `limit`). */
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
  /** How following calls reached it: from which card, which way, and how many levels deep. */
  readonly via?: { readonly from: string; readonly direction: "callees" | "callers"; readonly depth: number };
  /** Where following stopped at this card. */
  readonly stops?: readonly AgentFollowStop[];
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

/** One item in an edit's result, tagged by `kind`. */
export type AgentItemResult = AgentCardResult | AgentGroupResult | AgentLinkResult | AgentLabelResult;

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

/** Ends an agent's piece of work on a board: its working marks clear and its summary shows as a toast. */
export interface AgentFinish {
  readonly board: AgentBoardRef;
  readonly agent: AgentIdentity;
  /** One line ("Framed the order flow in three services"). */
  readonly summary: string;
}

/** A request as a shell event: its `type` and its fields (flattened, so it reads as any event does). */
type ShellEventOf<T extends string, P> = { readonly type: T } & { readonly [K in keyof P]: P[K] };

/** Shell events the main process sends a window for an agent. */
export type AgentShellEvent =
  | ShellEventOf<"agent_edit", AgentEditRequest>
  | ShellEventOf<"agent_read", AgentReadRequest>
  | ShellEventOf<"agent_open_board", AgentOpenRequest>
  | ShellEventOf<"agent_finish", AgentFinish>;

/** What is selected in a window's view. */
export type ViewSelection =
  | { readonly kind: "none" }
  | { readonly kind: "card"; readonly id: string }
  /** A Flow step or a statement of a card. */
  | { readonly kind: "step"; readonly card: string; readonly key: string }
  | { readonly kind: "group"; readonly id: string }
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
    };

/** What a window shows: its active board and page (none on Home), selection, lens, the items in view, and its board tabs. */
export interface ViewReport {
  readonly board?: AgentBoardRef;
  readonly page?: string;
  readonly selection: ViewSelection;
  /** The page's lens, or its default view. */
  readonly lens?: AgentView;
  /** The ids of the page's cards, step groups, and links in view. */
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
