/**
 * What tools answer (design §6.1, §7.8): a header that says where the result is and who is asking,
 * then compact text, a `next` cursor when there's more, and never more than about 6,000 tokens.
 * Errors teach: what failed, why, and what to call instead.
 */

import type { CallToolResult } from "@modelcontextprotocol/server";
import type { AgentIdentity } from "../../shared/agent-api";

/** A tool's answer: MCP's `CallToolResult` (text blocks, and images). */
export type ToolResult = CallToolResult;

/** About 6,000 tokens, at about four characters a token: what one result may hold. */
export const MAX_RESULT_CHARS = 24_000;

/** Where a result is: the project, and the board, page, and the page's revision when it's about one. */
export interface ResultPlace {
  /** The project's shared name. */
  readonly project?: string;
  readonly board?: { readonly title: string; readonly draft: boolean };
  /** The page's title. */
  readonly page?: string;
  /** The page's revision (D4). */
  readonly revision?: string;
}

/**
 * The first line of every result (design §5.6):
 * `Unfold · ts-orders › Order placement (draft) › Placing an order · rev 1a2b3c4d · as Claude`.
 */
export function header(agent: AgentIdentity, place: ResultPlace = {}): string {
  const where = [
    place.project,
    place.board ? `${place.board.title}${place.board.draft ? " (draft)" : ""}` : undefined,
    place.page,
  ].filter((part) => part !== undefined);
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
 * Keeps a result under {@link MAX_RESULT_CHARS}: a longer text is cut at a line and says so. Tools
 * page with a cursor well before this; it is the last guard.
 */
export function fitResult(result: ToolResult): ToolResult {
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
    const text = `${block.text.slice(0, cut > 0 ? cut : budget)}\n… Cut here to stay under about 6,000 tokens. Ask for less: a narrower filter, or the next page.`;
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
  const start = cursor === undefined ? 0 : Number(cursor);
  if (!Number.isSafeInteger(start) || start < 0 || (start > 0 && start >= items.length)) {
    throw new ToolError(`“${cursor ?? ""}” isn't a cursor of this list. Leave cursor out for the first page, or pass the next cursor a result gave.`);
  }
  const end = Math.min(start + size, items.length);
  return { items: items.slice(start, end), total: items.length, ...(end < items.length ? { next: String(end) } : {}) };
}

/** The line that says how to get the next page, or null on the last page. */
export function nextLine(page: Paged<unknown>, tool: string): string | null {
  return page.next === undefined ? null : `Showing ${page.items.length} of ${page.total}. Next page: ${tool} with cursor "${page.next}".`;
}

/** A refusal a tool answers with: its message says what failed, why, and what to do instead. */
export class ToolError extends Error {
  override readonly name = "ToolError";
}

/**
 * A refusal in the shape §7.8 asks for: `Can't <what>: <why>. <instead>`, for example "Can't read the
 * board: no project named “shop” is in the workspace. list_projects shows the names."
 */
export function cant(what: string, why: string, instead: string): ToolError {
  return new ToolError(`Can't ${what}: ${why.replace(/\.$/, "")}. ${instead}`);
}

/** The result of a refusal. */
export function errorResult(error: ToolError): ToolResult {
  return { content: [{ type: "text", text: error.message }], isError: true };
}
