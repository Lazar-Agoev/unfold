/**
 * Read boards (design §6.5): `list_boards` and `read_page`.
 *
 * `list_boards` lists a project's boards (`list_boards`, which never wakes a project) with each
 * board's pages from its file (`read_board`), or, with no project to go by, every project's drafts and
 * unpublished changes (`drafts`). `read_page` reads a page as the window holding its board has it
 * (unsaved edits included), else as its file has it, then the code of its cards, so it can say what
 * the board draws between them; or the page as Mermaid (`export_mermaid_text`, as last saved).
 */

import { z } from "zod";
import type { AgentBoardRef, AgentIdentity } from "../../../shared/agent-api";
import type { ToolContext } from "../context";
import type { Anchor, BoardSummary, Frame, Link, Page, ProjectRow, Resource } from "../engine-types";
import { authorText, clip, count, joined, keyFile, keyName, labelText, place, typeText } from "../format";
import { pageCode, type CardCode, type PageCode } from "../page-code";
import { nextLine, paged, textResult } from "../results";
import { currentView, readPage as loadPage, pagePlace, projectOrView, resolveBoard, type ReadPage } from "../target";
import { defineTool } from "../tool";

/** Boards per page of `list_boards`. */
const BOARDS_PER_PAGE = 30;
/** Lines per page of `read_page`. */
const LINES_PER_PAGE = 120;

const VIEWS = { diagram: "Diagram", flow: "Flow", code: "Code" } as const;

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

/** A board's lines in a listing: its title, id, and state, then its pages (with `project`, the project it is in). */
async function boardLines(context: ToolContext, board: BoardSummary, project?: string): Promise<string[]> {
  const ref = { project: board.project, id: board.id };
  const view = currentView(context);
  const held = board.heldBy !== null || context.views.holder(ref) !== undefined;
  const facts = [
    project,
    board.place === "draft" ? "draft" : board.changes > 0 ? `published, ${count(board.changes, "unpublished change")}` : "published",
    board.folder !== null ? `in ${board.folder}` : null,
    count(board.cards, "card"),
    board.label !== null ? `needs review: ${board.label} (check lists it)` : null,
    board.error !== null ? `can't be read: ${board.error}` : null,
    view?.board?.project === ref.project && view.board.id === ref.id ? "in view" : held ? "open in Unfold" : null,
  ].filter((fact) => fact !== null && fact !== undefined);
  return [`- ${board.title ?? board.name} (board id ${board.id}): ${facts.join(", ")}`, `  ${board.error === null ? await pagesText(context, board) : count(board.pages, "page")}`];
}

export const listBoards = defineTool({
  name: "list_boards",
  title: "List boards",
  description:
    'A project\'s boards and drafts: each board\'s title and id, its pages with their ids, publish state (a draft, or published with unpublished changes), folder, card count, health ("needs review: 1 missing · 2 outdated"; check lists the problems and their repairs), and whether it is open in Unfold or in view; then the empty folders. project is a name, id, or link (default: the project of the board in view, else the only project). With several projects and no board in view, it lists every project\'s drafts and published boards with unpublished changes, newest first. Pass a board\'s and a page\'s id to read_page, check, and the tools that build pages. Reading changes nothing.',
  input: z.object({
    project: z.string().min(1).optional().describe("The project's name or id, or a link to one of its boards"),
    cursor: cursorArg,
  }),
  annotations: { readOnlyHint: true },
  async run({ project, cursor }, context) {
    const rows = await context.projects();
    const known = project !== undefined || rows.length === 1 || currentView(context)?.board !== undefined;
    if (!known) {
      return listDrafts(context, rows, cursor);
    }
    const row = await projectOrView(context, project);
    const list = await context.engine("list_boards", { project: row.id });
    const page = paged(list.boards, cursor, BOARDS_PER_PAGE);
    const lines = await Promise.all(page.items.map((board) => boardLines(context, board)));
    return textResult(
      context.header({ project: row.name }),
      `${row.name} (project id ${row.id}) has ${count(list.boards.length, "board")}${list.boards.length > 0 ? ":" : "."}`,
      ...lines.flat(),
      list.emptyFolders.length > 0 && page.next === undefined ? `Empty folders: ${list.emptyFolders.join(", ")}.` : null,
      nextLine(page, "list_boards"),
      list.boards.length > 0 ? "Next: read_page with a board's and a page's id; check with a board's id lists its problems." : "Next: create_board makes a draft.",
    );
  },
});

