/**
 * Cards from Rust (`card`), cached with TanStack Query under the engine key. A card is asked for with
 * its key, the page's overlay (its names, combined steps, and proposed steps, which apply page-wide), the
 * calls it opens in place, and the Show all calls switch, so changing any of them draws it again. What
 * comes back is a definition's drawn flow, or an app's or entrance's start card; the canvas lays it out
 * and applies no rules of its own. Engine events refresh a card when a file it draws changes
 * (ipc/queries).
 *
 * The blocks of a drawn flow (`flowBlocks`) are read from its parts, never from keys.
 */

import { keepPreviousData, useQueries, useQuery, type UseQueryResult } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { queryClient } from "../ipc/client";
import type { Actions, CallBox, Callback, Card, CardArgs, CardView, DrawnFlow, Form, Key, Lane, LinkEnd, OtherCall, OverlayArgs, Page, ProjectId, Sequence } from "../ipc/generated";
import { engineKey } from "../ipc/queries";
import { UnfoldError } from "../ipc/unfold";
import { cardKey, findCard, findPage } from "./boards/document";
import { call } from "./boards/engine";
import { getOpenBoard, useOpenBoard } from "./boards/store";
import type { PageTarget } from "./boards/types";
import { getShowAllCalls, useShowAllCalls } from "./showAllCalls";

/** The overlay a page's cards are drawn with: its names, combined steps, and proposed steps (empty parts left out). */
export function pageOverlay(page: Pick<Page, "names" | "combined"> & { readonly proposals?: Page["proposals"] | undefined }): OverlayArgs {
  const names = page.names.map(({ key, text }) => ({ key, text }));
  const combined = page.combined.map(({ id, title, members, collapsed }) => ({ id, title, members: [...members], collapsed }));
  const proposed = (page.proposals ?? []).map((proposal) => ({
    id: proposal.id,
    ...("after" in proposal ? { after: proposal.after } : { start: proposal.start }),
    title: proposal.title,
  }));
  return {
    ...(names.length > 0 ? { names } : {}),
    ...(combined.length > 0 ? { combined } : {}),
    ...(proposed.length > 0 ? { proposed } : {}),
  };
}

/** The `card` arguments of a card: its key, the page's overlay, the calls it opens in place, and Show all calls. */
export function cardArgs(project: ProjectId, card: Pick<Card, "ref" | "open">, overlay: OverlayArgs, showAllCalls: boolean): CardArgs {
  const open = card.open ?? [];
  return {
    project,
    key: cardKey(card),
    ...(Object.keys(overlay).length > 0 ? { overlay } : {}),
    ...(open.length > 0 ? { open: open.map((path) => [...path]) } : {}),
    ...(showAllCalls ? { showAllCalls } : {}),
  };
}

/** The query for one card. */
export function cardQuery(args: CardArgs) {
  return {
    queryKey: engineKey("card", args),
    queryFn: () => call("card", args),
    // A missing key is an answer, not a hiccup; engine events say when to ask again.
    retry: false,
    staleTime: Infinity,
  } as const;
}

/** The `card` arguments of a card on an open page now (undefined when the page or the card is gone). */
export function cardArgsOf(target: PageTarget, cardId: string): CardArgs | undefined {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  const card = page ? findCard(page, cardId) : undefined;
  return page && card ? cardArgs(target.board.project, card, pageOverlay(page), getShowAllCalls()) : undefined;
}

/** A card on an open page, read through the cache (asked for when it isn't there). */
export async function fetchCard(target: PageTarget, cardId: string): Promise<CardView | undefined> {
  const args = cardArgsOf(target, cardId);
  return args ? queryClient.query(cardQuery(args)) : undefined;
}

const NO_NAMES: Page["names"] = [];
const NO_COMBINED: Page["combined"] = [];
const NO_CARDS: Page["cards"] = [];

