/**
 * The agent bridge (design §5.1, §5.5): how the main process has the window act for an agent. The
 * window makes an agent's edits the way it makes the person's, on the boards whose sessions it holds
 * (from its view reports). An edit of a board it doesn't hold loads that board's session first,
 * without changing what the person sees (opening the window when it is closed); a new board's file is
 * created there too, so the window holds it. Every request has a timeout; a window that closes before
 * it answers fails its requests with `window_closed`. A window can't be told to drop a request, so a
 * change that times out (`unconfirmed`) may still land.
 *
 * Page revisions are checked here (D4): an edit that passes `expectRevision` reads the page from the
 * holding window first and is refused when it changed. The bridge also remembers which boards each
 * agent edited or repaired until it finishes its work on them, so `finish_work` knows where to finish.
 */

import type {
  AgentBoardEditRequest,
  AgentBoardEditResult,
  AgentBoardRef,
  AgentCreateRequest,
  AgentCreateSuccess,
  AgentEditRequest,
  AgentEditSuccess,
  AgentFailure,
  AgentFinish,
  AgentOpenResult,
  AgentReadSuccess,
  AgentRepairRequest,
  AgentRepairResult,
  AgentShellEvent,
  AgentShowResult,
} from "../../shared/agent-api";
import type { AppWindow, WindowEntry } from "../windows";
import { pageRevision } from "./revision";
import type { ViewTracker } from "./views";

/** How long the window may take to answer by default: opening the window included. */
const DEFAULT_TIMEOUT_MS = 30_000;

/** An edit as a tool asks for it: the request without its id, and the page revision the agent read. */
export type AgentEditInput = Omit<AgentEditRequest, "requestId"> & { readonly expectRevision?: string };

/** An applied edit with the page's new revision, or a refusal. */
export type AgentEditOutcome = (AgentEditSuccess & { readonly revision: string }) | AgentFailure;

/** A held page with its revision, or a refusal (`board_not_open` when the window doesn't hold the board). */
export type AgentReadOutcome = (AgentReadSuccess & { readonly revision: string }) | AgentFailure;

/** The window that holds the board now, or a refusal. */
export type AgentOpenOutcome = { readonly ok: true; readonly windowId: number } | AgentFailure;

/** A board a window created and holds, with the window's id, or a refusal. */
export type AgentCreateOutcome = (AgentCreateSuccess & { readonly windowId: number }) | AgentFailure;

/** The page a window showed and the window's id, or a refusal. */
export type AgentShowOutcome = { readonly ok: true; readonly windowId: number; readonly page?: string } | AgentFailure;

/** A repaired page with its new revision, or a refusal. */
export type AgentRepairOutcome = (Extract<AgentRepairResult, { ok: true }> & { readonly revision: string }) | AgentFailure;

/** What the bridge needs of the app's window. */
export type AgentWindows = Pick<AppWindow, "get" | "current" | "open" | "deliver" | "focus">;

export interface AgentBridgeOptions {
  readonly windows: AgentWindows;
  readonly views: ViewTracker;
  /** How long a window may take to answer one request. */
  readonly timeoutMs?: number;
}

/** The requests a window answers, by the window command it answers with. */
type RequestKind = "edit" | "read" | "open" | "create" | "board" | "show" | "repair";

/** The requests that change a board: one that times out may still be applied when the window gets to it. */
const CHANGES: ReadonlySet<RequestKind> = new Set(["edit", "create", "board", "repair"]);

interface Pending {
  readonly windowId: number;
  readonly kind: RequestKind;
  readonly resolve: (result: unknown) => void;
  readonly timer: ReturnType<typeof setTimeout>;
}