/** Every project's drafts and changed published boards, newest first. */
async function listDrafts(context: ToolContext, rows: readonly ProjectRow[], cursor: string | undefined) {
  const { drafts } = await context.engine("drafts", {});
  const page = paged(drafts, cursor, BOARDS_PER_PAGE);
  const names = new Map(rows.map((row) => [row.id, row.name]));
  const lines = await Promise.all(page.items.map((board) => boardLines(context, board, `in ${names.get(board.project) ?? board.project}`)));
  return textResult(
    context.header(),
    `${count(rows.length, "project")} and no board in view, so these are every project's drafts and published boards with unpublished changes, newest first (${drafts.length}):`,
    ...lines.flat(),
    nextLine(page, "list_boards"),
    "Next: list_boards with project lists all of one project's boards.",
  );
}

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

/** Unfold's name for a card: `OrderService.place`, `POST /orders`. */
function cardName({ model, key }: CardCode): string {
  if (model?.type === "block") {
    return model.header.qualifiedName;
  }
  return model?.type === "entrance" ? model.header.title : keyName(key);
}

/** Where a card's code is: `function at src/routes/orders.ts:25`, `http entrance`. */
function cardWhere({ model }: CardCode): string | null {
  if (model?.type === "block") {
    return `${model.header.kind} at ${place(model.header.path, model.header.startLine)}`;
  }
  return model?.type === "entrance" ? `${model.header.kind} entrance` : null;
}

/** A card's line: its name, id, code, view, place on the canvas, frame, and description. */
function cardLine(page: Page, read: ReadPage, code: CardCode, frames: ReadonlyMap<string, string>, agent: AgentIdentity): string {
  const { card } = code;
  const size = read.sizes[card.id];
  const frame = frames.get(card.id);
  return `- ${cardName(code)} (card id ${card.id}): ${joined([
    cardWhere(code),
    `key ${code.key}`,
    card.ref.project !== undefined ? `from project ${card.ref.project}` : null,
    card.view !== undefined ? VIEWS[card.view] : `${VIEWS[page.defaultView]} (the page's default)`,
    card.folded === true ? "folded" : null,
    `at [${card.at.join(", ")}]${size ? `, ${Math.round(size.width)}×${Math.round(size.height)}` : ""}`,
    frame !== undefined ? `in frame ${frame}` : null,
    code.problem !== undefined ? `code unavailable: ${code.problem}` : null,
    wordsText("description", card.description, agent),
  ])}`;
}

/** `2 statements, lines 28–30`: a count, and the lines when they are known. */
function joinedLines(statements: string, lines: string | null): string {
  return lines === null ? statements : `${statements}, ${lines}`;
}

/** The lines a step group's statements span, from its card's Code outline. */
function groupLines(code: CardCode | undefined, members: readonly string[]): string | null {
  const source = code?.model?.type === "block" ? code.model.code?.source : undefined;
  if (source?.state !== "current") {
    return null;
  }
  const items = source.outline.filter((item) => members.includes(item.key));
  if (items.length === 0) {
    return null;
  }
  const from = Math.min(...items.map((item) => item.startLine));
  const to = Math.max(...items.map((item) => item.endLine));
  return from === to ? `line ${from}` : `lines ${from}–${to}`;
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

/** An anchor as results cite it: `OrderRepository (key …)`, `db/schema.sql:3–9`. */
function anchorText(anchor: Anchor): string {
  const project = anchor.project !== undefined ? ` in ${anchor.project}` : "";
  return "key" in anchor ? `${keyName(anchor.key)} (key ${anchor.key}${project})` : `${place(anchor.file, anchor.lines[0], anchor.lines[1])}${project}`;
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
    resource.type !== undefined ? `backed by ${keyName(resource.type)} (key ${resource.type}${resource.project !== undefined ? ` in ${resource.project}` : ""})` : null,
    columns.length > 0 ? `${resource.type !== undefined ? "marks" : "columns"} ${columns.join(", ")}` : null,
    resource.anchors.length > 0 ? `anchors ${resource.anchors.map(anchorText).join(", ")}` : "no anchors",
    frame !== undefined ? `in frame ${frame}` : null,
    `by ${authorText(resource.author, agent)}`,
    wordsText("description", resource.description, agent),
  ])}`;
}