/** The overlay of a page and the Show all calls switch, kept current. */
function useDrawing(target: PageTarget): { readonly overlay: OverlayArgs; readonly showAllCalls: boolean } {
  const names = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.names) ?? NO_NAMES;
  const combined = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.combined) ?? NO_COMBINED;
  const proposals = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.proposals);
  const showAllCalls = useShowAllCalls();
  const overlay = useMemo(() => pageOverlay({ names, combined, proposals }), [names, combined, proposals]);
  return { overlay, showAllCalls };
}

/**
 * A card of a page, kept current as the page's overlay, the card's open calls, and Show all calls
 * change. While it is asked for again, the card as last drawn stays.
 */
export function useCard(target: PageTarget, card: Pick<Card, "ref" | "open">): UseQueryResult<CardView, UnfoldError> {
  const { overlay, showAllCalls } = useDrawing(target);
  return useQuery<CardView, UnfoldError, CardView, ReturnType<typeof cardQuery>["queryKey"]>({
    ...cardQuery(cardArgs(target.board.project, card, overlay, showAllCalls)),
    placeholderData: keepPreviousData,
  });
}

/** The data of each query, in order (one function, so the combined list is kept while nothing changes). */
function dataOf(results: readonly { readonly data: CardView | undefined }[]): (CardView | undefined)[] {
  return results.map((result) => result.data);
}

/** The cards of a page that are loaded, by card id, kept current (a card still loading, or missing, is absent). */
export function usePageCards(target: PageTarget): ReadonlyMap<string, CardView> {
  const cards = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.cards) ?? NO_CARDS;
  const { overlay, showAllCalls } = useDrawing(target);
  const views = useQueries({
    queries: cards.map((card) => ({ ...cardQuery(cardArgs(target.board.project, card, overlay, showAllCalls)), placeholderData: keepPreviousData })),
    combine: dataOf,
  });
  return useMemo(() => new Map(cards.flatMap((card, index) => (views[index] ? [[card.id, views[index]] as const] : []))), [cards, views]);
}

/** What each card of a page is called (`cardName`), kept current; a card not loaded yet goes by its id. */
export function useCardNames(target: PageTarget): (cardId: string) => string {
  const views = usePageCards(target);
  return useCallback(
    (cardId: string) => {
      const view = views.get(cardId);
      return view ? cardName(view) : cardId;
    },
    [views],
  );
}

/** A card that draws a block, and the block as it draws it. */
interface PageBlock {
  readonly cardId: string;
  readonly block: DrawnBlock;
}

/** The first of `views` that draws block `key`, and the block. */
function pageBlock(views: ReadonlyMap<string, CardView>, key: Key): PageBlock | undefined {
  for (const [cardId, view] of views) {
    const found = view.type === "definition" ? findBlock(view.flow, key) : undefined;
    if (found) {
      return { cardId, block: found };
    }
  }
  return undefined;
}

/** The first card of a page that draws block `key`, and the block as it draws it (undefined while none loaded does). */
export function usePageBlock(target: PageTarget, key: Key | null): PageBlock | undefined {
  const views = usePageCards(target);
  return useMemo(() => (key === null ? undefined : pageBlock(views, key)), [views, key]);
}

/** What a card is called: the page's name for a definition, else its name from the code; a start's name. */
export function cardName(view: CardView): string {
  return view.type === "definition" ? (view.flow.header.name ?? view.flow.header.label) : view.name;
}

/** Whether a failed card query means the key is gone from the graph (the card is a placeholder). */
export function isMissing(error: unknown): boolean {
  return error instanceof UnfoldError && error.code === "not_found";
}

