/**
 * Read boards: `list_boards` and `read_page`.
 *
 * `list_boards` lists a project's boards (`list_boards`, which never wakes a project), last edited
 * first, with each board's pages from its file (`read_board`). `read_page` reads a page as the
 * window holding its board has it (unsaved edits included), else as its file has it, then the code
 * of its cards, so it can say what the board draws between them, how each card shows (compact or
 * expanded, and the calls it opens in place, with an expanded card's drawn flow from `flow_text`),
 * and where each proposed step goes, with a note when its anchor is missing or held (from the
 * board's health while a window holds the board).
 */

import { z } from "zod";
import type { AgentBoardRef, AgentIdentity } from "../../../shared/agent-api";
import type { ToolContext } from "../context";
import type { Anchor, BoardHealthView, BoardSummary, Connection, Frame, Page, Proposal, Resource } from "../engine-types";
import { authorText, callPathText, clip, count, joined, place, typeText } from "../format";
import { cardView, pageCode, type CardCode, type PageCode } from "../page-code";
import { nextLine, paged, pagedLines, textResult } from "../results";
import { currentView, readPage as loadPage, pagePlace, projectOrView, resolveBoard, type ReadPage } from "../target";
import { defineTool } from "../tool";

/** Boards per page of `list_boards`. */
const BOARDS_PER_PAGE = 30;
/** Lines per page of `read_page` (fewer when they are long). */
const LINES_PER_PAGE = 120;

/** How a page shows the cards without their own setting, by its default view. */
const DEFAULT_VIEW = {
  compact: "Cards without their own setting are compact.",
  expanded: "Cards without their own setting are expanded.",
} as const;

const cursorArg = z.string().optional().describe("The next cursor a result gave (absent: the first page)");

// ---------------------------------------------------------------------------------------------
// list_boards

/** A board's pages from its file, or how many it has when the file can't be read. */
async function pagesText(context: ToolContext, board: BoardSummary): Promise<string> {
  try {
    const file = await context.engine("read_board", { project: board.project, id: board.id });
    return `pages: ${file.document.pages.map((page) => `${page.title} (page id ${page.id})`).join(", ")}`;
  } catch {
    return count(board.pages, "page");
  }
}

/** A board's lines in a listing: its title, id, and state, then its pages. */
async function boardLines(context: ToolContext, board: BoardSummary): Promise<string[]> {
  const ref = { project: board.project, id: board.id };
  const view = currentView(context);
  const held = context.views.holds(ref);
  const facts = [
    board.folder !== null ? `in ${board.folder}` : null,
    `edited ${new Date(board.updatedAtMs).toISOString()}`,
    count(board.cards, "card"),
    board.label !== null ? `needs review: ${board.label} (check lists it)` : null,
    board.error !== null ? `can't be read: ${board.error}` : null,
    view?.board?.project === ref.project && view.board.id === ref.id ? "in view" : held ? "open in Unfold" : null,
  ].filter((fact) => fact !== null);
  return [`- ${board.title ?? board.name} (board id ${board.id}): ${facts.join(", ")}`, `  ${board.error === null ? await pagesText(context, board) : count(board.pages, "page")}`];
}

export const listBoards = defineTool({
  name: "list_boards",
  title: "List boards",
  description:
    'A project\'s boards, last edited first: each board\'s title and id, its pages with their ids, folder, when it was last edited, card count, health ("needs review: 1 missing · 2 outdated"; check lists the problems and their repairs), and whether it is open in Unfold or in view; then the empty folders. project is a name, id, or link (default: the project of the board in view, else the only project). Pass a board\'s and a page\'s id to read_page, check, and the tools that build pages. Reading changes nothing.',
  input: z.object({
    project: z.string().min(1).optional().describe("The project's name or id, or a link to one of its boards"),
    cursor: cursorArg,
  }),
  annotations: { readOnlyHint: true },
  async run({ project, cursor }, context) {
    const row = await projectOrView(context, project);
    const list = await context.engine("list_boards", { project: row.id });
    const boards = list.boards.toSorted((a, b) => b.updatedAtMs - a.updatedAtMs);
    const page = paged(boards, cursor, BOARDS_PER_PAGE);
    const lines = await Promise.all(page.items.map((board) => boardLines(context, board)));
    return textResult(
      context.header({ project: row.name }),
      `${row.name} (project id ${row.id}) has ${count(boards.length, "board")}${boards.length > 0 ? ":" : "."}`,
      ...lines.flat(),
      list.emptyFolders.length > 0 && page.next === undefined ? `Empty folders: ${list.emptyFolders.join(", ")}.` : null,
      nextLine(page, "list_boards"),
      boards.length > 0 ? "Next: read_page with a board's and a page's id; check with a board's id lists its problems." : "Next: create_board makes one.",
    );
  },
});

