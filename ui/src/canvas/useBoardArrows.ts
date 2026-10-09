/** The card models of a board (from the query cache) and the arrows and ports derived from them. */

import { useQueries } from "@tanstack/react-query";
import { useMemo } from "react";
import type { Card, CardView } from "../ipc/generated";
import { cardKey } from "../state/boards/document";
import { computeArrows, portsByCard, type Arrow } from "./arrows";
import { cardQuery, lastVerifiedCode } from "./cardData";

export interface BoardArrows {
  readonly models: ReadonlyMap<string, CardView>;
  readonly arrows: readonly Arrow[];
  readonly ports: ReadonlyMap<string, readonly string[]>;
  /** Whether every card's model has loaded (or failed). */
  readonly settled: boolean;
}

export function useBoardArrows(cards: readonly Card[]): BoardArrows {
  const keys = useMemo(() => [...new Set(cards.map(cardKey))], [cards]);
  const loaded = useQueries({
    queries: keys.map((key) => cardQuery(key)),
    // Structurally shared: `data` keeps its identity while no model changed.
    combine: (results) => ({ data: results.map((result) => result.data), settled: results.every((result) => !result.isPending) }),
  });
  const models = useMemo(() => {
    const map = new Map<string, CardView>();
    keys.forEach((key, index) => {
      const data = loaded.data[index];
      if (data) {
        map.set(key, data);
      }
    });
    return map;
  }, [keys, loaded.data]);
  const arrows = useMemo(
    () => computeArrows(cards.map((card) => ({ id: card.id, key: cardKey(card), entrance: "entrance" in card.ref })), models, lastVerifiedCode),
    [cards, models],
  );
  const ports = useMemo(() => portsByCard(arrows), [arrows]);
  return { models, arrows, ports, settled: loaded.settled };
}