/** The ids of the combined and proposed steps a card draws, in the calls it opens in place too. */
export function overlayIds(view: CardView): string[] {
  const inSequence = (sequence: Sequence): string[] =>
    sequence.steps.flatMap((step): string[] => {
      switch (step.kind) {
        case "combined":
          return [step.item, ...(step.steps ? inSequence(step.steps) : [])];
        case "proposed":
          return [step.item];
        case "call":
          return [...(step.inset ? inFlow(step.inset) : []), ...step.callbacks.flatMap((callback) => (callback.inset ? inFlow(callback.inset) : []))];
        case "decision":
        case "loop":
        case "try":
        case "with":
          return step.lanes.flatMap((lane) => inSequence(lane.steps));
        case "thin":
        case "exit":
          return [];
      }
    });
  const inFlow = (flow: DrawnFlow): string[] => inSequence(flow.steps);
  return view.type === "definition" ? inFlow(view.flow) : [];
}

/** The cards of an open page that draw the combined or proposed step `itemId`, as the cache holds them now. */
export function cardsDrawing(target: PageTarget, itemId: string): string[] {
  const board = getOpenBoard(target.board);
  const page = board ? findPage(board.document, target.page) : undefined;
  return (page?.cards ?? []).flatMap((card) => {
    const args = cardArgsOf(target, card.id);
    const view = args ? queryClient.getQueryData<CardView>(cardQuery(args).queryKey) : undefined;
    return view && overlayIds(view).includes(itemId) ? [card.id] : [];
  });
}

/**
 * A block drawn in a card: its key, its form, its words from the code, the page's name for it, what the
 * overlay may do to it, and, for a call, where it goes.
 */
export interface DrawnBlock {
  readonly key: Key;
  readonly form: Form;
  readonly label: string;
  readonly name?: string;
  readonly actions?: Actions;
  readonly to?: LinkEnd;
}

function block(key: Key, form: Form, label: string, name: string | undefined, actions: Actions | undefined, to?: LinkEnd): DrawnBlock {
  return { key, form, label, ...(name !== undefined ? { name } : {}), ...(actions ? { actions } : {}), ...(to ? { to } : {}) };
}

function otherCalls(calls: readonly OtherCall[]): DrawnBlock[] {
  return calls.flatMap((other) => [block(other.key, other.form, other.callee, undefined, other.actions, other.to), ...otherCalls(other.inside)]);
}

function callBlocks(box: CallBox): DrawnBlock[] {
  return [
    block(box.key, box.form, box.callee, box.name, box.actions, box.to),
    ...otherCalls(box.calls),
    ...(box.inset ? flowBlocks(box.inset) : []),
    ...box.callbacks.flatMap((callback) => [
      block(callback.key, "lambda", callback.label, callback.name, callback.actions),
      ...(callback.inset ? flowBlocks(callback.inset) : []),
    ]),
  ];
}

function sequenceBlocks(sequence: Sequence): DrawnBlock[] {
  const hidden = (gap: { readonly hidden: readonly { readonly key: Key; readonly form: Form; readonly label: string }[] } | undefined) =>
    (gap?.hidden ?? []).map((item) => block(item.key, item.form, item.label, undefined, undefined));
  return [
    ...sequence.steps.flatMap((step): DrawnBlock[] => {
      const gap = hidden(step.gap);
      switch (step.kind) {
        case "call":
          return [...gap, ...callBlocks(step)];
        case "thin":
          return [...gap, ...otherCalls(step.calls)];
        case "decision":
        case "loop":
        case "try":
        case "with":
          return [
            ...gap,
            block(step.key, step.form, step.header ?? step.form, step.name, step.actions),
            ...step.headerCalls.flatMap(callBlocks),
            ...step.lanes.flatMap((lane) => [
              block(lane.key, lane.form, lane.clause ?? lane.test?.text ?? lane.form, lane.name, lane.actions),
              ...(lane.test?.calls ?? []).flatMap(callBlocks),
              ...sequenceBlocks(lane.steps),
            ]),
          ];
        case "exit":
          return [
            ...gap,
            block(step.key, step.form, step.value?.kind === "text" ? `${step.form} ${step.value.text}` : step.form, step.name, step.actions),
            ...(step.value?.kind === "call" ? callBlocks(step.value) : []),
          ];
        case "combined":
          return [...gap, ...(step.steps ? sequenceBlocks(step.steps) : [])];
        case "proposed":
          return gap;
      }
    }),
    ...hidden(sequence.endGap),
  ];
}

