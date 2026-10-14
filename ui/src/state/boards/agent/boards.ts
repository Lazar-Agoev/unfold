/**
 * An agent's requests about a board as a whole: creating a board, opened without
 * showing it, changing an open board's pages and title as one undo step, and showing a board to the
 * person.
 *
 * A new board is created here, not in the main process, so its session lives here. Its pages are
 * written with it, before its session opens, so creating a board is no undo step. Page and title
 * changes use the same pure edits as the page menus, saved at once (`saveStep`); deleting a page that
 * holds a person's words is refused unless the call says the person asked for it.
 */

import type {
  AgentBoardEditRequest,
  AgentBoardEditResult,
  AgentBoardOp,
  AgentBoardOpResult,
  AgentCreateRequest,
  AgentCreateResult,
  AgentPageSummary,
  AgentShowRequest,
  AgentShowResult,
} from "@unfold/desktop/agent-api";
import { produce, type Draft } from "immer";
import type { BoardDocument, BoardState, Page } from "../../../ipc/generated";
import { toUnfoldError } from "../../../ipc/unfold";
import { showBoard } from "../../shownBoard";
import { cardsDrawing } from "../../cards";
import { addPage, deletePage, findCard, findCombined, findPage, renamePage, setTitle } from "../document";
import { call } from "../engine";
import { adoptBoardState, getOpenBoard, requestCardFocus, saveStep, select, setActivePage } from "../store";
import type { BoardRef, PageTarget, Selection } from "../types";
import { AgentRefusal, boardInfo, failure, guardPeopleText, invalid, itemWords, type Words } from "./context";

function pagesOf(document: BoardDocument): AgentPageSummary[] {
  return document.pages.map((page) => ({ id: page.id, title: page.title }));
}

/** What `save_board` takes: the title and pages. */
function edited(document: BoardDocument) {
  return { title: document.title, pages: document.pages };
}

/** A new board with its named pages, saved before its session opens. */
async function newBoardFile(request: AgentCreateRequest): Promise<BoardRef> {
  const { project } = request;
  const [first, ...more] = request.pages ?? [];
  const state = await call("new_board", {
    project,
    ...(request.title !== undefined ? { title: request.title } : {}),
    ...(request.folder !== undefined ? { folder: request.folder } : {}),
    ...(first !== undefined ? { pageTitle: first } : {}),
  });
  let saved = state;
  if (more.length > 0) {
    const document = produce(state.document, (draft) => {
      for (const title of more) {
        addPage(draft, { title });
      }
    });
    saved = await savePages(state, document);
  }
  return adoptBoardState(saved);
}

/** Saves a new board's other pages; when that fails, the board is let go with its first page. */
async function savePages(state: BoardState, document: BoardDocument): Promise<BoardState> {
  const { project } = state;
  try {
    return await call("save_board", { project, id: state.id, base: state.revision, board: edited(document) });
  } catch (error) {
    // No session holds the half-made board: let it go, and say what happened.
    await call("close_board", { project, id: state.id }).catch(() => undefined);
    const { message } = toUnfoldError(error);
    throw new AgentRefusal("board_problem", `The board “${state.document.title}” was created with one page, but its other pages couldn't be saved: ${message}`);
  }
}

/** Creates a board for an agent and opens it without showing it. */
export async function createBoardForAgent(request: AgentCreateRequest): Promise<AgentCreateResult> {
  try {
    const ref = await newBoardFile(request);
    const board = getOpenBoard(ref);
    if (!board) {
      return failure("board_not_open", "The new board closed before it could be read.");
    }
    return { ok: true, board: { project: ref.project, id: ref.id }, info: boardInfo(board), folder: board.document.folder ?? null, pages: pagesOf(board.document) };
  } catch (error) {
    if (error instanceof AgentRefusal) {
      return failure(error.code, error.message);
    }
    const { code, message } = toUnfoldError(error);
    return failure(code, message);
  }
}

// ---------------------------------------------------------------------------------------------
// Pages and title

function pageOf(document: Draft<BoardDocument>, id: string): Draft<Page> {
  const page = document.pages.find((item) => item.id === id);
  if (!page) {
    throw new AgentRefusal("page_not_found", `The board has no page with id “${id}”. Its pages: ${document.pages.map((item) => `${item.title} (id ${item.id})`).join(", ")}.`);
  }
  return page;
}

function titleOf(text: string, what: string): string {
  const title = text.trim();
  if (title.length === 0) {
    throw invalid(`${what} can't be blank.`);
  }
  return title;
}

/** Every text on a page and who wrote it (`itemWords`, and the names), named as a person would find it. */
function pageWords(page: Page): Words[] {
  const items = [page.cards, page.combined, page.links, page.frames ?? [], page.resources ?? [], page.notes ?? []].flatMap((list) => list.map((item) => item.id));
  return [
    ...page.names.map((name): Words => [`the name “${name.text}”`, name.author]),
    ...items.flatMap((id) => itemWords(page, id, (card) => `card “${card.id}”`)),
  ];
}

