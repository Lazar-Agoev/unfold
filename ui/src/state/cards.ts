/**
 * Cards from Rust (`card`), cached with TanStack Query under the engine key. A card is asked for with
 * its key, the page's overlay (its names, combined steps, and proposed steps, which apply page-wide), the
 * calls it opens in place, and the Show all calls switch. What comes back is a definition's drawn flow,
 * or an app's or entrance's start card; the canvas lays it out and applies no rules of its own. Engine
 * events refresh a card when a file it draws changes (ipc/queries).
 *
 * A card is drawn again only when what it draws can change (`cardQuery`): where the overlay's entries
 * go (which blocks are named, what each combined step holds and whether it is collapsed, where each
 * proposed step goes) changes what any card draws, but an entry's words show only on the cards that
 * draw it. So typing a name redraws the cards that draw its block, not the whole page.
 *
 * A drawn flow is read through the one walk the window and the MCP server share
 * (`@unfold/desktop/drawing`), never from keys.
 */

import { hashKey, keepPreviousData, useQueries, useQuery, type QueryClient, type UseQueryResult } from "@tanstack/react-query";
import { flowBlocks, visitFlow, type DrawnBlock } from "@unfold/desktop/drawing";
import { useCallback, useMemo } from "react";
import { queryClient } from "../ipc/client";
import type { Card, CardArgs, CardView, DrawnFlow, Key, OverlayArgs, Page, ProjectId, RepoPath, Sequence } from "../ipc/generated";
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

/**
 * What a card's drawing shows the words of: the blocks it draws (in the calls it opens in place too, and
 * those a collapsed combined step holds) and the page's combined and proposed steps it draws.
 */
function shownWords(view: CardView): ReadonlySet<string> {
  return view.type === "definition" ? new Set([...flowBlocks(view.flow).map((block) => block.key), ...heldBlocks(view.flow), ...overlayIds(view)]) : new Set();
}

const NO_WORDS: ReadonlySet<string> = new Set();

/** The overlay with the words of every entry whose block or step `shown` doesn't name left blank. */
function wordsShown(overlay: OverlayArgs, shown: ReadonlySet<string>): OverlayArgs {
  const words = (id: string, text: string) => (shown.has(id) ? text : "");
  const { names, combined, proposed } = overlay;
  return {
    ...(names ? { names: names.map(({ key, text }) => ({ key, text: words(key, text) })) } : {}),
    ...(combined ? { combined: combined.map((step) => ({ ...step, title: words(step.id, step.title) })) } : {}),
    ...(proposed ? { proposed: proposed.map((step) => ({ ...step, title: words(step.id, step.title) })) } : {}),
  };
}

/** A card as the engine last drew it: where its page's overlay entries went then, and whose words it showed. */
interface Drawn {
  readonly places: string;
  readonly shown: ReadonlySet<string>;
}

/** Each card's last drawing, by the card (its key, open calls, and Show all calls). */
const lastDrawn = new Map<string, Drawn>();

/** Which card `args` asks for, its page's overlay aside. */
function cardIdentity({ project, key, open, showAllCalls }: CardArgs): string {
  return JSON.stringify([project, key, open ?? [], showAllCalls === true]);
}

/** Where the overlay's entries go, without their words. */
function placesOf(args: CardArgs): string {
  return JSON.stringify(args.overlay ? wordsShown(args.overlay, NO_WORDS) : null);
}

/**
 * The arguments a card is cached under: its own, with the words of the overlay entries it doesn't show
 * left blank once its drawing with the same places says which those are (until then, all of them).
 */
function cachedArgs(args: CardArgs): CardArgs {
  const drawn = lastDrawn.get(cardIdentity(args));
  return args.overlay && drawn?.places === placesOf(args) ? { ...args, overlay: wordsShown(args.overlay, drawn.shown) } : args;
}

/** The query for one card: asked for with `args`, cached under the words it shows (`cachedArgs`). */
export function cardQuery(args: CardArgs) {
  const queryKey = engineKey("card", cachedArgs(args));
  return {
    queryKey,
    queryFn: async ({ client }: { readonly client: QueryClient }) => {
      const view = await call("card", args);
      // Now that the drawing says whose words it shows, the card is cached under those alone too.
      lastDrawn.set(cardIdentity(args), { places: placesOf(args), shown: shownWords(view) });
      const shownKey = engineKey("card", cachedArgs(args));
      if (hashKey(shownKey) !== hashKey(queryKey)) {
        client.setQueryData(shownKey, view);
      }
      return view;
    },
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

/**
 * The ids of the combined and proposed steps a card draws, wherever it draws them: in the calls it
 * opens in place too, from a step, a frame's header, a test, or an exit.
 */
export function overlayIds(view: CardView): string[] {
  const ids: string[] = [];
  if (view.type === "definition") {
    visitFlow(view.flow, {
      step: (step) => {
        if (step.kind === "combined" || step.kind === "proposed") {
          ids.push(step.item);
        }
      },
    });
  }
  return ids;
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

/** The block of a drawn flow with key `key`, if the flow draws it (in the calls it opens in place too). */
export function findBlock(flow: DrawnFlow, key: Key): DrawnBlock | undefined {
  return flowBlocks(flow).find((item) => item.key === key);
}

/**
 * Calls `visit` with every sequence a drawn flow draws (its body, its parts', its open combined
 * steps', and those of the calls it opens in place), the body it is (a definition's or a part's key,
 * or an open combined step's id), and the file of the flow drawing it.
 */
export function eachSequence(flow: DrawnFlow, visit: (sequence: Sequence, body: string, path: RepoPath) => void): void {
  visitFlow(flow, {
    sequence: (sequence, body, at) => {
      visit(sequence, body, at.flow.header.path);
    },
  });
}

/** Every block a drawn flow's collapsed combined steps hold, which each outlines. */
export function heldBlocks(flow: DrawnFlow): Key[] {
  const held: Key[] = [];
  visitFlow(flow, {
    step: (step) => {
      if (step.kind === "combined") {
        held.push(...(step.holds ?? []));
      }
    },
  });
  return held;
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
  visitFlow(flow, {
    box: (box, where) => {
      at(box.key, where.around);
    },
    callback: (callback, where) => {
      at(callback.key, where.around);
    },
  });
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
    flow: ({ flow: drawn }) => {
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
