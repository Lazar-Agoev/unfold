/**
 * What the code panel opens for what is selected on a page (docs/reader.md#the-code-panels-model),
 * and what a click on a line selects. The reader decides which lines a target shows; this
 * only names the target, from the page and what its cards draw, never from keys.
 */

import type { Anchor, CardView, CodeTarget, DrawnFlow, GapTick, Key, Page, RepoPath, Sequence, Step, ThinRow } from "../ipc/generated";
import { cardKey, findCard, findResource, type Selection } from "../state/boards";
import { eachSequence, findBlock } from "../state/cards";
import type { CodeSource, GapPlace } from "../state/codePanel";

/** A target for the panel, the card the selection is in, and for lines a card draws, what draws them. */
export interface CodeChoice {
  readonly target: CodeTarget;
  readonly cardId: string | null;
  readonly from?: CodeSource;
}

/** The first of `items` for which `find` finds something. */
function firstOf<T, R>(items: Iterable<T>, find: (item: T) => R | undefined): R | undefined {
  for (const item of items) {
    const found = find(item);
    if (found !== undefined) {
      return found;
    }
  }
  return undefined;
}

/** What `find` finds first among the steps of a drawn flow's sequences, given the file of the flow drawing each and the body it is. */
function findStep<R>(flow: DrawnFlow, find: (sequence: Sequence, path: RepoPath, body: string) => R | undefined): R | undefined {
  let found: R | undefined;
  eachSequence(flow, (sequence, body, path) => {
    found ??= find(sequence, path, body);
  });
  return found;
}

/** What a step is known by in its flow: its block's key, a row's first call's, or a combined or proposed step's id. */
function stepId(step: Step): string | undefined {
  switch (step.kind) {
    case "call":
    case "decision":
    case "loop":
    case "try":
    case "with":
    case "exit":
      return step.key;
    case "thin":
      return step.calls[0]?.key;
    case "combined":
    case "proposed":
      return step.item;
  }
}

/** Combined step `item` drawn in `flow`: its lines, titled. */
function combinedIn(flow: DrawnFlow, item: string): CodeTarget | undefined {
  return findStep(flow, (sequence, path) =>
    firstOf(sequence.steps, (step) => (step.kind === "combined" && step.item === item ? { kind: "lines" as const, path, lines: [...step.lines], title: step.title } : undefined)),
  );
}

/** The row of other calls drawn in `flow` whose first call is `key`. */
function rowIn(flow: DrawnFlow, key: Key): CodeTarget | undefined {
  return findStep(flow, (sequence, path) => firstOf(sequence.steps, (step) => (step.kind === "thin" && step.calls[0]?.key === key ? rowTarget(path, step) : undefined)));
}

/** The gaps of a sequence: before each step that has one, then at its end. */
function gapsOf(sequence: Sequence, body: string): { readonly place: GapPlace; readonly gap: GapTick }[] {
  const before = sequence.steps.flatMap((step) => {
    const id = stepId(step);
    return step.gap && id !== undefined ? [{ place: { before: id }, gap: step.gap }] : [];
  });
  return sequence.endGap ? [...before, { place: { end: body }, gap: sequence.endGap }] : before;
}

function samePlace(a: GapPlace, b: GapPlace): boolean {
  return "before" in a ? "before" in b && a.before === b.before : "end" in b && a.end === b.end;
}

/** The lines of the gap tick at `place` in `flow`. */
function gapIn(flow: DrawnFlow, place: GapPlace): CodeTarget | undefined {
  return findStep(flow, (sequence, path, body) =>
    firstOf(gapsOf(sequence, body), (found) => (samePlace(found.place, place) ? { kind: "lines" as const, path, lines: [...found.gap.lines] } : undefined)),
  );
}

/** Where the gap tick counting `lines` of `path` sits in `flow`. */
export function gapPlace(flow: DrawnFlow, path: RepoPath, lines: readonly [number, number]): GapPlace | undefined {
  return findStep(flow, (sequence, file, body) =>
    file === path ? firstOf(gapsOf(sequence, body), (found) => (found.gap.lines[0] === lines[0] && found.gap.lines[1] === lines[1] ? found.place : undefined)) : undefined,
  );
}

/** What `from` draws in `view` now, as the panel opens it; null when the card no longer draws it. */
export function sourceLines(from: CodeSource, view: CardView): CodeTarget | null {
  if (view.type !== "definition") {
    return null;
  }
  switch (from.kind) {
    case "combined":
      return combinedIn(view.flow, from.item) ?? null;
    case "row":
      return rowIn(view.flow, from.key) ?? null;
    case "gap":
      return gapIn(view.flow, from.place) ?? null;
  }
}

/** The first card of `views` that draws combined step `item`, and its lines. */
function combinedLines(views: ReadonlyMap<string, CardView>, item: string): CodeChoice | undefined {
  return firstOf(views, ([cardId, view]) => {
    const target = view.type === "definition" ? combinedIn(view.flow, item) : undefined;
    return target ? { target, cardId, from: { kind: "combined" as const, item } } : undefined;
  });
}

/**
 * What a citation names: a block; an app or entrance (its lines in `unfold.toml`), which its guard on
 * the page records as an `entrance`; or lines of an evidence file (marked).
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
 * resource card or a connection cites first, in the reader's order: a table's type or a connection's
 * step, then its first anchor. Null for what has no source: nothing, a frame, a note, a proposed step,
 * a resource card or connection that cites nothing, or an arrow that stands for several calls.
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
    case "combined":
      return combinedLines(views, selection.combinedId) ?? null;
    case "line":
      return { target: { kind: "block", key: selection.call }, cardId: selection.from };
    case "resource": {
      const resource = findResource(page, selection.resourceId);
      const target = resource?.type !== undefined ? { kind: "block" as const, key: resource.type } : anchorTarget(page, resource?.anchors[0]);
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

/** The collapsed combined step of `views` that holds block `key`. */
function holder(views: ReadonlyMap<string, CardView>, key: Key): string | undefined {
  return firstOf(views.values(), (view) =>
    view.type === "definition"
      ? findStep(view.flow, (sequence) => firstOf(sequence.steps, (step) => (step.kind === "combined" && step.holds?.includes(key) ? step.item : undefined)))
      : undefined,
  );
}

/**
 * What a click on a line selects for the first of `keys` (a call on it, then its own block) that
 * something on the page draws: the block in the card the panel's selection is in (`preferred`) or
 * else the first card that draws it (the card itself for its own key), or the collapsed combined step
 * that holds it. Null when nothing on the page draws any of them.
 */
export function blockSelection(page: Page, views: ReadonlyMap<string, CardView>, keys: readonly Key[], preferred: string | null): Selection | null {
  const first = preferred !== null && views.get(preferred) ? [preferred] : [];
  return (
    firstOf(keys, (key): Selection | undefined => {
      for (const cardId of [...first, ...views.keys()]) {
        const view = views.get(cardId);
        if (view && draws(view, key)) {
          return cardSelection(page, cardId, key) ?? undefined;
        }
      }
      const combined = holder(views, key);
      return combined !== undefined ? { kind: "combined", combinedId: combined } : undefined;
    }) ?? null
  );
}