// ---------------------------------------------------------------------------------------------
// read_page

/** Who wrote a text, and the text: `by you: “Places the order…”`. */
function wordsText(label: string, words: { readonly text: string; readonly author: string } | undefined, agent: AgentIdentity): string | null {
  return words ? `${label} by ${authorText(words.author, agent)}: “${clip(words.text, 200)}”` : null;
}

/** Unfold's name for an item of the page, as results show it: a card's name, a frame's title, a resource card's name. */
function itemName(page: Page, code: PageCode, id: string): string {
  const card = code.cards.get(id);
  if (card) {
    return cardName(card);
  }
  const named = page.frames?.find((frame) => frame.id === id)?.title ?? page.resources?.find((resource) => resource.id === id)?.name;
  if (named !== undefined) {
    return named;
  }
  const note = page.notes?.find((item) => item.id === id);
  return note ? `note “${clip(note.text, 40)}”` : id;
}

/** `createOrder (create-order)`: an item's name and its id. */
function named(page: Page, code: PageCode, id: string): string {
  return `${itemName(page, code, id)} (${id})`;
}

/** Unfold's name for a card: the page's name for it, else its code name (`OrderService.place`, `Place an order`). */
function cardName({ model, key }: CardCode): string {
  if (model?.type === "definition") {
    return model.flow.header.name ?? model.flow.header.label;
  }
  return model?.type === "start" ? model.name : key;
}

/** Where a card's code is: `function at src/routes/orders.ts:25`, `entrance of shop-api`. */
function cardWhere({ model }: CardCode): string | null {
  if (model?.type === "definition") {
    return `${model.flow.header.form} at ${place(model.flow.header.path, model.flow.header.line)}`;
  }
  return model?.type === "start" ? (model.kind === "app" ? "app" : `entrance of ${model.app}`) : null;
}

/** How a card shows: a type's fields, else compact or expanded (and whose setting that is), with the calls it opens in place. */
function expansionText(page: Page, code: CardCode): string {
  const { card, model } = code;
  if (model?.type === "definition" && model.flow.header.kind === "type") {
    return "fields";
  }
  const shown = `${cardView(page, card)}${card.view === undefined ? " (the page's default)" : ""}`;
  const open = card.open ?? [];
  return open.length > 0 ? `${shown}, calls open in place ${open.map(callPathText).join(", ")}` : shown;
}

/** A card's lines: its name, id, code, view, place on the canvas, frame, and description; then an expanded card's drawn flow. */
function cardLines(page: Page, read: ReadPage, code: CardCode, frames: ReadonlyMap<string, string>, agent: AgentIdentity): string[] {
  const { card } = code;
  const size = read.sizes[card.id];
  const frame = frames.get(card.id);
  const line = `- ${cardName(code)} (card id ${card.id}): ${joined([
    cardWhere(code),
    `key ${code.key}`,
    expansionText(page, code),
    `at [${card.at.join(", ")}]${size ? `, ${Math.round(size.width)}×${Math.round(size.height)}` : ""}`,
    frame !== undefined ? `in frame ${frame}` : null,
    code.problem !== undefined ? `code unavailable: ${code.problem}` : null,
    wordsText("description", card.description, agent),
  ])}`;
  const flow = code.flow === null ? [] : code.flow.trimEnd().split("\n").map((text) => `    ${text}`);
  return [line, ...flow];
}

/** The frames as a tree: each frame, then the frames nested in it, indented. */
function frameLines(page: Page, code: PageCode, agent: AgentIdentity): string[] {
  const frames = page.frames ?? [];
  const nested = new Set(frames.flatMap((frame) => frame.members));
  const line = (frame: Frame, depth: number): string[] => {
    const shape = [frame.look, frame.icon, frame.direction, frame.collapsed === true ? "collapsed" : null].filter((part) => part !== null && part !== undefined).join(", ");
    const children = frame.members.flatMap((id) => {
      const child = frames.find((item) => item.id === id);
      return child ? line(child, depth + 1) : [];
    });
    const members = frame.members.filter((id) => !frames.some((item) => item.id === id)).map((id) => named(page, code, id));
    return [
      `${"  ".repeat(depth)}- ${frame.title} (frame id ${frame.id}; ${shape}; by ${authorText(frame.author, agent)}): ${joined([
        members.length > 0 ? `holds ${members.join(", ")}` : "holds only frames",
        wordsText("description", frame.description, agent),
      ])}`,
      ...children,
    ];
  };
  return frames.filter((frame) => !nested.has(frame.id)).flatMap((frame) => line(frame, 0));
}

