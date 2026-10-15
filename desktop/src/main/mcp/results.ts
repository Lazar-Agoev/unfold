/**
 * What tools answer: a header that says where the result is and who is asking,
 * then compact text, a `next` cursor when there's more, and never more than about 6,000 tokens.
 * Errors teach: what failed, why, and what to call instead.
 */

import type { CallToolResult } from "@modelcontextprotocol/server";
import type { AgentFailure, AgentIdentity } from "../../shared/agent-api";

/** A tool's answer: MCP's `CallToolResult` (text blocks, and images). */
export type ToolResult = CallToolResult;

/** About 6,000 tokens, at about four characters a token: what one result may hold. */
export const MAX_RESULT_CHARS = 24_000;

/** Where a result is: the project, and the board, page, and the page's revision when it's about one. */
export interface ResultPlace {
  /** The project's name. */
  readonly project?: string;
  /** The board's title. */
  readonly board?: string;
  /** The page's title. */
  readonly page?: string;
  /** The page's revision. */
  readonly revision?: string;
}

/**
 * The first line of every result:
 * `Unfold · ts-orders › Order placement › Placing an order · rev 1a2b3c4d · as Claude`.
 */
export function header(agent: AgentIdentity, place: ResultPlace = {}): string {
  const where = [place.project, place.board, place.page].filter((part) => part !== undefined);
  return [
    "Unfold",
    ...(where.length > 0 ? [where.join(" › ")] : []),
    ...(place.revision !== undefined ? [`rev ${place.revision}`] : []),
    `as ${agent.name}`,
  ].join(" · ");
}

/** A text result from lines; blank lines and lines given as null, undefined, or false are dropped. */
export function textResult(...lines: readonly (string | null | undefined | false)[]): ToolResult {
  return { content: [{ type: "text", text: lines.filter((line): line is string => typeof line === "string" && line.length > 0).join("\n") }] };
}

/**
 * Keeps a result, or a refusal, under {@link MAX_RESULT_CHARS}: a longer text is cut at a line and
 * says so. Tools page with a cursor well before this; it is the last guard.
 */
export function fitResult(result: ToolResult): ToolResult {
  const note = result.isError === true ? "… Cut here to stay under about 6,000 tokens." : "… Cut here to stay under about 6,000 tokens. Ask for less: a narrower filter, or the next page.";
  let budget = MAX_RESULT_CHARS;
  const content = result.content.map((block) => {
    if (block.type !== "text") {
      return block;
    }
    if (block.text.length <= budget) {
      budget -= block.text.length;
      return block;
    }
    const cut = block.text.lastIndexOf("\n", budget);
    const text = `${block.text.slice(0, cut > 0 ? cut : budget)}\n${note}`;
    budget = 0;
    return { ...block, text };
  });
  return { ...result, content };
}

/** A page of a list, and the cursor of the next page when there is one. */
export interface Paged<T> {
  readonly items: readonly T[];
  /** Pass it back as `cursor` for the next page. */
  readonly next?: string;
  /** How many there are in all. */
  readonly total: number;
}

/**
 * One page of `items`: `size` of them from `cursor` (a cursor an earlier page returned; absent for
 * the first page). A cursor that isn't one is a {@link ToolError}.
 */
export function paged<T>(items: readonly T[], cursor: string | undefined, size: number): Paged<T> {
  const start = cursorOffset(cursor);
  if (start > 0 && start >= items.length) {
    throw badCursor(cursor);
  }
  const end = Math.min(start + size, items.length);
  return { items: items.slice(start, end), total: items.length, ...(end < items.length ? { next: String(end) } : {}) };
}

/** Room a result keeps under {@link MAX_RESULT_CHARS} for its header and the lines around a page. */
export const RESULT_ROOM = 2_000;

/**
 * The first of `lines` whose text fits in `budget` characters (at least one): a page of text ends
 * early rather than be cut by the size guard, so the line that says where the next page starts stays.
 */
export function fittingLines(lines: readonly string[], budget: number): readonly string[] {
  let chars = 0;
  const end = lines.findIndex((line, index) => {
    chars += line.length + 1;
    return index > 0 && chars > budget;
  });
  return end === -1 ? lines : lines.slice(0, end);
}

/**
 * One page of `lines`, as {@link paged}, that also ends before its text passes what a result may hold
 * ({@link fittingLines}), so the size guard never cuts a page with its cursor.
 */
export function pagedLines(lines: readonly string[], cursor: string | undefined, size: number): Paged<string> {
  const start = cursorOffset(cursor);
  const fitting = fittingLines(lines.slice(start, start + size), MAX_RESULT_CHARS - RESULT_ROOM).length;
  return paged(lines, cursor, Math.max(fitting, 1));
}

/** The refusal of a cursor that isn't one of the list's. */
export function badCursor(cursor: string | undefined): ToolError {
  return new ToolError(`“${cursor ?? ""}” isn't a cursor of this list. Leave cursor out for the first page, or pass the next cursor a result gave.`);
}

/**
 * Where a page starts: 0 without a cursor, else the offset a cursor holds. A cursor that isn't one is
 * a {@link ToolError}. For lists a tool pages itself (several at once, or from the engine's pages).
 */
export function cursorOffset(cursor: string | undefined): number {
  const start = cursor === undefined ? 0 : Number(cursor);
  if (!Number.isSafeInteger(start) || start < 0) {
    throw badCursor(cursor);
  }
  return start;
}

/** The line that says how to get the next page, or null on the last page. */
export function nextLine(page: Paged<unknown>, tool: string): string | null {
  return page.next === undefined ? null : `Showing ${page.items.length} of ${page.total}. Next page: ${tool} with cursor "${page.next}".`;
}

/**
 * A refusal a tool answers with: its message says what failed, why, and what to do instead. `code` is
 * the engine's or window's error code when the refusal is theirs, so a tool can teach more for it.
 */
export class ToolError extends Error {
  override readonly name = "ToolError";

  constructor(
    message: string,
    readonly code?: string,
  ) {
    super(message);
  }
}

/**
 * A refusal, shaped `Can't <what>: <why>. <instead>`, for example "Can't read the
 * board: no project named “shop” is in the workspace. list_projects shows the names."
 */
export function cant(what: string, why: string, instead: string): ToolError {
  return new ToolError(`Can't ${what}: ${why.replace(/\.$/, "")}. ${instead}`);
}

/** What to do about a window's refusal, by its code. */
const WINDOW_HINTS: Readonly<Record<string, string>> = {
  unsaved_edits: "The person's own edits are being saved; try again in a moment.",
  page_changed: "read_page reads it.",
  revision_mismatch: "read_page reads it, with its new rev.",
  window_closed: "Try again.",
  timeout: "Unfold may be busy; try again.",
  unconfirmed: "Read what you changed first (read_page, or list_boards for a board's pages and title), and try again only if it didn't land.",
};

/** A window's (or the bridge's) refusal as a refusal that teaches, keeping its code. */
export function windowRefusal(failure: AgentFailure): ToolError {
  const hint = WINDOW_HINTS[failure.code];
  return new ToolError(hint === undefined ? failure.message : `${failure.message} ${hint}`, failure.code);
}

/** The result of a refusal, kept under the size limit as every result is ({@link fitResult}). */
export function errorResult(error: ToolError): ToolResult {
  return fitResult({ content: [{ type: "text", text: error.message }], isError: true });
}
