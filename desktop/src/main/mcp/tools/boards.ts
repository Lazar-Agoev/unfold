/**
 * Navigate (design §6.3): `create_board`, `organize_boards`, `edit_pages`, and `show`.
 *
 * A board is a file in `.unfold/boards/`, and the window holds a session for each board it loaded, so
 * everything that changes a board goes to the window (loading the board's session first, without
 * changing what the person sees): a new board's file is created there, so the window holds it; a
 * title or page change is one undo step there. Folders are the engine's, so folder operations and
 * moving a board into a folder go straight to it. Agents never delete a board.
 */

import { z } from "zod";
import type { AgentBoardOp, AgentBoardOpResult, AgentBoardRef, AgentPageSummary } from "../../../shared/agent-api";
import type { ToolContext } from "../context";
import type { Page, ProjectRow } from "../engine-types";
import { cardText, keyName } from "../format";
import { cant, textResult, ToolError, windowRefusal } from "../results";
import { pagePlace, readPage, resolveBoard } from "../target";
import { defineTool } from "../tool";

function pagesText(pages: readonly AgentPageSummary[]): string {
  return pages.map((page, index) => `${index + 1}. ${page.title} (page id ${page.id})`).join("\n");
}

const boardArg = z.string().min(1).describe("A board's id (from list_boards) or a link the person pasted");

// ---------------------------------------------------------------------------------------------
// create_board

/** Puts an entrance and its handlers on the board's first page, as Trace on a New Board does. */
async function traced(context: ToolContext, board: AgentBoardRef, page: string, entrance: string): Promise<string[]> {
  const edit = await context.bridge.edit({
    board,
    page,
    agent: context.agent,
    title: "Trace",
    ops: [{ op: "add_cards", cards: [{ key: entrance, follow: { callees: 1 } }] }],
  });
  if (!edit.ok) {
    return [`Couldn't trace ${keyName(entrance)} onto it: ${edit.message} add_cards with the entrance's key places it.`];
  }
  const cards = edit.items.flatMap((item) => (item.kind === "card" ? [item] : []));
  return [`Traced ${keyName(entrance)} on the first page (rev ${edit.revision}):`, ...cards.map((card) => `- ${cardText(card)}`), ...edit.warnings.map((warning) => `Warning: ${warning}`)];
}