/** Where a connection's step is, `src/routes/orders.ts:28`, from its card's models. */
function stepPlace(code: CardCode | undefined, step: string): string {
  const file = keyFile(step) ?? step;
  const model = code?.model?.type === "block" ? code.model : undefined;
  const source = model?.code?.source;
  const line =
    (source?.state === "current" ? (source.outline.find((item) => item.key === step)?.startLine ?? source.links.find((link) => link.source === step)?.line) : undefined) ??
    model?.flow?.nodes.find((node) => node.key === step)?.startLine;
  return place(file, line);
}

/** A connection's or drawn link's line: its kind, ends, id, step, channel, carried type, anchors, and words. */
function linkLine(page: Page, code: PageCode, link: Link, agent: AgentIdentity): string {
  const what = link.kind === undefined ? "link" : `${link.kind} connection`;
  return `- ${what} ${named(page, code, link.from)} → ${named(page, code, link.to)} (link id ${link.id}): ${joined([
    link.step !== undefined ? `from step at ${stepPlace(code.cards.get(link.from), link.step)} (key ${link.step})` : null,
    link.fromColumn !== undefined || link.toColumn !== undefined ? `columns ${link.fromColumn ?? "?"} → ${link.toColumn ?? "?"}` : null,
    link.cardinality,
    link.channel !== undefined ? `channel ${link.channel}` : null,
    link.carries !== undefined ? `carries ${keyName(link.carries)} (key ${link.carries}${link.carriesProject !== undefined ? ` in ${link.carriesProject}` : ""})` : null,
    link.anchors !== undefined && link.anchors.length > 0 ? `anchors ${link.anchors.map(anchorText).join(", ")}` : link.kind !== undefined ? "no anchors" : null,
    link.author !== undefined ? `by ${authorText(link.author, agent)}` : null,
    wordsText("label", link.label, agent),
    wordsText("description", link.description, agent),
  ]) || "no label"}`;
}

/** The page's items, section by section, as lines. */
function pageLines(page: Page, read: ReadPage, code: PageCode, agent: AgentIdentity): string[] {
  const frames = new Map((page.frames ?? []).flatMap((frame) => frame.members.map((member) => [member, frame.id] as const)));
  const section = (title: string, lines: readonly string[]) => (lines.length > 0 ? [`${title} (${lines.length}):`, ...lines] : []);
  const cards = page.cards.flatMap((card) => {
    const found = code.cards.get(card.id);
    return found ? [cardLine(page, read, found, frames, agent)] : [];
  });
  const groups = page.groups.map(
    (group) =>
      `- “${group.title}” (step group id ${group.id}) in card ${group.card}: ${joined([
        joinedLines(count(group.members.length, "statement"), groupLines(code.cards.get(group.card), group.members)),
        group.collapsed ? "collapsed" : null,
        `titled by ${authorText(group.author, agent)}`,
        wordsText("description", group.description, agent),
      ])}`,
  );
  const notes = (page.notes ?? []).map(
    (note) =>
      `- note ${note.id}, ${note.attach !== undefined ? `attached to ${named(page, code, note.attach)} at offset [${note.at.join(", ")}]` : `free at [${note.at.join(", ")}]`}, by ${authorText(note.author, agent)}: “${note.text}”`,
  );
  const arrows = code.arrows.map((arrow) => {
    const call =
      arrow.kind === "handler"
        ? `handler ${arrow.text}`
        : `${arrow.kind} ${arrow.text}${arrow.line !== null ? ` at line ${arrow.line}` : ""}${arrow.label ? `: ${labelText(arrow.label)}` : ""}; source ${arrow.source ?? ""}, seq ${arrow.seq ?? 0}`;
    return `- ${named(page, code, arrow.from)} → ${named(page, code, arrow.to)}: ${call}`;
  });
  const typeArrows = code.typeArrows.map((arrow) => `- ${named(page, code, arrow.from)} → ${named(page, code, arrow.to)}: field ${arrow.field}: ${typeText(arrow.type)}`);
  const unlabeled = code.unlabeled.map(
    (step) => `- ${named(page, code, step.card)}: ${step.line !== undefined ? `line ${step.line} ` : ""}${step.kind} “${step.name}” (key ${step.key})`,
  );
  return [
    ...section("Cards", cards),
    ...section("Step groups", groups),
    ...((page.frames ?? []).length > 0 ? [`Frames (${(page.frames ?? []).length}), nested as they are:`, ...frameLines(page, code, agent)] : []),
    ...section("Resource cards", (page.resources ?? []).map((resource) => resourceLine(resource, frames, agent))),
    ...section("Connections and drawn links", page.links.map((link) => linkLine(page, code, link, agent))),
    ...section("Notes", notes),
    ...section("Arrows Unfold draws for calls and handlers", arrows),
    ...section("Type arrows", typeArrows),
    ...section("Flow steps still unlabeled, in cards shown in Flow (label_steps labels them)", unlabeled),
  ];
}

