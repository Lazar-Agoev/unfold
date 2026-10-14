/**
 * Tracing an app or entrance from How it starts (entrance-definitions.md, "Home: How it starts"): a new
 * board named after it, showing its card and the cards for its code, expanded, in a column to its right
 * in the order they run. Its arrows point at them. Code that names no block gets no card: the start card
 * lists it, dashed and labeled with its state. A failure is a toast; it never throws.
 */

import { startCodePlacement } from "../../canvas/layout";
import { measureText } from "../../canvas/measure";
import { estimatedCardSize } from "../../canvas/sizing";
import { showToast } from "../../components";
import { queryClient } from "../../ipc/client";
import type { CardRef, Key, ProjectId } from "../../ipc/generated";
import { toUnfoldError } from "../../ipc/unfold";
import { addCard, editPage, findPage, getOpenBoard, newBoard, requestCardFocus } from "../../state/boards";
import { cardArgs, cardName, cardQuery } from "../../state/cards";
import { getShowAllCalls } from "../../state/showAllCalls";

/** What the reader draws for `ref` on a page with no overlay, through the cache the board then draws from. */
function drawn(project: ProjectId, ref: CardRef, showAllCalls: boolean) {
  return queryClient.query(cardQuery(cardArgs(project, { ref }, {}, showAllCalls)));
}

/** Traces the app or entrance `key`, called `name`, onto a new board, and shows it. */
export async function traceStart(project: ProjectId, key: Key, name: string): Promise<void> {
  try {
    const showAllCalls = getShowAllCalls();
    const start = await drawn(project, { entrance: key }, showAllCalls);
    if (start.type !== "start") {
      return;
    }
    // The blocks it starts at, each once, with what the reader draws for them (they size the cards).
    const keys = [...new Set(start.code.flatMap((code) => (code.state === "found" || code.state === "stale" ? [code.key] : [])))];
    const models = await Promise.all(keys.map((code) => drawn(project, { block: code }, showAllCalls)));
    const ref = await newBoard(project, { title: name });
    const board = getOpenBoard(ref);
    const page = board ? findPage(board.document, board.activePage) : undefined;
    if (!board || !page) {
      return;
    }
    const target = { board: ref, page: page.id };
    const startSize = estimatedCardSize({ model: start, view: page.defaultView }, measureText);
    const sizes = models.map((model) => estimatedCardSize({ model, view: "expanded" }, measureText));
    const positions = startCodePlacement({ x: 0, y: 0, ...startSize }, sizes);
    const ids = editPage(target, (draft) => [
      addCard(draft, { ref: { entrance: key }, name, at: [0, 0] }),
      ...keys.map((code, index) => {
        const model = models[index];
        return addCard(draft, { ref: { block: code }, name: model ? cardName(model) : code, at: positions[index] ?? [0, 0], view: "expanded" });
      }),
    ]);
    if (ids) {
      requestCardFocus(target, ids, "fit");
    }
  } catch (error) {
    showToast({ title: `Couldn't trace ${name}`, body: toUnfoldError(error).message, tone: "danger" });
  }
}
