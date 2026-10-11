/**
 * Show callers (V1's References, now on the card menu): asks the engine for the distinct definitions
 * that call a card's block (`callers`), and adds the ones not on the page yet as cards in a column to
 * its left, sized from their models so each one's right edge lines up. One undo step; Back returns.
 */

import { queryClient } from "../ipc/client";
import type { ProjectId } from "../ipc/generated";
import { toUnfoldError } from "../ipc/unfold";
import { showToast } from "../components";
import {
  addCard,
  displayedRects,
  findCard,
  findCardByRef,
  findPage,
  getOpenBoard,
  getPageView,
  editPage,
  keyDisplayName,
  pushFocusMove,
  requestCardFocus,
  type PageTarget,
} from "../state/boards";
import { call } from "../state/boards/engine";
import { getProjectIndex, resolveRefProject } from "../state/workspace";
import { cardQuery } from "./cardData";
import { callerPlacement } from "./layout";
import { measureText } from "./measure";
import { estimatedCardSize } from "./sizing";

/** How many callers one Show callers adds at most (the engine's default page). */
const CALLERS_LIMIT = 20;
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
  // New cards show the page's lens, else its default view.
  const shown = getPageView(target).lens ?? page.defaultView;
  const size = (key: string, index: number) => estimatedCardSize({ key, model: models[index], view: shown, monogram: refProject !== undefined }, measureText);
  const missing = keys.flatMap((key, index) => (findCardByRef(page, key, refProject) ? [] : [{ key, size: size(key, index) }]));
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