/** The page as Mermaid, as last saved (the window that holds the board exports it). */
async function mermaid(context: ToolContext, project: ProjectRow, read: ReadPage, board: AgentBoardRef) {
  const opened = context.views.holder(board) === undefined;
  const { text } = await context.held(board, "export_mermaid_text", { project: board.project, board: { id: board.id, page: read.page.id } });
  return textResult(
    context.header(pagePlace(project, read)),
    `The page as Mermaid, as last saved${read.unsaved ? " (the window's unsaved edits aren't in it)" : ""}${opened ? "; Unfold opened the board in a background tab to export it" : ""}:`,
    "```mermaid",
    text.trimEnd(),
    "```",
  );
}

export const readPage = defineTool({
  name: "read_page",
  title: "Read page",
  description:
    'Everything on a page, with ids: cards (name, file:line, key, view, folded, frame, position and measured size, description and who wrote it), step groups, frames as a tree, resource cards (kind, name, type, columns, anchors), connections and drawn links (kind, ends, the step they leave from at file:line, channel, carried type, anchors, label), notes, the arrows Unfold draws for calls (each with its typed label, and the source and seq read_call takes) and handlers, type arrows from a field to a type, and the Flow steps still unlabeled (with the keys label_steps takes). It reads the page as the window holding the board has it, unsaved edits included, else as its file has it. format "mermaid" gives the page as a Mermaid flowchart instead, as last saved. page is an id or a link (default: the page in view); a board without a page reads its first page. Long pages come in pages: pass cursor. The result\'s rev is the page\'s revision: pass it as expectRevision to the next edit. Reading changes nothing.',
  input: z.object({
    board: z.string().min(1).optional().describe("A board's id or a link (default: the board in view)"),
    page: z.string().min(1).optional().describe("A page's id or a link (default: the page in view)"),
    project: z.string().min(1).optional().describe("The board's project, when the board's id isn't enough"),
    format: z.enum(["items", "mermaid"]).default("items"),
    cursor: cursorArg,
  }),
  annotations: { readOnlyHint: true },
  async run({ board, page, project, format, cursor }, context) {
    const target = await resolveBoard(context, { board, page, project });
    const read = await loadPage(context, target.board, target.page);
    if (format === "mermaid") {
      return mermaid(context, target.project, read, target.board);
    }
    const code = await pageCode(context, target.project, read.page);
    const lines = pageLines(read.page, read, code, context.agent);
    const shown = paged(lines, cursor, LINES_PER_PAGE);
    return textResult(
      context.header(pagePlace(target.project, read)),
      `Ids: project ${target.project.id}, board ${target.board.id}, page ${read.page.id}. Cards without their own view show ${VIEWS[read.page.defaultView]}.${read.unsaved ? " The window has edits it hasn't saved yet; they're included." : ""}`,
      lines.length === 0 ? "The page is empty: add_cards puts code on it." : null,
      ...shown.items,
      nextLine(shown, "read_page"),
      shown.next === undefined ? "Next: check reviews the page; pass its rev as expectRevision to an edit that builds on what you read." : null,
    );
  },
});
