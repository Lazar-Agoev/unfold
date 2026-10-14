/**
 * How the write tools edit a page (design §5.5, §6.1, §7.8): find the page (ids, a link, or the page
 * in view), send one agent edit (one undo step in the window that holds the board, saved at once,
 * signed with the agent's name, refused when the page changed since `expectRevision`), and turn what
 * the window answers into compact text: the header with the page's new revision, each item with its
 * name, id, and what happened to it (a card's expansion when an edit set it), the warnings, and what
 * to call next.
 */

import { z } from "zod";
import type { AgentBoardRef, AgentCardRef, AgentEditSuccess, AgentItemResult, AgentItemStatus, AgentOp } from "../../shared/agent-api";
import type { ToolContext } from "./context";
import type { ProjectRow } from "./engine-types";
import { cardText, count, joined } from "./format";
import { cant, textResult, ToolError, windowRefusal, type ToolResult } from "./results";
import { isLink, resolveBoard, type TargetArgs } from "./target";

/** Where a tool works: a board, a page, and the board's project; each may be a link. */
export const pageArgs = {
  board: z.string().min(1).optional().describe("A board's id (from list_boards) or a link (default: the board in view)"),
  page: z.string().min(1).optional().describe("A page's id or a link (default: the page in view)"),
  project: z.string().min(1).optional().describe("The board's project, when the board's id isn't enough"),
};

/** Lines `[from, to]`, 1-based and inclusive. */
const lineRange = z
  .tuple([z.number().int().min(1), z.number().int().min(1)])
  .refine(([from, to]) => from <= to, "lines is [from, to] with from ≤ to");

/** What a claim cites (design §4.9): a block's or an entrance's key, or an evidence file's lines. */
export const anchorArg = z.union([
  z.object({ key: z.string().min(1).describe("A block's or entrance's key") }),
  z.object({
    file: z.string().min(1).describe("An evidence file's path in the project (read_file reads it)"),
    lines: lineRange.describe("Its lines [from, to], 1-based and inclusive"),
  }),
]);

/** The page revision a write expects (D4). */
export const expectRevision = z
  .string()
  .regex(/^[0-9a-f]{8}$/, "expectRevision is a page's revision: the 8 hex digits after rev in a result's header")
  .optional()
  .describe("The page's revision you read (rev in a result's header); the edit is refused if the page changed since");

/** Zod's output for optional fields (`{a?: string | undefined}`) as the channel's types take them (`{a?: string}`), at every depth. */
type Defined<T> = T extends object ? { [K in keyof T]: Defined<Exclude<T[K], undefined>> } : T;

/** Arguments as the channel takes them: fields that are undefined left out, at every depth. */
export function defined<T>(value: T): Defined<T> {
  if (Array.isArray(value)) {
    return value.map(defined) as Defined<T>;
  }
  if (typeof value === "object" && value !== null) {
    return Object.fromEntries(
      Object.entries(value)
        .filter(([, field]) => field !== undefined)
        .map(([key, field]) => [key, defined(field)]),
    ) as Defined<T>;
  }
  return value as Defined<T>;
}

/** A project's apps and entrances (`entrances`), as written in its `unfold.toml`: each name by its key. */
export async function startNames(context: ToolContext, project: string): Promise<ReadonlyMap<string, string>> {
  const view = await context.engine("entrances", { project });
  return new Map(
    view.apps.flatMap((app) => [
      ...(app.key !== undefined ? [[app.key, app.name ?? app.key] as const] : []),
      ...app.entrances.flatMap((entrance) => (entrance.key !== undefined ? [[entrance.key, `${entrance.name ?? entrance.key} of ${app.name ?? "an app"}`] as const] : [])),
    ]),
  );
}

/** What a card for `key` shows: an app's start or an entrance (one of `starts`), else a block. */
export function cardRef(starts: ReadonlyMap<string, string>, key: string): AgentCardRef {
  return starts.has(key) ? { entrance: key } : { block: key };
}

/** The page a write tool edits. */
export interface EditTarget {
  readonly project: ProjectRow;
  readonly board: AgentBoardRef;
  readonly page: string;
}

/**
 * The page a write tool edits: the one its arguments name (a link in any of them names it), else the
 * page in view. A board named without a page is edited only when it has one page.
 */
export async function editTarget(context: ToolContext, args: TargetArgs): Promise<EditTarget> {
  const target = await resolveBoard(context, args);
  if (target.page !== undefined) {
    return { project: target.project, board: target.board, page: target.page };
  }
  const file = await context.engine("read_board", { project: target.board.project, id: target.board.id });
  const [only, ...others] = file.document.pages;
  if (only && others.length === 0) {
    return { project: target.project, board: target.board, page: only.id };
  }
  throw cant(
    "tell which page you mean",
    `“${file.document.title}” has ${count(file.document.pages.length, "page")} and none was named`,
    `Pass page, one of: ${file.document.pages.map((page) => `${page.title} (page id ${page.id})`).join(", ")}.`,
  );
}

/**
 * Item ids as an agent gave them, a link the person pasted turned into the id of the item it names.
 * A link must name an item of the page being edited.
 */
export async function localIds(context: ToolContext, target: EditTarget, ids: readonly string[]): Promise<string[]> {
  return Promise.all(
    ids.map(async (id) => {
      if (!isLink(id)) {
        return id;
      }
      const link = await context.link(id);
      if (link.item === undefined) {
        throw cant("use the link", "it names a board or a page, not an item", "Pass an item's id (read_page lists them), or a link to the item.");
      }
      if (link.project !== target.board.project || link.board !== target.board.id || link.page !== target.page) {
        throw cant("use the link", `it names an item of another page (${link.page ?? "the board's first page"} of ${link.board})`, "Edit one page per call: pass ids of items on the page you're editing.");
      }
      return link.item;
    }),
  );
}

