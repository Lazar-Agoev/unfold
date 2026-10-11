/**
 * What a tool works on (design §5.6, §6.1): the project, board, page, and item its arguments name, by
 * id or by a link the person pasted, or else what the person is looking at in the window they used
 * last. A board id names a board within a project; given alone, it is found among the projects'
 * boards. Reading a page goes to the window that holds its board (unsaved edits included), else to
 * the board's file.
 */

import type { AgentBoardRef, ViewReport } from "../../shared/agent-api";
import type { ToolContext } from "./context";
import type { Page, ProjectRow } from "./engine-types";
import { cant, windowRefusal, type ResultPlace } from "./results";

/** What a tool's arguments may name. Any of them may be a link (`unfold://…`), which names the rest. */
export interface TargetArgs {
  /** A project's id or shared name. */
  readonly project?: string | undefined;
  /** A board's id. */
  readonly board?: string | undefined;
  /** A page's id. */
  readonly page?: string | undefined;
  /** An item's id (a card, step group, link, frame, resource card, or note). */
  readonly item?: string | undefined;
}

/** A board a tool works on, with the page and item named or in view. */
export interface BoardTarget {
  readonly project: ProjectRow;
  readonly board: AgentBoardRef;
  readonly page?: string;
  readonly item?: string;
}

/** A page as the window holding its board has it, else as its file holds it. */
export interface ReadPage {
  readonly page: Page;
  readonly title: string;
  readonly draft: boolean;
  readonly revision: string;
  /** The window has edits it hasn't saved yet. */
  readonly unsaved: boolean;
  /** The sizes the window's canvas measured for the page's items, by id (empty when no window drew it). */
  readonly sizes: Readonly<Record<string, { readonly width: number; readonly height: number }>>;
}

const LINK = /^\s*unfold:\/\//i;

/** Whether text is a link the person pasted (`unfold://…`). */
export function isLink(text: string | undefined): boolean {
  return text !== undefined && LINK.test(text);
}

/** The view of the window the person used last, if any window reported one. */
export function currentView(context: ToolContext): ViewReport | undefined {
  const windowId = context.views.lastUsed();
  return windowId === undefined ? undefined : context.views.get(windowId);
}

async function projectById(context: ToolContext, id: string): Promise<ProjectRow> {
  const row = (await context.projects()).find((project) => project.id === id);
  if (!row) {
    throw cant("find the project", `the project with id “${id}” is no longer in the workspace`, "list_projects shows each project's name and id.");
  }
  return row;
}

/** The project whose boards include `id`: the board in view, else the one project that has it. */
async function projectOfBoard(context: ToolContext, id: string): Promise<ProjectRow> {
  const view = currentView(context);
  if (view?.board?.id === id) {
    return projectById(context, view.board.project);
  }
  const rows = await context.projects();
  const holders = (
    await Promise.all(
      rows.map((row) =>
        context.engine("list_boards", { project: row.id }).then(
          (list) => (list.boards.some((board) => board.id === id) ? [row] : []),
          () => [],
        ),
      ),
    )
  ).flat();
  const [only, ...others] = holders;
  if (!only) {
    throw cant("find the board", `no project has a board with id “${id}”`, "list_boards lists each project's boards with their ids; a draft's id changes when it is published.");
  }
  if (others.length > 0) {
    throw cant("tell which board you mean", `${holders.map((row) => row.name).join(" and ")} each have a board with id “${id}”`, "Pass project as well, or a link to the board.");
  }
  return only;
}

/**
 * The project a tool works in: the one named, else the project of the board in view, else the
 * workspace's only project.
 */
export async function projectOrView(context: ToolContext, named: string | undefined): Promise<ProjectRow> {
  if (named !== undefined) {
    return context.project(named);
  }
  const view = currentView(context);
  if (view?.board) {
    return projectById(context, view.board.project);
  }
  const rows = await context.projects();
  const [only, ...others] = rows;
  if (only && others.length === 0) {
    return only;
  }
  throw cant(
    "tell which project you mean",
    rows.length === 0 ? "the workspace has no projects" : `the workspace has ${rows.length} projects and no board is in view`,
    "Pass project: a name or id from list_projects.",
  );
}

/**
 * The board, page, and item the arguments name (a link in any of them names the rest; plain ids in
 * the others override its page and item), else the board and page in view.
 */
export async function resolveBoard(context: ToolContext, args: TargetArgs): Promise<BoardTarget> {
  const linked = [args.item, args.page, args.board, args.project].find(isLink);
  const page = isLink(args.page) ? undefined : args.page;
  const item = isLink(args.item) ? undefined : args.item;
  if (linked !== undefined) {
    const link = await context.link(linked);
    const project = await projectById(context, link.project);
    const named = { page: page ?? link.page, item: item ?? link.item };
    return { project, board: { project: project.id, id: link.board }, ...withDefined(named) };
  }
  const view = currentView(context);
  if (args.board !== undefined) {
    const project = args.project !== undefined ? await context.project(args.project) : await projectOfBoard(context, args.board);
    const inView = view?.board?.project === project.id && view.board.id === args.board ? view.page : undefined;
    return { project, board: { project: project.id, id: args.board }, ...withDefined({ page: page ?? inView, item }) };
  }
  if (!view?.board) {
    throw cant(
      "tell which board you mean",
      "no board is in view in Unfold, and none was named",
      "Pass board (an id from list_boards) or a link the person pasted, or ask them to open the board.",
    );
  }
  const project = await projectById(context, view.board.project);
  return { project, board: view.board, ...withDefined({ page: page ?? view.page, item }) };
}