/** Every block a drawn flow draws, its definition first, then its steps in the order they run, insets included. */
export function flowBlocks(flow: DrawnFlow): DrawnBlock[] {
  const { header } = flow;
  return [block(header.key, header.form, header.label, header.name, header.actions), ...otherCalls(header.definedWith), ...sequenceBlocks(flow.steps)];
}

/** The block of a drawn flow with key `key`, if the flow draws it. */
export function findBlock(flow: DrawnFlow, key: Key): DrawnBlock | undefined {
  return flowBlocks(flow).find((item) => item.key === key);
}

/** What `visitFlow` hands each part to, with the calls opened in place around it (outermost first). */
interface FlowVisitor {
  readonly flow?: (flow: DrawnFlow, around: readonly Key[]) => void;
  readonly box?: (box: CallBox, around: readonly Key[]) => void;
  readonly callback?: (callback: Callback, around: readonly Key[]) => void;
  readonly lane?: (lane: Lane) => void;
}

/** Walks a drawn flow's parts in the order they run, into the calls and callbacks it opens in place. */
function visitFlow(flow: DrawnFlow, visitor: FlowVisitor, around: readonly Key[] = []): void {
  const box = (call: CallBox) => {
    visitor.box?.(call, around);
    if (call.inset) {
      visitFlow(call.inset, visitor, [...around, call.key]);
    }
    for (const callback of call.callbacks) {
      visitor.callback?.(callback, around);
      if (callback.inset) {
        visitFlow(callback.inset, visitor, [...around, callback.key]);
      }
    }
  };
  const sequence = (steps: Sequence) => {
    for (const step of steps.steps) {
      switch (step.kind) {
        case "call":
          box(step);
          break;
        case "decision":
        case "loop":
        case "try":
        case "with":
          step.headerCalls.forEach(box);
          for (const lane of step.lanes) {
            visitor.lane?.(lane);
            (lane.test?.calls ?? []).forEach(box);
            sequence(lane.steps);
          }
          break;
        case "exit":
          if (step.value?.kind === "call") {
            box(step.value);
          }
          break;
        case "combined":
          if (step.steps) {
            sequence(step.steps);
          }
          break;
        case "thin":
        case "proposed":
          break;
      }
    }
  };
  visitor.flow?.(flow, around);
  sequence(flow.steps);
}

/**
 * The open path that opens or closes call or callback `key` of a drawn flow in place: the calls opened
 * around it, outermost first, then itself. Undefined when the flow draws no such call or callback.
 */
export function openPathOf(flow: DrawnFlow, key: Key): readonly Key[] | undefined {
  let found: readonly Key[] | undefined;
  const at = (own: Key, around: readonly Key[]) => {
    if (found === undefined && own === key) {
      found = [...around, own];
    }
  };
  visitFlow(flow, { box: (box, around) => { at(box.key, around); }, callback: (callback, around) => { at(callback.key, around); } });
  return found;
}

/**
 * The steps of a body that can be combined, in written order, drawn or not (`CombineWith` `steps`): a
 * definition's own (`DrawnFlow.combinable`) or a part's (`Lane.combinable`), in the calls the flow opens
 * in place too. Undefined when the flow doesn't draw that body.
 */
export function combinableSteps(flow: DrawnFlow, body: Key): readonly Key[] | undefined {
  let found: readonly Key[] | undefined;
  visitFlow(flow, {
    flow: (drawn) => {
      if (found === undefined && drawn.header.key === body) {
        found = drawn.combinable;
      }
    },
    lane: (lane) => {
      if (found === undefined && lane.key === body) {
        found = lane.combinable;
      }
    },
  });
  return found;
}