/** An anchor as results cite it: its key, or `db/schema.sql:3–9`. */
function anchorText(anchor: Anchor): string {
  return "key" in anchor ? anchor.key : place(anchor.file, anchor.lines[0], anchor.lines[1]);
}

/** A resource card's line: its kind, name, id, place, type, columns, anchors, frame, and words. */
function resourceLine(resource: Resource, frames: ReadonlyMap<string, string>, agent: AgentIdentity): string {
  const columns = (resource.columns ?? []).map(
    (column) =>
      `${column.name}${column.type !== undefined ? ` ${column.type}` : ""}${column.optional === true ? "?" : ""}${column.key !== undefined ? ` (${column.key}${column.references !== undefined ? ` → ${column.references}` : ""})` : ""}`,
  );
  const frame = frames.get(resource.id);
  return `- ${resource.kind} “${resource.name}” (resource id ${resource.id}): ${joined([
    `at [${resource.at.join(", ")}]`,
    resource.type !== undefined ? `backed by ${resource.type}` : null,
    columns.length > 0 ? `${resource.type !== undefined ? "marks" : "columns"} ${columns.join(", ")}` : null,
    resource.anchors.length > 0 ? `anchors ${resource.anchors.map(anchorText).join(", ")}` : "no anchors",
    frame !== undefined ? `in frame ${frame}` : null,
    `by ${authorText(resource.author, agent)}`,
    wordsText("description", resource.description, agent),
  ])}`;
}

/** A connection's line: its kind, ends, id, step, channel, carried type, anchors, and words. */
function linkLine(page: Page, code: PageCode, link: Connection, agent: AgentIdentity): string {
  return `- ${link.kind} connection ${named(page, code, link.from)} → ${named(page, code, link.to)} (link id ${link.id}): ${joined([
    link.step !== undefined ? `from the call ${link.step}` : null,
    link.fromColumn !== undefined || link.toColumn !== undefined ? `columns ${link.fromColumn ?? "?"} → ${link.toColumn ?? "?"}` : null,
    link.cardinality,
    link.channel !== undefined ? `channel ${link.channel}` : null,
    link.carries !== undefined ? `carries ${link.carries}` : null,
    link.anchors !== undefined && link.anchors.length > 0 ? `anchors ${link.anchors.map(anchorText).join(", ")}` : "no anchors",
    `by ${authorText(link.author, agent)}`,
    wordsText("label", link.label, agent),
    wordsText("description", link.description, agent),
  ]) || "no label"}`;
}

/** What the board's health says of each proposed step's anchor, by proposal id. */
type ProposalHealth = ReadonlyMap<string, string>;

const ANCHOR_HEALTH: Readonly<Record<string, string>> = { missing: "anchor missing", review: "anchor held for review" };

/**
 * What the board's health says of the page's proposed steps, when a window holds the board (its
 * session answers `board_health`); empty otherwise, or when the health can't be read.
 */
async function proposalHealth(context: ToolContext, board: AgentBoardRef, page: Page): Promise<ProposalHealth> {
  if ((page.proposals ?? []).length === 0 || !context.views.holds(board)) {
    return new Map();
  }
  try {
    const view: BoardHealthView = await context.held(board, "board_health", { project: board.project, id: board.id });
    return new Map(
      view.health.items.flatMap((item) =>
        item.page === page.id && "proposal" in item.target ? [[item.target.proposal, `${ANCHOR_HEALTH[item.kind] ?? item.kind}: ${item.message}`] as const] : [],
      ),
    );
  } catch {
    return new Map();
  }
}

/** A proposed step's line: its title, id, where it goes, who proposed it, its reason, and its anchor's health. */
function proposalLine(proposal: Proposal, health: ProposalHealth, agent: AgentIdentity): string {
  const anchor = health.get(proposal.id);
  return `- “${proposal.title}” (proposal id ${proposal.id}): ${joined([
    "after" in proposal ? `after ${proposal.after}` : `at the start of ${proposal.start}`,
    `by ${authorText(proposal.author, agent)}`,
    proposal.reason !== undefined ? `reason “${clip(proposal.reason, 200)}”` : null,
    anchor !== undefined ? `${anchor} (propose with its id and a new spot moves it; remove_items takes it off)` : null,
  ])}`;
}

