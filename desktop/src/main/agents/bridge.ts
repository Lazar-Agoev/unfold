/**
 * The agent bridge (design §5.1, §5.5): how the main process has windows act for an agent. A board is
 * open in one window at a time, and that window makes its edits, the way it makes the person's, so
 * the bridge routes each request to the window that holds the board (from the windows' view
 * reports). An edit of a board no window holds opens it first, in a background tab of the window used
 * last (a new window when none is open); a new board is created there too, so that window holds it.
 * Every request has a timeout; a window that closes before it answers fails its requests with
 * `window_closed`.
 *
 * Page revisions are checked here (D4): an edit that passes `expectRevision` reads the page from the
 * holding window first and is refused when it changed.
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
  AgentShellEvent,
  AgentShowResult,
} from "../../shared/agent-api";
import type { WindowEntry, WindowRegistry } from "../windows";
import { pageRevision } from "./revision";
import type { ViewTracker } from "./views";

/** How long a window may take to answer by default: opening a board in a new window included. */
const DEFAULT_TIMEOUT_MS = 30_000;

/** An edit as a tool asks for it: the request without its id, and the page revision the agent read. */
export type AgentEditInput = Omit<AgentEditRequest, "requestId"> & { readonly expectRevision?: string };

/** An applied edit with the page's new revision, or a refusal. */
export type AgentEditOutcome = (AgentEditSuccess & { readonly revision: string }) | AgentFailure;

/** A held page with its revision, or a refusal (`board_not_open` when no window holds the board). */
export type AgentReadOutcome = (AgentReadSuccess & { readonly revision: string }) | AgentFailure;

/** The window that holds the board now, or a refusal. */
export type AgentOpenOutcome = { readonly ok: true; readonly windowId: number } | AgentFailure;

/** A board a window created and holds, with the window's id, or a refusal. */
export type AgentCreateOutcome = (AgentCreateSuccess & { readonly windowId: number }) | AgentFailure;

/** The page a window showed and the window's id, or a refusal. */
export type AgentShowOutcome = { readonly ok: true; readonly windowId: number; readonly page?: string } | AgentFailure;

/** What the bridge needs of the window registry. */
export type AgentWindows = Pick<WindowRegistry, "get" | "all" | "create" | "deliver" | "focus">;

export interface AgentBridgeOptions {
  readonly windows: AgentWindows;
  readonly views: ViewTracker;
  /** How long a window may take to answer one request. */
  readonly timeoutMs?: number;
}