function failure(code: string, message: string): AgentFailure {
  return { ok: false, code, message };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasPage(result: Readonly<Record<string, unknown>>): boolean {
  const page = result["page"];
  return isRecord(page) && typeof page["id"] === "string" && isRecord(result["board"]);
}

/** A board's key in maps: its project and id. */
function boardKey(board: AgentBoardRef): string {
  return `${board.project}\u0000${board.id}`;
}

/** What a successful answer of each kind must hold, beyond `ok: true`. */
const SHAPES: Readonly<Record<RequestKind, (result: Readonly<Record<string, unknown>>) => boolean>> = {
  edit: (result) => hasPage(result) && Array.isArray(result["items"]) && Array.isArray(result["warnings"]),
  read: hasPage,
  open: () => true,
  create: (result) => {
    const board = result["board"];
    return isRecord(board) && typeof board["project"] === "string" && typeof board["id"] === "string" && isRecord(result["info"]) && Array.isArray(result["pages"]);
  },
  board: (result) => isRecord(result["board"]) && Array.isArray(result["pages"]) && Array.isArray(result["results"]),
  show: (result) => typeof result["page"] === "string",
  repair: hasPage,
};

/** A window's answer, checked enough that callers can rely on its shape. */
function checked(kind: RequestKind, result: unknown): unknown {
  if (isRecord(result) && result["ok"] === false && typeof result["code"] === "string" && typeof result["message"] === "string") {
    return result;
  }
  const valid = isRecord(result) && result["ok"] === true && SHAPES[kind](result);
  return valid ? result : failure("internal", "The window answered with a malformed result.");
}

export class AgentBridge {
  readonly views: ViewTracker;
  private readonly windows: AgentWindows;
  private readonly timeoutMs: number;
  private readonly pending = new Map<number, Pending>();
  /** The boards each agent (by author) edited since it last finished, by `boardKey`. */
  private readonly worked = new Map<string, Map<string, AgentBoardRef>>();
  private nextId = 1;

  constructor(options: AgentBridgeOptions) {
    this.windows = options.windows;
    this.views = options.views;
    this.timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  }

  /**
   * Applies an agent's operations to a page as one undo step, in the window (loading the board's
   * session first). With `expectRevision`, refuses (`revision_mismatch`) when the page changed since
   * the agent read it.
   */
  async edit(input: AgentEditInput): Promise<AgentEditOutcome> {
    const { expectRevision, ...request } = input;
    for (let attempt = 0; ; attempt += 1) {
      const opened = await this.open({ board: request.board, page: request.page });
      if (!opened.ok) {
        return opened;
      }
      const entry = this.windows.get(opened.windowId);
      if (!entry) {
        return this.closed();
      }
      const read = expectRevision === undefined ? null : ((await this.ask(entry, "read", { type: "agent_read", board: request.board, page: request.page })) as AgentReadSuccess | AgentFailure);
      if (read?.ok === true) {
        const now = pageRevision(read.page);
        if (now !== expectRevision) {
          return failure(
            "revision_mismatch",
            `The page changed since you read it (rev ${expectRevision}, now rev ${now}). Read it again, then make the change.`,
          );
        }
      }
      const result = read?.ok === false ? read : ((await this.ask(entry, "edit", { type: "agent_edit", ...request })) as AgentEditSuccess | AgentFailure);
      // A report that still listed a board the window no longer holds: load it again, once.
      if (!result.ok && result.code === "board_not_open" && attempt === 0) {
        this.views.released(entry.id, request.board);
        continue;
      }
      if (!result.ok) {
        return result;
      }
      this.recordWork(request.agent.author, request.board);
      return { ...result, revision: pageRevision(result.page) };
    }
  }

  /**
   * Reads a page of a board the window holds, unsaved edits included. Refuses with `board_not_open`
   * when it doesn't hold it (the engine reads it then).
   */
  async read(target: { readonly board: AgentBoardRef; readonly page: string }): Promise<AgentReadOutcome> {
    const entry = this.holderOf(target.board);
    if (!entry) {
      return failure("board_not_open", "Unfold's window doesn't hold this board.");
    }
    const result = (await this.ask(entry, "read", { type: "agent_read", ...target })) as AgentReadSuccess | AgentFailure;
    if (!result.ok && result.code === "board_not_open") {
      // Its report still listed a board it no longer holds.
      this.views.released(entry.id, target.board);
    }
    return result.ok ? { ...result, revision: pageRevision(result.page) } : result;
  }

  /**
   * Makes sure the window holds the board's session: it loads it, on `page` when given, without
   * changing what the person sees, unless it holds it already. The window opens when it is closed.
   */
  async open(target: { readonly board: AgentBoardRef; readonly page?: string }): Promise<AgentOpenOutcome> {
    const holder = this.holderOf(target.board);
    if (holder) {
      return { ok: true, windowId: holder.id };
    }
    const entry = this.windows.open();
    const result = (await this.ask(entry, "open", { type: "agent_open_board", ...target })) as AgentOpenResult;
    if (!result.ok) {
      return result;
    }
    this.views.holds(entry.id, target.board);
    return { ok: true, windowId: entry.id };
  }

  /**
   * Creates a board file for an agent, whose session the window then holds without showing it (the
   * window opens when it is closed).
   */
  async create(request: Omit<AgentCreateRequest, "requestId">): Promise<AgentCreateOutcome> {
    const entry = this.windows.open();
    const result = (await this.ask(entry, "create", { type: "agent_create_board", ...request })) as AgentCreateSuccess | AgentFailure;
    if (!result.ok) {
      return result;
    }
    this.views.holds(entry.id, result.board);
    return { ...result, windowId: entry.id };
  }

  /**
   * Changes a board's pages or title as one undo step, in the window (loading the board's session
   * first).
   */
  async editBoard(request: Omit<AgentBoardEditRequest, "requestId">): Promise<AgentBoardEditResult> {
    return (await this.askHolder({ board: request.board }, "board", { type: "agent_edit_board", ...request })) as AgentBoardEditResult;
  }

  /**
   * Shows a board to the person (design §6.3 `show`). In the `background`, the board's session is
   * loaded as for an edit and nothing moves; otherwise the window switches to it and `page`, selects
   * and centers `item`, and comes to the front.
   */
  async show(target: { readonly board: AgentBoardRef; readonly page?: string; readonly item?: string }, background: boolean): Promise<AgentShowOutcome> {
    const opened = await this.open({ board: target.board, ...(target.page !== undefined ? { page: target.page } : {}) });
    if (!opened.ok || background) {
      return opened;
    }
    const entry = this.windows.get(opened.windowId);
    if (!entry) {
      return this.closed();
    }
    const result = (await this.ask(entry, "show", { type: "agent_show", ...target })) as AgentShowResult;
    if (!result.ok) {
      return result;
    }
    this.windows.focus(entry);
    return { ok: true, windowId: entry.id, page: result.page };
  }

  /**
   * Applies one repair a health item offers (`repair_board`), as one undo step in the window (loading
   * the board's session first).
   */
  async repair(request: Omit<AgentRepairRequest, "requestId">): Promise<AgentRepairOutcome> {
    const result = (await this.askHolder({ board: request.board, page: request.page }, "repair", { type: "agent_repair", ...request })) as AgentRepairResult;
    if (!result.ok) {
      return result;
    }
    this.recordWork(request.agent.author, request.board);
    return { ...result, revision: pageRevision(result.page) };
  }

  /** The boards an agent edited or repaired since it last finished work on them, in the order it started. */
  workedOn(author: string): AgentBoardRef[] {
    return [...(this.worked.get(author)?.values() ?? [])];
  }

  /**
   * Ends an agent's piece of work on a board: the window, when open, clears the agent's working marks
   * and shows its summary as a toast.
   */
  finish(finish: AgentFinish): void {
    this.worked.get(finish.agent.author)?.delete(boardKey(finish.board));
    const entry = this.windows.current();
    if (entry) {
      this.windows.deliver(entry, { type: "agent_finish", ...finish });
    }
  }

  /**
   * A window answered request `requestId` with its window command (`kind`). Answers from a window not
   * asked (an earlier one, or the render window), of another kind, or after the timeout are ignored.
   */
  settle(windowId: number, kind: RequestKind, requestId: unknown, result: unknown): void {
    if (typeof requestId !== "number") {
      return;
    }
    const pending = this.pending.get(requestId);
    if (pending?.windowId !== windowId || pending.kind !== kind) {
      return;
    }
    clearTimeout(pending.timer);
    this.pending.delete(requestId);
    pending.resolve(checked(kind, result));
  }

  /** A window closed: its view is forgotten and the requests waiting on it fail. */
  windowClosed(windowId: number): void {
    this.views.forget(windowId);
    for (const [requestId, pending] of this.pending) {
      if (pending.windowId === windowId) {
        clearTimeout(pending.timer);
        this.pending.delete(requestId);
        pending.resolve(this.closed());
      }
    }
  }

  /**
   * Asks the window about a board (loading its session first, on `page` when given). A window whose
   * report still listed a board it no longer holds answers `board_not_open`: the board is loaded
   * again, once.
   */
  private async askHolder(target: { readonly board: AgentBoardRef; readonly page?: string }, kind: RequestKind, event: DistributiveOmit<AgentShellEvent, "requestId">): Promise<unknown> {
    for (let attempt = 0; ; attempt += 1) {
      const opened = await this.open(target);
      if (!opened.ok) {
        return opened;
      }
      const entry = this.windows.get(opened.windowId);
      if (!entry) {
        return this.closed();
      }
      const result = (await this.ask(entry, kind, event)) as AgentFailure | { readonly ok: true };
      if (!result.ok && result.code === "board_not_open" && attempt === 0) {
        this.views.released(entry.id, target.board);
        continue;
      }
      return result;
    }
  }

  private holderOf(board: AgentBoardRef): WindowEntry | undefined {
    const holder = this.views.holder(board);
    return holder === undefined ? undefined : this.windows.get(holder);
  }

  private recordWork(author: string, board: AgentBoardRef): void {
    const boards = this.worked.get(author) ?? new Map<string, AgentBoardRef>();
    boards.set(boardKey(board), board);
    this.worked.set(author, boards);
  }

  private closed(): AgentFailure {
    return failure("window_closed", "Unfold's window closed before it answered. Read the page again to see what was saved.");
  }

  /** Sends a request to a window (held until its UI has loaded) and waits for its answer. */
  private ask(entry: WindowEntry, kind: RequestKind, event: DistributiveOmit<AgentShellEvent, "requestId">): Promise<unknown> {
    const requestId = this.nextId++;
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.pending.delete(requestId);
        const waited = `The Unfold window didn't answer within ${Math.round(this.timeoutMs / 1000)} s`;
        resolve(CHANGES.has(kind) ? failure("unconfirmed", `${waited}, so the change may still land.`) : failure("timeout", `${waited}.`));
      }, this.timeoutMs);
      this.pending.set(requestId, { windowId: entry.id, kind, resolve, timer });
      this.windows.deliver(entry, { ...event, requestId } as AgentShellEvent);
    });
  }
}

/** `Omit` over each member of a union. */
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;
