/**
 * What the code panel opens for what is selected on a page (docs/designs/viewing-code.md, "What each
 * block shows"), and what a click on a line selects. The reader decides which lines a target shows; this
 * only names the target, from the page and what its cards draw, never from keys.
 */

import type { Anchor, CardView, CodeTarget, DrawnFlow, Key, Page, RepoPath, Sequence, ThinRow } from "../ipc/generated";
import { cardKey, findCard, findResource, type Selection } from "../state/boards";
import { findBlock } from "../state/cards";

/** A target for the panel, and the card the selection is in. */
export interface CodeChoice {
  readonly target: CodeTarget;
  readonly cardId: string | null;
}

/** A combined step's lines in its file. */
interface CombinedLines {
  readonly path: RepoPath;
  readonly lines: [number, number];
  readonly title: string;
}

/** The first of `items` for which `find` finds something. */
function firstOf<T, R>(items: readonly T[], find: (item: T) => R | undefined): R | undefined {
  for (const item of items) {
    const found = find(item);
    if (found !== undefined) {
      return found;
    }
  }
  return undefined;
}

/** Combined step `item` among a sequence's steps (in `path`), or in what they hold or open in place. */
function inSequence(sequence: Sequence, path: RepoPath, item: string): CombinedLines | undefined {
  return firstOf(sequence.steps, (step) => {
    switch (step.kind) {
      case "combined":
        return step.item === item ? { path, lines: [...step.lines], title: step.title } : step.steps ? inSequence(step.steps, path, item) : undefined;
      case "call":
        return firstOf([step.inset, ...step.callbacks.map((callback) => callback.inset)], (inset) => (inset ? inFlow(inset, item) : undefined));
      case "decision":
      case "loop":
      case "try":
      case "with":
        return firstOf(step.lanes, (lane) => inSequence(lane.steps, path, item));
      case "thin":
      case "exit":
      case "proposed":
        return undefined;
    }
  });
}

function inFlow(flow: DrawnFlow, item: string): CombinedLines | undefined {
  return inSequence(flow.steps, flow.header.path, item);
}

/** The first card of `views` that draws combined step `item`, and its lines. */
function combinedLines(views: ReadonlyMap<string, CardView>, item: string): { readonly cardId: string; readonly lines: CombinedLines } | undefined {
  for (const [cardId, view] of views) {
    const lines = view.type === "definition" ? inFlow(view.flow, item) : undefined;
    if (lines) {
      return { cardId, lines };
    }
  }
  return undefined;
}

/**
 * What a resource card or a connection cites first: a block; an app or entrance (its lines in
 * `unfold.toml`), which its guard on the page records as an `entrance`; or lines of an evidence file
 * (marked).
 */
function anchorTarget(page: Page, anchor: Anchor | undefined): CodeTarget | null {
  if (!anchor) {
    return null;
  }
  if ("file" in anchor) {
    return { kind: "lines", path: anchor.file, lines: [...anchor.lines], mark: [...anchor.lines] };
  }
  const entrance = page.guards.some((guard) => guard.key === anchor.key && guard.kind === "entrance");
  return { kind: entrance ? "start" : "block", key: anchor.key };
}

/** A row of other calls: from its first call's statement to its last one's, in the file `path` of the flow drawing it. */
export function rowTarget(path: RepoPath, row: ThinRow): CodeTarget {
  return { kind: "lines", path, lines: [...row.lines] };
}

/**
 * What the panel opens for a selection: a card's block (an app's or entrance's lines in `unfold.toml`),
 * a block drawn in a card, a field of a type card, a combined step's lines, an arrow's call, or what a
 * resource card or a connection cites first. Null for what has no source: nothing, a frame, a note, a
 * proposed step, or an arrow that stands for several calls.
 */
export function selectionCode(selection: Selection, page: Page, views: ReadonlyMap<string, CardView>): CodeChoice | null {
  switch (selection.kind) {
    case "card": {
      const card = findCard(page, selection.cardId);
      if (!card) {
        return null;
      }
      if (selection.key !== null) {
        return { target: { kind: "block", key: selection.key }, cardId: card.id };
      }
      return { target: "entrance" in card.ref ? { kind: "start", key: card.ref.entrance } : { kind: "block", key: card.ref.block }, cardId: card.id };
    }
    case "field":
      return findCard(page, selection.cardId) ? { target: { kind: "field", key: selection.key, index: selection.index }, cardId: selection.cardId } : null;
    case "combined": {
      const found = combinedLines(views, selection.combinedId);
      return found ? { target: { kind: "lines", ...found.lines }, cardId: found.cardId } : null;
    }
    case "line":
      return { target: { kind: "block", key: selection.call }, cardId: selection.from };
    case "resource": {
      const target = anchorTarget(page, findResource(page, selection.resourceId)?.anchors[0]);
      return target ? { target, cardId: null } : null;
    }
    case "link": {
      const link = page.links.find((item) => item.id === selection.linkId);
      const target = link?.step !== undefined ? { kind: "block" as const, key: link.step } : anchorTarget(page, link?.anchors?.[0]);
      return target ? { target, cardId: null } : null;
    }
    case "none":
    case "frame":
    case "note":
    case "proposal":
    case "calls":
      return null;
  }
}

/** Whether a card draws block `key`: its own key, or a block of its drawn flow. */
function draws(view: CardView, key: Key): boolean {
  return view.type === "definition" ? findBlock(view.flow, key) !== undefined : view.key === key;
}

function cardSelection(page: Page, cardId: string, key: Key): Selection | null {
  const card = findCard(page, cardId);
  return card ? { kind: "card", cardId, key: cardKey(card) === key ? null : key } : null;
}

/**
 * What a click on a line selects for block `key`: the block in the card the panel's selection is in
 * (`preferred`) or else the first card that draws it (the card itself for its own key), or the combined
 * step that holds it. Null when nothing on the page draws it.
 */
export function blockSelection(page: Page, views: ReadonlyMap<string, CardView>, key: Key, preferred: string | null): Selection | null {
  const first = preferred !== null && views.get(preferred) ? [preferred] : [];
  for (const cardId of [...first, ...views.keys()]) {
    const view = views.get(cardId);
    if (view && draws(view, key)) {
      return cardSelection(page, cardId, key);
    }
  }
  const combined = page.combined.find((step) => step.members.includes(key));
  return combined ? { kind: "combined", combinedId: combined.id } : null;
}