/** The requests a window answers, by the window command it answers with. */
type RequestKind = "edit" | "read" | "open" | "create" | "board" | "show";

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
  private nextId = 1;

  constructor(options: AgentBridgeOptions) {
    this.windows = options.windows;
    this.views = options.views;
    this.timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  }

  /**
   * Applies an agent's operations to a page as one undo step, in the window that holds the board
   * (opening it in a background tab first). With `expectRevision`, refuses (`revision_mismatch`) when
   * the page changed since the agent read it.
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
      if (expectRevision !== undefined) {
        const read = (await this.ask(entry, "read", { type: "agent_read", board: request.board, page: request.page })) as AgentReadOutcome;
        if (!read.ok) {
          return read;
        }
        const now = pageRevision(read.page);
        if (now !== expectRevision) {
          return failure(
            "revision_mismatch",
            `The page changed since you read it (rev ${expectRevision}, now rev ${now}). Read it again, then make the change.`,
          );
        }
      }
      const result = (await this.ask(entry, "edit", { type: "agent_edit", ...request })) as AgentEditSuccess | AgentFailure;
      // A report that still listed a board the window closed: open it again, once.
      if (!result.ok && result.code === "board_not_open" && attempt === 0) {
        this.views.released(entry.id, request.board);
        continue;
      }
      return result.ok ? { ...result, revision: pageRevision(result.page) } : result;
    }
  }

  /**
   * Reads a page of a board a window holds, unsaved edits included. Refuses with `board_not_open` when
   * no window holds it (the engine reads it then, without claiming it).
   */
  async read(target: { readonly board: AgentBoardRef; readonly page: string }): Promise<AgentReadOutcome> {
    const entry = this.holderOf(target.board);
    if (!entry) {
      return failure("board_not_open", "No Unfold window holds this board.");
    }
    const result = (await this.ask(entry, "read", { type: "agent_read", ...target })) as AgentReadSuccess | AgentFailure;
    return result.ok ? { ...result, revision: pageRevision(result.page) } : result;
  }

  /**
   * Makes sure a window holds the board: the one that holds it already, or else the window used last
   * (a new window when none is open) opens it in a background tab, on `page` when given.
   */
  async open(target: { readonly board: AgentBoardRef; readonly page?: string }): Promise<AgentOpenOutcome> {
    const holder = this.holderOf(target.board);
    if (holder) {
      return { ok: true, windowId: holder.id };
    }
    const entry = this.lastUsed();
    const result = (await this.ask(entry, "open", { type: "agent_open_board", ...target })) as AgentOpenResult;
    if (result.ok) {
      this.views.holds(entry.id, target.board);
      return { ok: true, windowId: entry.id };
    }
    if (result.heldBy !== undefined) {
      if (this.windows.get(result.heldBy)) {
        this.views.holds(result.heldBy, target.board);
        return { ok: true, windowId: result.heldBy };
      }
      return failure(
        "board_held_elsewhere",
        "The board is open somewhere Unfold's windows can't reach (another copy of Unfold?). Ask the person to close it there.",
      );
    }
    return { ok: false, code: result.code, message: result.message };
  }

  /**
   * Creates a draft for an agent, which a window then holds in a background tab: an empty one in the
   * window used last (a new window when none is open), or a copy of `copyOf` in the window that holds
   * that board (opening it first).
   */
  async create(request: Omit<AgentCreateRequest, "requestId">): Promise<AgentCreateOutcome> {
    let entry: WindowEntry | undefined;
    if (request.copyOf === undefined) {
      entry = this.lastUsed();
    } else {
      const opened = await this.open({ board: { project: request.project, id: request.copyOf } });
      if (!opened.ok) {
        return opened;
      }
      entry = this.windows.get(opened.windowId);
    }
    if (!entry) {
      return this.closed();
    }
    const result = (await this.ask(entry, "create", { type: "agent_create_board", ...request })) as AgentCreateSuccess | AgentFailure;
    if (!result.ok) {
      return result;
    }
    this.views.holds(entry.id, result.board);
    return { ...result, windowId: entry.id };
  }

  /**
   * Changes a board's pages or title as one undo step, in the window that holds it (opening it in a
   * background tab first).
   */
  async editBoard(request: Omit<AgentBoardEditRequest, "requestId">): Promise<AgentBoardEditResult> {
    for (let attempt = 0; ; attempt += 1) {
      const opened = await this.open({ board: request.board });
      if (!opened.ok) {
        return opened;
      }
      const entry = this.windows.get(opened.windowId);
      if (!entry) {
        return this.closed();
      }
      const result = (await this.ask(entry, "board", { type: "agent_edit_board", ...request })) as AgentBoardEditResult;
      // A report that still listed a board the window closed: open it again, once.
      if (!result.ok && result.code === "board_not_open" && attempt === 0) {
        this.views.released(entry.id, request.board);
        continue;
      }
      return result;
    }
  }

  /**
   * Shows a board to the person (design §6.3 `show`). In the `background`, the board gets a tab
   * (opened as for an edit) and nothing moves; otherwise the window that holds it switches to its tab
   * and `page`, selects and centers `item`, and comes to the front.
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
   * Ends an agent's piece of work on a board: the window that holds it (else the window used last)
   * clears the agent's working marks and shows its summary as a toast.
   */
  finish(finish: AgentFinish): void {
    const last = this.views.lastUsed();
    const entry = this.holderOf(finish.board) ?? (last !== undefined ? this.windows.get(last) : undefined);
    if (entry) {
      this.windows.deliver(entry, { type: "agent_finish", ...finish });
    }
  }

  /**
   * A window answered request `requestId` with its window command (`kind`). Answers from another
   * window, of another kind, or after the timeout are ignored.
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

  /** The window used last, else the first window, else a new one. */
  private lastUsed(): WindowEntry {
    const last = this.views.lastUsed();
    return (last !== undefined ? this.windows.get(last) : undefined) ?? this.windows.all()[0] ?? this.windows.create();
  }

  private holderOf(board: AgentBoardRef): WindowEntry | undefined {
    const holder = this.views.holder(board);
    return holder === undefined ? undefined : this.windows.get(holder);
  }

  private closed(): AgentFailure {
    return failure("window_closed", "The window holding the board closed before it answered. Read the page again to see what was saved.");
  }

  /** Sends a request to a window (held until its UI has loaded) and waits for its answer. */
  private ask(entry: WindowEntry, kind: RequestKind, event: DistributiveOmit<AgentShellEvent, "requestId">): Promise<unknown> {
    const requestId = this.nextId++;
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.pending.delete(requestId);
        resolve(failure("timeout", `The Unfold window didn't answer within ${Math.round(this.timeoutMs / 1000)} s. Try again.`));
      }, this.timeoutMs);
      this.pending.set(requestId, { windowId: entry.id, kind, resolve, timer });
      this.windows.deliver(entry, { ...event, requestId } as AgentShellEvent);
    });
  }
}

/** `Omit` over each member of a union. */
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;