/** The page's items, section by section, as lines. */
function pageLines(page: Page, read: ReadPage, code: PageCode, health: ProposalHealth, agent: AgentIdentity): string[] {
  const frames = new Map((page.frames ?? []).flatMap((frame) => frame.members.map((member) => [member, frame.id] as const)));
  const section = (title: string, lines: readonly string[]) => (lines.length > 0 ? [`${title} (${lines.length}):`, ...lines] : []);
  const cards = page.cards.flatMap((card) => {
    const found = code.cards.get(card.id);
    return found ? cardLines(page, read, found, frames, agent) : [];
  });
  const names = page.names.map((name) => `- “${name.text}” on ${name.key}, by ${authorText(name.author, agent)}`);
  const combined = page.combined.map(
    (entry) =>
      `- “${entry.title}” (combined step id ${entry.id}): ${joined([
        `combines ${entry.members.join(", ")}`,
        entry.collapsed ? "collapsed" : "open",
        `titled by ${authorText(entry.author, agent)}`,
        wordsText("description", entry.description, agent),
      ])}`,
  );
  const notes = (page.notes ?? []).map(
    (note) =>
      `- note ${note.id}, ${note.attach !== undefined ? `attached to ${named(page, code, note.attach)} at offset [${note.at.join(", ")}]` : `free at [${note.at.join(", ")}]`}, by ${authorText(note.author, agent)}: “${note.text}”`,
  );
  const arrows = code.arrows.map(
    (arrow) => `- ${named(page, code, arrow.from)} → ${named(page, code, arrow.to)}: ${arrow.call !== undefined ? `calls ${arrow.text} (call key ${arrow.call})` : `starts at ${arrow.text}`}`,
  );
  const typeArrows = code.typeArrows.map((arrow) => `- ${named(page, code, arrow.from)} → ${named(page, code, arrow.to)}: field ${arrow.field}: ${typeText(arrow.type)}`);
  return [
    ...(page.cards.length > 0 ? [`Cards (${page.cards.length}):`, ...cards] : []),
    ...section("Names", names),
    ...section("Combined steps", combined),
    ...((page.frames ?? []).length > 0 ? [`Frames (${(page.frames ?? []).length}), nested as they are:`, ...frameLines(page, code, agent)] : []),
    ...section("Resource cards", (page.resources ?? []).map((resource) => resourceLine(resource, frames, agent))),
    ...section("Connections", page.links.map((link) => linkLine(page, code, link, agent))),
    ...section("Notes", notes),
    ...section("Proposed steps, intentions, not code", (page.proposals ?? []).map((proposal) => proposalLine(proposal, health, agent))),
    ...section("Arrows Unfold draws for calls and starts", arrows),
    ...section("Type arrows", typeArrows),
  ];
}

export const readPage = defineTool({
  name: "read_page",
  title: "Read page",
  description:
    'Everything on a page, with ids: cards (name, file:line, key, compact or expanded and the calls each opens in place, frame, position and measured size, description and who wrote it; an expanded card\'s drawn flow, each line ending with its blocks\' keys), names, combined steps, frames as a tree, resource cards (kind, name, type, columns, anchors), connections (kind, ends, the call they leave from, channel, carried type, anchors, label), notes, proposed steps (title, reason, who proposed it, the block they go after or the body they start, and a note when that anchor is missing or held), the arrows Unfold draws for calls (each with the call key read_call takes) and from apps and entrances to their code, and type arrows from a field to a type. It reads the page as the window holding the board has it, unsaved edits included, else as its file has it. page is an id or a link (default: the page in view); a board without a page reads its first page. Long pages come in pages: pass cursor. The result\'s rev is the page\'s revision: pass it as expectRevision to the next edit. Reading changes nothing.',
  input: z.object({
    board: z.string().min(1).optional().describe("A board's id or a link (default: the board in view)"),
    page: z.string().min(1).optional().describe("A page's id or a link (default: the page in view)"),
    project: z.string().min(1).optional().describe("The board's project, when the board's id isn't enough"),
    cursor: cursorArg,
  }),
  annotations: { readOnlyHint: true },
  async run({ board, page, project, cursor }, context) {
    const target = await resolveBoard(context, { board, page, project });
    const read = await loadPage(context, target.board, target.page);
    const [code, health] = await Promise.all([pageCode(context, target.project, read.page), proposalHealth(context, target.board, read.page)]);
    const lines = pageLines(read.page, read, code, health, context.agent);
    const shown = pagedLines(lines, cursor, LINES_PER_PAGE);
    return textResult(
      context.header(pagePlace(target.project, read)),
      `Ids: project ${target.project.id}, board ${target.board.id}, page ${read.page.id}. ${DEFAULT_VIEW[read.page.defaultView]}${read.unsaved ? " The window has edits it hasn't saved yet; they're included." : ""}`,
      lines.length === 0 ? "The page is empty: add_cards puts code on it." : null,
      ...shown.items,
      nextLine(shown, "read_page"),
      shown.next === undefined ? "Next: check reviews the page; pass its rev as expectRevision to an edit that builds on what you read." : null,
    );
  },
});