/** The window's advice on a person's words, for tools that take `replacePeopleText`. */
const PEOPLE_ADVICE = /pass replacePeopleText only when the person asked for that change\./;

/** An applied edit, with the page's new revision. */
export type AppliedEdit = AgentEditSuccess & { readonly revision: string };

export interface EditRequest {
  /** Names the undo step ("Claude · Frame cards"). */
  readonly title: string;
  readonly op: AgentOp;
  readonly expectRevision?: string | undefined;
  /**
   * For a tool without `replacePeopleText`: what to do instead when the window refuses to replace a
   * person's words (in place of its advice to pass the flag).
   */
  readonly peopleAdvice?: string;
}

/** Sends one operation to the page as one undo step; a refusal becomes a {@link ToolError} that teaches. */
export async function applyEdit(context: ToolContext, target: EditTarget, request: EditRequest): Promise<AppliedEdit> {
  const edit = await context.bridge.edit({
    board: target.board,
    page: target.page,
    agent: context.agent,
    title: request.title,
    ops: [request.op],
    ...(request.expectRevision !== undefined ? { expectRevision: request.expectRevision } : {}),
  });
  if (edit.ok) {
    return edit;
  }
  if (edit.code === "people_text" && request.peopleAdvice !== undefined) {
    throw new ToolError(edit.message.replace(PEOPLE_ADVICE, request.peopleAdvice), edit.code);
  }
  throw windowRefusal(edit);
}

const STATUSES: readonly AgentItemStatus[] = ["created", "updated", "kept", "removed"];

/** `Frames (2 created, 1 kept):`: what a list of results did, by status. */
export function itemsHeading(noun: string, items: readonly AgentItemResult[]): string {
  const counts = STATUSES.flatMap((status) => {
    const n = items.filter((item) => item.status === status).length;
    return n > 0 ? [`${n} ${status}`] : [];
  });
  return `${noun.charAt(0).toUpperCase()}${noun.slice(1)} (${counts.join(", ") || "none"}):`;
}

/** One item an edit created, updated, kept, or removed, as a result line with its name and id. */
export function itemLine(item: AgentItemResult): string {
  switch (item.kind) {
    case "card":
      return `- ${cardText(item)}`;
    case "combined":
      return `- “${item.name}” (${joined([`combined step id ${item.id}`, item.collapsed ? "collapsed" : "open", item.status])}): ${item.members.join(", ")}`;
    case "link": {
      const what = `${item.connection} connection`;
      const details = [
        item.step !== undefined ? `from step ${item.step}` : null,
        item.channel !== undefined ? `channel ${item.channel}` : null,
        item.carries !== undefined ? `carries ${item.carries}` : null,
        item.name !== undefined ? `label “${item.name}”` : null,
      ].filter((detail) => detail !== null);
      return `- ${what} ${item.from} → ${item.to} (${joined([`link id ${item.id}`, item.status])})${details.length > 0 ? `: ${details.join(", ")}` : ""}`;
    }
    case "frame": {
      const shape = [item.look, item.icon, item.direction, item.collapsed === true ? "collapsed" : null, item.parent !== undefined ? `in frame ${item.parent}` : null]
        .filter((part) => part !== null && part !== undefined)
        .join(", ");
      const members = item.members.map((member) => `${member.name} (${member.kind} id ${member.id})`).join(", ");
      return `- ${item.name} (${joined([`frame id ${item.id}`, shape, item.status])})${members.length > 0 ? ` holds ${members}` : ""}`;
    }
    case "resource": {
      const columns = (item.columns ?? []).map((column) => `${column.name}${column.key !== undefined ? ` (${column.key}${column.references !== undefined ? ` → ${column.references}` : ""})` : ""}`);
      const details = [item.type !== undefined ? `backed by ${item.type}` : null, columns.length > 0 ? `columns ${columns.join(", ")}` : null].filter(
        (detail) => detail !== null,
      );
      return `- ${item.resource} “${item.name}” (${joined([`resource id ${item.id}`, item.status, item.frame !== undefined && `in frame ${item.frame}`])})${details.length > 0 ? `: ${details.join("; ")}` : ""}`;
    }
    case "note":
      return `- note “${item.name}” (${joined([`note id ${item.id}`, item.status])})${item.attach !== undefined ? `, attached to ${item.attach}` : ", free"}`;
    case "proposal": {
      const spot = "after" in item ? `after ${item.after}` : `at the start of ${item.start}`;
      return `- proposed step “${item.title}” (${joined([`proposal id ${item.id}`, item.status])}), ${spot}${item.reason !== undefined ? `; reason “${item.reason}”` : ""}`;
    }
  }
}

/** A write tool's answer: the header with the new revision, what changed, the warnings, the ids, and what to do next. */
export function editResult(context: ToolContext, target: EditTarget, edit: AppliedEdit, lines: readonly (string | null)[], next: string): ToolResult {
  return textResult(
    context.header({ project: target.project.name, board: edit.board.title, page: edit.page.title, revision: edit.revision }),
    ...lines,
    ...edit.warnings,
    `Ids: board ${target.board.id}, page ${target.page}. One undo step in Unfold, saved.`,
    `Next: ${next}`,
  );
}