/** The fields of `values` that are defined (for `exactOptionalPropertyTypes`). */
function withDefined<T extends Record<string, string | undefined>>(values: T): { [K in keyof T]?: string } {
  return Object.fromEntries(Object.entries(values).filter(([, value]) => value !== undefined)) as { [K in keyof T]?: string };
}

/**
 * Reads a page (`page` absent: the board's first page, as its file has them) from the window that
 * holds its board, unsaved edits included, else from the board's working file.
 */
export async function readPage(context: ToolContext, board: AgentBoardRef, page: string | undefined): Promise<ReadPage> {
  const listed = page === undefined ? await context.engine("read_board", { project: board.project, id: board.id }) : undefined;
  const id = page ?? listed?.document.pages[0]?.id;
  if (id !== undefined) {
    const held = await context.bridge.read({ board, page: id });
    if (held.ok) {
      return {
        page: held.page as unknown as Page,
        title: held.board.title,
        draft: held.board.place === "draft",
        revision: held.revision,
        unsaved: held.board.unsaved,
        sizes: held.sizes ?? {},
      };
    }
    if (held.code !== "board_not_open") {
      throw windowRefusal(held);
    }
  }
  const file = listed ?? (await context.engine("read_board", { project: board.project, id: board.id }));
  const found = file.document.pages.find((item) => item.id === id);
  if (!found) {
    throw cant(
      "find the page",
      `the board “${file.document.title}” has no page with id “${id ?? ""}”`,
      `Its pages: ${file.document.pages.map((item) => `${item.title} (id ${item.id})`).join(", ")}.`,
    );
  }
  return { page: found, title: file.document.title, draft: file.place === "draft", revision: context.revision(found), unsaved: false, sizes: {} };
}

/** The header place of a page that was read. */
export function pagePlace(project: ProjectRow, read: ReadPage): ResultPlace {
  return { project: project.name, board: { title: read.title, draft: read.draft }, page: read.page.title, revision: read.revision };
}

/** Compiled code a tool reads: a key in a project, and the step selected in it. */
export interface CodeTarget {
  readonly project: ProjectRow;
  readonly key: string;
  /** The Flow step or statement the person selected in the card. */
  readonly step?: string;
}

/** The key a card on a page shows, in the project it reads from (its `ref.project` names one by shared name). */
async function cardCode(context: ToolContext, target: BoardTarget, page: Page, id: string): Promise<CodeTarget> {
  const card = page.cards.find((item) => item.id === id);
  if (!card) {
    throw cant("read the card", `there is no card with id “${id}” on the page “${page.title}”`, "read_page lists the page's cards with their ids and keys.");
  }
  const key = "block" in card.ref ? card.ref.block : card.ref.entrance;
  const named = card.ref.project;
  if (named === undefined) {
    return { project: target.project, key };
  }
  const project = (await context.projects()).find((row) => row.name === named);
  if (!project) {
    throw cant("read the card", `it comes from the project “${named}”, which isn't in the workspace`, "Ask the person to add that project, or read another card.");
  }
  return { project, key };
}

/**
 * The code a read tool works on: `key` (a key, or a link to a card), else the card selected in the
 * person's view (a selected step's or step group's card too).
 */
export async function codeTarget(context: ToolContext, args: { readonly key?: string | undefined; readonly project?: string | undefined }): Promise<CodeTarget> {
  if (args.key !== undefined && isLink(args.key)) {
    const target = await resolveBoard(context, { item: args.key });
    if (target.item === undefined) {
      throw cant("read the code", "the link names a board or a page, not a card", "Pass a key (from search or read_page), or a link to a card.");
    }
    const read = await readPage(context, target.board, target.page);
    return cardCode(context, target, read.page, target.item);
  }
  if (args.key !== undefined) {
    return { project: await projectOrView(context, args.project), key: args.key };
  }
  const view = currentView(context);
  const selection = view?.selection;
  if (view?.board && view.page !== undefined && selection && (selection.kind === "card" || selection.kind === "step" || selection.kind === "group")) {
    const target = await resolveBoard(context, {});
    const read = await readPage(context, view.board, view.page);
    const cardId = selection.kind === "card" ? selection.id : selection.kind === "step" ? selection.card : read.page.groups.find((group) => group.id === selection.id)?.card;
    if (cardId !== undefined) {
      const code = await cardCode(context, target, read.page, cardId);
      return selection.kind === "step" ? { ...code, step: selection.key } : code;
    }
  }
  throw cant(
    "tell which code you mean",
    "no key was given and no card is selected in Unfold",
    "Pass key: from search, list_entrances, read_page, or get_relations.",
  );
}
