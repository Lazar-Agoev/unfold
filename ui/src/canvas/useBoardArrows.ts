/**
 * What a page's cards drew (the `card` queries, shared with the cards themselves), the type definitions
 * of the cards that show fields, how each card is drawn, and the arrows derived from them.
 */

import { useQueries } from "@tanstack/react-query";
import { cardShape, type CardShape } from "@unfold/desktop/drawing";
import { useMemo } from "react";
import type { Card, CardView, Page, TypeDefinition } from "../ipc/generated";
import { engineQueryOptions } from "../ipc/queries";
import { cardKey, cardRefKey, cardView, findPage, useOpenBoard, type PageTarget } from "../state/boards";
import { cardArgs, cardQuery, pageOverlay } from "../state/cards";
import { useShowAllCalls } from "../state/showAllCalls";
import { computeArrows, type Arrow, type ArrowCard } from "./arrows";

export interface BoardArrows {
  /** What each loaded card drew, by card id. */
  readonly views: ReadonlyMap<string, CardView>;
  readonly arrows: readonly Arrow[];
  /** How each card is drawn, by card id (a card not loaded yet, as its view says). */
  readonly shapes: ReadonlyMap<string, CardShape>;
  /** Whether every card has loaded (or failed). */
  readonly settled: boolean;
}

const NO_NAMES: Page["names"] = [];
const NO_COMBINED: Page["combined"] = [];

/** The arrows of a page; its cards are of its board's project, and each shows its own view. */
export function useBoardArrows(target: PageTarget, cards: readonly Card[]): BoardArrows {
  const { project } = target.board;
  const defaultView = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.defaultView) ?? "compact";
  const names = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.names) ?? NO_NAMES;
  const combined = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.combined) ?? NO_COMBINED;
  const proposals = useOpenBoard(target.board, (board) => findPage(board.document, target.page)?.proposals);
  const showAllCalls = useShowAllCalls();
  const overlay = useMemo(() => pageOverlay({ names, combined, proposals }), [names, combined, proposals]);
  const loaded = useQueries({
    queries: cards.map((card) => cardQuery(cardArgs(project, card, overlay, showAllCalls))),
    // Structurally shared: `data` keeps its identity while no card changed. A card whose refetch failed
    // (its definition is gone) draws nothing, though the query keeps what it drew before.
    combine: (results) => ({ data: results.map((result) => (result.isError ? undefined : result.data)), settled: results.every((result) => !result.isPending) }),
  });
  const views = useMemo(
    () => new Map(cards.flatMap((card, index) => (loaded.data[index] ? [[card.id, loaded.data[index]] as const] : []))),
    [cards, loaded.data],
  );
  const shapes = useMemo(() => new Map(cards.map((card) => [card.id, cardShape(views.get(card.id), cardView(card, defaultView))])), [cards, views, defaultView]);
  // The keys whose cards show fields: their type definitions draw type arrows.
  const typeKeys = useMemo(() => [...new Set(cards.filter((card) => shapes.get(card.id) === "fields").map(cardKey))], [cards, shapes]);
  const loadedTypes = useQueries({
    queries: typeKeys.map((key) => engineQueryOptions("type_definition", { project, key })),
    combine: (results) => results.map((result) => (result.isError ? undefined : result.data)),
  });
  const arrows = useMemo(() => {
    const arrowCards = cards.map((card): ArrowCard => ({ id: card.id, key: cardKey(card), project, view: cardView(card, defaultView) }));
    const models = new Map(cards.flatMap((card) => {
      const view = views.get(card.id);
      return view ? [[cardRefKey(project, cardKey(card)), view] as const] : [];
    }));
    const definitions = new Map(typeKeys.flatMap((key, index): (readonly [string, TypeDefinition])[] => {
      const definition = loadedTypes[index];
      return definition ? [[cardRefKey(project, key), definition]] : [];
    }));
    return computeArrows(arrowCards, models, definitions);
  }, [cards, views, project, defaultView, typeKeys, loadedTypes]);
  return { views, arrows, shapes, settled: loaded.settled };
}