/** Applies one operation to the document being edited and says what it did. */
function applyBoardOp(document: Draft<BoardDocument>, op: AgentBoardOp, replacePeopleText: boolean | undefined): AgentBoardOpResult {
  switch (op.op) {
    case "add_page": {
      if (op.after !== undefined) {
        pageOf(document, op.after);
      }
      const title = titleOf(op.title, "A page's title");
      const id = addPage(document, { title, ...(op.after !== undefined ? { after: op.after } : {}) });
      return { op: op.op, status: "created", page: id, title };
    }
    case "rename_page": {
      const page = pageOf(document, op.page);
      const title = titleOf(op.title, "A page's title");
      if (page.title === title) {
        return { op: op.op, status: "kept", page: page.id, title };
      }
      renamePage(document, page.id, title);
      return { op: op.op, status: "updated", page: page.id, title };
    }
    case "move_page": {
      const page = pageOf(document, op.page);
      const from = document.pages.indexOf(page);
      if (!Number.isInteger(op.to) || op.to < 0 || op.to >= document.pages.length) {
        throw invalid(`A page moves to a position from 0 (first) to ${document.pages.length - 1} (last); ${op.to} isn't one.`);
      }
      if (from !== op.to) {
        document.pages.splice(from, 1);
        document.pages.splice(op.to, 0, page);
      }
      return { op: op.op, status: from === op.to ? "kept" : "updated", page: page.id, title: page.title };
    }
    case "delete_page": {
      const page = pageOf(document, op.page);
      if (document.pages.length === 1) {
        throw invalid(`“${page.title}” is the board's only page, and a board keeps at least one. Add another page first, or rename this one.`);
      }
      for (const [what, author] of pageWords(page)) {
        guardPeopleText(`${what} on the page “${page.title}”`, author, replacePeopleText);
      }
      const removed = { op: op.op, status: "removed", page: page.id, title: page.title } as const;
      deletePage(document, page.id);
      return removed;
    }
    case "set_title": {
      const title = titleOf(op.title, "A board's title");
      if (document.title === title) {
        return { op: op.op, status: "kept", title };
      }
      setTitle(document, title);
      return { op: op.op, status: "updated", title };
    }
  }
}

/** Changes a held board's pages and title as one undo step, saved at once. */
export async function editBoardForAgent(request: AgentBoardEditRequest): Promise<AgentBoardEditResult> {
  const ref: BoardRef = { project: request.board.project, id: request.board.id };
  const board = getOpenBoard(ref);
  if (!board) {
    return failure("board_not_open", "This window doesn't hold the board.");
  }
  try {
    const outcome = await saveStep(ref, board.activePage, `${request.agent.name} · ${request.title}`, (document) => {
      let results: AgentBoardOpResult[] = [];
      const next = produce(document, (draft) => {
        results = request.ops.map((op) => applyBoardOp(draft, op, request.replacePeopleText));
      });
      return { document: next, value: results };
    });
    if (!outcome.ok) {
      return failure(outcome.error.code, outcome.error.message);
    }
    const now = getOpenBoard(ref);
    if (now && !findPage(now.document, now.activePage)) {
      // The page the window showed was deleted: show the first page.
      const first = now.document.pages[0];
      if (first) {
        setActivePage(ref, first.id);
      }
    }
    return { ok: true, board: boardInfo(outcome.state), pages: pagesOf(outcome.state.document), results: outcome.value };
  } catch (error) {
    if (error instanceof AgentRefusal) {
      return failure(error.code, error.message);
    }
    const { code, message } = toUnfoldError(error);
    return failure(code, message);
  }
}

// ---------------------------------------------------------------------------------------------
// Showing

/** What showing an item selects, and the items the camera brings into view. */
function itemFocus(target: PageTarget, page: Page, id: string): { readonly selection: Selection; readonly cards: string[] } | null {
  const isCard = (cardId: string) => findCard(page, cardId) !== undefined;
  if (isCard(id)) {
    return { selection: { kind: "card", cardId: id, key: null }, cards: [id] };
  }
  if (findCombined(page, id)) {
    return { selection: { kind: "combined", combinedId: id }, cards: cardsDrawing(target, id) };
  }
  const link = page.links.find((item) => item.id === id);
  if (link) {
    return { selection: { kind: "link", linkId: id }, cards: [link.from, link.to] };
  }
  const frames = page.frames ?? [];
  const members = (frameId: string): string[] =>
    (frames.find((frame) => frame.id === frameId)?.members ?? []).flatMap((member) => (isCard(member) ? [member] : members(member)));
  if (frames.some((frame) => frame.id === id)) {
    return { selection: { kind: "frame", frameId: id }, cards: members(id) };
  }
  if ((page.resources ?? []).some((resource) => resource.id === id)) {
    return { selection: { kind: "resource", resourceId: id }, cards: [id] };
  }
  if ((page.notes ?? []).some((item) => item.id === id)) {
    return { selection: { kind: "note", noteId: id }, cards: [id] };
  }
  return null;
}

/**
 * Shows an open board to the person: the board, the page, and the item selected and centered. The main
 * process brings the window to the front.
 */
export function showForAgent(request: AgentShowRequest): AgentShowResult {
  const ref: BoardRef = { project: request.board.project, id: request.board.id };
  const board = getOpenBoard(ref);
  if (!board) {
    return failure("board_not_open", "This window doesn't hold the board.");
  }
  const page = findPage(board.document, request.page ?? board.activePage);
  if (!page) {
    return failure("page_not_found", `The board has no page with id “${request.page ?? ""}”. Its pages: ${board.document.pages.map((item) => `${item.title} (id ${item.id})`).join(", ")}.`);
  }
  const target: PageTarget = { board: ref, page: page.id };
  const focus = request.item === undefined ? null : itemFocus(target, page, request.item);
  if (request.item !== undefined && !focus) {
    return failure("invalid_op", `There is no item with id “${request.item}” on the page “${page.title}”. read_page lists the page's ids.`);
  }
  showBoard(ref);
  setActivePage(ref, page.id);
  if (focus) {
    select(target, focus.selection);
    if (focus.cards.length > 0) {
      requestCardFocus(target, focus.cards, focus.cards.length === 1 ? "center" : "fit");
    }
  }
  return { ok: true, page: page.id };
}
