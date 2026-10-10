/**
 * Show callers (redesign §6, References → card menu): asks the engine for the distinct definitions
 * that call a card's block (`callers`), and adds the ones not on the page yet as cards in a column to
 * its left, sized from their models so each one's right edge lines up. One undo step; Back returns.
 */

import { queryClient } from "../ipc/client";
import type { CardView, ProjectId } from "../ipc/generated";
import { toUnfoldError } from "../ipc/unfold";
import { showToast } from "../components";
import { shortPath } from "../code/text";
import {
  addCard,
  displayedRects,
  findCard,
  findCardByRef,
  findPage,
  getOpenBoard,
  editPage,
  keyDisplayName,
  pushFocusMove,
  requestCardFocus,
  type PageTarget,
} from "../state/boards";
import { call } from "../state/boards/engine";
import { getProjectIndex, resolveRefProject } from "../state/workspace";
import { cardQuery } from "./cardData";
import { callerPlacement, fallbackSize, type Size } from "./layout";
import { measureText } from "./measure";
import { blockCardWidth, codeContentWidth } from "./sizing";

/** How many callers one Show callers adds at most (the engine's default page). */
const CALLERS_LIMIT = 20;
/** A block card's header with its mini lens, its code's top and bottom padding, and a code row (cards.css, code.css). */
const HEADER_HEIGHT = 44;
const CODE_PADDING = 18;
const CODE_ROW = 20;

/** A caller card's size before it is drawn: its width from its model, its height from its lines. */
function estimatedSize(view: CardView | undefined, foreign: boolean): Size {
  const code = view?.type === "block" && view.code?.source.state === "current" ? view.code.source : null;
  if (view?.type !== "block" || !code) {
    return fallbackSize(false);
  }
  const width = blockCardWidth(
    { name: view.header.qualifiedName, location: shortPath(view.header.path), monogram: foreign, body: codeContentWidth(code, measureText) },
    measureText,
  );
  // The header, the code's padding, and one row per line (long lines wrap, so this is a floor).
  return { width, height: HEADER_HEIGHT + CODE_PADDING + code.lines.length * CODE_ROW };
}

/** Adds the callers of a block card to its left (or brings those already on the page into view). */
export async function showCallers(target: PageTarget, cardId: string): Promise<void> {
  const start = getOpenBoard(target.board);
  const startPage = start ? findPage(start.document, target.page) : undefined;
  const card = startPage ? findCard(startPage, cardId) : undefined;
  if (!card || !("block" in card.ref)) {
    return;
  }
  const project: ProjectId | null = resolveRefProject(getProjectIndex(), target.board.project, card.ref.project);
  if (project === null) {
    return;
  }
  const name = keyDisplayName(card.ref.block);
  let total: number;
  let keys: string[];
  try {
    const result = await call("callers", { project, key: card.ref.block, limit: CALLERS_LIMIT });
    total = result.total;
    keys = result.callers.map((caller) => caller.key);
  } catch (error) {
    showToast({ title: "Couldn't find the callers", body: toUnfoldError(error).message, tone: "danger" });
    return;
  }
  if (total === 0) {
    showToast({ title: `Nothing in the graph calls ${name}.` });
    return;
  }
  // Their models size the new cards, and the cards draw from the same cache at once.
  const models = await Promise.all(keys.map((key) => queryClient.query(cardQuery(project, key)).catch(() => undefined)));

  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const rect = board ? displayedRects(board, target.page).get(cardId) : undefined;
  if (!board || !page || !rect) {
    return;
  }
  const refProject = card.ref.project;
  const present = keys.flatMap((key) => findCardByRef(page, key, refProject)?.id ?? []);
  const missing = keys.flatMap((key, index) => (findCardByRef(page, key, refProject) ? [] : [{ key, size: estimatedSize(models[index], refProject !== undefined) }]));
  const positions = callerPlacement(
    rect,
    missing.map((item) => item.size),
  );
  const added =
    missing.length > 0
      ? (editPage(target, (draft) =>
          missing.map(({ key }, index) =>
            addCard(draft, { ref: refProject !== undefined ? { block: key, project: refProject } : { block: key }, at: positions[index] ?? [rect.x, rect.y] }),
          ),
        ) ?? [])
      : [];
  pushFocusMove(target);
  requestCardFocus(target, [cardId, ...present, ...added], "fit");
  if (total > keys.length) {
    showToast({ title: `Showing the first ${keys.length} of ${total} callers of ${name}.` });
  }
}