export const createBoard = defineTool({
  name: "create_board",
  title: "Create board",
  description:
    'Creates a board in a project, a file in .unfold/boards/ saved as it is edited, and loads it in Unfold without moving the person\'s view. title names the story ("Order placement"); pages titles its pages, in order (default one page, "Page 1"); folder files it (one name: folders are one level deep). trace, an entrance\'s key (from list_entrances or search), puts that entrance and its handlers on the first page, as Trace on a New Board does, but compact: expand the ones the page is about with set_view. Following stops at external and unresolved handlers, and the result says where. The result gives the board\'s id and each page\'s id: pass them to the next tools, since the person may switch boards. Before building a new page, read get_guide("diagrams") and post a short diagram brief in chat. When the work is ready, show presents it.',
  input: z.object({
    project: z.string().min(1).describe("The project's name or id, or a link to one of its boards"),
    title: z.string().trim().min(1).max(200).describe("The board's title"),
    folder: z.string().min(1).optional().describe("The board folder's name (absent: the top level)"),
    trace: z.string().startsWith("entrance:").optional().describe("An entrance's key to trace onto the first page"),
    pages: z.array(z.string().trim().min(1).max(200)).min(1).max(20).optional().describe("The pages' titles, in order"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false },
  async run({ project, title, folder, trace, pages }, context) {
    const row = await context.project(project);
    if (trace !== undefined) {
      // Refuse an unknown entrance before anything is created.
      await context.engine("peek", { project: row.id, key: trace }).catch((error: unknown) => {
        throw error instanceof ToolError && error.code === "not_found"
          ? cant("trace the entrance", `${row.name} has no entrance with the key ${trace}`, "list_entrances lists its entrances with their keys.")
          : error;
      });
    }
    const created = await context.bridge.create({
      project: row.id,
      title,
      ...(folder !== undefined ? { folder } : {}),
      ...(pages !== undefined ? { pages } : {}),
    });
    if (!created.ok) {
      throw windowRefusal(created);
    }
    const [first] = created.pages;
    const traceLines = trace !== undefined && first ? await traced(context, created.board, first.id, trace) : [];
    const read = first ? await readPage(context, created.board, first.id) : null;
    return textResult(
      context.header(read ? pagePlace(row, read) : { project: row.name, board: created.info.title }),
      `Created the board “${created.info.title}” (board id ${created.board.id})${created.folder !== null ? ` in the folder ${created.folder}` : ""}. Unfold loaded it; the person's view didn't move.`,
      `Pages:\n${pagesText(created.pages)}`,
      ...traceLines,
      "Next: add_cards to build a page (pass the board and page ids), then arrange_page and check. show presents it when it's ready.",
    );
  },
});

// ---------------------------------------------------------------------------------------------
// organize_boards

const FORBIDDEN = "Agents can't delete boards: that destroys work, so it is the person's, from the board's menu.";

const organizeOp = z.discriminatedUnion(
  "op",
  [
    z.object({ op: z.literal("create_folder"), path: z.string().min(1).describe("The folder's name (orders)") }),
    z.object({ op: z.literal("rename_folder"), from: z.string().min(1), to: z.string().min(1) }),
    z.object({ op: z.literal("delete_folder"), path: z.string().min(1).describe("An empty folder") }),
    z.object({ op: z.literal("move"), board: boardArg, folder: z.string().min(1).nullable().describe("The folder; null for the top level") }),
    z.object({ op: z.literal("rename"), board: boardArg, title: z.string().trim().min(1).max(200) }),
  ],
  { error: () => `op is one of create_folder, rename_folder, delete_folder, move, or rename. ${FORBIDDEN}` },
);

type OrganizeOp = z.output<typeof organizeOp>;

/** A board of `row` an op names: by id, or by a link to a board of the same project. */
async function boardOf(context: ToolContext, row: ProjectRow, board: string): Promise<AgentBoardRef> {
  const target = await resolveBoard(context, { project: row.id, board });
  if (target.project.id !== row.id) {
    throw cant("organize the board", `the link names a board of ${target.project.name}, not ${row.name}`, "Organize each project's boards in its own call.");
  }
  return target.board;
}

/** Does one op and says what it did. */
async function organize(context: ToolContext, row: ProjectRow, op: OrganizeOp): Promise<string> {
  const project = row.id;
  switch (op.op) {
    case "create_folder":
      await context.engine("create_folder", { project, path: op.path });
      return `Created the folder ${op.path}.`;
    case "rename_folder": {
      const renamed = await context.engine("rename_folder", { project, from: op.from, to: op.to });
      return `Renamed the folder ${op.from} to ${op.to} (${renamed.boards} boards moved).`;
    }
    case "delete_folder":
      await context.engine("delete_folder", { project, path: op.path }).catch((error: unknown) => {
        throw error instanceof ToolError && error.code === "folder_not_empty"
          ? new ToolError(`${error.message} Move its boards out first (move); agents never delete boards.`, error.code)
          : error;
      });
      return `Deleted the empty folder ${op.path}.`;
    case "move": {
      const board = await boardOf(context, row, op.board);
      const moved = await context.engine("set_board_folder", { project, id: board.id, folder: op.folder });
      const name = moved.title ?? moved.name;
      return `Moved “${name}” (board id ${moved.id}) ${op.folder === null ? "to the top level" : `into ${op.folder}`}.`;
    }
    case "rename": {
      const board = await boardOf(context, row, op.board);
      const edit = await context.bridge.editBoard({ board, agent: context.agent, title: "Rename board", ops: [{ op: "set_title", title: op.title }] });
      if (!edit.ok) {
        throw windowRefusal(edit);
      }
      return edit.results[0]?.status === "kept" ? `“${op.title}” (board id ${board.id}) already has that title.` : `Renamed board ${board.id} to “${edit.board.title}” (one undo step in Unfold).`;
    }
  }
}

export const organizeBoards = defineTool({
  name: "organize_boards",
  title: "Organize boards",
  description: `Organizes a project's boards and folders. ops run in order: create_folder; rename_folder (its boards move with it); delete_folder (an empty one only); move (a board into a folder, or null for the top level); rename (a board's title: one undo step in Unfold, which loads the board without moving the person's view). board is a board's id (from list_boards) or a link the person pasted. If an op fails, the result says what was done before it and why it failed; the rest isn't tried. Folders are one level deep: a folder is one name, such as orders. ${FORBIDDEN}`,
  input: z.object({
    project: z.string().min(1).describe("The project's name or id, or a link to one of its boards"),
    ops: z.array(organizeOp).min(1).describe("The changes, in order"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false },
  async run({ project, ops }, context) {
    const row = await context.project(project);
    const done: string[] = [];
    for (const op of ops) {
      try {
        done.push(await organize(context, row, op));
      } catch (error) {
        if (error instanceof ToolError && done.length > 0) {
          throw new ToolError(`${error.message}\nDone before it:\n${done.map((line) => `- ${line}`).join("\n")}\nThe ops after it weren't tried.`, error.code);
        }
        throw error;
      }
    }
    return textResult(context.header({ project: row.name }), ...done.map((line) => `- ${line}`), "Next: list_boards shows the boards and folders now.");
  },
});

// ---------------------------------------------------------------------------------------------
// edit_pages

const pageOp = z.discriminatedUnion("op", [
  z.object({ op: z.literal("add"), title: z.string().trim().min(1).max(200), after: z.string().min(1).optional().describe("The page it follows (absent: last)") }),
  z.object({ op: z.literal("rename"), page: z.string().min(1), title: z.string().trim().min(1).max(200) }),
  z.object({ op: z.literal("move"), page: z.string().min(1), to: z.number().int().min(0).describe("Its new position, 0 for first") }),
  z.object({ op: z.literal("delete"), page: z.string().min(1) }),
]);

function boardOp(op: z.output<typeof pageOp>): AgentBoardOp {
  switch (op.op) {
    case "add":
      return { op: "add_page", title: op.title, ...(op.after !== undefined ? { after: op.after } : {}) };
    case "rename":
      return { op: "rename_page", page: op.page, title: op.title };
    case "move":
      return { op: "move_page", page: op.page, to: op.to };
    case "delete":
      return { op: "delete_page", page: op.page };
  }
}

const DONE: Readonly<Record<AgentBoardOpResult["op"], string>> = {
  add_page: "Added",
  rename_page: "Renamed",
  move_page: "Moved",
  delete_page: "Deleted",
  set_title: "Retitled the board",
};

function resultText(result: AgentBoardOpResult): string {
  const page = result.page === undefined ? `“${result.title}”` : `the page “${result.title}” (page id ${result.page})`;
  return result.status === "kept" ? `${page} was already so.` : `${DONE[result.op]} ${page}.`;
}

export const editPages = defineTool({
  name: "edit_pages",
  title: "Edit pages",
  description:
    'Adds, renames, moves, and deletes pages of a board, as one undo step in Unfold (which loads the board without moving the person\'s view), saved at once. board is a board\'s id or a link (default: the board in view). ops apply in order: add (title; after: a page id, default last), rename (page, title), move (page, to: its new position, 0 first), and delete (page; a board keeps at least one). Deleting a page that holds text a person wrote (a description, label, title, or note) is refused unless replacePeopleText is true: set it only when the person asked for that page to go. A page\'s title says what it explains, in 1 to 6 words ("Placing an order"). The result lists the pages in order with their ids; pass a page\'s id to the tools that build it.',
  input: z.object({
    board: boardArg.optional(),
    project: z.string().min(1).optional().describe("The board's project, when its id isn't enough"),
    ops: z.array(pageOp).min(1).describe("The changes, in order"),
    replacePeopleText: z.boolean().optional().describe("Only when the person asked to delete pages with their words on them"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false },
  async run({ board, project, ops, replacePeopleText }, context) {
    const target = await resolveBoard(context, { board, project });
    const edit = await context.bridge.editBoard({
      board: target.board,
      agent: context.agent,
      title: "Edit pages",
      ops: ops.map(boardOp),
      ...(replacePeopleText !== undefined ? { replacePeopleText } : {}),
    });
    if (!edit.ok) {
      throw windowRefusal(edit);
    }
    return textResult(
      context.header({ project: target.project.name, board: edit.board.title }),
      `Board id ${target.board.id}: one undo step in Unfold.`,
      ...edit.results.map((result) => `- ${resultText(result)}`),
      `Pages now:\n${pagesText(edit.pages)}`,
      "Next: add_cards with a page's id builds it.",
    );
  },
});

// ---------------------------------------------------------------------------------------------
// show

/** An item of a page as the person sees it: `the card createOrder`, `the frame “Request checks”`. */
function itemName(page: Page, id: string): string {
  const card = page.cards.find((item) => item.id === id);
  if (card) {
    return `the card ${keyName("block" in card.ref ? card.ref.block : card.ref.entrance)}`;
  }
  const named =
    page.groups.find((item) => item.id === id)?.title ??
    page.frames?.find((item) => item.id === id)?.title ??
    page.resources?.find((item) => item.id === id)?.name;
  return named === undefined ? `the item ${id}` : `“${named}”`;
}

export const show = defineTool({
  name: "show",
  title: "Show",
  description:
    'Shows the person a board, a page, or an item in Unfold. Use it only when they ask to see something, or to present finished work, never while building: it moves their view. mode "focus" (the default) brings the window forward, switches to the board and the page, and selects and centers the item; "background" only loads the board in Unfold, without switching to it. board, page, and item are ids or a link (default: the board and page in view); an item is a card, step group, link, frame, resource card, or note of the page.',
  input: z.object({
    board: boardArg.optional(),
    page: z.string().min(1).optional().describe("A page's id or a link (absent: the page the board shows)"),
    item: z.string().min(1).optional().describe("An item's id on the page, or a link to it"),
    project: z.string().min(1).optional().describe("The board's project, when its id isn't enough"),
    mode: z.enum(["focus", "background"]).default("focus").describe("focus brings it into view; background only loads it"),
  }),
  annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true },
  async run({ board, page, item, project, mode }, context) {
    const target = await resolveBoard(context, { board, page, item, project });
    if (mode === "background" && target.item !== undefined) {
      throw cant("show the item in the background", "a board loaded in the background shows nothing to the person", 'Use mode "focus" to select and center it, or leave item out.');
    }
    const shown = await context.bridge.show(
      { board: target.board, ...(target.page !== undefined ? { page: target.page } : {}), ...(target.item !== undefined ? { item: target.item } : {}) },
      mode === "background",
    );
    if (!shown.ok) {
      throw windowRefusal(shown);
    }
    const read = await readPage(context, target.board, shown.page ?? target.page);
    const where = `the page “${read.page.title}” of “${read.title}”`;
    return textResult(
      context.header(pagePlace(target.project, read)),
      mode === "background"
        ? `Loaded “${read.title}” in Unfold; the person's view didn't move.`
        : target.item === undefined
          ? `Showed ${where}; its window came to the front.`
          : `Showed ${itemName(read.page, target.item)} on ${where}, selected and centered; its window came to the front.`,
      `Ids: board ${target.board.id}, page ${read.page.id}.`,
    );
  },
});
